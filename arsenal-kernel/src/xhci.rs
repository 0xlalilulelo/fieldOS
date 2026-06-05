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
//! **3-2 state: device enumeration.** After the No-Op round-trip,
//! `enumerate` walks each connected, enabled root port and drives the
//! default control endpoint through Enable Slot → device + input
//! contexts (CSZ-aware) → Address Device → GET_DESCRIPTOR (device,
//! then configuration) → SET_CONFIGURATION, parsing the descriptors
//! and emitting `ARSENAL_USB_ENUM_OK`. It reuses the 3-1 command ring,
//! event-ring drain, MSI-X interrupter, and DCBAA; a per-device EP0
//! transfer ring carries the control TRBs. 3-3 / 3-4 add the HID and
//! mass-storage class drivers. `run` no-ops when no xHCI controller is
//! present.
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
const TRB_TYPE_SETUP_STAGE: u32 = 2; // control-transfer Setup Stage (§6.4.1.2.1)
const TRB_TYPE_DATA_STAGE: u32 = 3; // control-transfer Data Stage
const TRB_TYPE_STATUS_STAGE: u32 = 4; // control-transfer Status Stage
const TRB_TYPE_ENABLE_SLOT: u32 = 9; // Enable Slot command
const TRB_TYPE_ADDRESS_DEVICE: u32 = 11; // Address Device command
const TRB_TYPE_NOOP_CMD: u32 = 23;
const TRB_TYPE_TRANSFER: u32 = 32; // Transfer Event
const TRB_TYPE_CMD_COMPLETION: u32 = 33;
const TRB_TYPE_PORT_STATUS_CHANGE: u32 = 34;

// USB standard device requests (USB 2.0 §9.4) + descriptor types
// (§9.5). bmRequestType for a standard GET_DESCRIPTOR is 0x80
// (device-to-host, standard, device); SET_CONFIGURATION is 0x00.
const USB_REQ_GET_DESCRIPTOR: u8 = 6;
const USB_REQ_SET_CONFIGURATION: u8 = 9;
const USB_DESC_DEVICE: u8 = 1;
const USB_DESC_CONFIG: u8 = 2;
const USB_DESC_INTERFACE: u8 = 4;

// The default control endpoint is Device Context Index 1; the slot's
// doorbell takes the target DCI as its value.
const DCI_EP0: u32 = 1;

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
    let (caplength, hciversion, hcs1, hcs2, hcc1, dboff, rtsoff) = unsafe {
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
    // HCCPARAMS1.CSZ (bit 2): 0 → 32-byte contexts, 1 → 64-byte.
    // qemu-xhci reports 32; real controllers vary, so 3-2's context
    // layout is scaled by this rather than hardcoded.
    let ctx_size: usize = if hcc1 & (1 << 2) != 0 { 64 } else { 32 };
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
         max_scratch={max_scratch} ctx_size={ctx_size}B",
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

    // --- 3-2: device enumeration ---
    // Carry the live ring state forward. The No-Op consumed command
    // ring slot 0 (cycle still 1, no wrap), and the event-ring drain
    // above left `deq`/`ccs` at the consumer's current position with
    // ERDP already advanced there. enumerate() opens its own sti
    // window and continues from here.
    let mut xhci = Xhci {
        regs,
        dcbaa_phys,
        cmd_ring_phys,
        cmd_enqueue: 1,
        cmd_cycle: 1,
        event_ring_phys,
        event_deq: deq,
        event_ccs: ccs,
        ctx_size,
        max_ports,
    };
    xhci.enumerate();
}

/// Per-device enumeration state. Holds the default-control-endpoint
/// (EP0) transfer ring so control transfers can enqueue Setup/Data/
/// Status TRBs and advance the producer cycle bit independently of the
/// command ring. One per addressed device.
struct Device {
    slot_id: u8,
    ep0_ring_phys: u64,
    ep0_enqueue: usize,
    ep0_cycle: u32,
}

