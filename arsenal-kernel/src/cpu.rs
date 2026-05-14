// SPDX-License-Identifier: BSD-2-Clause
//
// Per-CPU data area. M0 step 4-1: GS-base self-pointer idiom.
// Each CPU's CpuLocal lives in a fixed slot of the static CPUS
// array. cpu::init_bsp (BSP) and 4-2's ap_init (APs) populate the
// slot's self_ptr and write MSR_GS_BASE so a `mov reg, gs:[0]`
// returns the self-pointer — i.e., &CpuLocal for the current core.
//
// At 4-1 only slot 0 is in use; 4-2's INIT-SIPI-SIPI populates
// additional slots from inside ap_entry. Per the HANDOFF.md trade-
// off pair "Per-CPU storage mechanism": the GS-base idiom is one
// asm instruction per access, scales to userspace per-thread
// storage via swapgs at M2, and is the canonical x86_64 kernel
// pattern (Linux, FreeBSD, illumos, Solaris all do this).

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicUsize, Ordering};
use spin::Mutex;

use crate::apic;
use crate::task::Task;

pub const MAX_CPUS: usize = 64;

/// IA32_GS_BASE (Intel SDM Vol. 4 §2.5). Writing the CpuLocal slot
/// address here makes `gs:[N]` reach offset N of that struct from
/// any context running on this core.
const MSR_GS_BASE: u32 = 0xC0000101;

#[repr(C)]
pub struct CpuLocal {
    /// Self-pointer at offset 0 — `mov rax, gs:[0]` returns &CpuLocal
    /// for this core. Null until cpu::init_bsp / ap_init runs;
    /// calling current_cpu() before that is UB.
    self_ptr: AtomicPtr<CpuLocal>,
    pub id: u32,
    /// LAPIC ID for this core. Cached from apic::lapic_id() at
    /// init_bsp; 4-2's ap_init writes each AP's own. The shell's
    /// `hw` command reads this; 4-2 uses it for IPI target matching.
    #[allow(dead_code)]
    pub apic_id: AtomicU32,
    /// Currently executing task on this CPU. Updated by sched::init
    /// (initial install) and sched::yield_now (every switch).
    pub current: AtomicPtr<Task>,
    /// Idle task. Spawned by sched::init at boot; never enqueued,
    /// never exits.
    pub idle: AtomicPtr<Task>,
    /// Round-robin runqueue of Ready tasks.
    pub runqueue: Mutex<VecDeque<Box<Task>>>,
    /// Periodic timer tick counter. Incremented by apic::timer_handler
    /// on this core; read by apic::ticks() / observe_timer_ok. Per-core
    /// because 4-2's BSP-driven scheduler-tick IPIs will increment each
    /// AP's counter independently.
    pub ticks: AtomicUsize,
    /// Latched once per core on the first spurious LAPIC IRQ so the
    /// serial log records the event exactly once and a misconfigured
    /// bring-up cannot drown serial output before it can be observed.
    pub spurious_seen: AtomicBool,
    /// Preempt-disable counter. 4-4's timer-handler dispatcher will
    /// skip the runqueue rotation when this is non-zero; declared at
    /// 4-1 so the layout is stable for AP startup at 4-2 before the
    /// preemption code lands.
    #[allow(dead_code)]
    pub preempt_count: AtomicUsize,
}

impl CpuLocal {
    const fn new(id: u32) -> Self {
        Self {
            self_ptr: AtomicPtr::new(core::ptr::null_mut()),
            id,
            apic_id: AtomicU32::new(0),
            current: AtomicPtr::new(core::ptr::null_mut()),
            idle: AtomicPtr::new(core::ptr::null_mut()),
            runqueue: Mutex::new(VecDeque::new()),
            ticks: AtomicUsize::new(0),
            spurious_seen: AtomicBool::new(false),
            preempt_count: AtomicUsize::new(0),
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

/// Initialize the BSP's per-CPU storage. Must run after apic::init
/// (we cache lapic_id) and before any IRQ-context code can fire on
/// this core — the timer handler at vector 0xEF calls current_cpu()
/// and would dereference garbage if GS base were still zero.
pub fn init_bsp() {
    init_slot(0, apic::lapic_id() as u32);
}

/// Initialize an AP's per-CPU storage at slot `slot` for the core
/// whose LAPIC ID is `apic_id`. Called from smp::ap_entry on the AP
/// itself, after Limine has handed the core to us in 64-bit long
/// mode with the kernel's CR3 loaded. From the moment this returns,
/// `current_cpu()` is callable on this core and resolves to slot.
pub fn init_ap(slot: u32, apic_id: u32) {
    init_slot(slot, apic_id);
}

fn init_slot(slot_idx: u32, apic_id: u32) {
    assert!(
        (slot_idx as usize) < MAX_CPUS,
        "cpu: slot index {slot_idx} ≥ MAX_CPUS={MAX_CPUS}",
    );
    let slot = &CPUS[slot_idx as usize] as *const CpuLocal as *mut CpuLocal;

    // SAFETY: slot points at CPUS[slot_idx], a 'static slot with no
    // &mut aliases. Writing to its atomic fields is a relaxed store.
    let cpu = unsafe { &*slot };
    cpu.self_ptr.store(slot, Ordering::Relaxed);
    cpu.apic_id.store(apic_id, Ordering::Relaxed);

    // SAFETY: MSR_GS_BASE write per Intel SDM Vol. 3A §9.11.13.
    // In 64-bit mode the GS base is set by this MSR regardless of
    // any segment-register reload; subsequent `gs:[N]` accesses on
    // this core resolve to slot + N. Ring 0.
    unsafe {
        wrmsr(MSR_GS_BASE, slot as u64);
    }
}

/// This CPU's local data. Reads the self-pointer at gs:[0], loaded
/// by init_bsp / ap_init on this core. Calling this before either
/// has run on the current core is undefined — the GS base will be
/// zero post-reset, and gs:[0] would fault or read stale memory.
pub fn current_cpu() -> &'static CpuLocal {
    let ptr: *const CpuLocal;
    // SAFETY: gs:[0] holds the self-pointer that init_bsp / ap_init
    // wrote on this core. Its pointee is a 'static slot in CPUS.
    unsafe {
        core::arch::asm!(
            "mov {0}, gs:[0]",
            out(reg) ptr,
            options(readonly, nostack, preserves_flags),
        );
        &*ptr
    }
}

/// Write `value` to MSR `msr`.
///
/// # Safety
/// Caller must be in ring 0 (CPL=0) and must understand the
/// architectural effect of writing the target MSR. Several MSRs
/// (EFER, PAT, the IA32_APIC_BASE control bits) reconfigure the
/// kernel's execution environment globally.
unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    // SAFETY: caller asserts ring 0 and understanding of side
    // effects; wrmsr takes its operands in ecx (MSR index) and
    // edx:eax (value) per Intel SDM Vol. 2B WRMSR.
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") low,
            in("edx") high,
            options(nostack, preserves_flags),
        );
    }
}
