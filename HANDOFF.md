Kickoff for M1 step 5 sub-block 5-1 — GOP first-light on real Strix:
the first time Arsenal boots on physical silicon. Per
[ADR-0010](docs/adrs/0010-amdgpu-kms-scope.md) decision A3 (GOP-first)
+ C1 (merge first-light with the step-7 boot bring-up). This is not an
amdgpu sub-block — it ships no DC, no shim expansion, no firmware. It
lights the Framework 13 (Ryzen AI 300 / Strix Point) eDP panel with
the framebuffer code Arsenal has shipped since M0, sourced from UEFI
GOP via Limine.

## Where we are

The M1-5-0 gate closed on 2026-06-09. The closure audit
([`docs/audits/2026-06-amdgpu-kms-closure-audit.md`](docs/audits/2026-06-amdgpu-kms-closure-audit.md))
measured a KMS-only amdgpu port at ~925k LOC + ~200 API headers, proved
DC is monolithic (no "minimal modeset"), and pinned the target to Strix
Point (DCN 3.5/3.5.1, DML2). ADR-0010 chose **GOP-first**: get a
picture on the real panel cheaply, land the `Display` trait (5-2), then
do the ~925k-LOC DC port (5-3+). GOP-first **defers but does not
shrink** the DC port.

The whole recommendation rests on one fact, now verified in-tree:
`arsenal-kernel/src/fb.rs` draws to a Limine-provided linear
framebuffer, and on UEFI hardware Limine sources that framebuffer from
GOP. So first-light on Strix is shipped M0 code plus a real-hardware
boot — not a new driver.

HEAD is the M1-5-0 gate commit (audit + ADR-0010 + STATUS + the
ADR-0005 numbering cascade). Smoke is **22/22**, stable. Push before
5-1 lands so the gate is on origin (the user pushes each sub-block).

## Read before proposing

read CLAUDE.md (build loop sacred; "real iron" is the M1 milestone;
the step-7 real-hardware-boot carry-forwards in STATUS — esp. the
step-3 #1 BME / Address-Device and the balloon round-22d BME trap,
which are about to meet real silicon) → STATUS.md (M1-5-0 gate closed,
GOP-first, the redrawn 5-1..5-N plan; the step-3 carry-forward list is
now load-bearing) → [ADR-0010](docs/adrs/0010-amdgpu-kms-scope.md)
(decisions A3/B1/C1; the fb.rs-is-GOP insight; the **GOP-takeover
handoff risk** — the one unverified premise this sub-block exists to
confirm) → **arsenal-kernel/src/main.rs** (the boot order — see the
map below; the path to first-light is the thing 5-1 hardens) →
arsenal-kernel/src/fb.rs (`init` asserts `bpp == 32`; `clear`,
`put_pixel`, `render_string`, `print_str` — the panel-side console
already exists) → arsenal-kernel/src/shell.rs (the fb console's known
limits, documented at the top: no fb cursor, serial-only destructive
backspace — these surface on the panel) → arsenal-kernel/src/serial.rs
(COM1 0x3F8 — **invisible on the Framework**, which has no RS-232; this
is why the panel must be the console) → xtask/src/main.rs (the iso
recipe is already BIOS+UEFI hybrid: `BOOTX64.EFI` under `EFI/BOOT`,
`limine-uefi-cd.bin` — the ISO is UEFI-bootable as-is) →
docs/devlogs/2026-05-arsenal-*.md (the M0 framebuffer + SMP devlogs —
the manual-display-check precedent this sub-block follows) → the Limine
boot protocol framebuffer feature → git log --oneline -6 → run the
sanity check → propose the 5-1 shape (or argue a different split) →
wait for the pick.

## The boot order (the path to first-light)

From `arsenal-kernel/src/main.rs`, current sequence:

    serial::init                         :96   (COM1 — invisible on Strix)
    ARSENAL_BOOT_OK                      :97
    heap init                            :118
    frames init                          :135
    ACPI → MADT → SMP bring-up           :145-191   << real-HW-fragile
    PS/2 keyboard                        :194
    fb::init + clear(NAVY) + amber sq    :227-231   << FIRST-LIGHT
    fb::render_string("ARSENAL")         :240       << FIRST-LIGHT
    sched::switch_test                   :258
    PCI scan / virtio probe / NVMe       :263-320   << real-HW-fragile
    ... shell / prompt

The key observation: **first-light already exists in code** (navy field
+ amber square + "ARSENAL" text, the same identity virtio-gpu's scanout
used). On Strix via Limine GOP, lines 227-240 should light the eDP.
But first-light currently sits *after* the ACPI/SMP block — real Strix
ACPI/MADT is far richer than QEMU's, and if that block panics or hangs,
the panel never lights and (serial being invisible) you debug blind.
That ordering is the first thing 5-1 fixes.

## What 5-1 is

A layered deliverable, smallest-observable-first:

1. **Limine boots on the real Framework and its menu renders on the
   eDP.** Proves UEFI boot + GOP before any kernel code runs. (Limine's
   own UEFI terminal draws to GOP.) Mostly a BIOS/USB exercise.