/// Live controller state carried from 3-1 bring-up into 3-2
/// enumeration: register bases, the DCBAA, and the command + event
/// ring producer/consumer positions. `run` builds this after the
/// No-Op round-trip and hands it to `enumerate`.
struct Xhci {
    regs: Regs,
    dcbaa_phys: u64,
    cmd_ring_phys: u64,
    /// Next command-ring slot to write; cycle bit OR'd in on push.
    cmd_enqueue: usize,
    cmd_cycle: u32,
    event_ring_phys: u64,
    /// Consumer dequeue index + cycle state, shared across the No-Op
    /// drain and every enumeration event.
    event_deq: usize,
    event_ccs: u32,
    /// 32 or 64 (HCCPARAMS1.CSZ) — scales the slot/endpoint context
    /// stride inside the input + device contexts.
    ctx_size: usize,
    max_ports: u8,
}

impl Xhci {
    /// Enqueue a command TRB (low three dwords + the type/flags dword,
    /// without the cycle bit — OR'd in here) on the command ring and
    /// ring command doorbell 0. M1 issues a handful of commands total,
    /// so we never approach the 256-TRB ring end; no link TRB is placed
    /// and the bound is asserted.
    fn push_command(&mut self, d0: u32, d1: u32, d2: u32, d3_type_flags: u32) {
        assert!(
            self.cmd_enqueue < 255,
            "xhci: command ring near wrap (no link TRB placed at M1)"
        );
        // SAFETY: command-ring frame owned + HHDM-mapped; enqueue < 255.
        unsafe {
            let trb = (phys_to_virt(self.cmd_ring_phys) as *mut u32).add(self.cmd_enqueue * 4);
            write_volatile(trb, d0);
            write_volatile(trb.add(1), d1);
            write_volatile(trb.add(2), d2);
            write_volatile(trb.add(3), d3_type_flags | self.cmd_cycle);
        }
        self.cmd_enqueue += 1;
        // SAFETY: doorbell array entry 0 (command ring) within the BAR.
        unsafe { w32(self.regs.db_base, 0) };
    }

    /// Consume the next event-ring TRB, advancing the consumer cycle
    /// state and ERDP. Spins until a TRB with the expected cycle bit
    /// appears (the caller must have interrupts enabled so the MSI-X
    /// keeps the controller posting). Returns (ev_type, completion_code,
    /// d0, d1, d3).
    fn next_event(&mut self) -> (u32, u32, u32, u32, u32) {
        let mut spins = 0u64;
        loop {
            // SAFETY: event_deq < EVENT_RING_TRBS; ring frame owned + mapped.
            let (d0, d1, d2, d3) = unsafe {
                let p = (phys_to_virt(self.event_ring_phys) as *const u32).add(self.event_deq * 4);
                (
                    read_volatile(p),
                    read_volatile(p.add(1)),
                    read_volatile(p.add(2)),
                    read_volatile(p.add(3)),
                )
            };
            if (d3 & 1) != self.event_ccs {
                spins += 1;
                assert!(spins < POLL_LIMIT, "xhci: expected event never arrived (wiring?)");
                core::hint::spin_loop();
                continue;
            }
            let ev_type = (d3 >> 10) & 0x3F;
            let cc = (d2 >> 24) & 0xFF;
            self.event_deq += 1;
            if self.event_deq == EVENT_RING_TRBS {
                self.event_deq = 0;
                self.event_ccs ^= 1;
            }
            // Advance ERDP (clears EHB) and clear IMAN.IP (keeping IE) so
            // the controller can re-assert the interrupt for the next batch.
            // SAFETY: interrupter 0 registers within the mapped BAR.
            unsafe {
                let ir0 = self.regs.rt_base + 0x20;
                w64(
                    ir0 + IR0_ERDP,
                    (self.event_ring_phys + self.event_deq as u64 * TRB_BYTES) | ERDP_EHB,
                );
                w32(ir0 + IR0_IMAN, IMAN_IE | IMAN_IP);
            }
            return (ev_type, cc, d0, d1, d3);
        }
    }

