# Arsenal

*A modern desktop operating system, designed for performance, usability, and security as peer concerns.*

## TL;DR

**Arsenal** is a from-scratch desktop operating system written primarily in Rust, targeting commodity 2026 hardware (Framework 13 AMD/Intel, Snapdragon X laptops, Apple Silicon M1/M2 Macs, generic AMD/Intel desktops). It is built by a single author over a multi-year arc with the explicit goal of treating performance, usability, and security as **peer concerns**, none subordinated to the others.

The project formerly explored a TempleOS-modernization framing ("Field OS") but has been re-architected on technical merit. HolyC, DolDoc heritage, religious framing, and TempleOS-derived naming have been removed entirely. The MGS3-warm tactical component vocabulary (Patrol, Stage, Cardboard Box, Cache, Codec, etc.) is preserved as the project's design identity.

The kernel is a Rust monolith with capability-secured userspace. Drivers are inherited from Linux 6.12 LTS via a LinuxKPI-style shim. The compositor is a custom wgpu/Skia "Stage" rendering the iDroid + Big Sur visual identity. Applications ship as native Rust binaries, sandboxed Wasm components (WASI 0.2 today, 0.3 when stable), and a curated POSIX subset for porting Firefox, mpv, and similar. The base is BSD-2-licensed; inherited Linux drivers retain GPLv2.

Realistic timeline: **7–10 calendar years to v1.0** on Framework 13 AMD, with first real-hardware boot at month 9–24 and first daily-driver alpha at month 24–36. This is the same shape as the comparable Redox / SerenityOS / Genode trajectories.

---

## Mission: The Three Peers

Arsenal explicitly commits that **performance, usability, and security are peer concerns**. When they conflict, the resolution is not a single dominant axis but an honest engineering trade with a documented decision record.

| Pillar | What it means concretely | Non-negotiables |
|---|---|---|
| **Performance** | Cold boot < 5 s on Framework 13 AMD; idle RAM < 200 MB for the kernel + Stage + supervisor; <16 ms compositor frame budget at 60 Hz; gigabit network throughput; NVMe near-line-rate. | No language-induced slowdown beyond Rust's normal cost. No interpreted hot paths. No GC. |
| **Usability** | First-boot user reaches a working browser, mail client, and music player without reading a manual. Display, Wi-Fi, audio, suspend, and external monitors work on supported hardware. Accessibility (screen reader, high-contrast, keyboard navigation) shipped before v1.0 — not retrofitted. | No "read the wiki to install" flows. No required terminal use. No untranslated kernel error codes shown to end users. |
| **Security** | Memory safety via Rust's type system across the kernel and base. Capability-based IPC. Per-app sandboxing via Cardboard Box. No setuid root. Signed system updates. TLS 1.3 by default. | No unsafe Rust without a documented invariant comment. No daemon runs as root that doesn't need to. No telemetry without opt-in. |

When two pillars conflict, the resolution is recorded as an Architecture Decision Record (ADR) and shipped with the release notes. **The project does not silently rank one above the others.**

### Concrete trade-off examples

- **CHERI-style overhead vs. raw performance.** CHERI hardware is unavailable in 2026 commodity silicon; the question is moot until it ships. When it does, Arsenal will evaluate per-workload (5–10% on most code, up to 1.7× on hot interpreters per arXiv 2308.05076) and may offer CHERI as an opt-in security mode rather than a default.
- **Wasm sandbox overhead vs. native performance.** Native Rust binaries are the default for first-party apps. Wasm components are the default for third-party apps. The user is informed; the choice is explicit.
- **Verified microkernel vs. solo-buildable monolith.** seL4 is the verified-IPC champion, but no solo builder has shipped a verified desktop OS, and Genode-on-seL4 is a 19-year multi-person project. Arsenal accepts a Rust monolith with type-system memory safety as the security floor and adds capability-based IPC and Cardboard Box sandboxing on top. Verification is a Phase 4+ research lane, not a v1.0 commitment.

---

## Visual Identity (Preserved)

Arsenal carries forward the iDroid + Big Sur fusion identity established in earlier design work:

