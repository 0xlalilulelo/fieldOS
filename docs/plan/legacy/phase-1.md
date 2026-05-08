# Field OS — Phase 1 Engineering Plan
## "From a QEMU PoC to a Daily Driver on Real Silicon"

> Phase numbering continues from Phase 0 (M0–M10). Phase 1 covers M11–M50, ending in the public **v0.1** release: a Field OS install that a stranger can use as a daily driver on a Framework 13 AMD for one full week without going back to their previous OS.

---

## 0. Executive Summary

Phase 0 closed with a believable QEMU demo: HolyC compiler on bare metal, the Brief document renderer in a software-composited window, PS/2 input, BGA framebuffer, Patrol v0 preemptive scheduler, Cache v0 file manager. That demo is a *promise*, not a *product*. Phase 1 is where the promise meets real silicon, real users, and real timelines.

The honest framing is this: **Phase 1 is the year-2-to-year-3 of a from-scratch desktop OS.** SerenityOS reached "from zero to HTML" in one year of mostly full-time solo work (October 2018 → October 2019, with a paid job in the second half), and even with hundreds of contributors it took until *year five* before sdomi began the very first serious push to run SerenityOS on a single Chromebook. Asahi Linux took roughly 15 months from project announcement (December 2020) to a usable alpha installer (18 March 2022) — and that was a funded, multi-person team standing on Linux's full driver and userspace tree. Asahi Lina's first GPU kernel-mode driver shipped in December 2022, two years in; conformant Vulkan 1.3 shipped on the M1 in mid-2024, four years in. Redox OS, after a decade of work, is still landing dynamic linking and a stable `relibc` ABI in 2025–2026 as preconditions for its 1.0.

Field OS Phase 1 is therefore calibrated against this reality, not against marketing. With Phase 0 having taken **12–18 months part-time / 5–7 months full-time**, Phase 1 is plausibly:

- **18–30 months at 15 h/week**, or
- **9–15 months at 35 h/week**,

with the central span being roughly **24 months part-time / 12 months full-time**. Anyone quoting less is selling the soft tissue of QEMU as if it were silicon.

The single largest force-multiplier in Phase 1 is the **LinuxKPI strategy**: porting AMDGPU, i915/Xe, iwlwifi, mt76, snd_hda_intel, and xhci-hcd from the Linux 6.12 LTS tree behind a thin HolyC compatibility shim, in the same lineage as FreeBSD's `linuxkpi`/`drm-kmod`, Haiku's FreeBSD compatibility layer, NetBSD's drm port, and Genode's DDE Linux. AMDGPU alone is over 5 million lines in `drivers/gpu/drm/amd/` as of Linux 6.6 (Phoronix, 2023); a solo builder cannot rewrite this. The LinuxKPI shim is how Field OS gets working hardware in calendar time that one human can survive.

The "definition of done" for Phase 1 is concrete: a fresh Framework 13 AMD Ryzen 7040, USB installer, ten minutes, working Wi‑Fi/Bluetooth/audio/display/keyboard backlight/external HDMI, ≤2.5 s cold boot, ≤0.8 s login‑to‑desktop, ≤220 MB idle RAM with Patrol+Stage+Wavelength+Comm Tower running, suspend/resume reliable across 24 cycles, Recon loads gmail.com and youtube.com, Dispatch sends and receives mail through Gmail IMAP, Projector plays a 4K H.265 file with hardware decode, VoiceOver-equivalent screen reader navigates the system without visual reference, and the entire install fits in 6 GB.

---

## 1. Phase 1 Scope and Explicit Non-Goals

### 1.1 What ships in v0.1

| Layer | Phase 1 deliverable |
|---|---|
| Boot | Limine UEFI (the Limine boot protocol is the reference for `BOOTX64.EFI` + `limine.conf`) chain-loading the Field OS kernel; ACPI table parse via ACPICA |
| CPU | x86-64 only; AMD Ryzen 7040 / Intel Core Ultra; SMT-aware scheduler; full preempt; per-CPU run queues |
| Storage | NVMe 1.4, AHCI; RedSea II R/W; ext2 R/O; NTFS R/O |
| GPU | AMDGPU + i915/Xe ported via LinuxKPI; Foundry v1 Vulkan-shaped API; Stage v1 GPU compositor |
| Net | Comm Tower v1 (lwIP behind HolyC binding) + BearSSL TLS 1.2 + DNS resolver; iwlwifi + mt76 ported; mac80211 ported; Bluetooth HCI minimal |
| Audio | Wavelength v1 over Intel HDA + USB Audio Class; <8 ms round-trip target |
| Sandbox | Cardboard Box v1 + capability broker as a Patrol unit |
| Pkg | Stockpile v1 (.fbox bundles) + Patch (atomic snapshot updates) |
| UI | Vector text via FreeType + HarfBuzz; IBM Plex Sans/Mono/Serif (SIL OFL 1.1); Field Symbols icon set (Lucide ISC fork + Field glyphs); four material classes; 8/12/20 px corner radii; 4 px grid |
| Apps | Recon, Dispatch, Roster, Schedule, Negatives, Frequency, Projector, Cure, Survival Kit, Stockpile, Camo Index, Codec, Listening Post |
| A11y | VoiceOver-equivalent screen reader, Dynamic Type 80–250%, Reduce Motion, Reduce Transparency — gating the release |
| Power | S3 reliable; S0ix Tier-1 only on the specific reference hardware |

### 1.2 Explicit Phase 1 non-goals (deferred to Phase 2)

- ARM64 (no Apple Silicon, no Snapdragon, no RPi5) — Phase 2.
- NVIDIA. No Nouveau, no proprietary blob. Phase 1 is a **deliberate AMD/Intel monoculture**.
- Internal webcams beyond UVC class. Most laptop IPU cameras require massive vendor pipelines (Intel IPU6/MIPI-CSI2). Phase 2.
- Thunderbolt/USB4 PCIe tunneling. Phase 2.
- ext4, btrfs, exFAT R/W, NTFS R/W. Phase 2.
- Wi-Fi 7 (BE200), Bluetooth LE Audio (the MT7922 still has open issues here on Linux), MIDI 2.0 networked transport.
- Polyglot story (Rust/Zig/Python/Go via WASM Tabernacles). Phase 2.
- DAW-class audio editing, RAW pipeline with full color-management. Phase 2.
- Armory (LSP-class IDE) and Manual (document editor). Phase 2.
- HDR / wide-gamut display path. Phase 2.
- Discrete GPU (any RDNA3+ dGPU on a desktop) is *opportunistic* — same driver, but tested on the iGPU first.

### 1.3 The "definition of done" — single-sentence form

> *A volunteer who has never seen Field OS before walks into the room with a fresh Framework 13 Ryzen 7 7840U and a USB stick, installs Field OS in under ten minutes, uses it as their only computer for seven consecutive days — including web, email, music, video, sleep/wake on a charger, sleep/wake on battery, and an external HDMI monitor — and at the end of the week reports zero data loss, no panics, and no need to reach for their previous OS.*

Everything else in Phase 1 — every milestone, every shim, every line of HolyC — is in service of that sentence.

### 1.4 Realistic timeline

For the engineer reading this: SerenityOS, with Andreas Kling working full-time then full-time-with-day-job, took roughly **two years of dedicated effort** to render HTML and run a credible app suite *in QEMU only*. SerenityOS only began booting on a single Chromebook in 2024–2025. Asahi Linux, with Hector Martin and Alyssa Rosenzweig and Asahi Lina full-time on funding, took **15 months** from kickoff to alpha installer and **24 months** to alpha GPU acceleration. Redox, ten years in, is still landing dynamic linking. Haiku's R1 has been "almost there" since 2009.

Field OS Phase 1 is calibrated as follows:

| Cadence | Range | Central estimate |
|---|---|---|
| 15 h/week (part-time, evenings + weekends) | 18–30 months | **24 months** |
| 35 h/week (sabbatical / funded full-time) | 9–15 months | **12 months** |

This is a *gross* number that already absorbs the **real-hardware shock**: the four to eight weeks (full-time-equivalent) of regression discovery the first time an OS that ran cleanly under QEMU is asked to boot on real silicon. Every solo OS project has rediscovered that QEMU is a kind lie. ACPI tables on real machines have non-trivial AML; PCIe enumeration order is not deterministic; SATA controllers have pre-OS handoff quirks; the BGA framebuffer in QEMU is *nothing* like a real GOP framebuffer with PSR2; the hidden 24 MHz from `pmc_core` debugfs that decides whether you reach S0ix at all is not in QEMU. Budget the shock; do not pretend it isn't there.

---

## 2. Milestone Breakdown (M11 → M50)

Each milestone has a name, scope, exit criterion, dependencies, and an effort estimate in **full-time-equivalent weeks (FTE-w)**. Multiply by ~2.3 for 15 h/week part-time real time.

