// SPDX-License-Identifier: BSD-2-Clause

//! Linux workqueue surface — cooperative single-runner implementation
//! per [ADR-0011](../../docs/adrs/0011-deferred-work-cooperative-runner.md).
//!
//! One shared PENDING queue, drained by one cooperative task spawned
//! from arsenal-kernel's boot before any inherited driver init.
//! `INIT_WORK` records the func into the work_struct's opaque buffer;
//! `queue_work` atomically transitions IDLE → PENDING and pushes onto
//! PENDING; the runner calls `drain_one()` in a loop, popping +
//! invoking + transitioning back to IDLE between yields.
//!
//! M1 collapse: freezable / unbound / system / per-driver workqueues
//! all share the same singleton runner. `system_freezable_wq` is a
//! non-null sentinel; `alloc_workqueue` returns the same sentinel;
//! `destroy_workqueue` is a no-op. See ADR-0011 § "Decision" for the
//! rationale; ADR-0013 is the per-workqueue / freezable successor
//! trigger.

extern crate alloc;

use alloc::collections::VecDeque;
use core::ffi::c_void;
use core::sync::atomic::{AtomicU8, Ordering};

use crate::locks::Mutex;
use crate::types::{c_char, c_int, c_uint};

/// Mirror of <linux/workqueue.h>'s `struct work_struct` — a 64-byte
/// opaque buffer that the C side passes by pointer. We overlay
/// `WorkInner` on its first ~16 bytes (state + func); the remaining
/// 48 bytes are unused at M1 but preserved for layout stability.
#[repr(C)]
pub struct work_struct {
    _opaque: [u8; 64],
}

impl work_struct {
    /// Zero-initialized work_struct. Used by the shim self-test to
    /// stack-allocate a work_struct without needing the C-side
    /// initializer. Inherited drivers reach here via a global
    /// static `= { 0 }` initializer in C; both yield the same all-
    /// zeros state, which `linuxkpi_work_init` overwrites with the
    /// real func + IDLE state.
    pub const fn new() -> Self {
        Self { _opaque: [0; 64] }
    }
}

impl Default for work_struct {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal view of the first bytes of `work_struct._opaque`. The
/// state byte is at offset 0 (matches C-ABI alignment for a single
/// atomic byte) and the func pointer at offset 8 (natural 8-byte
/// alignment on x86_64). Callers ALWAYS cast `*mut work_struct` to
/// `*mut WorkInner` — never construct a WorkInner separately, since
/// the storage backs a C-side allocation.
#[repr(C)]
struct WorkInner {
    state: AtomicU8,
    _pad: [u8; 7],
    func: Option<unsafe extern "C" fn(*mut work_struct)>,
}

const STATE_IDLE: u8 = 0;
const STATE_PENDING: u8 = 1;
const STATE_RUNNING: u8 = 2;

/// Wrapper around `*mut work_struct` that's `Send` — `*mut T` isn't
/// `Send` by default, but the queue only holds these pointers while
/// the owning C-side state is statically alive (typically a static
/// in the inherited driver), and we never race on the pointed-to
/// memory without the work_struct's own state atomic.
#[repr(transparent)]
struct WorkPtr(*mut work_struct);
unsafe impl Send for WorkPtr {}

/// The shared queue of pending work pointers. Drained by the
/// cooperative runner task spawned from arsenal-kernel boot.
static PENDING: Mutex<VecDeque<WorkPtr>> = Mutex::new(VecDeque::new());

/// Cast `*mut work_struct` to its internal view. SAFETY: the caller
/// must hold a valid pointer; the cast itself is layout-checked at
/// compile time via the `#[repr(C)]` on both types.
#[inline]
unsafe fn inner(work: *mut work_struct) -> *mut WorkInner {
    work as *mut WorkInner
}

/// `INIT_WORK` dispatch target. Records `func` into `work` and resets
/// the state to IDLE. Idempotent across re-init (Linux semantics).
///
/// # Safety
/// `work` must point to a writable `work_struct` (or be NULL — NULL
/// is silently ignored). `func` is reinterpreted as a `work_func_t`;
/// the C side guarantees it has the correct signature.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_work_init(
    work: *mut work_struct,
    func: *const c_void,
) {
    if work.is_null() {
        return;
    }
    // SAFETY: caller's contract.
    unsafe {
        let i = inner(work);
        (*i).state = AtomicU8::new(STATE_IDLE);
        (*i).func = if func.is_null() {
            None
        } else {
            // SAFETY: the C side promises `func` has the work_func_t
            // signature `void (*)(struct work_struct *)`, which
            // matches our extern "C" Option<fn> shape.
            Some(core::mem::transmute::<
                *const c_void,
                unsafe extern "C" fn(*mut work_struct),
            >(func))
        };
        (*i)._pad = [0; 7];
    }
}

/// `queue_work` — push `work` onto the PENDING queue iff it's
/// currently IDLE. Returns `true` if newly queued, `false` if
/// already PENDING or RUNNING (Linux semantics).
///
/// The `_wq` argument is ignored at M1 — every queue is the same
/// shared queue per ADR-0011.
///
/// # Safety
/// `work` must point to a `work_struct` previously initialized via
/// `linuxkpi_work_init` (or be NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn queue_work(_wq: *mut c_void, work: *mut work_struct) -> bool {
    if work.is_null() {
        return false;
    }
    // SAFETY: caller's contract.
    let i = unsafe { inner(work) };
    let prev = unsafe {
        (*i).state.compare_exchange(
            STATE_IDLE,
            STATE_PENDING,
            Ordering::SeqCst,
            Ordering::SeqCst,
        )
    };
    if prev.is_ok() {
        PENDING.lock().push_back(WorkPtr(work));
        true
    } else {
        false
    }
}

