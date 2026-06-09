# ADR-0010: amdgpu KMS scope — GOP-first first-light, then the DC port

## Status

**Accepted.** 2026-06-09. The M1-5-0 gate's part-3 deliverable; the
three decisions below were taken at the design review per the step-5
[HANDOFF](../../HANDOFF.md) — **A3 (GOP-first, then the DC port), B1
(minimal `Display` trait), C1 (merge GOP first-light with step-7 boot
bring-up)**. Cites the M1-5-0 closure audit
([`docs/audits/2026-06-amdgpu-kms-closure-audit.md`](../audits/2026-06-amdgpu-kms-closure-audit.md),
parts 1–2). Subordinate to [ADR-0004](0004-arsenal-pivot.md)
(combined-work driver strategy), [ADR-0005](0005-linuxkpi-shim-layout.md)
/ [ADR-0006](0006-linuxkpi-headers-are-shim.md) (shim layout;
headers-are-shim), and [ADR-0009](0009-xhci-native-rust.md) (the
LinuxKPI port budget was deliberately concentrated on amdgpu).

This first-use decision claims the **0010** slot (the provisional
three-crate-split reservation) per the established convention, and
cascades the reservations below it up by one each: three-crate split →
ADR-0012, cbindgen → ADR-0013, initcall-style table → ADR-0014,
per-workqueue runner → ADR-0015 (0011 stays the accepted deferred-work
runner). The one-line edit to ADR-0005's "Reserved successor ADRs" list
lands in this accepting commit.

## Context

M1 step 5 is amdgpu KMS via the LinuxKPI shim — the headlining M1
driver and the shim's first complex-driver port. The step-5 HANDOFF
made the first deliverable a *gate*, not code, because the scope choice
here is the difference between a multi-week sub-block and a multi-month
one, and that choice cannot be made responsibly without measurement.
The M1-5-0 closure audit supplies it. Three findings dominate:

1. **Size.** A KMS-only amdgpu port is **~925,000 LOC** of inherited C
   to vendor (amdgpu device core + DC display core + pm + amdgpu_dm +
   ttm/scheduler/drm core) and **~200 distinct API headers** to
   reimplement in BSD-2 under ADR-0006 — of which **71 `<drm/*>`
   headers are the entire DRM/KMS subsystem the shim has zero code
   for**. That is ~30× the xHCI LinuxKPI port ADR-0009 already rejected
   as too large, and ~755× virtio-balloon, the shim's only shipped
   inherited driver.

2. **The DC monolith.** The Display Core does not decompose. Its weight
   is shared infrastructure — `dc/core` + the DML/DML2 floating-point
   timing libraries + `resource` + `link` + `hwss` ≈ **283k LOC needed
   for any DCN target** — against a per-ASIC slice of only ~2.5–3.9k
   LOC (a 75:1 ratio). "Minimal amdgpu modeset" as a small carve-out
   does not exist; `dc_create()` drags in the whole shared core
   regardless. The target ASIC is **Strix Point** (RDNA 3.5, DCN 3.5/
   3.5.1), which is on the newer, larger, **less-aged DML2** path.

3. **Firmware.** Even a *modeset* on Strix loads ~15 signed blobs —
   `dcn_3_5*_dmcub`, `psp_14_0_0_{toc,ta}`, the full GFX microcode
   `gc_11_5_0_{imu,me,mec,mes1,mes_2,pfp,rlc}`, `sdma_6_1_0`,
   `smu_14_0_*` — because amdgpu device init brings up the whole GPU
   unconditionally. This needs a `request_firmware` + PSP secure-boot
   path the shim has never built, plus packaging the `linux-firmware`
   blobs into the Arsenal image.

Two structural facts frame the options:

- **QEMU cannot emulate amdgpu.** There is no per-commit CI for it. The
  22/22 QEMU smoke stays green guarding everything *except* amdgpu;
  amdgpu's validation is manual, on real Strix hardware, recorded in
  the devlog — never a CI gate (the validation model the HANDOFF
  fixed).

