// SPDX-License-Identifier: BSD-2-Clause

//! M1-3 xHCI host controller — native Rust per
//! [ADR-0009](../../docs/adrs/0009-xhci-native-rust.md).
//!
//! **3-1 state: host-controller bring-up, MSI-X-driven.** Runs the
//! xHCI 1.2 §4.2 initialization sequence — read capability
//! registers, reset, allocate the DCBAA + command ring + event ring
//! (+ ERST), enable bus mastering, wire interrupter 0 to an MSI-X
//! vector, run the controller — then posts a No-Op command and waits
//! for its Command Completion Event to arrive via an MSI-X interrupt
//! (the NVMe pattern: a thin IRQ handler bumps a counter, the boot
//! path opens a brief `sti` window and spins on it). Also resets any
//! connected root ports so 3-2's enumeration starts from enabled
//! ports. Emits `ARSENAL_XHCI_OK` on the interrupt-delivered
//! completion.
//!
//! 3-2 builds device enumeration (Enable Slot → Address Device →
//! GET_DESCRIPTOR → SET_CONFIGURATION) on top; 3-3 / 3-4 add the HID
//! and mass-storage class drivers. `run` no-ops when no xHCI
//! controller is present.
//!
//! The spec-fragile pieces (xHCI §5): CAPLENGTH/RTSOFF/DBOFF offset
//! math, 32-vs-64-byte contexts (HCCPARAMS1.CSZ), ring producer/
//! consumer cycle bits, the DCBAA, ERST, and the interrupter
//! IMAN/ERDP handshake. The 3-0 spike validated the polled path;
//! 3-1 adds the interrupt path.

use core::fmt::Write;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU64, Ordering};

use x86_64::structures::idt::InterruptStackFrame;

use crate::{apic, frames, idt, paging, pci, serial};

// xHCI class code (PCI): 0x0C serial bus, 0x03 USB, 0x30 xHCI.
const XHCI_CLASS: u8 = 0x0C;
const XHCI_SUBCLASS: u8 = 0x03;
const XHCI_PROG_IF: u8 = 0x30;

// qemu-xhci's register file fits comfortably in 64 KiB.
const BAR_MAP_SIZE: u64 = 0x1_0000;

// Capability registers (offsets from the BAR base).
const CAP_CAPLENGTH: usize = 0x00; // u8 (+ HCIVERSION at 0x02)
const CAP_HCSPARAMS1: usize = 0x04;
const CAP_HCSPARAMS2: usize = 0x08;
const CAP_HCCPARAMS1: usize = 0x10;
const CAP_DBOFF: usize = 0x14;
const CAP_RTSOFF: usize = 0x18;

// Operational registers (offsets from op_base = cap_base + CAPLENGTH).
const OP_USBCMD: usize = 0x00;
const OP_USBSTS: usize = 0x04;
const OP_CRCR: usize = 0x18; // 64-bit
const OP_DCBAAP: usize = 0x30; // 64-bit
const OP_CONFIG: usize = 0x38;
const OP_PORTS_BASE: usize = 0x400; // PORTSC[n] at +0x400 + n*0x10
const PORT_REG_STRIDE: usize = 0x10;

const USBCMD_RS: u32 = 1 << 0;
const USBCMD_HCRST: u32 = 1 << 1;
const USBCMD_INTE: u32 = 1 << 2;
const USBSTS_HCH: u32 = 1 << 0;
const USBSTS_CNR: u32 = 1 << 11; // Controller Not Ready

// PORTSC bits.
const PORTSC_CCS: u32 = 1 << 0; // current connect status
const PORTSC_PED: u32 = 1 << 1; // port enabled/disabled
const PORTSC_PR: u32 = 1 << 4; // port reset
const PORTSC_PRC: u32 = 1 << 21; // port reset change
// Write-1-to-clear status-change bits to preserve on a R-M-W write.
const PORTSC_RW1C: u32 =
    (1 << 17) | (1 << 18) | (1 << 19) | (1 << 20) | (1 << 21) | (1 << 22) | (1 << 23);

