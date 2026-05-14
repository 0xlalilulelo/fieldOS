// SPDX-License-Identifier: BSD-2-Clause
//
// IRQ-disable critical sections — M0 step 4-4. Cooperative paths
// (sched::yield_now, sched::spawn) that touch the runqueue under
// preemptive scheduling must hold IF=0 across the access, or the
// periodic timer handler can fire mid-lock and re-enter the
// runqueue mutex on the same core (deadlock).
//
// IrqGuard captures rflags on construction and clears IF (cli);
// Drop restores the saved rflags via popfq, which sets IF back to
// whatever it was on entry. Composes cleanly with spin::MutexGuard:
// the IrqGuard outlives the MutexGuard, so the unlock happens with
// IRQs still disabled, then IrqGuard's drop restores.

use core::arch::asm;

/// Held across an IRQ-sensitive critical section. On Drop, restores
/// the rflags state captured at save_and_disable — typically setting
/// IF back to 1 in the caller's frame.
pub struct IrqGuard {
    rflags: u64,
}

impl IrqGuard {
    /// Save rflags into the guard and clear IF (cli). Drop restores
    /// the captured rflags. The atomic pushfq + cli sequence ensures
    /// no IRQ can deliver between observing the prior IF state and
    /// disabling.
    pub fn save_and_disable() -> Self {
        let rflags: u64;
        // SAFETY: pushfq + pop reg + cli is the standard atomic
        // save-and-disable-interrupts sequence (Intel SDM Vol. 2A).
        // pushfq does not modify flags; the pop adjusts rsp but the
        // matching pushfq paired it; cli clears IF. Net rsp change
        // is zero. We do touch the stack briefly so `nostack` would
        // be incorrect; `preserves_flags` would also be incorrect
        // (cli changes IF).
        unsafe {
            asm!(
                "pushfq",
                "pop {0}",
                "cli",
                out(reg) rflags,
            );
        }
        Self { rflags }
    }
}

impl Drop for IrqGuard {
    fn drop(&mut self) {
        // SAFETY: push reg + popfq writes self.rflags back into the
        // rflags register, restoring IF (and every other writable
        // flag) to the value captured at save_and_disable. Net rsp
        // change is zero.
        unsafe {
            asm!(
                "push {0}",
                "popfq",
                in(reg) self.rflags,
            );
        }
    }
}