- **Arsenal already draws on a UEFI-GOP framebuffer.** `arsenal-kernel/
  src/fb.rs` renders to a Limine-provided linear framebuffer
  (`limine::framebuffer::Framebuffer`, mapped into HHDM). On UEFI
  hardware Limine *sources that framebuffer from GOP*. This means the
  cheapest path to a picture on the real Strix panel is not a new
  driver at all — it is the M0 framebuffer code Arsenal has shipped
  since boot, lit by Limine's UEFI GOP hand-off. This reframes the
  "intermediate" option from "write a simpledrm driver" to "confirm the
  existing LFB path lights the eDP," and is the key to the
  recommendation below.

The compositor (Stage, M2) and the M1 Slint software-rendered app both
sit on a kernel display surface; ARSENAL.md's M1 deliverable is a
software-rendered framebuffer app, not a modesetting compositor. M1
needs *a picture at the panel's native mode*, not runtime modeset.

## Decision

Three sub-decisions, taken at the design review. The recommended option
in each was accepted (marked RECOMMENDED below); the rejected
alternatives are retained for the record.

### A. How much amdgpu to inherit — **GOP-first first-light, then the DC port (recommended)**

- **A1 — Full DC now.** Vendor the ~925k LOC, reimplement the ~200 API
  headers (incl. the whole 71-header DRM/KMS surface), build the
  TTM/dma-buf/fence/i2c/firmware shim subsystems, port DML2, run
  amdgpu's real atomic modeset on Strix. Highest fidelity (real
  modeset, DPMS, backlight, eventually external displays). Multi-month;
  the M1 calendar variance lives here. First light is gated behind the
  entire port.

- **A2 — Minimal modeset.** *Rejected by the audit.* Finding 2 of the
  closure audit shows DC is monolithic; the "minimal" slice is a
  mirage — you pay ~283k LOC of shared DC + DML2 to set one mode.

- **A3 — GOP-first, then DC (RECOMMENDED).** Sub-block 5-1 gets a
  picture on the real Strix eDP by confirming the existing Limine/GOP
  framebuffer path (`fb.rs`) lights the panel — **near-zero new
  inherited C, zero firmware, no DC, no shim expansion**. Sub-block 5-2
  lands the `Display` trait (decision B) against the real backends.
  *Then* the full amdgpu DC port (= A1) proceeds as sub-blocks 5-3+,
  now de-risked: first-light is already in hand, the trait is shaped,
  and the ~925k-LOC port can iterate and fail on real silicon without
  blocking "Arsenal draws on the Framework." amdgpu DC remains the
  named step-5 deliverable — **GOP-first defers the DC port, it does
  not delete it.**

  **Why recommended.** Three independent audit axes — raw LOC, the DC
  monolith, and the ~15-blob firmware path — all point the same way:
  the full port is the multi-month tail, and GOP buys the milestone
  ("a picture on real Arsenal-driven hardware") for almost nothing
  because the LFB code already exists. This is the smaller-version-first
  call CLAUDE.md asks for (Y at 70% of the value for far less than 30%
  of the work). It honors peer concerns rather than ranking them:
  *usability* gets a picture early; *performance* — a native-mode LFB is
  sufficient for the M1 Slint app and shell; *security/correctness* — a
  925k-LOC GPL port is not rushed under a first-light deadline. And it
  keeps the build-loop discipline honest: first-light is observable and
  bisectable on hardware long before the DC port could be.

  **What GOP costs.** The firmware's chosen mode only (for an internal
  eDP, its native resolution — acceptable), no runtime modeset, no
  DPMS/backlight/power management, no external/DP outputs, no
  acceleration. All explicitly M1-acceptable per ARSENAL.md, and all
  delivered later by the A1 DC port that A3 still commits to.

### B. The `Display` trait shape — **minimal common surface (recommended)**

Deferred from step 4 (the 4-0 decision shipped `display.rs`'s
`DisplayInfo` + `PixelFormat` data vocabulary and deferred the trait to
n≥2 backends). It is now due, designable against the real set: the
Limine LFB / GOP framebuffer (write-through), the virtio-gpu scanout
(explicit transfer + flush), and — later, under A1/A3's DC port —
amdgpu (atomic page-flip).

