// SPDX-License-Identifier: BSD-2-Clause
//
// Local APIC bring-up — M0 step 3F. xAPIC mode (MMIO at the base
// reported by IA32_APIC_BASE; on QEMU q35 and on every commodity
// x86_64 chipset this is conventionally 0xfee00000, but the address
// is not architecturally guaranteed, so we read the MSR rather than
// hard-coding it). From 3F onward the kernel drives interrupts
// exclusively through the LAPIC; the legacy 8259 PIC gets masked so
// it stops competing for vectors 0x20..0x2F.
//
// Permanently out of scope for 3F: x2APIC mode (xAPIC suffices on
// single-core M0; x2APIC revisits at SMP), IPIs / AP startup (M0
// step 4), TSC-deadline timer mode (periodic is what M0 wants),
// ACPI MADT parsing (the MSR read covers what single-core needs).
// 3F-0 does discovery + MMIO mapping only; 3F-1 software-enables
// the LAPIC and installs the spurious vector.

use core::fmt::Write;
use core::sync::atomic::{AtomicU64, Ordering};

use x86_64::registers::model_specific::{ApicBase, ApicBaseFlags};

use crate::paging;
use crate::serial;

// LAPIC register offsets (Intel SDM Vol. 3A §10.4.1 Table 10-1).
const LAPIC_REG_ID: u32 = 0x20;
const LAPIC_REG_VERSION: u32 = 0x30;

// Legacy 8259A data ports — OCW1 (interrupt mask register).
const PIC1_DATA: u16 = 0x21;
const PIC2_DATA: u16 = 0xA1;

/// Physical base of the LAPIC MMIO page, stashed by `init`. Reads
/// before init return 0 and `lapic_reg` panics — this is a one-shot
/// bring-up, not a "call when convenient" helper.
static LAPIC_BASE: AtomicU64 = AtomicU64::new(0);

/// HHDM-virtual pointer to LAPIC register at offset `reg`. Panics if
/// `init` has not run.
fn lapic_reg(reg: u32) -> *mut u32 {
    let base = LAPIC_BASE.load(Ordering::Relaxed);
    assert_ne!(base, 0, "apic: lapic_reg called before init");
    let virt = base + paging::hhdm_offset() + reg as u64;
    virt as *mut u32
}

/// Read a 32-bit LAPIC register.
///
/// # Safety
/// `reg` must be a valid xAPIC register offset (Intel SDM Vol. 3A
/// §10.4.1 Table 10-1) and `init` must have mapped the LAPIC MMIO
/// page through paging::map_mmio.
unsafe fn lapic_read(reg: u32) -> u32 {
    // SAFETY: caller upholds the precondition that LAPIC MMIO is
    // mapped at lapic_reg(reg). xAPIC registers are naturally 16-byte
    // aligned and 32-bit-wide; a 32-bit volatile read is the spec-
    // defined access.
    unsafe { core::ptr::read_volatile(lapic_reg(reg)) }
}

/// Mask every line on both 8259A PICs by writing 0xFF to each IMR.
/// After this the legacy PIC's INTR line stays deasserted regardless
/// of incoming hardware lines, freeing vectors 0x20..0x2F for the
/// LAPIC.
fn mask_8259() {
    // SAFETY: 0x21 and 0xA1 are the 8259A primary/secondary data
    // ports per IBM PC convention since 1981. Writing OCW1 (IMR)
    // with all bits set is the canonical "mask all IRQ lines"
    // sequence per the Intel 8259A datasheet, Table 2. No other
    // hardware aliases these ports on x86 / x86_64.
    unsafe {
        outb(PIC1_DATA, 0xFF);
        outb(PIC2_DATA, 0xFF);
    }
}

pub fn init() {
    mask_8259();

    let (frame, flags) = ApicBase::read();
    let phys = frame.start_address().as_u64();

    assert!(
        flags.contains(ApicBaseFlags::LAPIC_ENABLE),
        "apic: IA32_APIC_BASE LAPIC_ENABLE bit clear (flags={flags:?}) — \
         firmware should have left the LAPIC hardware-enabled"
    );
    assert!(
        flags.contains(ApicBaseFlags::BSP),
        "apic: IA32_APIC_BASE BSP bit clear (flags={flags:?}) on the only \
         CPU we know about — SMP bring-up is M0 step 4"
    );
    assert!(
        !flags.contains(ApicBaseFlags::X2APIC_ENABLE),
        "apic: x2APIC is enabled (flags={flags:?}) — xAPIC MMIO access \
         pattern will not work; x2APIC is post-M0 work"
    );

    paging::map_mmio(phys, 0x1000);
    LAPIC_BASE.store(phys, Ordering::Relaxed);

    // SAFETY: LAPIC MMIO is mapped at phys + hhdm_offset by the
    // map_mmio call above; ID and VERSION are read-only registers per
    // Intel SDM Vol. 3A §10.4.6 and §10.4.8 with no read side effects.
    let id = unsafe { lapic_read(LAPIC_REG_ID) };
    let version = unsafe { lapic_read(LAPIC_REG_VERSION) };

    let _ = writeln!(
        serial::Writer,
        "apic: 8259 masked; LAPIC phys={phys:#018x} id={} version={version:#010x}",
        id >> 24,
    );
}

/// Write `val` to x86 I/O port `port`.
///
/// # Safety
/// Caller must ensure `port` is a valid I/O port and that writing
/// `val` produces the intended hardware effect.
unsafe fn outb(port: u16, val: u8) {
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") val,
            options(nomem, nostack, preserves_flags),
        );
    }
}
