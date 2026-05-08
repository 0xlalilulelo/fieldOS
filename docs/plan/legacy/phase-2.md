# Field OS — Phase 2 Engineering Plan (M51–M90)
## Snapdragon X bring-up, hybrid polyglot, pro-creative apps, and v1.0

---

## 0. Executive summary

Phase 0 took Field OS from nothing to a QEMU PoC (M0–M10). Phase 1 took it to v0.1 — a daily-driver-quality build on Framework 13 AMD with HolyC, Brief, Stage, Foundry, Comm Tower, Wavelength, Cardboard Box, Stockpile, Patch, accessibility v1.0, and the launch app suite (Recon / Dispatch / Roster / Schedule / Negatives / Frequency / Projector / Cure / Survival Kit) (M11–M50).

**Phase 2 (M51–M90) takes Field OS to v1.0.** The four anchor deliverables are:

1. **Snapdragon X bring-up** — a full, second Tier-1 architecture, leaning on Linaro/Qualcomm's mainline Linux work, OpenBSD's day-one port, and Mesa's Freedreno/Turnip A7xx driver. ARM64 is the *correct* second target precisely because it is standard ACPI + PCIe and therefore mostly a port, not an Asahi-style reverse engineering project.
2. **Hybrid polyglot via WASM Tabernacles** — Wasmtime hosted in a Cardboard Box, exposing WASI 0.2 (Preview 2, stable since 25 January 2024) plus the Component Model. HolyC stays the first-class native; Rust / Zig / Python / Go / C / C++ are second-class but production-grade through WASM.
3. **Three new pro-tier apps** — **Manual** (Pages-class document editor), **Armory** (VS Code-class IDE), **Cassette** (Logic Pro-class DAW; recommended final name from the MGS3-warm shortlist), plus a substantial upgrade — **Negatives v2** (Capture One-class RAW workflow).
4. **The v1.0 release itself** — stable ABI, multi-user, MDM, 11 localisations including RTL Arabic, Field OS SDK shipped as an `.fbox` for Mac/Linux/Windows hosts, Stockpile remote repository live, three Tier-1 hardware families certified.

**Honest timeline.** Phase 2 is plausibly **24–36 months at 15 h/wk, or 12–18 months at 35 h/wk.** Including Phase 0 and Phase 1, a v1.0 release is a **4–6 calendar-year total project from M0**. This calibrates against SerenityOS's year 4–5 (Ladybird forking off in June 2024 to become its own project under the Ladybird Browser Initiative — alpha 2026, beta 2027, stable 2028); Asahi Linux's year 3–5 (Vulkan 1.3 conformance October 2024, Vulkan 1.4 same-day conformance 2 December 2024, project lead Hector Martin stepping down Feb 2025); Redox's slower 12-year-plus arc to 1.0 (dynamic linker was an RSoC project in 2024, ARM64 dynamic linking landed Jan 2025); and Haiku's R1 Beta cadence (Beta 5 shipped 13 September 2024, 1.5 years after Beta 4, R1 stable still indeterminate). Field OS's competitive advantage is LinuxKPI-style reuse plus a single solo author with kernel comfort; the disadvantage is, of course, being one person.

