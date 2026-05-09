// SPDX-License-Identifier: BSD-2-Clause

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

use core::fmt::Write;
use core::panic::PanicInfo;
use limine::BaseRevision;
use limine::memory_map::EntryType;
use limine::request::{HhdmRequest, MemoryMapRequest, RequestsEndMarker, RequestsStartMarker};

mod cpu;
mod frames;
mod gdt;
mod heap;
mod idt;
mod paging;
mod sched;
mod serial;
mod task;

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

const HEAP_CAP: usize = 16 << 20; // 16 MiB — sustains frame-allocator Vec growth (the reclaim path doubles to ~1 MiB) plus future-step churn without re-tuning every milestone.

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

    let mem = init_heap();
    gdt::init();
    idt::init();

    // Self-test: trigger a breakpoint and confirm we re-enter _start.
    // The handler prints "EXCEPTION #BP at <addr>" and returns; if
    // the IDT is mis-loaded we'd triple-fault here instead.
    // SAFETY: int3 is the architecturally-defined breakpoint trap;
    // its only effect is to dispatch through IDT entry 3 to our
    // breakpoint handler, which prints and returns.
    unsafe { core::arch::asm!("int3", options(nomem, nostack, preserves_flags)) };

    let memmap = MEMMAP_REQUEST
        .get_response()
        .expect("limine: memory map response missing post-int3");
    frames::init(memmap.entries(), mem.heap_phys_start, mem.heap_phys_end);
    paging::init(mem.hhdm_offset);

    heap_round_trip();
    serial::write_str("ARSENAL_HEAP_OK\n");

    let reclaimed = frames::reclaim_bootloader(memmap.entries());
    let _ = writeln!(
        serial::Writer,
        "frames: reclaimed {reclaimed} bootloader frames; {} free / {} total",
        frames::FRAMES.free_count(),
        frames::FRAMES.total_added()
    );
    serial::write_str("ARSENAL_FRAMES_OK\n");

    let cpu = cpu::current_cpu();
    let _ = writeln!(serial::Writer, "cpu: id={} (single-CPU stage)", cpu.id);

    // Smoke the Task allocator: build one, log its shape, drop it.
    // 3B-3 wires the asm that actually runs through saved_rsp. For
    // 3B-2 the assertion is just that Task::new returns a sensibly-
    // shaped struct and the heap can absorb / return 16 KiB without
    // tripping the linked-list allocator.
    let t = task::Task::new(task_smoke_entry);
    let _ = writeln!(
        serial::Writer,
        "task: built (entry={:#018x}, saved_rsp={:#018x}, state={:?}, stack={} KiB)",
        t.entry as usize as u64,
        t.saved_rsp,
        t.state,
        task::STACK_SIZE / 1024
    );
    drop(t);

    sched::switch_test();

    // Cross the threshold from main's Limine boot stack into the
    // scheduler-managed idle task. Never returns; main's stack
    // becomes dead.
    sched::init();
}

/// Placeholder entry for the 3B-2 Task::new smoke. 3B-3 lands the
/// asm that would actually invoke this; today it's never executed.
fn task_smoke_entry() -> ! {
    halt();
}

struct BootMem {
    hhdm_offset: u64,
    heap_phys_start: u64,
    heap_phys_end: u64,
}

/// Allocate, mutate, and read back through the global allocator after
/// the kernel-owned PML4 is live. A failure here would manifest as a
/// page fault on heap addresses that resolved fine before CR3 swap —
/// the load-bearing assertion that paging::init preserved HHDM.
fn heap_round_trip() {
    use alloc::boxed::Box;
    use alloc::vec::Vec;

    let b = Box::new(0xDEAD_BEEF_u32);
    assert_eq!(*b, 0xDEAD_BEEF);
    drop(b);

    let mut v: Vec<u32> = Vec::with_capacity(8);
    for i in 0..8u32 {
        v.push(i * i);
    }
    let sum: u32 = v.iter().sum();
    assert_eq!(sum, 140);
}

fn init_heap() -> BootMem {
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

    BootMem {
        hhdm_offset: hhdm_offset as u64,
        heap_phys_start: heap_phys as u64,
        heap_phys_end: (heap_phys + heap_size) as u64,
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // PanicInfo's Display impl already includes the message and (when
    // present) the location, so a single writeln! is the whole
    // diagnostic. The 3-4 silent OOM during 3A cost an hour because
    // there was nothing on the wire; the 3B context-switch work is
    // the next place a silent panic could hide.
    let _ = writeln!(serial::Writer, "ARSENAL_PANIC {info}");
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