### M11 — Real-Hardware First Boot (Framework 13 AMD)
- **Scope.** Limine UEFI image deployed to `\EFI\BOOT\BOOTX64.EFI`; `limine.conf` with `protocol: limine`; HolyC kernel built per the Limine bare-bones template (request markers in `.limine_requests`, base revision 6); GOP framebuffer initialized from the Limine-provided framebuffer response; serial console over USB‑C debug cable for kernel printk; AMD Ryzen 7040 microcode load; ACPI RSDP parsed but not yet interpreted.
- **Exit criterion.** "Hello, Field" rendered on the actual 13" 2.8K Framework display, with a live serial console reachable from a developer Mac/Linux via `screen /dev/ttyUSB0 115200`.
- **Dependencies.** Phase 0 kernel; Limine 9.x or later.
- **Effort.** 2–3 FTE-w. The shock starts here. Things that worked in QEMU (CPUID order, the 8259 vs APIC default, the BGA's lying linear framebuffer) will break in subtle ways. Budget a full week just to fight the framebuffer alone.

### M12 — ACPI Subsystem (ACPICA Port)
- **Scope.** Port ACPICA (Intel-licensed *or* dual BSD/GPL; we choose **BSD**, which is OSI-approved and used by FreeBSD/NetBSD/Haiku/Genode for the same reason). Bring up: RSDP→XSDT→DSDT/SSDT, AML interpreter, `_OSI`, `_OSC`, `_PIC` (route GSIs through the IO‑APIC), `_PSx` (device power states), `_BAT`/`_BIF` battery, `_TMP`/`_PSV` thermal, `_FAN` fan. Implement the OS-services-layer (OSL) glue in HolyC: memory, mutexes, spinlocks, timers, IRQ install, port/memory I/O, semaphores, cache.
- **Exit criterion.** Field OS boots, parses every DSDT on the four reference machines, exposes `/sys/acpi/battery/state` to userspace, and successfully transitions a single `_PS0`/`_PS3` cycle on the audio codec.
- **Dependencies.** M11.
- **Effort.** 4–6 FTE-w. ACPICA itself is ~150 kLOC C; the OSL is ~2 kLOC of careful HolyC.

### M13 — NVMe Driver
- **Scope.** From-scratch HolyC NVMe 1.4 baseline. Single admin queue + single I/O queue first, then per-CPU I/O queues; PRP lists; namespace enumeration; basic SMART; identify-controller and identify-namespace; Namespace Multi-path passthrough for namespace 1.
- **Exit criterion.** RedSea II mounts on the laptop's WD SN850X; sequential read ≥3 GB/s; 4K random read ≥300 k IOPS at QD32 on the Tier-1 NVMe.
- **Dependencies.** M11. PCIe enumeration in M11.
- **Effort.** 3–4 FTE-w. The NVMe spec is one of the cleanest hardware specs ever published; this is the rare driver that's *easier* than ACHI.

### M14 — AHCI Driver
- **Scope.** SATA 3.x AHCI controller driver for the desktop reference recipe. NCQ, hot-plug, port-multiplier optional.
- **Exit criterion.** A Samsung 870 EVO mounts and survives `dd if=/dev/zero of=… bs=1M count=8192`.
- **Dependencies.** M11.
- **Effort.** 2 FTE-w.

### M15 — xHCI USB Driver
- **Scope.** USB 3.x via xHCI 1.2; transfer rings (control, bulk, interrupt, isochronous-deferred); event ring; port reset; device enumeration; LPM L1/L2.
- **Exit criterion.** A USB-A port on the Framework expansion card enumerates a USB 3 stick at SuperSpeed (5 Gb/s); a USB 2 keyboard works through it; isochronous reserved for M24.
- **Dependencies.** M11. PCIe MSI-X.
- **Effort.** **8–10 FTE-w.** This is the single most painful driver in Phase 1. Reference: the OSDev wiki's xHCI page, Haiku's `src/add-ons/kernel/busses/usb/xhci.cpp`, and Redox's `xhcid`. Do not attempt to compress this estimate.

### M16 — USB HID Class Driver
- **Scope.** Boot keyboards and mice; report descriptor parser; HID over USB; multi-touch (HID Touchscreen) optional; trackballs and Logitech receivers.
- **Exit criterion.** External keyboard, external mouse, and a Logitech Unifying receiver all attach via M15 and produce events on Patrol's input bus.
- **Dependencies.** M15.
- **Effort.** 3 FTE-w.

### M17 — AMDGPU Port (LinuxKPI)
- **Scope.** Port the Linux 6.12 LTS `drivers/gpu/drm/amd/` tree behind the Field OS LinuxKPI. **Phase 1 target: KMS only.** Mode-set, framebuffer scanout, page-flips, hot-plug detect, DPMS, basic atomic commits, GPU memory manager (TTM stub), DC display engine for RDNA3 iGPUs only. The userspace command-submission UAPI is implemented, but Vulkan-class command graphs come in M19.
- **Strategy.** Apply the exact pattern proven by FreeBSD's `drm-kmod`: keep the Linux source tree as a *vendored copy* with `#ifdef __linux__`/`#else` brackets at every divergence point; do not rewrite, do not refactor, do not modernize. The LinuxKPI shim provides every primitive the Linux drm-helpers expect (see §3 below).
- **Exit criterion.** External 4K HDMI display lights up at native resolution; vsync-locked page-flip at the panel's refresh rate; brightness control via `_BCM`; 8-bit color works; no kernel logspam during a five-minute run.
- **Dependencies.** M11, M12, M28 (Linux interrupt/work-queue surface), and the LinuxKPI itself.
- **Effort.** **10–14 FTE-w** *after* the LinuxKPI is up. Without LinuxKPI it would be a multi-year solo project (5 M LOC, Phoronix 2023).

### M18 — i915/Xe Port (LinuxKPI)
- **Scope.** Same shim approach, applied to `drivers/gpu/drm/i915/` and `drivers/gpu/drm/xe/`. Lower priority because the Tier-1 reference is Framework 13 AMD.
- **Exit criterion.** Framework 13 Intel Core Ultra (Meteor Lake / Xe-LPG) lights up to native resolution; same QC bar as M17.
- **Dependencies.** M17 (LinuxKPI maturity).
- **Effort.** 6–8 FTE-w (most of the heavy lifting is already amortized into M17).

### M19 — Foundry v1 (Vulkan-class graphics API)
- **Scope.** A HolyC-callable graphics API in the *shape* of Vulkan, not Vulkan-spec-conformant. It exposes: instances, devices, queues (graphics + compute + transfer), swapchains over the KMS framebuffer, command buffers, render passes, descriptor sets, pipelines, shader modules consuming SPIR‑V (vendored SPIRV-Tools port), and fences/semaphores. Compute and graphics share descriptor binding semantics. Synchronization is explicit (Asahi's "explicit-sync" stance — Lina's *Paving the Road to Vulkan on Asahi Linux*, March 2023 — is the design north star here: do not inherit Linux's implicit-sync world).
- **Exit criterion.** Stage v1 (M20) renders the entire desktop through Foundry; a sample triangle, a textured quad, and a Gaussian blur compute shader all run on RDNA3 and Xe-LPG.
- **Dependencies.** M17, M18.
- **Effort.** 12–16 FTE-w. The honest estimate; Asahi's experience shows even "Vulkan-shaped" wrappers eat months.

### M20 — Stage v1 (GPU-Accelerated Compositor)
- **Scope.** Replace Phase 0's software framebuffer compositor. The four material classes (chrome, panel, sheet, overlay) are real-time blurs (Kawase or compute-shader Gaussian, ~6 ms budget at 2.8 K). Rounded corners are antialiased on the GPU (signed-distance-field shader). Animations run at the panel's native refresh, including 120 Hz on the Framework's high-refresh option. Vibrancy reads the layer beneath through a precomputed blur LUT, refreshed only when the underlying content invalidates.
- **Exit criterion.** Open ten Brief documents, drag windows, watch animations stay locked to vsync, GPU utilization <30% on idle desktop, frame budget never exceeds 8 ms during interactive use.
- **Dependencies.** M19.
- **Effort.** 6–8 FTE-w.

### M21 — Vector Text Rendering
- **Scope.** Vendor FreeType 2.x (FTL/GPLv2 dual license — choose **FTL**, which is BSD-compatible) and HarfBuzz (MIT). HolyC bindings for `FT_Init_FreeType`, `FT_New_Memory_Face`, `FT_Set_Char_Size`, `FT_Load_Glyph`; HarfBuzz `hb_buffer_t` and `hb_shape`. IBM Plex Sans, Mono, and Serif (SIL OFL 1.1; reserved-name discipline observed). Subpixel positioning. CTL (complex text layout) for at least Latin, Cyrillic, Greek; CJK and RTL deferred to Phase 2.
- **Exit criterion.** Brief documents render at any size with proper kerning; the entire system uses no bitmap fonts; the Field Manual reads at SF-class quality on the 2.8 K panel.
- **Dependencies.** M19 (the rasterizer's glyph atlas lives in GPU memory).
- **Effort.** 4 FTE-w.

### M22 — Field Symbols Icon Set
- **Scope.** Fork Lucide (ISC license — MIT-compatible, derived list explicitly enumerated on lucide.dev/license) and add ~80 Field-specific glyphs: Codec, Camo Index, Patrol, Cache, Cure, Stamina, Cardboard Box, Channel, Frequency, Listening Post, Briefing, Field Symbols, Survival Kit, Comm Tower, Wavelength, Foundry, Engine, Stockpile, Operator, Calling Card, Roster, Schedule, Recon, Dispatch, Negatives, Projector, Manual. 24 px stroke grid, three weights (Regular 1.5 px, Medium 2 px, Bold 2.5 px), four optical sizes (16/20/24/32). SVG via NanoSVG (zlib license).
- **Exit criterion.** All system surfaces use Field Symbols at the correct optical size; no bitmap icons remain anywhere in the install.
- **Dependencies.** M21.
- **Effort.** 3 FTE-w (mostly drawing time; bring an iPad).

### M23 — HD-Audio Driver (Intel HDA)
- **Scope.** Port `snd_hda_intel` + `snd_hda_codec_realtek` + `snd_hda_codec_hdmi` from Linux 6.12 LTS via LinuxKPI. Codec quirks tables for the Tier-1 machines: Realtek ALC295 (Framework 13 AMD), Realtek ALC285 (X1 Carbon Gen 12), Cirrus CS35L41 amplifiers (Framework Intel Ultra). Internal speakers + headphone jack auto-mute + jack sense + microphone array.
- **Exit criterion.** A FLAC plays through internal speakers, switches to a headphone jack on insert with no pop, and back; mic array records; HDMI audio works through M17.
- **Dependencies.** M12 (codec gets `_PS0` from ACPICA), LinuxKPI.
- **Effort.** 4 FTE-w. Recommend tackling this **first** as the LinuxKPI smoke test (it's smaller than AMDGPU; if the shim doesn't work for HDA it definitely won't work for AMDGPU).

### M24 — USB Audio Class Driver
- **Scope.** UAC1 + UAC2 (UAC3 deferred). Isochronous endpoints on M15. Topaz/USB DACs, headsets, USB microphones.
- **Exit criterion.** A class-compliant DAC enumerates and plays at 96 kHz/24-bit; a USB headset's mic shows up on Wavelength's input bus.
- **Dependencies.** M15.
- **Effort.** 3 FTE-w.

### M25 — Wavelength v1 (Audio Server)
- **Scope.** HolyC audio server analogous to PipeWire (see PipeWire's *Design* document — graph-based, low-latency, SPA plugin nodes). Pure HolyC, *not* a port. One graph; nodes are HolyC closures; ports are typed (PCM int16/int24/int32/float32, MIDI 1.0 byte stream, MIDI 2.0 UMP). Hard real-time priority via Patrol's FIFO class. Single resampler at the device boundary (SoX-class via `soxr` vendored, LGPL → choose libsoxr-lsr replacement or write minimal polyphase). Per-stream volume; system mixer; per-app routing. Target: <8 ms round-trip on commodity hardware at 48 kHz / 128-frame buffer (PipeWire achieves sub-10 ms with quantum=128 at 48 k; we adopt the same).
- **Exit criterion.** Frequency plays a FLAC through Wavelength → snd_hda → speakers; Codec mixes a notification ping over the music without crackle; round-trip latency measured at <10 ms (Wavelength ships its own loopback test, modelled on PipeWire's `pw-mon`).
- **Dependencies.** M23, M24.
- **Effort.** 6 FTE-w.

### M26 — iwlwifi Port (LinuxKPI)
- **Scope.** Port `drivers/net/wireless/intel/iwlwifi/` from Linux 6.12 LTS for AX210/AX211. Firmware blobs ship in `/lib/firmware/iwlwifi-*` exactly as Linux ships them. WPA3-SAE.
- **Exit criterion.** Framework 13 Intel Core Ultra associates with a WPA3 network and pulls 800 Mbit on a 2 m line-of-sight test.
- **Dependencies.** M28, LinuxKPI maturity.
- **Effort.** 4 FTE-w (after mac80211/cfg80211).

### M27 — MT7921/MT7922 Port (LinuxKPI)
- **Scope.** Port `drivers/net/wireless/mediatek/mt76/mt7921/` for the AMD/MediaTek RZ616 (the chip in the Framework 13 AMD; the Framework community has tracked instability and FLR-readiness issues — *not ready Nms after FLR* — on this part since launch, see the Framework community thread "Unstable and unreliable WLAN RZ616/MT7922"; the workaround `rtc_cmos.use_acpi_alarm=1` and the MT7921 firmware update path must be carried).
- **Exit criterion.** Framework 13 AMD associates with WPA3 networks; FLR-readiness regression does not surface in the test rig over a 24-hour soak.
- **Dependencies.** M28, LinuxKPI maturity.
- **Effort.** 4 FTE-w. Bluetooth co-existence is in M30.

### M28 — 802.11 mac80211 + cfg80211 Port (LinuxKPI)
- **Scope.** Port the Linux mac80211 framework — the substrate both `iwlwifi` and `mt76` ride on. nl80211/cfg80211 for userspace via a HolyC-native control socket. Regulatory database (`wireless-regdb`).
- **Exit criterion.** Comm Tower's `radar` CLI lists scan results from both vendors on the same code path.
- **Dependencies.** LinuxKPI minimal skeleton.
- **Effort.** 6 FTE-w.

### M29 — Comm Tower v1 (TCP/IP + DNS + TLS)
- **Scope.** Three architectural choices were considered:
  1. **lwIP** (BSD-3-Clause, ~50 kLOC, embedded-grade, mature) → recommended.
  2. **smoltcp** (MIT/Apache, Rust, modern) → would force a Rust toolchain into the kernel-adjacent path.
  3. From-scratch in HolyC → ~30 kLOC budget; correct long-term answer; not a Phase 1 answer.

  **Recommendation: ship lwIP behind a HolyC binding for Phase 1; long-term goal is option (3) in Phase 3.** TLS via **BearSSL** (MIT; explicitly designed for embedded systems with no `malloc`; constant-time crypto; ~25 kB code/RAM minimal server; stable through TLS 1.2 — TLS 1.3 still pending in BearSSL itself, see `bearssl.org/goals.html`). If TLS 1.3 is mandatory for Phase 1 (it is — gmail, youtube, fastly), choose between (a) **rustls** behind a C ABI and a Rust toolchain in the build, or (b) a careful subset of **OpenSSL 3.x** as a fallback. The honest call: **rustls via the FFI** for the daily-driver acceptance test, with BearSSL retained as the small embedded fallback. DNS resolver is from-scratch HolyC (~1 500 LOC): UDP/53, TCP/53 fallback, DoH deferred to Phase 2.
- **Exit criterion.** `radar https://gmail.com` returns the gmail HTML; full HTTPS to `youtube.com` works; `dispatch` connects to Gmail IMAP-over-TLS.
- **Dependencies.** M26 or M27.
- **Effort.** 6–8 FTE-w (most of which is the TLS choice and integration).

### M30 — Bluetooth HCI
- **Scope.** Bluetooth 5.0+ via standard Host Controller Interface over USB (the HCI transport is well-documented and uniform across vendors; Asahi's Bluetooth story shows the hard part is firmware loading, not HCI). HID profile (keyboards, trackballs, mice) and A2DP sink (headphones) only. AVRCP basic. **LE Audio is not in Phase 1 scope** — the MT7922 controller has known crash issues on `LE Setup Isochronous Data Path` per the Framework community, and macOS-class LE Audio is nowhere in the open-source world yet.
- **Exit criterion.** AirPods Pro pair, play A2DP audio through Wavelength, and survive a sleep/wake cycle.
- **Dependencies.** M15, M25, M27 (for the AMD shared radio).
- **Effort.** 4 FTE-w.

### M31 — ACPI Power Management (S3 + S0ix)
- **Scope.** S3 (suspend-to-RAM) on every Tier-1 machine. **S0ix** (Modern Standby; on Linux this is `s2idle`) on the specific Tier-1 reference machines after Intel's S0ixSelftestTool and AMD's `amd-debug-tools` confirm residency. The Asahi `s2idle` November 2022 progress report is the canonical reverse-engineering chronicle — Asahi explicitly chose s2idle first because it does not require platform firmware suspend, and only began considering "true" S3-equivalent later. Field OS adopts the same staircase: s2idle first, deeper states behind further work.
- **Implementation details.** Freeze userspace; force devices to D3hot via ACPI `_PS3`; quiesce DMA; drive PMC via `intel_pmc_core` debugfs interface for S0ix substate residency on Intel; for AMD, bring up the AMD STB / SMU mailbox interface as exposed by Linux 6.12. Patrol's power supervisor exposes `/sys/power/state` and `/sys/power/s0ix_residency` to userspace.
- **Exit criterion.** Framework 13 AMD enters s2idle, idles for 8 hours on battery, and resumes successfully ≥24 consecutive cycles. Battery loss budget: ≤1.5%/hour while in s2idle (reference: Asahi M2 ~2%/hour was considered too high; we set the bar tighter on x86 because PC-class S0ix is more mature).
- **Dependencies.** M12, M17/M18, M23, M26/M27.
- **Effort.** **10–14 FTE-w.** Honest framing: this is the single biggest non-Apple-OS pain point in PC hardware. Tier‑2 for S0ix in Phase 1; Tier‑1 only on the specific reference machines tested.

### M32 — RedSea II Filesystem
- **Scope.** Copy-on-write descendant of TempleOS RedSea. Snapshots (per-volume), clones, file-level encryption (AES-256-XTS), per-block checksums (xxh3), 64-bit space, sparse files. Studied references: the APFS *Reference* (Apple Developer, 2018), the ZFS COW paper, Dominic Giampaolo's *Practical File System Design* (BFS), and Btrfs's tree-of-trees layout. Target ~3 000 LOC HolyC for the core; ~1 500 LOC for ext2 R/O; ~2 500 LOC for an NTFS R/O subset (covers the system-volume read case).
- **Phase 1 ships.** Read/write RedSea II; read-only ext2; read-only NTFS. **Defer:** ext4, btrfs, exFAT R/W. The decision to defer exFAT *write* is uncomfortable but correct: Microsoft's exFAT specification is now royalty-free but the Linux exFAT driver (`drivers/fs/exfat/`) is GPL-only and not LinuxKPI-hostable; rewriting in HolyC for Phase 1 is wasted budget.
- **Exit criterion.** RedSea II survives the standard `fstest` suite; a `tar -xf linux-6.12.tar.xz` extracts cleanly and the snapshot taken before extraction can be cloned back; an ext2 USB stick mounts read-only; a Windows NTFS partition mounts read-only.
- **Dependencies.** M13, M14.
- **Effort.** 8–10 FTE-w.

### M33 — Cardboard Box v1 (Sandboxing + Capability Broker)
- **Scope.** Capability container for third-party binaries. Manifest-driven; the manifest enumerates filesystem-subtree capabilities (a directory subtree of read or write access; pattern is OpenBSD's `unveil`), network-endpoint capabilities (host:port pairs; OpenBSD `pledge "inet"` is too coarse — Cardboard Box names endpoints), and hardware-class capabilities (camera, microphone, location, USB device class). Capabilities are typed file descriptors in the FreeBSD Capsicum sense. The Cardboard Box broker is itself a Patrol unit; it brokers IPC requests via Channel.
- **Reference architecture.** FreeBSD Capsicum (Watson et al., Cambridge, "Capsicum: practical capabilities for UNIX") for the FD-as-capability model; OpenBSD `pledge`/`unveil` for the per-application syscall-class restriction; Fuchsia Zircon handles for the typed-handle model. We borrow from all three without adopting any of them whole.
- **Exit criterion.** Recon (M37) runs in a Cardboard Box that can talk to ports 80/443 of any host, can read fonts and the `~/Downloads` subtree only, cannot enumerate USB, and cannot open a microphone. An attempted `open("/etc/passwd")` from the engine returns `ECAPSICUM`.
- **Dependencies.** M11–M16.
- **Effort.** 6 FTE-w.

### M34 — Stockpile v1 (Package Manager)
- **Scope.** Single-file `.fbox` packages: a signed Cardboard Box bundle = manifest + sandboxed binary tree + capability declarations + signature. Apple's invisible-update model is the UX target; Flatpak's manifest discipline is the engineering target; Haiku's `pkgman` is the lightweight comparator. Local-first install (`stockpile install ./Recon-1.0.0.fbox`); remote repository optional and signed (HTTPS via Comm Tower).
- **Exit criterion.** `stockpile install`/`stockpile remove` works without restart; `stockpile list` is fast; signature verification rejects a tampered `.fbox`.
- **Dependencies.** M29 (HTTPS), M33 (capabilities), M32 (atomic FS operations).
- **Effort.** 4 FTE-w.

### M35 — Patch (System Update Channel)
- **Scope.** APFS-snapshot-style atomic updates: every system release is a single signed RedSea II snapshot; `patch apply` atomically swaps the live root subvol, with an automatic Limine boot-menu entry for the previous snapshot. References: Apple's macOS update mechanism (sealed system volume, snapshot-on-update), Fedora Silverblue's `rpm-ostree`. A failed boot triggers automatic rollback after three watchdog timeouts.
- **Exit criterion.** A staged update is downloaded in the background, applied at next boot in <30 s, and a forced kernel panic on the new image triggers automatic rollback to the previous snapshot.
- **Dependencies.** M32 (snapshots), M29.
- **Effort.** 4 FTE-w.

### M36 — Accessibility v1.0 (release-gating)
- **Scope.** Reduce Motion (animations replaced by 1-frame cross-fades), Reduce Transparency (vibrancy materials replaced by opaque tints), Dynamic Type (UI text scalar 80–250%, applied to every Stage typeface stack), Increase Contrast. **VoiceOver-equivalent screen reader**: implemented as a Patrol service consuming an AT-SPI-style accessibility tree exposed by every Field OS app; basic gesture model (Cmd-F5 toggle, VO-arrow navigation, VO-Space activate, rotor for headings/links/landmarks). Reference architectures: Apple's Accessibility Programming Guide and *VoiceOver: A Brief History* (WWDC sessions); Orca (Linux) for GNOME-class AT-SPI semantics; NVDA (Windows) for the screen-reader output buffer model. **Honest position.** Building a VoiceOver-equivalent in HolyC is genuinely hard; the right move is to *port AT-SPI semantics* and ship a HolyC reader that drives system text-to-speech (Festival / Flite / a Mimic 3 port) over Wavelength.
- **Exit criterion.** A user navigates the entire system — login, opening Recon, reading a Brief document, sending a Dispatch email — without any visual reference. **This is a release-gating milestone.** If M36 misses, v0.1 misses.
- **Dependencies.** Every UI app has an a11y tree. M21, M25.
- **Effort.** 8–10 FTE-w.

### M37 — Recon v1 (Browser)
- **Scope.** Web browser via a ported engine running inside a Cardboard Box. Three options were considered:
  - **Servo** (Rust; BSD-3-Clause + MPL-2.0; modern; LFEU-hosted; explicitly embeddable per `servo.org/about/` and the recently published 0.1.0 crate). Real-world capable.
  - **Ladybird/LibWeb** (BSD-2-Clause; smaller; less mature in 2026 but improving rapidly; proven by SerenityOS).
  - **WebKit fork** (LGPL-2.1; license-incompatibility risk for a BSD-2 OS; surface-area pain).

  **Recommendation: Servo with a HolyC integration shim.** Embed Servo via its WebView API (`ServoBuilder`/`WebView`/pixel readback), draw the chrome (tabs, address bar, bookmarks, history, downloads) in HolyC on Stage, and pump events Servo→Stage and Stage→Servo through Channel. The browser shell is HolyC; the engine is sandboxed in a Cardboard Box with a single network capability (HTTPS to any host) and a single FS capability (read-only profile, write-only cache).
- **Exit criterion.** Recon loads `gmail.com`, `youtube.com` (HTML5 playback through Foundry-side hardware decode in M43's pipeline), `news.ycombinator.com`, and the WHATWG HTML spec page itself. JavaScript execution is sufficient for Gmail's basic IMAP-free webmail UI.
- **Dependencies.** M19 (WebGL/WebGPU later — WebGL only in Phase 1), M29, M33.
- **Effort.** 12–16 FTE-w. The integration shim is the cost; Servo itself is a download.

### M38 — Dispatch v1 (Mail)
- **Scope.** HolyC mail client. IMAP4 + JMAP (where the server supports it) + SMTP; S/MIME (X.509 via BearSSL/rustls cert chain) and OpenPGP (port `librnp` or `gpgme`'s minimal core). Threaded conversation view; rich text rendering through Stage; attachment Cardboard Box for previews.
- **Exit criterion.** Connects to Gmail IMAP, sends and receives mail, attachments preview safely sandboxed.
- **Dependencies.** M29, M33, M21.
- **Effort.** 8–10 FTE-w (~25 kLOC HolyC; threading and IMAP IDLE are the cost centers).

### M39 — Roster v1 (Contacts)
- **Scope.** CardDAV + vCard 4.0; iCloud and Google Contacts as well-known endpoints; shared SQLite-replacement RedSea II contact store with Dispatch and Schedule.
- **Exit criterion.** Sync with iCloud Contacts; create/edit/delete a contact; vCard import/export.
- **Dependencies.** M29, M32.
- **Effort.** 3 FTE-w.

### M40 — Schedule v1 (Calendar)
- **Scope.** CalDAV + iCalendar (RFC 5545); day, week, month, and year views; recurring events via RRULE; event invitations from Dispatch.
- **Exit criterion.** Sync with iCloud Calendar; create a recurring event; receive an event invitation in Dispatch and accept it from the body.
- **Dependencies.** M38, M29.
- **Effort.** 4 FTE-w.

### M41 — Negatives v1 (Photos)
- **Scope.** Library, simple non-destructive edits (crop, exposure, white balance, contrast, structure), JPEG/PNG/HEIC/AVIF/WebP via libheif (LGPL-3 → vendor under the Cardboard Box's GPL boundary, see §3.5) and libavif (BSD-2). RAW pipeline via libraw (LGPL-2.1, same boundary).
- **Exit criterion.** Imports 1 000 photos in <60 s; non-destructive edits round-trip; a Sony ARW raw renders.
- **Dependencies.** M21, M19.
- **Effort.** 6 FTE-w.

### M42 — Frequency v1 (Music)
- **Scope.** Local library; FLAC, MP3, AAC, Opus, Vorbis (FLAC and Opus first-party; the rest via FFmpeg's libavcodec subset, vendored under the GPL boundary). Subsonic protocol for streaming (no DRM-bound services in Phase 1). Smart playlists.
- **Exit criterion.** Plays a 96 kHz/24-bit FLAC bit-perfect through Wavelength; gapless playback verified.
- **Dependencies.** M25, M21.
- **Effort.** 4 FTE-w.

### M43 — Projector v1 (Video)
- **Scope.** mpv-class via libmpv vendored in a Cardboard Box (LGPL-2.1+; license boundary at the Cardboard Box edge). Hardware-accelerated H.264/H.265/VP9/AV1 decode via VAAPI on AMD/Intel — VAAPI maps directly onto the AMDGPU and i915/Xe LinuxKPI drivers. HDR static metadata (HDR10) optional.
- **Exit criterion.** A 4K H.265 10-bit file plays at 60 fps with hardware decode (CPU <30%); subtitle rendering through Stage; AV1 8K trailer plays at native rate on the iGPU.
- **Dependencies.** M17/M18 (VAAPI surface), M19, M25.
- **Effort.** 5 FTE-w.

### M44 — Cure v1 (System Repair)
- **Scope.** First-aid menu modeled on MGS3's Cure UX (select symptom → apply treatment): disk repair (RedSea II `fsck`), permissions reset, Patrol reset, Brief library re-index, Stockpile cache rebuild, network reset (Comm Tower kill+restart). Each treatment is reversible (a snapshot precedes any destructive op).
- **Exit criterion.** Each of the eight initial treatments demonstrably fixes a seeded fault.
- **Dependencies.** M32, M34.
- **Effort.** 3 FTE-w.

### M45 — Survival Kit v1 (Recovery)
- **Scope.** Boots a minimal Field OS session for diagnostics, from a hidden RedSea II `recovery` subvolume *or* a USB stick. Includes a stripped Cure, a Listening Post log viewer, a network shell, a Patch rollback shortcut.
- **Exit criterion.** A bricked install boots Survival Kit from a USB stick, mounts the corrupt root, runs `cure --offline`, and recovers.
- **Dependencies.** M44, M35.
- **Effort.** 3 FTE-w.

### M46 — Camo Index v1 (Theming)
- **Scope.** Theme engine with named palettes: Jungle, Khaki, Briefing Room, Recon Night, Tactical Black, Splitter, Olive, Snow, Tigerstripe, Field Standard. Each theme is a single declarative file (color tokens, material tints, accent gradients). Hot-switch without restart. Light/dark auto-switch on a schedule (default: sunrise/sunset via location-free fixed times user-editable).
- **Exit criterion.** All ten themes ship; switching is instant and survives a Stage restart.
- **Dependencies.** M20.
- **Effort.** 2 FTE-w.

### M47 — Codec v1 (Notification Surface)
- **Scope.** The bottom-left notification surface modeled on the MGS3 Codec layout: two-pane design, sender identity + frequency string left, message + actions right. Cmd-Shift-C opens the Codec inbox. Per-app notification permissions are Cardboard Box capabilities.
- **Exit criterion.** Dispatch, Schedule, Stockpile, and Patch all push through Codec; Do Not Disturb works; Cmd-Shift-C opens the inbox.
- **Dependencies.** M20, M33.
- **Effort.** 2 FTE-w.

### M48 — Listening Post v1 (Logs/Profiler/Debugger)
- **Scope.** Centralized log collection for every Patrol unit; in-place HolyC tracing on running units (similar to ftrace tracepoints exposed to userspace); sampling profiler integrated (Brendan Gregg-style flame graphs via a HolyC port of the `flamegraph.pl` toolchain). The HolyC source-as-documentation principle from TempleOS is preserved: stack frames symbolicate to source lines, F5 hot-patch survives.
- **Exit criterion.** Capture a 30-second flame graph of the entire system under interactive load; drill into a Patrol unit and modify a function with F5 hot-patch with no restart.
- **Dependencies.** Phase 0 Patrol scheduler, M21.
- **Effort.** 4 FTE-w.

### M49 — Performance Polish + 6 GB Footprint Enforcement
- **Scope.** A profiling pass against §7.1 of the design brief: cold boot ≤2.5 s on NVMe (measured from Limine entry to Stage's first frame); login → populated desktop ≤0.8 s; app cold launch — Cache ≤250 ms, Recon to first paint single-tab ≤1.2 s; idle RAM ≤220 MB with Patrol+Stage+Wavelength+Comm Tower up. Disk-footprint audit: every artifact tagged, fonts subset (IBM Plex full Latin extended only, CJK deferred), icons compressed, debug symbols split into a separate `.fdbg` package not installed by default. Boot-time tuning: defer non-critical Patrol units to post-login; build the kernel with LTO + size-favoring `-Oz` for cold-path code.
- **Exit criterion.** All four §7.1 numbers met on Framework 13 AMD; install size ≤6.0 GB.
- **Dependencies.** Everything.
- **Effort.** 6–8 FTE-w.

### M50 — v0.1 Release
- **Scope.** Public release. Installable from USB on Framework 13 AMD. Documented hardware compatibility list (HCL). Public devlog post announcing GA. A 6-minute demo video. A signed `.iso` and `.fbox` artifact set on the project repository. SBOM published. License inventory (BSD-2 base; GPLv2 LinuxKPI-ported drivers; SIL OFL fonts; ISC icons; MIT/Apache vendored libraries) published as `LICENSES.md`.
- **Exit criterion.** One stranger installs in <10 minutes and uses Field OS as their primary OS for one week.
- **Dependencies.** M11–M49.
- **Effort.** 2 FTE-w.

### Phase 1 effort summary

| Cluster | FTE-w |
|---|---|
| Bring-up (M11–M16) | ~22 |
| GPU stack (M17–M22) | ~41 |
| Audio (M23–M25) | ~13 |
| Networking (M26–M30) | ~24 |
| Power (M31) | ~12 |
| FS / sandbox / pkg / patch (M32–M35) | ~22 |
| A11y (M36) | ~9 |
| Apps (M37–M47) | ~52 |
| Tooling + polish + release (M48–M50) | ~14 |
| **Total** | **~209 FTE-w (~4 FTE-years)** |

A solo full-time builder at 35 h/week realistically clears ~30–35 FTE-w of "real" engineering throughput per calendar year (the rest is reading, debugging, life). 209 FTE-w / 33 ≈ **6.3 calendar years full-time, or ~14 calendar years part-time, if this were strictly serial**. Two things bend that curve down:

1. **LinuxKPI leverage.** A massive fraction of M17/M18/M23/M26/M27 collapses once the shim works for one driver; the *first* port costs 12–16 weeks, the *fifth* costs 3.
2. **Vendored libraries.** FreeType, HarfBuzz, lwIP, BearSSL, libmpv, libheif, FFmpeg subsets, NanoSVG, ACPICA — none of them rewritten. The 100,000-line base-system budget *excludes* these.

Even with both bends, the **9–15 month full-time / 18–30 month part-time** envelope is tight and assumes M11–M30 do not surprise. They will surprise. Plan accordingly.

---

## 3. The LinuxKPI Strategy in Depth

### 3.1 What is a Linux Compatibility Layer?

A Linux Compatibility Layer (LCL), often called LinuxKPI (Linux Kernel Programming Interface), is a translation layer that lets a non-Linux kernel host *unmodified* (or near-unmodified) Linux kernel driver source code by implementing the Linux internal kernel API surface in terms of the host kernel's primitives. The pattern is well-established:

- **FreeBSD `linuxkpi` + `drm-kmod`.** Production for ~7 years. The `freebsd/drm-kmod` repository on GitHub keeps amdgpu, i915, and radeon source files almost verbatim, with `#ifdef __linux__ ... #else /* FreeBSD */ ... #endif` brackets at every divergence. The FreeBSD desktop blog's *Drm Kmod Primer* (December 2018) explains the architecture: "This approach has allowed us to import the Linux drivers with relatively minor modifications which hopefully will ease long-term FreeBSD support."
- **Haiku's FreeBSD compatibility layer.** Reuses FreeBSD network drivers, particularly wireless. The Haiku project's *System Development* page calls out that "Haiku uses a FreeBSD network compatibility layer to support many network devices."
- **NetBSD's drm port.** Similar tactic, smaller scope.
- **Genode's DDE Linux** (Device Driver Environment for Linux). A Genode-specific shim that lets selected Linux drivers run as Genode components. Multiple academic papers; the `dde_linux` source tree is the practical artifact.

Field OS adopts the same pattern. The shim is called **Field LinuxKPI** (no fancier name; the engineering value is in being boring).

### 3.2 ABI/API surface to port

For a *minimal usable* LinuxKPI hosting `snd_hda_intel`, the surface is roughly:

- Memory: `kmalloc`/`kfree`/`kzalloc`/`vmalloc`/`vfree`, `GFP_KERNEL`/`GFP_ATOMIC` flags.
- Synchronization: `struct mutex`, `struct spinlock` (raw + IRQ-safe), `struct rw_semaphore`, `struct completion`, `wait_queue_head_t`.
- Time: `jiffies`, `msleep`, `udelay`, `ktime_get`, `hrtimer`.
- Work: `struct workqueue`, `INIT_WORK`, `schedule_work`, `cancel_work_sync`, `delayed_work`.
- IRQ: `request_irq`, `free_irq`, `tasklet`/`softirq` mapped to Patrol's bottom-half class.
- PCI: `struct pci_dev`, `pci_enable_device`, `pci_set_master`, `pci_iomap`, MSI/MSI-X allocation.
- DMA: `dma_alloc_coherent`, `dma_map_single`, `dma_map_sg`, IOMMU mapping (the AMD IOMMUv2 surface).
- I/O: `readl`/`writel`/`readq`/`writeq`, `ioread32_rep`, port I/O.
- Device model: `mod_devicetable.h` structures (`pci_device_id`), `kobject`/`kref`/`device` minimal subset, sysfs (write-only set of attributes), procfs (a couple of well-known nodes).
- Firmware: `request_firmware`/`release_firmware` reading from `/lib/firmware/`.

For a LinuxKPI hosting **AMDGPU + iwlwifi** the surface roughly **doubles**: add the `device-tree`-style binding scaffold, `regmap`, the I²C/SMBus subset, the GPIO and pinctrl shims (mostly stubs), the `drm` core (KMS, atomic helpers, fence, dma-buf, gem, ttm), `cfg80211`, `mac80211`, `nl80211`, the `crypto` subset (CCMP/GCMP), and the workqueue/tasklet model with real-time guarantees.

### 3.3 Effort estimate

- **Minimal skeleton** (boots one HD-Audio codec end-to-end): **4–8 FTE-w**.
- **Enough to host AMDGPU + iwlwifi**: **12–20 FTE-w** *additional*.
- **Steady-state maintenance** per Linux LTS bump: 2–4 FTE-w per upgrade; we recommend pinning to **Linux 6.12 LTS** for the entire Phase 1 cycle, then planning a single bump to Linux 6.18 LTS in Phase 2.

### 3.4 License compatibility

This is the central, non-technical question. Linux drivers are GPLv2; Field OS's base is BSD-2.

**The legal posture Field OS adopts:**

1. The LinuxKPI shim itself is **BSD-2**. It contains zero copied Linux kernel source.
2. The ported drivers — `amdgpu`, `i915`, `xe`, `iwlwifi`, `mt76`, `mac80211`, `cfg80211`, `snd_hda_intel`, `xhci-hcd` — are vendored in a separate top-level directory `drivers/ported/` and **remain GPLv2**, with the original headers, copyrights, and license preserved verbatim. Any patches to these files keep the GPLv2 boundary.
3. Field OS ships as a **mere aggregation** of BSD-2 base + GPLv2 drivers, with the license boundary explicitly drawn at the LinuxKPI symbol table. This is the same legal posture FreeBSD has held for ~7 years with `drm-kmod`. The Software Freedom Conservancy and the GPL community have not challenged FreeBSD's `drm-kmod` model.
4. Apps with LGPL dependencies (libheif, libraw, libmpv, libavcodec) live inside Cardboard Boxes and load their LGPL libraries dynamically; the LGPL-2.1+ "use as a library" relief applies.
5. **Get a real lawyer** before v0.1 ships. This plan is engineering, not legal advice. The honest read is that the FreeBSD posture is well-tested in practice but has not been litigated.

### 3.5 Why this beats writing from scratch

`drivers/gpu/drm/amd/` was 5+ million lines as of Linux 6.6 (Phoronix, August 2023) and crossed 6 million in Linux 7.0 (Phoronix / WebProNews, February 2026). Even discounting auto-generated headers (~70% of the line count), the *hand-written* logic — quirks, workarounds, firmware-version compatibility tables, per-ASIC display engine code — is 1.5+ million lines. A solo builder writes ~400 LOC/day of *new* code on a great day and ~100 LOC/day on a normal one. The from-scratch route is a **fifteen-year solo project**. LinuxKPI compresses it to one quarter.

The same logic applies to `iwlwifi` (~150 kLOC + firmware blob coupling), `mt76` (~80 kLOC + the FLR-readiness quirks the Framework community has documented), and `snd_hda_intel` (~50 kLOC + the codec quirks tables that take years to accumulate).

### 3.6 Why this is hard

- **Linux internal APIs change every release.** The LinuxKPI must be pinned to a single LTS. **Choose Linux 6.12 LTS** for Phase 1; it is well into stable backports by the v0.1 ship date.
- **Upgrades are explicit project decisions, not free.** Budget 2–4 FTE-w per LTS bump.
- **GPL boundary discipline must be enforced in CI.** Static analysis to ensure no GPL-only header is included from BSD-2 code.
- **Firmware blobs** (`amdgpu/*.bin`, `iwlwifi-*.ucode`, `mt7922_*.bin`) ship under their own license terms, often the AMD/Intel/MediaTek redistribution licenses. Bundle them with attribution; do not rebuild them.

---

## 4. Skill-Building / Ramp-Up Reading List for Phase 1

### 4.1 LinuxKPI / driver porting
- FreeBSD `sys/compat/linuxkpi/` source tree (canonical).
- `freebsd/drm-kmod` GitHub README and *Porting a new version of DRM drivers from Linux* wiki.
- Haiku's `src/libs/compat/freebsd_*` source tree.
- Genode's `repos/dde_linux/` source + the *DDE Linux* documentation chapter in the Genode Foundations book.
- LWN.net "Linux Driver Subsystems" series and Jonathan Corbet's *Linux Device Drivers* (the third edition is dated but still the right starting point).

### 4.2 GPU / DRM / KMS
- Linux DRM subsystem documentation (`Documentation/gpu/` in the Linux source).
- *The Linux Graphics Stack* — overview chapters from the X.Org Foundation and freedesktop.org.
- The Mesa documentation (`docs.mesa3d.org`).
- The AMDGPU source tree itself (`drivers/gpu/drm/amd/`, especially `amdgpu_device.c`, `dc/`, and the `display/` tree).
- Asahi blog: *Tales of the M1 GPU* (Lina, November 2022), *Apple GPU drivers now in Asahi Linux* (December 2022), *Paving the Road to Vulkan on Asahi Linux* (Lina, March 2023), *The first conformant M1 GPU driver* (Rosenzweig, August 2023), *Vulkan 1.3 on the M1 in 1 month* (Rosenzweig, June 2024), *Dissecting the Apple M1 GPU, the end* (Rosenzweig). These are the canonical chronicle of writing a modern GPU driver as a small team.

### 4.3 Vulkan
- vulkan-tutorial.com (Sascha Willems mirrors the Khronos worked example).
- The Vulkan 1.3 specification.
- Sascha Willems' samples repository.
- The Khronos SPIR-V cookbook and the SPIRV-Tools README.

### 4.4 Audio
- PipeWire's `docs.pipewire.org/page_design.html` (the *Design* page) — the architecture target for Wavelength.
- Apple's CoreAudio HAL Programming Guide.
- The Linux ALSA Project's `doc/`, especially the codec topology references for HDA.
- FreeBSD's `sys/dev/sound/` and `sys/dev/sound/pcm/` source.

### 4.5 Networking
- The lwIP documentation tree (`savannah.nongnu.org/projects/lwip`) and the lwIP repository in Contiki.
- The smoltcp documentation (for comparison; not for porting in Phase 1).
- *Computer Networks: A Systems Approach* (Peterson & Davie) for the design vocabulary.
- RFC 8446 (TLS 1.3) end-to-end.
- BearSSL's `bearssl.org/goals.html` and `bearssl.org/api1.html`.

### 4.6 Filesystems
- Apple's *APFS Reference* (the public PDF).
- The OpenZFS *On-Disk Format* document.
- Marshall Kirk McKusick's papers on FFS/UFS2.
- Dominic Giampaolo's *Practical File System Design* (the BFS book, free online from Be Inc.'s archives).
- The Btrfs *On-disk format* wiki page.

### 4.7 Sandboxing
- Watson, Anderson, Laurie, Kennaway, *Capsicum: practical capabilities for UNIX*, USENIX Security 2010.
- OpenBSD `pledge(2)` and `unveil(2)` man pages and the source in `sys/kern/kern_pledge.c`.
- Fuchsia's *Zircon Concepts* documentation, especially *Handles* and *Object Capabilities*.
- The Genode capability paper *Practical Capability-Based Operating System for Embedded Systems* and the Genode Foundations book chapter on capabilities.

### 4.8 Power management / S0ix
- Intel's *Modern Standby* design guide (Intel Developer Zone).
- Microsoft's *Modern Standby* documentation on Microsoft Learn.
- The Linux Power Management Subsystem documentation in `Documentation/power/`.
- The Asahi Linux *Updates galore!* November 2022 progress report (s2idle bring-up).
- Intel's S0ixSelftestTool README and the AMD `amd-debug-tools` README.
- Arch Linux's *Power management/Suspend and hibernate* page (the practical-troubleshooting bridge between vendor and OS).

### 4.9 Browser engine porting
- Servo's *Embedding* guide (`book.servo.org/embedding.html`) and the `paulrouget/servo-embedding-example` repository.
- Servo 0.1.0 release notes (April 2026).
- Ladybird's source tour (`github.com/LadybirdBrowser/ladybird`).
- *The Browser Hacker's Handbook* (Alcorn et al.) for the security model.

### 4.10 Accessibility
- Apple's Accessibility Programming Guide.
- WWDC 2020 session 10104 *App Accessibility for Switch Control* for the model.
- Orca's source (`gitlab.gnome.org/GNOME/orca`).
- NVDA's source (`github.com/nvaccess/nvda`).
- AT-SPI specification (freedesktop.org).
- WAI-ARIA 1.2 specification (W3C).

---

## 5. Tooling Additions for Phase 1

### 5.1 Hardware-in-the-loop CI

Topology:

```
GitHub Actions  ──────────────►  self-hosted runner (Linux mini-PC at home)
                                      │
                                      │  PXE/USB boot via NetBoot
                                      ▼
                              Framework 13 AMD test rig
                                      │
                                      │  serial-over-USB-C  +  USB-net kernel-panic
                                      ▼
                              runner captures logs, boot timing, dmesg
```

Reference: Asahi's CI (their `linux-asahi-edge` regression suite) and the Linux kernel's KernelCI. The runner reflashes the SUT (system under test) on every push to `main`, runs a smoke suite (boot ≤2.5 s, login ≤0.8 s, FlatBuffer perf, suspend/resume × 3), and posts results to the PR. **This single piece of infrastructure is the difference between a serious project and a hobby project.** It also catches the "real hardware shock" early.

### 5.2 Real-hardware debugging

- **Serial-over-USB-C** on the Framework: USB-C debug cable, `ttyUSB0`, Patrol's `printk`-equivalent on `COM1` at 115 200 8N1. The Framework's USB-C alt-mode is the cleanest debug path on commodity laptops in 2026.
- **JTAG** where supported (Intel Direct Connect Interface on Core Ultra; not on Ryzen).
- **Kernel panic over USB-net**: a tiny in-kernel CDC-NCM driver that stays alive after Patrol panics and dumps the panic log to a host on the other end.
- **PCIe sniffer** (a Teledyne / Ellisys equivalent is out of price range for a solo builder; budget ~US$2 000 for a used Beagle USB 5000 v2 SuperSpeed protocol analyzer instead — invaluable for M15).

### 5.3 Profiling

- A HolyC port of `perf`-style sampling: Listening Post v1 (M48). 1 ms timer-based sampling; symbol resolution from the kernel ELF.
- Flame graphs: a HolyC port of Brendan Gregg's `flamegraph.pl`, rendered as SVG and viewable in Recon.

### 5.4 Tracing

- ftrace-equivalent in Patrol: static tracepoints at every syscall, IPC send, IPC receive, IRQ entry/exit, schedule. Ring buffer per CPU; user-space reader.
- Dynamic probes via the F5 hot-patch mechanism: the source-as-documentation model means a developer can edit a function, hit F5, and the running kernel patches the symbol in place.

### 5.5 Power testing

- Intel S0ixSelftestTool, run nightly on the Tier-1 Intel reference.
- AMD `amd-debug-tools`, run nightly on the Framework 13 AMD reference.
- `powertop` ported to Field OS for interactive use.
- A custom **Field Power Audit** tool: a Patrol unit that logs per-process wakeups/sec, per-IRQ counts, GPU residency in PSR2, and correlates with battery draw.

---

## 6. Solo-Builder Cadence and Discipline for Phase 1

### 6.1 Hardware purchase recommendations

Buy now, before M11:

- **2 × Framework 13 AMD Ryzen 7 7840U** (Wi-Fi: AMD RZ616 / MediaTek MT7922; this is the canonical Tier-1). One for development, one for clean-install regression testing. Use the AMD board so the same machine covers M11, M17, M23, M27, M31. Purchase the high-refresh 2.8 K display option.
- **1 × Framework 13 Intel Core Ultra (Meteor Lake)** for M18 / M26 / Intel S0ix coverage.
- **1 × ThinkPad X1 Carbon Gen 12** (Intel Core Ultra) for the second Tier-1 datapoint and to test on a different vendor's ACPI tables.
- **1 × generic AMD desktop** (B650 motherboard, Ryzen 7700, 32 GB DDR5, RDNA3 iGPU): the desktop reference; covers AHCI (M14) and the desktop-class power profile.

**Do not buy NVIDIA hardware in Phase 1.** Nouveau is not a Phase 1 dependency, NVIDIA's open kernel module assumes a recent kernel that LinuxKPI is not pinning to, and NVIDIA's display path is a Phase 2 problem.

Total hardware budget: ~US$ 7 500 for the four reference machines + ~US$ 2 000 for debug tooling (USB protocol analyzer, USB-C debug cables, a managed switch for the home CI lab, a small UPS).

### 6.2 The shift from QEMU to real hardware

Plan the first 4–8 FTE-w of Phase 1 (M11 itself plus the early shock) as **regression management, not feature work.** Things that worked in QEMU and will *not* work on real silicon include, with high confidence:

- The PIT / HPET selection logic — real machines have HPETs that lie about frequency.
- IO-APIC initialization order — real machines need `_PIC` invoked first.
- The framebuffer's stride and pixel format — GOP gives `BltOnly` framebuffers more often than QEMU does.
- PS/2 controller assumption — modern laptops have only emulated PS/2 over the EC, with subtle quirks.
- TSC frequency calibration — real CPUs need MSR-based calibration, not PIT cross-checks.
- The implicit ordering in the boot path — the order of "init PCIe, init USB, init storage, init display" is rarely safe in practice; ACPI `_INI` callbacks must drive ordering.

Document each shock with a public devlog entry. The community values honesty about this; vendors and users alike learn from it.

### 6.3 When to accept outside contributions in Phase 1

Phase 0 was a single-author project on purpose. Phase 1 should remain *primarily* solo for code-architecture coherence, but the natural first contribution areas open up:

- **Hardware drivers and ports.** Other Tier‑2 hardware (additional Intel Wi‑Fi parts, additional Realtek HDA codecs, additional Framework expansion-card combinations) are excellent first patches.
- **LinuxKPI shim coverage.** Adding the next missing kmalloc-class function is a one-day patch with clear test cases.
- **Documentation and the Field Manual.** As soon as M21 ships, the Field Manual itself becomes editable in Brief; documentation contributions are then frictionless.
- **Accessibility expertise.** A genuine VoiceOver-equivalent built without prior a11y experience is a known risk; *recruit* an a11y collaborator no later than M30.
- **Theme design.** Camo Index themes (M46) are tractable first contributions for designers.

Do **not** accept code architecture changes from outside in Phase 1. Cohesion is the dearest resource a solo OS has; do not trade it for velocity.

### 6.4 Community management

- **Discord and IRC** (libera.chat) bridged into a single channel.
- **Monthly devlog** (the Asahi cadence); the *Asahi Linux blog* is the model — long, technical, honest about what's broken.
- **Demo videos every 4 weeks**, 5–10 minutes, screencast + voice-over (the SerenityOS Andreas Kling cadence; this single discipline carried Serenity from a hobby to a 4 000-Discord-member project).
- **Quarterly "year in review" post** even within Phase 1 (Field OS Year 1 in review, Year 2 in review).

### 6.5 The "v0.1 release" psychology

This is the first release that can be reviewed by tech press. Prepare for the spotlight (Ars Technica, Phoronix, Hacker News, OSnews) — and prepare for the silence. Both are possible. Have:

- A pre-written press release (the SerenityOS year-3 Ars Technica pickup is the reference).
- A landing page with a 90-second hero video, the four reference machines pictured, and the install command.
- An honest known-issues page.
- A funding model (Patreon / GitHub Sponsors) ready to receive support if the spotlight hits — Asahi Lina's GitHub Sponsors page and Andreas Kling's Patreon both materially funded their projects.

---

## 7. Risk Register for Phase 1

| # | Risk | Probability | Impact | Mitigation |
|---|---|---|---|---|
| R1 | LinuxKPI port fails for AMDGPU (largest single technical risk) | M | Catastrophic | Validate the shim approach on `snd_hda_intel` first (M23 before M17). If HDA cannot be ported in 4–6 weeks, the AMDGPU plan does not survive contact with reality and Phase 1 must be re-scoped |
| R2 | Real-hardware variance breaks Tier-1 promise | M | High | Strict Tier-1 device list; HCL marked ⚠ for everything outside it; CI hardware-in-the-loop on the *exact* Tier-1 SKUs |
| R3 | Modern Standby / S0ix never reaches the 24-cycle bar | M | High | Buy machines with documented Linux S0ix support (Framework 13 AMD has documented S0ix on Linux; tlvince/framework-laptop-13-amd-7640u tracks this). Budget S0ix to Tier-2 in Phase 1 if it does not converge by M31 + 8 weeks |
| R4 | Servo regresses between picking and shipping | L | Medium | Pin to a specific Servo crate version; evaluate Ladybird as a fallback at the M37 mid-point review |
| R5 | VoiceOver-equivalent is genuinely too hard solo | H | Catastrophic (release-gating) | Recruit an a11y collaborator no later than M30; consider porting Orca's AT-SPI architecture wholesale; be prepared to slip M50 rather than ship without M36 |
| R6 | License compatibility on ported GPL drivers | L | Catastrophic | Get a real lawyer's review before v0.1 ships; document boundaries in `LICENSES.md`; mirror the FreeBSD `drm-kmod` legal posture |
| R7 | Performance regression as functionality lands | M | Medium | Performance CI from M11; budget tracker per milestone; M49 explicitly carves time for the polish pass |
| R8 | Solo-builder pace risk / burnout | H | Catastrophic | Calendar-time, not deadline-time. Two 2-week breaks per year, mandatory. Public funding model so financial pressure does not compound |
| R9 | Wi-Fi MT7922 FLR-readiness regression resurfaces post-port | M | High | Carry the workaround `rtc_cmos.use_acpi_alarm=1` and the MT7921 firmware update path; soak-test 24 h on every push |
| R10 | TLS 1.3 not landed in BearSSL by ship | M | Medium | Have rustls-via-FFI ready as the fallback; do not let TLS choice block other milestones |

---

## 8. Definition of "Done" for Phase 1 — Concrete Acceptance Tests

These are the v0.1 release gates. Every one must pass on the *primary* Tier‑1 reference (Framework 13 AMD Ryzen 7 7840U) before M50 ships:

1. **Install** Field OS from a USB stick on a fresh machine in **<10 minutes**.
2. **Cold boot** to login screen ≤**2.5 s** (measured from Limine entry).
3. **Login → populated desktop** ≤**0.8 s**.
4. **Suspend (S0ix)** and **resume** reliable across **24 consecutive cycles** with no panic, no data loss, no peripheral lost.
5. **Internal hardware functional**: Wi-Fi (RZ616/MT7922), Bluetooth, audio (Realtek ALC295), internal display brightness control, keyboard backlight, fingerprint reader if present (Phase 1: Tier-2 / nice-to-have), webcam (Tier-2 in Phase 1: UVC USB only on Framework's modular camera, which is UVC-class).
6. **External display** via HDMI expansion card and USB-C alt-mode DP up to 4 K @ 60 Hz.
7. **Battery life ≥8 h** on light productivity load, matching Linux on the same hardware.
8. **Recon** loads `gmail.com`, `youtube.com`, `news.ycombinator.com` correctly with HTTPS.
9. **Dispatch** successfully connects to a Gmail IMAP account, sends and receives mail.
10. **Frequency** plays a 96 kHz/24-bit FLAC library bit-perfect.
11. **Projector** plays a 4K H.265 file with hardware-accelerated decode (CPU <30%).
12. **VoiceOver** navigates the system entirely without visual reference.
13. **Reduce Motion / Reduce Transparency / Dynamic Type** all functional.
14. **Total install size** ≤ **6.0 GB**.
15. **Stockpile** installs and removes a `.fbox` package without restart.
16. **Patch** stages, applies, and rolls back a system update.
17. **Cure** repairs a seeded RedSea II inconsistency.
18. **Idle RAM** ≤ **220 MB** with Patrol + Stage + Wavelength + Comm Tower up.

---

## 9. Comparison to Other Small-Team / Solo OS Projects

### 9.1 SerenityOS year 2–3

- **Year 1 (Oct 2018 → Oct 2019, *From zero to HTML in a year*):** Andreas Kling, mostly full-time first six months (between jobs) then with a job, single-handedly shipped a kernel, ext2 on top of it, an ELF loader, a GUI toolkit, a window manager, a userland Unix-like, the LibWeb HTML engine to the point of rendering its own birthday page. Single-laptop QEMU only.
- **Year 2 (Oct 2019 → Oct 2020, *The second year*):** Userland security (`pledge`/`unveil` adopted from OpenBSD), a userspace x86 emulator (Valgrind-class), Spreadsheet app, multiple games, JS LibJS engine, an HTTP server. Still QEMU-only.
- **Year 3 (Oct 2020 → Oct 2021, *Year 3 in review*):** Discord community 4 000+ members, Ars Technica feature, x86_64 port (Gunnar Beutner), Quake II on multiple cores. **Still QEMU-only.**
- **Year 4–5 (Oct 2021 → Oct 2023):** 32-bit x86 retired, more media codecs, Ladybird spun out as a cross-platform browser. **Still QEMU-only.**
- **Year 6 (2024–2025):** sdomi's *Bringing SerenityOS to real hardware, one driver at a time* on a Dell 3100 Chromebook — a single solitary person, in a side-project, pushing SerenityOS onto a single piece of real silicon. *Six years in*, this was the first bare-metal effort.

The lesson: **a from-scratch OS without an explicit Linux-driver leverage strategy stays in QEMU for the better part of a decade.** Field OS's LinuxKPI strategy is the entire reason real-hardware bring-up is plausible in Phase 1 calendar time.

### 9.2 Asahi Linux year 1–2

- **December 2020:** Hector Martin announces the project.
- **January 2021:** funded work begins.
- **March 18, 2022:** *The first Asahi Linux Alpha Release is here!* — a working installer, Wi-Fi, USB2, NVMe, framebuffer display, ethernet, keyboard, touchpad, headphone jack. **No GPU acceleration. No Bluetooth. No DisplayPort. No Thunderbolt. No sleep.** This was alpha-quality after **15 months** of well-funded multi-person reverse engineering on a single SoC family. Note especially what was *not* in the alpha: the very list of items Field OS Phase 1 must ship to claim parity.
- **November 2022 progress report:** s2idle works. Cpuidle driver lands. CPU boost states enabled.
- **December 2022:** *Apple GPU drivers now in Asahi Linux* — alpha GPU acceleration, OpenGL 2.1, OpenGL ES 2.0. **Two years in.**
- **March 2023:** Lina's *Paving the Road to Vulkan* — explicit-sync UAPI design.
- **August 2023:** *The first conformant M1 GPU driver* — OpenGL ES 3.1 conformance. **Three years in.**
- **June 2024:** *Vulkan 1.3 on the M1 in 1 month* — Honeykrisp lands conformant Vulkan. **Three and a half years in.**
- **2025–2026:** Vulkan 1.4 conformance same-day; Honeykrisp ships as the basis for LunarG's KosmicKrisp on macOS itself.

The Asahi timeline is the **upper bound of how fast a small, world-class, well-funded team can move on new silicon.** Field OS targets *commodity* x86 silicon with *Linux drivers* — the floor of difficulty, not the ceiling — but a solo builder. The two factors roughly cancel. **One year of real-hardware bring-up + six months of app polish is the floor.**

### 9.3 Redox OS year 2–3 and beyond

- **2015:** project starts.
- **2018:** initial `relibc`, FAT32 filesystem in GSOC, AArch64 work begins.
- **2023:** September *Development Priorities* post — establishing a stable ABI is still ahead; relibc must become a dynamic library.
- **2024–2025:** Orbital adds GPU-based mouse cursor rendering, VirtIO-GPU support; bjorn3 lands `redox-scheme` integration.
- **January 2026:** Anhad Singh's dynamic-linking work makes relibc a stable ABI candidate. *Then* the 1.0 conversation can begin. This is **eleven years in.**

The Redox lesson: a from-scratch OS *without* a leverage strategy for hardware (Redox is pure-Rust including drivers) takes a decade to reach the ABI-stability prerequisites for 1.0. Field OS's choice to *ship LinuxKPI early* is the single most important architectural decision separating its timeline from Redox's.

### 9.4 Haiku 2010–2015

- **2009:** Haiku R1/Alpha 1. Already had the BeOS API (the *Be Book*), Tracker, Deskbar.
- **2010–2012:** ASLR, DEP, SMAP, FreeBSD network compatibility layer (the model Field LinuxKPI follows for non-DRM drivers).
- **2012–2018:** WebPositive based on WebKit, package management evolution (`pkgman`/HPKG — the model for Stockpile), Java port.
- **2018:** Beta 1.
- **2025:** R1 still not declared.

The Haiku lesson: even with 5+ contributors and a clear API target (BeOS), shipping a "1.0" desktop OS is a 15+ year project. Field OS's Phase 1 v0.1 is *not* Haiku R1; it is closer to Haiku R1/Alpha 1 in scope. That parity is the right framing for the press around v0.1.

### 9.5 Honest comparison

|  | Real-HW first boot | Compositor on real HW | Browser on real HW | Suspend/resume | "Daily driver" |
|---|---|---|---|---|---|
| SerenityOS | year 6 | not yet | (Ladybird outside) | no | no |
| Asahi Linux | year 1.25 | year 2 (alpha) | year 1.25 (Linux apps) | year 2 (s2idle) | year 3 (first conformant GL) |
| Redox | year 5+ | year 5+ (Orbital, basic) | only ports | partial | not yet |
| Haiku | year 2 | year 2 | year 4 (WebPositive) | year 6+ | partial / niche |
| **Field OS Phase 1 target** | **year 1.5 part-time / 0.75 full-time** | **same** | **same** | **same** | **same** |

Field OS Phase 1 is calibrated to be *Asahi-fast on commodity x86, alone*. That is plausible only because LinuxKPI compresses the driver dimension. It is not plausible without it.

---

## 10. Phase 2 Preview (Why Phase 1 Discipline Matters)

Phase 2's sketch (12–24 months after v0.1):

- **ARM64 bring-up.** Either Apple Silicon via Asahi-style RE (collaboration likely), or Snapdragon X (ACPI + standard PCIe; the easier path), or RPi5 (broadest reach). The choice will be data-driven post-v0.1.
- **Creative app suite.** Manual (document editor, Pages-class), Armory (LSP-class IDE in HolyC, source-as-documentation native), DAW-class audio editing in Wavelength, RAW pipeline + color management in Negatives.
- **Polyglot story.** Rust/Zig/Python/Go support via either WASM Tabernacles (sandboxed WASM runtime; the spiritual successor to Cardboard Box for non-native code) or a POSIX-y compatibility layer (the Haiku/SerenityOS approach).
- **More daily-driver apps.** Operator (terminal), Briefing (presentations), Listening Post v2 (telemetry dashboard), Calling Card (auth manager).
- **Better filesystems.** ext4 R/W, btrfs R/O, exFAT R/W, NTFS R/W (via a careful `ntfs-3g` LinuxKPI port if BSD-compatible licensing emerges, otherwise via a from-scratch HolyC implementation).
- **HDR, wide-gamut, Display Stream Compression.** Foundry v2.
- **Engine v1 (OpenCL-class compute API).** Stub in Phase 1 (M19 covers the compute primitives); productized in Phase 2.

**Why Phase 1 must be tight.** Every line of Phase 1 code that is "good enough but not great" becomes a Phase 2 maintenance burden. The 100,000-line base-system budget exists precisely for this reason: a small, comprehensible, BSD-2 base survives a Phase 2 expansion. A sprawling base does not. The discipline of the line-budget is what allows Field OS to be reviewed, in Year 5, by an engineer at Apple, Google Fuchsia, Asahi Linux, or System76 and respected.

---

## 11. Closing — A Note on What Phase 1 Is For

Phase 0 proved Field OS can exist. Phase 1 proves it can be *used*.

The deliverable is not the milestone list. The deliverable is the moment a stranger plugs in a USB stick, reboots their Framework 13 AMD into Field OS for the first time, and a week later still hasn't reached for their old laptop. Every line of HolyC, every shimmed Linux driver, every Plex Sans glyph, every IRQ that survives a sleep cycle — they all exist to make that single moment ordinary.

The honest read of the calendar is twenty-four months part-time, twelve months full-time, and a four-to-eight-week real-hardware shock that no plan can prevent. The honest read of the engineering is that LinuxKPI is the only solo-builder strategy that fits in a human lifetime, and Asahi is the only living example of that strategy succeeding on hard new silicon. Field OS targets the easier silicon — commodity x86 with Linux drivers — and so the strategy is more plausible. It is not easy. It is plausible.

Phase 1 ends with a release that the press can review, a hardware compatibility list a stranger can buy from, an a11y story that holds up in front of a sighted user *and* a blind one, and a solo builder who is still standing, still able to write the next line of HolyC, and still building Field OS in calendar year three.

That is the bar. Begin at M11.