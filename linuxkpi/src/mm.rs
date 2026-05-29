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
//! Also houses the <linux/oom.h> and <linux/page_reporting.h>
//! register/unregister entry points (added as balloon.c's #includes
//! surfaced): Arsenal has neither an OOM-notifier nor a free-page-
//! reporting subsystem at M1, so they are panic-on-call — balloon
//! reaches them only under VIRTIO_BALLOON_F_DEFLATE_ON_OOM /
//! VIRTIO_BALLOON_F_REPORTING feature negotiation, which the M1
//! smoke device does not enable.

use core::ffi::c_void;

use crate::types::{c_char, c_int, c_long, c_uint};

unsafe extern "C" {
    fn linuxkpi_frames_free_count() -> u64;
    fn linuxkpi_frames_total_count() -> u64;
}

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

/// `si_meminfo` — fill `info` with the kernel's view of memory.
/// `totalram` is the total physical frames the frame allocator
/// tracks; `freeram` is the currently-free frame count. `mem_unit`
/// is the page size (4096) so balloon's `pages_to_bytes(i.freeram)`
/// computation yields the right byte total. Other fields are zero
/// at M1 — Arsenal has no swap, no page cache, no huge-page split.
///
/// # Safety
/// `info` must point to a writable `struct sysinfo` (or be NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn si_meminfo(info: *mut sysinfo) {
    if info.is_null() {
        return;
    }
    // SAFETY: caller's contract.
    unsafe {
        (*info).uptime = 0;
        (*info).loads = [0; 3];
        (*info).totalram = linuxkpi_frames_total_count();
        (*info).freeram = linuxkpi_frames_free_count();
        (*info).sharedram = 0;
        (*info).bufferram = 0;
        (*info).totalswap = 0;
        (*info).freeswap = 0;
        (*info).procs = 0;
        (*info).pad = 0;
        (*info).totalhigh = 0;
        (*info).freehigh = 0;
        (*info).mem_unit = 4096;
    }
}

/// `si_mem_available` — return the number of frames that could be
/// allocated without page-reclaim. Arsenal has no reclaim
/// subsystem at M1; the frame allocator's free count is the
/// honest answer.
///
/// # Safety
/// Takes no arguments and dereferences nothing; `unsafe` only to
/// match the `extern "C"` ABI the inherited drivers link against.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn si_mem_available() -> c_long {
    // SAFETY: bridge fn — returns a count.
    unsafe { linuxkpi_frames_free_count() as c_long }
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

/// `page_reporting_register` — register a free-page-reporting
/// callback. Arsenal has no free-page-reporting subsystem at M1;
/// panic-on-call. `prdev` is opaque (`struct page_reporting_dev_info
/// *`) to this stub.
///
/// # Safety
/// Calling this during M1 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn page_reporting_register(_prdev: *mut c_void) -> c_int {
    panic!("linuxkpi: page_reporting_register not yet implemented (no reporting subsystem at M1)")
}

/// `page_reporting_unregister` — remove a free-page-reporting
/// callback (pairs with page_reporting_register). Panic-on-call.
///
/// # Safety
/// Calling this during M1 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn page_reporting_unregister(_prdev: *mut c_void) {
    panic!("linuxkpi: page_reporting_unregister not yet implemented (no reporting subsystem at M1)")
}

/// `shrinker_alloc` — allocate a memory-reclaim shrinker. Arsenal
/// has no reclaim subsystem at M1; panic-on-call. balloon registers
/// a shrinker only under VIRTIO_BALLOON_F_FREE_PAGE_HINT.
///
/// # Safety
/// Calling this during M1 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shrinker_alloc(_flags: c_uint, _name: *const c_char) -> *mut c_void {
    panic!("linuxkpi: shrinker_alloc not yet implemented (no reclaim subsystem at M1)")
}

/// `shrinker_free` — release a shrinker (pairs with shrinker_alloc).
/// Panic-on-call.
///
/// # Safety
/// Calling this during M1 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shrinker_free(_shrinker: *mut c_void) {
    panic!("linuxkpi: shrinker_free not yet implemented (no reclaim subsystem at M1)")
}

/// `shrinker_register` — activate a shrinker on the reclaim path.
/// Panic-on-call.
///
/// # Safety
/// Calling this during M1 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shrinker_register(_shrinker: *mut c_void) {
    panic!("linuxkpi: shrinker_register not yet implemented (no reclaim subsystem at M1)")
}
