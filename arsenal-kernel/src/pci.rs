// SPDX-License-Identifier: BSD-2-Clause
//
// PCI configuration-space scanner (legacy CF8/CFC). Walks every
// (bus, device, function) triple, reads the vendor/device IDs at
// config offset 0x00, prints a one-line summary per device, and
// tags virtio devices (vendor 0x1AF4) for 3C-1's transport probe.
//
// CF8/CFC works on every x86 / x86_64 chipset since 1995 — no ACPI,
// no MCFG, no ECAM dependency. ECAM (memory-mapped configuration
// access) is faster and is the only option on some PCIe-era boards,
// but adopting it requires a (light) ACPI parser to walk MCFG and
// pull the per-segment base addresses; ACPI lands post-3F when SMP
// brings MADT into the picture. CF8/CFC suffices on QEMU q35 and on
// real Framework hardware (which retains the legacy port pair).
//
// Bus 0..=255 are scanned brute force. PCIe topologies that extend
// past bus 0 do so via PCI-to-PCI bridges; the proper walk follows
// secondary-bus-number registers on each bridge. The brute force is
// trivially cheap on QEMU (microseconds under TCG) and catches
// devices on every numbered bus on real hardware too — phantom
// reads return vendor 0xFFFF and we skip them. Replace with a
// bridge-aware walk if real iron ever shows a config-space pattern
// the brute force misses.

use core::arch::asm;
use core::fmt::Write;

use crate::serial;

const CONFIG_ADDRESS: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

const VIRTIO_VENDOR: u16 = 0x1AF4;

/// PCI Capability ID for MSI-X (PCI Local Bus Spec 3.0, Appendix
/// G; PCIe Base Spec §7.7.1). 16-byte table entries; per-vector
/// mask via a Pending Bit Array (PBA).
const PCI_CAP_ID_MSIX: u8 = 0x11;

/// Status register bit 4 (within the upper half of dword 0x04) —
/// Capabilities List supported. If clear, the capability pointer at
/// offset 0x34 is reserved and the cap walk must not run.
const STATUS_CAP_LIST: u32 = 1 << 20;

/// Config-space offset of the capability pointer for header-type 0
/// devices (the only header type that matters at M1).
const CFG_CAPS_PTR: u8 = 0x34;

/// MSI-X capability layout (12 bytes total):
///   off 0    Cap ID = 0x11
///   off 1    Next Cap pointer
///   off 2-3  Message Control
///   off 4-7  Table Offset / BAR Indicator
///   off 8-11 PBA   Offset / BAR Indicator
///
/// Message Control bits:
///   0..10   Table Size minus one (so add 1 for the count)
///   14      Function Mask (one-shot mask of all vectors)
///   15      MSI-X Enable
const MSIX_CTRL_TABLE_SIZE_MASK: u16 = 0x07FF;

/// PCI Bus / Device / Function. Newtype over the (bus, dev, func)
/// triple — easier to pass around than three separate u8s, and
/// the Debug impl prints in the conventional `bb:dd.f` form.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Bdf {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
}

impl core::fmt::Debug for Bdf {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:02x}:{:02x}.{}", self.bus, self.dev, self.func)
    }
}

/// Parsed MSI-X capability for one PCIe function. The driver
/// consumes this to find the BAR + offset where the MSI-X vector
/// table and PBA live; step 1-4 maps the BAR, writes per-vector
/// Message Address / Message Data, and unmasks entries.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct MsixInfo {
    pub bdf: Bdf,
    /// Offset of the MSI-X capability header within the device's
    /// config space (so step 1-4 can write back to Message Control
    /// to set the Enable bit).
    pub cap_offset: u8,
    /// Number of MSI-X table entries (already biased to count, not
    /// the on-wire "N-1" encoding).
    pub table_size: u32,
    /// BAR number holding the MSI-X table.
    pub table_bar: u8,
    /// Byte offset within `table_bar` to the MSI-X table.
    pub table_offset: u32,
    /// BAR number holding the Pending Bit Array.
    pub pba_bar: u8,
    /// Byte offset within `pba_bar` to the PBA.
    pub pba_offset: u32,
}

/// Read a 32-bit dword from PCI config space at (bus, dev, func, offset).
///
/// # Safety
/// `offset` must be 4-byte aligned (low two bits zero) and within
/// the 256-byte legacy config space (0..0xFC). I/O ports CF8/CFC are
/// reserved for PCI configuration access on every x86 chipset; no
/// other hardware aliases them.
pub(crate) unsafe fn config_read32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    debug_assert!(dev < 32, "pci: dev must be 0..32");
    debug_assert!(func < 8, "pci: func must be 0..8");
    debug_assert_eq!(offset & 0x03, 0, "pci: offset must be dword-aligned");
    let addr = (1u32 << 31)
        | ((bus as u32) << 16)
        | ((dev as u32 & 0x1F) << 11)
        | ((func as u32 & 0x07) << 8)
        | (offset as u32 & 0xFC);
    // SAFETY: CF8/CFC are reserved PCI configuration ports. Writing
    // CF8 selects the BDF + offset; reading CFC returns the dword at
    // the selected location. Both ops are side-effect-free per the
    // PCI Local Bus Spec rev 3.0 § 3.2.2.3.2.
    unsafe {
        outl(CONFIG_ADDRESS, addr);
        inl(CONFIG_DATA)
    }
}

