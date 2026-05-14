// SPDX-License-Identifier: BSD-2-Clause
//
// PS/2 keyboard driver. M0 step 4-5 replaces 3G-0's cooperative
// polling with an IOAPIC-routed IRQ1 handler that decodes scancodes
// and pushes ASCII bytes into a single-producer / single-consumer
// ring buffer. The shell task at shell.rs reads via recv_blocking,
// which yields cooperatively while the ring is empty.
//
// The i8042 controller lives at I/O ports 0x60 (data) and 0x64
// (status / command). On QEMU q35, and on every commodity x86
// motherboard since the IBM PC/AT in 1984, the controller is alive
// at boot with the keyboard on port 1 and scancode set 1 translation
// active inside the controller. `init` trusts that configuration;
// it drains any bytes firmware left in the output buffer, logs the
// count, then `init_irq` programs the IOAPIC redirection-table
// entry for the keyboard's GSI to deliver vector 0x21 to the BSP.
//
// IRQ delivery model: the IOAPIC routes IRQ1 → GSI N (typically N=1
// on QEMU q35; the MADT Interrupt Source Override table from 4-0
// is consulted at init_irq for non-identity firmware). Each
// keyboard byte triggers one edge-triggered IRQ; the handler reads
// one byte, decodes through the same scancode → ASCII state machine
// the polled path used, pushes the resulting ASCII byte (if any)
// to the ring, and EOIs the LAPIC.
//
// Permanently out of scope for M0: scancode set 2, USB HID, PS/2
// mouse on port 2, controller self-test / reconfigure sequences,
// extended (E0/E1) scancode interpretation past consume-and-ignore.

use core::arch::asm;
use core::cell::UnsafeCell;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

use x86_64::structures::idt::InterruptStackFrame;

use crate::acpi;
use crate::apic;
use crate::ioapic;
use crate::sched;
use crate::serial;

const PS2_DATA: u16 = 0x60;
const PS2_STATUS: u16 = 0x64;

/// Status register bit 0 — Output Buffer Full. Set when there is
/// a byte in the controller-to-host buffer at port 0x60 to read.
const STATUS_OBF: u8 = 1 << 0;

/// Scancode set 1 release-bit mask. Press codes are 0x01..0x58;
/// release codes are press + 0x80. (Extended-set release codes
/// follow E0/E1 prefixes and are absorbed by the state machine
/// below before this mask matters.)
const SC_RELEASE_MASK: u8 = 0x80;

const SC_EXTENDED_E0: u8 = 0xE0;
const SC_EXTENDED_E1: u8 = 0xE1;

const SC_LSHIFT_PRESS: u8 = 0x2A;
const SC_LSHIFT_RELEASE: u8 = 0xAA;
const SC_RSHIFT_PRESS: u8 = 0x36;
const SC_RSHIFT_RELEASE: u8 = 0xB6;

/// Maximum bytes drained from the output buffer at init. Firmware
/// rarely leaves more than a key-release or two; a hard cap keeps
/// `init` bounded even if the controller is wedged.
const INIT_DRAIN_BUDGET: usize = 16;

/// ISA IRQ number for the i8042 keyboard. The IOAPIC GSI it maps
/// to comes from the MADT Interrupt Source Override table (4-0);
/// identity-mapped on QEMU q35 (GSI 1) absent an override.
const KEYBOARD_ISA_IRQ: u8 = 1;

