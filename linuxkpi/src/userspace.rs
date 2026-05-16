// SPDX-License-Identifier: BSD-2-Clause

//! Linux userspace-copy primitives — `copy_to_user`,
//! `copy_from_user`, `get_user`, `put_user`. M1 has no
//! userspace yet (Cardboard Box capability sandbox arrives
//! at M2); these symbols exist so inherited C linking
//! against shim_c.h finds them, but any call panics with
//! a clear message.
//!
//! Fail-loud-not-silent: a quiet 0 return would be the worst
//! possible behavior for these — drivers would think the copy
//! succeeded and proceed with garbage data. Panic is correct.
//!
//! When userspace arrives at M2 with the Cardboard Box runtime,
//! these get real implementations that validate the user-pointer
//! is in the calling process's address space, perform the copy
//! with page-fault handling, return the byte count not copied
//! (Linux convention: returns 0 on full success, > 0 on partial).

use crate::types::{c_ulong, c_void};

/// Copy `n` bytes from kernel `from` to user-space `to`.
/// Linux returns the number of bytes NOT copied (0 = success).
///
/// # Safety
/// M1 panics on call (no userspace). M2's real implementation
/// requires `to` to point to writable user-mapped memory in
/// the calling process's address space, `from` to point to
/// readable kernel memory of size `n`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn copy_to_user(
    _to: *mut c_void,
    _from: *const c_void,
    _n: c_ulong,
) -> c_ulong {
    panic!("linuxkpi: copy_to_user called before userspace exists (M2 work)")
}

/// Copy `n` bytes from user-space `from` to kernel `to`.
/// Linux returns the number of bytes NOT copied (0 = success).
///
/// # Safety
/// As `copy_to_user`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn copy_from_user(
    _to: *mut c_void,
    _from: *const c_void,
    _n: c_ulong,
) -> c_ulong {
    panic!("linuxkpi: copy_from_user called before userspace exists (M2 work)")
}

/// `get_user(x, ptr)` — read one value from user-space.
/// Linux's macro; here a fn that always panics.
///
/// # Safety
/// As `copy_to_user`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_get_user_u8(_ptr: *const u8) -> u8 {
    panic!("linuxkpi: get_user called before userspace exists (M2 work)")
}

/// `put_user(x, ptr)` — write one value to user-space.
///
/// # Safety
/// As `copy_to_user`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_put_user_u8(_val: u8, _ptr: *mut u8) {
    panic!("linuxkpi: put_user called before userspace exists (M2 work)")
}
