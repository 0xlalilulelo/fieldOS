// SPDX-License-Identifier: BSD-2-Clause

//! M1-3 xHCI host controller — the **3-1 seed**, promoted from the
//! M1-3-0 native bring-up spike per
//! [ADR-0009](../../docs/adrs/0009-xhci-native-rust.md) (xHCI is
//! native Rust, not a LinuxKPI port).
//!
//! At its current seed state this does the xHCI 1.2 §4.2 Host
//! Controller Initialization sequence and one command-ring round-
//! trip: read the capability registers, reset, allocate the DCBAA +
//! command ring + event ring (+ ERST), run the controller, post a
//! No-Op command TRB, and observe its Command Completion Event on
//! the event ring. The 3-0 spike confirmed this is NVMe-shaped and
//! that the spec-fragile pieces (CAPLENGTH / RTSOFF / DBOFF offset
//! math, 32-vs-64-byte contexts, ring cycle bits, DCBAA, ERST) work
//! on `qemu-xhci`.
//!
//! 3-1 builds enumeration (Enable Slot → Address Device →
//! GET_DESCRIPTOR → SET_CONFIGURATION) on top, converts the polled
//! event-ring observation to MSI-X-driven completion, adds
//! `-device qemu-xhci` + the `ARSENAL_XHCI_OK` sentinel to the
//! smoke, and the HID / mass-storage class drivers follow at 3-3 /
//! 3-4. Until then `run` is a no-op when no xHCI controller is
//! present, so the production smoke (no qemu-xhci) is unaffected.

use core::fmt::Write;
use core::ptr::{read_volatile, write_volatile};

use crate::{frames, paging, pci, serial};

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

const USBCMD_RS: u32 = 1 << 0;
const USBCMD_HCRST: u32 = 1 << 1;
const USBSTS_HCH: u32 = 1 << 0;
const USBSTS_CNR: u32 = 1 << 11; // Controller Not Ready

// Interrupter 0 register set (offsets from rt_base + 0x20).
const IR0_ERSTSZ: usize = 0x08;
const IR0_ERSTBA: usize = 0x10; // 64-bit
const IR0_ERDP: usize = 0x18; // 64-bit

// TRB types.
const TRB_TYPE_NOOP_CMD: u32 = 23;
const TRB_TYPE_CMD_COMPLETION: u32 = 33;

const POLL_LIMIT: u64 = 50_000_000;