/// Write a 32-bit dword to x86 I/O port `port`.
///
/// # Safety
/// Caller must ensure `port` is a valid I/O port and that writing
/// `val` produces the intended hardware effect.
unsafe fn outl(port: u16, val: u32) {
    // SAFETY: caller's contract.
    unsafe {
        asm!(
            "out dx, eax",
            in("dx") port,
            in("eax") val,
            options(nomem, nostack, preserves_flags),
        );
    }
}

/// Read a 32-bit dword from x86 I/O port `port`.
///
/// # Safety
/// Caller must ensure `port` is a valid I/O port and that reading
/// it produces the intended hardware behaviour (no destructive
/// side effects).
unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    // SAFETY: caller's contract.
    unsafe {
        asm!(
            "in eax, dx",
            out("eax") val,
            in("dx") port,
            options(nomem, nostack, preserves_flags),
        );
    }
    val
}

/// Walk the entire bus / device / function space, printing each
/// present device. Returns the count of (total devices, virtio
/// devices) so the caller can sanity-check that scan ran.
pub fn scan() -> (usize, usize) {
    let mut total = 0usize;
    let mut virtio = 0usize;
    for bus in 0u16..=255 {
        for dev in 0u8..32 {
            let (added_total, added_virtio) = scan_device(bus as u8, dev);
            total += added_total;
            virtio += added_virtio;
        }
    }
    let _ = writeln!(
        serial::Writer,
        "pci: scan complete; {total} devices, {virtio} virtio"
    );
    (total, virtio)
}

fn scan_device(bus: u8, dev: u8) -> (usize, usize) {
    // SAFETY: (bus, dev, 0, 0x00) is a valid BDF + dword-aligned
    // offset; absent devices return 0xFFFF_FFFF which we filter on.
    let id = unsafe { config_read32(bus, dev, 0, 0x00) };
    let vendor = (id & 0xFFFF) as u16;
    if vendor == 0xFFFF {
        return (0, 0);
    }

    let mut total = 0usize;
    let mut virtio = 0usize;
    let func0_virtio = print_function(bus, dev, 0);
    total += 1;
    if func0_virtio {
        virtio += 1;
    }

    // Multi-function bit (bit 7 of header type at offset 0x0E).
    // Only set on function 0; if absent, functions 1..8 are wired
    // to nothing and a config read on them returns vendor 0xFFFF
    // anyway. The bit lets us short-circuit the seven extra reads
    // for the common single-function case.
    // SAFETY: same invariants as the previous read; offset 0x0C is
    // dword-aligned.
    let header_dword = unsafe { config_read32(bus, dev, 0, 0x0C) };
    let header_type = ((header_dword >> 16) & 0xFF) as u8;
    if header_type & 0x80 != 0 {
        for func in 1u8..8 {
            // SAFETY: func is in 0..8.
            let id = unsafe { config_read32(bus, dev, func, 0x00) };
            if (id & 0xFFFF) as u16 == 0xFFFF {
                continue;
            }
            let is_virtio = print_function(bus, dev, func);
            total += 1;
            if is_virtio {
                virtio += 1;
            }
        }
    }
    (total, virtio)
}

/// Print the one-line summary for a present (bus, dev, func) and
/// return whether it's a virtio device.
fn print_function(bus: u8, dev: u8, func: u8) -> bool {
    // SAFETY: caller verified the function is present (vendor !=
    // 0xFFFF). Offsets 0x00 / 0x08 are dword-aligned and within
    // the 256-byte legacy config space.
    let id = unsafe { config_read32(bus, dev, func, 0x00) };
    let class = unsafe { config_read32(bus, dev, func, 0x08) };
    let vendor = (id & 0xFFFF) as u16;
    let device = ((id >> 16) & 0xFFFF) as u16;
    let class_code = ((class >> 24) & 0xFF) as u8;
    let subclass = ((class >> 16) & 0xFF) as u8;
    let is_virtio = vendor == VIRTIO_VENDOR;
    let virtio_tag = if is_virtio { " (virtio)" } else { "" };

    // M1 step 1-0: peek the MSI-X capability for the log line so
    // drivers know at-a-glance which devices they can wire IRQs
    // against. The capability walk is a few extra config reads —
    // negligible on QEMU TCG, harmless on real hardware.
    let msix = msix_info(bus, dev, func);
    let _ = write!(
        serial::Writer,
        "pci {bus:02x}:{dev:02x}.{func} vendor={vendor:#06x} device={device:#06x} class={class_code:#04x}:{subclass:#04x}{virtio_tag}",
    );
    if let Some(info) = msix {
        let _ = write!(
            serial::Writer,
            " msix=table_size:{} bar:{} table_off:{:#x} pba_bar:{} pba_off:{:#x}",
            info.table_size,
            info.table_bar,
            info.table_offset,
            info.pba_bar,
            info.pba_offset,
        );
    }
    let _ = writeln!(serial::Writer);
    is_virtio
}

