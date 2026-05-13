// SPDX-License-Identifier: BSD-2-Clause
//
// IDT bring-up. Handlers print exception details to COM1 and halt;
// recoverable exceptions (currently only #BP) print and return so
// callers can use `int3` as a self-test signal. The three faults
// that cannot share the kernel's normal stack — #DF, #NMI, #MC —
// route to their dedicated IST stacks reserved by gdt::init.

use core::fmt::Write;
use spin::Lazy;
use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{
    InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode,
};

use crate::apic;
use crate::gdt;
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

static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();

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
    // Intel SDM Vol. 3A §10.9. Wiring the entry here (inside the
    // Lazy initializer) means later device IRQs added past M0 step 3
    // — which arrive after IDT.load() has already run — will need a
    // different installation mechanism (the IDT is one-shot today).
    idt[apic::SPURIOUS_VECTOR].set_handler_fn(apic::spurious_handler);

    idt
});

pub fn init() {
    IDT.load();
}
