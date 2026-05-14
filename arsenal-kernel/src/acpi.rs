// SPDX-License-Identifier: BSD-2-Clause
//
// ACPI MADT parser — M0 step 4-0. Walks the RSDP / (X)SDT / MADT
// chain to enumerate the system's logical CPUs, I/O APICs, and ISA
// IRQ overrides. The output feeds 4-2's AP startup (which APIC IDs
// to INIT-SIPI-SIPI) and 4-3's IOAPIC bring-up (which MMIO bases to
// map and which redirection-table entries to program for ISA IRQs).
//
// Scope: precisely what step 4 consumes. Type 0 (Processor Local
// APIC), Type 1 (I/O APIC), Type 2 (Interrupt Source Override).
// Other MADT entry types — NMI sources (3, 4), LAPIC Address
// Override (5), x2APIC (9, 10, 11) — are skipped via the entry's
// length field, not parsed. Other ACPI tables — FADT, HPET, MCFG,
// SRAT — are post-M0; a hand-rolled MADT walker beats the
// rust-osdev `acpi` crate at this scope (HANDOFF.md ACPI parser
// depth trade-off at 4-0).
//
// Limine 0.5's RsdpResponse gives the RSDP address as an HHDM-
// shaped virtual address under base revision 1+ (which main.rs uses
// — revision 3). The RSDP, the (X)SDT, and each child table sit in
// firmware-reserved memory (legacy BIOS ROM at 0xE0000-0xFFFFF, or
// firmware-allocated regions on UEFI) which Limine's HHDM does not
// cover (paging.rs:106-108 — HHDM covers USABLE, reclaimable, ACPI,
// and framebuffer memory only). So every table page must be passed
// through paging::map_mmio before we dereference its HHDM-virtual
// address. map_mmio is idempotent for already-mapped pages, so the
// per-table map call is harmless when the firmware does happen to
// place tables in HHDM-covered memory.

use alloc::vec::Vec;
use core::fmt::Write;
use core::sync::atomic::{AtomicU32, Ordering};

use spin::Once;

use crate::apic;
use crate::paging;
use crate::serial;

// 4-1 / 4-2 / 4-3 / 4-5 consume these. 4-0 only populates them and
// emits ARSENAL_ACPI_OK; the getters and most fields are dead until
// later sub-blocks read them.

/// MADT Processor Local APIC entry — one per logical CPU.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct CpuInfo {
    pub acpi_processor_id: u8,
    pub apic_id: u8,
    pub flags: u32,
}

#[allow(dead_code)]
impl CpuInfo {
    /// MADT flag bit 0 — processor is enabled and ready to use.
    pub fn enabled(&self) -> bool {
        self.flags & 0x1 != 0
    }
    /// MADT flag bit 1 — processor is online-capable (BIOS marked
    /// it as a hot-add candidate). M0 brings up everything that's
    /// either enabled or online-capable.
    pub fn online_capable(&self) -> bool {
        self.flags & 0x2 != 0
    }
}

/// MADT I/O APIC entry — one per IOAPIC chip.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct IoapicInfo {
    pub ioapic_id: u8,
    pub base: u32,
    /// GSI number that maps to this IOAPIC's redirection-table entry 0.
    pub gsi_base: u32,
}

/// MADT Interrupt Source Override entry — ISA IRQ → GSI remapping.
/// On QEMU q35, IRQ0 (PIT) typically gets overridden to GSI 2; other
/// ISA IRQs are usually identity-mapped. 4-5 consults this for IRQ1
/// (keyboard) → GSI; 4-3 records the table so 4-5 doesn't repeat the
/// walk.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct IrqOverride {
    pub bus: u8,
    pub source: u8,
    pub gsi: u32,
    pub flags: u16,
}

static CPUS: Once<Vec<CpuInfo>> = Once::new();
static IOAPICS: Once<Vec<IoapicInfo>> = Once::new();
static IRQ_OVERRIDES: Once<Vec<IrqOverride>> = Once::new();
/// MADT-reported Local APIC base address. Step 3F's apic::init
/// trusted IA32_APIC_BASE; 4-2's AP bring-up cross-checks against
/// this value so a firmware quirk surfaces immediately instead of
/// silently aliasing the LAPIC into the wrong page.
static LOCAL_APIC_BASE: AtomicU32 = AtomicU32::new(0);

