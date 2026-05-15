// SPDX-License-Identifier: BSD-2-Clause

//! Linux IRQ bridge — `request_irq` / `free_irq` for inherited
//! drivers + a 16-slot dispatcher pool that wires Linux-shaped
//! `(irq_handler_t, dev_id)` callbacks to arsenal-kernel's IDT.
//!
//! Why a dispatcher pool: the IDT requires
//! `extern "x86-interrupt" fn(InterruptStackFrame)` handler
//! signatures, which cannot capture per-IRQ state. Linux's
//! `irq_handler_t` signature is `int (*)(int irq, void *dev_id)`
//! — different ABI, plus per-IRQ state. The bridge is N
//! pre-generated `extern "x86-interrupt"` dispatchers (one per
//! slot) that each consult a static slot table populated by
//! `request_irq`. The dispatcher calls the registered Linux
//! handler then sends LAPIC EOI.
//!
//! Init flow:
//!   1. arsenal-kernel/src/main.rs calls
//!      `linuxkpi::irq::register_dispatchers(idt::register_vector)`
//!      early in boot.
//!   2. Each of the 16 dispatchers gets registered with `idt`,
//!      receiving a unique IDT vector. The shim records the
//!      (slot → IDT vector) mapping in `SLOT_TO_IDT_VEC`.
//!   3. `pci_alloc_irq_vectors` (in `pci.rs`) allocates a
//!      contiguous range of slots for a device, programs the
//!      MSI-X table to deliver to the corresponding IDT vectors.
//!   4. Driver calls `request_irq(irq, handler, ...)` with `irq`
//!      = the slot index returned by `pci_irq_vector`. The slot
//!      is populated; the next IRQ delivery calls the handler.
//!
//! Slot count: 16 is generous for M1's inherited driver fleet
//! (virtio-balloon at 2-5 needs 1; xHCI at step 3 might want
//! 4-8; amdgpu at step 5 wants 8-16). When real-hardware demand
//! exceeds 16, grow the pool — the macro expansion is mechanical.

extern crate alloc;

use core::sync::atomic::{AtomicUsize, Ordering};
use spin::{Mutex, Once};
use x86_64::structures::idt::InterruptStackFrame;

use crate::types::{c_char, c_int, c_uint, c_ulong, c_void};

unsafe extern "C" {
    fn linuxkpi_lapic_eoi();
}

/// Linux's `irq_handler_t`. Returns one of `IRQ_NONE` (0),
/// `IRQ_HANDLED` (1), `IRQ_WAKE_THREAD` (2). M1's shim ignores
/// the return value (no threaded IRQs yet).
#[allow(non_camel_case_types)]
pub type irq_handler_t = unsafe extern "C" fn(c_int, *mut c_void) -> c_int;

/// `IRQ_HANDLED` — the only return value M1 cares about for
/// documentation purposes; the dispatcher discards it.
pub const IRQ_HANDLED: c_int = 1;

/// Per-slot Linux-handler registration. Populated by `request_irq`,
/// drained by `free_irq` / `pci_free_irq_vectors`.
#[derive(Clone, Copy)]
struct SlotEntry {
    handler: irq_handler_t,
    dev_id: *mut c_void,
    irq: c_int,
}

// SAFETY: SlotEntry is shared across cores via SLOTS's mutex; the
// fn pointer + opaque dev_id pointer are passive payload.
unsafe impl Send for SlotEntry {}

pub(crate) const SLOT_COUNT: usize = 16;

static SLOTS: Mutex<[Option<SlotEntry>; SLOT_COUNT]> = Mutex::new([None; SLOT_COUNT]);

/// Slot → IDT-vector mapping. Populated once at boot via
/// `register_dispatchers`; immutable thereafter. `Once` for
/// fail-loud-on-uninitialized-use.
static SLOT_TO_IDT_VEC: Once<[u8; SLOT_COUNT]> = Once::new();

/// Watermark for `pci_alloc_irq_vectors`'s contiguous-range
/// allocator. Slots are not reclaimed at M1 (a driver `pci_
/// free_irq_vectors` clears `SLOTS` entries but doesn't bump this
/// counter back); reclaim arrives if real-hardware driver churn
/// exhausts the 16-slot pool, which won't happen at M1 scale.
static NEXT_SLOT: AtomicUsize = AtomicUsize::new(0);

// =====================================================================
// Pre-generated dispatchers — one per slot. Each is a unique
// `extern "x86-interrupt" fn` so it has a distinct fn pointer that
// `idt::register_vector` can install at a unique IDT vector. The
// macro avoids 16 copies of identical-shape boilerplate.
// =====================================================================

macro_rules! gen_dispatcher {
    ($name:ident, $slot:expr) => {
        pub extern "x86-interrupt" fn $name(_: InterruptStackFrame) {
            dispatch_common($slot);
        }
    };
}

gen_dispatcher!(dispatch_0, 0);
gen_dispatcher!(dispatch_1, 1);
gen_dispatcher!(dispatch_2, 2);
gen_dispatcher!(dispatch_3, 3);
gen_dispatcher!(dispatch_4, 4);
gen_dispatcher!(dispatch_5, 5);
gen_dispatcher!(dispatch_6, 6);
gen_dispatcher!(dispatch_7, 7);
gen_dispatcher!(dispatch_8, 8);
gen_dispatcher!(dispatch_9, 9);
gen_dispatcher!(dispatch_10, 10);
gen_dispatcher!(dispatch_11, 11);
gen_dispatcher!(dispatch_12, 12);
gen_dispatcher!(dispatch_13, 13);
gen_dispatcher!(dispatch_14, 14);
gen_dispatcher!(dispatch_15, 15);