/// Scancode set 1 → unshifted ASCII. Indexed by the press scancode
/// byte (0x00..=0x58). A 0 entry means "no printable character at
/// M0" — modifier keys, function keys, CapsLock, numpad without
/// NumLock translation. The shell layer interprets 0x08 (BS),
/// 0x0A (LF), 0x09 (HT), 0x1B (ESC) as line-edit signals.
static SC_TO_ASCII: [u8; 0x59] = [
    0,    0x1B, b'1',  b'2',  b'3', b'4', b'5', b'6', // 0x00..0x07
    b'7', b'8', b'9',  b'0',  b'-', b'=', 0x08, b'\t', // 0x08..0x0F
    b'q', b'w', b'e',  b'r',  b't', b'y', b'u', b'i', // 0x10..0x17
    b'o', b'p', b'[',  b']',  b'\n', 0,   b'a', b's', // 0x18..0x1F  (0x1D = LCtrl)
    b'd', b'f', b'g',  b'h',  b'j', b'k', b'l', b';', // 0x20..0x27
    b'\'', b'`', 0,    b'\\', b'z', b'x', b'c', b'v', // 0x28..0x2F  (0x2A = LShift)
    b'b', b'n', b'm',  b',',  b'.', b'/', 0,    b'*', // 0x30..0x37  (0x36 = RShift)
    0,    b' ', 0,     0,     0,    0,    0,    0,    // 0x38..0x3F  (LAlt / CapsLock / F1..F5)
    0,    0,    0,     0,     0,    0,    0,    0,    // 0x40..0x47  (F6..F10 / NumLock / ScrollLock / KP_7)
    0,    0,    0,     0,     0,    0,    0,    0,    // 0x48..0x4F  (numpad)
    0,    0,    0,     0,     0,    0,    0,    0,    // 0x50..0x57
    0,                                                  // 0x58       (F12)
];

/// Scancode set 1 → shifted ASCII. Same indexing as `SC_TO_ASCII`.
/// Differs only at the keys where shift produces a different
/// printable character; alphabetic keys uppercase.
static SC_TO_ASCII_SHIFTED: [u8; 0x59] = [
    0,    0x1B, b'!',  b'@',  b'#', b'$', b'%', b'^',  // 0x00..0x07
    b'&', b'*', b'(',  b')',  b'_', b'+', 0x08, b'\t',  // 0x08..0x0F
    b'Q', b'W', b'E',  b'R',  b'T', b'Y', b'U', b'I',  // 0x10..0x17
    b'O', b'P', b'{',  b'}',  b'\n', 0,   b'A', b'S',  // 0x18..0x1F
    b'D', b'F', b'G',  b'H',  b'J', b'K', b'L', b':',  // 0x20..0x27
    b'"', b'~', 0,     b'|',  b'Z', b'X', b'C', b'V',  // 0x28..0x2F
    b'B', b'N', b'M',  b'<',  b'>', b'?', 0,    b'*',  // 0x30..0x37
    0,    b' ', 0,     0,     0,    0,    0,    0,    // 0x38..0x3F
    0,    0,    0,     0,     0,    0,    0,    0,    // 0x40..0x47
    0,    0,    0,     0,     0,    0,    0,    0,    // 0x48..0x4F
    0,    0,    0,     0,     0,    0,    0,    0,    // 0x50..0x57
    0,                                                  // 0x58
];

/// Set when the last consumed byte was 0xE0. The next byte (a
/// single scancode in the extended set) gets absorbed and ignored
/// at M0.
static IN_E0_SEQUENCE: AtomicBool = AtomicBool::new(false);

/// Number of bytes remaining to absorb in an E1 (Pause/Break)
/// sequence. The Pause key sends `E1 1D 45 E1 9D C5` — two
/// scancodes after each E1 prefix. We absorb 2 bytes per E1.
static IN_E1_SEQUENCE_REMAINING: AtomicU8 = AtomicU8::new(0);

static LSHIFT_DOWN: AtomicBool = AtomicBool::new(false);
static RSHIFT_DOWN: AtomicBool = AtomicBool::new(false);

/// SPSC ring buffer for decoded ASCII bytes. Producer: the IRQ
/// handler `keyboard_handler` (BSP only). Consumer: the shell task
/// via recv_blocking. Lock-free using head / tail AtomicUsize; the
/// IRQ handler never spins (drops the byte on overflow), so it
/// cannot deadlock against a cooperative-context consumer.
const RING_SIZE: usize = 256;

