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
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use x86_64::registers::model_specific::{ApicBase, ApicBaseFlags};
use x86_64::structures::idt::InterruptStackFrame;

use crate::paging;
use crate::serial;

// LAPIC register offsets (Intel SDM Vol. 3A §10.4.1 Table 10-1).
const LAPIC_REG_ID: u32 = 0x20;
const LAPIC_REG_VERSION: u32 = 0x30;
const LAPIC_REG_EOI: u32 = 0xB0;
const LAPIC_REG_SVR: u32 = 0xF0;
const LAPIC_REG_LVT_TIMER: u32 = 0x320;
const LAPIC_REG_TIMER_INITIAL_COUNT: u32 = 0x380;
const LAPIC_REG_TIMER_CURRENT_COUNT: u32 = 0x390;
const LAPIC_REG_TIMER_DIVIDE: u32 = 0x3E0;

/// SVR bit 8 — APIC software-enable. Set to allow the LAPIC to
/// deliver interrupts; clear to suppress all delivery (the hardware
/// equivalent of unloading the LAPIC, modulo the global IA32_APIC_BASE
/// LAPIC_ENABLE bit which only firmware should ever toggle).
const LAPIC_SVR_ENABLE: u32 = 1 << 8;

/// LVT entry bit 16 — interrupt is masked (won't deliver).
const LVT_MASKED: u32 = 1 << 16;
/// LVT timer entry bits 17..18 == 0b01 — periodic timer mode (the
/// initial-count register reloads automatically on each expiration).
const LVT_TIMER_PERIODIC: u32 = 1 << 17;

/// Timer divide-configuration register encoding for /16 (Intel SDM
/// Vol. 3A §10.5.4 Table 10-7). Common choice — gives a comfortable
/// 32-bit count range against typical bus frequencies (QEMU TCG
/// reports ~1 GHz; /16 yields ~62.5 M ticks/sec, comfortably above
/// the 100 Hz tick we want).
const LAPIC_TIMER_DIVIDE_16: u32 = 0b0011;

/// Spurious interrupt vector. Per Intel SDM Vol. 3A §10.9 this fires
/// when an IRQ was generated but is no longer pending by the time the
/// LAPIC tries to deliver it; it requires no EOI. We pick 0xFF by
/// convention (the highest vector — easy to reserve, well outside any
/// real IRQ vector range we plan to use in M0).
pub const SPURIOUS_VECTOR: u8 = 0xFF;

/// LAPIC timer vector. 0xEF sits in the "high-priority, no-conflict"
/// region above the legacy ISA range (0x20..0x2F, all masked at the
/// 8259) and below the spurious vector at 0xFF.
pub const TIMER_VECTOR: u8 = 0xEF;

/// PIT channel-2 setup. PC compatible since 1981: channel 2's gate is
/// software-controlled via port 0x61 bit 0 and its OUT line is
/// readable via 0x61 bit 5. We use mode 0 (interrupt on terminal
/// count) so OUT goes high exactly when the programmed count expires
/// — a clean, polled edge to time against.
const PIT_CH2_DATA: u16 = 0x42;
const PIT_COMMAND: u16 = 0x43;
const PIT_CONTROL_B: u16 = 0x61;
const PIT_CH2_GATE_BIT: u8 = 1 << 0;
const PIT_SPEAKER_BIT: u8 = 1 << 1;
const PIT_CH2_OUT_BIT: u8 = 1 << 5;

/// PIT crystal frequency in Hz (1.193182 MHz — the architectural
/// constant from the original IBM PC 8253, preserved on every x86
/// chipset since).
const PIT_FREQ_HZ: u32 = 1_193_182;

/// Calibration window — 10 ms is long enough to amortize the polling
/// loop's I/O-port overhead (microseconds) into noise, short enough
/// not to noticeably stretch boot time. Linux's `calibrate_APIC_clock`
/// uses the same 10 ms order of magnitude.
const CALIBRATION_MS: u32 = 10;
const CALIBRATION_PIT_COUNT: u16 = ((PIT_FREQ_HZ * CALIBRATION_MS) / 1000) as u16;