- **Color system.** Amber `#FFB200` (primary signal), cyan `#00C8E0` (secondary signal), navy `#0A1A2A` (chrome base). Accent reds for warnings only.
- **Typography.** IBM Plex Mono 13 px for chrome (menus, status, readouts). IBM Plex Sans 14 px for body. IBM Plex Serif for long-form Brief documents.
- **Geometry.** 4 px spacing grid throughout. Corner radii 8 / 12 / 20 px (small / medium / large surfaces).
- **Surfaces.** Big Sur translucent vibrancy via dual-pass blur. Holographic milspec scan-line shader on the Stage compositor chrome. Generous whitespace; no Acme/Plan-9-style information density in default UI.
- **Mood.** Holographic milspec readouts on a surface that breathes. The chrome looks like a tactical operations display; the content area looks like a Big Sur app.

Visual mockups generated via the existing Stitch prompts remain valid; only the wordmark changes from "Field OS" to **Arsenal**.

---

## Naming Catalog (Preserved)

All MGS3-derived component names are preserved. Project name only is changed. Arsenal Gear (Metal Gear Solid 2) is itself an MGS reference, so the project name is consistent with the vocabulary.

| Subsystem | Name | Role |
|---|---|---|
| Service supervisor | **Patrol** | Init, service management, lifecycle. systemd-equivalent. |
| Compositor / window server | **Stage** | wgpu/Skia-based Wayland-protocol-compatible compositor. |
| File manager | **Cache** | Default GUI file manager. |
| Notification surface | **Codec** | System notifications and ambient alerts. |
| Search / launcher | **Radar** | System-wide search and command launcher. |
| Command palette | **CQC** | Power-user keyboard-driven action palette. |
| System settings | **Frequencies** | User-facing settings UI. |
| Diagnostics / repair | **Cure** | First-aid for failed boots and broken systems. |
| Recovery environment | **Survival Kit** | Bootable rescue partition. |
| Resource monitor | **Stamina** | Activity-Monitor-equivalent. |
| Package manager | **Stockpile** | Install / update / remove apps. |
| Network stack | **Comm Tower** | Userland networking daemon (smoltcp + rustls). |
| Audio stack | **Wavelength** | PipeWire-equivalent userland audio server. |
| Graphics API | **Foundry** | Vulkan-class API for apps. |
| Compute API | **Engine** | OpenCL/Metal-equivalent for GPGPU. |
| Logs / observability | **Listening Post** | journald-equivalent. |
| Identity / auth | **Calling Card** | User accounts and authentication. |
| Sandbox container | **Cardboard Box** | Per-app capability container. |
| Terminal | **Operator** | Terminal emulator. |
| IDE | **Armory** | Native developer environment. |
| Document viewer/editor | **Manual** | Default Brief / document app. |
| Web browser | **Recon** | Web browser (likely Servo-based or WebKit shim). |
| Onboarding | **Briefing** | First-run UX. |
| Help system | **Field Manual** | In-system documentation. |
| Icon set | **Field Symbols** | Custom iconography. |
| Themes | **Camo Index** | Visual theme selector. |
| DAW | **Cassette** | Audio production app. |
| Vector graphics | **Stencil** | Illustrator-equivalent. |
| Video editor | **Sequence** | NLE. |

**Brief** is preserved as the executable document format — the concept (notebook-style documents with embedded executable code blocks, hyperlinks, and inline macros) is generic to Jupyter, Pluto.jl, Quarto, and Mathematica, not specific to TempleOS. The word "Brief" is milspec vocabulary, not religious framing.

---

## Architectural Decisions

