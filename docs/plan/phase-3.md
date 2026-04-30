# Field OS — Phase 3 Engineering Plan (M91–M130, v2.0 "Horizon" Release)

> **Status:** Authoritative engineering plan for Phase 3 of the Field OS project.
> **Predecessors:** Phase 0 (M0–M10, QEMU PoC), Phase 1 (M11–M50, v0.1 on Framework 13 AMD), Phase 2 (M51–M90, v1.0 with Snapdragon X, WASM Tabernacles, Manual/Armory/Cassette/Negatives v2).
> **This phase:** v1.0 → v2.0. Apple Silicon M1/M2, full tablet experience, Vector graphics editor, Video editor, cellular, server profile, ecosystem maturity.
> **Author:** Solo builder, kernel-comfortable, 15 h/wk part-time or 35 h/wk full-time.
> **Honest schedule:** **24–36 months part-time / 12–18 months full-time.** v2.0 lands roughly **6–9 calendar years** after Phase 0 began.

---

## 0. Executive Summary

Phase 3 is the phase where Field OS earns the right to be called "a serious modern desktop operating system" rather than "an impressive solo project." The work breaks into five tracks, run roughly in series with overlap:

1. **Apple Silicon M1/M2** (M91–M100) via formal collaboration with the post-collective-governance Asahi project.
2. **Full tablet experience** (M101–M110) — pen, multi-touch, on-screen keyboard, rotation, split-view, tablet Stage mode.
3. **Vector graphics editor — Stencil v1** (M111–M115), Illustrator-class.
4. **Video editor — Sequence v1** (M116–M120), DaVinci Resolve-class.
5. **System maturity, performance, ecosystem** (M121–M130) — cellular, server profile, WASI 0.3/1.0, Recon v3, Calling Card v2, Snow-Leopard-grade polish, Stockpile ecosystem, Field OS Conf, hardware partnerships, v2.0 release.

**Recommended app names (rationale below):**
- Vector graphics editor → **Stencil**
- Video editor → **Sequence**

**Definition-of-done for v2.0:** a Field OS install used by **≥10,000 daily-driver users** for ≥30 days each, certified across **four Tier-1 hardware families** — Framework 13 AMD, Framework 13 Intel, Snapdragon X reference, Apple Silicon M1/M2 — with Stencil and Sequence shipping in Stockpile, ≥100 third-party apps, tablet experience scoring ≥4/5 against iPadOS in usability study, and "Snow Leopard"-grade performance (boot ≤1.5 s, idle ≤180 MB, cold launch ≤150 ms).

**What is explicitly out of scope** (deferred to Phase 4): M3/M4 Apple Silicon, Apple Pencil 2, Touch ID / Secure Enclave, 3D modeling (Blender-class), CAD (Fusion-class), Vision-Pro-class spatial computing, HFS+/APFS read support, GPU compute parity with CUDA/MPS.

---

## 1. Strategic Context: Calibrating v2.0 Against Real OS Maturation

Post-1.0 OS releases historically take longer than v0.1 → v1.0:

| Reference | Span | Years |
|---|---|---|
| macOS Big Sur (11) → Tahoe (16) | five major versions, "refinement era" | ~5 |
| Windows 11 → Windows 12 (rumoured) | major version increment | 3–4 |
| GNOME 40 → GNOME 49 | 9 releases over 4.5 years | ~5 |
| KDE Plasma 5 → Plasma 6 | major Qt6 transition | ~10 |

Field OS Phase 3 is a **deliberately compressed analog** of this refinement era: one new architecture (Apple Silicon), one new form-factor (tablet), two flagship pro apps (Stencil, Sequence), and a "Snow Leopard" performance pass — packed into 24–36 months part-time. This is achievable because Phase 1 + Phase 2 already paid the costs of multi-arch toolchain, LinuxKPI on ARM64, stable ABI, SDK, and remote Stockpile.

**The "third-OS shock" thesis.** When Phase 1 brought up Framework 13 AMD it took ~40 milestones; when Phase 2 brought up Snapdragon X it took ~20 milestones (roughly 1.5×). For Apple Silicon as the third architecture I budget **~10 milestones (1.2×)**: LinuxKPI on ARM64 already works, the build, packaging, signing, and Patrol unit conventions are already multi-arch, and most of the integration cost — driver glue, devicetree handling, firmware loading — is amortised. Most of the residual cost is *novelty* in Apple-specific firmware/coprocessors, which is exactly where collaboration with Asahi pays off.

**Honest read on the calendar.** At 15 h/wk, a 10-milestone Apple Silicon track is ~6–9 months, tablet is ~9–12 months, Stencil is ~6–9 months, Sequence is ~6–9 months, system maturity ~6 months. That stacks to ~33–45 months naively; with parallel work (e.g. running Apple Silicon CI in the background while Stencil engineering proceeds) the realistic envelope is 24–36 months part-time. At 35 h/wk full-time, 12–18 months. This places v2.0 at calendar year 6–9 from Phase 0 — comparable to the Be / Haiku and SerenityOS / Ladybird arcs.

---

## 2. Apple Silicon Strategy (M91–M100)

### 2.1 Why M1/M2 only

The M1 (2020) and M2 (2022) families are the most thoroughly reverse-engineered Apple Silicon parts on the planet. Asahi's M1 work began in 2021; by 2024 the project shipped conformant OpenGL 4.6 / OpenGL ES 3.2 drivers, in mid-2024 Honeykrisp became the **first conformant Vulkan 1.3 driver for Apple hardware on any operating system**, and in 2025 Honeykrisp shipped **day-one Vulkan 1.4 conformance** alongside the Khronos 1.4 spec release. M2 inherits the same SoC architecture with minor tweaks. M3 changed SoC partitioning and DCP firmware interfaces (macOS 14+ structural changes); M4 introduced dynamic caching. Asahi's own progress reports through 2026 describe M3 as "roughly the same level as the original Asahi alpha for M1" — basic boot, no GPU. Phase 3 deliberately stops at M2.

### 2.2 Reference machines

- MacBook Air M1 (2020, J313)
- MacBook Air M2 (2022, J413)
- MacBook Pro 13" M1 (2020, J293)
- MacBook Pro 13" M2 (2022, J493)
- Mac mini M1 (2020, J274)
- Mac mini M2 (2023, J473)
- iMac M1 (2021, J456/J457)

The 14"/16" MacBook Pros (M1 Pro/Max, M2 Pro/Max) are stretch targets — Asahi supports them but speaker safety profiles, ProMotion VRR, and DCP firmware versioning add complexity. They land in v2.x point releases, not v2.0.

### 2.3 Why formal Asahi collaboration is the right answer

Hector Martin resigned as Asahi project lead in February 2025 and the project transitioned to a seven-person collective. The published governance document calls this "lazy consensus" and explicitly aims at "sustainable project governance" beyond a single individual. Two things follow:

1. The project is structurally more open to formal downstream relationships than it was under a single charismatic lead.
2. Field OS becomes the **third major Asahi-downstream consumer** after Asahi proper (Arch-based) and Fedora Asahi Remix — a meaningful contributor profile, not a leech.

**Field OS's contribution back:**
- An alternative testbed for kernel patches on a non-Linux userspace (a form of fuzzing for ABI stability).
- LinuxKPI-on-ARM64 maturity work from Phase 2 (port-back welcome to the BSDs and other LinuxKPI consumers).
- Documentation: every Apple device tree binding Field OS uses gets prose docs in the Field OS handbook that link back upstream.
- Funding: a Field OS Open Collective recurring contribution to the Asahi Open Collective once Stockpile revenue exists (Phase 3 stretch goal).