/// LAPIC timer tick rate. 100 Hz = 10 ms period = Linux desktop
/// default. Gives 10 ticks of evidence inside 100 ms, well under
/// sched::init's wall time, while keeping IRQ overhead negligible.
const TIMER_HZ: u32 = 100;

// Legacy 8259A data ports — OCW1 (interrupt mask register).
const PIC1_DATA: u16 = 0x21;
const PIC2_DATA: u16 = 0xA1;

/// Physical base of the LAPIC MMIO page, stashed by `init`. Reads
/// before init return 0 and `lapic_reg` panics — this is a one-shot
/// bring-up, not a "call when convenient" helper.
static LAPIC_BASE: AtomicU64 = AtomicU64::new(0);

/// LAPIC version register snapshot from `init`. 3G-2's `hw` command
/// displays this; cached so the cooperative `hw` path does not need
/// to re-issue an MMIO read.
static LAPIC_VERSION: AtomicU32 = AtomicU32::new(0);

/// Latched on the first spurious interrupt so the log records the
/// occurrence exactly once. A spurious "storm" (continuous delivery
/// during a mis-configured bring-up) would otherwise drown serial
/// output before the smoke could even observe a failure mode.
static SPURIOUS_SEEN: AtomicBool = AtomicBool::new(false);

/// Periodic-timer tick counter. Incremented from the timer IRQ
/// handler; readable from anywhere via `ticks()`. 3F-3's
/// ARSENAL_TIMER_OK sentinel observes this crossing a threshold.
static TICKS: AtomicUsize = AtomicUsize::new(0);

/// Latches the first time `observe_timer_ok` sees TICKS cross the
/// threshold, so the sentinel prints exactly once. Lives next to
/// SPURIOUS_SEEN and TICKS because all three are timer-observability
/// state that conceptually belongs to the LAPIC.
static TIMER_OK_LATCHED: AtomicBool = AtomicBool::new(false);

/// Number of timer ticks the cooperative observer must see before
/// asserting that the periodic LAPIC IRQ is actually delivering.
/// 10 ticks at 100 Hz = 100 ms of evidence — far enough above one
/// to dodge any first-tick boundary flakiness, far enough below
/// sched::init's wall time that the smoke observes it well within
/// the 15 s budget.
const TIMER_OK_THRESHOLD: usize = 10;

/// Snapshot of the tick counter. Cooperative tasks call this from
/// non-IRQ context to observe progress.
pub fn ticks() -> usize {
    TICKS.load(Ordering::Relaxed)
}

/// Cooperative-context probe: if the periodic timer has delivered
/// at least `TIMER_OK_THRESHOLD` ticks and we haven't printed yet,
/// emit ARSENAL_TIMER_OK. Idempotent — subsequent calls are a
/// single relaxed load past the latch check. Called from
/// `sched::idle_loop` after each yield, which is the only
/// cooperative context that routinely sees IF=1 + post-hlt wakeups
/// on single-core M0.
/// LAPIC version register snapshot cached at `init` time. Returns 0
/// if `init` has not yet run.
pub fn version() -> u32 {
    LAPIC_VERSION.load(Ordering::Relaxed)
}

pub fn observe_timer_ok() {
    if ticks() >= TIMER_OK_THRESHOLD
        && !TIMER_OK_LATCHED.swap(true, Ordering::Relaxed)
    {
        serial::write_str("ARSENAL_TIMER_OK\n");
    }
}

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

