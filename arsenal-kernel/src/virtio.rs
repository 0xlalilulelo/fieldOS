// SPDX-License-Identifier: BSD-2-Clause
//
// virtio modern PCI transport (virtio v1.2 § 4.1). For each PCI
// device with vendor 0x1AF4, walks the capability list at config
// offset 0x34, picks out VIRTIO_PCI_CAP_* entries (common cfg,
// notify, isr, device cfg, pci-cfg-window), resolves their BAR /
// offset / length to kernel virtual addresses through HHDM, and
// logs each.
//
// Modern only. Legacy (pre-1.0) devices used PIO-mapped registers
// in BAR 0; transitional devices expose both interfaces. Our
// driver-time acceptance of VIRTIO_F_VERSION_1 (3C-3) forces the
// modern path; this transport probe doesn't try to support legacy
// register layouts. Capability layout reference: virtio v1.2
// § 4.1.4.3 (struct virtio_pci_cap, virtio_pci_notify_cap).
//
// 3C-1 prints what it found and exits. 3C-2 builds virtqueue
// infrastructure on top of the resolved pointers; 3C-3 / 3C-4 add
// the actual blk and net drivers and read/write through the
// common-cfg pointer.

use core::fmt::Write;

use crate::paging;
use crate::pci;
use crate::serial;

const VIRTIO_VENDOR: u16 = 0x1AF4;
const PCI_CAP_ID_VENDOR: u8 = 0x09;

const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;
const VIRTIO_PCI_CAP_ISR_CFG: u8 = 3;
const VIRTIO_PCI_CAP_DEVICE_CFG: u8 = 4;
const VIRTIO_PCI_CAP_PCI_CFG: u8 = 5;

/// Walk every PCI BDF, probe virtio devices, log each resolved
/// capability. Idempotent — safe to call multiple times.
pub fn probe() {
    for bus in 0u16..=255 {
        for dev in 0u8..32 {
            probe_function(bus as u8, dev, 0);
            // Multifunction handling.
            // SAFETY: (bus, dev, 0, 0x0C) is dword-aligned; an
            // absent function-0 returned 0xFFFF earlier and we
            // would have skipped. We don't recheck here because
            // probe_function already filtered to virtio vendor.
            let header_dword =
                unsafe { pci::config_read32(bus as u8, dev, 0, 0x0C) };
            let header_type = ((header_dword >> 16) & 0xFF) as u8;
            if header_type & 0x80 != 0 {
                for func in 1u8..8 {
                    probe_function(bus as u8, dev, func);
                }
            }
        }
    }
}

fn probe_function(bus: u8, dev: u8, func: u8) {
    // SAFETY: bus / dev / func legal for PCI; offset 0 dword-aligned.
    let id = unsafe { pci::config_read32(bus, dev, func, 0x00) };
    let vendor = (id & 0xFFFF) as u16;
    if vendor != VIRTIO_VENDOR {
        return;
    }
    let device_id = ((id >> 16) & 0xFFFF) as u16;

    let _ = writeln!(
        serial::Writer,
        "virtio: probing {bus:02x}:{dev:02x}.{func} device={device_id:#06x}"
    );

    walk_caps(bus, dev, func);
}

fn walk_caps(bus: u8, dev: u8, func: u8) {
    // Status register at config offset 0x06 (high half of dword
    // at 0x04). Bit 4 indicates a capabilities list.
    // SAFETY: standard PCI config read.
    let status_dword = unsafe { pci::config_read32(bus, dev, func, 0x04) };
    let status = ((status_dword >> 16) & 0xFFFF) as u16;
    if status & 0x10 == 0 {
        let _ = writeln!(serial::Writer, "  no capabilities list");
        return;
    }

    // Caps pointer at offset 0x34 (low byte of the dword at 0x34).
    // SAFETY: standard PCI config read.
    let cap_ptr_dword = unsafe { pci::config_read32(bus, dev, func, 0x34) };
    let mut cap_offset = (cap_ptr_dword & 0xFC) as u8;

    while cap_offset != 0 {
        // SAFETY: cap pointers within the spec-mandated 0x40..0xFF
        // window of the config space. Misbehaving devices could
        // return arbitrary nexts; we don't bound-check because a
        // bad next eventually walks into 0xFF / 0x00 terminators.
        let cap_header = unsafe { pci::config_read32(bus, dev, func, cap_offset) };
        let cap_id = (cap_header & 0xFF) as u8;
        let next = ((cap_header >> 8) & 0xFC) as u8;
        let cap_len = ((cap_header >> 16) & 0xFF) as u8;
        let cfg_type = ((cap_header >> 24) & 0xFF) as u8;

        if cap_id == PCI_CAP_ID_VENDOR && cap_len >= 16 {
            print_virtio_cap(bus, dev, func, cap_offset, cfg_type);
        }

        cap_offset = next;
    }
}