**Risks:**
- Collective decision-making is slower than benevolent dictatorship — schedule risk on getting Field-OS-specific patches accepted upstream. Mitigation: Field OS maintains its own downstream tree and rebases.
- Apple firmware updates can break drivers (DCP firmware interface has changed significantly between macOS 13 and 14). Mitigation: Field OS pins a known-good firmware blob set in the installer, exactly as Asahi does, and only follows Asahi's tested firmware bumps.
- Apple has historically been hostile-but-not-litigious to Asahi. Mitigation: no DMCA-circumventing work; Field OS uses only Asahi's clean-room reverse-engineered drivers, never Apple-derived code.

### 2.4 Apple Silicon kernel surface area

| Component | Origin | Language | Field OS integration |
|---|---|---|---|
| `m1n1` bootloader | Asahi (clean-room) | Python (proxy) + C (firmware) + a little Rust | Adopt as-is; chainload Field OS HolyC kernel as an `Image.gz` payload |
| Devicetree corpus | Asahi (upstream Linux) | DTS source | Consume the Apple `*.dts` files; Field OS HolyC kernel learns FDT parsing (already partly present from Snapdragon X bring-up) |
| Apple SoC drivers (AIC, SMC, PMU, I²C, SPI, GPIO, PCIe, NVMe, USB, WiFi, BT) | Linux upstream + Asahi out-of-tree | C, some Rust | Port via Field OS's existing **LinuxKPI shim** from Phase 2 |
| AGX Mesa userspace + Honeykrisp Vulkan driver | Mesa (Alyssa Rosenzweig) | C + Rust | Vendor as a Cardboard Box; Foundry retargets to AGX |
| AGX kernel-side DRM driver | Asahi (Asahi Lina) | Rust | Port via LinuxKPI's Rust subset (introduced in Phase 2 for Wasmtime; minimally extended here) |
| DCP (Display Coprocessor) | Asahi (out-of-tree, stabilising) | C | LinuxKPI port; ship as an "edge" driver in v2.0 |
| Apple Audio (MCA + speakersafetyd + DSP profiles) | Asahi (asahi-audio repo) | C + LADSPA / LSP plugins + per-machine YAML profiles | Wavelength integrates speakersafetyd as a Patrol unit; per-machine DSP profiles vendored |
| Broadcom WiFi/BT (`brcmfmac` over Apple PCIe transport) | Linux upstream + Asahi firmware loader | C | LinuxKPI port; firmware loader is the novel piece |
| `nvme-apple` | Asahi (mainlined) | C | LinuxKPI port; mainline kernel driver, low novelty |
| HID-over-SPI (keyboard, trackpad) | Asahi (mainlined) | C | LinuxKPI port |

### 2.5 M91–M100 milestone breakdown

#### M91 — Asahi collaboration formalised
**Scope.** Open formal communication with the Asahi collective. Email + Mastodon DMs to the seven collective members. Propose Field OS as a downstream consumer with three commitments: (1) no patching of upstream Asahi without PR, (2) crediting Asahi in every shipped binary, (3) recurring Open Collective contribution once Stockpile revenue starts. Establish a shared `#field-os` channel on the Asahi IRC/Matrix.
**Exit criteria.** Field OS listed in the Asahi "downstreams" docs page. A shared CI test rig (Mac mini M1) loaned or co-funded.
**Effort.** 2 weeks part-time (mostly correspondence).
**Dependencies.** None.

#### M92 — m1n1 integration
**Scope.** Field OS boots on Apple Silicon via the standard Asahi boot chain: `iBoot2 → m1n1 stage 1 (in stub macOS APFS partition) → m1n1 stage 2 (in EFI system partition, FAT32) → Field OS HolyC kernel as `Image.gz` payload alongside the Apple devicetree blob`. m1n1 hands off in EL2 with the devicetree describing the SoC. Field OS's existing FDT parser (from Snapdragon X) handles the Apple bindings with extensions.

```
$ cat build/m1n1.bin \
      apple-j313.dtb \
      field-os-kernel.Image.gz \
    > /efi/m1n1/boot.bin
```

The Field OS installer ships a payload-only second stage; the first stage lives in a stub macOS partition created by Apple's `bputil`/`bless` tooling, exactly per Asahi's open OS interop spec.

**Exit criteria.** A MacBook Air M1 boots Field OS to a serial console via m1n1 proxy mode in ≤7 s. `dmesg`-equivalent prints the Apple devicetree compatible string.
**Effort.** 4–6 weeks part-time.
**Dependencies.** M91. Existing Field OS HolyC kernel ARM64 port from Phase 2.

#### M93 — Apple SoC platform drivers via LinuxKPI
**Scope.** Port the Asahi platform driver stack: Apple AIC (replaces GICv3 — a fundamentally different interrupt controller architecture), Apple SMC (System Management Controller — battery, thermals, fan, lid switch), Apple I²C, Apple SPI, Apple GPIO, Apple PMU/PMP (Power Management Processor; new in late 2025 Asahi work), Apple PCIe (custom controller hosting an XHCI block for USB).
**Exit criteria.** `lspci`-equivalent enumerates all PCIe devices on the M1 and M2 Mac mini. Battery percentage readable on MacBook Air. Lid close generates a suspend signal.
**Effort.** 6–10 weeks. AIC is the biggest novelty — it requires Field OS's interrupt subsystem to learn a second top-level controller type. Mitigation: model AIC as a peer to GICv3, both implementing a common `irq_chip` interface that Patrol uses.
**Dependencies.** M92.

#### M94 — AGX / Honeykrisp GPU port
**Scope.** Port Honeykrisp via LinuxKPI. Two halves: (1) the kernel-side AGX DRM driver (Rust, by Asahi Lina) — reuses Phase 2's Rust-in-LinuxKPI scaffolding; (2) the Mesa userspace including the AGX Gallium driver (OpenGL 4.6, OpenGL ES 3.2) and the Honeykrisp Vulkan driver (Vulkan 1.4 conformant). Foundry, Field OS's Vulkan-class API, retargets to AGX through the standard Honeykrisp ICD.
**Exit criteria.** vkcube renders at full refresh on M1 Mac mini. The Foundry conformance suite (from Phase 1) passes on AGX. HoloCure-equivalent test workload runs at 60 fps.
**Effort.** 8–12 weeks. This is the single most technically novel milestone in Phase 3. Honeykrisp is a fast-moving target — the project has continued adding features and porting to more hardware after 1.4 conformance. Field OS pins to a known-good Mesa SHA at start of milestone, lets Asahi mainline whatever they want, then rebases at the end.
**Dependencies.** M93. LinuxKPI Rust support from Phase 2.

#### M95 — DCP integration
**Scope.** Port the Asahi DCP driver. DCP is a 9 MiB-firmware-blob coprocessor that handles all real display work (mode-setting, framebuffer, brightness, hot-plug). The firmware interface changes between macOS releases; Asahi pins to specific firmware blobs that ship via the Asahi Installer. Field OS does the same. Stage compositor learns to call DCP for mode-set and brightness rather than poking GPIOs (the M92 placeholder).

VRR / ProMotion is *not* in v2.0. Asahi's own VRR work as of 2026 is behind a kernel module parameter and acknowledged as a workaround relative to the VESA spec. Field OS will expose `appledrm.force_vrr=1` as a power-user knob but the default and certified-mode is fixed-rate.
**Exit criteria.** External display via HDMI on Mac mini works. Internal MacBook Air M1 display brightness adjustable. DPMS off works (real DCP off, not GPIO hack).
**Effort.** 6–8 weeks.
**Dependencies.** M94 (DCP shares plumbing with AGX).