| Layer | Decision | Rationale |
|---|---|---|
| **Kernel architecture** | Rust monolithic kernel with capability-secured userspace IPC. NOT seL4, NOT SASOS, NOT pure microkernel. | seL4 is too low-level for solo desktop builds; SASOS without CHERI is a research bet; pure microkernels (Genode-on-seL4) take 19 years with paid teams. A Rust monolith (Redox/Asterinas pattern) gives driver inheritance, performance, and type-system memory safety without formal-verification debt. |
| **Primary language** | **Rust**, end-to-end. No second language in the base system. | Rust wins on architectural merit: memory safety, ecosystem, rust-for-linux driver inheritance path, mature verification tools (Verus, Kani, Prusti, Creusot) for future hardening. C is permitted *only* in inherited Linux drivers (which remain GPLv2 in their original form). |
| **Driver strategy** | LinuxKPI-style shim hosting Linux 6.12 LTS drivers (amdgpu, i915/xe, iwlwifi, mac80211, sof-audio, xhci, nvme, bluetooth). Selected native Rust rewrites for low-complexity drivers (NVMe queueing, USB-HID, virtio, basic Wi-Fi management). | This is the only solo-tractable path to commodity-hardware support. FreeBSD's LinuxKPI is the most battle-tested implementation and confirms the model works. Genode DDE is architecturally purer but tied to Genode's release cadence. |
| **Network stack** | smoltcp (TCP/IP, UDP, no_std, zero-allocation) + rustls (TLS 1.2/1.3) in a userland Comm Tower daemon. mac80211 inherited via LinuxKPI for 802.11 / WPA3 / regulatory. | smoltcp's claims hold up (Gbps throughput, no heap). rustls is production-grade and the only credible TLS 1.3 in pure Rust. The 802.11 layer cannot reasonably be rewritten — inherit from Linux. |
| **Graphics** | Custom Stage compositor in Rust + wgpu (primary) / Skia (fallback). Wayland protocol compatibility for app portability. Foundry exposes Vulkan to apps via vulkano or ash. | wgpu provides cross-vendor GPU acceleration with reasonable abstraction. Wayland compatibility lets existing apps (Firefox, mpv, VS Code via wlroots clients) run unmodified. Stage owns the iDroid/Big Sur identity rendering. |
| **GUI toolkit** | **Slint** for application UI + custom Skia/wgpu Stage compositor for shell chrome. | Slint handles 90% of standard app UI cheaply with reactive declarative DSL. Stage handles the bespoke holographic milspec / scan-line / vibrancy effects where Slint would fight on text shaping, IME, and a11y. Apps render *into* Stage. |
| **Application distribution** | Three-tier: (1) **native Rust binaries** for first-party apps, (2) **Wasm components** (WASI 0.2 today, 0.3 when stable) for sandboxed third-party apps, (3) **POSIX subset** (relibc-style) for ports of Firefox, mpv, git, rust-analyzer, foot, kitty. | Native binaries for performance. Wasm for sandboxed extensibility. POSIX subset to avoid the "Wasm-only ecosystem from zero" trap that would otherwise sink the project. |
| **Application sandbox** | **Cardboard Box** — capability-based per-app container. Apps declare required capabilities at install time (filesystem paths, network endpoints, devices). User grants per-capability at first launch. No app-runs-as-user-with-full-access default. | Modeled on capability systems (Capsicum, Fuchsia handles, Genode capabilities). The single biggest security improvement over conventional desktop OSes. |
| **Developer UX** | Three modes: (1) standard polished GUI (default), (2) **Inspector overlay** (`Super+I` keybind) exposing live component graph / IPC / capability tree (Genode Leitzentrale pattern), (3) **Brief documents** as a separate notebook app for executable-document workflows. | Achievable with existing toolkits. No three-button mouse / Acme chording required. Each mode has shipped precedent. |
| **License** | **BSD-2-Clause** for the Arsenal base (kernel, supervisor, compositor, system apps). MIT/BSD/Apache-2.0 acceptable for vendored Rust crates. **GPLv2 preserved on inherited Linux drivers** — non-negotiable; Linux drivers must retain their original license, which means Arsenal ships as a *combined work* with explicit license boundaries. | BSD-2 matches the project's permissive aesthetic and lets the LinuxKPI shim respect Linux licensing. The combined-work model is what FreeBSD/drm-kmod has done for a decade. |
| **Project name** | **Arsenal**. | Replaces "Field OS." Consistent with the MGS vocabulary (Arsenal Gear is the floating fortress in Metal Gear Solid 2). Single word, easy to say, no religious or TempleOS associations. |

---

## Architectural Layers (Boot to App)

