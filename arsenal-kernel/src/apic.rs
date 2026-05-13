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
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use x86_64::registers::model_specific::{ApicBase, ApicBaseFlags};
use x86_64::structures::idt::InterruptStackFrame;

use crate::paging;
use crate::serial;

// LAPIC register offsets (Intel SDM Vol. 3A §10.4.1 Table 10-1).
const LAPIC_REG_ID: u32 = 0x20;
const LAPIC_REG_VERSION: u32 = 0x30;
const LAPIC_REG_SVR: u32 = 0xF0;

/// SVR bit 8 — APIC software-enable. Set to allow the LAPIC to
/// deliver interrupts; clear to suppress all delivery (the hardware
/// equivalent of unloading the LAPIC, modulo the global IA32_APIC_BASE
/// LAPIC_ENABLE bit which only firmware should ever toggle).
const LAPIC_SVR_ENABLE: u32 = 1 << 8;

/// Spurious interrupt vector. Per Intel SDM Vol. 3A §10.9 this fires
/// when an IRQ was generated but is no longer pending by the time the
/// LAPIC tries to deliver it; it requires no EOI. We pick 0xFF by
/// convention (the highest vector — easy to reserve, well outside any
/// real IRQ vector range we plan to use in M0).
pub const SPURIOUS_VECTOR: u8 = 0xFF;

// Legacy 8259A data ports — OCW1 (interrupt mask register).
const PIC1_DATA: u16 = 0x21;
const PIC2_DATA: u16 = 0xA1;

/// Physical base of the LAPIC MMIO page, stashed by `init`. Reads
/// before init return 0 and `lapic_reg` panics — this is a one-shot
/// bring-up, not a "call when convenient" helper.
static LAPIC_BASE: AtomicU64 = AtomicU64::new(0);

/// Latched on the first spurious interrupt so the log records the
/// occurrence exactly once. A spurious "storm" (continuous delivery
/// during a mis-configured bring-up) would otherwise drown serial
/// output before the smoke could even observe a failure mode.
static SPURIOUS_SEEN: AtomicBool = AtomicBool::new(false);

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

/// Write a 32-bit LAPIC register.
///
/// # Safety
/// Same preconditions as `lapic_read`, plus: `val` must be a
/// spec-legal bit pattern for `reg` (e.g. an SVR write with reserved
/// bits respected, an LVT write with a non-NMI delivery mode unless
/// the caller actually wants NMI semantics). xAPIC writes have
/// hardware side effects.
unsafe fn lapic_write(reg: u32, val: u32) {
    // SAFETY: caller upholds the same mapping precondition as for
    // read; volatile write at the natural register width is the
    // spec-defined access. The hardware effect is whatever `reg`
    // documents — that responsibility lives with the caller.
    unsafe { core::ptr::write_volatile(lapic_reg(reg), val) }
}

/// Spurious interrupt handler for vector `SPURIOUS_VECTOR`. Logs
/// the first occurrence and silently absorbs subsequent ones. Per
/// Intel SDM Vol. 3A §10.9, spurious delivery does *not* set the
/// LAPIC's ISR bit and therefore does not require an EOI write.
pub extern "x86-interrupt" fn spurious_handler(_frame: InterruptStackFrame) {
    if !SPURIOUS_SEEN.swap(true, Ordering::Relaxed) {
        let _ = writeln!(
            serial::Writer,
            "apic: spurious interrupt (vector {:#x}) — logged once",
            SPURIOUS_VECTOR,
        );
    }
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

    // Software-enable the LAPIC. SVR bit 8 set, vector bits hold our
    // spurious vector. From this point on, the LAPIC will deliver
    // interrupts whose LVT entries are configured — no LVT is armed
    // yet, so nothing fires until 3F-2.
    //
    // SAFETY: LAPIC MMIO is mapped; the SVR bit pattern below is
    // spec-legal (Intel SDM Vol. 3A §10.9): bits 0..7 = spurious
    // vector, bit 8 = APIC software-enable, bits 9..31 reserved /
    // model-specific, written as zero per "preserve reserved fields"
    // convention (the SVR powers up with bits 9..31 clear, so we are
    // simply restating that state).
    unsafe {
        lapic_write(LAPIC_REG_SVR, LAPIC_SVR_ENABLE | SPURIOUS_VECTOR as u32);
    }

    let _ = writeln!(
        serial::Writer,
        "apic: 8259 masked; LAPIC phys={phys:#018x} id={} version={version:#010x} \
         svr-enabled; spurious vector={:#x}",
        id >> 24,
        SPURIOUS_VECTOR,
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