- **B1 — Minimal trait (RECOMMENDED).** Resolution + a writable
  framebuffer + a `present()`/flush call. Write-through backends
  (Limine LFB, GOP) make `present()` a no-op or a cache flush;
  virtio-gpu's `present()` does `TRANSFER_TO_HOST_2D` + `RESOURCE_FLUSH`;
  amdgpu's later queues an atomic flip. Generalizes the existing
  `display.rs` data vocabulary into behavior, satisfies every real
  backend, and is what the M2 Stage compositor sits on — designed once,
  here, against concrete backends rather than speculation.

- **B2 — KMS-like surface** (planes, page-flip queues, damage rects,
  multi-output). *Rejected* — speculation against M2 Stage requirements
  not yet pinned; the same over-abstraction step 4's option C rejected
  at n=1. The 71 `<drm/*>` headers are *Linux's* abstraction; Arsenal's
  trait stays the minimal common one, not a DRM clone.

  Under A3, B1 is designable in 5-2 immediately (GOP + virtio-gpu are
  two live backends), without waiting on the DC port.

### C. Step-7 pull-forward — **merge GOP-first with step-7 boot bring-up (recommended)**

amdgpu/GOP validation needs a real Strix boot and a console off the
hardware (the Framework 13 has no RS-232 header).

- **C1 — Pull the minimum step-7 boot/console infra into step 5
  (RECOMMENDED, natural under A3).** GOP-first first-light *is* booting
  Limine on the real Strix over UEFI — which is exactly step-7's boot
  bring-up. And once `fb.rs` lights the panel, the kernel log can render
  to the framebuffer, giving a console off the hardware for free (with
  a USB-CDC serial or debug cable as the pre-framebuffer early-boot
  backstop). Step 5's first sub-block and step 7's boot bring-up merge;
  this is efficient, not scope creep, because GOP first-light cannot
  happen *without* the boot bring-up.

- **C2 — Defer all on-silicon validation to step 7.** Step 5 develops
  the DC port against compile gates only, unproven on hardware until
  step 7. *Rejected* unless A1-without-GOP is chosen — it leaves amdgpu
  regression-blind on real silicon for the whole port and forfeits the
  cheap early milestone A3 offers.

## Consequences

**Easier:**

- First-light on the real Strix panel arrives in sub-block 5-1, on
  shipped M0 code, decoupled from the ~925k-LOC port. "Arsenal draws on
  the Framework" stops being hostage to amdgpu.
- The `Display` trait is designed against concrete backends now (B1),
  unblocking the M2 Stage surface early.
- The shim's hard scaling test (DRM/KMS, TTM, dma-buf, firmware, DML2)
  still happens — but with breathing room, off the critical path to
  first-light, and after the trait and console exist.
- **The three-crate-split trigger (provisional ADR-0010→0012) is itself
  deferred.** GOP-first means the massive shim expansion that would
  force the Cargo reorganization does not land in 5-1/5-2; the split is
  reconsidered when the DC port (5-3+) actually balloons `linuxkpi/`.

**Harder:**

- Two display paths exist transiently (the GOP LFB and, later, amdgpu
  DC). The `Display` trait (B1) is what keeps that from leaking.
- GOP gives the firmware's mode only — no modeset/DPMS/PM/external
  outputs until the DC port lands. Acceptable for M1, but it is a real
  capability gap stated plainly.
- The DC port is still ~925k LOC of multi-month inherited-C work. **A3
  defers it; it does not shrink it.** amdgpu's M1 calendar variance is
  real and lands in 5-3+; the HANDOFF's "treat the compile-error
  iteration as the unbudgetable part, and when one bug owns multiple
  sessions, step away for a day" posture applies there in full.

**New risks:**

