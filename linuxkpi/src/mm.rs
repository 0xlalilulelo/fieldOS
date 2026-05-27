// SPDX-License-Identifier: BSD-2-Clause

//! Linux mm/swap surface — panic-on-call stubs for the memory-info
//! helpers balloon's stats path calls. Created during M1-2-5 Part B
//! sub-task 3's iteration arc when balloon.c's #include
//! <linux/swap.h> surfaced the need.
//!
//! Real implementations land in the M1-2-5-closing commit, sourced
//! from arsenal-kernel's frame allocator (free / total frame
//! counts). Arsenal has no swap and no kernel-style file-page
//! cache; the unsupported metrics return zero in the eventual
//! impl, not random kernel state.
//!
//! Also houses the <linux/oom.h> register/unregister entry points
//! (added when balloon.c's #include <linux/oom.h> surfaced): Arsenal
//! has no OOM-notifier subsystem at M1, so they are panic-on-call —
//! balloon only reaches them under VIRTIO_BALLOON_F_DEFLATE_ON_OOM
//! feature negotiation, which the M1 smoke device does not enable.

use core::ffi::c_void;

use crate::types::{c_int, c_long};

#[repr(C)]
pub struct sysinfo {
    pub uptime: c_long,
    pub loads: [u64; 3],
    pub totalram: u64,
    pub freeram: u64,
    pub sharedram: u64,
    pub bufferram: u64,
    pub totalswap: u64,
    pub freeswap: u64,
    pub procs: u16,
    pub pad: u16,
    pub totalhigh: u64,
    pub freehigh: u64,
    pub mem_unit: u32,
    pub _f: [u8; 0],
}

/// `si_meminfo` — fill `info` with the kernel's view of memory:
/// totalram + freeram (in frames, scaled by mem_unit). Real impl
/// reads arsenal-kernel's frame allocator state.
///
/// # Safety
/// Calling this during M1-2-5 Part B iteration arc panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn si_meminfo(_info: *mut sysinfo) {
    panic!("linuxkpi: si_meminfo not yet implemented (lands at M1-2-5 close)")
}

/// `si_mem_available` — return the number of frames that could be
/// allocated without page-reclaim. Real impl reads
/// arsenal-kernel's frame allocator free count.
///
/// # Safety
/// Calling this during M1-2-5 Part B iteration arc panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn si_mem_available() -> c_long {
    panic!("linuxkpi: si_mem_available not yet implemented (lands at M1-2-5 close)")
}

/// `register_oom_notifier` — add `nb` to the OOM notifier chain.
/// Arsenal has no OOM subsystem at M1; panic-on-call. `nb` is opaque
/// (`struct notifier_block *`) to this stub, which never derefs it.
///
/// # Safety
/// Calling this during M1 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn register_oom_notifier(_nb: *mut c_void) -> c_int {
    panic!("linuxkpi: register_oom_notifier not yet implemented (no OOM subsystem at M1)")
}

/// `unregister_oom_notifier` — remove `nb` from the OOM notifier
/// chain. Panic-on-call (pairs with register_oom_notifier).
///
/// # Safety
/// Calling this during M1 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unregister_oom_notifier(_nb: *mut c_void) -> c_int {
    panic!("linuxkpi: unregister_oom_notifier not yet implemented (no OOM subsystem at M1)")
}