    /// Wait for a Command Completion Event, draining (and logging) any
    /// interleaved Port Status Change or other events. Returns
    /// (completion_code, slot_id).
    fn wait_command(&mut self) -> (u32, u32) {
        loop {
            let (ev_type, cc, _d0, _d1, d3) = self.next_event();
            match ev_type {
                TRB_TYPE_CMD_COMPLETION => return (cc, (d3 >> 24) & 0xFF),
                TRB_TYPE_PORT_STATUS_CHANGE => {
                    let _ = writeln!(serial::Writer, "xhci: drained port-status-change event");
                }
                other => {
                    let _ = writeln!(serial::Writer, "xhci: drained event type={other} (cc={cc})");
                }
            }
        }
    }

    /// Wait for a Transfer Event (the IOC-flagged Status Stage of a
    /// control transfer). Returns its completion code.
    fn wait_transfer(&mut self) -> u32 {
        loop {
            let (ev_type, cc, ..) = self.next_event();
            if ev_type == TRB_TYPE_TRANSFER {
                return cc;
            }
            let _ = writeln!(serial::Writer, "xhci: drained event type={ev_type} awaiting transfer");
        }
    }

    /// Enqueue one TRB on a device's EP0 transfer ring (cycle bit OR'd
    /// in). As with the command ring, M1 stays far from the 256-TRB end,
    /// so no link TRB is placed.
    fn push_transfer(&self, dev: &mut Device, d0: u32, d1: u32, d2: u32, d3_type_flags: u32) {
        assert!(
            dev.ep0_enqueue < 255,
            "xhci: EP0 ring near wrap (no link TRB placed at M1)"
        );
        // SAFETY: EP0 ring frame owned + HHDM-mapped; enqueue < 255.
        unsafe {
            let trb = (phys_to_virt(dev.ep0_ring_phys) as *mut u32).add(dev.ep0_enqueue * 4);
            write_volatile(trb, d0);
            write_volatile(trb.add(1), d1);
            write_volatile(trb.add(2), d2);
            write_volatile(trb.add(3), d3_type_flags | dev.ep0_cycle);
        }
        dev.ep0_enqueue += 1;
    }

    /// Ring a device's slot doorbell targeting EP0 (DCI 1).
    fn ring_ep0(&self, dev: &Device) {
        // SAFETY: slot doorbell (db_base + slot*4) within the mapped BAR.
        unsafe { w32(self.regs.db_base + dev.slot_id as usize * 4, DCI_EP0) };
    }

    /// Control IN transfer on EP0: Setup (IN data stage) → Data (IN) →
    /// Status (OUT, IOC). Reads `length` bytes into `buf_phys`. Returns
    /// the Status Stage completion code.
    // A control IN names the four SETUP-packet fields plus the data
    // buffer + length; typing each is clearer than a builder at 3-2's
    // three call sites.
    #[allow(clippy::too_many_arguments)]
    fn control_in(
        &mut self,
        dev: &mut Device,
        bm_request_type: u8,
        b_request: u8,
        w_value: u16,
        w_index: u16,
        buf_phys: u64,
        length: u16,
    ) -> u32 {
        let setup_d0 = (bm_request_type as u32)
            | ((b_request as u32) << 8)
            | ((w_value as u32) << 16);
        let setup_d1 = (w_index as u32) | ((length as u32) << 16);
        // Setup Stage: 8-byte immediate data (IDT), TRT=3 (IN data).
        self.push_transfer(
            dev,
            setup_d0,
            setup_d1,
            8,
            (3 << 16) | (1 << 6) | (TRB_TYPE_SETUP_STAGE << 10),
        );
        // Data Stage: DIR=IN (bit 16).
        self.push_transfer(
            dev,
            buf_phys as u32,
            (buf_phys >> 32) as u32,
            length as u32,
            (1 << 16) | (TRB_TYPE_DATA_STAGE << 10),
        );
        // Status Stage: DIR=OUT (0, opposite of an IN data stage), IOC.
        self.push_transfer(dev, 0, 0, 0, (1 << 5) | (TRB_TYPE_STATUS_STAGE << 10));
        self.ring_ep0(dev);
        self.wait_transfer()
    }

