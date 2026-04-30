# PLAN.md

> The full multi-year plan for Field OS. This is the contract between past-me, present-me, and future-me. Phase plans in `docs/plan/phase-*.md` are the long form; this file is the index.

## Phase 0 — QEMU Proof of Concept (M0–M10)

**Target:** A bootable ISO that demonstrates HolyC on bare metal + a windowed Brief renderer, runnable as `qemu-system-x86_64 -cdrom field-os-poc.iso -m 1G -smp 2 -enable-kvm`.

**Definition of done:** A 90-second video a stranger can watch and understand, showing (1) HolyC REPL alive, (2) Brief documents with executable macros and hyperlinks, (3) the skeleton of a real OS — Stage compositor, Cache file manager, Operator shell, all in software-rendered windows.

**Honest schedule:** 12–18 months part-time / 5–7 months full-time.

| ID | Name | FT-weeks | Status |
|---|---|---|---|
| M0 | Tooling and bootstrap (cross-GCC, holyc-lang fork, repo skeleton, CI) | 2–3 | ☐ |
| M1 | Boot to long mode (Limine, GDT/IDT, serial, framebuffer "Hello, Field") | 1–2 | ☐ |
| M2 | Memory management (PMM, VMM, kernel heap, higher-half) | 2–3 | ☐ |
| M3 | HolyC runtime on bare metal (in-kernel JIT, REPL over serial) | 3–6 | ☐ |
| M4 | Patrol v0 — preemptive scheduler stub | 2–3 | ☐ |
| M5 | I/O stack — PS/2 KB/mouse, framebuffer, PSF font rendering | 2–3 | ☐ |
| M6 | Stage v0 — proto-compositor with one window primitive | 3–4 | ☐ |
| M7 | Brief renderer v0 — text, hyperlinks, sprites, [Run] macros | 3–4 | ☐ |
| M8 | Operator v0 — shell as a live Brief in a Stage window | 2 | ☐ |
| M9 | Cache v0 — trivial file manager on a ramfs | 1 | ☐ |
| M10 | PoC packaging and demo — reproducible ISO, 90-sec video | 1–2 | ☐ |

Detail: [`docs/plan/phase-0.md`](docs/plan/phase-0.md)

## Phase 1 — Real Hardware on Framework 13 AMD (M11–M50)

**Target:** Field OS installable from USB on a Framework 13 AMD Ryzen 7 7840U; daily-driver-quality on that machine and the Tier-1 reference set; v0.1 released to the public.

**Definition of done:** A volunteer who has never seen Field OS before walks in with a fresh Framework 13 AMD and a USB stick, installs Field OS in <10 minutes, uses it as their only computer for 7 consecutive days — including web, email, music, video, sleep/wake on charger and battery, external HDMI — and at the end of the week reports zero data loss, no panics, no need for their previous OS.

**Honest schedule:** 18–30 months part-time / 9–15 months full-time.

| ID | Name | FT-weeks |
|---|---|---|
| M11 | Real-hardware first boot (Framework 13 AMD via Limine UEFI) | 2–3 |
| M12 | ACPI subsystem (ACPICA port) | 4–6 |
| M13 | NVMe driver (HolyC, from spec) | 3–4 |
| M14 | AHCI driver | 2 |
| M15 | xHCI USB driver — the painful one | 8–10 |
| M16 | USB HID class | 3 |
| M17 | AMDGPU port via LinuxKPI (KMS only) | 10–14 |
| M18 | i915/Xe port via LinuxKPI | 6–8 |
| M19 | Foundry v1 (Vulkan-class API) | 12–16 |
| M20 | Stage v1 (GPU-accelerated compositor) | 6–8 |
| M21 | Vector text rendering (FreeType + HarfBuzz, IBM Plex) | 4 |
| M22 | Field Symbols icon set (Lucide fork + custom glyphs) | 3 |
| M23 | HD-Audio (snd_hda_intel via LinuxKPI) — first LinuxKPI smoke test | 4 |
| M24 | USB Audio Class | 3 |
| M25 | Wavelength v1 (audio server, <8ms RTT target) | 6 |
| M26 | iwlwifi via LinuxKPI | 4 |
| M27 | MT7921/MT7922 via LinuxKPI | 4 |
| M28 | mac80211 + cfg80211 | 6 |
| M29 | Comm Tower v1 (lwIP + rustls/BearSSL + DNS) | 6–8 |
| M30 | Bluetooth HCI | 4 |
| M31 | ACPI power management (S3 + S0ix) | 10–14 |
| M32 | RedSea II filesystem + ext2 R/O + NTFS R/O | 8–10 |
| M33 | Cardboard Box v1 (sandbox + capability broker) | 6 |
| M34 | Stockpile v1 (.fbox packages, local + signed remote) | 4 |
| M35 | Patch (atomic snapshot updates) | 4 |
| M36 | Accessibility v1.0 — release-gating | 8–10 |
| M37 | Recon v1 (Servo-embedded browser) | 12–16 |
| M38 | Dispatch v1 (mail) | 8–10 |
| M39 | Roster v1 (contacts) | 3 |
| M40 | Schedule v1 (calendar) | 4 |
| M41 | Negatives v1 (photos) | 6 |
| M42 | Frequency v1 (music) | 4 |
| M43 | Projector v1 (video) | 5 |
| M44 | Cure v1 (system repair) | 3 |
| M45 | Survival Kit v1 (recovery) | 3 |
| M46 | Camo Index v1 (theming) | 2 |
| M47 | Codec v1 (notification surface) | 2 |
| M48 | Listening Post v1 (logs/profiler/debugger) | 4 |
| M49 | Performance polish + 6 GB footprint enforcement | 6–8 |
| M50 | v0.1 release | 2 |