#[allow(dead_code)]
pub fn cpus() -> &'static [CpuInfo] {
    CPUS.get().map(Vec::as_slice).unwrap_or(&[])
}

#[allow(dead_code)]
pub fn ioapics() -> &'static [IoapicInfo] {
    IOAPICS.get().map(Vec::as_slice).unwrap_or(&[])
}

#[allow(dead_code)]
pub fn irq_override(isa_irq: u8) -> Option<IrqOverride> {
    IRQ_OVERRIDES
        .get()?
        .iter()
        .find(|o| o.source == isa_irq)
        .copied()
}

#[allow(dead_code)]
pub fn local_apic_base() -> u32 {
    LOCAL_APIC_BASE.load(Ordering::Relaxed)
}

/// Parse the ACPI tables starting from `rsdp_addr` (the value Limine's
/// RsdpResponse reports — HHDM-mapped pointer under base revision 1+).
/// Populates the CPU / IOAPIC / IRQ-override tables and asserts the
/// MADT-reported BSP APIC ID matches the LAPIC ID register from
/// `apic::init`. Emits ARSENAL_ACPI_OK on success.
pub fn init(rsdp_addr: usize) {
    let rsdp = map_table(rsdp_phys_from(rsdp_addr), 36);

    // SAFETY: rsdp points at the 20-byte RSDP that Limine reported.
    // Limine's RsdpResponse guarantees the pointer is to a valid RSDP
    // structure; we re-verify the signature and revision before
    // walking further.
    let signature: [u8; 8] = unsafe { read_bytes(rsdp, 0) };
    assert_eq!(
        &signature, b"RSD PTR ",
        "acpi: RSDP signature mismatch (got {signature:?}) — Limine reported \
         a bad RSDP pointer or revision is below 1",
    );
    // SAFETY: rsdp[15] is the revision byte (ACPI 6.x §5.2.5.3); within
    // the 20-byte RSDP structure that Limine guarantees.
    let revision = unsafe { read_u8(rsdp, 15) };

    // Choose RSDT (revision 0) or XSDT (revision 2+). XSDT entries are
    // 8-byte physical pointers; RSDT entries are 4-byte. ACPI 2.0+
    // systems must provide an XSDT; we prefer it when present so
    // 64-bit table pointers are not truncated.
    let (sdt_phys, entry_width) = if revision >= 2 {
        // SAFETY: rsdp[24..32] is the XsdtAddress field in the 36-byte
        // extended RSDP (ACPI 6.x §5.2.5.3). Revision ≥ 2 guarantees
        // the structure extends past 20 bytes.
        (unsafe { read_u64(rsdp, 24) as usize }, 8usize)
    } else {
        // SAFETY: rsdp[16..20] is the RsdtAddress field in the 20-byte
        // RSDP (ACPI 6.x §5.2.5.3). Always present regardless of revision.
        (unsafe { read_u32(rsdp, 16) as usize }, 4usize)
    };

    // Map enough for the header; we read length next and remap if the
    // table extends past one page.
    let sdt = map_table(sdt_phys, 36);
    let sdt_signature: [u8; 4] = unsafe { read_bytes(sdt, 0) };
    let expected_sig: &[u8; 4] = if entry_width == 8 { b"XSDT" } else { b"RSDT" };
    assert_eq!(
        &sdt_signature, expected_sig,
        "acpi: (X|R)SDT signature mismatch (got {sdt_signature:?}, \
         expected {expected_sig:?})",
    );

    // SAFETY: SDT header layout per ACPI 6.x §5.2.6. Length at [4..8]
    // covers the header and all entries.
    let sdt_length = unsafe { read_u32(sdt, 4) as usize };
    // Remap if the table extends past the page we mapped initially.
    let sdt = map_table(sdt_phys, sdt_length);
    let entry_count = (sdt_length - 36) / entry_width;

    // Walk the (X)RSDT looking for "APIC" (MADT). At M0 we expect
    // exactly one MADT; multi-MADT systems are not in scope.
    let mut madt_virt: Option<usize> = None;
    for i in 0..entry_count {
        let entry_offset = 36 + i * entry_width;
        let table_phys = if entry_width == 8 {
            // SAFETY: XSDT entry at offset entry_offset is a 64-bit
            // physical pointer (ACPI 6.x §5.2.8); within sdt_length.
            unsafe { read_u64(sdt, entry_offset) as usize }
        } else {
            // SAFETY: RSDT entry at offset entry_offset is a 32-bit
            // physical pointer (ACPI 6.x §5.2.7); within sdt_length.
            unsafe { read_u32(sdt, entry_offset) as usize }
        };
        // Map just the header so we can read the signature; the matched
        // MADT gets re-mapped at full length below.
        let table = map_table(table_phys, 36);
        // SAFETY: each pointer in the (X)SDT entries points at an SDT
        // header (4-byte signature first); map_table above ensures the
        // page is mapped before we dereference.
        let sig: [u8; 4] = unsafe { read_bytes(table, 0) };
        if &sig == b"APIC" {
            madt_virt = Some(table_phys);
            break;
        }
    }

    let madt_phys = madt_virt.expect("acpi: no MADT (APIC) table in (X)RSDT");
    let madt = map_table(madt_phys, 36);

    // SAFETY: MADT layout per ACPI 6.x §5.2.12. Header + 8 bytes of
    // MADT-specific fields, then variable-length entries.
    let madt_length = unsafe { read_u32(madt, 4) as usize };
    // Remap full table now that we know its length.
    let madt = map_table(madt_phys, madt_length);
    let lapic_base = unsafe { read_u32(madt, 36) };
    let _madt_flags = unsafe { read_u32(madt, 40) };

    LOCAL_APIC_BASE.store(lapic_base, Ordering::Relaxed);

    let mut cpus: Vec<CpuInfo> = Vec::new();
    let mut ioapics: Vec<IoapicInfo> = Vec::new();
    let mut overrides: Vec<IrqOverride> = Vec::new();

    let mut cursor = 44usize;
    while cursor + 2 <= madt_length {
        // SAFETY: each MADT entry begins with a 2-byte header — type
        // and length. Per ACPI 6.x §5.2.12 the length is at least 2
        // and never advances cursor past madt_length when entries
        // are well-formed.
        let entry_type = unsafe { read_u8(madt, cursor) };
        let entry_len = unsafe { read_u8(madt, cursor + 1) } as usize;
        assert!(
            entry_len >= 2,
            "acpi: MADT entry at offset {cursor} reports length {entry_len} (must be ≥ 2)",
        );
        assert!(
            cursor + entry_len <= madt_length,
            "acpi: MADT entry at offset {cursor} length {entry_len} runs past table end {madt_length}",
        );

        match entry_type {
            0 if entry_len >= 8 => {
                // SAFETY: Type 0 (Processor Local APIC) layout per
                // ACPI 6.x §5.2.12.2; 8 bytes including the 2-byte
                // header.
                cpus.push(CpuInfo {
                    acpi_processor_id: unsafe { read_u8(madt, cursor + 2) },
                    apic_id: unsafe { read_u8(madt, cursor + 3) },
                    flags: unsafe { read_u32(madt, cursor + 4) },
                });
            }
            1 if entry_len >= 12 => {
                // SAFETY: Type 1 (I/O APIC) layout per ACPI 6.x §5.2.12.3.
                ioapics.push(IoapicInfo {
                    ioapic_id: unsafe { read_u8(madt, cursor + 2) },
                    base: unsafe { read_u32(madt, cursor + 4) },
                    gsi_base: unsafe { read_u32(madt, cursor + 8) },
                });
            }
            2 if entry_len >= 10 => {
                // SAFETY: Type 2 (Interrupt Source Override) layout
                // per ACPI 6.x §5.2.12.5.
                overrides.push(IrqOverride {
                    bus: unsafe { read_u8(madt, cursor + 2) },
                    source: unsafe { read_u8(madt, cursor + 3) },
                    gsi: unsafe { read_u32(madt, cursor + 4) },
                    flags: unsafe { read_u16(madt, cursor + 8) },
                });
            }
            _ => {
                // Types 4 (LAPIC NMI), 5 (LAPIC Address Override),
                // 9-11 (x2APIC variants), etc. Skip via length field.
            }
        }
        cursor += entry_len;
    }

    // Cross-check: the MADT-reported BSP APIC ID must match the LAPIC
    // ID register that 3F's apic::init cached. A mismatch means
    // firmware reported a topology we cannot reconcile — bail before
    // 4-2 sends IPIs to the wrong target.
    let bsp_apic_id = apic::lapic_id();
    let madt_lists_bsp = cpus.iter().any(|c| c.apic_id == bsp_apic_id);
    assert!(
        madt_lists_bsp,
        "acpi: MADT does not list BSP APIC ID {bsp_apic_id} among its \
         {} processor entries — firmware/MADT inconsistent",
        cpus.len(),
    );

    let cpu_count = cpus.len();
    let ioapic_count = ioapics.len();
    let override_count = overrides.len();

    CPUS.call_once(|| cpus);
    IOAPICS.call_once(|| ioapics);
    IRQ_OVERRIDES.call_once(|| overrides);

    let _ = writeln!(
        serial::Writer,
        "acpi: RSDP rev={revision} ({}); MADT lapic_base={lapic_base:#010x}; \
         {cpu_count} CPUs, {ioapic_count} IOAPICs, {override_count} IRQ overrides; \
         BSP apic_id={bsp_apic_id} matched",
        if entry_width == 8 { "XSDT" } else { "RSDT" },
    );

    serial::write_str("ARSENAL_ACPI_OK\n");
}

