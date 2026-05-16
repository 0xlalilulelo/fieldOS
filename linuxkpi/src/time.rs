// SPDX-License-Identifier: BSD-2-Clause

//! Linux time primitives — `jiffies` (read tick counter),
//! `msleep` (millisecond delay), `udelay` (microsecond delay).
//! M1-2-5 Part A — kernel TICKS exposed via the bridge.
//!
//! Calibration: arsenal-kernel/src/apic.rs runs the LAPIC timer
//! at HZ=100 (10 ms per tick) per the M0 step 3F-2 PIT
//! calibration. `jiffies` returns the global TICKS counter
//! directly. `HZ` is hardcoded to 100; if a future inherited
//! driver wants different timer resolution, expose via the
//! bridge.
//!
//! Sleep semantics: at our M1 cooperative scheduler, `msleep`
//! and `udelay` busy-wait. The HANDOFF M1-2-5 (c) failure mode
//! ("GFP_ATOMIC honored too liberally") applies — calling
//! `msleep` from IRQ context or any GFP_ATOMIC path violates
//! Linux's "must not sleep" invariant; M2 with sleep-capable
//! mutex will add the IrqGuard-aware enforcement that catches
//! these at the call site. M1 ships busy-wait + the fail-loud
//! discipline (`udelay` for microsecond-scale assumes a future
//! TSC-based fine-grained timer; M1 approximates via spinning).

use crate::types::{c_uint, c_ulong};

unsafe extern "C" {
    fn linuxkpi_jiffies() -> u64;
}

/// Linux `HZ` — the number of timer ticks per second. arsenal-
/// kernel calibrates LAPIC at 100 Hz (10 ms tick); inherited
/// drivers reading `HZ` from shim_c.h see this value.
pub const HZ: c_ulong = 100;

/// Read the global timer-tick counter. Returns the current
/// kernel TICKS (from arsenal-kernel/src/apic.rs).
#[unsafe(no_mangle)]
pub extern "C" fn jiffies() -> c_ulong {
    // SAFETY: bridge fn — apic::ticks() reads an AtomicUsize.
    let t = unsafe { linuxkpi_jiffies() };
    t as c_ulong
}

/// Sleep at least `msecs` milliseconds. M1: busy-waits on
/// jiffies; M2 with sleep-capable mutex will yield to the
/// scheduler.
///
/// `msecs` is in milliseconds; HZ=100 means each jiffy is 10 ms,
/// so the wait is ceil(msecs / 10) jiffies.
///
/// # Safety
/// Calling from IRQ context violates Linux's "msleep must not
/// be called from atomic context" invariant. M1 doesn't enforce
/// this (the M1-2-5 (c) failure mode); M2's IrqGuard-aware
/// enforcement will catch the misuse at the call site.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn msleep(msecs: c_uint) {
    let needed_ticks = ((msecs as u64) * HZ).div_ceil(1000);
    // SAFETY: bridge fn — read-only.
    let start = unsafe { linuxkpi_jiffies() };
    while unsafe { linuxkpi_jiffies() }.wrapping_sub(start) < needed_ticks {
        core::hint::spin_loop();
    }
}

/// Sleep at least `usecs` microseconds. M1 approximation: the
/// LAPIC timer's 10 ms tick can't resolve sub-millisecond
/// delays, so udelay rounds up to one full tick. Future TSC-
/// based fine-grained timer would replace this.
///
/// # Safety
/// As `msleep`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn udelay(usecs: c_uint) {
    if usecs == 0 {
        return;
    }
    // Round up to one jiffy at minimum. Sub-tick delay arrives
    // when arsenal-kernel exposes a TSC-based timer.
    // SAFETY: bridge fn.
    let start = unsafe { linuxkpi_jiffies() };
    while unsafe { linuxkpi_jiffies() } == start {
        core::hint::spin_loop();
    }
}

/// Linux `ndelay` — nanosecond delay. M1 approximation: rounds
/// to udelay(1).
///
/// # Safety
/// As `msleep`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ndelay(_nsecs: c_uint) {
    // SAFETY: udelay's contract.
    unsafe { udelay(1) }
}