```
┌──────────────────────────────────────────────────────────────┐
│ Native apps (Rust)    Wasm apps (WASI 0.2)    POSIX ports    │
│   Cache, Cassette,      third-party              Firefox,    │
│   Stencil, Sequence,    sandboxed                mpv, git,   │
│   Recon, Manual         components               foot        │
├──────────────────────────────────────────────────────────────┤
│ Slint UI toolkit  ◄──────────────────────► Wayland clients   │
├──────────────────────────────────────────────────────────────┤
│ Stage compositor (Rust + wgpu/Skia, Wayland protocol server) │
│ Foundry (Vulkan)   Engine (compute)   Wavelength (audio)     │
├──────────────────────────────────────────────────────────────┤
│ Patrol (init / service supervisor)                           │
│ Comm Tower (smoltcp + rustls)   Cardboard Box (sandbox)      │
│ Stockpile (packages)   Calling Card (auth)                   │
├──────────────────────────────────────────────────────────────┤
│ Arsenal kernel (Rust monolith)                               │
│   • Scheduler, memory mgmt, capability IPC                   │
│   • Filesystem (ext4 + Arsenal-native)                       │
│   • LinuxKPI shim                                            │
│   • Native Rust drivers (NVMe, USB-HID, virtio)              │
├──────────────────────────────────────────────────────────────┤
│ Inherited Linux 6.12 LTS drivers (GPLv2, isolated)           │
│   amdgpu, i915/xe, iwlwifi, sof, xhci, bluetooth             │
├──────────────────────────────────────────────────────────────┤
│ UEFI firmware → Limine bootloader                            │
└──────────────────────────────────────────────────────────────┘
```

---

## Realistic Timeline

Calibrated against Redox (11 years, pre-1.0, single primary author + community), SerenityOS (7 years to a usable desktop with a paid full-time founder + 140+ contributors), Genode (19 years with a paid team for desktop maturity), and Asahi Linux (5 years from start to daily-driver M1 Mac support with a small paid core team).

| Year | Milestone | Deliverable |
|---|---|---|
| 0–1 | M0: Boot and breathe | Rust kernel skeleton, UEFI boot via Limine, serial console, virtio drivers in QEMU, basic scheduler, Rust no_std std-library, simple shell. Equivalent to Redox at year 1. |
| 1–2 | M1: Real iron | LinuxKPI shim functional. amdgpu (KMS only), NVMe, xHCI, iwlwifi+mac80211 ported. First boot on real Framework 13 AMD hardware. Slint app runs in software-rendered framebuffer. |
| 2–3.5 | M2: It looks like Arsenal | Stage compositor with iDroid/Big Sur identity. Wayland shim so existing Wayland apps run. First five native apps: Cache, Operator, Manual, Frequencies, Inspector. Browser: Servo embedded as a Slint app, or WebKitGTK via Wayland shim. **First public alpha.** |
| 3.5–5 | v0.5 | Wasm component runtime (Wasmtime). POSIX/relibc subset. Ports of Firefox, mpv, foot. Brief notebook app. Cardboard Box sandbox. |
| 5–7 | **v1.0** | Daily-driver maturity on Framework 13 AMD. Snapdragon X port. Cassette, Stencil, Sequence apps. Mail (Dispatch), music (Frequency), video (Projector), IDE (Armory). Accessibility shipped. Stockpile remote repository. |
| 7–10 | v2.0 | Apple Silicon via Asahi collaboration. Tablet experience (touch, pen). CHERI experimental support if commodity silicon arrives (currently nonexistent). |

---

## Three Concrete Starting Milestones

### M0 — Boot and breathe (months 0–9)

- [ ] Rust kernel project, no_std, x86_64 + aarch64 targets.
- [ ] UEFI bootloader (Limine — do not write your own).
- [ ] Serial console, framebuffer console, basic SMP, paging, scheduler.
- [ ] virtio block + virtio-net, smoltcp + rustls.
- [ ] Boots to a `>` prompt in QEMU.
- [ ] Public repository, BSD-2 license, MGS3-themed naming throughout.
- [ ] **Performance gate:** boot to prompt in < 2 s under QEMU.
- [ ] **Security gate:** zero unsafe Rust outside designated FFI boundaries.
- [ ] **Usability gate:** prompt is keyboard-navigable; shows hardware summary.

### M1 — Real iron (months 9–24)

