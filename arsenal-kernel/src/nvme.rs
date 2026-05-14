// SPDX-License-Identifier: BSD-2-Clause
//
// NVMe driver — M1 step 1. Native Rust per ARSENAL.md (~5K LOC
// ceiling; target ~600-800 LOC at step exit). No LinuxKPI shim
// dependency. The first M1 driver and the first to exercise PCIe
// MSI-X paths every later driver (xHCI, virtio-gpu, amdgpu,
// iwlwifi) also needs.
//
// M1-1-1 (this commit) covers device discovery, BAR mapping, and
// controller-register read/write primitives. The Controller
// handle returned by `init` is what 1-2's reset + admin queue
// work consumes; 1-3 builds the I/O queue + first sector read on
// top of that; 1-4 converts to MSI-X interrupt-driven completion.
//
// Spec reference: NVMe 1.4 base specification, particularly §3.1
// (Register Definition), §7.6.1 (Controller Initialization), and
// §4 (Admin and NVM Command Set). The 1.4 spec is what QEMU's
// nvme device emulates by default and what every consumer SSD
// shipped in the last 5+ years implements.

use core::fmt::Write;
use core::ptr::{read_volatile, write_volatile};

use crate::paging;
use crate::pci;
use crate::serial;

/// PCI class code for NVMe controllers: 01 (mass storage) :
/// 08 (NVMe) : 02 (NVMe I/O command set).
const NVME_CLASS: u8 = 0x01;
const NVME_SUBCLASS: u8 = 0x08;
const NVME_PROG_IF: u8 = 0x02;

/// Bytes of BAR0 mapping. Spec mandates controller registers at
/// offsets 0x00..=0x1000 plus per-queue doorbells starting at
/// 0x1000; with default DSTRD=0 (4-byte stride) and 32 queue
/// pairs, doorbells occupy 0x1000..=0x1100. Map 16 KiB to cover
/// that plus the optional Controller Memory Buffer header
/// registers up to ~0x4000. 1-2 widens this if CAP.DSTRD or a
/// large queue count demands more.
const BAR0_MAP_SIZE: u64 = 0x4000;

/// Spec register offsets (NVMe 1.4 §3.1, Table 27).
#[allow(dead_code)]
pub const REG_CAP: usize = 0x0000; // 64-bit (controller capabilities)
#[allow(dead_code)]
pub const REG_VS: usize = 0x0008; // 32-bit (version)
#[allow(dead_code)]
pub const REG_CC: usize = 0x0014; // 32-bit (controller configuration)
#[allow(dead_code)]
pub const REG_CSTS: usize = 0x001C; // 32-bit (controller status)
#[allow(dead_code)]
pub const REG_AQA: usize = 0x0024; // 32-bit (admin queue attributes)
#[allow(dead_code)]
pub const REG_ASQ: usize = 0x0028; // 64-bit (admin submission queue base)
#[allow(dead_code)]
pub const REG_ACQ: usize = 0x0030; // 64-bit (admin completion queue base)
#[allow(dead_code)]
pub const DOORBELL_BASE: usize = 0x1000;

// CAP register field accessors (NVMe 1.4 §3.1.1, Table 28).
fn cap_mqes(cap: u64) -> u16 {
    (cap & 0xFFFF) as u16
}
fn cap_dstrd(cap: u64) -> u8 {
    ((cap >> 32) & 0xF) as u8
}
fn cap_css(cap: u64) -> u8 {
    ((cap >> 37) & 0xFF) as u8
}
fn cap_mpsmin(cap: u64) -> u8 {
    ((cap >> 48) & 0xF) as u8
}
fn cap_mpsmax(cap: u64) -> u8 {
    ((cap >> 52) & 0xF) as u8
}

/// Resolved NVMe controller handle. Step 1-2 consumes this for
/// the reset + admin-queue bring-up; 1-3 uses it for the I/O
/// queue; 1-4 for MSI-X. The fields above bar0_phys are read-only
/// snapshots captured at init time so cooperative-context code
/// doesn't need to re-issue the MMIO read for properties that
/// can't change at runtime (CAP and VS are spec-immutable).
#[allow(dead_code)]
pub struct Controller {
    pub bdf: pci::Bdf,
    pub bar0_phys: u64,
    pub bar0_virt: usize,
    pub cap: u64,
    pub version: u32,
    pub doorbell_stride: u8,
}