// Interrupter 0 register set (offsets from rt_base + 0x20).
const IR0_IMAN: usize = 0x00;
const IR0_IMOD: usize = 0x04;
const IR0_ERSTSZ: usize = 0x08;
const IR0_ERSTBA: usize = 0x10; // 64-bit
const IR0_ERDP: usize = 0x18; // 64-bit

const IMAN_IP: u32 = 1 << 0; // interrupt pending (write-1-clear)
const IMAN_IE: u32 = 1 << 1; // interrupt enable
const ERDP_EHB: u64 = 1 << 3; // event handler busy (write-1-clear)

// MSI-X: a single config vector for interrupter 0.
const MSIX_CTRL_ENABLE_DWORD_BIT: u32 = 1 << 31;

// xHCI TRB size (event/command/transfer ring entries are 16 bytes).
const TRB_BYTES: u64 = 16;

// TRB types.
const TRB_TYPE_NOOP_CMD: u32 = 23;
const TRB_TYPE_CMD_COMPLETION: u32 = 33;
const TRB_TYPE_PORT_STATUS_CHANGE: u32 = 34;

// Event ring segment size in TRBs (matches the ERSTSZ=1 entry below).
const EVENT_RING_TRBS: usize = 16;

const POLL_LIMIT: u64 = 50_000_000;

/// Bumped by the interrupter-0 MSI-X handler; the boot path waits on
/// it across a brief `sti` window (the NVMe pattern).
static XHCI_IRQ_COUNT: AtomicU64 = AtomicU64::new(0);

/// Interrupter-0 MSI-X handler. Thin: count + EOI. The cooperative
/// boot path drains the event ring and clears IMAN.IP / advances
/// ERDP after observing the bump — same division of labor as nvme's
/// `nvme_io_handler`.
extern "x86-interrupt" fn xhci_irq_handler(_frame: InterruptStackFrame) {
    XHCI_IRQ_COUNT.fetch_add(1, Ordering::Release);
    apic::send_eoi();
}

struct Regs {
    /// 3-2 reads this for the extended-capability list (HCCPARAMS1
    /// xECP). The port register file is op-relative (op_base + 0x400).
    #[allow(dead_code)]
    cap_base: usize,
    op_base: usize,
    rt_base: usize,
    db_base: usize,
}

#[inline]
unsafe fn r32(p: usize) -> u32 {
    // SAFETY: caller guarantees p is in the mapped BAR window.
    unsafe { read_volatile(p as *const u32) }
}
#[inline]
unsafe fn w32(p: usize, v: u32) {
    // SAFETY: caller's contract.
    unsafe { write_volatile(p as *mut u32, v) }
}
#[inline]
unsafe fn w64(p: usize, v: u64) {
    // SAFETY: caller's contract; p is 8-byte aligned.
    unsafe { write_volatile(p as *mut u64, v) }
}

fn phys_to_virt(phys: u64) -> usize {
    phys as usize + paging::hhdm_offset() as usize
}

fn alloc_zeroed_frame() -> u64 {
    let f = frames::FRAMES
        .alloc_frame()
        .expect("xhci: OOM allocating a DMA frame");
    let phys = f.start_address().as_u64();
    // SAFETY: freshly-allocated frame, HHDM-mapped, exclusively owned.
    unsafe { core::ptr::write_bytes(phys_to_virt(phys) as *mut u8, 0, 4096) };
    phys
}

