// SPDX-License-Identifier: BSD-2-Clause

//! Linux workqueue surface — panic-on-call stubs per ADR-0005 § 6
//! ("synchronous module init at M1; deferred-path stubs
//! (schedule_work, queue_work, kthread_run) panic-on-call").
//! Real implementations come whenever the workqueue subsystem
//! earns its own design decision — M1 step 5 (amdgpu) or step 6
//! (iwlwifi), whichever needs them first.
//!
//! INIT_WORK in inherited C is a macro dispatching to
//! `linuxkpi_work_init` which records the work_func_t inside the
//! opaque buffer of struct work_struct for the future real impl.
//! At M1 the recording itself is a panic — any code path reaching
//! INIT_WORK ends loudly, which surfaces during balloon probe
//! exactly when an unexpected workqueue dependency appears.

use core::ffi::c_void;

use crate::types::{c_char, c_int, c_uint};

/// Mirror of <linux/workqueue.h>'s `struct work_struct`.
/// 64-byte opaque buffer — generous headroom for the eventual
/// real impl to extend without breaking the C ABI.
#[repr(C)]
pub struct work_struct {
    _opaque: [u8; 64],
}

/// `INIT_WORK` dispatch target. Real impl records `func` and the
/// owning workqueue inside the opaque buffer; M1 panics so the
/// surface is link-clean but functionally vocal.
///
/// # Safety
/// Calling this during M1 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_work_init(
    _work: *mut work_struct,
    _func: *const c_void,
) {
    panic!("linuxkpi: INIT_WORK not yet implemented (ADR-0005 § 6 defers to M1 step 5/6)")
}

/// `alloc_workqueue` — create a named workqueue. M1 panic-on-call.
///
/// # Safety
/// Calling this during M1 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn alloc_workqueue(
    _fmt: *const c_char,
    _flags: c_uint,
    _max_active: c_int,
) -> *mut c_void {
    panic!("linuxkpi: alloc_workqueue not yet implemented (ADR-0005 § 6)")
}

/// `destroy_workqueue` — release a workqueue. M1 panic-on-call.
///
/// # Safety
/// Calling this during M1 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn destroy_workqueue(_wq: *mut c_void) {
    panic!("linuxkpi: destroy_workqueue not yet implemented (ADR-0005 § 6)")
}

/// `queue_work` — enqueue `work` on `wq`. Returns true if the
/// item was successfully enqueued (false if already pending).
/// M1 panic-on-call.
///
/// # Safety
/// Calling this during M1 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn queue_work(_wq: *mut c_void, _work: *mut work_struct) -> bool {
    panic!("linuxkpi: queue_work not yet implemented (ADR-0005 § 6)")
}

/// `cancel_work` — remove `work` from any queue it's on. Returns
/// true if the work was pending (and is now cancelled). M1
/// panic-on-call.
///
/// # Safety
/// Calling this during M1 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cancel_work(_work: *mut work_struct) -> bool {
    panic!("linuxkpi: cancel_work not yet implemented (ADR-0005 § 6)")
}

/// `cancel_work_sync` — cancel `work` and wait for any in-flight
/// execution to finish. M1 panic-on-call (no deferred-work path).
///
/// # Safety
/// Calling this during M1 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cancel_work_sync(_work: *mut work_struct) -> bool {
    panic!("linuxkpi: cancel_work_sync not yet implemented (ADR-0005 § 6)")
}

/// Pointer-sized wrapper so a `*const` workqueue handle can be a
/// `static`. The pointer is null and never dereferenced at M1
/// (queue_work panics first); it exists only so inherited C links
/// against the `system_freezable_wq` symbol.
#[repr(transparent)]
pub struct WqPtr(*const c_void);

// SAFETY: the contained pointer is null and immutable; nothing reads
// through it at M1. Sharing a null sentinel across threads is sound.
unsafe impl Sync for WqPtr {}

/// `system_freezable_wq` — the shared freezable workqueue balloon
/// queues its stats / size work onto. NULL at M1; the real shared
/// workqueues land with the deferred-work subsystem (ADR-0010).
#[unsafe(no_mangle)]
pub static system_freezable_wq: WqPtr = WqPtr(core::ptr::null());
