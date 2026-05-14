// SPDX-License-Identifier: BSD-2-Clause
//
// Context switch — M0 step 3B-3 (cooperative) augmented at 4-4 for
// hard preemption. switch_to is the asm primitive: saves callee-save
// GP regs + rflags onto the current stack, redirects RSP, restores
// the same state from the new stack, then `ret`s into whatever sits
// at the top of the new stack — either a return into a previously-
// suspended switch_to caller, or the entry function for fresh Tasks
// built by Task::new.
//
// The save-area shape is the load-bearing contract between this
// file and the synthetic frame Task::new lays down at task.rs. If
// the push order here disagrees with what Task::new wrote, the first
// switch into a fresh task lands with corrupted state and the bug
// only surfaces when entry's compiled code uses the wrong reg —
// which can be far from the fault site.
//
// Push order (declared first → highest stack address last):
//
//   pushfq             ; ends at saved_rsp + 48   ← 4-4 added
//   push rbx           ;          saved_rsp + 40
//   push rbp           ;          saved_rsp + 32
//   push r12           ;          saved_rsp + 24
//   push r13           ;          saved_rsp + 16
//   push r14           ;          saved_rsp + 8
//   push r15           ;          saved_rsp + 0   ← saved_rsp here
//
// Pop order on switch-in is the reverse: r15, r14, r13, r12, rbp,
// rbx, popfq, ret. Task::new's synthetic frame zero-fills the six
// reg slots, writes 0x202 (IF=1, reserved bit 1) into the rflags
// slot, writes `entry` at saved_rsp + 56 (the slot ret reads), and
// reserves alignment padding above. The rflags save/restore is
// what makes IRQ-driven preemption correct: the timer handler runs
// with IF=0 (Interrupt Gate clears it), so a preempting switch_to
// captures IF=0 on prev. When prev is later resumed by another
// preempt, popfq restores IF=0; control returns to preempt() in
// the timer handler frame, which returns up the stack to IRET —
// IRET pops the IRQ frame's rflags (typically IF=1 from pre-IRQ).
// The cooperative path (yield_now) is similarly correct: IF=1 at
// switch_to entry is captured and restored.

use core::arch::global_asm;
use core::sync::atomic::{AtomicU64, Ordering};

use alloc::boxed::Box;

use crate::apic;
use crate::cpu;
use crate::irq;
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
        pushfq
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
        popfq
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

/// Time-slice budget in periodic-timer ticks. 10 ticks at 100 Hz =
/// 100 ms slice per task. Whole-second cycle through ~10 cooperative
/// tasks; matches HANDOFF.md's M0 single-core target.
const SLICE_TICKS: usize = 10;

/// Add a fresh Ready task to this CPU's runqueue.
#[allow(dead_code)] // wired by 3B-5's ping-pong demo
pub fn spawn(entry: fn() -> !) {
    let task = task::Task::new(entry);
    let _g = irq::IrqGuard::save_and_disable();
    cpu::current_cpu().runqueue.lock().push_back(task);
}

/// Cooperatively yield to the next runnable task. If the runqueue
/// is empty, return immediately without switching — caller resumes.
pub fn yield_now() {
    let cpu = cpu::current_cpu();

    // Disable IRQs for the entire rotation so the periodic timer
    // can't fire mid-swap and call preempt() against a half-rotated
    // cpu.current / runqueue. The IrqGuard lives across switch_to;
    // when this task later resumes, switch_to's popfq restores the
    // saved IF=0 state we captured here, then the IrqGuard's Drop
    // restores the caller's IF=1.
    let _irq = irq::IrqGuard::save_and_disable();

    // Pop next runnable. Empty → no one to switch to; caller resumes.
    let next_box = match cpu.runqueue.lock().pop_front() {
        Some(t) => t,
        None => return,
    };

    let next_saved_rsp = next_box.saved_rsp;
    let next_ptr = Box::into_raw(next_box);

    // Atomically install next as current; receive prev. Single-CPU
    // cooperative makes the swap formally redundant under IF=0, but
    // 4-2's SMP means another core's preempt could race with our
    // observation of cpu.current — the swap closes that window.
    let prev_ptr = cpu.current.swap(next_ptr, Ordering::Relaxed);
    assert!(
        !prev_ptr.is_null(),
        "yield_now: cpu.current was null — sched::init not run?"
    );

    // Reset the slice window: the new "current" gets a fresh
    // SLICE_TICKS budget. Done before the runqueue push so a stray
    // tick that observes inconsistent state still finds preempt_count
    // == 0 and a runqueue empty enough to bail.
    let now = cpu.ticks.load(Ordering::Relaxed);
    cpu.last_switch_tick.store(now, Ordering::Relaxed);

    // Pointer to prev's saved_rsp slot. Stable across the move into
    // the runqueue because Box's heap address doesn't change when
    // the Box itself migrates between containers.
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

    // Switch. When prev later resumes (some future yield or preempt
    // picks it off the runqueue and swaps it back into current),
    // control returns to the instruction after this asm call.
    // switch_to's popfq restores the IF=0 we captured at IrqGuard
    // creation; the IrqGuard's Drop restores the caller's IF state
    // as the function returns up the stack.
    // SAFETY: prev_saved_rsp_ptr points into prev's Task on the
    // heap (lifetime tied to the Box now in the runqueue, which
    // outlives this switch). next_saved_rsp was the saved_rsp of
    // the task currently installed as cpu.current; its frame is
    // either Task::new's synthetic init frame (fresh) or a
    // previously-suspended switch_to frame.
    unsafe { switch_to(prev_saved_rsp_ptr, next_saved_rsp) };
}