/// Bring up the xHCI host controller. Panics on any spec-violation
/// it can detect — a wedged controller at boot is a hard failure.
pub fn run() {
    let (bdf, bar_phys) = match find_controller() {
        Some(x) => x,
        // No xHCI controller present (the production smoke has no
        // qemu-xhci) — stay silent so the smoke serial is unaffected.
        None => return,
    };
    let _ = writeln!(serial::Writer, "xhci: === host controller bring-up ===");

    paging::map_mmio(bar_phys, BAR_MAP_SIZE);
    let cap_base = bar_phys as usize + paging::hhdm_offset() as usize;

    // --- Capability registers ---
    // SAFETY: BAR mapped above; these offsets are within the cap window.
    let (caplength, hciversion, hcs1, hcs2, _hcc1, dboff, rtsoff) = unsafe {
        let caplength = read_volatile((cap_base + CAP_CAPLENGTH) as *const u8);
        let hciversion = read_volatile((cap_base + 0x02) as *const u16);
        (
            caplength,
            hciversion,
            r32(cap_base + CAP_HCSPARAMS1),
            r32(cap_base + CAP_HCSPARAMS2),
            r32(cap_base + CAP_HCCPARAMS1),
            r32(cap_base + CAP_DBOFF),
            r32(cap_base + CAP_RTSOFF),
        )
    };
    let max_slots = (hcs1 & 0xFF) as u8;
    let max_ports = ((hcs1 >> 24) & 0xFF) as u8;
    let max_scratch = (((hcs2 >> 21) & 0x1F) << 5) | ((hcs2 >> 27) & 0x1F);
    let regs = Regs {
        cap_base,
        op_base: cap_base + caplength as usize,
        rt_base: cap_base + (rtsoff & !0x1F) as usize,
        db_base: cap_base + (dboff & !0x3) as usize,
    };

    let _ = writeln!(
        serial::Writer,
        "xhci: ctrl at {bdf:?} bar={bar_phys:#x} hciversion={hciversion:#06x} \
         caplength={caplength:#x} max_slots={max_slots} max_ports={max_ports} \
         max_scratch={max_scratch}",
    );

    // qemu-xhci reports 0 scratchpad buffers; a nonzero count would
    // need a scratchpad array in DCBAA[0] before the controller runs.
    assert_eq!(
        max_scratch, 0,
        "xhci: controller wants {max_scratch} scratchpad buffers; 3-1 does not allocate them"
    );

    // Enable PCI bus mastering + memory space. BME is the step-2
    // carry-forward: an MSI is a bus-master write QEMU drops when BME
    // is clear, and ring DMA needs it regardless.
    // SAFETY: standard PCI config RMW; COMMAND is at offset 0x04.
    unsafe {
        let cmd = pci::config_read32(bdf.bus, bdf.dev, bdf.func, 0x04);
        pci::config_write32(bdf.bus, bdf.dev, bdf.func, 0x04, cmd | 0x06);
    }

    // --- Reset (§4.2 step 1-3) ---
    // SAFETY: op registers within the mapped BAR.
    unsafe {
        let cmd = r32(regs.op_base + OP_USBCMD);
        if cmd & USBCMD_RS != 0 {
            w32(regs.op_base + OP_USBCMD, cmd & !USBCMD_RS);
            poll_until(regs.op_base + OP_USBSTS, USBSTS_HCH, USBSTS_HCH, "halt");
        }
        w32(regs.op_base + OP_USBCMD, USBCMD_HCRST);
        poll_until(regs.op_base + OP_USBCMD, USBCMD_HCRST, 0, "reset-clear");
        poll_until(regs.op_base + OP_USBSTS, USBSTS_CNR, 0, "controller-ready");
    }

    // --- Rings + DCBAA (§4.2 step 6-9) ---
    let dcbaa_phys = alloc_zeroed_frame();
    let cmd_ring_phys = alloc_zeroed_frame();
    let event_ring_phys = alloc_zeroed_frame();
    let erst_phys = alloc_zeroed_frame();
    // ERST[0] = { ring segment base, size = 16 TRBs }.
    // SAFETY: erst frame owned + HHDM-mapped.
    unsafe {
        let erst = phys_to_virt(erst_phys) as *mut u32;
        write_volatile(erst, event_ring_phys as u32);
        write_volatile(erst.add(1), (event_ring_phys >> 32) as u32);
        write_volatile(erst.add(2), EVENT_RING_TRBS as u32);
        write_volatile(erst.add(3), 0);
    }

    // --- MSI-X: route interrupter 0 to a fresh IDT vector ---
    let vector = idt::register_vector(xhci_irq_handler);
    program_msix_entry(bdf, vector, apic::lapic_id());

    // --- Program operational + interrupter-0 registers ---
    // SAFETY: all offsets within the mapped BAR; values per spec.
    unsafe {
        w32(regs.op_base + OP_CONFIG, max_slots as u32);
        w64(regs.op_base + OP_DCBAAP, dcbaa_phys);
        w64(regs.op_base + OP_CRCR, cmd_ring_phys | 1); // RCS=1
        let ir0 = regs.rt_base + 0x20;
        w32(ir0 + IR0_ERSTSZ, 1);
        w32(ir0 + IR0_IMOD, 0); // no interrupt moderation
        w64(ir0 + IR0_ERDP, event_ring_phys);
        w64(ir0 + IR0_ERSTBA, erst_phys);
        w32(ir0 + IR0_IMAN, IMAN_IE); // enable interrupter 0 (IP starts clear)
    }

    // --- Run (§4.2 step 10), interrupts enabled ---
    // SAFETY: op registers mapped.
    unsafe {
        let cmd = r32(regs.op_base + OP_USBCMD);
        w32(regs.op_base + OP_USBCMD, cmd | USBCMD_RS | USBCMD_INTE);
        poll_until(regs.op_base + OP_USBSTS, USBSTS_HCH, 0, "running");
    }
    let _ = writeln!(
        serial::Writer,
        "xhci: running; interrupter 0 -> vector {vector:#04x} (BME + INTE set)",
    );

    // --- Reset any connected root ports (sets up 3-2 enumeration) ---
    reset_connected_ports(&regs, max_ports);

    // --- Post a No-Op command, wait for its completion via MSI-X ---
    // SAFETY: command ring frame owned + mapped; TRB layout §6.4.3.1.
    unsafe {
        let trb = phys_to_virt(cmd_ring_phys) as *mut u32;
        write_volatile(trb.add(3), (TRB_TYPE_NOOP_CMD << 10) | 1); // type | cycle=1
    }
    let target = XHCI_IRQ_COUNT.load(Ordering::Acquire) + 1;
    // Ring the command doorbell (DB 0, target 0).
    // SAFETY: doorbell array within the mapped BAR.
    unsafe { w32(regs.db_base, 0) };

    // Open a brief sti window for the MSI to land. main runs with
    // IF=0 until sched's sti; xhci::run executes before any
    // sched::spawn, so the runqueue is empty and a concurrent timer
    // tick's preempt is a no-op — identical to nvme's boot-time MSI
    // window.
    // SAFETY: sti/cli are ring-0 privileged; IF toggle only.
    unsafe { core::arch::asm!("sti", options(nomem, nostack)) };

    // Drain the event ring. It is multi-event: the port resets above
    // enqueued Port Status Change Events (type 34) ahead of the No-Op's
    // Command Completion Event (type 33), so we cannot read a fixed
    // slot — we walk the ring honoring the consumer cycle state (CCS,
    // starts 1; flips on each 16-TRB wrap) until we observe the
    // command completion. Interrupts stay on, so the MSI fires and
    // bumps XHCI_IRQ_COUNT; we assert it advanced afterward to prove
    // the interrupt path delivered (not just that the ring updated).
    let mut deq: usize = 0;
    let mut ccs: u32 = 1;
    let mut cmd_cc: Option<u32> = None;
    let mut spins = 0u64;
    while cmd_cc.is_none() {
        // SAFETY: deq < 16; event ring frame owned + mapped.
        let (ctrl, status) = unsafe {
            let p = phys_to_virt(event_ring_phys) as *const u32;
            (read_volatile(p.add(deq * 4 + 3)), read_volatile(p.add(deq * 4 + 2)))
        };
        if (ctrl & 1) != ccs {
            // No more events produced yet; wait for the controller / MSI.
            spins += 1;
            assert!(
                spins < POLL_LIMIT,
                "xhci: No-Op completion never appeared (BME/interrupter/ring wiring?)"
            );
            core::hint::spin_loop();
            continue;
        }
        let ev_type = (ctrl >> 10) & 0x3F;
        let cc = (status >> 24) & 0xFF;
        match ev_type {
            TRB_TYPE_CMD_COMPLETION => cmd_cc = Some(cc),
            TRB_TYPE_PORT_STATUS_CHANGE => {
                let _ = writeln!(serial::Writer, "xhci: drained port-status-change event");
            }
            other => {
                let _ = writeln!(serial::Writer, "xhci: drained event type={other} (cc={cc})");
            }
        }
        deq += 1;
        if deq == EVENT_RING_TRBS {
            deq = 0;
            ccs ^= 1;
        }
    }
    // SAFETY: see above.
    unsafe { core::arch::asm!("cli", options(nomem, nostack)) };

    // Advance ERDP to the current dequeue position (clearing EHB) and
    // clear the interrupt-pending bit (keeping IE set).
    // SAFETY: interrupter 0 registers within the mapped BAR.
    unsafe {
        let ir0 = regs.rt_base + 0x20;
        w64(ir0 + IR0_ERDP, (event_ring_phys + deq as u64 * TRB_BYTES) | ERDP_EHB);
        w32(ir0 + IR0_IMAN, IMAN_IE | IMAN_IP);
    }

    let irq_count = XHCI_IRQ_COUNT.load(Ordering::Acquire);
    assert!(
        irq_count >= target,
        "xhci: command completed but no MSI-X delivered (irq_count={irq_count}); interrupt path not validated"
    );
    let cc = cmd_cc.unwrap();
    assert!(cc == 1, "xhci: No-Op command completion_code={cc} (wanted 1=success)");
    let _ = writeln!(
        serial::Writer,
        "xhci: No-Op completion via MSI-X (cc={cc} irq_count={irq_count})",
    );
    serial::write_str("ARSENAL_XHCI_OK\n");
}