struct KbdRing {
    buf: UnsafeCell<[u8; RING_SIZE]>,
    head: AtomicUsize,
    tail: AtomicUsize,
}

// SAFETY: producer (IRQ handler) and consumer (cooperative shell)
// coordinate via head/tail; neither ever writes the slot the other
// is reading. The static lives in .data with internal mutability.
unsafe impl Sync for KbdRing {}

impl KbdRing {
    const fn new() -> Self {
        Self {
            buf: UnsafeCell::new([0u8; RING_SIZE]),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }
}

static RING: KbdRing = KbdRing::new();
static DROPPED: AtomicUsize = AtomicUsize::new(0);

fn ring_push(b: u8) {
    let head = RING.head.load(Ordering::Relaxed);
    let tail = RING.tail.load(Ordering::Acquire);
    let next_head = (head + 1) % RING_SIZE;
    if next_head == tail {
        DROPPED.fetch_add(1, Ordering::Relaxed);
        return;
    }
    // SAFETY: producer owns the slot at `head` until the Release
    // store below makes it visible to the consumer. The Acquire
    // load of tail above ensures we don't overwrite an unread byte.
    unsafe {
        (*RING.buf.get())[head] = b;
    }
    RING.head.store(next_head, Ordering::Release);
}

fn ring_pop() -> Option<u8> {
    let tail = RING.tail.load(Ordering::Relaxed);
    let head = RING.head.load(Ordering::Acquire);
    if head == tail {
        return None;
    }
    // SAFETY: consumer owns the slot at `tail` until the Release
    // store below makes it visible to the producer. The Acquire
    // load of head above ensures the slot has been written.
    let b = unsafe { (*RING.buf.get())[tail] };
    let next_tail = (tail + 1) % RING_SIZE;
    RING.tail.store(next_tail, Ordering::Release);
    Some(b)
}

/// Bring the keyboard online. Drains any pending bytes left in the
/// output buffer by firmware / Limine (the BIOS Int 16h path can
/// leave a key-release or two queued), logs the count.
///
/// IOAPIC routing for IRQ1 is programmed by init_irq, called after
/// init returns and after ioapic::init has masked all entries.
pub fn init() {
    let mut drained = 0usize;
    while drained < INIT_DRAIN_BUDGET {
        // SAFETY: 0x64 is the i8042 status register per the IBM PC/AT
        // system reference (1984) and every subsequent x86 chipset.
        // Reads are side-effect-free; bit 0 (OBF) is the documented
        // "data ready to read" flag.
        let status = unsafe { inb(PS2_STATUS) };
        if status & STATUS_OBF == 0 {
            break;
        }
        // SAFETY: 0x60 is the i8042 data port. Reading clears OBF
        // and consumes one byte from the controller-to-host buffer.
        let _ = unsafe { inb(PS2_DATA) };
        drained += 1;
    }

    let _ = writeln!(
        serial::Writer,
        "kbd: i8042 ready (drained {drained} pending byte{})",
        if drained == 1 { "" } else { "s" },
    );
}

/// Program the IOAPIC redirection-table entry for the keyboard
/// GSI to deliver vector 0x21 to the BSP. Must run after
/// apic::init (we need the BSP's APIC ID), acpi::init (we read
/// the IRQ override table for the GSI), and ioapic::init (which
/// installed the masked baseline).
pub fn init_irq() {
    let bsp = apic::lapic_id();
    let gsi = acpi::irq_override(KEYBOARD_ISA_IRQ)
        .map(|o| o.gsi)
        .unwrap_or(KEYBOARD_ISA_IRQ as u32);
    ioapic::program(gsi, apic::KEYBOARD_VECTOR, bsp);
    let _ = writeln!(
        serial::Writer,
        "kbd: irq routed (isa_irq={KEYBOARD_ISA_IRQ} -> gsi={gsi} -> \
         vector={:#x} -> apic_id={bsp})",
        apic::KEYBOARD_VECTOR,
    );
}

/// Block (cooperatively yielding) until one decoded ASCII byte is
/// available, then return it. The shell task is the only caller on
/// single-core M0.
pub fn recv_blocking() -> u8 {
    loop {
        if let Some(b) = ring_pop() {
            return b;
        }
        sched::yield_now();
    }
}

/// IDT handler for vector 0x21 — keyboard IRQ. Reads one byte from
/// the i8042 data port (the IRQ delivery implied OBF=1), runs it
/// through the scancode → ASCII state machine, pushes a decoded
/// byte to the ring if one resulted, EOIs the LAPIC.
pub extern "x86-interrupt" fn keyboard_handler(_frame: InterruptStackFrame) {
    // SAFETY: keyboard IRQ delivery implies OBF=1 in the i8042
    // status register; reading 0x60 consumes the byte and clears
    // OBF. The i8042's port assignment is universal x86 ISA.
    let sc = unsafe { inb(PS2_DATA) };
    if let Some(b) = decode_scancode(sc) {
        ring_push(b);
    }
    apic::send_eoi();
}

/// Run one scancode byte through the M0 state machine. Returns the
/// resulting ASCII byte if the byte completes a printable / line-
/// edit press; returns None for modifier-state updates, release
/// codes, extended-sequence absorption, and unmapped scancodes.
fn decode_scancode(sc: u8) -> Option<u8> {
    // Extended-sequence absorption takes precedence over every
    // other interpretation: an E1-followed-by-two-bytes pattern
    // must finish even if one of those bytes happens to match a
    // shift release (0xAA) or any other special code.
    let e1_remaining = IN_E1_SEQUENCE_REMAINING.load(Ordering::Relaxed);
    if e1_remaining > 0 {
        IN_E1_SEQUENCE_REMAINING.store(e1_remaining - 1, Ordering::Relaxed);
        return None;
    }
    if IN_E0_SEQUENCE.swap(false, Ordering::Relaxed) {
        return None;
    }

    match sc {
        SC_EXTENDED_E0 => {
            IN_E0_SEQUENCE.store(true, Ordering::Relaxed);
            return None;
        }
        SC_EXTENDED_E1 => {
            IN_E1_SEQUENCE_REMAINING.store(2, Ordering::Relaxed);
            return None;
        }
        SC_LSHIFT_PRESS => {
            LSHIFT_DOWN.store(true, Ordering::Relaxed);
            return None;
        }
        SC_LSHIFT_RELEASE => {
            LSHIFT_DOWN.store(false, Ordering::Relaxed);
            return None;
        }
        SC_RSHIFT_PRESS => {
            RSHIFT_DOWN.store(true, Ordering::Relaxed);
            return None;
        }
        SC_RSHIFT_RELEASE => {
            RSHIFT_DOWN.store(false, Ordering::Relaxed);
            return None;
        }
        _ => {}
    }

    // Release byte for any non-modifier key: ignore. Press handling
    // below covers the only press codes we care about for M0.
    if sc & SC_RELEASE_MASK != 0 {
        return None;
    }

    let idx = sc as usize;
    if idx >= SC_TO_ASCII.len() {
        return None;
    }
    let shifted = LSHIFT_DOWN.load(Ordering::Relaxed) || RSHIFT_DOWN.load(Ordering::Relaxed);
    let byte = if shifted {
        SC_TO_ASCII_SHIFTED[idx]
    } else {
        SC_TO_ASCII[idx]
    };
    if byte == 0 { None } else { Some(byte) }
}

/// Read a byte from x86 I/O port `port`.
///
/// # Safety
/// Caller must ensure `port` is a valid I/O port. The i8042 reads
/// performed by this module (0x60 data, 0x64 status) are
/// side-effect-free for status and "consume one byte from the
/// controller-to-host buffer" for data — both documented behaviors
/// per the IBM PC/AT and 8042 datasheet.
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