#### M96 — Apple Audio (MCA + speaker DSP)
**Scope.** Port the Asahi audio stack: MCA controller (the I²S peripheral and Apple DMA), Cirrus Logic CS42L42 headphone codec, Texas Instruments TAS2764 amp drivers, and the userspace `speakersafetyd` plus per-machine DSP YAML profiles. Wavelength (Field OS's audio server) absorbs `speakersafetyd` as a Patrol unit named `wavelength.speakersafety`. The DSP profiles for the seven reference Macs are vendored from `AsahiLinux/asahi-audio` under their license.

This is a hard requirement. Without speaker safety, kernel updates can physically destroy MacBook Air speakers — exactly the failure mode Asahi guards against in their docs ("you may very well blow up your speakers"). Field OS adopts Asahi's deployment ordering: `asahi-audio + speakersafetyd → kernel`, never reversed.

**Exit criteria.** All seven reference Mac models produce balanced audio without speaker damage at 100% volume for 30-minute sustained sine sweep. Mic input works via the AOP (Always-On Processor) PDM array on MacBooks.
**Effort.** 4–6 weeks.
**Dependencies.** M93.

#### M97 — Apple WiFi + Bluetooth
**Scope.** Port `brcmfmac` (the Broadcom FullMAC driver) and the Apple-PCIe-transport firmware loader. The firmware lives in macOS's `/usr/share/firmware/wifi/` and is extracted to the Field OS root filesystem at install time by the installer. BT is a sibling on the same chip; reuse Linux's `hci_bcm` / `btbcm` via LinuxKPI.
**Exit criteria.** WiFi 6 connection at full speed on M2 MacBook Air. Bluetooth pairing with AirPods Pro 2 (via Comm Tower's existing BT stack from Phase 1).
**Effort.** 3–4 weeks.
**Dependencies.** M93.

#### M98 — Apple NVMe + storage
**Scope.** Port the mainlined `nvme-apple` driver via LinuxKPI. Apple's NVMe variant uses non-standard sector handling and a custom queue arrangement, but it has been upstreamed and is therefore the lowest-novelty driver in this track.
**Exit criteria.** Field OS's filesystem (Field FS, designed in Phase 2's filesystem expansion) reads/writes at >2 GB/s on M2 Air internal SSD.
**Effort.** 2 weeks.
**Dependencies.** M93.

#### M99 — Apple keyboard / trackpad / fingerprint stub
**Scope.** HID-over-SPI for the M1/M2 keyboards and Force-Touch trackpad. Touch ID is a capability stub: the Secure Enclave Processor (SEP) is not reverse-engineered by Asahi to a usable degree, so Field OS exposes a `field.auth.touchid` capability that returns "unavailable" and falls back to password. Defer real Touch ID to Phase 4.
**Exit criteria.** Keyboard typing latency <8 ms. Multi-finger trackpad gestures work with the same library used in M101.
**Effort.** 3 weeks.
**Dependencies.** M93.

#### M100 — Apple Silicon v0.1-equivalent acceptance
**Scope.** Field OS on the seven reference Macs reaches the **Phase 1 acceptance bar**: boot, login, suspend/resume, Recon (browser), Dispatch (mail), audio playback, video playback, ≥6 h battery on MBA M2.
**Exit criteria.** Internal acceptance test suite passes on MBA M1 and MBA M2; remaining five machines pass ≥80% (some DCP / audio profile finishing required for v2.0 GA).
**Effort.** 2–3 weeks of integration testing.
**Dependencies.** M92–M99.

### 2.6 Apple Silicon track total
~6–9 months full-time / 12–18 months part-time. Scheduled in **months 1–9 part-time / months 1–6 full-time** of Phase 3, because the Apple Silicon work is mostly serial and must be largely done before Stencil/Sequence development gets first-class testing on Apple hardware.

---

## 3. Tablet Experience Strategy (M101–M110)

### 3.1 Why full tablet, not half measures

Half measures are uncompetitive. iPadOS is mature; GNOME 49+ has decent tablet support; KDE Plasma Mobile exists. A "Field OS that supports a pen on a laptop" is a feature; a "Field OS that competes for tablet daily-driver users" requires **all** of: multi-touch gestures, pen with pressure/tilt/eraser/hover, on-screen keyboard with autocorrect, screen rotation, split-view, and a tablet-optimised compositor mode. Anything less and reviewers will (rightly) call it incomplete.

### 3.2 Reference hardware

- **Microsoft Surface Pro 11** (Snapdragon X Elite/Plus, Slim Pen 2 / MPP, 13" OLED option, 2880×1920, optional 5G via Snapdragon X20 LTE / X65 5G modem). Released June 2024. Primary target.
- **Lenovo ThinkPad X12 Detachable Gen 2** (Wacom AES). Secondary.
- **Samsung Galaxy Book 360 / Galaxy Tab S-series** when Snapdragon X tablet variants ship in volume (Phase 3 stretch).
- **Framework 13 with optional touchscreen** as a development reference even though it's not a true tablet — useful for testing pen and on-screen keyboard without rotation.

### 3.3 The pen protocol matrix

| Protocol | Used in | Source | Field OS approach |
|---|---|---|---|
| **Microsoft Pen Protocol (MPP) 2.0+** | Surface Pen, Surface Slim Pen 2, HP/Dell tilt pens | HID over USB / I²C / Bluetooth, descriptors documented in Microsoft's hardware design guidance | Use Linux's MPP-compatible stack (`hid-multitouch`, IPTS for Surface) via LinuxKPI |
| **Wacom EMR (Electromagnetic Resonance)** | ThinkPad X12, Galaxy Tab S Pen, Wacom tablets | Linux `wacom` driver, decades of reverse engineering | LinuxKPI port |
| **Wacom AES** | Some ThinkPads, HP convertibles | Linux `wacom_w8001` / vendor variants | LinuxKPI port |
| **USI** | Chromebooks, some new Windows tablets | Universal Stylus Initiative open spec | Native HolyC implementation; spec is small enough |
| **Apple Pencil 2 (custom Apple MPP variant)** | iPad Pro / Apple displays | Not fully reverse-engineered by Asahi | **Defer to Phase 4** — capability stub |

For v2.0, the Surface Pen / Slim Pen 2 (4096-level pressure, tilt, barrel button, eraser, hover) is the certified pen experience. Wacom EMR is the certified non-Microsoft option.

### 3.4 The on-screen keyboard problem

A competitive on-screen keyboard with autocorrect is a multi-month project. Two realistic options:

1. **Vendor a Hunspell-based predictor.** Hunspell is BSD-3 / MPL-1.1 / GPL-2.0 tri-licensed; the BSD path is compatible with Field OS's BSD-2 core. Predictive text is a thin wrapper: tokenise the recent context, run candidates against Hunspell with Levenshtein-distance scoring, present top-3.
2. **Native HolyC predictor on a small n-gram model.** Smaller code (~500 lines), lower memory, weaker accuracy.

**Recommendation:** Option 1 for v2.0, with the predictor as a Cardboard Box (sandboxed userspace). HolyC keyboard widget itself is ~1500 lines and lives in Stage. Layouts: QWERTY, AZERTY, QWERTZ, Dvorak, Colemak. CJK input via Phase 2's localisation framework with **a separate IME architecture** (input-method-editor); IMEs are a deep topic and full CJK input is a v2.x deliverable, with a v2.0-bundled ChickenPinyin and SKK-class IME for Japanese as MVP.

Voice dictation (Wavelength + on-device whisper.cpp via Engine compute API) is a **stretch goal** for v2.0; if it slips, document and ship in v2.1.

### 3.5 Stage compositor: tablet mode

Stage already exists from Phase 1 with translucent vibrancy and 8/12/20 px corner radii. Tablet mode is a parallel render path with shared underlying primitives:

- **Hit targets** ≥44 pt (Apple HIG baseline).
- **Gesture nav.** Swipe up from bottom = app switcher; swipe from top-right = Codec (Field OS's Control-Center-equivalent); swipe from top-left = notifications/Cache; swipe-and-hold from left = Cardboard Box overview.
- **Default presentation.** Apps fullscreen by default; window chrome hidden.
- **Translucency.** Big Sur vibrancy preserved but alpha bumped slightly (touch UIs read better with a touch more contrast).
- **Auto-switch.** Form-factor signals: lid angle (if convertible), keyboard attachment (Surface kickstand keyboard detected via HID), accelerometer orientation. Hysteresis to prevent flapping.

### 3.6 M101–M110 milestone breakdown

#### M101 — Multi-touch input pipeline
**Scope.** Touch event capture via I²C-HID (Surface) and USB-HID (most tablets). Gesture recognition library (HolyC, ~2k lines): tap, double-tap, long-press, pan, two-finger pinch, two-finger rotate, three-finger swipe, four-finger swipe. Events accumulate into Patrol's input bus alongside keyboard/mouse.
**Exit criteria.** Surface Pro 11 multi-touch generates correct events at 100 Hz; pinch-zoom in Recon hits 60 fps.
**Effort.** 4–5 weeks.

#### M102 — Microsoft Pen Protocol (MPP) support
**Scope.** Port the Linux IPTS (Intel Precise Touch & Stylus) driver via LinuxKPI for Surface devices. MPP events: 4096-level pressure, tilt (X/Y), barrel button, eraser end, hover. Pen events flow alongside touch events in Patrol's input bus with a pen-source tag so apps can disambiguate.
**Exit criteria.** Surface Slim Pen 2 on Surface Pro 11: pressure curve correct, tilt accurate to ±2°, hover detected at 10 mm, end-to-end pen latency <20 ms (measured camera-to-screen).
**Effort.** 5–6 weeks.

#### M103 — Wacom EMR / AES
**Scope.** Port Linux's `wacom` and `wacom_w8001` drivers via LinuxKPI. EMR is battery-free; AES is battery-powered; both go through the HID layer. Field OS's pen abstraction is protocol-agnostic.
**Exit criteria.** ThinkPad X12 Detachable pen works in Stencil, Manual, and the system-wide Sketch app.
**Effort.** 2–3 weeks.

#### M104 — Screen rotation
**Scope.** Read accelerometer via the IIO subsystem (port via LinuxKPI). Userspace daemon `patrol.rotation` polls at 4 Hz, applies hysteresis (3 s dwell time), and signals Stage. Stage reflows in <100 ms via the existing Brief layout engine — Brief was designed in Phase 1 to handle layout changes (it's the same machinery used for window resize). Manual override in Frequencies (settings).
**Exit criteria.** Rotating Surface Pro 11 from landscape to portrait completes rotation animation in <100 ms with no dropped frames.
**Effort.** 2 weeks.

#### M105 — On-screen keyboard
**Scope.** HolyC keyboard widget in Stage. Five Latin layouts at v2.0; Hunspell-backed autocorrect via a sandboxed Cardboard Box; emoji picker; haptic feedback if the device exposes a haptic actuator. Voice dictation is a v2.0 stretch / v2.1 deliverable.
**Exit criteria.** On-screen keyboard feels comparable to iPadOS in a 5-user usability study (subjective, but use ≥4/5 average rating as the bar).
**Effort.** 8–10 weeks. The largest single tablet milestone.

#### M106 — Tablet Stage compositor mode
**Scope.** The parallel render path described in §3.5. Auto-switch logic.
**Exit criteria.** Detaching the Surface Pro Flex Keyboard triggers tablet mode in <500 ms; reattaching triggers desktop mode. All system apps reflow correctly.
**Effort.** 6–8 weeks.

#### M107 — Split-view + Slide Over
**Scope.** Two apps side-by-side with a draggable divider snapping to 30/50/70%. A third app can hover as a Slide Over panel (iPadOS-modelled). Apps must opt-in via a `field.tablet.splittable = true` manifest flag; legacy apps run only fullscreen.
**Exit criteria.** Manual + Recon side-by-side at 50/50 on Surface Pro 11 with no visual artefacts during resize.
**Effort.** 4 weeks.

#### M108 — Tablet-aware app modes for system apps
**Scope.** Recon, Manual, Briefing, Negatives, Cassette, Sequence (when ready), Stencil (when ready) all gain tablet layouts: collapsible sidebars, finger-friendly toolbars, gesture-based zoom in canvas-style apps.
**Exit criteria.** Each system app passes a tablet usability checklist (16 items: hit-target sizing, gesture support, rotation handling, etc.).
**Effort.** 4–6 weeks (parallelisable with Stencil / Sequence work).

#### M109 — Surface Pen Pro / hover preview
**Scope.** The 2024 Surface Pen Pro adds squeeze and double-tap gestures; expose these as input events. Hover preview: when a pen hovers, apps receive a pen-hover event for cursor preview / link preview. Apple Pencil 2 deferred to Phase 4 (its custom Apple-MPP variant is not adequately reverse-engineered).
**Exit criteria.** Squeeze in Stencil cycles between brush and selection tools.
**Effort.** 2 weeks.

#### M110 — Tablet acceptance suite
**Scope.** Internal usability study with 10 users (a mix of iPad/Android tablet/Surface owners) using Field OS in tablet mode for 2-week trials.
**Exit criteria.** Touch latency <50 ms (measured), pen latency <20 ms, gesture recognition >99% on a 1000-gesture standard test vector, ≥4/5 in subjective usability study.
**Effort.** 3 weeks of running the study + fix-ups.

### 3.7 Tablet track total
~9–12 months part-time / 5–6 months full-time. Runs **months 4–13 part-time / months 3–8 full-time** with deliberate overlap with Apple Silicon (M101 multi-touch and M104 rotation can begin while M93–M97 are still in flight).

---

## 4. Stencil — Vector Graphics Editor (M111–M115)

### 4.1 Name

I evaluated the candidate set: **Outline, Diagram, Render, Schematic, Stencil, Vector, Plot, Trace, Sketchpad, Compass, Drafting Table.** Recommendation: **Stencil.**

Rationale: (1) MGS3-warm — a stencil is a tactical reconnaissance tool ("trace the outline of the enemy installation") and a draftsman's tool. (2) Concrete and specific (unlike "Vector" or "Diagram", which feel categorical). (3) Two syllables, scans well in Stockpile. (4) No collision: there is no major existing vector graphics editor named Stencil. (5) Pairs naturally with Field OS's "Field Symbols" icon set — a stencil makes symbols. (6) Sits well in the existing app naming: Recon / Dispatch / Roster / Schedule / Negatives / Frequency / Projector / Cure / Survival Kit / Manual / Armory / Cassette / Stencil / Sequence.

Runner-up: **Trace** (also strong; tactical, kept in reserve for a related tool — perhaps a tracing/calligraphy app in Phase 4).

### 4.2 Architecture

Stencil is a HolyC-native application (~12k lines for the path engine, ~6k lines for UI, ~4k lines for color management = ~22k LOC outside the 100k-line base-system budget). The path engine draws on:

- **Inkscape's `livarot/`**: concrete reference for boolean operations on Bezier paths via polygon approximation + sweep-line. Field OS replicates the `Path → Convert → Fill (directed graph) → ConvertToShape (no self-intersection) → ConvertToForme (back to path)` pipeline. Inkscape itself is moving away from livarot toward `lib2geom` with the `PathIntersectionGraph` / Greiner-Hormann approach; Stencil cherry-picks the Greiner-Hormann algorithm directly because it's cleaner and well-documented in the academic literature (Greiner & Hormann 1998).
- **Cairo's path engine** for stroke tessellation reference.
- **Skia's path engine** for performance patterns (precomputed analytic Bezier bounding boxes, flatness-adaptive subdivision).
- **Bezier curve mathematics:** Faux & Pratt, *Computational Geometry for Design and Manufacture*; Yamaguchi, *Curves and Surfaces in Computer Aided Geometric Design*.
- **Figma's WebGL rendering blog posts** for tile-based GPU compositing of vector content.
- **Affinity Designer's documented architecture** (live, non-destructive, GPU-accelerated) for product direction.

### 4.3 M111–M115

#### M111 — Vector engine architecture
Bezier path math: cubic and quadratic, rational support optional (deferred). Stroke profiles (variable width along path). Gradient compositing: linear and radial via GPU shaders, mesh gradients via CPU-side tessellation to triangles fed to the GPU. Boolean operations via Greiner-Hormann. Shape builder (Illustrator-style on-canvas geometry combination). ~12k lines HolyC.
**Exit criteria.** Path engine passes a 200-test conformance suite covering edge cases (self-intersection, degenerate beziers, t-parameter precision near 0 and 1).
**Effort.** 8–10 weeks.

#### M112 — SVG round-trip
Lossless SVG 1.1 read/write; SVG 2.0 subset (geometry-related additions). CSS-in-SVG (in-document `<style>` and `style=` attribute). Inkscape extension namespace preserved on import and re-emitted on export so Inkscape users can round-trip files through Stencil without losing Inkscape-specific data.
**Exit criteria.** Stencil round-trips a 50-document Inkscape corpus and a 50-document Illustrator corpus with byte-equality after canonicalisation.
**Effort.** 5–6 weeks.

#### M113 — AI / EPS / PDF import
Adobe Illustrator (.ai) files since CS2 are dual-format: a PDF base layer plus an Illustrator-private appended section. Import via the existing PDFium Cardboard Box from Manual; the Illustrator-specific section is parsed best-effort. EPS via a sandboxed Ghostscript Cardboard Box (GPL — strict isolation; output goes to an internal SVG that Stencil re-imports). PDF import reuses Manual's PDFium directly.
**Exit criteria.** 25-document AI/EPS/PDF corpus opens recognisably (no crashes; ≥90% visual fidelity).
**Effort.** 4 weeks.

#### M114 — Color management for print
CMYK colour space alongside RGB. ICC v4 profile management (LittleCMS-2 vendored as a Cardboard Box; LCMS is MIT-licensed, BSD-2 compatible). Soft proofing, separation preview, overprint preview. Colour-managed gradients. Reference: Scribus's colour engineering.
**Exit criteria.** A CMYK swatch round-trips through a print-target ICC profile with ΔE2000 <1.0 against a reference Scribus output.
**Effort.** 5–6 weeks.

#### M115 — Stencil v1 release
Pages-class document model: artboards with independent dimensions and bleed. Layers with opacity, blending modes, layer effects. Type on path. Library / symbol management (reusable instances; edits propagate). Signed and shipped to Stockpile.
**Exit criteria.** Stencil v1 in Stockpile. v1 release notes published. Internal test corpus of 50 Illustrator files round-trips losslessly to SVG.
**Effort.** 4–5 weeks of integration / polish.

### 4.4 Stencil track total
~6–7 months full-time / 12–14 months part-time. Scheduled **months 10–22 part-time / months 7–13 full-time**. Stencil and Sequence overlap; Stencil leads because vector is simpler and exercises more of the same UI paradigms (canvas-based pro app) that Sequence will reuse.

---

## 5. Sequence — Video Editor (M116–M120)

### 5.1 Name

Candidate set: **Sequence, Cut, Splice, Edit, Reel, Footage, Camera, Director, Tape, Take.** Recommendation: **Sequence.** It is what film editors actually call the timeline ("the main sequence", "an A-roll sequence"), it is MGS3-warm (a "sequence of operations" / "tactical sequence"), and it pairs cleanly with Stencil. Runner-up: **Reel** (more poetic; reserved for a possible future "Reel Engine" — the rendering core that Sequence sits on top of, mirroring how Resolve has "Fusion" as its compositor).

### 5.2 Architecture

Sequence is HolyC + a vendored FFmpeg Cardboard Box. The timeline NLE engine is HolyC-native (~15k LOC), similar to Cassette's MIDI/audio timeline from Phase 2. The codec pipeline is FFmpeg.

Key references:
- **OpenTimelineIO (OTIO):** the in-memory data model is `Timeline → Stack → Track → (Clip | Gap | Stack | Transition)` with `RationalTime` and `TimeRange` for opentime. Sequence's internal data model is **isomorphic to OTIO** so that import/export is a structure-preserving map, not a translation. This means Sequence is an OTIO-native NLE — a meaningful market positioning decision.
- **DaVinci Resolve color page architecture** as documented in Blackmagic whitepapers and the Filmlight Baselight publications.
- **FFmpeg's prores_kostya encoder** (Konstantin Shishkov 2012) — the canonical open-source ProRes encoder. ITU-R BT.709 / BT.2020 / BT.2100 colour science.
- **Final Cut Pro X XML** spec (Apple) and the FCP 7 XML format (used as Adobe Premiere's recommended interchange).

### 5.3 M116–M120

#### M116 — Timeline NLE engine
Multi-track video and audio. Edit operations: ripple, roll, slip, slide, blade, snap. Keyframe animation on clip properties (position, scale, opacity, volume, pan). Magnetic-timeline mode (FCPX-style) optional alongside traditional mode. ~15k LOC HolyC.
**Exit criteria.** A 200-clip 50-track timeline edits at 60 fps UI on M2 MacBook Air.
**Effort.** 10–12 weeks.

#### M117 — Codec pipeline
Hardware-accelerated decode for H.264, H.265, VP9, AV1 via VAAPI on AMD/Intel; via Adreno video acceleration on Snapdragon X; via VideoToolbox-equivalent through Honeykrisp's video extensions on Apple Silicon (best-effort — not all M1/M2 codecs are exposed yet by Asahi, so AV1 hardware decode on M1/M2 falls back to software). Encode for the same codecs.

ProRes encode/decode via FFmpeg's `prores_kostya` encoder (ProRes 422, Proxy/LT/SQ/HQ profiles 0–3, plus 4444). DNxHR via FFmpeg's `dnxhd` encoder. FFmpeg vendored as a Cardboard Box; it is LGPL-2.1 — Field OS's BSD-2 core is not contaminated because FFmpeg is in a sandbox accessed only via Field OS's stable codec ABI.

A canonical export command from Sequence's render farm:
```
ffmpeg -r 24 -i timeline.\u0004d.exr \
  -c:v prores_ks -profile:v 3 -vendor apl0 \
  -pix_fmt yuv422p10le -color_primaries bt709 \
  -color_trc bt709 -colorspace bt709 \
  out.mov
```

**Exit criteria.** A 1-hour 4K H.265 timeline exports in <real-time on M2 MacBook Air; ProRes 422 HQ export validates against Apple's reference decoder bit-for-bit on a 100-frame test clip.
**Effort.** 6–8 weeks.

#### M118 — Color grading
Primaries (lift / gamma / gain / offset). Curves (per-channel, luma, custom). Wheels (shadows / midtones / highlights). Working spaces: Rec.709, Rec.2020, log (S-Log3, Log-C, V-Log) with input transforms. LUTs: 1D and 3D, .cube and .3dl formats. Optional node-based grading graph (Resolve-style; v1 ships linear stack with nodes as a v2.x feature flag).
**Exit criteria.** A 32-shot grade matches a Resolve reference within ΔE2000 <2.0 on a standardised test sequence.
**Effort.** 8 weeks.

#### M119 — OpenTimelineIO + FCPXML interchange
Read/write OTIO native (.otio JSON). Read FCPX XML (.fcpxml). Read FCP 7 XML / Premiere XML (read-only). Implement as Sequence-native parsers in HolyC for OTIO and FCPX (the schemas are stable and tractable — OTIO is roughly 30 schema classes), plus a vendored Python OTIO via Tabernacle for the long tail of community adapters (CMX 3600 EDL, AAF, HLS playlist, ALE).
**Exit criteria.** A 100-clip Sequence project round-trips through OTIO with structural equality. A FCPX project from the OTIO test corpus opens with ≥95% fidelity.
**Effort.** 3–4 weeks.

#### M120 — Sequence v1 release
Compound clips (nest a sub-sequence as a clip). Proxy media workflow (auto-generate ProRes Proxy on import; transparently swap during playback, render at full res). Audio mixing with per-track effects. Transitions (cross-dissolve, dip-to-color, wipes — 12 in v1). Titles (basic, ≤12 templates; Stencil files can be imported as title overlays). Hardware encode for export. Signed and shipped to Stockpile.
**Exit criteria.** Sequence v1 in Stockpile. Internal "edit a 5-min short film" exercise completed by 3 testers with no critical bugs.
**Effort.** 5–6 weeks.

### 5.4 Sequence track total
~7–9 months full-time / 14–18 months part-time. Scheduled **months 18–34 part-time / months 11–17 full-time**, partly in parallel with Stencil's later milestones.

---

## 6. System Maturity (M121–M125)

#### M121 — Cellular (5G / LTE) via MBIM
**Scope.** Standard `cdc_mbim` (USB MBIM) and `mhi`-based PCIe MBIM (Snapdragon X built-in modems). Userspace `mbimcli`-equivalent vendored as a Patrol unit `comm.cellular` that drives the MBIM control channel and configures a `wwanX` interface. Comm Tower (Field OS's networking stack) handles routing.

The Surface Pro 11 5G uses a Snapdragon X65 modem on the SoC; the Linux qmi_wwan / cdc_mbim path is well-trodden. A Mac Mini M1 has no cellular, so this milestone is primarily for Snapdragon X tablets and Framework 13 with WWAN modules.
**Exit criteria.** SIM-based 5G connection on Surface Pro 11 5G hits ≥90% of Windows-on-the-same-hardware throughput.
**Effort.** 4–5 weeks.

#### M122 — Server / cloud edition profile
**Scope.** Field OS Server: a build profile that omits Stage, Wavelength, Stencil/Sequence, on-screen keyboard, and tablet code paths. Adds: SSH server (vendored OpenSSH Cardboard Box), `field-systemctl`-equivalent CLI for Patrol unit management, no graphical login (only `field-tty`), and a "headless installer" that consumes a JSON manifest. Useful for: Field Sync server hosting (Phase 2), Stockpile build farm, edge compute, Field-OS-on-Field-OS development VMs. The minimal install fits in <300 MB and idles <60 MB RAM.

Comparison: FreeBSD's jail architecture vs Field OS's Cardboard Boxes — Cardboard Boxes are roughly equivalent to jails plus a manifest-based capability declaration. The server profile makes this explicit by exposing `field-jail`-style commands.
**Exit criteria.** Field OS Server installs from a 200 MB ISO; idle RAM <60 MB; SSH login latency <500 ms; passes a 50-test server conformance suite.
**Effort.** 4 weeks.

#### M123 — WASI 0.3 / 1.0 Tabernacle update
**Scope.** WASI 0.3 previews are available in Wasmtime 37+; the official roadmap targets completion **around February 2026**, with WASI 1.0 production-stable targeted late 2026 / early 2027. WASI 0.3's headline is **native async** at the Component Model ABI level via `stream<T>` and `future<T>` types, replacing the WASI 0.2 polling model. By the time Phase 3 reaches this milestone (calendar 2027–2028 in realistic schedules), 0.3 should be stable and 1.0 shipping.

Update Tabernacles' embedded Wasmtime to the WASI-0.3-or-1.0 release. Provide a compatibility shim for v1.0-era WASI 0.2 Tabernacles — at minimum a deprecation warning and a one-release support window.

**Exit criteria.** All v1.0 Tabernacles continue to work via shim. New SDK targets WASI 0.3 / 1.0 by default. Async Tabernacles compose without callback ceremony (per the WASI 0.3 design).
**Effort.** 3–4 weeks.

#### M124 — Recon v3 (browser maturity)
**Scope.** Decision point. As of April 2026, Ladybird's published roadmap is Alpha 2026, Beta 2027, Stable 2028. Field OS's Recon currently uses Servo (Phase 1 baseline). At M124 (likely calendar 2027–2028), Field OS evaluates a switch to LibWeb if Ladybird's alpha is real and the engine has matured.

**Decision criteria for Recon v3 LibWeb switch:**
1. Ladybird passes ≥95% of Web Platform Tests (it was at ~90% in October 2025, fourth-highest after Chrome, Safari, Firefox).
2. JavaScript engine performance within 2× of V8 on Speedometer 3.
3. Memory footprint <800 MB on the Field OS standard 50-tab benchmark.
4. License (BSD-2) — already verified.
5. Compatibility on Field OS's top-100 test sites ≥98%.

If yes → switch. If no → stay on Servo and revisit in v2.x. Either way, WebGPU production-grade is a v2.0 commitment (Field OS's Foundry is exposed to web content). WebRTC if feasible from the chosen engine.

Extension API: WASM Tabernacle extensions, declared via a manifest mirroring WebExtensions but with capabilities as first-class.

**Exit criteria.** Recon v3 ships with the chosen engine; passes Field OS's site-compatibility suite; loads the 8-site standard benchmark in <5 s aggregate.
**Effort.** 6–10 weeks (much higher if engine switch).

#### M125 — Calling Card v2
**Scope.** Calling Card v1 (Phase 1) is local identity. v2 adds federation: link Calling Cards to other users for sharing, family/organisation roles. End-to-end-encrypted messaging primitive — choose **MLS (Messaging Layer Security)** over Signal protocol because MLS is RFC 9420 (IETF-standardised), has multiple open implementations, and is designed for groups (Brief libraries, family pools) without Signal's pairwise-and-then-fan-out awkwardness. Group keys for shared Brief libraries.
**Exit criteria.** Two Field OS users can establish an MLS session and share a Brief with E2EE in <2 s. Family pool of 6 users works.
**Effort.** 5–6 weeks.

---

## 7. Performance and Ecosystem (M126–M130)

#### M126 — Performance polish v2.0 ("Snow Leopard moment")
**Scope.** Dedicated performance pass. Profile-guided optimisation across Stage, Patrol, Brief, Wavelength, Comm Tower. Kernel hot path tuning: scheduler decisions in syscall fast path inlined; LinuxKPI shim layers reviewed for unnecessary indirection; Tabernacle cold start reduced via Wasmtime AOT cache (already a feature; tune cache eviction).

**Targets (measured on Tier-1 reference hardware: Framework 13 AMD Ryzen 7, M2 MacBook Air):**

| Metric | v1.0 baseline | v2.0 target |
|---|---|---|
| Cold boot (firmware-handoff → desktop) | 2.4 s | **≤1.5 s** |
| Login → desktop | 0.7 s | **≤0.4 s** |
| App cold launch (Cache, Stockpile, Frequencies) | 280 ms | **≤150 ms** |
| Recon to first paint (major web property) | 1.4 s | **≤0.8 s** |
| Idle RAM | 240 MB | **≤180 MB** |

**Exit criteria.** All targets met on both reference machines; CI publishes daily perf graphs.
**Effort.** 6–8 weeks of dedicated work, with continuous measurement throughout Phase 3.

#### M127 — Stockpile ecosystem maturity
**Scope.** Grow Stockpile to **≥100 third-party apps** with **≥50 active developers** (defined as ≥1 release in the last 90 days). Reviewer rotation: 5+ paid contractors handling submissions on a 3-business-day SLA. Featured-apps program with weekly editorial. Optional revenue sharing (Field OS takes 15%, comparable to F-Droid's 0% but enabling paid app developers; Field OS Foundation can match Stockpile revenue with developer grants).
**Exit criteria.** 100 apps, 50 active developers, SLA met on ≥95% of submissions.
**Effort.** Ongoing throughout Phase 3 — this is a developer-relations milestone, not a single block of code.

#### M128 — Field OS Conf
**Scope.** First annual Field OS conference. ~200–300 attendees. Single-track, two days, free + sponsor-supported. Talks: contributors, third-party developers, hardware partners. Live-streamed. Reference events: BeOSCon 1998, GUADEC, Akademy, Fedora Flock. Co-located with another OSS event in Year 1 to lower logistics burden (proposal: alongside All Systems Go in Berlin or LCA in the southern hemisphere).
**Exit criteria.** Conf held; recordings published; ≥80% attendee satisfaction.
**Effort.** 3 months of part-time event work, mostly in the final 6 months before the conf date.

#### M129 — Hardware partnerships
**Scope.** Formal partnership agreements:
- **Framework**: pre-installed Field OS as a build option on Framework 13 (AMD and Intel). Co-marketing.
- **System76**: Field OS image certified for the Lemur Pro / Pangolin lineup (where supported by hardware compatibility).
- **Tuxedo Computers**: similar to System76 in the European market.
- **Lenovo / Dell** (stretch): a single Snapdragon X reference machine certified with Field OS as a "developer edition" SKU.

Partner certification programme: hardware that meets a defined Field OS hardware compatibility spec earns the "Field-Certified" badge.
**Exit criteria.** ≥2 formal partnerships announced before v2.0 GA.
**Effort.** 2 months of part-time business development, spread over Phase 3.

#### M130 — v2.0 release
**Scope.** Cut the v2.0 branch. Marketing campaign: blog post, video tour (with Field OS Conf as launch venue), press outreach to Phoronix, Ars Technica, The Register, LWN. Launch-day Stockpile featured apps including Stencil and Sequence. SBOM published; license inventory verified; a **third-party security audit passed** (allocate budget for one auditor-week from a reputable Linux-kernel-adjacent firm, e.g. Trail of Bits or NCC Group).
**Exit criteria.** Acceptance criteria below (§9) all met.

---

## 8. Tooling Additions for Phase 3

### 8.1 Cross-architecture CI

Phase 2 ended with x86_64 (AMD + Intel) and ARM64 (Snapdragon X) CI. Phase 3 expands to:

- **Apple Silicon CI:** dedicated Mac mini M1 (8 GB) and Mac mini M2 (16 GB). Both run Field OS natively, polled by the CI controller (a Framework 13 AMD running Field OS Server). Boot tests, driver tests, Stencil/Sequence smoke tests run on every main-branch commit.
- **Tablet test rigs:** Surface Pro 11 (Snapdragon X Plus, 16 GB, OLED) and Lenovo ThinkPad X12 Detachable Gen 2. Manual gesture testing weekly; automated touch event injection nightly.
- **GPU compute rig:** AMD RX 7700 XT in a Framework 16 for VAAPI / Vulkan compute conformance.

### 8.2 Conformance suites

- **Vulkan CTS** for Foundry on AGX, Adreno, AMD, Intel.
- **SVG 1.1 + 2.0 W3C test suites** for Stencil.
- **Web Platform Tests** for Recon (continuous, comparing to the chosen engine's published WPT score).
- **OpenTimelineIO test corpus** for Sequence (interchange round-trip tests).
- **H.264/H.265/AV1/VP9 reference media** from Joint Video Team / AOMedia for codec correctness.
- **Apple ProRes reference clips** and the Avid DNxHR reference material for codec round-trip validation.

### 8.3 LOC budget tracking

Phase 0–2 set a 100,000-line budget for the base system (excluding drivers and apps). Phase 3 adds nothing structural to the base system other than tablet-mode Stage paths, on-screen keyboard, and IME architecture. Estimated base-system additions: ~9k LOC (3k tablet Stage, 1.5k OSK, 2k IME, 1k rotation/IIO, 1.5k MBIM userspace). Phase 3 ends the base system at ~85k LOC if Phases 0–2 spent ~76k. Stencil (~22k), Sequence (~30k), Apple Silicon drivers (~40k via LinuxKPI port — counted as drivers, not base) sit outside the 100k budget. **CI tracks daily LOC growth and fails the build on >2% week-over-week growth in the base system without a corresponding ADR (architecture decision record).**

---

## 9. v2.0 Definition of Done — Acceptance Criteria

A Field OS v2.0 GA build must satisfy **all** of the following, certified by automated CI plus human sign-off:

**Performance (measured on Tier-1 reference hardware):**
1. Cold boot ≤1.5 s
2. Login → desktop ≤0.4 s
3. App cold launch (Cache, Stockpile, Frequencies) ≤150 ms
4. Recon → first paint ≤0.8 s on a 10-site benchmark median
5. Idle RAM ≤180 MB

**Hardware (Tier-1 certification):**
6. Framework 13 AMD: full hardware support, all peripherals, suspend/resume.
7. Framework 13 Intel: same.
8. Snapdragon X reference machine (Surface Pro 11 + a clamshell, e.g. ASUS Vivobook S15 Snapdragon): full hardware support including pen and touch.
9. Apple Silicon M1/M2: full support on the seven reference Macs. Phase 1 acceptance bar passed (boot, login, suspend/resume, browser, mail, audio, video).

**Tablet experience:**
10. Touch latency <50 ms; pen latency <20 ms (camera-measured).
11. Tablet usability study scores ≥4/5 vs iPadOS on the same hardware (Surface Pro 11 dual-boot).

**Apps:**
12. Stencil v1 round-trips a 50-document Illustrator corpus losslessly (after canonicalisation).
13. Sequence v1 exports a 4K H.265 1-hour timeline in <real-time on Tier-1 hardware.
14. Recon v3 passes the Field OS site-compatibility suite (≥98%).

**Ecosystem:**
15. ≥10,000 daily-driver users active for ≥30 days each (instrumented via opt-in Patrol telemetry — privacy-respecting, aggregate counts only).
16. ≥100 third-party apps in Stockpile.
17. ≥50 active third-party developers.
18. Stockpile reviewer SLA ≤3 business days on ≥95% of submissions.

**Engineering:**
19. SBOM published.
20. License inventory verified (every vendored Cardboard Box has a recorded license matching its source).
21. Third-party security audit passed.
22. CI green on all Tier-1 hardware on the v2.0 release commit.

---

## 10. Skill-building / Reading List for Phase 3

**Apple Silicon:**
- Hector Martin's *Asahi Linux progress reports* 2021–2025, especially the January/February 2021 report (m1n1 architecture) and the 2024 OpenGL 4.6 / Honeykrisp Vulkan posts.
- Hector Martin's *"Passing the torch"* (Feb 2025) and the post-collective-governance progress reports through 2026 ("Linux 6.19", "Linux 7.0").
- The *m1n1 source tree* — read `payload.c`, `chainload.c`, the Apple Device Tree (ADT) parsing, and the proxy mode protocol.
- *Honeykrisp Mesa source* in the Mesa GitLab repository (`src/asahi/`).
- *Asahi Lina's blog posts* on the AGX Rust kernel driver — particularly the explanation of the Apple GPU's tile-based deferred renderer.
- *Alyssa Rosenzweig's blog* — "Vulkan 1.3 on the M1 in 1 month" is the technical heart of Honeykrisp.
- The *Asahi open OS interop spec* on `asahilinux.org/docs/platform/open-os-interop/`.

**Tablet:**
- *iPadOS Human Interface Guidelines* (Apple developer docs).
- *Surface design guidelines* (Microsoft Learn) for kickstand / detachable form factors.
- *Microsoft Pen Protocol* — Wikipedia article (the most accessible synthesis), Microsoft hardware design guidance for HID descriptors, and the Linux IPTS driver source.
- *Linux `wacom` driver source* for EMR / AES.
- *GNOME tablet experience proposals* (gitlab.gnome.org Design discussions).
- *KDE Plasma Mobile architecture* docs.

**Vector graphics:**
- *Inkscape source* — `livarot/`, particularly `Path.cpp` and `Shape.cpp`. The "Killing Livarot" wiki page documents the migration plan to lib2geom.
- *Cairo* path engine source.
- *Skia* path engine source (`src/core/SkPath.cpp`, `SkPathOps`).
- *Affinity Designer architecture* discussions (reverse-engineered from public Serif blog posts).
- Faux & Pratt, *Computational Geometry for Design and Manufacture* (1979 — still authoritative on Bezier mathematics).
- Yamaguchi, *Curves and Surfaces in Computer Aided Geometric Design* (1988).
- *Greiner & Hormann*, "Efficient Clipping of Arbitrary Polygons" (1998) — the boolean operations algorithm.
- Figma engineering blog: WebGL rendering posts.
- LittleCMS-2 (Marti Maria) source for ICC colour management.

**Video editing:**
- *DaVinci Resolve* architecture (reverse-engineered from public training material; SIGGRAPH presentations).
- *OpenTimelineIO specification* and Pixar's published architecture document.
- *FFmpeg internals* — the avformat / avcodec API, and `proresenc_kostya.c` / `prores_videotoolbox.c` source.
- ITU-R BT.709 / BT.2020 / BT.2100 specifications for colour.
- Apple's *ProRes whitepapers*.
- Avid's *DNxHR whitepapers*.
- Filmlight's Baselight technical publications on colour science.

**Cellular:**
- USB-IF *MBIM specification* (Mobile Broadband Interface Model).
- Linux kernel `cdc_mbim` and `qmi_wwan` source; kernel docs page.
- *ModemManager*, libmbim, libqmi source (Aleksander Morgado).

**Server:**
- Marshall Kirk McKusick et al., *The Design and Implementation of the FreeBSD Operating System* — the chapters on jails are the closest mainstream analogue to Cardboard Boxes for a server profile.

**WASI / WebAssembly:**
- WASI.dev *roadmap* and the *WASI 0.3 RC notes*.
- Bytecode Alliance Wasmtime documentation, particularly the Component Model and async support.
- The Component Model proposal text on GitHub.

---

## 11. Risk Register

| ID | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | Apple firmware update breaks DCP / GPU drivers | High | High | Pin firmware blob set; update only when Asahi has tested; ship "edge" kernels for forward porting |
| R2 | Asahi collective governance slows patch acceptance | Medium | Medium | Maintain a Field OS downstream tree; rebase regularly; only depend on already-merged work |
| R3 | Tablet experience scope creep | High | High | Hard milestone gates; voice dictation explicitly stretch; CJK IME a v2.0-MVP-then-v2.1 |
| R4 | Stencil and Sequence ship under-baked | Medium | High | Internal corpus tests (50-doc Illustrator, 5-min short film exercise); willingness to slip v1 by one release rather than ship broken |
| R5 | Solo-builder burnout in years 5–6 | High | Critical | Sabbatical built into schedule (1 month off after Apple Silicon track); deliberate slowdowns; community contributors for non-core (third-party app reviews, Stockpile editorial) |
| R6 | Ladybird/LibWeb 2026 alpha slips | Medium | Low | Recon v3 default-stays-on-Servo; LibWeb switch is opt-in, not a release gate |
| R7 | WASI 0.3/1.0 stabilisation slips | Medium | Low | Tabernacle update is a point release; ship v2.0 on whichever WASI is current and update in v2.x |
| R8 | Performance targets not met | Medium | High | Continuous perf CI from Phase 0; flag regressions per-commit; early-and-often profiling rather than end-of-phase crunch |
| R9 | Hardware-partner negotiations fall through | Medium | Medium | Two-partner threshold for v2.0 is conservative; even one partner is shippable |
| R10 | Apple legal action against Asahi or downstream | Low | Critical | Strict adherence to Asahi's clean-room methodology; never use Apple-derived code; legal review of installer flow before v2.0 |

---

## 12. Phase 4 Preview (Motivating Phase 3 Discipline)

Phase 4 (M131–M170) is plausibly **24–36 months part-time / 12–18 months full-time** after v2.0 GA. Probable scope:

- **M3 / M4 Apple Silicon support** (when Asahi's M3 work matures and M4 reverse engineering completes).
- **Apple Pencil 2** support (requires Apple's MPP custom variant, which Asahi has not fully reverse-engineered as of 2026).
- **3D modeling app** (Blender-class) — name TBD from the MGS3 set; *Mortar*, *Workshop*, *Foundry* (collision with Vulkan API; rule out), *Diorama* are candidates.
- **CAD app** (Fusion 360-class) — name TBD; *Plot*, *Schematic* (now available since Stencil claims a different name), *Drafting Table* are candidates.
- **HFS+ / APFS read-only support** for Mac dual-boot scenarios.
- **GPU compute parity with CUDA / Metal Performance Shaders** — Engine v2 with first-class ML primitives.
- **Spatial computing** (Vision Pro-class XR) — highly speculative; depends on whether the form factor stabilises into something a solo project can credibly target.
- **Touch ID / Secure Enclave** integration once Apple SEP is sufficiently understood.

Naming Phase 4 here serves Phase 3 discipline: each item that "could be in v2.0" is explicitly pushed forward, and the Phase 3 plan is allowed to be smaller and shippable.

---

## 13. Closing

Phase 3 is the phase where Field OS becomes plural. Two architectures become three. One form factor becomes two. Two pro apps (Manual + Armory + Cassette + Negatives v2 from Phase 2) become four (add Stencil + Sequence). The base system gets a "Snow Leopard" pass that earns the right to call itself fast. The community becomes large enough to have its own conference. Hardware partners ship machines with Field OS as a build option.

The HolyC universal-language commitment, the Brief executable-document format, the F5 hot-patch, source-as-documentation, the BSD-2 core, the Big Sur visual identity, the MGS3-warm tactical naming — all preserved unchanged. The 100k-line base-system budget holds. The single-language base remains: HolyC for kernel, supervisor, compositor, runtime, document format, shell, file manager. Drivers and ported third-party libraries keep their C / C++ / Rust as before.

Phase 3 deliberately defers the truly speculative (spatial computing, M4, Touch ID, 3D, CAD) to Phase 4. That discipline is what makes v2.0 reachable inside 24–36 months part-time. v2.0 is not the end — macOS Big Sur to Tahoe is a five-year refinement era; Field OS will have its own. But v2.0 is the moment Field OS becomes a **credible third desktop operating system** alongside macOS and Windows for a meaningful user population — and it is reachable from where the project stands at v1.0.

The work is hard. The schedule is honest. The plan is real.