/// Reset each root port that reports a connected device, so 3-2's
/// enumeration starts from enabled ports. Logs the per-port result.
/// PORTSC has write-1-to-clear change bits — mask them on the R-M-W
/// reset write so they are not inadvertently cleared.
fn reset_connected_ports(regs: &Regs, max_ports: u8) {
    for n in 0..max_ports {
        let portsc = regs.op_base + OP_PORTS_BASE + n as usize * PORT_REG_STRIDE;
        // SAFETY: PORTSC[n] is within the mapped operational window for n < max_ports.
        let sc = unsafe { r32(portsc) };
        if sc & PORTSC_CCS == 0 {
            continue; // nothing attached
        }
        // SAFETY: R-M-W with RW1C change bits masked off + PR set.
        unsafe {
            w32(portsc, (sc & !PORTSC_RW1C) | PORTSC_PR);
        }
        // Wait for the reset to complete (PR clears, PRC sets).
        let mut spins = 0u64;
        loop {
            // SAFETY: PORTSC[n] mapped.
            let cur = unsafe { r32(portsc) };
            if cur & PORTSC_PR == 0 && cur & PORTSC_PRC != 0 {
                let enabled = cur & PORTSC_PED != 0;
                // Clear PRC (write-1-clear) without disturbing other bits.
                // SAFETY: write PRC bit back to clear it; keep PR clear.
                unsafe { w32(portsc, (cur & !PORTSC_RW1C) | PORTSC_PRC) };
                let _ = writeln!(
                    serial::Writer,
                    "xhci: port {} reset done (enabled={enabled} portsc={cur:#x})",
                    n + 1,
                );
                break;
            }
            spins += 1;
            if spins >= POLL_LIMIT {
                let _ = writeln!(
                    serial::Writer,
                    "xhci: port {} reset timeout (portsc={cur:#x})",
                    n + 1,
                );
                break;
            }
        }
    }
}

