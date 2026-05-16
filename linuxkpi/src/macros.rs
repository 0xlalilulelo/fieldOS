// SPDX-License-Identifier: BSD-2-Clause

//! Linux kernel macros — `container_of`, `BUG_ON`, `WARN_ON`,
//! `WARN_ONCE`. M1-2-5 Part A gap-filling — primitives the
//! HANDOFF M1-2-5 named for "when first inherited driver
//! demands them," translated from the canonical kernel-docs
//! description without copying upstream Linux source verbatim.
//!
//! `container_of` (Rust): a `macro_rules!` form intended for
//! Rust callers inside the shim. The C-side `container_of`
//! macro lives in `linuxkpi/include/shim_c.h` (it's a
//! preprocessor macro by necessity — needs the type and member
//! name as compile-time constants).
//!
//! `BUG_ON` / `WARN_ON`: shim-side `extern "C" fn linuxkpi_bug` /
//! `linuxkpi_warn` that the C-side macros call after evaluating
//! the predicate. `BUG_ON` panics; `WARN_ON` writes a serial
//! warning + returns the predicate value (so inherited C can
//! `if (WARN_ON(cond)) return -EINVAL;`).

use crate::types::{c_char, c_int};

/// Recover a pointer to the containing struct from a pointer to
/// one of its members. Linux <linux/kernel.h> idiom translated
/// to a Rust macro.
///
/// # Safety
/// `ptr` must point to the `member` of a valid `type` instance;
/// the result is a `*const type` pointing to the containing
/// struct, with the same lifetime semantics as `ptr`.
#[macro_export]
macro_rules! container_of {
    ($ptr:expr, $type:ty, $member:ident) => {{
        let ptr = $ptr as *const _ as *const u8;
        let offset = core::mem::offset_of!($type, $member);
        // SAFETY: caller's contract — ptr points to the member
        // at `offset` bytes into a valid `type` instance.
        unsafe { ptr.sub(offset) as *const $type }
    }};
}

/// Inherited C calls this after evaluating the BUG_ON predicate
/// as true. Panics with the file/line/condition formatted into
/// the panic message; the kernel's panic handler takes over.
///
/// # Safety
/// `file` and `cond` must be valid NUL-terminated C strings
/// (typically `__FILE__` and the stringified predicate).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_bug(
    file: *const c_char,
    line: c_int,
    cond: *const c_char,
) -> ! {
    let file_s = cstr_or(file, "<file?>");
    let cond_s = cstr_or(cond, "<cond?>");
    panic!("linuxkpi BUG_ON({cond_s}) at {file_s}:{line}")
}

/// Inherited C calls this after evaluating the WARN_ON
/// predicate as true. Writes a `[WARN] linuxkpi WARN_ON(...)`
/// line to serial; does not panic. Returns to the caller so
/// inherited C can take the `if (WARN_ON(...))` branch.
///
/// # Safety
/// `file` and `cond` must be valid NUL-terminated C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_warn(
    file: *const c_char,
    line: c_int,
    cond: *const c_char,
) {
    let file_s = cstr_or(file, "<file?>");
    let cond_s = cstr_or(cond, "<cond?>");
    crate::log::pr(b"[WARN] linuxkpi WARN_ON(");
    crate::log::pr(cond_s.as_bytes());
    crate::log::pr(b") at ");
    crate::log::pr(file_s.as_bytes());
    crate::log::pr(b":");
    log_decimal_int(line);
    crate::log::pr(b"\n");
}

/// Helper: turn a possibly-null `*const c_char` into a `&str`
/// for diagnostics. Falls back to `default` on null / non-UTF-8.
fn cstr_or(p: *const c_char, default: &'static str) -> &'static str {
    if p.is_null() {
        return default;
    }
    // SAFETY: caller's contract — p is a NUL-terminated C
    // string. CStr::from_ptr walks until NUL. The returned
    // &'static is justified by the inherited driver convention
    // of passing __FILE__ literals (.rodata-resident) — but
    // since we discard the str at end-of-fn after writing to
    // serial, the lifetime caveat is academic.
    let bytes = unsafe { core::ffi::CStr::from_ptr(p).to_bytes() };
    core::str::from_utf8(bytes).unwrap_or(default)
}

fn log_decimal_int(mut n: c_int) {
    if n < 0 {
        crate::log::pr(b"-");
        n = -n;
    }
    let mut n = n as u64;
    if n == 0 {
        crate::log::pr(b"0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    crate::log::pr(&buf[i..]);
}