#[allow(dead_code)]
impl Controller {
    /// Read a 32-bit MMIO register at offset `reg`.
    ///
    /// # Safety
    /// `reg` must be within the BAR0 region mapped by `init`
    /// (0..BAR0_MAP_SIZE) and 4-byte aligned. The 64-bit CAP at
    /// offset 0 should use `read64`; reading it as two 32-bit
    /// halves under-the-hood works but loses the
    /// natural-alignment atomicity the spec relies on.
    pub unsafe fn read32(&self, reg: usize) -> u32 {
        // SAFETY: caller asserts reg is within the mapped BAR0;
        // bar0_virt comes from paging::map_mmio at init.
        unsafe { read_volatile((self.bar0_virt + reg) as *const u32) }
    }

    /// Read a 64-bit MMIO register at offset `reg`. Required for
    /// CAP (offset 0) — the spec specifies an atomic 64-bit read.
    ///
    /// # Safety
    /// As `read32`, plus `reg` must be 8-byte aligned and the
    /// register must be defined as 64-bit by the spec.
    pub unsafe fn read64(&self, reg: usize) -> u64 {
        // SAFETY: as read32.
        unsafe { read_volatile((self.bar0_virt + reg) as *const u64) }
    }

    /// Write a 32-bit MMIO register at offset `reg`.
    ///
    /// # Safety
    /// As `read32`, plus the value must be a spec-legal bit
    /// pattern for the register; writes to CC, AQA, ASQ, ACQ,
    /// and the doorbells have hardware side effects.
    pub unsafe fn write32(&self, reg: usize, val: u32) {
        // SAFETY: caller's contract.
        unsafe { write_volatile((self.bar0_virt + reg) as *mut u32, val) };
    }

    /// Write a 64-bit MMIO register at offset `reg`.
    ///
    /// # Safety
    /// As `write32`, plus `reg` must be 8-byte aligned.
    pub unsafe fn write64(&self, reg: usize, val: u64) {
        // SAFETY: caller's contract.
        unsafe { write_volatile((self.bar0_virt + reg) as *mut u64, val) };
    }
}