/// Reduce the RSDP address Limine reports to a physical address.
/// Under base revision 1+ Limine returns an HHDM-shaped virtual
/// address; under revision 0 it was physical. main.rs uses revision
/// 3 today, but accepting either keeps the parser tolerant of a
/// future base-revision downgrade.
fn rsdp_phys_from(addr: usize) -> usize {
    let hhdm = paging::hhdm_offset() as usize;
    if addr >= hhdm { addr - hhdm } else { addr }
}

/// Map (or accept already mapped) `len` bytes of physical memory
/// starting at `phys`, rounded out to whole pages, and return the
/// HHDM-virtual address for `phys`. ACPI tables live in firmware-
/// reserved regions outside Limine's HHDM coverage; this is the
/// chokepoint that makes every downstream `read_*` dereference safe.
fn map_table(phys: usize, len: usize) -> usize {
    let page_mask = 0xFFFusize;
    let aligned_phys = phys & !page_mask;
    let span = (phys - aligned_phys + len + page_mask) & !page_mask;
    paging::map_mmio(aligned_phys as u64, span as u64);
    phys + paging::hhdm_offset() as usize
}

/// # Safety
/// `base + offset` must be a readable byte inside an HHDM-mapped
/// region.
unsafe fn read_u8(base: usize, offset: usize) -> u8 {
    unsafe { core::ptr::read_unaligned((base + offset) as *const u8) }
}