const DISPATCHERS: [extern "x86-interrupt" fn(InterruptStackFrame); SLOT_COUNT] = [
    dispatch_0, dispatch_1, dispatch_2, dispatch_3,
    dispatch_4, dispatch_5, dispatch_6, dispatch_7,
    dispatch_8, dispatch_9, dispatch_10, dispatch_11,
    dispatch_12, dispatch_13, dispatch_14, dispatch_15,
];

/// Common dispatch path. Reads the slot's registered handler (if
/// any), invokes it with the (irq, dev_id) pair, then sends LAPIC
/// EOI. Called from each per-slot dispatcher.
fn dispatch_common(slot: usize) {
    let entry = SLOTS.lock()[slot];
    if let Some(e) = entry {
        // SAFETY: handler is a Linux extern "C" fn the driver
        // registered via request_irq; (irq, dev_id) match what
        // the driver expects. Discarding the return value is
        // intentional at M1 (no threaded IRQ support).
        unsafe {
            let _ = (e.handler)(e.irq, e.dev_id);
        }
    }
    // SAFETY: bridge fn — apic::send_eoi is the kernel's LAPIC
    // EOI write. Called once per IRQ regardless of whether a
    // handler was registered (spurious is the only correct
    // response to a fired-but-unregistered slot).
    unsafe { linuxkpi_lapic_eoi() }
}

// =====================================================================
// Init — called once from arsenal-kernel during boot.
// =====================================================================

/// Register all 16 dispatchers with the kernel's IDT. Must be
/// called once before any `pci_alloc_irq_vectors`. Idempotent
/// thanks to `Once`; second + later calls return immediately.
///
/// `register` is the kernel's `idt::register_vector` (or any
/// equivalent allocator that accepts an `extern "x86-interrupt"
/// fn` handler and returns the assigned IDT vector). Passing it
/// in avoids the cyclic-crate-dep that would otherwise be needed
/// for linuxkpi to call into arsenal-kernel directly.
pub fn register_dispatchers<F>(mut register: F)
where
    F: FnMut(extern "x86-interrupt" fn(InterruptStackFrame)) -> u8,
{
    SLOT_TO_IDT_VEC.call_once(|| {
        let mut vecs = [0u8; SLOT_COUNT];
        for (i, &dispatcher) in DISPATCHERS.iter().enumerate() {
            vecs[i] = register(dispatcher);
        }
        vecs
    });
}

/// Look up the IDT vector assigned to `slot`. Used by
/// `pci_alloc_irq_vectors` when programming the MSI-X table. Panics
/// if `register_dispatchers` hasn't been called — that's a boot-
/// order bug, not a graceful-degradation case.
pub(crate) fn slot_idt_vector(slot: usize) -> u8 {
    SLOT_TO_IDT_VEC
        .get()
        .expect("linuxkpi::irq::register_dispatchers must be called before any IRQ allocation")[slot]
}

/// Allocate `count` contiguous slots from the pool. Returns the
/// first slot index, or `None` if the pool is exhausted (slot +
/// count > SLOT_COUNT). Slots are not reclaimed at M1.
pub(crate) fn alloc_slots(count: usize) -> Option<usize> {
    if count == 0 || count > SLOT_COUNT {
        return None;
    }
    let first = NEXT_SLOT.fetch_add(count, Ordering::SeqCst);
    if first + count > SLOT_COUNT {
        // Roll back the bump so subsequent callers see the right
        // watermark. Concurrent callers may interleave; the
        // resulting underflow is bounded by SLOT_COUNT and harms
        // nothing (subsequent allocations also fail).
        NEXT_SLOT.fetch_sub(count, Ordering::SeqCst);
        return None;
    }
    Some(first)
}

// =====================================================================
// request_irq / free_irq — the Linux-facing surface.
// =====================================================================

/// Install `handler` for IRQ `irq`. Inherited drivers call this
/// after `pci_alloc_irq_vectors` + `pci_irq_vector` have given
/// them an irq number (which in our shim is the slot index).
/// Returns 0 on success, negative on failure.
///
/// # Safety
/// `handler` must be a valid `extern "C" fn(c_int, *mut c_void)
/// -> c_int`; `dev_id` is opaque and stored verbatim. `name`
/// is informational and not currently captured (Linux uses it
/// for /proc/interrupts; we have no such surface at M1).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn request_irq(
    irq: c_uint,
    handler: irq_handler_t,
    _flags: c_ulong,
    _name: *const c_char,
    dev_id: *mut c_void,
) -> c_int {
    let slot = irq as usize;
    if slot >= SLOT_COUNT {
        return -1;
    }
    SLOTS.lock()[slot] = Some(SlotEntry {
        handler,
        dev_id,
        irq: irq as c_int,
    });
    0
}

/// Release IRQ `irq`. Returns the `dev_id` that was registered (Linux
/// convention; balloon and most drivers don't consume the return).
///
/// # Safety
/// `irq` must have been previously registered via `request_irq`;
/// `_dev_id` is informational (Linux uses it for shared-IRQ
/// disambiguation, which M1 doesn't support).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free_irq(irq: c_uint, _dev_id: *mut c_void) -> *mut c_void {
    let slot = irq as usize;
    if slot >= SLOT_COUNT {
        return core::ptr::null_mut();
    }
    let prior = SLOTS.lock()[slot].take();
    prior.map_or(core::ptr::null_mut(), |e| e.dev_id)
}