/// Program MSI-X table entry 0 (interrupter 0 → `vector` on the BSP)
/// and enable MSI-X in the capability. Mirrors `nvme::program_msix_entry`
/// but resolves the table's BAR generically (xHCI may place it in a
/// different BAR than the register file).
fn program_msix_entry(bdf: pci::Bdf, vector: u8, apic_id: u8) {
    let msix = pci::msix_info(bdf.bus, bdf.dev, bdf.func)
        .expect("xhci: device advertises no MSI-X capability");

    // Interrupter 0 maps to MSI-X table entry 0.
    // SAFETY: bar_address resolves the MSI-X table's BAR for a present
    // function; map_mmio is idempotent if it overlaps the register BAR.
    let entry = unsafe {
        let table_bar_phys = pci::bar_address(bdf.bus, bdf.dev, bdf.func, msix.table_bar);
        assert!(table_bar_phys != 0, "xhci: MSI-X table BAR not assigned");
        let table_phys = table_bar_phys + msix.table_offset as u64;
        paging::map_mmio(table_phys & !0xFFF, 0x1000);
        table_phys as usize + paging::hhdm_offset() as usize
    };
    let addr_lo = 0xFEE0_0000u32 | ((apic_id as u32) << 12);
    // SAFETY: entry..entry+16 is the mapped MSI-X table entry 0.
    unsafe {
        write_volatile(entry as *mut u32, addr_lo);
        write_volatile((entry + 4) as *mut u32, 0);
        write_volatile((entry + 8) as *mut u32, vector as u32);
        write_volatile((entry + 12) as *mut u32, 0); // vector control: unmasked
    }

    // Enable MSI-X: set bit 15 of Message Control (== bit 31 of the
    // capability dword).
    // SAFETY: standard PCI config dword RMW; spec-legal per PCIe §6.8.6.
    unsafe {
        let dw = pci::config_read32(bdf.bus, bdf.dev, bdf.func, msix.cap_offset);
        pci::config_write32(
            bdf.bus,
            bdf.dev,
            bdf.func,
            msix.cap_offset,
            dw | MSIX_CTRL_ENABLE_DWORD_BIT,
        );
    }
}

