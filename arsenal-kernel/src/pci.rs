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
    let tag = if is_virtio { " (virtio)" } else { "" };
    let _ = writeln!(
        serial::Writer,
        "pci {bus:02x}:{dev:02x}.{func} vendor={vendor:#06x} device={device:#06x} class={class_code:#04x}:{subclass:#04x}{tag}"
    );
    is_virtio
}
