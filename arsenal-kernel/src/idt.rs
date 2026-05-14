// SPDX-License-Identifier: BSD-2-Clause
//
// IDT bring-up. Handlers print exception details to COM1 and halt;
// recoverable exceptions (currently only #BP) print and return so
// callers can use `int3` as a self-test signal. The three faults
// that cannot share the kernel's normal stack — #DF, #NMI, #MC —
// route to their dedicated IST stacks reserved by gdt::init.
//
// M1 step 1-0 changes the storage shape from spin::Lazy<IDT> (M0)
// to spin::Mutex<IDT> + a `register_vector` API. M0 wired every
// IRQ vector inside the Lazy initializer; M1 needs PCIe drivers
// (NVMe at step 1, xHCI at step 3, etc.) to register their IRQ
// handlers after boot at vectors allocated from a dynamic pool.
// Modifications to a loaded IDT are safe — `lidt` records the
// table's address into IDTR, and subsequent in-memory updates to
// entries take effect on the next dispatch without a re-load. The
// IrqGuard around register_vector keeps the write atomic against
// the local core; cross-core visibility comes from the implicit
// fence in the device wiring that happens after register returns
// (caller writes vector into MSI-X table entry → caller unmasks
// the entry → device may now fire; the IDT write strictly
// precedes the device-side enable).

use core::fmt::Write;
use core::sync::atomic::{AtomicU8, Ordering};

use spin::Mutex;
use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{
    HandlerFunc, InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode,
};

use crate::apic;
use crate::gdt;
use crate::irq;
use crate::kbd;
use crate::serial;

fn halt_loop() -> ! {
    loop {
        // SAFETY: cli + hlt is the canonical "we are definitively
        // dead, ignore any further interrupts" sequence. No other
        // requirements at this site beyond ring-0 execution, which
        // is guaranteed since we got here via an exception.
        unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)) }
    }
}

extern "x86-interrupt" fn double_fault_handler(
    frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    let _ = writeln!(
        serial::Writer,
        "EXCEPTION #DF (double fault) error_code={error_code:#x}\n  frame: {frame:#?}"
    );
    halt_loop();
}

extern "x86-interrupt" fn non_maskable_handler(frame: InterruptStackFrame) {
    let _ = writeln!(serial::Writer, "EXCEPTION #NMI\n  frame: {frame:#?}");
    halt_loop();
}

extern "x86-interrupt" fn machine_check_handler(frame: InterruptStackFrame) -> ! {
    let _ = writeln!(
        serial::Writer,
        "EXCEPTION #MC (machine check)\n  frame: {frame:#?}"
    );
    halt_loop();
}

extern "x86-interrupt" fn general_protection_handler(
    frame: InterruptStackFrame,
    error_code: u64,
) {
    let _ = writeln!(
        serial::Writer,
        "EXCEPTION #GP error_code={error_code:#x}\n  frame: {frame:#?}"
    );
    halt_loop();
}

extern "x86-interrupt" fn page_fault_handler(
    frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    let cr2 = Cr2::read_raw();
    let _ = writeln!(
        serial::Writer,
        "EXCEPTION #PF cr2={cr2:#018x} error_code={error_code:?}\n  frame: {frame:#?}"
    );
    halt_loop();
}

extern "x86-interrupt" fn breakpoint_handler(frame: InterruptStackFrame) {
    let _ = writeln!(
        serial::Writer,
        "EXCEPTION #BP at {:#018x}",
        frame.instruction_pointer.as_u64()
    );
}

extern "x86-interrupt" fn invalid_opcode_handler(frame: InterruptStackFrame) {
    let _ = writeln!(serial::Writer, "EXCEPTION #UD\n  frame: {frame:#?}");
    halt_loop();
}

extern "x86-interrupt" fn divide_by_zero_handler(frame: InterruptStackFrame) {
    let _ = writeln!(serial::Writer, "EXCEPTION #DE\n  frame: {frame:#?}");
    halt_loop();
}

static IDT: Mutex<InterruptDescriptorTable> = Mutex::new(InterruptDescriptorTable::new());

/// Tracks whether the IDT's static-vector entries have been wired.
/// init runs once per core (BSP + each AP) and LIDTs; the in-memory
/// table only needs entries populated the first time.
static IDT_POPULATED: AtomicU8 = AtomicU8::new(0);

/// First vector available to dynamic allocation. Skips CPU exception
/// vectors (0x00..0x1F), the legacy-ISA range (0x20..0x2F, where the
/// 4-5 keyboard at 0x21 lives), and a generous bottom-end safety
/// margin. Real PCIe IRQ vectors typically live above 0x30 on every
/// commodity x86_64 OS; we pad to 0x40.
#[allow(dead_code)]
const DYN_VECTOR_FIRST: u8 = 0x40;

