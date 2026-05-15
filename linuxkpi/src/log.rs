// SPDX-License-Identifier: BSD-2-Clause

//! Linux `printk` + `pr_*` family routed to the kernel's serial
//! sink. M1-2-1 minimal: no varargs, no format-string expansion.
//! The Rust side accepts a NUL-terminated C string and writes its
//! bytes to serial. Linux's KERN_* level prefix (a SOH (0x01) byte
//! followed by an ASCII digit '0'..'7' at the start of the format
//! string) is detected and translated to a `[LEVEL]` tag prepended
//! to the message; unrecognized prefix bytes pass through
//! unmodified.
//!
//! Varargs `printk(fmt, ...)` for inherited C consumers lands at
//! M1-2-4 when the first inherited driver (virtio-balloon) calls
//! `printk` with format specifiers. The HANDOFF M1-2-1 failure
//! mode (g) is the long-tail format-specifier coverage; M1-2-1
//! ships the prefix detection + byte passthrough path that all
//! later format-string work builds on.
//!
//! The sink itself lives in arsenal-kernel — `linuxkpi` would
//! reimplement COM1 I/O port writes otherwise, breaking the
//! kernel's implicit single-writer-to-COM1 invariant. The link
//! step resolves `linuxkpi_serial_sink` against arsenal-kernel's
//! definition; calling `printk` before that symbol is wired
//! results in a link error, not a runtime panic.

use crate::types::{c_char, c_int};

// SOH + ASCII digit per Linux <linux/kern_levels.h>.
pub const KERN_SOH_BYTE: u8 = 0x01;
pub const KERN_EMERG_LEVEL: u8 = b'0';
pub const KERN_ALERT_LEVEL: u8 = b'1';
pub const KERN_CRIT_LEVEL: u8 = b'2';
pub const KERN_ERR_LEVEL: u8 = b'3';
pub const KERN_WARNING_LEVEL: u8 = b'4';
pub const KERN_NOTICE_LEVEL: u8 = b'5';
pub const KERN_INFO_LEVEL: u8 = b'6';
pub const KERN_DEBUG_LEVEL: u8 = b'7';

// Symbol arsenal-kernel must provide. Implementation lives in
// arsenal-kernel/src/serial.rs as a `pub extern "C" fn` with the
// matching signature; the link step resolves it.
unsafe extern "C" {
    fn linuxkpi_serial_sink(ptr: *const u8, len: usize);
}

fn write_to_sink(bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    // SAFETY: ptr/len describe a valid borrow of `bytes` for the
    // duration of the call. arsenal-kernel's sink does not retain
    // the pointer past the call.
    unsafe { linuxkpi_serial_sink(bytes.as_ptr(), bytes.len()) }
}

/// Linux `printk` — minimal C-callable variant. Reads a NUL-
/// terminated C string from `fmt`, detects the `KERN_*` prefix if
/// present, prepends a `[LEVEL]` tag, and writes the message to
/// serial.
///
/// Returns the number of payload bytes written (excludes the
/// `[LEVEL]` tag).
///
/// # Safety
/// `fmt` must point to a valid NUL-terminated C string for the
/// duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn printk(fmt: *const c_char) -> c_int {
    if fmt.is_null() {
        return 0;
    }
    // SAFETY: caller guarantees a valid NUL-terminated C string.
    let bytes = unsafe { core::ffi::CStr::from_ptr(fmt) }.to_bytes();

    let (level_tag, body) = strip_kern_level(bytes);
    if let Some(tag) = level_tag {
        write_to_sink(b"[");
        write_to_sink(tag);
        write_to_sink(b"] ");
    }
    write_to_sink(body);

    body.len() as c_int
}

fn strip_kern_level(bytes: &[u8]) -> (Option<&'static [u8]>, &[u8]) {
    if bytes.len() < 2 || bytes[0] != KERN_SOH_BYTE {
        return (None, bytes);
    }
    let tag: &'static [u8] = match bytes[1] {
        KERN_EMERG_LEVEL => b"EMERG",
        KERN_ALERT_LEVEL => b"ALERT",
        KERN_CRIT_LEVEL => b"CRIT",
        KERN_ERR_LEVEL => b"ERR",
        KERN_WARNING_LEVEL => b"WARN",
        KERN_NOTICE_LEVEL => b"NOTICE",
        KERN_INFO_LEVEL => b"INFO",
        KERN_DEBUG_LEVEL => b"DEBUG",
        _ => return (None, bytes),
    };
    (Some(tag), &bytes[2..])
}

/// Rust-side `printk` for callers inside `linuxkpi` and the kernel
/// crate. Takes a byte slice with no NUL-termination requirement.
/// The `KERN_*` prefix detection works identically.
pub fn pr(bytes: &[u8]) {
    let (level_tag, body) = strip_kern_level(bytes);
    if let Some(tag) = level_tag {
        write_to_sink(b"[");
        write_to_sink(tag);
        write_to_sink(b"] ");
    }
    write_to_sink(body);
}