/// # Safety
/// `base + offset .. base + offset + 2` must be readable inside an
/// HHDM-mapped region.
unsafe fn read_u16(base: usize, offset: usize) -> u16 {
    unsafe { core::ptr::read_unaligned((base + offset) as *const u16) }
}

/// # Safety
/// `base + offset .. base + offset + 4` must be readable inside an
/// HHDM-mapped region.
unsafe fn read_u32(base: usize, offset: usize) -> u32 {
    unsafe { core::ptr::read_unaligned((base + offset) as *const u32) }
}

/// # Safety
/// `base + offset .. base + offset + 8` must be readable inside an
/// HHDM-mapped region.
unsafe fn read_u64(base: usize, offset: usize) -> u64 {
    unsafe { core::ptr::read_unaligned((base + offset) as *const u64) }
}

/// # Safety
/// `base + offset .. base + offset + N` must be readable inside an
/// HHDM-mapped region.
unsafe fn read_bytes<const N: usize>(base: usize, offset: usize) -> [u8; N] {
    let mut buf = [0u8; N];
    for (i, slot) in buf.iter_mut().enumerate() {
        // SAFETY: caller asserts the [base+offset, base+offset+N)
        // range is readable; loop bound respects it.
        *slot = unsafe { read_u8(base, offset + i) };
    }
    buf
}
