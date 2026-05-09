// SPDX-License-Identifier: BSD-2-Clause
//
// Per-CPU data area. Single-CPU through M0 step 3; the array shape
// exists so SMP arrival in 3F populates additional entries rather
// than refactoring every caller.
//
// 3B-1 lit up just the id; 3B-4 augments CpuLocal with the
// scheduler's per-CPU state — `current` (running task), `idle`
// (fallback task), and `runqueue` (Ready tasks). 3F replaces the
// array-index path with a GS-base register + SWAPGS so each CPU
// reaches its own CpuLocal in O(1) without consulting the LAPIC.

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use core::sync::atomic::AtomicPtr;
use spin::Mutex;

use crate::task::Task;

const MAX_CPUS: usize = 64;

#[repr(C)]
pub struct CpuLocal {
    pub id: u32,
    /// Currently executing task on this CPU. Updated by sched::init
    /// (initial install) and sched::yield_now (every switch).
    /// AtomicPtr because reads cross stack switches where the
    /// borrow checker can't follow ownership; the swap on yield is
    /// the atomic primitive that prevents a window where two Boxes
    /// alias the same Task.
    pub current: AtomicPtr<Task>,
    /// Idle task. Runs when the runqueue is empty. Spawned by
    /// sched::init at boot; never enqueued. Never exits.
    pub idle: AtomicPtr<Task>,
    /// Round-robin runqueue of Ready tasks. yield_now pulls front,
    /// pushes the previous-current to the back. Box ownership lives
    /// in the queue when the task is Ready; transfers to `current`
    /// for the duration of execution.
    pub runqueue: Mutex<VecDeque<Box<Task>>>,
}

impl CpuLocal {
    const fn new(id: u32) -> Self {
        Self {
            id,
            current: AtomicPtr::new(core::ptr::null_mut()),
            idle: AtomicPtr::new(core::ptr::null_mut()),
            runqueue: Mutex::new(VecDeque::new()),
        }
    }
}

const fn build_cpus() -> [CpuLocal; MAX_CPUS] {
    let mut arr: [CpuLocal; MAX_CPUS] = [const { CpuLocal::new(0) }; MAX_CPUS];
    let mut i = 0;
    while i < MAX_CPUS {
        arr[i].id = i as u32;
        i += 1;
    }
    arr
}

static CPUS: [CpuLocal; MAX_CPUS] = build_cpus();

/// This CPU's local data. Single-CPU through M0 step 3 — always
/// returns &CPUS[0]. 3F replaces the body with a GS-relative fetch
/// backed by SWAPGS.
pub fn current_cpu() -> &'static CpuLocal {
    &CPUS[0]
}