    /// Control transfer with no data stage (e.g. SET_CONFIGURATION):
    /// Setup (TRT=0) → Status (IN, IOC). Returns the completion code.
    fn control_no_data(
        &mut self,
        dev: &mut Device,
        bm_request_type: u8,
        b_request: u8,
        w_value: u16,
        w_index: u16,
    ) -> u32 {
        let setup_d0 = (bm_request_type as u32)
            | ((b_request as u32) << 8)
            | ((w_value as u32) << 16);
        let setup_d1 = w_index as u32; // wLength = 0
        // Setup Stage: TRT=0 (no data stage), IDT.
        self.push_transfer(dev, setup_d0, setup_d1, 8, (1 << 6) | (TRB_TYPE_SETUP_STAGE << 10));
        // Status Stage: DIR=IN (bit 16, the no-data case), IOC.
        self.push_transfer(dev, 0, 0, 0, (1 << 16) | (1 << 5) | (TRB_TYPE_STATUS_STAGE << 10));
        self.ring_ep0(dev);
        self.wait_transfer()
    }

    /// Enumerate every connected, enabled root port. Opens one sti
    /// window for the whole phase (same rationale as the No-Op window:
    /// the runqueue is empty so a timer preempt is a no-op, and the MSI
    /// keeps the controller's event delivery flowing). Emits
    /// `ARSENAL_USB_ENUM_OK` once at least one device's descriptors read
    /// back and it accepts SET_CONFIGURATION.
    fn enumerate(&mut self) {
        let _ = writeln!(serial::Writer, "xhci: === device enumeration ===");
        // SAFETY: sti/cli are ring-0 privileged; IF toggle only.
        unsafe { core::arch::asm!("sti", options(nomem, nostack)) };
        let mut enumerated = 0u32;
        for n in 0..self.max_ports {
            let portsc = self.regs.op_base + OP_PORTS_BASE + n as usize * PORT_REG_STRIDE;
            // SAFETY: PORTSC[n] within the mapped operational window.
            let sc = unsafe { r32(portsc) };
            if sc & PORTSC_CCS == 0 || sc & PORTSC_PED == 0 {
                continue;
            }
            let speed = (sc >> 10) & 0xF;
            if self.enumerate_port(n + 1, speed) {
                enumerated += 1;
            }
        }
        // SAFETY: see above.
        unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
        if enumerated > 0 {
            serial::write_str("ARSENAL_USB_ENUM_OK\n");
        } else {
            let _ = writeln!(serial::Writer, "xhci: no devices enumerated");
        }
    }