/// `cancel_work` — remove `work` from the PENDING queue if present.
/// Returns `true` if the work was pending (and is now cancelled),
/// `false` otherwise (already running / already idle).
///
/// # Safety
/// `work` must point to a `work_struct` previously initialized.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cancel_work(work: *mut work_struct) -> bool {
    if work.is_null() {
        return false;
    }
    // SAFETY: caller's contract.
    let i = unsafe { inner(work) };
    let prev = unsafe {
        (*i).state.compare_exchange(
            STATE_PENDING,
            STATE_IDLE,
            Ordering::SeqCst,
            Ordering::SeqCst,
        )
    };
    if prev.is_ok() {
        PENDING.lock().retain(|w| !core::ptr::eq(w.0, work));
        true
    } else {
        false
    }
}

/// `cancel_work_sync` — cancel `work` and wait until any in-flight
/// execution finishes. Returns `true` if the work was pending or
/// running, `false` if already idle.
///
/// At M1 balloon's `cancel_work_sync` calls are all in the remove
/// path, which M1 never reaches (no inherited-driver exit). The
/// running-wait is a spin loop rather than a yield — the path is
/// theoretical at M1; the proper yield-via-bridge wires in if /
/// when an inherited driver actually exits.
///
/// # Safety
/// `work` must point to a `work_struct` previously initialized.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cancel_work_sync(work: *mut work_struct) -> bool {
    if work.is_null() {
        return false;
    }
    // SAFETY: caller's contract; cancel_work re-checks NULL.
    let was_pending = unsafe { cancel_work(work) };
    // SAFETY: same as above.
    let i = unsafe { inner(work) };
    // Spin while the runner is invoking this work's body. Theoretical
    // path at M1 (see fn doc); see ADR-0011 § Consequences.
    while unsafe { (*i).state.load(Ordering::SeqCst) } == STATE_RUNNING {
        core::hint::spin_loop();
    }
    // If was_pending we already returned-from-cancelled; else the work
    // was either RUNNING (now drained) or IDLE; the "running" case is
    // still a true "we waited" — return true in either of those.
    was_pending || unsafe { (*i).state.load(Ordering::SeqCst) != STATE_IDLE }
}

/// Sentinel storage backing every workqueue handle the shim hands
/// out. The address-of (`&SENTINEL_WQ_STORAGE`) is what the C side
/// sees when it reads `system_freezable_wq` or stores the
/// `alloc_workqueue` return value. Per ADR-0011 every queue is the
/// same queue at M1; the pointer's only job is to be non-null.
static SENTINEL_WQ_STORAGE: u8 = 0;

/// `alloc_workqueue` — allocate a workqueue. At M1 returns the
/// shared sentinel (one runner drains all work). `fmt`, `flags`, and
/// `max_active` are accepted for ABI compatibility but ignored;
/// ADR-0013 is the successor that honors per-queue semantics.
///
/// # Safety
/// Pointer args are not dereferenced. Always returns non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn alloc_workqueue(
    _fmt: *const c_char,
    _flags: c_uint,
    _max_active: c_int,
) -> *mut c_void {
    &SENTINEL_WQ_STORAGE as *const u8 as *mut c_void
}

/// `destroy_workqueue` — release a workqueue handle. No-op at M1
/// (single shared runner, no per-wq state).
///
/// # Safety
/// `_wq` is not dereferenced.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn destroy_workqueue(_wq: *mut c_void) {
    // No-op at M1; see fn doc + ADR-0011 § Consequences.
}

/// `*const c_void` wrapper that's `Sync` — needed so the
/// `system_freezable_wq` static (a pointer) can be shared across the
/// kernel without a `static mut` UB hazard.
#[repr(transparent)]
pub struct WqPtr(*const c_void);
unsafe impl Sync for WqPtr {}

/// `system_freezable_wq` — the shared freezable workqueue balloon
/// queues stats / size work onto. Non-null sentinel at M1; the C
/// side reads the variable as a `struct workqueue_struct *`, which
/// is the pointer queue_work receives as its first argument. Per
/// ADR-0011 the value is ignored on the receive side; it just needs
/// to be non-null so balloon's null-checks pass.
#[unsafe(no_mangle)]
pub static system_freezable_wq: WqPtr =
    WqPtr(&SENTINEL_WQ_STORAGE as *const u8 as *const c_void);

/// Runner-side primitive — pop one pending work, invoke its body,
/// transition back to IDLE. Returns `true` if a work body ran,
/// `false` if the queue was empty.
///
/// arsenal-kernel spawns a cooperative task that calls this in a
/// loop, yielding when it returns false. Idempotent across empty
/// queues; safe to call before any inherited driver registers work.
pub fn drain_one() -> bool {
    let work = match PENDING.lock().pop_front() {
        Some(w) => w.0,
        None => return false,
    };
    // SAFETY: `work` was pushed via queue_work, which only enqueues
    // pointers that came from C-side static storage (typically
    // embedded in a driver's per-device struct). Lifetimes are the
    // driver's responsibility (Linux convention); the work_struct
    // outlives every queue_work / runner cycle by construction.
    unsafe {
        let i = inner(work);
        (*i).state.store(STATE_RUNNING, Ordering::SeqCst);
        if let Some(func) = (*i).func {
            func(work);
        }
        (*i).state.store(STATE_IDLE, Ordering::SeqCst);
    }
    true
}
