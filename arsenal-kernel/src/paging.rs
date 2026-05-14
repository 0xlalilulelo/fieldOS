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
use x86_64::VirtAddr;
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::structures::paging::mapper::MapToError;
use x86_64::structures::paging::{
    FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags,
    PhysFrame, Size4KiB,
};

use crate::frames;
use crate::serial;

const PAGE_SIZE: usize = 4096;

/// HHDM offset captured at init. Other subsystems (virtio MMIO, future
/// driver code) need it to translate physical BAR addresses into kernel
/// virtual addresses without re-querying Limine.
static HHDM_OFFSET: AtomicU64 = AtomicU64::new(0);

/// Physical address of the kernel-owned PML4 produced by the deep
/// clone in `init`. APs at 4-2 read this and write their own CR3
/// before touching any post-clone mapping (e.g. the LAPIC MMIO that
/// apic::init added via map_mmio) — Limine starts APs with Limine's
/// PML4 loaded, which has Limine's mappings only.
static KERNEL_PML4_PHYS: AtomicU64 = AtomicU64::new(0);

/// HHDM offset reported by Limine and stashed by `init`. Reads are
/// undefined before `init` runs — callers gate themselves behind
/// post-init points in the boot sequence.
pub fn hhdm_offset() -> u64 {
    HHDM_OFFSET.load(Ordering::Relaxed)
}

/// Physical address of the kernel-owned PML4. Returns 0 before
/// `init` runs.
pub fn kernel_pml4_phys() -> u64 {
    KERNEL_PML4_PHYS.load(Ordering::Relaxed)
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

/// Adapter so x86_64's OffsetPageTable can pull frames from our
/// global allocator for missing intermediate page tables.
struct FramesAdapter;
unsafe impl FrameAllocator<Size4KiB> for FramesAdapter {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        frames::FRAMES.alloc_frame()
    }
}

/// Map `length` bytes of physical memory starting at `phys` into the
/// kernel's page tables at virtual address `phys + hhdm_offset`,
/// using PRESENT | WRITABLE | NO_CACHE flags suitable for MMIO.
/// Pages already mapped (e.g. by Limine's HHDM coverage of RAM) are
/// silently accepted; missing intermediate page tables are allocated
/// from FRAMES.
///
/// Limine's HHDM only covers RAM (USABLE + reclaimable + ACPI per
/// the protocol); device MMIO regions like the PCI BAR space at
/// 0xfe000000 are not mapped by default. Callers that need to
/// touch MMIO must call this first.
pub fn map_mmio(phys: u64, length: u64) {
    let hhdm = hhdm_offset();
    // Round virtual range to page boundaries and walk one frame at
    // a time. This is overkill for huge mappings but the BAR
    // regions we touch are 4 KiB to ~16 KiB.
    let virt_start = (phys + hhdm) & !0xFFF;
    let virt_end = (phys + hhdm + length + 0xFFF) & !0xFFF;
    let phys_start = phys & !0xFFF;

    // SAFETY: cr3_frame is the live PML4. Translating its physical
    // address through HHDM gives a valid &mut PageTable; we hold the
    // unique kernel context and no concurrent walker exists in M0.
    let pml4 = unsafe {
        let cr3 = Cr3::read().0.start_address().as_u64();
        &mut *((cr3 + hhdm) as *mut PageTable)
    };
    let mut mapper = unsafe { OffsetPageTable::new(pml4, VirtAddr::new(hhdm)) };
    let mut allocator = FramesAdapter;
    let flags = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::NO_CACHE;

    let mut v = virt_start;
    let mut p = phys_start;
    while v < virt_end {
        let page: Page<Size4KiB> = Page::containing_address(VirtAddr::new(v));
        let frame: PhysFrame<Size4KiB> =
            PhysFrame::containing_address(PhysAddr::new(p));
        // SAFETY: we own the page tables; this device MMIO frame is
        // distinct from any RAM allocation and aliasing it via this
        // mapping is the entire purpose. NO_CACHE flag prevents
        // speculative reads from cached aliases.
        let result = unsafe { mapper.map_to(page, frame, flags, &mut allocator) };
        match result {
            Ok(flusher) => flusher.flush(),
            Err(MapToError::PageAlreadyMapped(_)) => {
                // Limine already had this region; nothing to do.
            }
            Err(MapToError::ParentEntryHugePage) => {
                // The region is covered by a huge page upstream;
                // already mapped at a coarser granularity.
            }
            Err(e) => panic!("map_mmio: unexpected map error {e:?} at virt {v:#018x}"),
        }
        v += PAGE_SIZE as u64;
        p += PAGE_SIZE as u64;
    }
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

    KERNEL_PML4_PHYS.store(new_pml4.start_address().as_u64(), Ordering::Relaxed);

    let _ = writeln!(
        serial::Writer,
        "paging: deep-cloned cr3 -> {:#018x} (all levels kernel-owned)",
        new_pml4.start_address().as_u64()
    );
}
