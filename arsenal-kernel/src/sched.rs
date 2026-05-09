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

use alloc::boxed::Box;

use crate::cpu;
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

// ---------------------------------------------------------------
// 3B-4 — scheduler proper: spawn, yield_now, init.
// ---------------------------------------------------------------
//
// Cooperative single-CPU round-robin. spawn enqueues a Ready task;
// yield_now rotates the runqueue (current → back, front → current);
// init builds the idle task and switches into it from _start's
// Limine boot stack (which is then abandoned).
//
// Box ownership: a task's Box lives either in `cpu.runqueue` (Ready)
// or in `cpu.current` (Running, as a Box::into_raw'd raw pointer).
// yield_now swaps current via AtomicPtr::swap so there's no window
// where two Boxes alias the same Task. Single-CPU cooperative means
// no preemption races — once the migration to preemptive (3F) lands,
// the lock-then-swap dance grows accordingly.

/// Add a fresh Ready task to this CPU's runqueue.
#[allow(dead_code)] // wired by 3B-5's ping-pong demo
pub fn spawn(entry: fn() -> !) {
    let task = task::Task::new(entry);
    cpu::current_cpu().runqueue.lock().push_back(task);
}

/// Cooperatively yield to the next runnable task. If the runqueue
/// is empty, return immediately without switching — caller resumes.
pub fn yield_now() {
    let cpu = cpu::current_cpu();

    // Pop next runnable. Empty → no one to switch to; caller resumes.
    let next_box = match cpu.runqueue.lock().pop_front() {
        Some(t) => t,
        None => return,
    };

    let next_saved_rsp = next_box.saved_rsp;
    let next_ptr = Box::into_raw(next_box);

    // Atomically install next as current; receive prev. Single-CPU
    // cooperative makes the swap formally redundant, but the shape
    // is the one preemptive 3F needs: the swap closes the brief
    // window between "decided next" and "made next visible".
    let prev_ptr = cpu.current.swap(next_ptr, Ordering::Relaxed);
    assert!(
        !prev_ptr.is_null(),
        "yield_now: cpu.current was null — sched::init not run?"
    );

    // Pointer to prev's saved_rsp slot. Stable across the move into
    // the runqueue because Box's heap address doesn't change when
    // the Box itself migrates between containers. Captured here
    // before re-Boxing so we can keep using it after the move.
    // SAFETY: prev_ptr came from a prior Box::into_raw on a Task,
    // so the pointee is a live Task on the heap until we re-Box and
    // drop. The &raw mut projection is valid for the field offset.
    let prev_saved_rsp_ptr = unsafe { &raw mut (*prev_ptr).saved_rsp };

    // Re-enqueue prev at the back of the runqueue.
    // SAFETY: prev_ptr was the unique raw pointer to the prev Task
    // (the previous content of cpu.current); reclaiming it as a
    // Box is the matching free for the Box::into_raw that originally
    // installed it.
    let prev_box = unsafe { Box::from_raw(prev_ptr) };
    cpu.runqueue.lock().push_back(prev_box);

    // Switch. When prev later resumes (some future yield picks it
    // off the runqueue and swaps it back into current), control
    // returns to the instruction after this asm call; locals here
    // are no longer relevant — ownership transfers happened above.
    // SAFETY: prev_saved_rsp_ptr points into prev's Task on the
    // heap (lifetime tied to the Box now in the runqueue, which
    // outlives this switch). next_saved_rsp was the saved_rsp of
    // the task currently installed as cpu.current; its frame is
    // either Task::new's synthetic init frame (fresh) or a
    // previously-suspended switch_to frame.
    unsafe { switch_to(prev_saved_rsp_ptr, next_saved_rsp) };
}

/// Bring the scheduler online. Builds the idle task, installs it as
/// `current`, and switches into it from the caller's stack. Never
/// returns; the caller's stack is abandoned. Call from `_start`
/// after every other init has run.
pub fn init() -> ! {
    let cpu = cpu::current_cpu();

    let idle_task = task::Task::new(idle_loop);
    let idle_saved_rsp = idle_task.saved_rsp;
    let idle_ptr = Box::into_raw(idle_task);

    cpu.idle.store(idle_ptr, Ordering::Relaxed);
    cpu.current.store(idle_ptr, Ordering::Relaxed);

    serial::write_str("sched: init complete; switching to idle\n");

    // Throwaway storage for the outgoing RSP. The caller's stack
    // (the Limine boot stack) is abandoned after this switch — its
    // BOOTLOADER_RECLAIMABLE frames were already added to the frame
    // allocator in 3A-4, so anyone allocating from FRAMES could
    // overwrite the saved bytes, which is fine because we never
    // read them. The asm still performs the write before swapping
    // RSP.
    let mut throwaway_rsp: u64 = 0;
    // SAFETY: idle_saved_rsp points at Task::new's synthetic frame
    // for idle_task; throwaway_rsp is a writable u64 on this stack.
    // We never resume on this stack frame (the function is `-> !`),
    // and the asm's write to throwaway_rsp completes before RSP is
    // swapped to idle's stack.
    unsafe { switch_to(&raw mut throwaway_rsp, idle_saved_rsp) };

    unreachable!("sched::init: switch into idle returned");
}

/// Idle task entry. Yields forever. 3B-4 had a hlt at the bottom
/// of this loop which was correct only when the runqueue was empty;
/// once 3B-5 adds non-idle tasks, hlting on a cooperative-no-IRQ
/// CPU would halt the only core forever and starve the workers.
/// 3F's preemptive timer brings hlt back as a proper power-save.
fn idle_loop() -> ! {
    serial::write_str("sched: idle running\n");
    loop {
        yield_now();
    }
}