/// Resolve BAR `bar` of (bus, dev, func) to a physical address.
/// Handles both 32-bit and 64-bit memory BARs. Returns 0 for I/O
/// BARs (which M1 drivers don't use) and for absent BARs.
///
/// # Safety
/// `bar` must be in 0..=5; for 64-bit BARs the caller should not
/// pass an index of 5 (the upper-half BAR would read off the end
/// of the BAR window). The (bus, dev, func) must reference a
/// present PCI function.
pub unsafe fn bar_address(bus: u8, dev: u8, func: u8, bar: u8) -> u64 {
    debug_assert!(bar < 6, "pci: bar must be 0..6");
    // SAFETY: caller's contract; offset 0x10 + bar*4 is
    // dword-aligned for bar in 0..=5 and lies within legacy
    // config space.
    let lo = unsafe { config_read32(bus, dev, func, 0x10 + bar * 4) };
    if lo & 0x01 != 0 {
        // I/O BAR — M1 drivers use MMIO exclusively. Return 0.
        return 0;
    }
    if (lo & 0x06) == 0x04 {
        // 64-bit memory BAR. Low 32 bits here (mask off type bits
        // in [3:0]); high 32 bits in the next BAR slot.
        // SAFETY: same constraints; bar+1 in range when bar < 5.
        let hi = unsafe { config_read32(bus, dev, func, 0x10 + (bar + 1) * 4) };
        ((hi as u64) << 32) | ((lo & 0xFFFF_FFF0) as u64)
    } else {
        (lo & 0xFFFF_FFF0) as u64
    }
}

/// Walk a function's capability list and return its MSI-X
/// capability if present. Returns None for devices without MSI-X
/// (the legacy MSI capability at ID 0x05 is *not* matched — M1
/// step 1+ assumes MSI-X exclusively; legacy MSI is post-M1 if
/// any real-hardware quirk demands it).
pub fn msix_info(bus: u8, dev: u8, func: u8) -> Option<MsixInfo> {
    // SAFETY: standard PCI dword read; dev / func bounded by
    // caller's verified-present BDF.
    let status_command = unsafe { config_read32(bus, dev, func, 0x04) };
    if status_command & STATUS_CAP_LIST == 0 {
        return None;
    }

    // SAFETY: same as above; offset 0x34 is dword-aligned.
    let caps_ptr_dword = unsafe { config_read32(bus, dev, func, CFG_CAPS_PTR) };
    let mut cap_offset = (caps_ptr_dword & 0xFC) as u8;
    // Bound the walk to avoid infinite loops on malformed
    // capability lists. PCI spec allows ~48 capability entries
    // within the 256-byte legacy config space; 64 is generous.
    for _ in 0..64 {
        if cap_offset == 0 {
            return None;
        }
        debug_assert!(cap_offset >= 0x40, "pci: cap_offset {cap_offset:#x} below the 0x40 PCI capability region");
        // SAFETY: cap_offset comes from the device's capability
        // chain; dword-aligned because we masked low two bits.
        let cap_header = unsafe { config_read32(bus, dev, func, cap_offset) };
        let cap_id = (cap_header & 0xFF) as u8;
        let next = ((cap_header >> 8) & 0xFC) as u8;
        if cap_id == PCI_CAP_ID_MSIX {
            // Parse Message Control (bits 16..31 of dword 0).
            let msg_ctrl = ((cap_header >> 16) & 0xFFFF) as u16;
            let table_size = (msg_ctrl & MSIX_CTRL_TABLE_SIZE_MASK) as u32 + 1;
            // SAFETY: cap is at least 12 bytes; cap_offset + 4 / + 8
            // are dword-aligned and within legacy config space when
            // cap_offset is ≤ 0xF4.
            let table_dword =
                unsafe { config_read32(bus, dev, func, cap_offset + 4) };
            let pba_dword =
                unsafe { config_read32(bus, dev, func, cap_offset + 8) };
            return Some(MsixInfo {
                bdf: Bdf { bus, dev, func },
                cap_offset,
                table_size,
                table_bar: (table_dword & 0x7) as u8,
                table_offset: table_dword & !0x7,
                pba_bar: (pba_dword & 0x7) as u8,
                pba_offset: pba_dword & !0x7,
            });
        }
        cap_offset = next;
    }
    None
}