Detail: [`docs/plan/phase-1.md`](docs/plan/phase-1.md)

## Phase 2 — v1.0 (M51–M90)

**Target:** Snapdragon X support equivalent to Framework 13 AMD. Hybrid polyglot via WASM Tabernacles (Wasmtime + WASI 0.2). Pro-tier apps: Manual (Pages-class), Armory (VS Code-class), Cassette (Logic Pro-class), Negatives v2 (Capture One-class). Stable ABI, multi-user, MDM, 11 localizations, SDK, remote Stockpile.

**Definition of done:** 1,000 daily-driver users active for ≥30 days each, across at least 3 Tier-1 hardware families.

**Honest schedule:** 24–36 months part-time / 12–18 months full-time.

Phase 2 is organized in five blocks:

- **Block A (M51–M58):** Snapdragon X bring-up. HolyC retargeted to AArch64 via QBE backend. Adreno X1 GPU port — highest-risk milestone.
- **Block B (M59–M62):** WASM Tabernacles. Wasmtime + WASI 0.2 + Component Model. Polyglot showcase apps.
- **Block C (M63–M68):** Manual v1, Briefing v1, Armory v1, Field Manual upgrade, Operator v1, Calling Card v1.
- **Block D (M69–M73):** Wavelength v2 (LV2/VST3-MIT/CLAP), Cassette v1 DAW, Negatives v2 RAW workflow, HDR/wide-gamut, Engine v1.
- **Block E (M74–M90):** Filesystem expansion, Listening Post v2, Stamina v1, Recon v2, multi-user, network sync, encrypted backups, MDM, localization framework, 11 language ports, stable ABI, SDK, Stockpile remote, dev docs, Snow Leopard performance pass, RC cycle, v1.0 release.

Detail: [`docs/plan/phase-2.md`](docs/plan/phase-2.md)

## Phase 3 — v2.0 "Horizon" (M91–M130)

**Target:** Apple Silicon M1/M2 via Asahi collaboration. Full tablet experience. Stencil (Illustrator-class). Sequence (DaVinci Resolve-class). Cellular, server profile, WASI 0.3/1.0.

**Definition of done:** ≥10,000 daily-driver users, 4 Tier-1 hardware families, ≥100 third-party apps in Stockpile, tablet usability ≥4/5 vs iPadOS, Snow-Leopard-grade performance (boot ≤1.5s, idle ≤180MB, cold launch ≤150ms).

**Honest schedule:** 24–36 months part-time / 12–18 months full-time.

Phase 3 tracks (overlapping):

- **Apple Silicon M1/M2 (M91–M100)** — m1n1, AGX/Honeykrisp, DCP, MCA audio, Broadcom WiFi/BT, Apple NVMe.
- **Tablet (M101–M110)** — multi-touch, MPP pen, Wacom EMR, rotation, on-screen keyboard, tablet Stage mode, split-view.
- **Stencil (M111–M115)** — vector engine, SVG round-trip, AI/EPS/PDF import, CMYK color management, v1 release.
- **Sequence (M116–M120)** — timeline NLE, codec pipeline, color grading, OTIO/FCPXML interchange, v1 release.
- **System maturity (M121–M130)** — cellular MBIM, server profile, WASI 0.3/1.0, Recon v3, Calling Card v2, Snow Leopard performance, Stockpile ecosystem, Field OS Conf, hardware partnerships, v2.0 release.

Detail: [`docs/plan/phase-3.md`](docs/plan/phase-3.md)

## Calibration

Total to v2.0: **6–9 calendar years from M0**, calibrated against:

- SerenityOS: year 1 to first windowed app, year 6 to first Chromebook port.
- Asahi Linux: 15 months to alpha installer, 24 months to GPU, 4 years to Vulkan 1.4 conformance.
- Redox OS: 10+ years, dynamic linking landed 2024–2025.
- Haiku: R1 Beta cadence — Beta 5 in Sept 2024, R1 stable still indeterminate.

This plan is conservative on timeline and specific on commands. The cadence is what matters most: monthly devlog, bi-weekly progress note, four-week demo video, one day off per week, refactor before adding.