**Definition of done for v1.0.** A Field OS install that **1,000 daily-driver users actively use for ≥30 days each, across at least three Tier-1 hardware families**: Framework 13 AMD, Framework 13 Intel, and a Snapdragon X reference machine (the leading candidate is the Lenovo ThinkPad T14s Gen 6 because FreeBSD's Poul-Henning Kamp got -CURRENT booting on it in November 2024 — proving the platform's openness — and because Linaro's TUXEDO partnership demonstrated full Debian on the same X1E-80-100 silicon at Connect 2025). The bar is tightened from "boots and runs" to "we recommend it to a friend without disclaimers."

---

## 1. Phase 2 scope and explicit non-goals

### 1.1 What ships in v1.0

| Bucket | Deliverable |
|---|---|
| Hardware | Framework 13 AMD (Phoenix/Hawk Point), Framework 13 Intel Core Ultra, Snapdragon X (reference: Lenovo T14s Gen 6 + Surface Laptop 7); secondary support for Surface Pro 11, Dell XPS 13 9345, Galaxy Book4 Edge, ASUS Vivobook S 15 |
| Kernel/runtime | HolyC compiler retargeted to AArch64; ARM64 SMP, GIC v3, PSCI, ACPI on ARM64; kernel ABI v1 declared stable |
| Compositor / GPU | Stage on Adreno X1 via Foundry v2 (HDR10, HDR10+, Display P3 native, Rec.2020 working space, Display Stream Compression); Engine v1 compute API productised |
| Polyglot | WASM Tabernacles (Wasmtime, WASI 0.2, Component Model); pinned to WASI 0.2 with a documented forward-compat plan for WASI 0.3 when it ships |
| New / upgraded apps | **Manual v1**, **Armory v1**, **Cassette v1**, **Briefing v1**, **Operator v1**, **Calling Card v1**, **Stamina v1**, **Listening Post v2**, **Negatives v2**, **Recon v2**, **Wavelength v2** |
| Filesystems | RedSea II (native, R/W, snapshots, encryption); ext4 R/W; exFAT R/W (HolyC, from spec); NTFS R/W (ntfs3 via LinuxKPI); btrfs R/O |
| System | Multi-user, network sync of Camo Index/Frequencies/Calling Card vault over WebDAV + S3, encrypted backups, MDM, accessibility v1.1, 11 localisations |
| Developer | Field OS SDK as `.fbox` for Mac/Linux/Windows; Stockpile remote repository; signed app submission; developer documentation site |

### 1.2 Explicit non-goals (defer to Phase 3)

- **Apple Silicon.** Requires Asahi-style RE; that is a Phase 3 project, ideally in collaboration with Asahi (now under collective governance after Hector Martin's February 2025 resignation as project lead).
- **Mobile / tablet form factor.** Snapdragon X tablets, Surface Pro pen, multi-touch, on-screen keyboard.
- **Cellular modem.** Even though MBIM is standardised, integration is a Phase 3 effort.
- **Pro-creative apps still missing at v1.0:** vector graphics editor (Illustrator-class), video editor (DaVinci Resolve-class), 3D modeller (Blender-class). Each is a multi-year effort.
- **Server profile.** A Field OS "without compositor" headless edition is Phase 3.
- **WebRTC in Recon.** Research-grade only at v1.0; not a release blocker.
- **Apple iCloud / Microsoft Graph compatibility.** Sync is WebDAV + S3-compatible only; reverse-engineered cloud APIs are not a v1.0 promise.
- **Hexagon NPU as a first-class compute API.** M57 exposes it; Engine v1 can dispatch to it on Snapdragon, but Negatives v2's AI features must work without it (compute shader fallback on AMDGPU/Xe).

### 1.3 The "second-OS shock"

Adding a second architecture is roughly **1.4–1.7×** the work of the first, not 2× and not 1.1×. The shock comes in several shapes the Phase 2 plan should budget for explicitly:

- **HolyC compiler retargeting** is fundamentally new code (M52). Asahi did not face this because Linux's compilers are pre-portable; Field OS does.
- **Driver bring-up rediscovers half-fixed assumptions.** Asahi's M1→M2→M3 progression shows this — M3 still only "boots to a blinking cursor" on m1n1 as of October 2025, two years after first M1 daily-driver work. FreeBSD's Snapdragon X port required mid-2024 ARM64 interrupt fixes that nobody anticipated.
- **GPU drivers are the long pole.** Asahi's Vulkan 1.3 took nearly two years after first conformant OpenGL ES 3.1; Vulkan 1.4 came on day one, two months later, only because the foundation was solid. For Field OS, M55 (Adreno X1 port) is the single highest-risk milestone in Phase 2.
- **Power management is the second-longest pole.** Snapdragon X has multiple compute clusters and many IP cores. Linaro is shipping `qcom-cpufreq-hw`, `rpmh`, and `rpmpd` upstream; Field OS rides those via LinuxKPI but must still wire and validate them.

The mitigation is the LinuxKPI bet from Phase 1: the same shim layer that hosts `amdgpu` and `i915/Xe` in Phase 1 can host `msm`/`drm/msm` and Qualcomm clock/power drivers in Phase 2. The retargeting work is not free, but it is mostly mechanical.

---

## 2. Milestone breakdown M51–M90

Each milestone has: **scope**, **exit criteria**, **dependencies**, **effort estimate** (in solo-builder weeks at 35 h/wk, with a ×2 multiplier for 15 h/wk).

### Block A — Snapdragon X bring-up (M51–M58) — 24–32 weeks FT

#### M51 — Snapdragon X first boot (3 weeks FT)
**Scope.** Limine on AArch64; UEFI handoff per the EFI/AArch64 binding; ACPI table parse via the same ACPICA port from Phase 1's M22 (ACPICA is architecture-neutral); a working serial console (Qualcomm GENI/QUP UART, exposed via SPCR); a GOP framebuffer at native panel resolution.
**Exit.** `kprintf("Field OS / Snapdragon X / hello")` on serial and on the panel of a Lenovo T14s Gen 6 from a USB stick.
**Dependencies.** Phase 1's bootloader, Phase 1's ACPICA port. Reference: Linaro/Qualcomm upstreaming work for X1E80100, OpenBSD's `arm64.html` Snapdragon entries, Patrick Wildt's June 2024 Yoga Slim 7 boot.
**Risks.** Qualcomm firmware requires 4 KB-aligned EFI applications (FreeBSD hit this on the Yoga C630); Limine should already be compliant, verify on first try.

#### M52 — HolyC retargeted to AArch64 (10 weeks FT)
**Scope.** Port the HolyC compiler's backend to emit AArch64. Three options were evaluated:

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Fork holyc-lang's existing x86-64 backend, hand-write AArch64 | Closest to TempleOS heritage | A complete instruction selector + register allocator + ABI is ≥ 8 kLoC; one person, one architecture; brittle | **Reject** |
| Plug HolyC's IR into an LLVM backend | Best codegen, both targets free, future RISC-V free | LLVM is ≥ 30 MB of C++ to vendor, a tooling commitment that grows the BSD-2 core's effective dependency footprint significantly | **Plausible** |
| Plug HolyC's IR into QBE | Tiny (~10 kLoC C), already supports x86-64 + AArch64 + RISC-V, BSD-class license | Codegen is plain rather than optimal; not all C-style features land cleanly | **Recommend** |

**Recommendation: QBE backend.** Cure the codegen quality through better IR generation in the HolyC frontend. Keep the existing x86-64 fast-path as a parallel emit option (good for regression testing and for preserving the F5 hot-patch experience, which depends on snappy single-function recompilation).
**Exit.** `holyc -t aarch64 hello.HC` produces a working AArch64 ELF; the kernel and supervisor build for AArch64 with no `#ifdef X86_64`/`#ifdef AARCH64` code rot exceeding 5% of the base system LoC.
**Dependencies.** M51.
**Effort note.** 10 weeks FT is a tight estimate; budget 14 if QBE integration surfaces ABI gotchas.

#### M53 — ARM64 boot stack (4 weeks FT)
**Scope.** 4-level page tables at 48-bit VA, exception-level model (kernel runs at EL1, no hypervisor in v1.0, EL0 for user); GIC v3 interrupt controller (GICv3 is the Snapdragon X choice, not GICv4); CNTPCT_EL0/CNTFRQ_EL0 generic timers; SMP startup via PSCI 1.1 (`PSCI_CPU_ON`); MMU enable / cache maintenance / TLB invalidation (DSB ISH; TLBI VAE1IS).
**Exit.** All 12 Oryon cores online; scheduler running on all of them; idle path entering WFI; `/proc/cpuinfo`-equivalent reporting all cores.
**Dependencies.** M52.

#### M54 — Snapdragon X platform drivers (5 weeks FT)
**Scope.** GPI DMA, QUP (UART/I²C/SPI controllers — the same blocks but multiplexed), USB-C controllers (XHCI per board's USB Type-C policy manager), embedded controller for laptop-specific keyboard/touchpad/charger (per OEM — T14s, Surface, XPS each different), thermal sensors (TSENS), CPU frequency scaling via `qcom-cpufreq-hw`. All bound through the LinuxKPI shim from Phase 1.
**Exit.** Keyboard, touchpad, USB-A and USB-C, fans spin under load, battery percent and charge state visible to Codec.
**Dependencies.** M53. Reference: Linaro Qualcomm Platform Services upstream patches (`lore.kernel.org/all/?q=X1E80100`), current Linux 6.15/6.16 mainline support matrix.

#### M55 — Adreno X1 GPU port (10 weeks FT) ⚠ HIGHEST-RISK MILESTONE
**Scope.** Port the Linux MSM kernel driver (`drivers/gpu/drm/msm`) via LinuxKPI for command submission, GMU power management, and GEM-equivalent buffer management. Take Mesa's Turnip Vulkan driver (Adreno A7xx Gen 7 family) and build it against Field OS's Foundry shim (Foundry exposes Vulkan 1.3, Turnip is now Vulkan 1.4-capable as of Mesa 25.x). The Freedreno Gallium driver is **not** ported — Adreno X1 has no Gallium support upstream (only Turnip), and Field OS does not ship OpenGL natively anyway; OpenGL apps run via Zink-on-Turnip, optionally, in a Cardboard Box.
**Exit.** vkmark and a representative subset of Foundry conformance tests pass; Stage runs at native resolution + native refresh on the T14s Gen 6 panel; dEQP-VK 1.3 mustpass passes ≥ 99% of cases (matching what Asahi Honeykrisp passed on day-one Vulkan 1.3 in October 2024).
**Dependencies.** M54, Foundry v1 from Phase 1.
**Risks.** UBWC (Universal Bandwidth Compression) bring-up; Turnip's `enable_tp_ubwc_flag_hint` workaround on certain X-Elite boards. Variable rate shading currently disabled in upstream Turnip; Field OS will also ship it disabled.
**Mitigation.** This is where Phase 2 most plausibly slips. Budget +6 weeks of contingency.

#### M56 — Snapdragon X power management (5 weeks FT)
**Scope.** S0ix-equivalent Modern Standby on Snapdragon X. Hooks into `rpmh` (Resource Power Manager-hardened), `rpmpd` (Power Domain), CPU C-states via WFI/PSCI suspend, GPU power down via Adreno GMU. Lid close, suspend-to-RAM, wake on key, wake on Wi-Fi.
**Exit.** Suspend → resume cycle preserves all state; battery drain in suspend ≤ 1%/hr; resume to login screen ≤ 1.5 s.
**Dependencies.** M54, M55.

#### M57 — Snapdragon X NPU exposure (Hexagon) (4 weeks FT) — *optional / soft target*
**Scope.** `fastrpc` driver via LinuxKPI, exposed as a Field OS Engine compute device. The QNN SDK is not redistributable; Field OS ships the open kernel side and lets users opt in to Qualcomm's userspace runtime via Stockpile. The Engine v1 API (M73) gates on a hardware capability flag, so software paths still work everywhere.
**Exit.** Negatives v2's AI-assisted masking runs ≥ 5× faster on the Hexagon NPU than on a compute-shader fallback path on Adreno X1.
**Dependencies.** M55, M73.

#### M58 — Snapdragon X v0.1-equivalent acceptance (1 week FT)
**Scope.** Run Phase 1's full M50 acceptance test suite on Snapdragon X. Boot, login, suspend/resume, Recon to a non-trivial site, Dispatch sending mail, Wavelength playing audio without xrun, Projector showing 1080p H.264 at full frame rate.
**Exit.** All Phase 1 acceptance tests pass on Snapdragon X with delta-from-AMD ≤ 10% on each non-power metric, ≤ 0% on power metrics (i.e., Snapdragon should be at least as good on battery).
**Dependencies.** M51–M56.

### Block B — Hybrid polyglot (M59–M62) — 12–16 weeks FT

#### M59 — Wasmtime in a Cardboard Box (4 weeks FT)
**Scope.** Vendor Wasmtime (Apache-2.0) at a pinned LTS version. Recommended pin: a Bytecode Alliance LTS release (the Alliance introduced 2-year LTS support windows in 2024–2025, which suits a v1.0-stable promise). Compile Wasmtime against Field OS's HolyC-callable C ABI shims; package as a Cardboard Box capability. Wasmtime, **not** Wasmer, because of: Bytecode Alliance backing, mature WASI 0.2 support since Q1 2024, Apache-2.0 (clean for BSD-2 core), Cranelift codegen quality, and production deployment evidence at Fastly / Shopify / Fermyon.
**Exit.** A trivial "hello" Wasm component runs from `wasm run hello.wasm` inside Field OS, isolated to its Cardboard Box.
**Dependencies.** Phase 1 Cardboard Box.

#### M60 — WASI 0.2 + Component Model surface (5 weeks FT)
**Scope.** Map Field OS resources to WASI 0.2 worlds:
- `wasi:filesystem` — Cardboard Box-restricted preopens (capability-passing, not path-passing).
- `wasi:sockets` — TCP/UDP via Comm Tower, capability-gated.
- `wasi:clocks`, `wasi:random`, `wasi:cli` — straight maps.
- `wasi:http` — both client and server worlds (the Component Model's incoming-handler interface).
- A Field OS-specific WIT package, `field-os:brief`, exposing read/append on Brief documents through a resource handle.
**Exit.** `wasmtime run --component foo.wasm` invokes a component that can `cat` a file the Cardboard Box has granted, send an HTTP request to a host the box has granted, and emit a Brief block.
**Dependencies.** M59. Reference: `bytecodealliance.org/articles/WASI-0.2`, `component-model.bytecodealliance.org`.
**Forward-compat.** Field OS pins to WASI 0.2 for v1.0. WASI 0.3 (Spin v3.5 shipped first RC November 2025; native async I/O; WASI 1.0 expected 2026) will be added in a Phase 3 Tabernacle update; the Component Model's virtualisability means polyfilling 0.2 in 0.3 is straightforward.

#### M61 — Language toolchain integration (5 weeks FT, mostly documentation)
**Scope.** Document and ship cookbook guides for each supported language. None of these toolchains live in the Field OS repository — they're host-side, run on Mac/Linux/Windows, producing `.wasm` components that Field OS executes.

| Language | Toolchain | Recommended target | Status today |
|---|---|---|---|
| Rust | `cargo` + `cargo component` | `wasm32-wasip2` | Production-ready |
| Zig | `zig build` | `wasm32-wasi` | Solid |
| C / C++ | `wasi-sdk` | `wasm32-wasi` / `wasm32-wasip2` | Mature |
| Python | CPython-WASM (Pyodide-style) running *inside* a Tabernacle, plus `componentize-py` for component packaging | n/a | Mature for batch; some I/O quirks |
| Go | `tinygo build -target=wasi` | `wasm32-wasi` | Constrained (no full goroutines) but workable |
| .NET | `componentize-dotnet` (NuGet) | `wasm32-wasip2` | Experimental, not a v1.0 promise |
| Java/Kotlin | TeaVM + `componentize-jvm` (Fermyon fork) | `wasm32-wasip2` | Experimental, not a v1.0 promise |

**Exit.** Five "Hello, Field OS" samples (Rust, Zig, C, Python, Go) build on a Mac, install via `stockpile install hello-rust.fbox`, and run.
**Dependencies.** M60.

#### M62 — Polyglot showcase apps (3 weeks FT)
**Scope.** Three samples in Stockpile to demonstrate the story is real:
- **`bat-fc`** — a Field OS port of `bat` (Rust, syntax-highlighted file viewer) running in a Tabernacle, talking to Manual via `field-os:brief`.
- **`spark`** — a minimal Jupyter-class notebook in Python, running CPython-in-WASM, with kernel I/O over WASI sockets.
- **`fz`** — a Zig CLI utility (file fuzzy-finder) integrating with the HolyC shell via stdout JSON.

**Exit.** All three are publishable to Stockpile; the Field Manual gets a "Polyglot apps" chapter linking them.
**Dependencies.** M61.

### Block C — Document and IDE app suite (M63–M68) — 36–48 weeks FT

#### M63 — Manual v1 (Pages-class document editor) (12 weeks FT)
**Scope.** A pro-tier word processor, pure HolyC where possible, vendored libraries where necessary.

- **Text engine.** Rope-based document buffer (the standard, used by VS Code, xi, and Lapce). Layout via a TeX-class line-breaker (Knuth–Plass). Bidi via Unicode UAX #9; FriBidi vendored as a Cardboard Box for the algorithm reference, but the integration is HolyC-native.
- **Tables.** CSS-class table model (rows × cells, rowspan/colspan, automatic + fixed widths). Not a TeX `\halign`; users expect Word-class tables.
- **Multi-column layouts.** Yes; via the same Knuth–Plass on per-column box machinery.
- **Master pages.** Yes; layered with paragraph and character styles.
- **Comments and change tracking.** Modelled on the OOXML revision-mark schema (`w:ins`, `w:del`, `w:moveTo`, `w:moveFrom`) so import/export with `.docx` round-trips cleanly.
- **PDF export.** PDFium (BSD-3-Clause) vendored as a Cardboard Box. Field OS does **not** write a PDF generator from scratch; that is a several-thousand-hour rabbit hole.
- **ePub export.** HolyC-native (ePub is a ZIP of XHTML; trivial relative to PDF).
- **Word `.docx` import.** OOXML is huge; vendor LibreOffice's filters as a Cardboard Box (MPL-2.0; clean for BSD-2 core).
- **Pages `.pages` import.** Pages files are a ZIP containing a binary `index.iwa` (Apple's protobuf-ish format). The community's `iwa-parser` (Node.js, MIT) provides the schema; port the schema to HolyC; ship `.pages` import as best-effort and `.pages` export as not-supported.

**Exit.** Manual opens, edits, and saves a Brief document of ≥ 200 pages with embedded tables, footnotes, comments, and tracked changes; export-to-PDF round-trips visually identical to a reference PDF; `.docx` import passes a 25-document corpus from the LibreOffice test suite.
**Dependencies.** Phase 1's Brief format, Stage compositor, Cardboard Box.

#### M64 — Briefing v1 (Keynote-class presentation app) (6 weeks FT)
**Scope.** Slide-based, 16:9 default + 16:10 + 4:3 supported; theme system (templates are themed Brief documents); builds and transitions powered by Stage's compositor (no heroics — the same primitives that animate window state); presenter notes; presenter mode with secondary display via Foundry's multi-display API; PowerPoint `.pptx` import via the same LibreOffice-filter Cardboard Box from M63; Keynote `.key` import treated like `.pages` (best-effort, schema-derived).
**Exit.** A 30-slide deck builds, exports to PDF, and runs in presenter mode driving an external display via DisplayPort and HDMI Alt-Mode.
**Dependencies.** M63's vendored filters, Stage multi-display.

#### M65 — Armory v1 (VS Code-class IDE) (14 weeks FT)
**Scope.** This is the most architecturally ambitious app in Phase 2.

- **Editor.** Multi-cursor (Sublime/VS Code semantics), ropes-based buffer (sharing the M63 rope implementation), tree-sitter port for semantic highlighting (tree-sitter is C, MIT, vendored in-tree; the grammar files are individually licensed and live in `vendor/tree-sitter-grammars/`).
- **LSP integration.** A from-scratch HolyC LSP server (~10 kLoC, budgeted conservatively at 14 weeks of overall Armory time, or ~4 weeks isolated). It must understand HolyC's `#help_index`, F5 hot-patch boundaries, and Brief-as-source. Bridges to external LSP servers for other languages, all of which run inside Tabernacles or as Cardboard Box subprocesses on the host: `clangd`, `rust-analyzer`, `gopls`, `zls`, `pylsp`, `tinygo` for Go-via-tinygo. The LSP wire protocol is unchanged; only transport differs.
- **Debugger.** A HolyC source-level debugger that **honours F5 hot-patch live-coding semantics** — when a user F5s a function, the debugger must dynamically remap breakpoints to the new code. This is one of the project's signature features; budget time for it. For non-HolyC languages, ship a thin `gdb`/`lldb` adapter via DAP (Debug Adapter Protocol).
- **Profiler.** A panel that hosts Listening Post v2's flame-graph and latency-histogram views (dependency on M75).
- **Refactoring.** Rename, extract function, find references — all driven through LSP; no Armory-specific refactoring engine.
- **Git.** libgit2 (GPLv2 with linking exception, OK for our model) vendored; UI is Brief-based change view, hunk staging, blame.
- **Integrated terminal.** Operator v1 (M67) embedded.
- **Source-as-documentation native.** F1-on-symbol opens the source location; M66 makes this universal.

**Exit.** Armory opens the entire Field OS source tree, indexes it in ≤ 30 s, jumps to definition across HolyC and Rust (via the Rust Tabernacle bridge), F5-hot-patches a HolyC function with the debugger attached, and commits a code change via libgit2.
**Dependencies.** M59–M61 (LSP bridges live in Tabernacles), M63 rope buffer, M67 Operator.

#### M66 — Source-as-documentation universal (Field Manual upgrade) (3 weeks FT)
**Scope.** Honour HolyC's `#help_index "Category;Topic"` directives across the entire base system. A small static-analysis pass walks the source tree, builds an index Brief document, links every directive to its source location and to its Field Manual chapter. F1-on-symbol in Armory or anywhere in Stage opens the source. The Field Manual itself is a Brief document tree under version control, shipped as part of `/Field Manual/`.
**Exit.** Every public symbol in the BSD-2 core has a `#help_index`; F1 in Manual on the word "rope" opens the rope buffer source; the Field Manual builds in CI as a search-indexed Brief tree.
**Dependencies.** M65.

#### M67 — Operator v1 (Warp/Wave-class block-UI terminal) (5 weeks FT)
**Scope.** Each command + its output is one Brief block. Hyperlinks in output (file paths matching a pattern, URLs, error locations from compiler output) are clickable and do the right thing per type (open in Manual, open in Recon, jump in Armory). AI-augmented commands are an opt-in feature that routes to a WASM Tabernacle host running an `llama.cpp`-style model (this is **not** a v1.0 promise — Operator ships with the block UI; AI augmentation is a Stockpile-installable extension). Reference: Warp's block UI (proprietary), Wave Terminal's notebook-style design (Apache-2.0).
**Exit.** A typical 30-command shell session is fully scrollable as a Brief, every block re-runnable, every output addressable as `block://session-id/N`.
**Dependencies.** Phase 1 shell, Brief format.

#### M68 — Calling Card v1 (identity / auth manager) (6 weeks FT)
**Scope.** A 1Password/Bitwarden-class credential manager. Stores: passwords, passkeys (FIDO2/WebAuthn), SSH keys, GPG keys, X.509 trust roots, OAuth refresh tokens. Encrypted at rest with a hardware-bound key (TPM 2.0 on Framework, Microsoft Pluton on Snapdragon X via the Pluton Crypto Provider, fTPM on AMD), wrapped by a user passphrase via Argon2id. Browser autofill is a Cardboard Box capability shared with Recon. Used by M78 (multi-user) as the user identity primitive.
**Exit.** A user can log into Field OS with a passphrase, unlock the vault, autofill a Recon login on a public site, sign a Git commit via stored GPG, and SSH into a remote machine — all without ever exposing the secret material outside the Calling Card daemon.
**Dependencies.** Phase 1 crypto stack, Recon, Comm Tower.

### Block D — Audio and photo creative apps (M69–M73) — 28–36 weeks FT

#### M69 — Wavelength v2 (audio stack maturity) (6 weeks FT)
**Scope.** Plugin architecture finalised:
- **LV2** (BSD-class license; designed for open ecosystems) — first-class, hosted in-process under Wavelength.
- **VST3** — the licensing landscape changed dramatically on **20 October 2025**, when Steinberg relicensed the VST3 SDK under MIT (it had been GPLv3-or-proprietary). This means the v0.1-era plan of forcing VST3 into a Cardboard Box for license isolation is **no longer required for license reasons**, although Cardboard Box hosting is still the right choice for **stability isolation** (a crashing plugin must not take down the host).
- **CLAP** (MIT) — added as a third tier; small implementation cost, real community uptake.
- **AU** — not supported on v1.0 (macOS-only in practice, and we don't ship on macOS).

MIDI 2.0 over Wavelength's existing transport (UMP packets, profiles, property exchange). Lower-latency target: **≤ 5 ms round-trip** on Tier-1 hardware (this is real-time-DAW territory, comparable to Linux PipeWire's good case).
**Exit.** A representative LV2 plugin (e.g., one from the LSP Plugins suite), a representative VST3 plugin (e.g., a free Surge XT VST3 build), and a representative CLAP plugin all load and run in Wavelength v2 without xruns at a 64-frame buffer at 48 kHz.
**Dependencies.** Phase 1 Wavelength.

#### M70 — Cassette v1 (Logic Pro-class DAW) (16 weeks FT)
**Final name recommendation: "Cassette."** Of the candidates (Studio, Console, Mixdown, Cassette, Reel, Soundboard, Field Recorder, Tape, Wire, Track, Live, Session, Take, Splice, Dub, Mic, Boom, Shotgun), **Cassette** is the strongest MGS3-warm tactical fit: cassette tapes are a *recurring diegetic element in MGS3* (Snake collects music tapes throughout Virtuous Mission and Operation Snake Eater), the word is short and unambiguous, it has nothing of the religious framing the brief excludes, and it pairs naturally with Wavelength as the underlying stack ("Cassette records to Wavelength"). Strong second choice: **Reel**. Tertiary: **Field Recorder** (slightly long).

**Scope.**
- Multi-track audio recording, 24/32-bit float internal, sample-rate-agnostic mixing graph.
- MIDI 2.0 throughout (UMP, per-note expression, profiles).
- Virtual instruments hosted as LV2/VST3/CLAP via Wavelength v2.
- Real-time effects: EQ, compression, reverb (convolution + algorithmic), delay, chorus, time-stretch (vendored Rubber Band Library, MIT), pitch correction (Rubber Band's formant-preserving mode).
- Automation lanes per parameter; clip-level + track-level.
- Mixing console UI (channel strips, sends, busses, groups; classic recording-studio metaphor).
- Project file format: HolyC-native, Brief-readable. A Cassette project is a Brief tree where each track is a sub-document and each clip is a referenced Brief block — i.e., projects are version-controllable, diff-able, reviewable as Brief.
- Export to WAV, AIFF, FLAC (libFLAC, BSD), MP3 (LAME via Cardboard Box for license clarity), AAC, Opus.

**Exit.** Build, mix, and bounce a representative 16-track project at 48 kHz/24-bit with five plugin instances, two automation lanes, and a side-chain compressor; bounce time ≤ 1× real-time on Tier-1 hardware.
**Dependencies.** M69, Stage compositor, Brief format.
**Effort.** 16 weeks FT is honest. A complete DAW is normally a multi-year team effort (Reaper took years, Ardour took 20+ years to get where it is). Cassette v1 is intentionally a **focused** DAW — multi-track + MIDI + plugins + automation + export. It is not Logic Pro at parity. It is "good enough that a working musician can finish a song without leaving Field OS."

#### M71 — Negatives v2 (Capture One-class RAW workflow) (10 weeks FT)
**Scope.** Upgrade Phase 1's Negatives v1.

- **ICCv4 colour management.** Display P3 native working space; Rec.2020 supported; soft proofing for sRGB and Adobe RGB; 16-bit and 32-bit float pipelines. LittleCMS 2 (LCMS2, MIT) vendored.
- **RAW pipeline.** LibRaw (LGPL-2.1 / CDDL dual; LGPL is fine for our linkage model) vendored. Camera profiles for Sony, Canon, Nikon, Fujifilm, Leica, Hasselblad, Phase One — sourced from the LibRaw / RawTherapee / dcraw heritage corpus.
- **Tone curves**, parametric (highlights / shadows / whites / blacks / clarity / dehaze).
- **Masking** — radial, linear, brush, **AI-assisted** (sky / subject / face detection) via Engine compute API (M73). The AI models are small ONNX networks shipped pre-quantised; on Snapdragon X they run on Hexagon NPU via M57's `fastrpc`, on AMDGPU/Xe via compute shaders.
- **Local adjustments** layered above the global pipeline.
- **Healing brush, perspective correction, lens corrections.** Lens database from the Lensfun project (LGPL).
- **Export** to JPEG, PNG, TIFF, **HEIF** (libheif, LGPL — careful linkage), **AVIF** (libavif, BSD), DNG (LibRaw), OpenEXR (BSD).

**Exit.** A 200-image RAW shoot (Sony A7 IV `.ARW`, Canon R5 `.CR3`, Fujifilm X-T5 `.RAF`) is imported, browsed in a contact-sheet view at 60 FPS on Tier-1 hardware, edited with masks and AI subject detection, exported as 16-bit TIFF for print and AVIF for web.
**Dependencies.** Engine v1 (M73), Foundry v2 (M72).

#### M72 — HDR / wide-gamut display path (4 weeks FT)
**Scope.** Foundry v2 surfaces:
- HDR10, HDR10+ (HDR10+ is a free open standard), Dolby Vision optional (Dolby Vision requires a license; defer to Phase 3).
- Display P3 native; Rec.2020 via tone-mapping for SDR displays.
- Display Stream Compression (DSC) for high-resolution panels (the Surface Laptop 7's 2304×1536 panel and the XPS 13's 2880×1800 OLED both use DSC).
- HDR primitives in Foundry's Vulkan-class API (HDR swapchain formats, EOTF metadata).

**Exit.** Negatives v2 displays a 12-bit HDR10 image correctly on the XPS 13 9345 OLED panel; Stage composites HDR and SDR windows correctly side-by-side without crushing either.
**Dependencies.** Phase 1 Foundry.

#### M73 — Engine v1 (OpenCL-class compute API) (6 weeks FT)
**Scope.** Productise the compute primitives stubbed in Phase 1's M19. A Field OS-native compute API, named **Engine**, that:
- Runs on AMDGPU (via the Phase 1 amdgpu LinuxKPI port and Mesa's RUSTICL or a direct Vulkan-compute backend).
- Runs on Intel Xe / i915.
- Runs on Adreno X1 (via Turnip's Vulkan-compute path; Mesa's RUSTICL-on-Turnip is in flight upstream).
- Runs on Hexagon NPU (via M57; second-class — exposed only when Qualcomm's QNN userspace is installed).
- Runs in software (CPU SIMD) as a fallback so apps can be authored against Engine without hardware-class assumptions.

Engine is **OpenCL-class**, not OpenCL-bit-compatible. It is a HolyC API (`engine_dispatch`, `engine_buffer_alloc`, etc.) with a Brief manifest describing kernels. SPIR-V is the kernel binary format.
**Exit.** Negatives v2's AI-masking ML models run via Engine on all three Tier-1 GPU paths plus Hexagon NPU on Snapdragon when available; Listening Post v2's GPU utilization graph shows compute load distinct from graphics load.
**Dependencies.** Foundry v2.

### Block E — System maturity and v1.0 (M74–M90) — 32–48 weeks FT

#### M74 — Filesystem expansion (8 weeks FT)
**Scope.** Four filesystems beyond RedSea II.

| FS | Mode | Strategy | Rationale |
|---|---|---|---|
| ext4 | R/W | Port Linux ext4 driver via LinuxKPI | The default Linux root FS; users dual-booting need this |
| btrfs | R/O | Port Linux btrfs read paths via LinuxKPI; defer write to Phase 3 | Read-only is enough for migration; btrfs write is operationally complex |
| exFAT | R/W | Write from scratch in HolyC against Microsoft's now-royalty-free spec | The Linux exFAT driver is GPL-only; cannot LinuxKPI-host cleanly. The exFAT specification is documented and tractable in a few thousand lines of HolyC. SD cards and external drives constantly use this |
| NTFS | R/W | Port Linux 6.12+ `ntfs3` driver (Paragon's contribution) via LinuxKPI | The modern in-tree ntfs3 is more permissively-coded than ntfs-3g and has been maturing since Linux 5.15 |

**Exit.** Each FS mounts a representative volume and round-trips a 1 GB write/read/sync test. exFAT passes the SDA's exFAT compliance vectors. NTFS round-trips ACLs and extended attributes against a Windows 11-formatted volume.
**Dependencies.** Phase 1 LinuxKPI shim.

#### M75 — Listening Post v2 (telemetry dashboard) (4 weeks FT)
**Scope.** A system observability dashboard (think Grafana, but rendered as Brief). Live metrics from every Patrol unit; flame graphs (vendored `inferno` analogue); latency histograms (HDR Histogram-class storage); Patrol unit health; Comm Tower stats; Wavelength latency / xruns; Engine GPU/NPU utilization; per-process resource attribution. UI is Brief-based — every chart is a live Brief block addressable via URL.
**Exit.** A user opens Listening Post, sees CPU/GPU/RAM/disk live, drills into Cassette and gets per-plugin CPU; the dashboard's round-trip latency from sample to render ≤ 100 ms.
**Dependencies.** Phase 1 Patrol units, Brief format.

#### M76 — Stamina v1 (resource monitor) (3 weeks FT)
**Scope.** User-facing companion to Listening Post v2 — Activity Monitor / Task Manager-class. Per-process CPU, memory (RSS, VMS, working set), disk I/O, network I/O, GPU compute and graphics. Energy impact estimation (a power-state heuristic, not a wattage measurement; Apple's Activity Monitor is the model). Integration with Codec for high-load notifications ("Cassette is using 95% of GPU; close it?").
**Exit.** Stamina opens in ≤ 100 ms cold; energy impact heuristic correlates ≥ 0.8 with actual battery drain over a 4-hour daily-driver session.
**Dependencies.** M75.

#### M77 — Recon v2 (browser maturity) (8 weeks FT)
**Scope.** Upgrade Phase 1's Recon. Servo (the BSD-2 / MIT Mozilla-spawned engine that the Linux Foundation Servo Project now stewards) at current; or — a critical decision point — switch to LibWeb (Ladybird's engine) now that Ladybird has spun out as a 501(c)(3) under Chris Wanstrath (July 2024) and is on a published path to alpha 2026 / beta 2027 / stable 2028.

**Recommendation: stay on Servo for v1.0; revisit LibWeb for Phase 3.** LibWeb is BSD-2 and architecturally appealing, but Ladybird's own roadmap places its **alpha** in 2026 and stable in 2028 — Field OS v1.0 cannot ship a browser less mature than Phase 1's. Servo is more of a known quantity in 2025–2026.

The lesson from SerenityOS that Phase 2 must heed: **do not let Recon become a "spinout-or-die" project.** Andreas Kling's exit from SerenityOS in June 2024 was specifically because Ladybird had outgrown SerenityOS as a hosting context. If Recon ever gets to that point in Field OS, the discipline is to keep the browser as a Field OS-first deliverable and resist the gravitational pull of "this engine deserves its own org." For solo-builder economics, you cannot sustain two organisations.

Recon v2 specifics:
- WebGPU support (via Foundry/Engine bridge — this is the obvious right call).
- WebRTC research-grade only; not a release blocker (defer to Phase 3).
- Better web-standards conformance (target ≥ 80% on Web Platform Tests subset for Phase 1's existing surface).
- Extension mechanism via WASM Tabernacles — a Recon extension is a `.fbox` containing a WASM component.

**Exit.** Recon v2 loads Hacker News, GitHub, Wikipedia, MDN, Gmail (read-only), Google Calendar (read-only), and Discord (read-only) without page-fatal errors; WebGPU samples (the hello-triangle and compute-shader demos) run.
**Dependencies.** Foundry v2, Engine v1.

#### M78 — Multi-user (4 weeks FT)
**Scope.** Calling Card v1 from M68 expanded into a full multi-user system. Separate Brief libraries per user (`/Brief/<username>/`), fast user switching (Stage holds two compositor sessions), parental controls (a profile attribute on Calling Card), family sharing primitives (a "Family" group in Calling Card with shared subscriptions). Patrol units run per-user instances under a per-user namespace.
**Exit.** Two users on the same Framework 13 AMD machine; switching takes ≤ 1.5 s; each user has their own Camo Index theme, Frequencies, vault, and apps.
**Dependencies.** M68, Phase 1 supervisor.

#### M79 — Network sync of preferences (3 weeks FT)
**Scope.** Camo Index theme, Frequencies, Calling Card vault, and Brief library subtrees syncable over user-controlled storage. Two backends:
- **WebDAV** (works against Nextcloud, Synology, OwnCloud, generic httpd-with-mod_dav).
- **S3-compatible** (works against AWS, Cloudflare R2, Backblaze B2, MinIO, self-hosted).

A self-hosted "Field Sync" server profile is a tarball you run on a Linux box; it's just a thin S3 + WebDAV façade. Encryption is end-to-end, derived from Calling Card; the server sees only ciphertext. **No iCloud, no Microsoft Graph** — the brief's "if any reverse-engineered API materializes — unlikely" is correct in 2026 and into 2027.
**Exit.** A user signs in on a second Field OS machine, pulls preferences and Brief libraries, makes a change, the change appears on the first machine in ≤ 10 s.
**Dependencies.** M68, Comm Tower.

#### M80 — Encrypted backups (4 weeks FT)
**Scope.** Time Machine-class continuous backup to external NVMe/HDD with RedSea II snapshot semantics — every hour, an immutable snapshot; retention policy is monthly forever / weekly for a year / hourly for a week. Encrypted-at-rest via per-volume key wrapped by Calling Card. Restore browser is a Brief-rendered timeline ("scroll back to last Tuesday at 14:00").
**Exit.** Hourly backup of a 100 GB working set; restore-of-deleted-file in ≤ 30 s; full-system restore to a new disk in ≤ 1 h for 100 GB.
**Dependencies.** RedSea II, M68.

#### M81 — Mobile Device Management (5 weeks FT)
**Scope.** Field OS MDM. Closely modelled on Apple's MDM Protocol Reference (publicly schematised at `github.com/apple/device-management`) plus Apple's Declarative Device Management (WWDC 2021–2024 evolution) for the modern proactive flow. Field OS's MDM is **self-hostable** — no equivalent of an APNs gateway; Field OS uses its own Comm Tower-based push channel. Profile types for v1.0:
- Wi-Fi configuration
- VPN configuration (WireGuard preferred; OpenVPN supported)
- Calling Card key escrow (optional)
- Stockpile allowlist (institutional app catalogues)
- Patch update windows
- Disk encryption requirement
- Camera/microphone disable

The reference Field OS MDM server is a Go program shipping in the Field OS SDK; institutions run it themselves. **Tightly bound scope** — institutional features can swallow Phase 2 if let, so anything beyond the above list is Phase 3.
**Exit.** A test institution enrolls 5 devices; pushes a profile that locks Wi-Fi and Stockpile to an allowlist; revokes a device.
**Dependencies.** Calling Card v1, Stockpile, Comm Tower.

#### M82 — Localization framework (3 weeks FT)
**Scope.** ICU (Unicode, Apache-2.0) vendored for locale-aware sorting, formatting, calendar, plural rules, BiDi. Translation file format: **Mozilla Fluent (FTL)** — chosen over XLIFF and gettext because Fluent is purpose-built for natural-language flexibility (the Russian/Polish/Arabic plural-form problem, the gendered nouns problem, message embedding) and is what a modern OS-class project should adopt. Field Manual is translatable; UI strings extracted to per-app `.ftl` files.
**Exit.** Pseudolocalisation works (Stage shows `[!! Hello, World !!]` style strings); switching the OS language at login changes every UI string in Stage and stock apps.
**Dependencies.** ICU, Fluent runtime.

#### M83 — Top-priority language ports (8 weeks FT)
**Scope.** Eleven languages: Spanish, French, German, Japanese, Mandarin (Simplified and Traditional, counted separately in Field OS), Portuguese (Brazilian + European), Russian, Korean, Arabic, Hindi.

- **RTL Arabic.** Plan for **4–8 weeks** of RTL-specific layout, font, and component testing alone. The brief is correct: RTL is harder than it looks. Mirroring affects Stage compositor (window decorations), Manual (paragraph direction), Briefing (slide builds), Armory (gutter direction in editor).
- **CJK fonts.** Noto Sans CJK (SIL OFL) shipped; ~300 MB of glyphs across the four CJK locales — 64 MB for Simplified Chinese, 60 MB for Traditional Chinese, 18 MB for Japanese, 16 MB for Korean (estimates per the Noto distribution).
- **Devanagari** for Hindi: Noto Sans Devanagari (SIL OFL).
- **Arabic shaping** via HarfBuzz (MIT), already a Phase 1 dependency.
- **Color emoji**: Noto Color Emoji (SIL OFL).

Translation labour is **not** the solo builder's. The pattern Phase 2 should adopt: ship the i18n framework, machine-translate the en-US source, recruit a community translator group via Stockpile; gate the release on professional review of the eleven priority languages by paid contractors (budget: ≈ $0.10/word × ~40,000 words × 11 languages ≈ $44,000 — this is the largest cash outlay in Phase 2 and must be planned).
**Exit.** All eleven languages render correctly; locale-aware date and number formatting works; RTL Arabic layout passes a defined accessibility audit.
**Dependencies.** M82.

#### M84 — Stable ABI declaration (2 weeks FT, much of it documentation)
**Scope.** Field OS declares three independently versioned ABIs at v1.0:
- **Kernel ABI** — syscall numbers, syscall semantics, struct layouts.
- **Brief format ABI** — the document format.
- **App SDK ABI** — the headers, library symbols, and protocols that apps link against.

Each follows semver-like rules: minor version may add but never remove or change; major version may break and requires a documented migration. v1.0 freezes all three at 1.0.0. The HolyC compiler emits ABI-version metadata per binary; Patrol verifies on load.
**Exit.** A document published on developer.field-os.org: "ABI Stability and Versioning." A test app built against v1.0 SDK runs on a v2.0 build (forward direction is mandatory; backward direction is best-effort).
**Dependencies.** All prior milestones must have settled their interfaces.

#### M85 — Field OS SDK as `.fbox` (3 weeks FT)
**Scope.** The SDK ships as a Cardboard Box install for **Mac (universal binary), Linux (x86_64 + aarch64), and Windows (x86_64 + aarch64)**. Contents:
- HolyC cross-compiler targeting x86_64, AArch64, and `wasm32-wasip2`.
- Field OS headers and stub libraries.
- Signing tools (`fbox-sign`, `stockpile-submit`).
- Simulator: a Field OS QEMU image with pre-baked Tier-1 hardware DTBs.
- Documentation (offline copy of developer.field-os.org).
- Sample apps (the M62 polyglot showcase, plus HolyC-native templates).

**Target install size: ≤ 1 GB** (the brief's number; achievable — Apple's Xcode is ~12 GB, Android SDK ~6 GB, the lighter target is realistic given Field OS's restraint).
**Exit.** A developer on a stock M3 MacBook Pro installs the SDK in ≤ 5 minutes, builds the M62 Rust sample, signs it, runs it in the simulator, and submits to Stockpile.
**Dependencies.** M52 (cross-compiler), M84.

#### M86 — Stockpile remote repository (4 weeks FT)
**Scope.** Centralised signed repository (`https://stockpile.field-os.org`) modelled on F-Droid + Flathub + a lighter App Store review. Submission flow:
1. Developer signs `.fbox` bundle with Calling Card.
2. Submits to a Stockpile review queue via `stockpile submit`.
3. Automated security scan (Patrol-unit dry-run, `fbox-lint` static checks against a deny-list, signing-chain verification).
4. Manual review by the project (initially: the solo builder; long-term: a volunteer reviewer rotation).
5. Approved apps publish to the public repository, signed by the Field OS organisation key.
6. A public review-time SLA: 5 business days at v1.0.

**Exit.** Stockpile remote is live; ≥ 10 third-party apps published at v1.0.
**Dependencies.** M85.

#### M87 — Developer documentation site (3 weeks FT)
**Scope.** `https://developer.field-os.org`. Tutorial series (build a HolyC app from zero; build a Rust app via Tabernacle; build a Python notebook; build a polyglot app). API reference auto-generated from `#help_index` via M66. Brief format spec. Camo Index theming spec (typography, colour, geometry; the corner-radius scheme 8/12/20 px, the 4 px spacing grid, the IBM Plex SIL OFL typography rules). Stage compositor protocol spec. Sample apps repository. Video tutorials (host on PeerTube + YouTube to avoid single-platform dependence).
**Exit.** The site is live, comprehensive, and indexable by Recon's web search.
**Dependencies.** All prior milestones (the documentation reflects them).

#### M88 — Performance and footprint v1.0 polish — "the Snow Leopard moment" (8 weeks FT)
**Scope.** No new features. Profile every app, every Patrol unit, every kernel hot path. Targets:

| Metric | Phase 1 v0.1 | v1.0 target | Method |
|---|---|---|---|
| Disk footprint | 6 GB | **≤ 7 GB** (slight expansion to absorb localization, Snapdragon binaries, additional apps) | Per-binary size budget; xz/zstd page compression on read-mostly data |
| Boot time on NVMe | 2.5 s | **≤ 2.0 s** | Critical-path linker; lazy-load non-essential Patrol units |
| Idle RAM | 220 MB | **≤ 200 MB** | Working-set audit; eliminate idle-time allocators |
| Login → desktop | 0.8 s | **≤ 0.6 s** | Pre-warm Stage; defer Calling Card vault unlock UI |
| Cold app launch (Cache, Stockpile, Frequencies) | n/a | **≤ 200 ms** | Snapshot-based app warm-up |
| Recon to first paint on a major site | n/a | **≤ 1.0 s** | Servo profile-guided optimisation |

The brief calls this "the Snow Leopard moment" and it is exactly that — a refinement-only release. The temptation to slip new features into M88 must be resisted; that is how v1.0 ships under-baked.
**Exit.** All targets met on the Tier-1 reference machines.
**Dependencies.** All preceding.

#### M89 — Release candidate cycle (12 weeks FT)
**Scope.** RC1, RC2, RC3, four weeks each. Public testing channel via Stockpile. **1,000 daily-driver beta users** instrumented through Calling Card + opt-in telemetry (Listening Post v2's data path, transparent and exportable). Bug triage tiers:
- **Sev-1**: data loss, security, system crash → blocks the next RC.
- **Sev-2**: regression from prior RC → must have a workaround or be fixed.
- **Sev-3**: cosmetic, edge cases → tracked, not blocking.

**Exit.** RC3 with zero open Sev-1 bugs and an external security audit (commercial firm; Trail of Bits / NCC Group / Cure53-class; budget ≈ $40,000–$80,000) signed off.
**Dependencies.** M88.

#### M90 — v1.0 release (4 weeks FT, includes press cycle)
**Scope.** Public release. Three Tier-1 hardware families certified (Framework 13 AMD, Framework 13 Intel, Snapdragon X reference: Lenovo T14s Gen 6 + Surface Laptop 7 secondary). Field OS SDK shipped. Stockpile remote live. Developer documentation site live. Press kit auto-generated; pitches placed in advance to Ars Technica, Phoronix, OSnews, The Register; Hacker News and `/r/osdev` and Lobste.rs posts queued for release-day at 09:00 ET. **No exclusives** — every outlet sees the same press kit at the same minute.
**Exit.** v1.0 is live; daily download count > 0; the project no longer says "alpha" or "beta" anywhere.

---

## 3. Calibration against comparable projects (year 4–5)

### 3.1 SerenityOS (year 4–5: 2022–2024)

What shipped: Ladybird matured from "simple HTML viewer" to "passes Acid3 and renders most of HN" inside SerenityOS. The project hit ~1,000 contributors. Two architectures (x86-64 and AArch64) with RISC-V experimental.

What stalled: Hardware support never approached daily-driver quality on bare metal — SerenityOS remained fundamentally a VM-first OS. The strict NIH policy that gave the project its identity also locked out third-party libraries that would have accelerated the browser.

What changed in year 5: Andreas Kling forked Ladybird off into a separate project on **3 June 2024**, stepped down as SerenityOS BDFL the same day, and on **1 July 2024** founded the Ladybird Browser Initiative as a 501(c)(3) with Chris Wanstrath. Ladybird then dropped the SerenityOS target and adopted a "relaxed NIH" policy that lets it use libcurl, Skia, HarfBuzz, simdutf. By March 2025 Ladybird ranked fourth on Web Platform Tests behind Chrome, Safari, Firefox; alpha is planned for 2026, beta 2027, stable 2028.

**Lesson for Field OS:** be wary of the spin-out-the-browser pattern. **Recon must remain a Field OS-first deliverable through v1.0.** If, during Phase 2, momentum on Recon becomes such that the natural next step is "Recon as its own org," the discipline is to defer that to **after** v1.0. A solo builder cannot sustain two organisations; SerenityOS has 1,000+ contributors and even there the split was painful.

### 3.2 Asahi Linux (year 3–5: 2023–2025)

What shipped: Conformant OpenGL ES 3.1 (mid-2023), conformant OpenGL 4.6 (2024), conformant Vulkan 1.3 (October 2024), **conformant Vulkan 1.4 on day one** (2 December 2024) — Asahi Honeykrisp was the first Vulkan 1.4 driver for Apple hardware, period. Fedora Asahi Remix went stable. The fully-integrated DSP audio story shipped (custom per-machine calibration). Steam + Proton + DXVK + FEX worked with muvm.

What stalled: M3 support is, as of October 2025, still "boots to a blinking cursor" via m1n1 — two years after first M1 daily-driver work. The Vulkan-loader package in mainstream distros lagged Vulkan 1.4 conformance for months even after Asahi's day-one release.

What changed: On **13 February 2025**, founder Hector Martin resigned as project lead; the project moved to collective governance.

**Lesson for Field OS:** pro-tier creative apps need pro-tier GPU drivers. The Adreno X1 driver in M55 must be **production quality** for Negatives v2, Cassette, Briefing, and Recon's WebGPU to be credible. Asahi proves it can be done by 1–2 lead engineers with the right architecture (Faith Ekstrand's NVK as Honeykrisp's foundation; Alyssa Rosenzweig's Mesa work; Asahi Lina's kernel work). Field OS's analogue is to **not** write the Adreno driver from scratch — vendor Mesa Turnip and put 90% of the effort into integration and conformance.

### 3.3 Redox OS (year 3–5: 2022–2024 of its modern era; 8–10 of its overall lifespan)

What shipped: Dynamic linker (RSoC 2024); GCC, Binutils, Make, Bash dynamically linked (December 2024); ARM64 dynamic linking (January 2025); a brand-new Deficit Weighted Round Robin scheduler; capability-based security; basic Intel modesetting (2025); ifconfig port.

What stalled: 1.0 is still indeterminate. relibc maturation has been steady but slow. Orbital compositor evolution has been steady. The honest read: **Redox is on a 12+ year arc to 1.0.**

**Lesson for Field OS:** the LinuxKPI bet is the difference. Redox writes everything from scratch in Rust because ideologically that is the project; Field OS writes the *novel* parts in HolyC and *reuses* the well-trodden parts (drivers, pipelines, codecs) via the LinuxKPI shim. This is why Field OS targets v1.0 in 5 years and Redox does not.

### 3.4 Haiku R1 Beta era (2018–2024)

Cadence: R1 Beta 1 (2018), Beta 2 (2020), Beta 3 (2021), Beta 4 (2022), **Beta 5 (13 September 2024)** — 1.5 years after Beta 4. Each beta resolved several hundred bugs; Beta 5 alone closed ~350 tickets.

What's still missing for R1 stable: hardware-3D acceleration, Wayland compatibility, certain modern hardware support, full POSIX conformance.

**Lesson for Field OS:** **don't ship "1.0" with caveats.** It is better to be Field OS 0.9 daily-driver-quality than Field OS 1.0 with disclaimers. The 1.0 expectation will be enormous: the press cycle around v1.0 will compare ruthlessly to macOS, Windows, GNOME, KDE; the OS must hold up. If at the end of M89 RC3 there are genuine showstoppers, ship v0.9 first and let v1.0 follow when it earns the number. Haiku's discipline of staying in beta for fifteen years rather than ship a too-early R1 is the reference behaviour.

---

## 4. The Snapdragon X port strategy in depth

### 4.1 Why Snapdragon X (not Apple Silicon, not Ampere, not RISC-V)

| Dimension | Snapdragon X | Apple Silicon | Ampere Altra | RISC-V (e.g. SiFive HiFive Premier) |
|---|---|---|---|---|
| Boot | Standard UEFI + ACPI | Custom (m1n1, devicetree) | Standard UEFI + ACPI | Standard, but immature |
| PCIe | Standard | Standard but quirky | Standard | Standard |
| GPU openness | Mesa Turnip (production) | Honeykrisp (Asahi RE; production but RE-derived) | None | Limited |
| Reference machines | 6+ OEMs, retail | One vendor, retail | Server only, $2k+ | Developer boards only |
| Linux mainline | Already in 6.15+ | Out-of-tree (Asahi) | Already in mainline for years | Improving |
| User base | Microsoft pushing aggressively 2025–2026 | Macs (large but RE-only) | Server niche | Hobbyist |
| Effort to port | **Mostly a port** | **Reverse engineering** | Easy but irrelevant audience | Easy but irrelevant audience |

Snapdragon X is the right answer because everything but the GPU and NPU is "type your existing Phase 1 driver list, change x86 → arm64, run."

### 4.2 The HolyC retargeting question — addressed in M52 (QBE recommended)

### 4.3 LinuxKPI for ARM64

LinuxKPI on ARM64 is newer ground than LinuxKPI on x86_64, but the precedent is real: FreeBSD's `drm-kmod` runs on ARM64 (the Pi 4 path); FreeBSD's Lendacky-style Snapdragon work in 2024 hit ARM64-specific interrupt-handling issues that were upstreamed within days. Field OS's LinuxKPI shim from Phase 1 needs ~1,500–2,000 lines of new architecture-specific glue to support AArch64 — atomics, barriers, cache maintenance, exception-context conversion. This is bundled inside M53.

### 4.4 The Adreno GPU driver

- **Userspace Vulkan**: Mesa's **Turnip**, Vulkan 1.3 (1.4-capable in current Mesa). Vendored.
- **Kernel-side**: the Linux **MSM** DRM driver (`drivers/gpu/drm/msm`). Ported via LinuxKPI.
- **OpenGL**: not a Field OS first-class API. If a Cardboard Box app needs OpenGL, it gets it via Mesa's **Zink** layered on Turnip.
- **A7xx Gen 7 (Adreno X1-85)**: supported in current Mesa; the device-ID-only enabling that hit Phoronix in 2024 was the bring-up step.
- **A8x Gen 8 (Snapdragon X2 future)**: merged into Mesa 26.0 in late 2025 / early 2026 — Field OS catches this for free at Mesa pin updates; this is a Phase 3 platform.

### 4.5 Power management — the second-longest pole

Snapdragon X has 12 Oryon cores in two clusters with shared L3, plus the Adreno GPU, plus the Hexagon NPU, plus a wide spread of IP cores each with independent clock and power domains. The Linux subsystems doing this work upstream:

- `qcom-cpufreq-hw` — CPU frequency scaling.
- `rpmh` — Resource Power Manager hardened, the SoC-level resource arbitrator.
- `rpmpd` — power domains.
- `interconnect` — fabric bandwidth voting (this is the famous "you forget to vote and your DDR runs at 200 MHz" gotcha).
- The Adreno **GMU** for GPU power.

Field OS routes all of this through LinuxKPI. M56 budgets 5 weeks; reality may be 7. Linaro's Snapdragon X enablement work is the canonical reference and is in active mainline as of 2025.

### 4.6 Timeline

**6–9 months full-time / 12–18 months part-time** for Block A (M51–M58) — consistent with the Phase 2 overall budget.

---

## 5. The Hybrid Polyglot Strategy in depth

### 5.1 Why WASM (and not POSIX, not a CLR/JVM, not a Linux compatibility layer)

- **Secure-by-default**: WASM modules cannot read arbitrary memory; capability-passed.
- **Language-agnostic**: Rust, C, C++, Zig, Go, Python, JavaScript, .NET, JVM all target it.
- **Predictable**: AOT compilation via Cranelift gives stable, fast code with no surprise GC pauses for compiled languages.
- **Standardised**: WASI 0.2 is W3C/Bytecode Alliance work; WASM itself is W3C. Field OS does not own this surface; it consumes it.
- **POSIX-free**: Field OS will *never* be POSIX-compliant, by design. WASM does not require a POSIX kernel underneath.
- **Cost-honest**: WASM is roughly 10–20% slower than native for compiled languages, sometimes faster than naïve native via Cranelift's type-aware optimisations. Acceptable for daily-driver apps; **not** acceptable for kernel paths or audio DSP inner loops — which is why the kernel and Wavelength's hot paths stay HolyC-native.

### 5.2 WASI 0.2 vs 0.1 vs 0.3

- **0.1** (`wasi_snapshot_preview1`, 2019): files, args, env, clocks, random. No networking, no threads, no composition. Insufficient.
- **0.2** (25 January 2024): files, args, env, clocks, random, **TCP/UDP sockets, HTTP**, refactored into the **Component Model** with WIT typing. **This is the v1.0 target.**
- **0.3** (RC in Spin v3.5, November 2025; final in 2026): native async I/O, futures and streams in WIT. Field OS adds it as a Phase 3 Tabernacle update.
- **1.0** (planned 2026): the long-term-stable surface. Field OS adopts when it ships.

### 5.3 Wasmtime over Wasmer

Both are good. Wasmtime wins on: Bytecode Alliance backing, mature production deployment (Fastly Compute@Edge, Shopify Functions, Fermyon Spin, AWS Lambda extensions), better WASI 0.2 support since Q1 2024 (it was the first major runtime with full Component Model loading), Apache-2.0 (clean), Cranelift codegen, and the new 2024–2025 LTS support windows that suit a v1.0-stable promise.

### 5.4 Performance budget

Field OS commits to: a Wasmtime Tabernacle starts in ≤ 100 ms cold (typical) and ≤ 30 ms hot (cached); a "hello world" component dispatch round-trip is ≤ 5 ms; representative compiled-language workloads are ≤ 1.2× native CPU time on x86_64 and ≤ 1.3× on AArch64. These are conservative; current Wasmtime + Cranelift often beats those numbers.

### 5.5 Security model

Tabernacles inherit the Cardboard Box capability surface (filesystem subtrees, network endpoints, IPC channels, GPU/Engine access) **and** add WASM's memory-safety guarantees as an additional layer. A Tabernacle that escapes its WASM sandbox still cannot escape its Cardboard Box. Defence in depth: two unrelated mechanisms must fail for a compromise.

### 5.6 Showcase — M62 deliverables

Already enumerated.

---

## 6. The pro-creative-app strategy

### 6.1 Why ship Pages-class and VS Code-class

At v1.0, Field OS must be **defensible as a serious pro tool, not a hobby OS with toy apps.** Reviewers and prospective users will compare to Pages, Word, Google Docs, VS Code, JetBrains, Logic Pro, Capture One, Lightroom. The bar is "can I do my day's work?" — not "is this a clever project?"

### 6.2 Manual v1 architecture

- **Buffer**: rope (B-tree of UTF-8 chunks, max chunk 1 KB, copy-on-write).
- **Layout**: TeX-class line breaker; a CSS-class page model on top. Field OS does not invent a layout language — it uses CSS-3 for box semantics and a Knuth–Plass extension for paragraph composition.
- **Tables**: full CSS table model.
- **Master pages**: a page is a Brief block; master pages are Brief block templates.
- **Comments / changes**: OOXML revision-mark schema (so round-trip with Word is faithful).
- **Imports**: LibreOffice filters via Cardboard Box for Word/Pages/RTF/ODT.
- **PDF**: PDFium via Cardboard Box.

### 6.3 Armory v1 architecture

- **Editor**: rope, multi-cursor primitives borrowed from Sublime/VS Code conventions.
- **LSP**: HolyC LSP server is from-scratch (~10 kLoC); other languages bridged via Tabernacle subprocesses.
- **Debugger**: HolyC source-level + DAP for everything else; F5 hot-patch is a first-class debugger feature, not an external command.
- **Refactoring**: through LSP (no engine of our own).
- **Git**: libgit2 vendored.

### 6.4 Cassette v1 architecture

- **DSP graph**: per-track ring buffers, sample-clock-driven, lock-free; the same primitives Wavelength v2 already exposes.
- **Plugin hosting**: LV2 in-process (process-isolated only optionally — most LV2 plugins are well-behaved); VST3 in a Cardboard Box for stability isolation despite Steinberg's October 2025 MIT relicensing of the SDK; CLAP in-process.
- **MIDI 2.0**: UMP throughout; per-note expression first-class.
- **Project format**: HolyC-native, Brief-readable; project is a Brief tree, version-controllable.
- **Time-stretch**: Rubber Band Library (MIT) vendored.
- **Pitch correction**: Rubber Band's formant-preserving mode.

### 6.5 Negatives v2 architecture

- **Pipeline**: stage-based — RAW decode (LibRaw) → linearise → demosaic → white-balance → camera profile → tone curve → local adjustments → output transform → render.
- **Colour**: LCMS2; ICCv4; 32-bit float internal.
- **Masking**: classical (radial, linear, brush) plus ML (small ONNX nets via Engine).
- **Lens corrections**: Lensfun database.
- **Output**: JPEG, PNG, TIFF (libtiff, BSD), HEIF (libheif), AVIF (libavif), DNG, OpenEXR.

### 6.6 Effort

Each pro app is **6–12 months of dedicated work** in a normal team. Together they're the bulk of Phase 2's calendar (Block C + Block D ≈ 64–84 weeks FT). This is correct and unavoidable; trying to compress it produces toys.

---

## 7. v1.0 release readiness criteria — concrete and testable

| Criterion | Threshold | How tested |
|---|---|---|
| Boot time | ≤ 2.0 s on Tier-1 NVMe | Automated boot-timer in CI, three machines |
| Login → desktop | ≤ 0.6 s | Same |
| Cold app launch (Cache, Stockpile, Frequencies) | ≤ 200 ms | Same |
| Recon to first paint on Wikipedia front page | ≤ 1.0 s | Same |
| Idle RAM | ≤ 200 MB | `stamina --idle-snapshot` after 5 min idle |
| Daily-driver beta users | ≥ 1,000 active for ≥ 30 days each | Calling Card + opt-in telemetry, anonymised |
| Tier-1 hardware | 3 families certified | Acceptance test suite per family |
| SDK shipped | Mac/Linux/Windows installs | Manual verification + CI |
| Stockpile remote | ≥ 10 third-party apps published | Public count |
| Localization | 11 languages including RTL Arabic | Pseudo-locale CI + native-speaker review |
| Accessibility audit | Passed by external reviewer | WCAG 2.2 AA equivalent |
| Security audit | Passed by external firm | Trail of Bits / NCC Group / Cure53-class |
| SBOM | Published, machine-readable | SPDX 2.3 |
| License inventory | Verified, no GPL contamination of BSD-2 core | `licensecheck` CI |

---

## 8. Skill-building / ramp-up reading list for Phase 2

**ARM64 architecture**
- ARMv9-A Architecture Reference Manual (free, Arm Developer Hub).
- Snapdragon X Elite Product Brief (Qualcomm publishes a sanitised version).
- Linaro's Snapdragon X documentation hub.

**WASM**
- WASI 0.2 specification (`bytecodealliance.org/articles/WASI-0.2`).
- Component Model documentation (`component-model.bytecodealliance.org`).
- Wasmtime book and developer docs.
- Bytecode Alliance blog (the WASI 0.2 launch post by Dan Gohman; the LTS-windows announcement).

**LSP**
- The Language Server Protocol specification (Microsoft, MIT).
- Tree-sitter documentation.
- The rust-analyzer architecture document (rust-analyzer.github.io); clangd source as reference.

**DAW architecture**
- PipeWire's audio graph design documents.
- JACK's connection model.
- The Reaper extension SDK (instructive for plugin architecture).
- The 1998–2002 BeOS pro-audio-era documents (BeOS was *targeted* at pro audio; the architectural lessons for a desktop OS are gold).

**RAW pipeline**
- LibRaw source.
- The dcraw heritage documents.
- ICC v4 specification.
- darktable's documentation on camera profiling and demosaicing.
- The various Capture One reverse-engineering blog posts (community resources; not Capture One's own).

**Localization**
- The ICU User Guide.
- Mozilla's Fluent documentation (especially the Pontoon UI design notes — they teach you what real localisers need).

**MDM**
- Apple's MDM Protocol Reference (the public schema mirror at `github.com/apple/device-management`).
- Open Mobile Alliance Device Management.
- The OMA-DM tutorial corpus (older but foundational).

**Reference small-team OS pro-app stories**
- SerenityOS Pixel Paint (Andreas Kling's photo editor — proof of concept that a small team can ship a respectable raster editor).
- SerenityOS Sound Player and Piano.
- The BeOS pro-audio era (1998–2001) — Be was specifically targeted at pro-audio; their architectural choices are informative for Wavelength + Cassette.

---

## 9. Tooling additions for Phase 2

- **Cross-architecture CI**. Three hardware-in-the-loop rigs: Framework 13 AMD, Framework 13 Intel, Lenovo T14s Gen 6 (Snapdragon X). Each runs a 30-minute acceptance suite per merge-to-main; full suite nightly.
- **WASM toolchain integration**. Wasmtime pinned in CI; `wasm-tools validate` on every component artifact; WASI 0.2 ABI compatibility check.
- **Localization CI**. Every PR that touches a `.ftl` source runs `compare-locales`-equivalent (a HolyC port — small) to flag missing or obsolete strings.
- **Automated press kit generation**. A CI job that, on tagged release, generates: a press release Markdown, a screenshot bundle from the simulator (HiDPI PNGs in light/dark), a 90-second screen-capture video, an SBOM, a license inventory, an "About this release" Brief. Output: a `press-kit.tar.zstd` posted alongside the binaries.
- **LoC budget tracking**. The 100,000-line budget for the BSD-2 core is enforced in CI: if a PR pushes the count over, it fails. A weekly dashboard at `dev.field-os.org/loc` shows the trajectory.

---

## 10. Risk register for Phase 2

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Adreno X1 GPU port (M55) slips beyond budget | High | High | +6 weeks of contingency; track Mesa Turnip mainline weekly; if Mesa's A7xx maturity drops, fall back to a software-rendered Stage on Snapdragon for v1.0 — a documented degradation, not a release blocker |
| HolyC AArch64 retargeting (M52) takes 14 weeks not 10 | Medium | High | QBE backend pre-selected to minimise scope; if QBE proves inadequate, fall back to LLVM with documented dependency-footprint cost |
| WASI 0.3 finalises during Phase 2 and the Component Model surface drifts | Medium | Medium | Pin to WASI 0.2; commit the Phase 3 0.3-upgrade plan in writing; the Component Model's virtualisability means a 0.2-in-0.3 polyfill is straightforward |
| Pro-creative apps ship under-baked v1 | High | High | Tightly scope v1 features (M63, M65, M70, M71); resist "Logic Pro at parity"; explicitly position Cassette v1 as "good enough that a working musician can finish a song without leaving Field OS" — not "Logic killer" |
| HolyC LSP server (inside M65) runs over budget | Medium | Medium | Architect as a thin layer over the existing HolyC compiler's symbol table; reuse, don't reinvent. If it slips, ship Armory v1 with HolyC LSP at "find references + go-to-definition" only, expand in 1.x |
| MDM scope creep (M81) | High | Medium | Profile types are a closed list at v1.0 (the seven types in M81); anything else is Phase 3 |
| RTL Arabic subtle bugs | High | Low (per-bug) but cumulatively high | Budget a full 4–8 weeks RTL pass; recruit one Arabic-fluent reviewer paid for the duration |
| The "v1.0" expectation becomes a press-cycle ambush | Medium | High | Pre-brief Phoronix and Ars; do not promise feature parity with macOS/Windows; publish the comparison table the press will publish anyway, on our own terms; ship v0.9 if v1.0 is not ready |
| External security audit finds Sev-1 issues during M89 | Medium | High | Schedule the audit at start of M89, not end; budget two RCs of fix-time |
| Solo-builder burnout | High | Project-ending | Enforce 35h/wk maximum; take real breaks between M58, M68, M73, M83; this is unfortunately the most important risk on the list |

---

## 11. Phase 3 preview (motivating Phase 2 discipline)

Phase 3 is plausibly **24–36 months part-time / 12–18 months full-time after v1.0**. Its likely scope:

- **Apple Silicon support** via Asahi-style RE — likely a multi-year collaboration with the Asahi project under its post-Hector Martin collective governance.
- **Tablet form factor** — Snapdragon X tablets (when they ship in volume), Surface Pro pen, multi-touch, on-screen keyboard.
- **Cellular modem support** — 5G via standard MBIM.
- **Vector graphics editor** (Illustrator-class), **video editor** (DaVinci Resolve-class), **3D modeling** (Blender-class) — each multi-year.
- **Server/cloud edition** — a Field OS profile for headless workloads (no Stage; no Wavelength; minimal Patrol units).
- **WASI 0.3 / 1.0** Tabernacle update.
- **HW3D-equivalent** for any Tier-2 hardware family that Phase 2 deferred.

**The discipline is to keep all of this out of Phase 2.** If a Phase 3 feature can be packed into Phase 2 "for free," it cannot — the integration cost alone destroys v1.0's calendar.

---

## 12. Closing

Phase 2 is the bridge between "an interesting hobby OS that boots on Framework hardware" and "a genuinely useful pro tool that 1,000 people daily-drive across three architectures." The architectural choices that make it feasible:

- **LinuxKPI for the second architecture** (Phase 1 set this up; Phase 2 cashes in).
- **WASM Tabernacles for the polyglot story** — never a Linux compatibility layer, never POSIX.
- **Vendored libraries with Cardboard Box isolation** for the things that genuinely take person-decades to write from scratch (PDFium, LibRaw, libheif, Wasmtime, Mesa Turnip, ntfs3, ext4, Servo).
- **HolyC at the centre** for everything novel — kernel, supervisor, compositor, runtime, Brief format, Manual rope buffer, Cassette mixing graph, Armory editor — preserving the TempleOS technical heritage and the source-as-documentation, F5 hot-patch, executable-document-format identity.
- **The BSD-2 core stays under 100 kLoC** and is tracked in CI.
- **Three independently versioned ABIs** declared stable at v1.0: kernel, Brief format, app SDK.

The honest forecast is that **a solo builder at 35 h/wk completes Phase 2 in 12–18 months; at 15 h/wk, in 24–36 months**. Combined with Phase 0 and Phase 1, v1.0 is a **4–6 calendar-year project from M0**. That timeline is not aspirational; it is calibrated against SerenityOS's year 4–5, Asahi Linux's year 3–5, Redox's grind toward 1.0, and Haiku's fifteen-year R1 Beta cadence — the comparable solo-and-small-team OS efforts of the 2018–2025 era.

If everything in this plan executes, Phase 3 inherits a stable ABI, three Tier-1 hardware families, eleven languages, a developer ecosystem, a real signed-app repository, and the credibility to attempt Apple Silicon, tablets, and the next tier of pro-creative apps.

— *End of Phase 2 Engineering Plan, M51–M90.*