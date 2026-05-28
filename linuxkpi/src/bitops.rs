// SPDX-License-Identifier: BSD-2-Clause

//! Linux atomic bit-operations surface (<linux/bitops.h>). Created
//! during M1-2-5 Part B sub-task 3's body-error phase when balloon's
//! config-read path reached test_and_set_bit / test_and_clear_bit.
//!
//! M1-2-5 Part B: panic-on-call stubs. The real atomic
//! test-and-set / test-and-clear (a locked bts/btr on the
//! `unsigned long` bitmap word, returning the previous bit) lands at
//! the M1-2-5-closing commit when balloon's stats/config-read path
//! actually runs — balloon uses them on VIRTIO_BALLOON_CONFIG_READ_*
//! command bits in start/end_update.

use core::ffi::c_void;

use crate::types::{c_int, c_long};

/// `test_and_set_bit` — atomically set bit `nr` in the bitmap at
/// `addr`, returning the previous value. M1-2-5 Part B: panic-on-call.
///
/// # Safety
/// Calling this during the M1-2-5 Part B iteration arc panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn test_and_set_bit(_nr: c_long, _addr: *mut c_void) -> c_int {
    panic!("linuxkpi: test_and_set_bit not yet implemented (lands at M1-2-5 close)")
}

/// `test_and_clear_bit` — atomically clear bit `nr` in the bitmap at
/// `addr`, returning the previous value. M1-2-5 Part B: panic-on-call.
///
/// # Safety
/// Calling this during the M1-2-5 Part B iteration arc panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn test_and_clear_bit(_nr: c_long, _addr: *mut c_void) -> c_int {
    panic!("linuxkpi: test_and_clear_bit not yet implemented (lands at M1-2-5 close)")
}
