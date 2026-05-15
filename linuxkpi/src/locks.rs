// SPDX-License-Identifier: BSD-2-Clause

//! Linux locking primitives — `atomic_t` + `mutex` + `spinlock`
//! shimmed over `core::sync::atomic` and `spin::Mutex`.
//!
//! Semantic note: Linux distinguishes `mutex` (sleep-capable, may
//! deschedule the calling task) from `spinlock` (must not sleep,
//! callable from IRQ context). At M1's cooperative-only scheduler,
//! both reduce to spinning — the distinction is informational. The
//! semantic split reappears at M2 when sleep-capable mutex
//! arrives; until then, calling `mutex_lock` from an IRQ context
//! is fine because no path actually sleeps.
//!
//! ABI: `atomic_t`'s C-side definition (`struct { int counter; }`)
//! is matched here byte-for-byte so inherited C reading the
//! `.counter` field directly via Linux's atomic-op macros sees the
//! same memory the Rust shim atomic ops manipulate. `mutex` and
//! `spinlock` wrap `spin::Mutex<()>` — opaque to inherited C, with
//! C-side declarations exposing only the size + alignment so
//! drivers can declare instances.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicI32, Ordering};
use spin::Mutex as SpinMutex;

// =====================================================================
// atomic_t — Linux <linux/atomic.h>.
// =====================================================================

/// C-ABI-compatible atomic integer. Layout matches Linux's
/// `typedef struct { int counter; } atomic_t` — `UnsafeCell<i32>`
/// is `#[repr(transparent)]` over `i32`, so C sees the same `int`
/// either way. The `UnsafeCell` is what tells Rust this type is
/// interior-mutable: without it, `static` instances of `atomic_t`
/// land in `.rodata` and atomic writes through `&atomic_t` page-
/// fault on the protection violation.
#[repr(C)]
pub struct atomic_t {
    pub counter: UnsafeCell<i32>,
}

impl atomic_t {
    pub const fn new(v: i32) -> Self {
        Self { counter: UnsafeCell::new(v) }
    }
}

// SAFETY: counter is only ever accessed via SeqCst atomic ops on
// the AtomicI32 view; concurrent access from multiple cores is
// sound under that discipline.
unsafe impl Sync for atomic_t {}

/// Atomically increment `*v` by 1.
///
/// # Safety
/// `v` must point to a valid `atomic_t` for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn atomic_inc(v: *mut atomic_t) {
    // SAFETY: caller's contract — v is valid. UnsafeCell<i32> is
    // repr(transparent) over i32, which AtomicI32 also matches per
    // Rust's atomic-type guarantees; UnsafeCell::get returns the
    // *mut i32 we pass to AtomicI32::from_ptr.
    unsafe {
        let cell_ptr = core::ptr::addr_of_mut!((*v).counter);
        let int_ptr = (*cell_ptr).get();
        AtomicI32::from_ptr(int_ptr).fetch_add(1, Ordering::SeqCst);
    }
}

/// Atomically decrement `*v` by 1.
///
/// # Safety
/// `v` must point to a valid `atomic_t` for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn atomic_dec(v: *mut atomic_t) {
    // SAFETY: see atomic_inc.
    unsafe {
        let cell_ptr = core::ptr::addr_of_mut!((*v).counter);
        let int_ptr = (*cell_ptr).get();
        AtomicI32::from_ptr(int_ptr).fetch_sub(1, Ordering::SeqCst);
    }
}

/// Atomically read `*v`.
///
/// # Safety
/// `v` must point to a valid `atomic_t` for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn atomic_read(v: *const atomic_t) -> i32 {
    // SAFETY: see atomic_inc; UnsafeCell::get returns *mut i32
    // even from a *const reference (interior mutability), and
    // AtomicI32::load is read-only.
    unsafe {
        let cell_ptr = core::ptr::addr_of!((*v).counter);
        let int_ptr = (*cell_ptr).get();
        AtomicI32::from_ptr(int_ptr).load(Ordering::SeqCst)
    }
}

/// Atomically store `i` into `*v`.
///
/// # Safety
/// `v` must point to a valid `atomic_t` for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn atomic_set(v: *mut atomic_t, i: i32) {
    // SAFETY: see atomic_inc.
    unsafe {
        let cell_ptr = core::ptr::addr_of_mut!((*v).counter);
        let int_ptr = (*cell_ptr).get();
        AtomicI32::from_ptr(int_ptr).store(i, Ordering::SeqCst);
    }
}

/// Rust-friendly atomic wrapper. Used by the shim self-test and
/// by future Rust-side shim code that wants a typed atomic.
pub struct AtomicInt {
    inner: atomic_t,
}