/// Periodic timer interrupt handler. Soft preemption: increment the
/// global tick counter and EOI. No context switch from inside the
/// IRQ — the cooperative `sched::yield_now` path remains
/// unchanged. Hard preemption (IRQ-driven context switch) is
/// deferred to M0 step 4 when SMP forces the design surface (per
/// HANDOFF.md at commit dcf2377).
pub extern "x86-interrupt" fn timer_handler(_frame: InterruptStackFrame) {
    TICKS.fetch_add(1, Ordering::Relaxed);
    // SAFETY: `apic::init` runs before `idt::init` installs this
    // handler in a way that any IRQ could deliver — and `init`
    // both maps the LAPIC MMIO and software-enables the LAPIC
    // before arming the timer LVT. By the time this handler can
    // fire, LAPIC MMIO is live. Writing 0 to the EOI register is
    // the spec-defined acknowledgement (Intel SDM Vol. 3A §10.8.5);
    // EOI has no read side effects and only a single legal write.
    unsafe { lapic_write(LAPIC_REG_EOI, 0) };
}

/// Calibrate the LAPIC timer against PIT channel 2. Programs the
/// PIT for a 10 ms one-shot, runs the LAPIC timer in masked one-shot
/// mode counting down from 0xFFFFFFFF, polls the PIT's OUT line
/// (port 0x61 bit 5) until terminal count, and returns the elapsed
/// LAPIC count. The result is "LAPIC timer ticks per 10 ms wall."
fn calibrate_lapic_timer() -> u32 {
    // Pre-clear PIT channel-2 gate and the speaker enable, preserving
    // the unrelated bits of port 0x61 (NMI status, parity check
    // enable). Reads of 0x61 are side-effect-free per the IBM PC AT
    // system reference.
    // SAFETY: 0x61 is the system control port B per IBM PC convention
    // (1981 onward); bits 0/1 own PIT ch2 gate/speaker, bit 5 reports
    // PIT ch2 OUT. No other hardware aliases these bits.
    let port_61_base = unsafe { inb(PIT_CONTROL_B) } & !(PIT_CH2_GATE_BIT | PIT_SPEAKER_BIT);
    // SAFETY: same.
    unsafe { outb(PIT_CONTROL_B, port_61_base) };

    // PIT channel 2: mode 0 (interrupt on terminal count),
    // lobyte/hibyte access, binary. Command byte 0xB0 =
    // 10_11_000_0 (sel=ch2, access=lobyte/hibyte, mode=0, binary).
    // Write count low byte then high byte; with gate still low the
    // counter is loaded but not counting.
    // SAFETY: 0x43 is the PIT command port; 0x42 is the channel-2
    // data port. Both are reserved for PIT use on every x86 chipset.
    unsafe {
        outb(PIT_COMMAND, 0xB0);
        outb(PIT_CH2_DATA, (CALIBRATION_PIT_COUNT & 0xFF) as u8);
        outb(PIT_CH2_DATA, (CALIBRATION_PIT_COUNT >> 8) as u8);
    }

    // Configure LAPIC timer for the calibration: divide /16, one-shot
    // (LVT timer mode bits = 00), masked (we don't want IRQ delivery
    // during calibration — we just want to read the count). Vector
    // doesn't matter while masked but we set it to TIMER_VECTOR for
    // consistency.
    // SAFETY: LAPIC MMIO mapped by the caller (init) before this
    // function runs.
    unsafe {
        lapic_write(LAPIC_REG_TIMER_DIVIDE, LAPIC_TIMER_DIVIDE_16);
        lapic_write(LAPIC_REG_LVT_TIMER, LVT_MASKED | TIMER_VECTOR as u32);
    }

    // Open the gate AND start the LAPIC countdown back-to-back. The
    // LAPIC initial-count write also serves as "start counting now."
    // Writing the PIT gate bit and the LAPIC initial count are two
    // I/O operations separated by only a few CPU cycles; any drift
    // from the perfect simultaneity is a small constant well below
    // the 10 ms window's resolution.
    // SAFETY: same hardware ownership as the writes above.
    unsafe {
        outb(PIT_CONTROL_B, port_61_base | PIT_CH2_GATE_BIT);
        lapic_write(LAPIC_REG_TIMER_INITIAL_COUNT, 0xFFFF_FFFF);
    }

    // Poll until PIT OUT goes high. With the count above and the
    // PIT clocked at 1.193182 MHz, this loop spins for ~10 ms.
    // SAFETY: same; inb on 0x61 is side-effect-free.
    while unsafe { inb(PIT_CONTROL_B) } & PIT_CH2_OUT_BIT == 0 {}

    // Snapshot the LAPIC current count immediately. Anything we do
    // after this read costs ticks against `elapsed`.
    // SAFETY: LAPIC MMIO mapped.
    let current = unsafe { lapic_read(LAPIC_REG_TIMER_CURRENT_COUNT) };
    let elapsed = 0xFFFF_FFFFu32.wrapping_sub(current);

    // Stop the LAPIC timer and close the PIT gate.
    // SAFETY: same hardware ownership.
    unsafe {
        lapic_write(LAPIC_REG_TIMER_INITIAL_COUNT, 0);
        outb(PIT_CONTROL_B, port_61_base);
    }

    elapsed
}

