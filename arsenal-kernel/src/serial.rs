// SPDX-License-Identifier: BSD-2-Clause
//
// Minimal COM1 driver — write-only, polled, no IRQ. Exists solely to
// emit the ARSENAL_BOOT_OK sentinel for the smoke test (C-5). Replace
// with a proper UART driver when Operator (terminal) needs reads,
// interrupts, or contention-safe access.

use core::arch::asm;

const COM1_BASE: u16 = 0x3F8;
const PORT_DATA: u16 = COM1_BASE;     // and DLL when DLAB=1
const PORT_IER: u16 = COM1_BASE + 1;  // and DLH when DLAB=1
const PORT_FCR: u16 = COM1_BASE + 2;
const PORT_LCR: u16 = COM1_BASE + 3;
const PORT_MCR: u16 = COM1_BASE + 4;
const PORT_LSR: u16 = COM1_BASE + 5;

const LSR_THRE: u8 = 1 << 5; // Transmitter Holding Register Empty

pub fn init() {
    // SAFETY: COM1 (0x3F8..0x3FF) is a reserved x86 ISA I/O port range
    // dedicated to a 16550-compatible UART. The values below configure
    // the UART for 115200 baud, 8N1, FIFOs on, IRQs off, per the 16550
    // data sheet (TI SPRG228 Table 4-1). No other hardware aliases
    // these ports.
    unsafe {
        outb(PORT_IER, 0x00);  // disable IRQs
        outb(PORT_LCR, 0x80);  // enable DLAB
        outb(PORT_DATA, 0x01); // divisor low (115200 baud)
        outb(PORT_IER, 0x00);  // divisor high
        outb(PORT_LCR, 0x03);  // 8N1, DLAB off
        outb(PORT_FCR, 0xC7);  // FIFO on, clear, 14-byte threshold
        outb(PORT_MCR, 0x0B);  // RTS/DTR set, OUT2 set
    }
}

pub fn write_str(s: &str) {
    for byte in s.bytes() {
        write_byte(byte);
    }
    // Mirror to the framebuffer console. fb::print_str gates on
    // FB_READY internally — calls before fb::init are silent
    // no-ops, so this is safe at ARSENAL_BOOT_OK time. Serial
    // lands first so the headless smoke sees sentinels at full
    // UART speed; the mirror's per-glyph pixel writes only
    // affect what shows under -display gtk/sdl.
    crate::fb::print_str(s);
}

/// Byte-oriented sink — emit `bytes` to COM1 verbatim. M1-2-1's
/// linuxkpi shim routes its `printk` output through this path
/// (via the `linuxkpi_serial_sink` extern below) because
/// inherited C may emit non-UTF-8 bytes that `write_str`'s &str
/// argument cannot carry. The fb mirror is not driven from here
/// — non-UTF-8 bytes have no meaningful glyph mapping; serial-
/// only output is the right shim diagnostic shape.
pub fn write_bytes(bytes: &[u8]) {
    for &byte in bytes {
        write_byte(byte);
    }
}

/// Sink symbol the linuxkpi crate calls into. The link step
/// resolves linuxkpi's `extern "C" fn linuxkpi_serial_sink` to
/// this definition; calling it before `serial::init` is well-
/// defined (write_bytes spins on LSR_THRE, which reads as
/// "ready" only after the UART is up — pre-init writes will
/// hang, which is the correct loud-failure mode for a missing
/// boot-order init).
///
/// # Safety
/// `ptr` + `len` must describe a valid byte slice for the
/// duration of the call. The caller (linuxkpi::log::printk and
/// friends) constructs the slice from a NUL-terminated CStr or
/// a `&[u8]` borrow, both of which satisfy the invariant.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_serial_sink(ptr: *const u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    // SAFETY: caller's contract — ptr/len describe a valid byte
    // slice. write_bytes does not retain the slice past the call.
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    write_bytes(bytes);
}

/// `core::fmt::Write` adapter so `write!` / `writeln!` work against COM1.
pub struct Writer;

impl core::fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        write_str(s);
        Ok(())
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
