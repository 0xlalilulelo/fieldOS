// SPDX-License-Identifier: BSD-2-Clause
//
// Take ownership of every level of the page-table tree by deep-cloning
// Limine's PML4 → PDPT → PD → PT down through every present, non-huge
// entry. Leaf entries (PT-level entries, or huge-page entries at PDPT
// / PD level) are copied verbatim — their target physical frames stay
// the same, since they describe what is mapped, not where the table
// lives. After init the kernel owns every page table; Limine's
// BOOTLOADER_RECLAIMABLE memory becomes free for 3-4 to add to the
// frame allocator.

use core::fmt::Write;
use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::PhysAddr;
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::structures::paging::{PageTable, PageTableFlags, PhysFrame, Size4KiB};

use crate::frames;
use crate::serial;

const PAGE_SIZE: usize = 4096;

/// HHDM offset captured at init. Other subsystems (virtio MMIO, future
/// driver code) need it to translate physical BAR addresses into kernel
/// virtual addresses without re-querying Limine.
static HHDM_OFFSET: AtomicU64 = AtomicU64::new(0);

/// HHDM offset reported by Limine and stashed by `init`. Reads are
/// undefined before `init` runs — callers gate themselves behind
/// post-init points in the boot sequence.
pub fn hhdm_offset() -> u64 {
    HHDM_OFFSET.load(Ordering::Relaxed)
}

/// Deep-clone a page table at `level` (4 = PML4, 3 = PDPT, 2 = PD,
/// 1 = PT). Returns the physical frame of the new table. Leaf
/// entries — anything at level 1, or any HUGE_PAGE-flagged entry at
/// level 2 or 3 — are copied verbatim so the new table reproduces
/// the source's mappings exactly. Non-leaf entries get a fresh
/// child via recursion.
///
/// # Safety
/// `src_phys` must point at a valid PageTable mapped via the HHDM
/// at `src_phys + hhdm_offset`. The frame allocator must be
/// initialized and contain enough frames for the entire walk —
/// running out mid-clone leaves the source intact (we never mutate
/// `src`) but the partial new tree leaks until the kernel halts.
unsafe fn deep_clone_table(
    src_phys: PhysAddr,
    level: u8,
    hhdm_offset: u64,
) -> PhysFrame<Size4KiB> {
    let new_frame = frames::FRAMES
        .alloc_frame()
        .expect("paging: OOM during deep clone");
    let new_virt = (new_frame.start_address().as_u64() + hhdm_offset) as *mut PageTable;
    let src_virt = (src_phys.as_u64() + hhdm_offset) as *const PageTable;

    // SAFETY: new_virt addresses freshly allocated, exclusively-owned
    // memory mapped by HHDM. Zeroing it produces an all-unused
    // PageTable layout. src_virt points at the live table.
    unsafe {
        core::ptr::write_bytes(new_virt as *mut u8, 0, PAGE_SIZE);
        let new = &mut *new_virt;
        let src = &*src_virt;
        for (i, src_entry) in src.iter().enumerate() {
            if src_entry.is_unused() {
                continue;
            }
            let is_leaf =
                level == 1 || src_entry.flags().contains(PageTableFlags::HUGE_PAGE);
            if is_leaf {
                new[i] = src_entry.clone();
            } else {
                let child_phys = src_entry.addr();
                let new_child = deep_clone_table(child_phys, level - 1, hhdm_offset);
                new[i].set_addr(new_child.start_address(), src_entry.flags());
            }
        }
    }

    new_frame
}

pub fn init(hhdm_offset: u64) {
    HHDM_OFFSET.store(hhdm_offset, Ordering::Relaxed);
    let (cur_pml4_frame, _) = Cr3::read();

    // SAFETY: cur_pml4_frame is the live PML4 — written either by
    // Limine or by the shallow clone in step 2-4 (we replace that
    // shallow clone with this deep clone here). HHDM maps every
    // physical frame, page tables included.
    let new_pml4 =
        unsafe { deep_clone_table(cur_pml4_frame.start_address(), 4, hhdm_offset) };

    // SAFETY: new_pml4 is a deep clone — every transitive mapping
    // is reproduced through tables we allocated. The kernel ELF,
    // HHDM, heap, current stack, and Limine response data all
    // resolve identically after the CR3 write. The CR3 write
    // flushes the TLB.
    unsafe { Cr3::write(new_pml4, Cr3Flags::empty()) };

    let _ = writeln!(
        serial::Writer,
        "paging: deep-cloned cr3 -> {:#018x} (all levels kernel-owned)",
        new_pml4.start_address().as_u64()
    );
}
