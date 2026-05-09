// SPDX-License-Identifier: BSD-2-Clause

#![no_std]
#![no_main]

use core::fmt::Write;
use core::panic::PanicInfo;
use limine::BaseRevision;
use limine::memory_map::EntryType;
use limine::request::{HhdmRequest, MemoryMapRequest, RequestsEndMarker, RequestsStartMarker};

mod heap;
mod serial;

// Limine base-revision-1+ requires explicit start/end marker pairs around
// the .requests section so the bootloader can bound its scan; without
// them, our BASE_REVISION is not seen and is_supported() silently
// returns false.

#[used]
#[unsafe(link_section = ".requests_start_marker")]
static REQUESTS_START: RequestsStartMarker = RequestsStartMarker::new();

#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests")]
static MEMMAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[unsafe(link_section = ".requests_end_marker")]
static REQUESTS_END: RequestsEndMarker = RequestsEndMarker::new();

const HEAP_CAP: usize = 1 << 20; // 1 MiB — sufficient for M0 step 2.

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    // If the bootloader doesn't support the base revision the limine
    // crate was compiled against, hang silently — emitting the sentinel
    // would lie about success.
    if !BASE_REVISION.is_supported() {
        halt();
    }

    serial::init();
    serial::write_str("ARSENAL_BOOT_OK\n");

    init_heap();

    halt();
}

fn init_heap() {
    let memmap = MEMMAP_REQUEST
        .get_response()
        .expect("limine: memory map response missing");
    let hhdm = HHDM_REQUEST
        .get_response()
        .expect("limine: hhdm response missing");
    let hhdm_offset = hhdm.offset() as usize;

    let region = memmap
        .entries()
        .iter()
        .filter(|e| e.entry_type == EntryType::USABLE)
        .max_by_key(|e| e.length)
        .expect("limine: no USABLE memory regions");

    let heap_phys = region.base as usize;
    let heap_size = (region.length as usize).min(HEAP_CAP);
    let heap_virt = heap_phys + hhdm_offset;

    // SAFETY: Limine reported [heap_phys, heap_phys + region.length) as
    // USABLE — exclusively available to the kernel — and maps it via
    // HHDM at heap_virt. No other code references this region.
    unsafe { heap::init(heap_virt, heap_size) };

    let usable_count = memmap
        .entries()
        .iter()
        .filter(|e| e.entry_type == EntryType::USABLE)
        .count();
    let _ = writeln!(
        serial::Writer,
        "mm: {usable_count} usable regions; heap @ {heap_virt:#018x} size {} KiB",
        heap_size / 1024
    );
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    halt();
}

fn halt() -> ! {
    loop {
        // SAFETY: `hlt` is a privileged instruction with no side effects beyond
        // halting the CPU until the next interrupt. We are in ring 0 (entered
        // from Limine) and the loop ensures we re-halt on spurious wakes.
        unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)) }
    }
}
