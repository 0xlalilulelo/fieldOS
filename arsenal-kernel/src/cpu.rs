// SPDX-License-Identifier: BSD-2-Clause
//
// Per-CPU data area. Single-CPU through M0 step 3; the array shape
// exists so SMP arrival in 3F populates additional entries rather
// than refactoring every caller. 3B-1 lands just the id + array +
// accessor; 3B-2 augments CpuLocal with task pointers; 3B-4 adds the
// runqueue head.
//
// Per-CPU access is by array index today. 3F replaces the index path
// with a GS-base register + SWAPGS so each CPU reaches its own
// CpuLocal in O(1) without consulting the LAPIC on the fast path.

const MAX_CPUS: usize = 64;

#[repr(C)]
pub struct CpuLocal {
    pub id: u32,
}

impl CpuLocal {
    const fn new(id: u32) -> Self {
        Self { id }
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