- **GOP-takeover handoff on Strix.** The recommendation assumes Limine's
  UEFI GOP path lights the internal eDP on this exact unit. Generally
  true, but unverified on Strix Point silicon — the first 5-1 task is to
  confirm it (and the native mode/topology), and the risk is a
  Strix-specific GOP/eDP subtlety. Cheap to falsify early, which is the
  point.
- **GOP "good enough" tempting DC deferral past M1.** Guard: amdgpu DC
  remains the *named* step-5 deliverable; GOP is sub-block 5-1, not the
  exit criterion. If the DC port slips, that is an explicit ADR/STATUS
  decision, not a silent drift.

**Follow-up work (on acceptance):**

- `STATUS.md`: record the M1-5-0 gate closed, the scope picks, and the
  redrawn 5-1..5-N sub-block plan (GOP first-light → `Display` trait →
  DC port).
- `docs/adrs/0005-*.md`: the reserved-list cascade edit (three-crate →
  0012, cbindgen → 0013, initcall → 0014, per-workqueue → 0015).
- No `docs/plan/ARSENAL.md` edit: GOP-first is a *sequencing* of the
  amdgpu-KMS deliverable, not a deviation from it — the M1 surface
  (amdgpu KMS) is unchanged. (If the review chooses A1-only or alters
  the M1 GPU deliverable, that becomes a plan revision per CLAUDE.md.)

## Alternatives rejected

- **A2 minimal modeset** — the audit's monolith finding kills it.
- **A1 full-DC-first, no GOP** — delays the cheap, available first-light
  milestone behind the multi-month port and keeps amdgpu regression-blind
  on hardware far longer, for no fidelity benefit that M1 needs.
- **B2 KMS-like trait** — premature M2 speculation; step 4 already
  rejected the same shape at n=1.
- **C2 defer-all-to-step-7** — only coherent without GOP; under A3 the
  boot bring-up is already being done for first-light.

## References

- [`docs/audits/2026-06-amdgpu-kms-closure-audit.md`](../audits/2026-06-amdgpu-kms-closure-audit.md)
  — the measured input (parts 1–2): ~925k LOC, the DC monolith, the
  Strix firmware set.
- [ADR-0004: Pivot from Field OS to Arsenal](0004-arsenal-pivot.md) —
  the combined-work driver strategy this scopes.
- [ADR-0006: LinuxKPI headers are the shim](0006-linuxkpi-headers-are-shim.md)
  — why the ~200 API headers are reimplementation, not vendoring.
- [ADR-0009: xHCI is native Rust](0009-xhci-native-rust.md) — the port
  budget concentrated on amdgpu, and the closure-must-be-measured
  lesson this gate applied.
- [`docs/plan/ARSENAL.md` § "M1 — Real iron"](../plan/ARSENAL.md) — the
  amdgpu-KMS deliverable and the software-rendered-framebuffer M1 app.
- [`arsenal-kernel/src/fb.rs`](../../arsenal-kernel/src/fb.rs) — the
  Limine LFB path that is the GOP framebuffer on real hardware.
- [Linux 6.12 `drivers/gpu/drm/amd`](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/tree/drivers/gpu/drm/amd?h=linux-6.12.y)
  — the amdgpu/DC source the audit measured.
- [Linux `simpledrm` / `efifb`](https://www.kernel.org/doc/html/latest/gpu/drm-kms.html)
  — the firmware-framebuffer-takeover pattern A3 mirrors (Arsenal gets
  it via Limine's GOP hand-off rather than a DRM driver).
- [Limine Boot Protocol — framebuffer feature](https://github.com/limine-bootloader/limine/blob/trunk/PROTOCOL.md)
  — the UEFI GOP → linear-framebuffer hand-off `fb.rs` consumes.
- Michael Nygard, "Documenting Architecture Decisions" (2011) — ADR
  template authority.

---

*This ADR sequences the M1 amdgpu-KMS deliverable; it does not reduce
it. GOP-first buys an early, cheap, observable first-light on real Strix
hardware and a stable `Display` trait, then commits to the full DC port
as the multi-month work the M1 calendar variance was always reserved
for. The decision is the review's; the recommendation is GOP-first.*