2. **First-light: the kernel lights the panel** with the navy + amber +
   "ARSENAL" pattern (main.rs:227-240) on real silicon. The
   ADR-0010-premise confirmation. This is the sub-block's core success.
3. **Boots to a prompt visible on the panel** — the fb console
   (`fb::print_str` + shell) renders the `>` prompt and echoes typed
   input on the eDP, with a real USB keyboard (step-3 xHCI HID path) if
   it enumerates, or PS/2 if the Framework exposes one.

The validation is **manual, on hardware, photographed for the devlog**
— there is no QEMU sentinel for real-silicon boot (the fb path already
smokes in QEMU; what 5-1 proves is that it survives real Strix). The
22/22 QEMU smoke stays green throughout and is unaffected.

## The split: QEMU-hardening (no hardware needed) + on-Strix validation

This sub-block divides cleanly, which matters because it lets progress
happen whether or not the physical unit is in hand yet:

**5-1a — boot-path hardening (QEMU-verifiable, keeps 22/22 green).**
Software changes that make a real-hardware boot survivable and
debuggable, all smoke-testable in QEMU first:

  - **Hoist `fb::init` to the earliest safe point** — right after heap
    (fb needs only the Limine framebuffer response + HHDM, both
    available immediately), *before* the ACPI/SMP block. Then first-light
    is the earliest possible signal on Strix, ahead of every
    real-HW-fragile probe. Bisect seam.
  - **Make the panic handler render to the framebuffer**, not only
    serial. On the Framework serial is invisible, so a panic before or
    after first-light must paint the panel (red field + message via
    `fb::render_string`/`print_str`) or you are blind. This is the single
    highest-value hardware-debuggability change. Bisect seam.
  - **Make device probing degrade gracefully on absence/difference** —
    on real Strix the virtio devices do not exist (blk/net/gpu/balloon
    all absent) and NVMe/xHCI are real, not QEMU models. Confirm each
    probe no-ops cleanly when its device is absent and does not panic
    the boot before the prompt (first-light already precedes them; this
    is about reaching the prompt). The step-3 carry-forwards (BSR=0
    Address Device, CSW unit-attention) and the balloon/NVMe **BME**
    findings are the known real-HW deltas to watch.
  - Optional: confirm `fb::init`'s `bpp == 32` assert holds for GOP's
    framebuffer format (GOP is typically 32-bpp BGRA — likely fine, but
    a hard assert on real hardware is worth a soft-fail path).

**5-1b — on-Strix validation (needs the physical unit).** Manual:

  - `cargo xtask iso`, write `arsenal.iso` to USB (`dd` / the user's
    tool of choice — the ISO is already UEFI-hybrid).
  - Framework BIOS: disable Secure Boot (Limine + the kernel are
    unsigned), enable USB boot, set boot order. (Signing Limine is the
    alternative; disable is the bring-up path.)
  - Boot; observe in order: Limine menu on the eDP → first-light pattern
    → prompt. Photograph each for the devlog.
  - Record the native mode Limine/GOP reports (the Framework 13 panel is
    high-DPI — note the resolution + that "ARSENAL" text is tiny at
    native scale, a real finding for the M2 Stage HiDPI work).

If the unit is not yet in hand, 5-1a still lands and smokes; 5-1b waits.

## Foundation 5-1 reuses

- **`fb.rs` in full** — the Limine LFB path is the GOP path on hardware;
  first-light is `clear`/`put_pixel`/`render_string` unchanged.
- **The shell's fb console** (`fb::print_str`, shell.rs) — the panel-side
  prompt already exists, with the documented cursor/backspace limits.
- **The Limine boot config** — the ISO is BIOS+UEFI hybrid already; no
  xtask change needed to boot UEFI, only (possibly) a Secure Boot
  decision.
- **The step-3 xHCI HID keyboard** — if a USB keyboard enumerates on
  real Strix, the existing path feeds the shell; the real-HW xHCI delta
  (BSR, quirks) is step-3 carry-forward territory.
- **The M0 manual-display-check discipline** — the framebuffer + SMP
  devlogs are the template for photographing first-light.

## Spec-fragile / risk pieces to watch

- **The GOP-takeover handoff on Strix is the one unverified premise.**
  Limine's UEFI GOP path lighting the *internal eDP* (not an external
  output, not a blank panel) on this exact unit is assumed, not proven.
  Falsify it early — the Limine menu appearing on the eDP is the first
  green light. If it does not, the issue is in Limine/UEFI/eDP, not
  Arsenal, and the debugging is boot-firmware-level.
- **Real Strix ACPI/MADT vs QEMU's.** The SMP block (main.rs:145-191)
  runs before first-light today; hoisting fb::init ahead of it (5-1a) is
  the mitigation. If ACPI parsing panics on real tables, that is its own
  finding — but the panel will already be lit to show it (once the panic
  handler paints fb).
