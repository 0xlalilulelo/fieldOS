// SPDX-License-Identifier: BSD-2-Clause

// 3G-1 will spawn the shell task that calls `poll`; until then,
// only `init` is reachable from main and clippy's dead-code lint
// would flag the scancode tables, modifier state, and `poll`
// itself. Drop this attribute when shell::run lands.
#![allow(dead_code)]

// PS/2 keyboard driver — M0 step 3G-0. Polled-only. The shell task
// (3G-1) calls `poll` on each cooperative iteration; if a scancode
// is pending, it is consumed, translated to ASCII, and returned.
// Otherwise `poll` returns None and the shell yields. With idle's
// 100 Hz `hlt`-wake from 3F-3 and the cooperative round-robin from
// 3B, the shell is scheduled at least every ~10 ms, giving an
// effective 100 Hz polling rate — well above human typing speed.
//
// IRQ-driven input is deferred to M0 step 4 because it requires
// IOAPIC bring-up to route IRQ1 through the LAPIC (the 8259 was
// masked at 3F-0 and we deliberately do not re-introduce it as a
// delivery path).
//
// The i8042 controller lives at I/O ports 0x60 (data) and 0x64
// (status / command). On QEMU q35, and on every commodity x86
// motherboard since the IBM PC/AT in 1984, the controller is alive
// at boot with the keyboard on port 1 and scancode set 1
// translation active inside the controller. `init` trusts that
// configuration; it drains any bytes firmware left in the output
// buffer and logs.
//
// Permanently out of scope for 3G-0: scancode set 2, USB HID,
// PS/2 mouse on port 2, controller self-test / reconfigure
// sequences, extended (E0/E1) scancode interpretation past
// consume-and-ignore. The E0/E1 framing is recognized so the
// state machine does not get confused; the meaning of arrow
// keys / numpad / Pause is post-M0.

use core::arch::asm;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

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
/// at M0. Atomic for SMP-readiness at step 4; Relaxed because the
/// shell task is the only caller of `poll` on single-core M0.
static IN_E0_SEQUENCE: AtomicBool = AtomicBool::new(false);

/// Number of bytes remaining to absorb in an E1 (Pause/Break)
/// sequence. The Pause key sends `E1 1D 45 E1 9D C5` — two
/// scancodes after the E1 prefix on press, then a release pattern
/// with another E1. We absorb 2 bytes per E1; the release E1
/// arrives later and gets its own count. AtomicU8 because the
/// remaining count is at most 2.
static IN_E1_SEQUENCE_REMAINING: AtomicU8 = AtomicU8::new(0);

static LSHIFT_DOWN: AtomicBool = AtomicBool::new(false);
static RSHIFT_DOWN: AtomicBool = AtomicBool::new(false);

/// Bring the keyboard online. Drains any pending bytes left in the
/// output buffer by firmware / Limine (the BIOS Int 16h path can
/// leave a key-release or two queued), logs the count.
///
/// No reconfiguration of the controller, no self-test sequence, no
/// scancode-set programming — QEMU q35 and every real x86 board
/// boots with sensible defaults and the M0 scope explicitly trusts
/// that. The right place to revisit is M1 when real-hardware quirks
/// (e.g. legacy-free SoCs that emulate i8042 imperfectly) start
/// surfacing.
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
        // SAFETY: 0x60 is the i8042 data port. Reading clears OBF and
        // consumes one byte from the controller-to-host buffer. The
        // byte's meaning (scancode, ACK, response to a controller
        // command) is whatever the controller most recently queued —
        // at init time we are discarding it deliberately.
        let _ = unsafe { inb(PS2_DATA) };
        drained += 1;
    }

    let _ = writeln!(
        serial::Writer,
        "kbd: i8042 ready (drained {drained} pending byte{})",
        if drained == 1 { "" } else { "s" },
    );
}

/// Poll the keyboard once. Returns the next printable / line-edit
/// ASCII byte if input is pending, else None. Modifier state and
/// extended scancode framing are absorbed internally.
///
/// Cooperative-context only. The shell task is the only caller on
/// single-core M0; if step 4's preempted scheduler grows additional
/// callers, the module-level atomics already use Relaxed loads /
/// stores so adding a per-CPU input queue is the natural next move
/// (not a lock around this function).
pub fn poll() -> Option<u8> {
    // SAFETY: same as in `init` — 0x64 status read is side-effect-free.
    if unsafe { inb(PS2_STATUS) } & STATUS_OBF == 0 {
        return None;
    }
    // SAFETY: same as in `init` — 0x60 read consumes one scancode.
    let sc = unsafe { inb(PS2_DATA) };

    // Extended-sequence absorption. Take precedence over every other
    // interpretation: an E1-followed-by-two-bytes pattern must finish
    // even if one of those bytes happens to match LSHIFT_RELEASE
    // (0xAA) or any other special code.
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
    // below covers the only press codes we care about for M0
    // (printable ASCII + the line-edit specials).
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
/// controller-to-host buffer" for data — both documented
/// behaviors per the IBM PC/AT and 8042 datasheet.
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
