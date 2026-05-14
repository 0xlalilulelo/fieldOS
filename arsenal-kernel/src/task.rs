// SPDX-License-Identifier: BSD-2-Clause
//
// Task = a kernel-mode flow of execution. M0 step 3B is cooperative
// single-CPU; preemption arrives in 3F. A Task owns its own kernel
// stack (16 KiB / 4 frames) and a saved RSP. The first time the
// scheduler switches into a Task, the asm landing in 3B-3 pops six
// callee-save GP regs off this stack and `ret`s into `entry`. The
// synthetic frame Task::new lays down must match the pop order
// switch_to uses; the layout is documented inline below and is the
// load-bearing contract between this file and the asm in 3B-3.

use alloc::boxed::Box;

pub const STACK_SIZE: usize = 32 * 1024;

/// 16-byte aligned so the saved-RSP arithmetic in Task::new lands on
/// 16-byte boundaries that satisfy the SysV ABI when entry is invoked
/// via `ret`.
#[repr(C, align(16))]
struct KernelStack([u8; STACK_SIZE]);

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Ready,
    // Running / Blocked / Exited get constructed when the scheduler
    // wires them in 3B-4 (state transitions on switch-in/out, exit).
    // Listed here in the shape the HANDOFF spec named so 3B-3's asm
    // can reference State::Running without re-opening this enum.
    #[allow(dead_code)]
    Running,
    #[allow(dead_code)]
    Blocked,
    #[allow(dead_code)]
    Exited,
}

pub struct Task {
    /// Saved kernel RSP — the value RSP held at the last switch-out.
    /// On the first switch-in, points into the synthetic frame
    /// Task::new laid down. The asm in 3B-3 reads from / writes to
    /// this through the StackPtr argument it gets.
    pub saved_rsp: u64,
    pub state: State,
    pub entry: fn() -> !,
    /// Backing memory for the kernel stack. Held as a field rather
    /// than leaked because the stack must not outlive the Task that
    /// runs on it. Read only by Drop.
    #[allow(dead_code)]
    stack: Box<KernelStack>,
}

impl Task {
    /// Construct a Task whose first scheduled run will land in `entry`.
    ///
    /// `entry` must not return — there is no exit trampoline yet
    /// (3B-4 wires the scheduler-managed exit path). If entry does
    /// return before 3B-4 lands, `ret` pops the alignment-padding
    /// slot (zero) into RIP and the kernel faults on RIP=0, which
    /// the page-fault handler at idt.rs:64 will at least surface.
    pub fn new(entry: fn() -> !) -> Box<Self> {
        let stack = Box::new(KernelStack([0u8; STACK_SIZE]));
        let stack_top = (&stack.0 as *const u8 as usize + STACK_SIZE) as u64;

        // Initial stack frame, matched to the 4-4 switch_to epilogue:
        //
        //     pop r15
        //     pop r14
        //     pop r13
        //     pop r12
        //     pop rbp
        //     pop rbx
        //     popfq         ; restores rflags (4-4 added)
        //     ret           ; pops entry into RIP
        //
        // Memory from saved_rsp upward:
        //
        //   saved_rsp + 0     r15 = 0
        //   saved_rsp + 8     r14 = 0
        //   saved_rsp + 16    r13 = 0
        //   saved_rsp + 24    r12 = 0
        //   saved_rsp + 32    rbp = 0
        //   saved_rsp + 40    rbx = 0
        //   saved_rsp + 48    rflags = 0x202 (IF=1, reserved bit 1)
        //   saved_rsp + 56    entry   (return address — popped by ret)
        //   saved_rsp + 64    alignment padding (unused)
        //
        // saved_rsp = stack_top - 72 places saved_rsp at offset 8
        // within a 16-byte aligned block (stack_top is 16-aligned;
        // 72 mod 16 = 8). After all pops + popfq + ret, RSP =
        // saved_rsp + 64 = stack_top - 8 → (RSP) mod 16 = 8,
        // satisfying the SysV ABI on entry's first instruction
        // (the ABI requires rsp ≡ 8 mod 16 at function entry).
        let saved_rsp = stack_top - 72;
        // SAFETY: [saved_rsp, saved_rsp+72) is the top 72 bytes of
        // a freshly-allocated KernelStack we exclusively own. The
        // pointer is 8-aligned (stack_top - 72, stack_top is 16-
        // aligned by KernelStack's repr); STACK_SIZE (32 KiB) is
        // well above 72 so we stay inside the allocation.
        unsafe {
            let p = saved_rsp as *mut u64;
            for i in 0..6 {
                p.add(i).write(0); // r15..rbx
            }
            // rflags slot: IF=1 (bit 9) + reserved bit 1 set per
            // Intel SDM Vol. 1 §3.4.3. Fresh tasks start with
            // interrupts enabled.
            p.add(6).write(0x202);
            p.add(7).write(entry as usize as u64);
            p.add(8).write(0); // alignment padding
            debug_assert_eq!(p.add(7).read(), entry as usize as u64);
        }

        Box::new(Self {
            saved_rsp,
            state: State::Ready,
            entry,
            stack,
        })
    }
}