/// IRQ-context preemption step. Called from the periodic timer
/// handler after EOI on every tick; rotates the runqueue only if
/// the current task's slice has expired AND no critical section
/// has raised preempt_count. Idempotent at sub-slice granularity.
pub fn preempt() {
    let cpu = cpu::current_cpu();

    if cpu.preempt_count.load(Ordering::Relaxed) > 0 {
        return;
    }

    let now = cpu.ticks.load(Ordering::Relaxed);
    let last = cpu.last_switch_tick.load(Ordering::Relaxed);
    if now.wrapping_sub(last) < SLICE_TICKS {
        return;
    }

    // IF is already 0 here (interrupt gate cleared it); no IrqGuard
    // needed. Try to grab a runnable next. Empty runqueue → nothing
    // to preempt to.
    let next_box = match cpu.runqueue.lock().pop_front() {
        Some(t) => t,
        None => return,
    };

    let next_saved_rsp = next_box.saved_rsp;
    let next_ptr = Box::into_raw(next_box);

    let prev_ptr = cpu.current.swap(next_ptr, Ordering::Relaxed);
    assert!(
        !prev_ptr.is_null(),
        "preempt: cpu.current was null — sched::init not run?"
    );

    cpu.last_switch_tick.store(now, Ordering::Relaxed);

    // SAFETY: identical contract to yield_now's switch_to path —
    // prev_ptr came from a prior Box::into_raw, the Box::from_raw
    // here matches it, and prev's saved_rsp slot lives on the heap
    // until the Box drops (which it won't until the task exits).
    let prev_saved_rsp_ptr = unsafe { &raw mut (*prev_ptr).saved_rsp };
    let prev_box = unsafe { Box::from_raw(prev_ptr) };
    cpu.runqueue.lock().push_back(prev_box);

    // switch_to here saves IF=0 on prev's frame (we're in IRQ
    // context). When prev is later switched back in, popfq restores
    // IF=0; control returns up through preempt() into timer_handler
    // into the IRET sequence the CPU built — IRET pops the original
    // IRQ frame's rflags, restoring whatever IF state prev had
    // pre-IRQ. The rflags propagation chain is the load-bearing
    // contract documented at the top of this file.
    // SAFETY: same as yield_now's switch_to.
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

/// Idle task entry. Yields, observes the timer, then hlts until
/// the next periodic IRQ wakes the core. 3B-4 stripped the hlt
/// because a cooperative CPU with no IRQs would halt forever and
/// starve the runqueue; 3F-3 restores it on top of the 100 Hz
/// LAPIC periodic timer armed by apic::init.
///
/// The `sti` runs once at entry. Cooperative `switch_to` does not
/// save or restore rflags, so IF=1 propagates from this site to
/// every task scheduled after the first idle switch-in — exactly
/// the soft-preemption posture documented at HANDOFF dcf2377.
/// x86-interrupt handlers run through Interrupt Gates (the x86_64
/// crate's default), which clear IF on entry and restore it on
/// `iretq`; there is no nested-IRQ window inside the timer or
/// spurious handler.
fn idle_loop() -> ! {
    serial::write_str("sched: idle running\n");

    // SAFETY: idt::init has already loaded the IDT with the timer
    // (0xEF) and spurious (0xFF) handlers; apic::init has masked
    // the 8259, software-enabled the LAPIC, and armed the periodic
    // timer. Enabling interrupts here is the first moment IF goes
    // to 1 in this kernel; from this instruction forward, any
    // instruction boundary in cooperative code may be interrupted
    // by the timer IRQ (whose handler is trivial — increment +
    // EOI — and runs with IF cleared by the Interrupt Gate).
    unsafe { core::arch::asm!("sti", options(nomem, nostack, preserves_flags)) };

    loop {
        yield_now();
        apic::observe_timer_ok();
        // SAFETY: hlt with IF=1 blocks until the next external
        // interrupt — here, the 100 Hz periodic timer. The
        // instruction has no memory or stack effects; on wake,
        // execution resumes at the next instruction (the loop
        // back-edge). If the runqueue had Ready peers when we
        // entered yield_now, we never reach hlt this iteration:
        // yield_now switched away and control returns here only
        // when idle is next scheduled in.
        unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