fn print_virtio_cap(bus: u8, dev: u8, func: u8, cap_off: u8, cfg_type: u8) {
    // virtio_pci_cap (v1.2 § 4.1.4.3):
    //   off+0  cap_vndr|cap_next|cap_len|cfg_type   (the header dword)
    //   off+4  bar|id|padding[2]
    //   off+8  offset (LE u32, within the BAR)
    //   off+12 length (LE u32)
    // Notify caps (cfg_type=2) extend by:
    //   off+16 notify_off_multiplier (LE u32)
    //
    // SAFETY: all reads at dword-aligned offsets within the cap.
    // cap_len ≥ 16 was verified by walk_caps for the vendor cap;
    // notify reads off+16 only when cfg_type guarantees the field
    // exists.
    let bar_dword = unsafe { pci::config_read32(bus, dev, func, cap_off + 4) };
    let bar = (bar_dword & 0xFF) as u8;
    let off_in_bar = unsafe { pci::config_read32(bus, dev, func, cap_off + 8) };
    let length = unsafe { pci::config_read32(bus, dev, func, cap_off + 12) };

    let cfg_name = match cfg_type {
        VIRTIO_PCI_CAP_COMMON_CFG => "common ",
        VIRTIO_PCI_CAP_NOTIFY_CFG => "notify ",
        VIRTIO_PCI_CAP_ISR_CFG => "isr    ",
        VIRTIO_PCI_CAP_DEVICE_CFG => "device ",
        VIRTIO_PCI_CAP_PCI_CFG => "pci-cfg",
        _ => "unknown",
    };

    // SAFETY: bar must be in 0..6 for PCI; virtio devices in
    // practice use bar 0 or 4 for their MMIO region. A bogus value
    // gets a config read that returns 0 and bar_address yields a
    // zero physical address, which translates to a kernel virtual
    // address that resolves to the start of HHDM — readable but
    // meaningless. We don't dereference here; 3C-3 / 3C-4 do, and
    // they validate the common-cfg signature first.
    let bar_phys = unsafe { bar_address(bus, dev, func, bar) };
    let mmio_phys = bar_phys + off_in_bar as u64;
    let mmio_virt = mmio_phys + paging::hhdm_offset();

    let _ = writeln!(
        serial::Writer,
        "  cap {cfg_name} bar={bar} off={off_in_bar:#x} len={length:#x} -> phys={mmio_phys:#018x} virt={mmio_virt:#018x}"
    );

    if cfg_type == VIRTIO_PCI_CAP_NOTIFY_CFG {
        // SAFETY: notify caps are at least 20 bytes per spec; we
        // got cfg_type=2 from the cap header, so the field exists.
        let mult = unsafe { pci::config_read32(bus, dev, func, cap_off + 16) };
        let _ = writeln!(serial::Writer, "    notify_off_multiplier={mult}");
    }
}

/// Resolve BAR `bar` of (bus, dev, func) to a physical address.
/// Handles both 32-bit and 64-bit memory BARs. Returns 0 for I/O
/// BARs (which virtio modern doesn't use).
///
/// # Safety
/// `bar` should be 0..6; for 64-bit BARs the caller should not pass
/// an index of 5 (the upper-half BAR would read off the end of the
/// BAR window, returning 0xFFFF_FFFF, yielding a nonsense address).
unsafe fn bar_address(bus: u8, dev: u8, func: u8, bar: u8) -> u64 {
    // SAFETY: caller's contract; offset 0x10 + bar*4 is dword-aligned
    // for bar in 0..=5 and lies within the legacy config space.
    let lo = unsafe { pci::config_read32(bus, dev, func, 0x10 + bar * 4) };
    if lo & 0x01 != 0 {
        // I/O BAR — virtio modern doesn't use these. Return 0.
        return 0;
    }
    if (lo & 0x06) == 0x04 {
        // 64-bit memory BAR. Low 32 bits in this BAR (mask off type
        // bits in [3:0]); high 32 bits in the next BAR slot.
        // SAFETY: same constraints, bar+1 in range when bar < 5.
        let hi = unsafe { pci::config_read32(bus, dev, func, 0x10 + (bar + 1) * 4) };
        ((hi as u64) << 32) | ((lo & 0xFFFF_FFF0) as u64)
    } else {
        // 32-bit memory BAR.
        (lo & 0xFFFF_FFF0) as u64
    }
}