/// Last vector available to dynamic allocation. Skips the LAPIC
/// timer (0xEF) and spurious (0xFF) vectors that 3F wired at M0.
#[allow(dead_code)]
const DYN_VECTOR_LAST: u8 = 0xEE;

#[allow(dead_code)]
static NEXT_DYN_VECTOR: AtomicU8 = AtomicU8::new(DYN_VECTOR_FIRST);

/// Install a handler at a freshly-allocated IDT vector and return
/// the vector. Allocates from a 175-vector pool (0x40..=0xEE) that
/// pads against M0's keyboard / timer / spurious choices. Panics
/// when the pool is exhausted — out-of-vector is a real bug, not a
/// graceful-degradation case.
///
/// The atomic vector counter advances monotonically; vectors are
/// not reclaimed at M1. M2's hot-unplug device flow may need a
/// release path, but until then unique allocation per call is
/// simpler and the 175-vector cap is far above what M1's driver
/// fleet (NVMe, xHCI, virtio-gpu, amdgpu, iwlwifi) consumes.
#[allow(dead_code)] // M1-1-4 (NVMe MSI-X), M1-3+ (xHCI), M1-4+ (virtio-gpu) consume.
pub fn register_vector(handler: HandlerFunc) -> u8 {
    let vector = NEXT_DYN_VECTOR.fetch_add(1, Ordering::Relaxed);
    assert!(
        vector <= DYN_VECTOR_LAST,
        "idt: dynamic vector pool exhausted at vector {vector:#x}"
    );

    let _g = irq::IrqGuard::save_and_disable();
    let mut idt = IDT.lock();
    idt[vector].set_handler_fn(handler);
    drop(idt);

    let _ = writeln!(serial::Writer, "idt: registered vector {vector:#04x}");
    vector
}

fn populate_static_entries(idt: &mut InterruptDescriptorTable) {
    idt.divide_error.set_handler_fn(divide_by_zero_handler);
    idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);
    idt.breakpoint.set_handler_fn(breakpoint_handler);
    idt.general_protection_fault
        .set_handler_fn(general_protection_handler);
    idt.page_fault.set_handler_fn(page_fault_handler);

    // SAFETY: gdt::init() runs before idt::init() and populated
    // tss.interrupt_stack_table[0..3] with valid stack tops. The
    // IST indices below refer to those entries.
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(gdt::DOUBLE_FAULT_IST);
        idt.non_maskable_interrupt
            .set_handler_fn(non_maskable_handler)
            .set_stack_index(gdt::NMI_IST);
        idt.machine_check
            .set_handler_fn(machine_check_handler)
            .set_stack_index(gdt::MACHINE_CHECK_IST);
    }

    // 3F-1: LAPIC spurious vector. The handler does nothing but log
    // the first occurrence; spurious delivery requires no EOI per
    // Intel SDM Vol. 3A §10.9.
    idt[apic::SPURIOUS_VECTOR].set_handler_fn(apic::spurious_handler);

    // 3F-2: LAPIC periodic timer at 100 Hz. The handler increments
    // a tick counter and dispatches to sched::preempt (4-4).
    idt[apic::TIMER_VECTOR].set_handler_fn(apic::timer_handler);

    // 4-5: keyboard IRQ at vector 0x21. The IOAPIC redirection-table
    // entry for the keyboard's GSI is masked at 4-3's ioapic::init
    // and unmasked by kbd::init_irq.
    idt[apic::KEYBOARD_VECTOR].set_handler_fn(kbd::keyboard_handler);
}

pub fn init() {
    // SAFETY: load_unsafe is required because our IDT lives behind
    // a Mutex (not exposed as &'static) so we cannot call the
    // `load()` variant that wants &'static self. The static IDT
    // Mutex outlives every IRQ (it lives until process termination,
    // which on bare metal is "never"), so the load_unsafe contract
    // — the IDT must outlive any interrupt that might fire — is
    // satisfied by the Mutex's static lifetime. Each core LIDTs
    // independently; the BSP at boot, each AP from ap_entry.
    let mut idt = IDT.lock();

    // Only the first caller populates static entries; subsequent
    // cores (APs at 4-2) just LIDT to the already-populated table.
    if IDT_POPULATED
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        populate_static_entries(&mut idt);
    }

    // SAFETY: the IDT lives in a static Mutex; its backing memory
    // outlives every interrupt. load_unsafe writes IDTR with the
    // table's base address; subsequent interrupts dispatch through
    // it. We hold the lock only to safely take &mut for population;
    // load_unsafe takes &self so the lock is incidental for the
    // load step itself.
    unsafe { idt.load_unsafe() };
}
