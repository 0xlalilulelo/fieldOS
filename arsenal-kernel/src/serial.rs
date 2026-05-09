// SPDX-License-Identifier: BSD-2-Clause
//
// Minimal COM1 driver — write-only, polled, no IRQ. Exists solely to
// emit the ARSENAL_BOOT_OK sentinel for the smoke test (C-5). Replace
// with a proper UART driver when Operator (terminal) needs reads,
// interrupts, or contention-safe access.

use core::arch::asm;

const COM1_BASE: u16 = 0x3F8;
const PORT_DATA: u16 = COM1_BASE;
const PORT_LSR: u16 = COM1_BASE + 5;

const LSR_THRE: u8 = 1 << 5; // Transmitter Holding Register Empty

pub fn init() {
    // SAFETY: COM1 (0x3F8..0x3FF) is a reserved x86 ISA I/O port range
    // dedicated to a 16550-compatible UART. The values below configure
    // the UART for 115200 baud, 8N1, FIFOs on, IRQs off, per the 16550
    // data sheet (TI SPRG228 Table 4-1). No other hardware aliases
    // these ports.
    unsafe {
        outb(COM1_BASE + 1, 0x00); // disable IRQs
        outb(COM1_BASE + 3, 0x80); // enable DLAB
        outb(COM1_BASE + 0, 0x01); // divisor low (115200 baud)
        outb(COM1_BASE + 1, 0x00); // divisor high
        outb(COM1_BASE + 3, 0x03); // 8N1, DLAB off
        outb(COM1_BASE + 2, 0xC7); // FIFO on, clear, 14-byte threshold
        outb(COM1_BASE + 4, 0x0B); // RTS/DTR set, OUT2 set
    }
}

pub fn write_str(s: &str) {
    for byte in s.bytes() {
        write_byte(byte);
    }
}

fn write_byte(b: u8) {
    // SAFETY: same reasoning as init() — COM1 is a dedicated UART.
    // Reading LSR is side-effect-free; writing PORT_DATA transmits.
    while unsafe { inb(PORT_LSR) } & LSR_THRE == 0 {}
    unsafe { outb(PORT_DATA, b) };
}

/// Write `val` to x86 I/O port `port`.
///
/// # Safety
/// Caller must ensure `port` is a valid I/O port and that writing
/// `val` produces the intended hardware effect.
unsafe fn outb(port: u16, val: u8) {
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") val,
            options(nomem, nostack, preserves_flags),
        );
    }
}

/// Read a byte from x86 I/O port `port`.
///
/// # Safety
/// Caller must ensure `port` is a valid I/O port. The COM1 LSR read by
/// this module is side-effect-free per the 16550 spec.
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        asm!(
            "in al, dx",
            out("al") val,
            in("dx") port,
            options(nomem, nostack, preserves_flags),
        );
    }
    val
}
