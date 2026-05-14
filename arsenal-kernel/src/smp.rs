// SPDX-License-Identifier: BSD-2-Clause
//
// SMP bring-up — M0 step 4-2. Uses Limine's MP (Multi-Processor)
// request to start every AP the firmware reports. Limine handles
// the real-mode → 32-bit → 64-bit transition for each AP and
// hands it to our `ap_entry` in long mode with the kernel's CR3
// already loaded.
//
// HANDOFF.md proposed a hand-rolled INIT-SIPI-SIPI trampoline at
// physical 0x8000 (~250-300 LOC including a real-mode asm stub).
// At code-time we picked Limine's MP infrastructure instead — same
// "use Limine, do not write your own" stance CLAUDE.md applies to
// the BSP boot path, ~60 LOC of kernel-side glue, and zero extra
// page-reservation / trampoline-asm surface. The interesting
// kernel-side work (per-AP CpuLocal, per-AP LAPIC bring-up, per-AP
// idle loop) is preserved. 4-6's STATUS retrospective records the
// deviation; the rest of step 4 is unaffected.
//
// One load-bearing constraint surfaced from the choice: Limine
// allocates each AP a 64 KiB stack inside BOOTLOADER_RECLAIMABLE
// memory. The APs sit on those stacks indefinitely (hlt loop at
// M0), so calling `frames::reclaim_bootloader` after smp::init
// would hand those stack frames back to the allocator and corrupt
// any AP that takes an interrupt later. Reclamation is therefore
// deferred until APs are on kernel-owned scheduler stacks at 4-4;
// main.rs notes this at the deferred reclaim site.

use core::fmt::Write;
use core::hint::spin_loop;
use core::sync::atomic::{AtomicU32, Ordering};

use limine::mp;
use limine::response::MpResponse;

use crate::apic;
use crate::cpu;
use crate::idt;
use crate::paging;
use crate::serial;

/// Incremented by each AP from `ap_entry` once its per-CPU state is
/// live and its LAPIC is software-enabled. BSP spins on this until
/// it matches the AP count it issued goto_address writes for, then
/// emits ARSENAL_SMP_OK.
static AP_ONLINE_COUNT: AtomicU32 = AtomicU32::new(0);

/// BSP-side: bring up every AP Limine reports. Caller passes the
/// already-fetched MpResponse (main.rs owns the request static).
pub fn init(mp_response: &MpResponse) {
    let cpus = mp_response.cpus();
    let bsp_lapic_id = mp_response.bsp_lapic_id();

    let _ = writeln!(
        serial::Writer,
        "smp: limine reports {} CPUs (BSP lapic_id={}); bringing up APs",
        cpus.len(),
        bsp_lapic_id,
    );

    let mut next_slot: u32 = 1;
    let mut launched: u32 = 0;
    for cpu in cpus {
        if cpu.lapic_id == bsp_lapic_id {
            continue;
        }
        if (next_slot as usize) >= cpu::MAX_CPUS {
            let _ = writeln!(
                serial::Writer,
                "smp: skipping AP lapic_id={} — slot {} ≥ MAX_CPUS",
                cpu.lapic_id, next_slot,
            );
            continue;
        }

        // Stash the slot index in `extra` before publishing the
        // goto_address. mp::GotoAddress::write uses SeqCst, which
        // synchronizes this prior store with the AP's first read.
        cpu.extra.store(next_slot as u64, Ordering::Relaxed);
        cpu.goto_address.write(ap_entry);

        let _ = writeln!(
            serial::Writer,
            "smp: launched AP lapic_id={} -> slot {}",
            cpu.lapic_id, next_slot,
        );
        next_slot += 1;
        launched += 1;
    }

    // Wait for every launched AP to report online. Empty loop is
    // fine; APs come up in ~10-100 ms on QEMU TCG.
    while AP_ONLINE_COUNT.load(Ordering::SeqCst) < launched {
        spin_loop();
    }

    let _ = writeln!(
        serial::Writer,
        "smp: {launched} APs online",
    );
    serial::write_str("ARSENAL_SMP_OK\n");
}

/// AP-side entry called by Limine on each non-BSP core after it has
/// brought the core to 64-bit long mode with the kernel's CR3
/// loaded. Limine passes the per-CPU mp::Cpu pointer; we read the
/// slot index out of `extra` (which the BSP populated before the
/// goto_address write).
///
/// # Safety
/// Invoked exclusively by Limine's MP infrastructure on a fresh AP.
/// Limine guarantees the function runs in 64-bit long mode on a
/// 64 KiB stack with the BSP's CR3 already loaded; we must never
/// return (signature `-> !`) because the caller is Limine's per-AP
/// loader stub with no Rust frame above us.
unsafe extern "C" fn ap_entry(cpu: &mp::Cpu) -> ! {
    let slot = cpu.extra.load(Ordering::Relaxed) as u32;
    let apic_id = cpu.lapic_id;

    // Limine starts APs with Limine's own PML4 loaded — Limine's
    // mappings only, none of the post-deep-clone additions the BSP
    // made via paging::map_mmio (LAPIC, IOAPIC, virtio BARs).
    // Switching to the kernel PML4 first thing here makes the AP's
    // view of memory identical to the BSP's; touching the LAPIC
    // MMIO in apic::ap_init below depends on this.
    //
    // SAFETY: the kernel PML4 is a deep clone produced by
    // paging::init on the BSP at boot; it contains every mapping
    // the BSP uses, including the code/data this function lives in.
    // Writing CR3 on the AP flushes the AP's TLB. Limine's stack
    // that this function runs on lies in Limine's HHDM mapping,
    // which paging::init preserved.
    unsafe {
        core::arch::asm!(
            "mov cr3, {0}",
            in(reg) paging::kernel_pml4_phys(),
            options(nostack, preserves_flags),
        );
    }

    cpu::init_ap(slot, apic_id);
    idt::init();
    apic::ap_init();

    AP_ONLINE_COUNT.fetch_add(1, Ordering::SeqCst);

    loop {
        // SAFETY: hlt with IF=0 blocks until the next interrupt or
        // exception. APs at M0 step 4-2 have no IPIs targeted at
        // them yet, so they stay halted until 4-4 drives the AP
        // scheduler. nomem/nostack/preserves_flags is standard.
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}
