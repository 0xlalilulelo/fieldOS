// SPDX-License-Identifier: BSD-2-Clause
//
// Take ownership of CR3 by allocating a fresh PML4 and shallow-cloning
// Limine's existing top-level table into it. Lower-level tables (PDPT,
// PD, PT) remain Limine's, in BOOTLOADER_RECLAIMABLE memory — a future
// step deep-clones or rebuilds them when we want to actually reclaim
// that physical RAM. For M0 step 2 the contract is narrower: the
// kernel's own CR3 points at a frame the kernel owns, so subsequent
// steps can mutate top-level entries without racing the bootloader's
// view of memory.

use core::alloc::Layout;
use core::fmt::Write;
use x86_64::PhysAddr;
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::structures::paging::{PhysFrame, Size4KiB};

use crate::serial;

const PAGE_SIZE: usize = 4096;

pub fn init(hhdm_offset: u64) {
    let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).expect("PML4 layout");

    // SAFETY: the bump allocator from heap.rs is initialized; the
    // layout is non-zero; alloc_zeroed returns null only on OOM,
    // which we assert against. The returned pointer is exclusive,
    // 4-KiB-aligned, 4-KiB long, and zeroed.
    let new_pml4_virt = unsafe { alloc::alloc::alloc_zeroed(layout) };
    assert!(!new_pml4_virt.is_null(), "paging: OOM allocating new PML4");

    // Heap virtuals live in HHDM (heap_phys + hhdm_offset). Subtract
    // to recover the physical address of the new PML4 frame.
    let new_pml4_phys = (new_pml4_virt as u64).wrapping_sub(hhdm_offset);

    let (cur_pml4_frame, _) = Cr3::read();
    let cur_pml4_virt =
        (cur_pml4_frame.start_address().as_u64() + hhdm_offset) as *const u8;

    // SAFETY: cur_pml4_virt points at the live PML4 Limine installed,
    // mapped via the HHDM. new_pml4_virt is the freshly allocated
    // zeroed page. Neither overlaps and both are valid for 4 KiB.
    unsafe {
        core::ptr::copy_nonoverlapping(cur_pml4_virt, new_pml4_virt, PAGE_SIZE);
    }

    let new_frame =
        PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(new_pml4_phys));

    // SAFETY: new_pml4 is a verbatim copy of Limine's PML4, so every
    // transitive mapping (kernel ELF, HHDM, heap, current stack,
    // Limine response data) resolves identically after the CR3
    // write. The CR3 write itself flushes the TLB.
    unsafe { Cr3::write(new_frame, Cr3Flags::empty()) };

    let _ = writeln!(
        serial::Writer,
        "paging: cr3 -> {new_pml4_phys:#018x} (kernel-owned PML4, shallow clone)"
    );
}