impl AtomicInt {
    pub const fn new(v: i32) -> Self {
        Self { inner: atomic_t::new(v) }
    }
    pub fn inc(&self) {
        // SAFETY: &self ensures inner is a valid atomic_t. Atomic
        // ops on an aliased &atomic_t are sound.
        unsafe { atomic_inc(&self.inner as *const _ as *mut _) }
    }
    pub fn dec(&self) {
        // SAFETY: see inc.
        unsafe { atomic_dec(&self.inner as *const _ as *mut _) }
    }
    pub fn read(&self) -> i32 {
        // SAFETY: see inc.
        unsafe { atomic_read(&self.inner) }
    }
    pub fn set(&self, v: i32) {
        // SAFETY: see inc.
        unsafe { atomic_set(&self.inner as *const _ as *mut _, v) }
    }
}

// SAFETY: all interior mutability is through SeqCst atomic ops on
// the `counter` field; no other access path exists.
unsafe impl Sync for AtomicInt {}

// =====================================================================
// mutex — Linux <linux/mutex.h>.
//
// Linux's struct mutex is sleep-capable. At M1 cooperative-only we
// reduce to spinning; the API surface stays identical.
// =====================================================================

/// C-ABI-compatible mutex. Inherited C declares instances by name
/// + passes pointers to `mutex_init` / `mutex_lock` / `mutex_unlock`.
#[repr(C)]
pub struct mutex {
    inner: SpinMutex<()>,
}

impl mutex {
    pub const fn new() -> Self {
        Self { inner: SpinMutex::new(()) }
    }
}

impl Default for mutex {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize `*m`. Must be called once before any `mutex_lock` /
/// `mutex_unlock`.
///
/// # Safety
/// `m` must point to writable memory of size + alignment matching
/// `mutex` for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mutex_init(m: *mut mutex) {
    // SAFETY: caller's contract — m is writable + correctly sized.
    unsafe { core::ptr::write(m, mutex::new()) }
}

/// Acquire `*m`, blocking (spinning at M1) until acquired.
///
/// # Safety
/// `m` must point to a `mutex` previously initialized via
/// `mutex_init` (or a `const` `mutex::new()` initializer in Rust).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mutex_lock(m: *mut mutex) {
    // SAFETY: caller's contract — m is initialized + valid. The
    // returned guard is forgotten so mutex_unlock can release the
    // lock manually; this matches Linux's lock-by-handle model.
    unsafe {
        let guard = (*m).inner.lock();
        core::mem::forget(guard);
    }
}

/// Release `*m`. Pairs with a prior `mutex_lock` on the same `m`.
///
/// # Safety
/// `m` must point to a `mutex` currently held by the calling
/// context. Releasing an unlocked mutex is undefined behavior at
/// M1; M2's mutex implementation may add detection.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mutex_unlock(m: *mut mutex) {
    // SAFETY: caller's contract — m is currently locked. spin's
    // force_unlock matches the lock-without-guard pattern that
    // mutex_lock established.
    unsafe { (*m).inner.force_unlock() }
}

/// Rust-friendly typed mutex. Used by the shim self-test.
pub struct Mutex<T> {
    inner: SpinMutex<T>,
}

impl<T> Mutex<T> {
    pub const fn new(val: T) -> Self {
        Self { inner: SpinMutex::new(val) }
    }
    pub fn lock(&self) -> spin::MutexGuard<'_, T> {
        self.inner.lock()
    }
}

// =====================================================================
// spinlock — Linux <linux/spinlock.h>.
//
// At M1 cooperative-only, identical implementation to mutex; the
// IRQ-disabled spin_lock_irqsave variant lands at M1-2-2 when the
// IRQ bridge needs it.
// =====================================================================

/// C-ABI-compatible spinlock.
#[repr(C)]
pub struct spinlock {
    inner: SpinMutex<()>,
}

impl spinlock {
    pub const fn new() -> Self {
        Self { inner: SpinMutex::new(()) }
    }
}

impl Default for spinlock {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize `*s`. Must be called once before any `spin_lock` /
/// `spin_unlock`.
///
/// # Safety
/// `s` must point to writable memory of size + alignment matching
/// `spinlock` for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spin_lock_init(s: *mut spinlock) {
    // SAFETY: caller's contract — s is writable + correctly sized.
    unsafe { core::ptr::write(s, spinlock::new()) }
}

/// Acquire `*s`, spinning until acquired.
///
/// # Safety
/// `s` must point to a `spinlock` previously initialized via
/// `spin_lock_init`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spin_lock(s: *mut spinlock) {
    // SAFETY: see mutex_lock.
    unsafe {
        let guard = (*s).inner.lock();
        core::mem::forget(guard);
    }
}

/// Release `*s`. Pairs with a prior `spin_lock` on the same `s`.
///
/// # Safety
/// `s` must point to a `spinlock` currently held by the calling
/// context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spin_unlock(s: *mut spinlock) {
    // SAFETY: see mutex_unlock.
    unsafe { (*s).inner.force_unlock() }
}
