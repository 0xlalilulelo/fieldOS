// SPDX-License-Identifier: BSD-2-Clause

//! Linux atomic bit-operations surface (<linux/bitops.h>). Created
//! during M1-2-5 Part B sub-task 3's body-error phase when balloon's
//! config-read path reached test_and_set_bit / test_and_clear_bit;
//! the real atomic implementations land here at the M1-2-5-closing
//! commit.
//!
//! Linux's `unsigned long` bitmaps pack bits little-endian across an
//! array of words: bit `nr` lives in word `nr / BITS_PER_LONG` at bit
//! position `nr % BITS_PER_LONG`. On x86_64 `unsigned long` is 64-bit
//! (LP64), so BITS_PER_LONG = 64. The operations are atomic
//! (LOCK-prefixed read-modify-write via AtomicU64::fetch_or /
//! fetch_and) to honor Linux's contract — balloon's config-read bits
//! can be touched from both the config-changed callback and the
//! stats/size work, and a future SMP driver will rely on atomicity.

use core::ffi::c_void;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::types::{c_int, c_long};

const BITS_PER_LONG: c_long = 64;

/// Resolve `(nr, addr)` to the owning 64-bit word (as an
/// `AtomicU64`) and the in-word bit mask.
///
/// # Safety
/// `addr` must point to a readable+writable `unsigned long` bitmap
/// with at least `nr / 64 + 1` words; the word stays valid for the
/// duration of the atomic op.
unsafe fn word_and_mask(nr: c_long, addr: *mut c_void) -> (&'static AtomicU64, u64) {
    let word_index = nr / BITS_PER_LONG;
    let bit = nr % BITS_PER_LONG;
    // SAFETY: caller guarantees addr is a valid bitmap with the
    // word in bounds; AtomicU64 has the same layout as u64, and the
    // word outlives the borrow (it is C-owned storage).
    let word = unsafe { &*(addr as *const AtomicU64).add(word_index as usize) };
    (word, 1u64 << bit)
}

/// `test_and_set_bit` — atomically set bit `nr` in the bitmap at
/// `addr`, returning the previous value (0 or 1).
///
/// # Safety
/// `addr` must be a valid `unsigned long` bitmap with bit `nr` in
/// bounds.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn test_and_set_bit(nr: c_long, addr: *mut c_void) -> c_int {
    // SAFETY: forwarded caller contract.
    let (word, mask) = unsafe { word_and_mask(nr, addr) };
    let prev = word.fetch_or(mask, Ordering::SeqCst);
    c_int::from((prev & mask) != 0)
}

/// `test_and_clear_bit` — atomically clear bit `nr` in the bitmap at
/// `addr`, returning the previous value (0 or 1).
///
/// # Safety
/// `addr` must be a valid `unsigned long` bitmap with bit `nr` in
/// bounds.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn test_and_clear_bit(nr: c_long, addr: *mut c_void) -> c_int {
    // SAFETY: forwarded caller contract.
    let (word, mask) = unsafe { word_and_mask(nr, addr) };
    let prev = word.fetch_and(!mask, Ordering::SeqCst);
    c_int::from((prev & mask) != 0)
}