- **Serial is invisible.** Every `serial::write_str` (incl. the
  `ARSENAL_*_OK` sentinels) produces nothing on the Framework. The panel
  is the only channel. This inverts the entire prior debugging model —
  internalize it before booting.
- **Secure Boot.** The Framework ships it on; an unsigned Limine/kernel
  will be rejected. Disable in BIOS for bring-up (or sign Limine).
- **HiDPI.** The Framework 13 panel is high-resolution; the 8-px-origin
  16×16 amber square and the "ARSENAL" glyph string will be physically
  tiny. Not a bug — a real M2-Stage HiDPI note, and a reason first-light
  might be easy to miss on a 2256×1504-class panel. Render larger if
  needed to confirm visually.
- **The real-HW probe panics are after first-light, not before.** So
  "no picture" and "no prompt" are different failures — the first is
  Limine/GOP/eDP or the boot-to-fb path; the second is a device probe.
  Keep them distinct when debugging.

## Estimates and cadence

This is the first real-silicon boot — qualitatively different from every
QEMU step, and first-silicon-boot always surprises. But the software is
unusually ready: first-light is shipped code, the console exists, the
ISO is UEFI-bootable. The realistic shape: 5-1a (hardening) is a focused
QEMU-verifiable session or two; 5-1b (on-Strix) is bounded by physical
access and BIOS/Secure-Boot/USB friction more than by code, plus
whatever the GOP-eDP handoff and the real-HW probes surface. If the
panel does not light and the cause is below Arsenal (Limine/UEFI/eDP),
that can eat sessions with little Arsenal code to show — the CLAUDE.md
"step away for a day" cue applies, and a `wip:` branch holds any partial
hardening. This sub-block does **not** touch the ~925k-LOC DC port; that
calendar variance is still banked in 5-3+.

A natural artifact to write during 5-1: a `docs/skills/limine.md` (or a
real-hardware-boot skill) capturing the USB-write + Framework BIOS +
Secure-Boot + boot-observe procedure, per CLAUDE.md's skill-file
pattern — future real-hardware sub-blocks (5-3+ amdgpu validation, step
6 iwlwifi, step 7) all repeat it.

## Sanity check before kicking off

    git tag --list | grep arsenal     # arsenal-M0-complete
    git log --oneline -6              # M1-5-0 gate (HEAD), M1-4-final,
                                      # f6c6da3, dd96e50, 0ea2814, c3258bd
    git status --short                # clean (or ?? HANDOFF.md while drafting)
    cargo build -p arsenal-kernel --target x86_64-unknown-none --release
    cargo clippy -p arsenal-kernel --target x86_64-unknown-none --release -- -D warnings
    cargo xtask iso                   # arsenal.iso, BIOS+UEFI hybrid
    ci/qemu-smoke.sh                  # ==> PASS (22 sentinels)

Expected: smoke PASSes with 22 sentinels; the QEMU framebuffer still
shows navy + amber + "ARSENAL" (the same path Strix will exercise). 5-1a
must keep this green; 5-1b does not run in CI.

## Out of scope for 5-1 specifically

- **amdgpu, DC, DML2, modeset, firmware.** All 5-3+. 5-1 is GOP only —
  the firmware's mode, no mode-setting.
- **The `Display` trait.** That is 5-2, designed against the GOP +
  virtio-gpu backends once first-light is in hand.
- **DPMS / backlight / power management / external displays / runtime
  resolution change.** GOP gives one fixed mode; the rest is the DC port.
- **Real NVMe / xHCI / networking working end-to-end on Strix.** 5-1
  needs *boot to a visible prompt*; full real-HW driver bring-up is
  step 7. A probe that no-ops or fails gracefully is acceptable here as
  long as it does not block the prompt.
- **Signing Limine / a Secure-Boot chain.** Disable SB for bring-up;
  signing is a later hardening decision.

## Permanently out of scope (do not propose)

- Any `unsafe` without a `// SAFETY:` comment. The fb path is MMIO
  writes into the GOP framebuffer; the panic-to-fb handler is unsafe-
  dense.
- Reverting any closed/tagged M0 or merged M1 commit. Force-pushing.
- Dropping the BSD-2 SPDX header from any Arsenal-base file.
- Conflating the 22/22 QEMU smoke with real-hardware success — they are
  independent; 5-1b is a manual, photographed observation, never a CI
  gate.
- Religious framing; HolyC; stable Rust. CLAUDE.md hard rules.

## First action

**Start 5-1a: hoist `fb::init` ahead of the ACPI/SMP block and make the
panic handler paint the framebuffer.** These are the two changes that
make a real-Strix boot both reach first-light earliest and show its
failures on the only visible channel, and both are verifiable in QEMU
without the hardware (the smoke must stay 22/22 and the QEMU window must
still show first-light). Land them as two bisect-seam commits. Then, if
the Strix unit is in hand, proceed to 5-1b: build the ISO, write USB,
disable Secure Boot, boot, and photograph the panel from Limine menu →
first-light → prompt. If the unit is not yet available, 5-1a is the
landing sub-block and 5-1b waits on physical access — say so in STATUS
rather than blocking.