struct Regs {
    /// 3-1 reads this for the port register file (op_base + 0x400)
    /// and the extended-capability list (HCCPARAMS1 xECP).
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

/// Run the spike. Panics on any spec-violation it can detect — a
/// spike crashes loudly so the failing step is obvious in serial.
pub fn run() {
    let (bdf, bar_phys) = match find_controller() {
        Some(x) => x,
        // No xHCI controller present (the production smoke has no
        // qemu-xhci) — stay silent so the smoke serial is unaffected.
        None => return,
    };
    let _ = writeln!(serial::Writer, "xhci: === host controller bring-up (3-1 seed) ===");

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
    let max_intrs = ((hcs1 >> 8) & 0x7FF) as u16;
    let max_ports = ((hcs1 >> 24) & 0xFF) as u8;
    let scratch_hi = (hcs2 >> 21) & 0x1F;
    let scratch_lo = (hcs2 >> 27) & 0x1F;
    let max_scratch = (scratch_hi << 5) | scratch_lo;
    let csz_64 = (hcc1 & (1 << 2)) != 0; // context size: true=64B, false=32B
    let regs = Regs {
        cap_base,
        op_base: cap_base + caplength as usize,
        rt_base: cap_base + (rtsoff & !0x1F) as usize,
        db_base: cap_base + (dboff & !0x3) as usize,
    };

    let _ = writeln!(
        serial::Writer,
        "xhci: ctrl at {bdf:?} bar={bar_phys:#x} hciversion={hciversion:#06x} \
         caplength={caplength:#x}",
    );
    let _ = writeln!(
        serial::Writer,
        "xhci: max_slots={max_slots} max_intrs={max_intrs} max_ports={max_ports} \
         max_scratch={max_scratch} ctx_size={}B",
        if csz_64 { 64 } else { 32 },
    );
    let _ = writeln!(
        serial::Writer,
        "xhci: op_base=+{:#x} rt_base=+{:#x} db_base=+{:#x}",
        caplength as usize,
        rtsoff & !0x1F,
        dboff & !0x3,
    );

    // qemu-xhci reports 0 scratchpad buffers; a nonzero count would
    // need a scratchpad array in DCBAA[0] before the controller runs.
    // The spike asserts the assumption rather than silently skipping
    // a required allocation — exactly the HCSPARAMS2 trap the HANDOFF
    // flagged.
    assert_eq!(
        max_scratch, 0,
        "xhci: controller wants {max_scratch} scratchpad buffers; \
         the spike does not allocate them"
    );

    // Enable PCI bus mastering + memory space. BME is the step-2
    // carry-forward: an MSI is a bus-master write QEMU drops when BME
    // is clear, and ring DMA needs it regardless. (NVMe gets BME from
    // elsewhere; the spike sets it explicitly — the robust posture.)
    // SAFETY: standard PCI config RMW; COMMAND is at offset 0x04.
    unsafe {
        let cmd = pci::config_read32(bdf.bus, bdf.dev, bdf.func, 0x04);
        pci::config_write32(bdf.bus, bdf.dev, bdf.func, 0x04, cmd | 0x06);
    }

    // --- Reset (xHCI §4.2 step 1-3) ---
    // SAFETY: op registers within the mapped BAR.
    unsafe {
        // Halt first if running.
        let cmd = r32(regs.op_base + OP_USBCMD);
        if cmd & USBCMD_RS != 0 {
            w32(regs.op_base + OP_USBCMD, cmd & !USBCMD_RS);
            poll_until(regs.op_base + OP_USBSTS, USBSTS_HCH, USBSTS_HCH, "halt");
        }
        // Reset.
        w32(regs.op_base + OP_USBCMD, USBCMD_HCRST);
        poll_until(regs.op_base + OP_USBCMD, USBCMD_HCRST, 0, "reset-clear");
        poll_until(regs.op_base + OP_USBSTS, USBSTS_CNR, 0, "controller-ready");
    }
    let _ = writeln!(serial::Writer, "xhci: reset complete (CNR clear)");

    // --- DCBAA (§4.2 step 6) ---
    let dcbaa_phys = alloc_zeroed_frame();
    // --- Command ring: one segment, producer cycle state (PCS) = 1 ---
    let cmd_ring_phys = alloc_zeroed_frame();
    // --- Event ring segment + ERST (one entry) ---
    let event_ring_phys = alloc_zeroed_frame();
    let erst_phys = alloc_zeroed_frame();
    // ERST[0] = { ring segment base, size=16 TRBs }.
    // SAFETY: erst frame is owned + HHDM-mapped.
    unsafe {
        let erst = phys_to_virt(erst_phys) as *mut u32;
        write_volatile(erst, event_ring_phys as u32);
        write_volatile(erst.add(1), (event_ring_phys >> 32) as u32);
        write_volatile(erst.add(2), 16); // segment size in TRBs
        write_volatile(erst.add(3), 0);
    }

    // --- Program operational + interrupter registers (§4.2 step 5-9) ---
    // SAFETY: all offsets are within the mapped BAR; values per spec.
    unsafe {
        // CONFIG.MaxSlotsEn
        w32(regs.op_base + OP_CONFIG, max_slots as u32);
        // DCBAAP
        w64(regs.op_base + OP_DCBAAP, dcbaa_phys);
        // CRCR = cmd ring base | RCS(bit0)=1
        w64(regs.op_base + OP_CRCR, cmd_ring_phys | 1);
        // Interrupter 0: ERSTSZ=1, ERSTBA, ERDP (point at first event TRB)
        let ir0 = regs.rt_base + 0x20;
        w32(ir0 + IR0_ERSTSZ, 1);
        w64(ir0 + IR0_ERDP, event_ring_phys);
        w64(ir0 + IR0_ERSTBA, erst_phys);
    }

    // --- Run (§4.2 step 10) ---
    // SAFETY: op registers mapped.
    unsafe {
        let cmd = r32(regs.op_base + OP_USBCMD);
        w32(regs.op_base + OP_USBCMD, cmd | USBCMD_RS);
        poll_until(regs.op_base + OP_USBSTS, USBSTS_HCH, 0, "running");
    }
    let _ = writeln!(serial::Writer, "xhci: controller running (HCH clear)");

    // --- Post a No-Op command TRB at command-ring[0], PCS=1 ---
    // SAFETY: command ring frame owned + mapped; TRB layout per §6.4.3.1.
    unsafe {
        let trb = phys_to_virt(cmd_ring_phys) as *mut u32;
        write_volatile(trb, 0);
        write_volatile(trb.add(1), 0);
        write_volatile(trb.add(2), 0);
        // control: type[15:10] | cycle(bit0)=1
        write_volatile(trb.add(3), (TRB_TYPE_NOOP_CMD << 10) | 1);
    }
    // Ring the command doorbell (DB 0, target 0).
    // SAFETY: doorbell array within the mapped BAR.
    unsafe { w32(regs.db_base, 0) };
    let _ = writeln!(serial::Writer, "xhci: No-Op command posted, doorbell rung");

    // --- Observe the Command Completion Event on event-ring[0] ---
    // Consumer cycle state starts at 1; the controller writes the event
    // with cycle=1 when it produces it.
    // SAFETY: event ring frame owned + mapped.
    let mut spins = 0u64;
    loop {
        let ctrl = unsafe { read_volatile((phys_to_virt(event_ring_phys) as *const u32).add(3)) };
        if ctrl & 1 == 1 {
            let ev_type = (ctrl >> 10) & 0x3F;
            let status = unsafe { read_volatile((phys_to_virt(event_ring_phys) as *const u32).add(2)) };
            let cc = (status >> 24) & 0xFF;
            let _ = writeln!(
                serial::Writer,
                "xhci: event ring TRB type={ev_type} completion_code={cc} (spins={spins})",
            );
            if ev_type == TRB_TYPE_CMD_COMPLETION && cc == 1 {
                let _ = writeln!(
                    serial::Writer,
                    "xhci: command-ring No-Op round-trip OK (HC bring-up validated)",
                );
            } else {
                let _ = writeln!(
                    serial::Writer,
                    "xhci: UNEXPECTED — type/cc not the success No-Op completion",
                );
            }
            break;
        }
        spins += 1;
        if spins >= POLL_LIMIT {
            let _ = writeln!(
                serial::Writer,
                "xhci: TIMEOUT — no event-ring completion after {spins} spins",
            );
            // Dump some state for diagnosis.
            let sts = unsafe { r32(regs.op_base + OP_USBSTS) };
            let crcr_lo = unsafe { r32(regs.op_base + OP_CRCR) };
            let _ = writeln!(serial::Writer, "xhci:   USBSTS={sts:#x} CRCR_lo={crcr_lo:#x}");
            break;
        }
    }
}

/// Spin-poll a 32-bit register until `(val & mask) == expected`.
/// Panics on timeout — a wedged controller during a spike is a
/// hard failure worth surfacing immediately.
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
