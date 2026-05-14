// SPDX-License-Identifier: BSD-2-Clause
//
// IOAPIC bring-up — M0 step 4-3. Maps the IOAPIC MMIO that 4-0's
// ACPI MADT walker discovered, reads the version register to learn
// the redirection-table size, and masks every entry so no ISA / PCI
// IRQ delivers until 4-5 explicitly unmasks IRQ1 for the keyboard.
//
// IOAPIC access is indirect: write the register index to IOREGSEL
// (offset 0x00), then read or write the 32-bit data at IOWIN
// (offset 0x10). Each redirection-table entry is 64 bits split
// across two consecutive registers (low half = 0x10 + 2N,
// high half = 0x11 + 2N for entry N). The two-step access is not
// atomic at the bus level, so multi-core callers must serialize
// through the module-level lock; 4-3 only touches the IOAPIC from
// BSP boot, but ioapic::program at 4-5+ runs from arbitrary
// context and the lock is the prep for that.
//
// Permanently out of scope for M0: multiple IOAPICs (one is the
// QEMU q35 / commodity-x86_64 default and matches the 4-0 MADT
// reading of 1 IOAPIC entry; M1 / M2 revisits if Framework or
// Asahi hardware surfaces multi-IOAPIC topologies).

use core::fmt::Write;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use spin::Mutex;

use crate::acpi;
use crate::paging;
use crate::serial;

const IOREGSEL: usize = 0x00;
const IOWIN: usize = 0x10;

/// IOAPIC version register. Bits 16..23 = max redirection-entry
/// index (zero-based; add 1 for entry count).
const IOAPIC_REG_VER: u32 = 0x01;

/// Base register index for the redirection table. Entry N occupies
/// IOREDTBL_BASE + 2N (low half) and IOREDTBL_BASE + 2N + 1 (high).
const IOREDTBL_BASE: u32 = 0x10;

/// Redirection-table entry low-half bit 16 — interrupt is masked.
const IOREDTBL_MASKED: u32 = 1 << 16;

static IOAPIC_BASE_VIRT: AtomicUsize = AtomicUsize::new(0);
static IOAPIC_GSI_BASE: AtomicU32 = AtomicU32::new(0);
static IOAPIC_REDIR_COUNT: AtomicU32 = AtomicU32::new(0);

/// Serializes the IOREGSEL → IOWIN pair across CPUs / preemption.
/// Held only for the duration of one register access; the IRQ-safe
/// version arrives at 4-4 if hard preemption forces it.
static IOAPIC_LOCK: Mutex<()> = Mutex::new(());

/// Map the first IOAPIC's MMIO page, read its version register,
/// stash the redirection-table size, and mask every entry. Emits
/// ARSENAL_IOAPIC_OK on success. M0 assumes exactly one IOAPIC
/// (QEMU q35 default; the MADT walker at 4-0 confirms).
pub fn init() {
    let ioapics = acpi::ioapics();
    assert!(
        !ioapics.is_empty(),
        "ioapic: ACPI MADT reported zero I/O APICs — bring-up cannot proceed",
    );
    let info = ioapics[0];
    let phys = info.base as u64;

    paging::map_mmio(phys, 0x1000);
    let virt = phys as usize + paging::hhdm_offset() as usize;

    IOAPIC_BASE_VIRT.store(virt, Ordering::Relaxed);
    IOAPIC_GSI_BASE.store(info.gsi_base, Ordering::Relaxed);

    let ver = unsafe { read_unlocked(IOAPIC_REG_VER) };
    let max_redir = (ver >> 16) & 0xFF;
    let entry_count = max_redir + 1;
    IOAPIC_REDIR_COUNT.store(entry_count, Ordering::Relaxed);

    // Mask every entry. Set bit 16 (MASK) in the low half; leave
    // the high half / destination at whatever the firmware programmed
    // — we'll overwrite both halves at 4-5 when programming a real
    // routing.
    for entry in 0..entry_count {
        let low_reg = IOREDTBL_BASE + 2 * entry;
        unsafe {
            let existing = read_unlocked(low_reg);
            write_unlocked(low_reg, existing | IOREDTBL_MASKED);
        }
    }

    let _ = writeln!(
        serial::Writer,
        "ioapic: id={} base={phys:#010x} gsi_base={} version={ver:#010x} \
         redir_entries={entry_count} (all masked)",
        info.ioapic_id, info.gsi_base,
    );

    serial::write_str("ARSENAL_IOAPIC_OK\n");
}