    /// Enumerate one connected, enabled port: Enable Slot → device +
    /// input contexts → Address Device → GET_DESCRIPTOR (device, config)
    /// → SET_CONFIGURATION. Returns true on a fully configured device.
    /// Logs and returns false on the first command/transfer that fails.
    fn enumerate_port(&mut self, port_num: u8, speed: u32) -> bool {
        // 1. Enable Slot (slot type 0 = USB2/USB3).
        self.push_command(0, 0, 0, TRB_TYPE_ENABLE_SLOT << 10);
        let (cc, slot_id) = self.wait_command();
        if cc != 1 {
            let _ = writeln!(serial::Writer, "xhci: port {port_num} Enable Slot cc={cc}");
            return false;
        }

        // 2. Allocate the output device context; install it in DCBAA[slot].
        let dev_ctx_phys = alloc_zeroed_frame();
        // SAFETY: DCBAA frame owned + mapped; slot_id <= max_slots < 512.
        unsafe {
            let dcbaa = (phys_to_virt(self.dcbaa_phys) as *mut u64).add(slot_id as usize);
            write_volatile(dcbaa, dev_ctx_phys);
        }

        // 3. Build the input context (Input Control + Slot + EP0) and the
        // EP0 transfer ring. Context blocks are ctx_size-strided.
        let input_ctx_phys = alloc_zeroed_frame();
        let ep0_ring_phys = alloc_zeroed_frame();
        // EP0 max packet size by port speed (xHCI §4.3, USB defaults):
        // SuperSpeed 512, High 64, Full/Low 8.
        let max_packet: u32 = match speed {
            4 => 512,
            3 => 64,
            _ => 8,
        };
        // SAFETY: input-context frame owned + mapped; all offsets within
        // 4 KiB for ctx_size ∈ {32, 64} and three context blocks.
        unsafe {
            let base = phys_to_virt(input_ctx_phys);
            // Input Control Context: Add Flags (dword1) = A0 (slot) | A1 (EP0).
            write_volatile((base + 4) as *mut u32, 0b11);
            // Slot Context: Context Entries = 1 (highest DCI), speed; root
            // hub port number (1-based) in dword1 bits 16..23.
            let slot = base + self.ctx_size;
            write_volatile(slot as *mut u32, (1 << 27) | (speed << 20));
            write_volatile((slot + 4) as *mut u32, (port_num as u32) << 16);
            // EP0 Context: dword1 = MaxPacketSize | EP Type 4 (Control) |
            // CErr 3; dword2/3 = TR Dequeue Pointer | DCS=1; dword4 = avg
            // TRB length 8.
            let ep0 = base + 2 * self.ctx_size;
            write_volatile((ep0 + 4) as *mut u32, (max_packet << 16) | (4 << 3) | (3 << 1));
            write_volatile((ep0 + 8) as *mut u32, (ep0_ring_phys as u32) | 1);
            write_volatile((ep0 + 12) as *mut u32, (ep0_ring_phys >> 32) as u32);
            write_volatile((ep0 + 16) as *mut u32, 8);
        }

        // 4. Address Device (BSR=0: also issues SET_ADDRESS on the bus —
        // qemu-xhci handles the single-phase form; the BSR=1 two-phase is
        // a real-hardware path some full-speed devices need, deferred to
        // step 7 if it bites).
        self.push_command(
            input_ctx_phys as u32,
            (input_ctx_phys >> 32) as u32,
            0,
            (slot_id << 24) | (TRB_TYPE_ADDRESS_DEVICE << 10),
        );
        let (cc, _sid) = self.wait_command();
        if cc != 1 {
            let _ = writeln!(serial::Writer, "xhci: port {port_num} Address Device cc={cc}");
            return false;
        }

        let mut dev = Device {
            slot_id: slot_id as u8,
            ep0_ring_phys,
            ep0_enqueue: 0,
            ep0_cycle: 1,
        };

        // 5. GET_DESCRIPTOR(device, 18 bytes).
        let buf_phys = alloc_zeroed_frame();
        let buf_virt = phys_to_virt(buf_phys);
        let cc = self.control_in(
            &mut dev,
            0x80,
            USB_REQ_GET_DESCRIPTOR,
            (USB_DESC_DEVICE as u16) << 8,
            0,
            buf_phys,
            18,
        );
        if cc != 1 {
            let _ = writeln!(serial::Writer, "xhci: slot {slot_id} GET_DESCRIPTOR(device) cc={cc}");
            return false;
        }
        // SAFETY: buf holds at least 18 valid bytes of the device descriptor.
        let dd = unsafe { core::slice::from_raw_parts(buf_virt as *const u8, 18) };
        let bcd_usb = u16::from_le_bytes([dd[2], dd[3]]);
        let dev_class = dd[4];
        let max_pkt0 = dd[7];
        let vid = u16::from_le_bytes([dd[8], dd[9]]);
        let pid = u16::from_le_bytes([dd[10], dd[11]]);
        let num_cfg = dd[17];
        let _ = writeln!(
            serial::Writer,
            "xhci: slot {slot_id} dev-desc usb={bcd_usb:#06x} class={dev_class:#04x} \
             maxpkt0={max_pkt0} vid={vid:#06x} pid={pid:#06x} ncfg={num_cfg}",
        );

        // 6. GET_DESCRIPTOR(config, 9 bytes) for wTotalLength + value.
        // SAFETY: buf frame owned + mapped; zero the header region first.
        unsafe { core::ptr::write_bytes(buf_virt as *mut u8, 0, 256) };
        let cc = self.control_in(
            &mut dev,
            0x80,
            USB_REQ_GET_DESCRIPTOR,
            (USB_DESC_CONFIG as u16) << 8,
            0,
            buf_phys,
            9,
        );
        if cc != 1 {
            let _ = writeln!(serial::Writer, "xhci: slot {slot_id} GET_DESCRIPTOR(config hdr) cc={cc}");
            return false;
        }
        // SAFETY: buf holds 9 valid bytes of the configuration descriptor.
        let ch = unsafe { core::slice::from_raw_parts(buf_virt as *const u8, 9) };
        let total_len = u16::from_le_bytes([ch[2], ch[3]]);
        let config_value = ch[5];

        // 7. GET_DESCRIPTOR(config, wTotalLength) full → first interface.
        let full = total_len.min(256);
        // SAFETY: buf frame owned + mapped.
        unsafe { core::ptr::write_bytes(buf_virt as *mut u8, 0, 256) };
        let cc = self.control_in(
            &mut dev,
            0x80,
            USB_REQ_GET_DESCRIPTOR,
            (USB_DESC_CONFIG as u16) << 8,
            0,
            buf_phys,
            full,
        );
        if cc != 1 {
            let _ = writeln!(serial::Writer, "xhci: slot {slot_id} GET_DESCRIPTOR(config full) cc={cc}");
            return false;
        }
        // SAFETY: buf holds `full` valid bytes of the configuration tree.
        let cfg = unsafe { core::slice::from_raw_parts(buf_virt as *const u8, full as usize) };
        let (iface_class, iface_sub, iface_proto, num_ep) = first_interface(cfg);

        // 8. SET_CONFIGURATION(config_value): device enters configured state.
        let cc = self.control_no_data(
            &mut dev,
            0x00,
            USB_REQ_SET_CONFIGURATION,
            config_value as u16,
            0,
        );
        if cc != 1 {
            let _ = writeln!(serial::Writer, "xhci: slot {slot_id} SET_CONFIGURATION cc={cc}");
            return false;
        }

        let _ = writeln!(
            serial::Writer,
            "xhci: slot {slot_id} configured (value={config_value} iface class={iface_class:#04x}/\
             {iface_sub:#04x}/{iface_proto:#04x} eps={num_ep})",
        );
        true
    }
}

/// Walk a configuration descriptor's tree and return the first
/// interface descriptor's (class, subclass, protocol, bNumEndpoints).
/// Returns zeros if no interface descriptor is present.
fn first_interface(cfg: &[u8]) -> (u8, u8, u8, u8) {
    let mut i = 0usize;
    while i + 2 <= cfg.len() {
        let len = cfg[i] as usize;
        if len < 2 || i + len > cfg.len() {
            break;
        }
        if cfg[i + 1] == USB_DESC_INTERFACE && len >= 9 {
            return (cfg[i + 5], cfg[i + 6], cfg[i + 7], cfg[i + 4]);
        }
        i += len;
    }
    (0, 0, 0, 0)
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