/// Find the first NVMe controller and probe it. Maps BAR0 via
/// paging::map_mmio, reads CAP / VS, asserts the spec features
/// M1 step 1 relies on, and logs a one-line summary. Returns a
/// Controller handle ready for 1-2's reset + admin-queue work.
///
/// Panics if no NVMe controller is found — M1 step 1 assumes
/// exactly one, attached via QEMU's `-device nvme` (or a real
/// Framework 13 AMD's onboard NVMe at step 7).
pub fn init() -> Controller {
    let (bdf, bar0_phys) = find_controller().expect(
        "nvme: no controller found — pass -device nvme,drive=nvme0 to QEMU",
    );

    // Limine's HHDM doesn't cover device MMIO; map the BAR before
    // the first dereference. Same chokepoint as 3C virtio BARs,
    // 3F LAPIC MMIO, 4-0 ACPI tables, 4-3 IOAPIC MMIO.
    paging::map_mmio(bar0_phys, BAR0_MAP_SIZE);
    let bar0_virt = bar0_phys as usize + paging::hhdm_offset() as usize;

    // SAFETY: BAR0 is mapped above; CAP at offset 0 is 64-bit and
    // 8-byte aligned (the BAR base is page-aligned); VS at offset
    // 8 is 32-bit and 4-byte aligned.
    let cap = unsafe { read_volatile(bar0_virt as *const u64) };
    let version = unsafe { read_volatile((bar0_virt + REG_VS) as *const u32) };

    let mqes = cap_mqes(cap);
    let dstrd = cap_dstrd(cap);
    let css = cap_css(cap);
    let mpsmin = cap_mpsmin(cap);
    let mpsmax = cap_mpsmax(cap);

    let v_major = (version >> 16) as u16;
    let v_minor = ((version >> 8) & 0xFF) as u8;
    let v_tertiary = (version & 0xFF) as u8;

    let _ = writeln!(
        serial::Writer,
        "nvme: controller at {bdf:?} bar0={bar0_phys:#018x} \
         version={v_major}.{v_minor}.{v_tertiary} cap={cap:#018x}",
    );
    let _ = writeln!(
        serial::Writer,
        "nvme: cap.mqes={mqes} (max queue entries {}) \
         cap.dstrd={dstrd} (doorbell stride {} bytes) \
         cap.css={css:#04x} cap.mpsmin={mpsmin} (min host page {} bytes) \
         cap.mpsmax={mpsmax}",
        mqes as u32 + 1,
        4u32 << dstrd,
        1u32 << (mpsmin + 12),
    );

    // M1 step 1 uses 4-KiB host pages exclusively; if CAP.MPSMIN
    // demands a larger minimum we'd need to re-think the frame
    // allocator's 4-KiB unit. Real-hardware controllers report
    // MPSMIN = 0 (4 KiB) universally; the assert protects against
    // a future virtual / hypothetical-hardware quirk.
    assert!(
        mpsmin <= 12,
        "nvme: CAP.MPSMIN={mpsmin} demands host page size > 4 KiB; \
         M1 step 1 uses 4-KiB pages exclusively"
    );
    // M1 step 1 uses the NVM command set; CAP.CSS bit 0 advertises
    // its support. Newer controllers may add ZNS (bit 1) or admin-
    // only (bit 7) modes; we don't consume those.
    assert!(
        css & 0x01 != 0,
        "nvme: CAP.CSS bit 0 (NVM command set) not advertised; \
         M1 step 1 uses NVM exclusively"
    );

    Controller {
        bdf,
        bar0_phys,
        bar0_virt,
        cap,
        version,
        doorbell_stride: dstrd,
    }
}

/// Scan the PCI bus for the first NVMe controller and return its
/// BDF + BAR0 physical address. Returns None if no NVMe device is
/// present. Walks the same brute-force bus/dev/func space pci::scan
/// uses; class-code match means we don't need a vendor allowlist
/// (every NVMe spec-compliant controller reports 01:08:02).
fn find_controller() -> Option<(pci::Bdf, u64)> {
    for bus in 0u16..=255 {
        for dev in 0u8..32 {
            if let Some(found) = check_function(bus as u8, dev, 0) {
                return Some(found);
            }
            // SAFETY: standard PCI dword read at the
            // multi-function header offset.
            let header_dword = unsafe { pci::config_read32(bus as u8, dev, 0, 0x0C) };
            if (header_dword >> 16) & 0x80 != 0 {
                for func in 1u8..8 {
                    if let Some(found) = check_function(bus as u8, dev, func) {
                        return Some(found);
                    }
                }
            }
        }
    }
    None
}

fn check_function(bus: u8, dev: u8, func: u8) -> Option<(pci::Bdf, u64)> {
    // SAFETY: standard PCI dword reads at dword-aligned offsets;
    // 0xFFFF vendor short-circuits absent functions.
    let id = unsafe { pci::config_read32(bus, dev, func, 0x00) };
    if (id & 0xFFFF) as u16 == 0xFFFF {
        return None;
    }
    let class_dword = unsafe { pci::config_read32(bus, dev, func, 0x08) };
    let class_code = ((class_dword >> 24) & 0xFF) as u8;
    let subclass = ((class_dword >> 16) & 0xFF) as u8;
    let prog_if = ((class_dword >> 8) & 0xFF) as u8;
    if class_code != NVME_CLASS || subclass != NVME_SUBCLASS || prog_if != NVME_PROG_IF
    {
        return None;
    }
    // SAFETY: BAR 0 is in range; NVMe specs that BAR0 holds the
    // 64-bit MMIO controller-register region.
    let bar0_phys = unsafe { pci::bar_address(bus, dev, func, 0) };
    if bar0_phys == 0 {
        return None;
    }
    Some((pci::Bdf { bus, dev, func }, bar0_phys))
}