/// Program redirection-table entry for `gsi` to deliver `vector` to
/// physical-destination `target_apic_id`, edge-triggered active-high,
/// fixed delivery, masked = false. 4-5 consumes this for IRQ1
/// (keyboard); kept #[allow(dead_code)] until then.
#[allow(dead_code)]
pub fn program(gsi: u32, vector: u8, target_apic_id: u8) {
    let gsi_base = IOAPIC_GSI_BASE.load(Ordering::Relaxed);
    let count = IOAPIC_REDIR_COUNT.load(Ordering::Relaxed);
    assert!(
        gsi >= gsi_base && gsi < gsi_base + count,
        "ioapic: GSI {gsi} outside [{}, {}) of this IOAPIC",
        gsi_base,
        gsi_base + count,
    );
    let entry = gsi - gsi_base;
    let low_reg = IOREDTBL_BASE + 2 * entry;
    let high_reg = low_reg + 1;

    // Low half: vector in bits 0..7; delivery mode 0 (fixed) bits
    // 8..10; destination mode 0 (physical) bit 11; polarity 0
    // (active-high) bit 13; trigger 0 (edge) bit 15; mask 0
    // bit 16. All other fields zero.
    let low = vector as u32;
    // High half: destination APIC ID in bits 24..31.
    let high = (target_apic_id as u32) << 24;

    let _g = IOAPIC_LOCK.lock();
    // SAFETY: IOAPIC_BASE_VIRT is set by init before any caller can
    // reach this site; reg indices are within the redirection
    // table per the gsi-range assert above.
    unsafe {
        // Write high half first while the entry is still masked,
        // then the low half to atomically (from the IOAPIC's
        // perspective) install the new routing and unmask in one
        // 32-bit write per the Intel I/O APIC datasheet §3.2.4.
        write_unlocked(high_reg, high);
        write_unlocked(low_reg, low);
    }
}

/// # Safety
/// Caller must hold IOAPIC_LOCK (or otherwise guarantee no
/// concurrent IOREGSEL writes), and IOAPIC_BASE_VIRT must be set.
unsafe fn read_unlocked(reg: u32) -> u32 {
    let base = IOAPIC_BASE_VIRT.load(Ordering::Relaxed);
    debug_assert_ne!(base, 0, "ioapic: read before init");
    // SAFETY: base is the HHDM-virtual address of the IOAPIC MMIO
    // page mapped by paging::map_mmio at init; IOREGSEL and IOWIN
    // are 32-bit volatile accesses per Intel datasheet §3.0.
    unsafe {
        core::ptr::write_volatile((base + IOREGSEL) as *mut u32, reg);
        core::ptr::read_volatile((base + IOWIN) as *const u32)
    }
}

/// # Safety
/// Same preconditions as `read_unlocked`.
unsafe fn write_unlocked(reg: u32, val: u32) {
    let base = IOAPIC_BASE_VIRT.load(Ordering::Relaxed);
    debug_assert_ne!(base, 0, "ioapic: write before init");
    // SAFETY: same as read_unlocked; IOWIN write transmits val to
    // the register selected by the prior IOREGSEL write.
    unsafe {
        core::ptr::write_volatile((base + IOREGSEL) as *mut u32, reg);
        core::ptr::write_volatile((base + IOWIN) as *mut u32, val);
    }
}