/// Arm the LAPIC timer in periodic mode. The timer fires every
/// `initial_count` LAPIC ticks (which we calibrated to ~10 ms,
/// giving the 100 Hz scheduling tick) and delivers `TIMER_VECTOR`
/// to whatever handler `idt::init` installed.
///
/// LVT timer must be configured (mode + vector) before the initial
/// count is written, since the initial-count write is what starts
/// the timer (Intel SDM Vol. 3A §10.5.4).
fn arm_periodic(initial_count: u32) {
    // SAFETY: LAPIC MMIO mapped by the caller (init).
    unsafe {
        lapic_write(LAPIC_REG_TIMER_DIVIDE, LAPIC_TIMER_DIVIDE_16);
        lapic_write(
            LAPIC_REG_LVT_TIMER,
            LVT_TIMER_PERIODIC | TIMER_VECTOR as u32,
        );
        lapic_write(LAPIC_REG_TIMER_INITIAL_COUNT, initial_count);
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
    LAPIC_VERSION.store(version, Ordering::Relaxed);

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

    // Calibrate against the PIT, then arm the LAPIC timer periodic
    // at TIMER_HZ. Sanity-bound the result: a sane QEMU TCG bus
    // clock yields ~625 k LAPIC ticks per 10 ms with divide /16;
    // real silicon ranges from a few hundred thousand to tens of
    // millions. Anything outside [1_000, 1_000_000_000] is broken
    // PIT/LAPIC behaviour we want to surface immediately, not
    // silently compensate for.
    let lapic_ticks_per_window = calibrate_lapic_timer();
    assert!(
        (1_000..=1_000_000_000).contains(&lapic_ticks_per_window),
        "apic: calibration produced wild value {lapic_ticks_per_window} — \
         expected ~10^5..10^7 for typical bus clocks under divide /16. \
         Check PIT gate sequencing or LAPIC divide-config write."
    );

    // initial_count = LAPIC ticks per (1 / TIMER_HZ) seconds. We
    // calibrated against CALIBRATION_MS milliseconds, so scale:
    // initial = ticks_per_window * CALIBRATION_MS / (1000 / TIMER_HZ).
    // At CALIBRATION_MS=10 and TIMER_HZ=100, the period is 10 ms —
    // exactly the calibration window, so initial = ticks_per_window.
    let period_ms = 1000 / TIMER_HZ;
    let initial_count = lapic_ticks_per_window
        .saturating_mul(period_ms)
        .saturating_div(CALIBRATION_MS);

    arm_periodic(initial_count);

    let _ = writeln!(
        serial::Writer,
        "apic: calibrated {lapic_ticks_per_window} LAPIC ticks per {CALIBRATION_MS} ms; \
         armed periodic {TIMER_HZ} Hz vector={:#x} initial_count={initial_count}",
        TIMER_VECTOR,
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

/// Read a byte from x86 I/O port `port`.
///
/// # Safety
/// Caller must ensure `port` is a valid I/O port whose read carries
/// the semantics the caller relies on.
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        core::arch::asm!(
            "in al, dx",
            out("al") val,
            in("dx") port,
            options(nomem, nostack, preserves_flags),
        );
    }
    val
}
