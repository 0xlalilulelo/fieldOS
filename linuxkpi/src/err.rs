// SPDX-License-Identifier: BSD-2-Clause

//! Linux pointer-encoded errnos — `IS_ERR` / `ERR_PTR` /
//! `PTR_ERR`. Linux <linux/err.h> encodes negative errnos as
//! pointers in the high-bit-set range (so the upper 4096 bytes
//! of address space are reserved for error returns); test
//! whether a pointer is an error via `IS_ERR`, recover the errno
//! via `PTR_ERR`, encode an errno as a pointer via `ERR_PTR`.
//!
//! M1-2-5 Part A — translated from the canonical kernel-docs
//! description without copying upstream Linux source verbatim.
//!
//! All three helpers are trivial; we ship them as `extern "C"`
//! fns rather than C-side `static inline` so the C-side header
//! can reference them as plain externs (matches our shim's
//! discipline of "real fns, not preprocessor magic, when
//! possible").

use crate::types::{c_int, c_long, c_void};

/// Maximum errno encoded as a pointer. Linux convention: the
/// top 4 KiB of address space is reserved for error pointers.
pub const MAX_ERRNO: u64 = 4095;

/// Encode `error` (a negative errno) as a pointer.
///
/// # Safety
/// `error` should be in the range `-MAX_ERRNO..=0`; values
/// outside that range produce pointers that may be confused
/// with real allocations (Linux convention; not enforced).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ERR_PTR(error: c_long) -> *mut c_void {
    error as usize as *mut c_void
}

/// Recover the errno from a pointer previously produced by
/// `ERR_PTR`. The result is meaningful only when `IS_ERR(ptr)`
/// returned true.
///
/// # Safety
/// `ptr` must be either a valid heap pointer or an `ERR_PTR`
/// encoding (Linux's `IS_ERR` discriminates).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn PTR_ERR(ptr: *const c_void) -> c_long {
    ptr as usize as c_long
}

/// Test whether `ptr` is an `ERR_PTR`-encoded errno (i.e.,
/// in the high-bit-set range). Returns 1 (true) or 0 (false).
///
/// # Safety
/// `ptr` need not be valid memory; the test is purely numeric.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn IS_ERR(ptr: *const c_void) -> c_int {
    if (ptr as u64) >= (0u64.wrapping_sub(MAX_ERRNO)) { 1 } else { 0 }
}

/// Convenience: returns 1 if `ptr` is `NULL` or an error
/// pointer. Linux's `IS_ERR_OR_NULL`.
///
/// # Safety
/// As `IS_ERR`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn IS_ERR_OR_NULL(ptr: *const c_void) -> c_int {
    if ptr.is_null() {
        return 1;
    }
    // SAFETY: see IS_ERR.
    unsafe { IS_ERR(ptr) }
}