/// Spin-poll a 32-bit register until `(val & mask) == expected`.
/// Panics on timeout.
///
/// # Safety
/// `reg` must be within the mapped BAR window.
unsafe fn poll_until(reg: usize, mask: u32, expected: u32, what: &str) {
    let mut spins = 0u64;
    loop {
        // SAFETY: caller's contract.
        if unsafe { r32(reg) } & mask == expected {
            return;
        }
        spins += 1;
        assert!(spins < POLL_LIMIT, "xhci: timeout waiting for {what}");
    }
}

fn find_controller() -> Option<(pci::Bdf, u64)> {
    for bus in 0u16..=255 {
        for dev in 0u8..32 {
            for func in 0u8..8 {
                if let Some(x) = check_function(bus as u8, dev, func) {
                    return Some(x);
                }
            }
        }
    }
    None
}

fn check_function(bus: u8, dev: u8, func: u8) -> Option<(pci::Bdf, u64)> {
    // SAFETY: standard PCI config reads on a probe BDF; absent
    // functions return 0xFFFF_FFFF.
    let id = unsafe { pci::config_read32(bus, dev, func, 0x00) };
    if (id & 0xFFFF) == 0xFFFF {
        return None;
    }
    let class_dword = unsafe { pci::config_read32(bus, dev, func, 0x08) };
    let class = (class_dword >> 24) as u8;
    let subclass = (class_dword >> 16) as u8;
    let prog_if = (class_dword >> 8) as u8;
    if class != XHCI_CLASS || subclass != XHCI_SUBCLASS || prog_if != XHCI_PROG_IF {
        return None;
    }
    // SAFETY: bar_address resolves BAR0 (xHCI's register BAR; 64-bit on
    // qemu-xhci) for a present function.
    let bar = unsafe { pci::bar_address(bus, dev, func, 0) };
    if bar == 0 {
        return None;
    }
    Some((pci::Bdf { bus, dev, func }, bar))
}
