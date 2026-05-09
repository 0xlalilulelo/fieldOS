// SPDX-License-Identifier: BSD-2-Clause
//
// Cooperative context switch (M0 step 3B-3). switch_to is the asm
// primitive: saves callee-save GP regs (rbx, rbp, r12-r15) onto the
// current stack, redirects RSP, restores the same six regs from the
// new stack, then `ret`s into whatever sits at the top of the new
// stack — either a return into a previously-suspended switch_to
// caller, or the entry function for fresh Tasks built by Task::new.
//
// The save-area shape is the load-bearing contract between this file
// and the synthetic frame Task::new lays down at task.rs:55-78. If
// the push order here disagrees with what Task::new wrote, the first
// switch into a fresh task lands with corrupted callee-save regs and
// the bug only surfaces when entry's compiled code uses the wrong
// reg — which can be far from the fault site.
//
// Push order (declared first → highest stack address last):
//
//   push rbx           ; ends at saved_rsp + 40
//   push rbp           ;          saved_rsp + 32
//   push r12           ;          saved_rsp + 24
//   push r13           ;          saved_rsp + 16
//   push r14           ;          saved_rsp + 8
//   push r15           ;          saved_rsp + 0   ← saved_rsp here
//
// Pop order on switch-in is the reverse: r15, r14, r13, r12, rbp,
// rbx, ret. Task::new's synthetic frame zero-fills all six reg slots
// and writes `entry` at saved_rsp + 48 (the slot ret reads), with
// 8 bytes of alignment padding above so saved_rsp is 16-aligned for
// fresh tasks. Suspended tasks (those switched out by this code,
// not by Task::new) have an 8-aligned saved_rsp because the call
// instruction that landed in switch_to pushed an 8-byte return
// address onto a 16-aligned stack; the asm doesn't distinguish
// because the layout above saved_rsp is identical in both cases.

use core::arch::global_asm;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::serial;
use crate::task;

unsafe extern "C" {
    /// Save callee-save GP regs to the current stack, store the
    /// resulting RSP into `*prev`, load `next` into RSP, restore
    /// callee-save GP regs from the new stack, and `ret`.
    ///
    /// # Safety
    /// `*prev` must be a writable u64. `next` must point at a stack
    /// region whose top 56 bytes hold a valid save frame (six
    /// callee-save reg slots + a return-address slot), built either
    /// by Task::new for fresh tasks or by a prior switch_to call
    /// for resumed tasks. Both stacks must be writable kernel
    /// memory mapped continuously above the switch sites; switching
    /// stacks invalidates any references rooted in the outgoing
    /// stack frame.
    pub fn switch_to(prev: *mut u64, next: u64);
}

global_asm!(
    r#"
    .global switch_to
    switch_to:
        push rbx
        push rbp
        push r12
        push r13
        push r14
        push r15
        mov [rdi], rsp
        mov rsp, rsi
        pop r15
        pop r14
        pop r13
        pop r12
        pop rbp
        pop rbx
        ret
    "#
);

// Switch-test storage. Two AtomicU64s hold saved RSPs across stack
// switches; the load/store calls themselves are non-atomic from the
// CPU's view (8-byte aligned writes are naturally atomic on x86_64),
// AtomicU64 is just the cleanest way to expose &mut storage from a
// static without UnsafeCell ceremony. Single-CPU cooperative; no
// races today.
static MAIN_RSP: AtomicU64 = AtomicU64::new(0);
static TEST_RSP: AtomicU64 = AtomicU64::new(0);

fn switch_test_entry() -> ! {
    serial::write_str("sched: switched INTO test task\n");
    // Switch back to main. After this call returns control to main
    // (right after switch_to in switch_test below), this function
    // never resumes. The `loop { halt }` after the unsafe block is
    // there only to satisfy the `-> !` signature; control never
    // reaches it because switch_to does not return on this path.
    // SAFETY: TEST_RSP is a writable u64; MAIN_RSP holds the value
    // saved by switch_test before the first switch into us; main's
    // stack frame at that saved RSP is intact (main hasn't returned
    // from switch_to yet, so its locals are still alive).
    unsafe {
        switch_to(TEST_RSP.as_ptr(), MAIN_RSP.load(Ordering::Relaxed));
    }
    crate::halt();
}

/// Self-test for switch_to. Builds a task, switches into it, that
/// task switches back, and main resumes. If all three serial lines
/// print and we return cleanly, the asm save area matches Task::new's
/// initial frame and the round-trip is sound. The HANDOFF flagged
/// this as 3B-3's "switch test before the ping-pong" — bisect-rich
/// against a wrong push order in switch_to.
pub fn switch_test() {
    let task = task::Task::new(switch_test_entry);
    TEST_RSP.store(task.saved_rsp, Ordering::Relaxed);

    serial::write_str("sched: switching to test task...\n");
    // SAFETY: MAIN_RSP is a writable u64 backing storage for our
    // saved RSP across the switch. task.saved_rsp points into a
    // freshly-built Task whose synthetic frame matches the layout
    // documented atop this file. The Task is owned through the
    // round-trip (`task` outlives this unsafe block), so its stack
    // memory remains valid throughout.
    unsafe {
        switch_to(MAIN_RSP.as_ptr(), task.saved_rsp);
    }
    serial::write_str("sched: returned to main\n");

    // task.saved_rsp now points into the test task's stack at the
    // suspension point inside switch_test_entry. Since that function
    // never resumes, dropping the task here returns its 16 KiB to
    // the heap. 3B-4's scheduler will own task lifetime properly.
    drop(task);
}