- [ ] LinuxKPI shim layer in Rust, modeled on FreeBSD drm-kmod patterns. **This is the single largest engineering task; budget accordingly.**
- [ ] amdgpu driver under LinuxKPI, KMS only (no Vulkan yet).
- [ ] NVMe driver (native Rust, ~5 K LOC).
- [ ] xHCI USB driver (native Rust or LinuxKPI port — evaluate at start).
- [ ] iwlwifi + mac80211 via LinuxKPI.
- [ ] First boot on real Framework 13 AMD hardware.
- [ ] Slint app runs in a software-rendered framebuffer.
- [ ] **Performance gate:** cold boot to login on Framework 13 AMD < 8 s.
- [ ] **Security gate:** Linux drivers run with minimum required kernel capabilities; no shared kernel state beyond explicit shim interfaces.
- [ ] **Usability gate:** Wi-Fi association via TUI works on first try.

### M2 — It looks like Arsenal (months 24–36)

- [ ] Stage compositor in Rust + wgpu/Skia, rendering iDroid/Big Sur identity:
  - amber `#FFB200`, cyan `#00C8E0`, navy `#0A1A2A`
  - IBM Plex Mono 13 px chrome, Plex Sans 14 px body
  - 4 px grid, 8/12/20 px corner radii
  - Scan-line shader overlay on chrome
  - Big Sur translucent vibrancy via dual-pass blur
- [ ] Wayland shim (Smithay-based) so existing Wayland apps run.
- [ ] First five native apps: Cache, Operator, Manual, Frequencies, Inspector overlay.
- [ ] Browser: Servo embedded as a Slint app (primary) or WebKitGTK via Wayland shim (fallback).
- [ ] **First public alpha** — installable on Framework 13 AMD, daily-drivable for one task (web browsing).
- [ ] **Performance gate:** Stage holds 60 fps at 2880×1920 with vibrancy + scan-line shader.
- [ ] **Security gate:** Cardboard Box capability model enforced for all third-party apps.
- [ ] **Usability gate:** screen reader (Orca-protocol-compatible) works for all native apps.

---

## Caveats and Watch Items

- **CHERI may flip in 5–7 years.** If SCI Semiconductor, Codasip, or another vendor ships a commodity 64-bit CHERI laptop SoC, Arsenal evaluates an opt-in CHERI mode in Phase 4. Until that silicon exists, do not architect around it.
- **WASI 0.3 timing.** Targeted February 2026 per the WASI roadmap with experimental support in Wasmtime 37+. WASI 1.0 is "planned for 2026" — these dates historically slip. Do not commit to async-Wasm-as-primary-IPC until WASI 1.0 ships.
- **Slint accessibility is the single biggest UX risk.** If a screen-reader user cannot navigate Arsenal, the project fails its usability mission. Either contribute upstream to Slint's AccessKit integration or write a dedicated a11y compatibility layer in Stage. Budget 2–4 person-months in Phase 1 for this work; it is a v1.0 release blocker.
- **Asahi Linux is in flux.** Asahi Lina paused work in March 2025; Hector Martin stepped down February 2025; the project is now under seven-person shared governance. The Apple Silicon port (Phase 3 / v2.0) depends on Asahi's continued upstreaming progress, which is not guaranteed. Have a Phase 3 contingency that does not require Asahi (M1 Mac mini only, no laptops; or skip Apple Silicon entirely if Asahi stalls).
- **Snapdragon X Linux support has visibly regressed in Q4 2025.** Tuxedo cancelled their X1 Elite laptop in November 2025. The Snapdragon X commitment may need a fallback target — Framework 13 Intel Core Ultra is a safer second platform.
- **The "solo builder" framing is generous.** Every comparable project that shipped — Redox, SerenityOS, Genode, Asahi — eventually became multi-person efforts. Plan for a transition from solo to small-team around year 3–4; the BSD-2 license is designed precisely to make that transition possible.
- **The original Convergent OS document's destination is correct.** A capability-based, memory-safe, declaratively-UI'd OS is a defensible 10-year community-project target. The disagreement is only about *speed* and *what runs where*. Arsenal walks toward that destination one milestone at a time, on hardware a user can buy in 2026, with the performance / usability / security peers held in genuine balance.
