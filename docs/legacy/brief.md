# Field OS — Revised Design Brief

*A serious, single-system desktop OS that synthesizes Big Sur's visual language, Snow Leopard's stability ethos, Apple's Human Interface principles, the technical core of TempleOS, and the warm tactical professionalism of the Metal Gear Solid 3 universe.*

---

## 0. Executive summary

Field OS is a small, single-team operating system for commodity x86_64 hardware. It takes its **engineering ethos** from Mac OS X 10.6 Snow Leopard ("no new features as a flagship feature": refinement, smaller-on-disk, system-level concurrency, a single coherent compiler/runtime, sandboxing as foundation). It takes its **visual and interaction language** from macOS 11 Big Sur (translucent vibrancy materials, full-height sidebars, inline toolbars, monochrome symbol iconography, deference to content, restrained accent theming). It takes its **interaction principles** from the shared Apple Human Interface Guidelines (clarity, deference, depth, consistency, hierarchy, harmony; accessibility as a first-class system service). It takes its **technical core** from TempleOS — and *only* the technical core — preserving HolyC as the universal language, a unified executable-document format, the shell-as-compiler REPL model, F5 hot-patching live coding, source-as-documentation auto-indexing, and the discipline of a hard line-count budget. And it takes its **aesthetic discipline** from Metal Gear Solid 3: Snake Eater — a 1964 Cold War analog warmth of olive drab, weathered brass, canvas, and signal red, expressed not as cosplay but as restraint, infrastructure-grade typography, and a sense that the system is a well-maintained kit rather than a consumer toy.

The original brief used a religious naming register throughout. That register is removed here in full. Naming, where evocative at all, draws on thematic infrastructure terms from the MGS3 universe (Codec, Cipher, Patrol, Outpost, Cache, Radar, Field Manual, Briefing) — never on character or faction names, never on overt HUD/CRT/scanline cosplay, and never on biblical or "holy" metaphor. The language remains called HolyC because that is its name in the literature; the system around it is wholly secular and tactical.

**Hard constraints carried over from the previous brief:**

- Core OS line-count budget: **≤ 100,000 lines of HolyC + assembly** for kernel, supervisor, compositor, language runtime, document format, shell, and base file manager. Apps live outside this budget.
- Single-language base system: HolyC for everything inside the budget. C/C++/Rust permitted only for hardware drivers and ported third-party libraries (browser engine, codecs, ML runtimes).
- Smaller on disk than its conceptual predecessors: target installation footprint **≤ 6 GB** for the full base system including default apps.
- Deference: the OS chrome must visually disappear when content is the subject. No system flourish ever fights user content for attention.
- Accessibility is non-negotiable and shipped at v1.0, not bolted on later.

---

## 1. Naming

### 1.1 The OS

The previous "Cathedral OS" name and its attendant register (Solomon, Covenant, Tabernacle, Levite, Cell, Reliquary, etc.) are removed. Replacement candidates, with rationale:

| Candidate | Rationale |
|---|---|
| **Field OS** *(primary recommendation)* | Connotes the field manual, field equipment, field operative. Restrained, professional, generic enough to live on letterhead and version banners, specific enough to set a tone. Pairs naturally with sub-naming ("Field Console," "Field Manual," "Field Notes"). |
| Codec OS | Centers the communications metaphor; strongest if the brand voice leans heavily on the call/Codec interaction pattern. Risk: collides with the audio-codec sense of the word. |
| Outpost OS | Small, defensible, well-stocked position. Good for a hobbyist/indie OS framing. |
| Garrison OS | Fortified, well-equipped. Slightly heavier than Outpost. |
| Bivouac OS | Temporary but disciplined encampment. Best for a community-driven, lightweight framing. |

The remainder of this document uses **Field OS** as the canonical name.

### 1.2 Themes (Camo Index)

The user-facing theming gallery is called the **Camo Index** — a direct mechanical reference to MGS3's camouflage system, where the player tunes appearance to environment. Each theme is one entry in the Camo Index. Light/dark variants are paired where it makes sense.

| Theme | Mode | Purpose |
|---|---|---|
| **Jungle** | Light (default) | The default look. Warm white, olive sidebar tint, brass accent. |
| **Khaki** | Light | Warmer, lower-contrast variant for long reading sessions. |
| **Briefing Room** | Light | Formal, paper-like; high-contrast serif-weighted typography for documentation work. |
| **Recon Night** | Dark (default) | The default dark look. Deep olive-black, subtle warm vibrancy. |
| **Tactical Black** | Dark | Pure-black OLED-friendly variant. Maximum content deference. |
| **Splitter** | Dark | High-contrast accessibility theme; thicker rules, denser keylines, no vibrancy. |
| **Olive** | Mono | Single-hue green monochrome; useful as a "focus mode" desktop. |
| **Snow** | Light/seasonal | Cool, high-key winter variant; the sole intentionally cool palette in the catalog. |
| **Tigerstripe** | Accessibility | High-contrast accessibility theme tuned for low-vision users; works with Reduce Transparency. |
| **Field Standard** | Legacy | The 16-color, 8×8 bitmap-font, 640×480-faithful TempleOS-respect theme. Bootable and runnable as a full session. The legacy is honored as a preserved technical artifact, not as a religious one. |

### 1.3 Components, services, apps

Naming is thematic infrastructure, never character or faction names. The full mapping is in §6 (Component & service catalog) and §8 (Daily-driver apps).

---

## 2. The three lineages

### 2.1 Snow Leopard: the engineering ethos

The Snow Leopard release (Mac OS X 10.6, August 2009), as documented in detail by John Siracusa's Ars Technica review, was Apple's clearest example of a refinement release. Its flagship feature was the absence of flagship features. Field OS adopts five concrete commitments from that release:

1. **A universal concurrency primitive at the system level.** Snow Leopard introduced Grand Central Dispatch (libdispatch) so that any well-behaved program could express work as queued blocks and let the system schedule cores. Field OS ships a HolyC-native equivalent — the **Patrol** dispatch system (see §7) — exposed both as a kernel scheduler primitive and as a shell verb. Concurrency is a system service, not a per-app project.
2. **A universal compute primitive for the GPU.** OpenCL was, in Siracusa's words, "Apple's baby." Field OS ships **Engine**, a HolyC-callable compute API over Vulkan-class hardware, plus **Foundry**, the corresponding graphics API. Both share buffer allocation with the compositor.
3. **A single, owned compiler stack.** Apple committed completely to Clang/LLVM with Snow Leopard. Field OS commits completely to its in-tree HolyC compiler. There is exactly one compiler in the base system. It is JIT, it is the shell, it is the build system.
4. **A universal service supervisor.** launchd in Snow Leopard era unified `init`, `cron`, `inetd`, `xinetd`, `mach_init`, and `watchdogd` into one supervisor with one configuration model. Field OS's **Patrol** plays exactly this role — every long-running thing in the system, from the audio stack to the window server to background indexers, is a Patrol unit, started, supervised, and re-started by the same code path with the same declarative configuration.
5. **Sandboxing and code signing introduced as foundation.** Snow Leopard introduced the seatbelt sandbox and quarantine attributes. Field OS ships sandboxing at v1.0 as **Cardboard Box** — an unfussy, capability-style container that wraps every third-party binary by default, with a HolyC manifest declaring required capabilities (filesystem subtrees, network endpoints, hardware classes).

The disposition is the same as Snow Leopard's: the version 1.0 changelog should be short, the diff against any previous toy version should be mostly subtractions, and the result should be measurably faster, smaller, and more stable than what came before. Where the old brief invoked a Cocoa Finder rewrite as inspiration, Field OS commits to a single, from-scratch file manager (**Cache**, §8) written entirely in HolyC against the native UI toolkit, with no legacy code paths.

### 2.2 Big Sur: the visual language

macOS Big Sur (announced WWDC 2020, "Adopt the new look of macOS," session 10104) reshaped the Mac around five visual ideas that Field OS adopts directly.

- **Full-height translucent sidebars.** The sidebar runs from the very top of the window to the very bottom, interrupting the toolbar and any status footer. This shifts the user's focus from a centered title to a left-anchored navigation. Field OS adopts this pattern as the default for any app with a primary navigation list.
- **Inline toolbars with no chrome borders.** Toolbar buttons are monochrome glyphs floating in a continuous translucent surface, with no rounded-rect bounding boxes. The title sits inline with the controls. Field OS does the same.
- **Vibrancy materials.** Sidebars, popovers, sheets, the menu bar, and the dock use platform-blurred desaturated translucency rather than flat fills. Field OS implements this as four named material levels (see §5.4), tuned warm for the MGS3 register rather than the cool gray of macOS.
- **Symbol iconography.** Big Sur introduced SF Symbols as a typographically aware, weight-matched glyph library used everywhere from sidebars to toolbars to controls. Field OS ships **Field Symbols**, an open-licensed equivalent (§5.6).
- **Restrained accent theming.** Big Sur lets each app choose an accent color that flows through its sidebar glyphs, selection highlights, sliders, and controls — but the system-wide accent is still chosen by the user from a small, curated palette. Field OS adopts the exact same architecture; the curated system palette is MGS3-warm rather than iOS-cool (§5.2).

Field OS also adopts Big Sur's specific numerical conventions:

- **Corner radii: 8 px / 12 px / 20 px.** 8 px for inset list rows, controls, popover tips. 12 px for sheets, popovers, secondary windows. 20 px for primary windows and the dock. (These are the corner radii Big Sur popularized.)
- **4 px spacing grid.** Every component edge, padding value, and gutter is a multiple of 4 px. Major rhythm at 8/16/24/32.
- **Optical-sized variable typography.** UI uses the appropriate optical size of the system typeface for body vs. display vs. micro labels — see §5.5.
- **Inset selection style.** Sidebar and list selection is an inset rounded-rect highlight, not a full-bleed bar.
- **Buttons appear when needed.** Controls fade in on hover/focus and out otherwise, except when explicitly persistent.
- **System sounds are remastered, not removed.** Tones remain functional; their timbre is warmer.

### 2.3 Apple HIG: the principles, applied

Apple's Human Interface Guidelines name **clarity, deference, depth, consistency, hierarchy, and harmony** as the foundational principles. Field OS subscribes to all of them and codifies them in its public design document (the **Operating Principles**, §11).

- **Clarity.** Type is legible at any size. Icons are precise. Adornment is minimal. A button must look like a button.
- **Deference.** The interface stays in the background; user content owns the screen. Translucency exists to sit *behind* content, not on top of it.
- **Depth.** Layers, subtle shadow, and motion communicate hierarchy in 2D. Field OS treats translucency as the primary depth cue and shadow as the secondary cue.
- **Consistency.** A Field OS app that does not implement the standard sidebar, toolbar, sheet, popover, alert, table, list, and form components is non-conformant. The framework provides them; apps consume them.
- **Hierarchy & Harmony.** Type sizes, spacing values, and color tokens are a small finite set. Apps compose from this set; they do not invent new ones.

Field OS hard-bakes four accessibility primitives, equivalent to Apple's:

- **Reduce Motion** — disables non-essential animation; sliding sheets become cross-fades.
- **Reduce Transparency** — replaces vibrancy materials with opaque solids derived from the same palette.
- **Dynamic Type** — every UI surface respects a user-chosen text-size scale (default 100%, range 80%–250%).
- **VoiceOver** — every interactive element has an accessibility label, role, and hint at the toolkit level. The screen reader is a Patrol service, always available, no add-on.

Spotlight is canonized as a universal entry point. In Field OS it is called **Radar**. The menu bar is always present, translucent, and reachable by keyboard alone. Command-Space (or its equivalent on non-Apple keyboards) opens Radar from anywhere.

### 2.4 The TempleOS technical core, preserved

The following technical primitives are taken from TempleOS as a preserved engineering inheritance. Religious framing, divination features, the Oracle, and biblical metaphor are not preserved and are not referenced.

1. **HolyC as the universal language.** Kernel, supervisor, compositor, shell, REPL, apps, scripts, and document macros are all HolyC. Functions can be defined at the shell prompt and called immediately. There is no separate scripting language.
2. **A unified, executable document format.** The DolDoc-equivalent in Field OS is the **Brief** format, file extension `.brf`. A Brief is a single hypertext document that can contain text, links, embedded sprites, executable HolyC macros, and structural widgets (lists, trees, forms, tables). Source code files are Briefs. Help documents are Briefs. The shell history is a Brief. The personal launcher menu is a Brief. The interface for any app, when not custom, is a Brief.
3. **Shell-is-the-compiler REPL.** The shell prompt is the compiler input. Top-level expressions and statements run on enter. `5+7` returns `12`. `#include "myfile.HC"` JIT-compiles a file into the current namespace. There is no separate calculator, no separate scripting host.
4. **Live coding via F5 hot-patch.** Pressing F5 in the editor over an open source file recompiles its function definitions and atomically swaps them in for any subsequent calls in running tasks. Long-lived state survives. This is the closest model to Smalltalk-style image programming on a statically typed system, and it is the daily-driver workflow for system development.
5. **Source code is the documentation.** Functions are tagged with `#help_index "Category;Subcategory"` directives. The help system rebuilds its tree from these tags whenever a Brief is saved. There is no separate documentation pipeline. F1 anywhere opens the indexed help for the symbol under the cursor; symbols in any printed output are hyperlinks to their definitions.
6. **A 100,000-line core budget.** This is a hard, enforced ceiling for the kernel + supervisor + compositor + language runtime + Brief format + shell + base file manager. Apps and drivers live outside the budget. Build CI fails if the budget is exceeded.
7. **Single-language, single-system simplicity.** No layered runtimes, no ABI versioning treadmill, no foreign syntax in the base. The user can read the entire system, end to end, in one sitting if they so choose.
8. **The 16-color legacy palette and 8×8 bitmap font as a respected legacy theme.** Available as **Field Standard** in the Camo Index. Field OS will boot, run a shell, and run the base apps in the legacy theme without recompilation; this is a quality-of-engineering test, not a marketing feature.

The non-preserved items, for the record: the Oracle / random-word / divination feature is removed entirely; the religious framing, naming, and dedications are removed entirely; the ring-0-only / single-address-map decision is *not* preserved (Field OS has paged virtual memory, user/kernel separation, and per-process address spaces, because those are non-negotiable for a daily-driver in 2026).

### 2.5 The MGS3 visual register

MGS3: Snake Eater (2004, Kojima Productions) is visually distinct from the rest of the Metal Gear Solid series in a way that is directly relevant to Field OS. Where MGS1 and MGS2 (Shadow Moses, the Big Shell) are cyan-industrial — cold, fluorescent, steel — and where MGS4 is bleached desaturated dystopia and MGS V is military-cold, MGS3 is **warm, analog, and jungle-coded**. Its palette is olive drab, jungle green, weathered khaki, brass and bronze, signal red, canvas tan, leather brown, and the deep brown-black of a moonlit forest. Its surfaces are paper maps, hand-tuned radio dials, leather-bound field manuals, and brass instruments rather than CRT monitors and chrome.

Field OS borrows from MGS3 the following discrete cues:

- **Color palette: Cold War analog warmth.** Tokens in §5.2.
- **Typography rhythm: mission-briefing.** Briefing screens in MGS3 use compressed, spaced uppercase headers (Helvetica Ultra Compressed family in the original) over body text in a clean sans. Field OS adopts this rhythm for any "briefing-class" surface — release notes, onboarding, error postmortems — using IBM Plex Sans Condensed Semibold (uppercase, 8% tracking) for headers over IBM Plex Serif body. Day-to-day UI uses IBM Plex Sans (§5.5).
- **Survival/craft sensibility: the system as a kit you maintain.** The user is positioned as a competent operator with a kit, not a consumer with a service. Settings is named **Frequencies** (you tune the system); the resource monitor is **Stamina**; the system-repair utility is **Cure**; the recovery toolkit is the **Survival Kit**; the package store is **Stockpile**.
- **Tactical restraint.** No flourish for flourish's sake. No scanlines on every window. No CRT bloom. No HUD-style angled brackets on chrome. The MGS3 aesthetic enters Field OS via *materials, palette, typographic rhythm, and naming* — never via skinning that imitates a HUD.
- **The Codec as a single, scoped interaction pattern, not a system-wide skin.** The Codec interface in MGS3 is a focused two-pane communication surface with a tunable frequency, scrollable contacts, transmission status, and save dialogue. Field OS takes from this the **Codec** notification metaphor: a small, modal-but-dismissible communication surface that appears at the bottom-left when the system or an app needs to address the user out-of-band (e.g., long-running build finished, system update available, low-battery). It is *not* the chrome of every window. The Codec surface uses warm desaturated vibrancy, a simple two-pane layout (sender identity left, message right), and a frequency-style affordance for switching between active conversations (e.g., between the build system, the package manager, and a notification provider). Crucially, scanlines and CRT effects are reserved for the optional **Field Standard** legacy theme; in the default themes the Codec is precise and modern, not retro.

The discipline is: **the OS should feel like a piece of well-engineered analog field equipment with modern precision rendering, not like a video-game skin**.

---

## 3. Name catalog (recommendation)

For the components renamed away from the previous brief's religious register, the canonical mapping is:

- **OS:** Field OS *(replaces Cathedral OS)*
- **Welcome / onboarding doc:** Briefing
- **Help system:** Field Manual *(replaces Codex/Lectern)*
- **Notification surface:** Codec
- **Search / Spotlight:** Radar
- **Command palette:** CQC *(close-quarters command)*
- **System settings:** Frequencies
- **Resource monitor:** Stamina
- **System repair / first-aid:** Cure
- **Recovery / safe-mode toolkit:** Survival Kit
- **Service supervisor (launchd-equivalent):** Patrol
- **Background services:** Patrols *(units)*
- **Network endpoints / servers:** Outpost
- **Telemetry / debugger / profiler hub:** Listening Post
- **Hardware/diagnostics tool:** Recon
- **File manager:** Cache *(replaces Vault/Reliquary)*
- **File metadata viewer:** Dossier
- **Package manager / store:** Stockpile *(replaces Tabernacle)*
- **System update channel:** Patch
- **IPC primitive:** Channel
- **Network stack:** Comm Tower
- **Sandbox / isolation:** Cardboard Box
- **Identity / account:** Calling Card
- **Workflow / task / job:** Operation *(also: Op)*
- **Dispatch primitive (GCD-equivalent):** Patrol Queue
- **Compositor:** Stage
- **Graphics API:** Foundry
- **Compute API:** Engine
- **Audio stack:** Wavelength
- **Universal document format (DolDoc-equivalent):** Brief *(.brf)*
- **Language:** HolyC *(retained name, secular framing)*
- **Legacy 16-color theme:** Field Standard

---

## 4. Architecture overview

### 4.1 Kernel

Field OS uses a **monolithic, preemptively scheduled, virtual-memory x86_64 kernel** written in HolyC. It is *not* ring-0-only and it does *not* identity-map memory; those TempleOS choices are incompatible with daily-driver use, multi-user accountability, and modern hardware capability gating. What it preserves from TempleOS is **directness**: every kernel data structure is a HolyC `class` whose definition is the documentation, and the kernel image is built by the same compiler used for shell input.

Numerical targets:

- Kernel + supervisor source: **≤ 60,000 lines** of the 100,000-line budget.
- Cold boot to login surface on a 2020-class laptop (NVMe SSD, 8 GB RAM, 4 cores): **≤ 2.5 s**.
- Hot context switch: **≤ 1.5 µs** at 4 GHz with caches warm.
- Average syscall round-trip (no I/O): **≤ 200 ns**.

### 4.2 Patrol (service supervisor)

Every long-running thing — Stage, Wavelength, Comm Tower, Radar's indexer, Stockpile's cache refresher, every user session, every background app — is a **Patrol unit**. A Patrol unit is described in a HolyC declarative manifest:

```holyc
PatrolUnit "comm.tower" {
  Description = "Network stack and DNS resolver";
  ExecStart   = "/sys/bin/comm_tower";
  Restart     = ON_FAILURE;
  Capabilities = { NET_RAW, NET_BIND };
  WatchdogSec  = 30;
  After        = { "patrol.target.basic" };
};
```

Patrol owns starts, restarts, dependencies, capability handoff, log capture (into Listening Post), and watchdogging. There is exactly one supervisor in the system. Per-user services are Patrol units that run under a Calling Card scope.

### 4.3 Stage (compositor)

Stage is a Wayland-style compositor with hard real-time scanout guarantees, written in HolyC against Foundry. It owns window placement, vibrancy material rendering, animation timing, and input routing. It is single-process, non-extensible by third parties, and fully deterministic in its frame-pacing (target ≥ 99.9% of frames within ±0.5 ms of the display vsync at 120 Hz).

Stage exposes four named material classes for vibrancy, derived from Big Sur's library, retuned warm:

| Material | Use | Light tint | Dark tint |
|---|---|---|---|
| `Material.Sidebar` | Sidebars, the dock | Warm off-white (5% olive) | Warm near-black (10% olive) |
| `Material.Menu` | Menu bar, popover menus | Higher saturation, lower opacity | Higher saturation, lower opacity |
| `Material.Sheet` | Modal sheets | Lower saturation, higher opacity | Lower saturation, higher opacity |
| `Material.HUD` | Volume, brightness HUDs, the Codec surface | High blur radius, warm tint | High blur radius, warm tint |

Under **Reduce Transparency**, all four materials become opaque solids whose color is the material's tint applied to the desktop average luminance.

### 4.4 Channel (IPC)

Channel is the universal IPC primitive — a typed, capability-bearing, bidirectional message pipe between two Patrol units or between a Patrol unit and a sandboxed Cardboard Box. Channels carry handles (capabilities) the same way Zircon channels do in Fuchsia, and they are the only sanctioned mechanism for cross-process state sharing. Shared memory is permitted but exposed as a Channel-vended capability. There is no global namespace; namespaces are per-Cardboard-Box.

### 4.5 Cardboard Box (sandbox)

Every third-party binary runs inside a Cardboard Box by default. The Box is a capability container (filesystem subtrees, network endpoints, hardware classes, subset of Channel namespaces). Manifests are a small HolyC file shipped alongside the binary; a missing or malformed manifest gets a Box with no capabilities — the binary runs, but it cannot read, write, or talk to anything. The naming acknowledges MGS3 humor without being cute about it; the technical content is serious.

### 4.6 The 100,000-line discipline

CI fails the build if the wc-line-count of in-tree HolyC + assembly inside the **base set** (kernel, Patrol, Stage, Channel, Cardboard Box, language runtime, Brief format, shell, Cache file manager, Field Manual help engine, Frequencies settings shell) exceeds 100,000. Apps live outside this set and have no individual cap. This is a forcing function: it discourages premature abstraction and forces the team to remove code as often as adding it.

---

## 5. Visual design system

### 5.1 Spacing & geometry

- **4 px base grid.** All paddings, gutters, control heights, and offsets are multiples of 4. Common rhythm: 4 / 8 / 12 / 16 / 20 / 24 / 32 / 48.
- **Corner radii.** 4 px (micro: tags, pills), 8 px (controls, list rows, popover tips), 12 px (sheets, popovers, secondary windows), 20 px (primary windows, the dock, the Codec surface).
- **Stroke widths.** Hairlines at 1 device-pixel (not 1 point). Keylines at 1.5 px. Rules between sections at 1 px in `Color.Border.Subtle`.
- **Window default minimum size.** 720 × 480 logical points.
- **Sidebar width.** Default 240 pt; min 200, max 360.
- **Toolbar height.** 52 pt (inline title + 28 pt control row).
- **Touch targets.** 44 × 44 pt minimum for any interactive glyph; HIG-aligned.

### 5.2 Color tokens

The accent palette below is the curated system palette. These are the only accents the user can pick from in the default Frequencies pane. (Apps may declare their own accent, exactly as in Big Sur, which the user can choose to honor or override.) All colors are tested for ≥ 4.5:1 contrast against the corresponding text token on both light and dark surfaces.

| Token | Light hex | Dark hex | Notes |
|---|---|---|---|
| `Accent.OliveDrab` *(default)* | `#4A5D23` | `#7E9248` | The default. Cold War olive. |
| `Accent.JungleGreen` | `#2E3D27` | `#4F6B43` | Deeper, more saturated. |
| `Accent.Brass` | `#8C7A2E` | `#C0A645` | Weathered brass; great on light surfaces. |
| `Accent.Khaki` | `#A89968` | `#C9B58F` | Canvas / field uniform. |
| `Accent.SignalRed` | `#A4312E` | `#D2554F` | Reserved for destructive and high-priority states. |
| `Accent.CanvasTan` | `#C9B58F` | `#A89968` | Mid-warm neutral; the "paper" accent. |
| `Accent.Midnight` | `#1A1F1A` | `#0E120E` | Very dark olive-black for the Tactical Black theme. |
| `Accent.RadioBlue` | `#3F5E78` | `#6F8FAB` | The single cool accent. Matches a Cold War radio dial face. |

Neutral surface tokens (Jungle / Recon Night defaults):

| Token | Jungle (light) | Recon Night (dark) |
|---|---|---|
| `Surface.Window` | `#F4F1EA` | `#1B1D17` |
| `Surface.Sidebar` | `#EAE5D8` w/ vibrancy | `#23271E` w/ vibrancy |
| `Surface.Sheet` | `#FBF8F1` | `#272B22` |
| `Surface.Inset` | `#E2DCCB` | `#1F221A` |
| `Color.Text.Primary` | `#1F2218` | `#EFE9DA` |
| `Color.Text.Secondary` | `#5A5747` | `#B9B19D` |
| `Color.Border.Subtle` | `#D7D0BD` | `#34382D` |
| `Color.Border.Strong` | `#9B9479` | `#65694F` |

These tokens are the single source of truth. There is no parallel "branding" palette.

### 5.3 Motion

- Default ease curve: `cubic-bezier(0.32, 0.72, 0, 1)` (the Big Sur "ease-out-quart"-feeling default).
- Default durations: micro (controls, hover) **120 ms**; standard (popovers, sheets opening) **240 ms**; large (window minimize, space switch) **400 ms**.
- Reduce Motion: micro becomes 0 ms, standard becomes 80 ms cross-fade, large becomes 120 ms cross-fade. No translation or scale animation under Reduce Motion.
- Frame-pacing: Stage targets ≥ 99.9% of frames within ±0.5 ms of vsync at the panel's native rate (60/90/120/144 Hz).

### 5.4 Vibrancy / materials

Materials are fully described in §4.3. They are the primary depth cue. Shadow is secondary and used only on the Codec surface, sheets, and the system menu.

### 5.5 Typography

All three faces are SIL OFL 1.1 licensed (verified at the IBM Plex repository); they are open source and free to redistribute as part of the OS image.

| Role | Face | Size & treatment |
|---|---|---|
| **UI body** | IBM Plex Sans (Regular 400, Medium 500) | 13 pt body / 11 pt secondary / 17 pt large title |
| **UI display** | IBM Plex Sans Condensed Semibold (600), uppercase, +80 (8%) letter-spacing | 22 pt window title, briefing headers |
| **Code & monospace** | IBM Plex Mono (Regular 400, Medium 500) | 13 pt editor default |
| **Documentation / Brief body** | IBM Plex Serif (Regular 400, Italic) | 15 pt long-form, 1.55 line-height |
| **Legacy** | 8×8 bitmap font (TempleOS-faithful) | Field Standard theme only |

Optical sizing is variable across the Plex family; Field OS uses the natural axis where supported. Berkeley Mono is a paid alternative the user can pin in Frequencies → Typography → Mono Override; it is *not* shipped due to license incompatibility.

Type ramp (Dynamic Type @ 100% scale):

- 11 pt — caption, footnote
- 13 pt — body
- 15 pt — emphasized body, doc body
- 17 pt — section title
- 22 pt — window title
- 28 pt — large display
- 34 pt — briefing display

Every value scales linearly with the user's Dynamic Type scalar (80%–250%).

### 5.6 Iconography (Field Symbols)

SF Symbols is Apple proprietary and cannot be used. Field OS ships **Field Symbols**, a from-scratch icon set that follows the SF Symbols *idea* (typographically aware, weight-matched, multi-style) and is licensed SIL OFL 1.1.

- **Base set** is a fork of **Lucide** (ISC license, ~1,600 24×24 stroke glyphs) — chosen because Lucide is single-license, MIT/ISC-style permissive, and visually compatible with a stroke-aware UI register.
- **Field-specific set** (~250 additional glyphs) is drawn from scratch in HolyC's vector primitives at the same 24 px grid: Codec (the call icon), Camo Index, Patrol, Cache (the kit), Briefing, Field Manual, Cure, Stamina, Survival Kit, Cardboard Box, CQC, Outpost, Listening Post, Frequency dial, etc.
- All symbols ship at three weights (Regular 1.5 px / Medium 2 px / Bold 2.5 px stroke) and four optical sizes (16 / 20 / 24 / 32). They co-baseline with IBM Plex Sans at the corresponding text size.
- Phosphor (MIT, multi-weight, more decorative) is provided as an optional Camo Index swap-in for users who prefer a softer register; Tabler (MIT) is provided as a denser alternative for power users.

### 5.7 Component library

The toolkit ships exactly the following first-class components. No app should ever roll its own equivalent without justification.

**Layout & chrome**

- Window (with traffic-light triple, inline toolbar, optional sidebar)
- Full-height Sidebar (with sections, inset selection, color-tinted icons)
- Inline Toolbar (with title, primary controls, overflow chevron)
- Status footer (optional, narrow)
- Tab bar (for document-class apps only)

**Containers**

- Sheet (modal, attached to window)
- Popover (anchored, with arrow tip)
- Alert (centered, structured)
- HUD (volume/brightness style)
- **Codec surface** (the system notification + IPC-call surface)
- **Briefing** (a full-window onboarding/welcome layout type)

**Controls**

- Button (filled, tinted, plain; small/medium/large)
- Segmented Control
- Toggle
- Checkbox / Radio
- Slider
- Stepper
- Combobox / Picker
- Tag / Pill
- Search field (with Radar promotion key)

**Data**

- List (with inset selection)
- Table (with sortable headers, sticky first column)
- Outline/Tree (collapsible — Brief native)
- Form (label + control rows, with section headers)
- Source list (the file-tree variant)

**Doc-native**

- Brief renderer (the inline Brief view used by Field Manual, Briefing, and any app that wants live executable documentation)
- Code editor (HolyC syntax-aware, F5-aware, hyperlinked)
- Sprite editor (the inline image/diagram editor, Brief-embedded)

**System surfaces**

- Menu bar (always present, translucent)
- Dock (translucent, optional auto-hide)
- Control surface (per-user pinned controls — Big Sur Control Center analogue)
- Codec inbox (history of recent Codec messages)
- Radar (universal search overlay)
- CQC (command palette overlay; key: ⌃Space)

---

## 6. Component & service catalog

### 6.1 Core services (Patrol units)

| Unit | Role |
|---|---|
| `patrol.target.basic` | Synchronization target for "system is ready for user-space" |
| `stage` | Compositor |
| `wavelength` | Audio stack |
| `comm.tower` | Network stack + DNS |
| `radar.indexer` | Filesystem & content indexer |
| `stockpile.refresh` | Package metadata refresher |
| `cure.daemon` | System-health watcher (disk, memory, thermal) |
| `listening.post` | Logging, telemetry collection (local-first; opt-in upstream) |
| `cardboard.box.broker` | Sandbox capability broker |
| `codec.broker` | Notification + Codec call routing |
| `frequencies.daemon` | Settings persistence + propagation |

### 6.2 Frameworks

- **HolyC runtime** — JIT compiler, link/load, GC for Brief temporaries
- **Brief format** — read/write/render, macro evaluation, sprite store
- **Foundry** — graphics API (Vulkan-class, HolyC-native, single binding for Stage and apps)
- **Engine** — compute API (OpenCL-class) over Foundry buffers
- **Patrol Queue** — system dispatch primitive (block-based concurrency, exposed in HolyC as `dispatch{ ... }` blocks)
- **Channel** — typed IPC

---

## 7. Performance engineering for commodity x86_64

### 7.1 Targets (2020-class reference laptop: 4 P-cores, 16 GB, NVMe)

- Cold boot to login: **≤ 2.5 s**
- Login to populated desktop: **≤ 0.8 s**
- App cold launch (Cache): **≤ 250 ms**
- App cold launch (Recon browser, single tab to first paint): **≤ 1.2 s**
- Window resize: ≥ 99% of frames within vsync at 120 Hz
- Idle RAM with Patrol + Stage + Wavelength + Comm Tower up: **≤ 220 MB**
- Idle CPU with the above: **≤ 0.5%**
- Full system disk footprint at install: **≤ 6.0 GB** (kernel, base, apps, default Brief library, fonts)

### 7.2 Engineering levers

- **Patrol Queue everywhere.** Every blocking operation in the base system is a Patrol Queue submission. The kernel exposes work-stealing queues per core; the runtime exposes serial and concurrent queues; all toolkit-level event handlers run on a main-actor-style serial queue.
- **A single compiler, hot.** The HolyC JIT is resident; first-run compile of a 5,000-line Brief is targeted at **≤ 80 ms** on the reference laptop. Subsequent runs hit the persistent cache.
- **Whole-system LTO at the source level.** Because there is one compiler and one source language, the base system is compiled with full cross-module inlining by default.
- **Stage scanout discipline.** The compositor uses direct scanout for full-screen surfaces and forwards damage rectangles only. There is no per-app GPU command-buffer indirection layer.
- **Memory: identity-mapped fast paths within tasks, paged across tasks.** TempleOS's identity-mapping cleverness is preserved as an *intra-task* optimization; per-task page tables are still real and enforced.
- **No telemetry tax.** Listening Post is local-first; no upstream is enabled by default.

### 7.3 The "smaller-on-disk-than-the-toy" rule

In the spirit of Snow Leopard's "frees up roughly 7 GB" framing, every minor release of Field OS must ship at the same or smaller disk footprint than the previous minor release at equivalent feature parity. This is enforced in CI.

---

## 8. Daily-driver apps

All apps are HolyC, sandboxed in Cardboard Boxes, and use the same component library. The naming below is final.

| App | Role | Notes |
|---|---|---|
| **Cache** | File manager (Finder-equivalent) | Full-height sidebar, columns view, gallery view, Quick-Look-equivalent press-Space preview; Brief-aware preview shows a rendered Brief. |
| **Manual** | Document/text/PDF viewer + editor | Reads/writes Brief, plain text, Markdown, PDF. The universal viewer. |
| **Armory** | IDE | HolyC-aware, F5 hot-patch, hyperlinked symbol jumps, integrated Listening Post for debug. |
| **Operator** | Terminal / Field Console | A Brief-rendering shell. Output is a live Brief: clickable filenames, embedded plots, executable history. |
| **Recon** | Web browser | Embeds a Servo-class engine (Rust, in a Cardboard Box). Big-Sur-style sidebar with bookmarks. |
| **Dispatch** | Mail | IMAP/JMAP. Sidebar of mailboxes; inbox is a list with inset selection. |
| **Roster** | Contacts | Each contact is a Brief; metadata is a Dossier. |
| **Schedule** | Calendar | Day/week/month/year. ICalendar import/export. |
| **Frequency** | Music player | Local library + open-protocol streaming (Subsonic, MPD). Sidebar by artist/album/playlist. |
| **Projector** | Video player | mpv-class engine in a Cardboard Box. |
| **Negatives** | Photos / images | Library, simple non-destructive edits, RAW pipeline. |
| **Field Notes** | Notes | Brief-native; macros and sprites are first-class. |
| **Frequencies** | System Settings | Sidebar of categories (Appearance / Camo Index, Typography, Privacy, Patrols, Cardboard Boxes, Network, Calling Card, Accessibility). |
| **Cure** | System repair | First-aid menu: disk repair, permissions reset, Patrol reset, Brief library re-index. The naming (Cure) is the one place the MGS3 sense is most direct, and it earns it because the UX literally mirrors the Cure menu's "select injury → apply treatment" model. |
| **Survival Kit** | Recovery / safe mode | Boots a minimal session for diagnostics. |
| **Stamina** | Resource monitor | Activity Monitor analogue. Per-Patrol CPU/RAM/disk/network. The "stamina" is the system's; processes deplete it. |
| **Stockpile** | Package manager | Curated app store; sideloading allowed via signed Cardboard Box manifests. |
| **Listening Post** | Logs / debugger / profiler | Reads the local Patrol log stream; supports in-place HolyC tracing on any running unit. |
| **Snapshot** | Screenshot tool | Region/window/full; results are Briefs by default (so you can annotate and add macros). |
| **Cipher** | Password manager | Encrypted local store; optional sync via WebDAV/Outpost endpoints. |
| **CQC** | Command palette | Always-on system surface. ⌃Space. |
| **Radar** | Universal search | ⌘Space (or super-Space). Indexed by `radar.indexer`. |
| **Briefing** | Welcome / onboarding | The first-run app and the destination for "Show Welcome Again." A set of Briefs that walk a new user through Field OS without religious or marketing register — direct, declarative, like a real briefing. |
| **Field Manual** | Help system | A live Brief tree built from `#help_index` directives across the system source plus authored long-form docs. F1 anywhere opens the relevant page. |

---

## 9. Creative workstation strategy

A serious daily-driver OS in 2026 must do creative work credibly. Field OS does not attempt to replace Adobe; it does ship a credible set of first-party creative tools that share the toolkit and the Brief format.

- **Manual** doubles as a writing app. Long-form mode uses Plex Serif at 17 pt with 1.6 line-height; "briefing mode" uses the condensed display headers. Export to PDF, HTML, ePub, Markdown.
- **Sprite editor** in Briefs is competent for diagrams and pixel art; not a Photoshop competitor.
- **Negatives** owns the photographic pipeline: a non-destructive, RAW-aware editor with reversible adjustments stored as a Brief sidecar.
- **Frequency** and **Projector** play media; they do not edit. Audio editing is a separate ported tool (Ardour-class, in a Cardboard Box).
- **Foundry + Engine** make Field OS a credible target for ports of Blender, Krita, Inkscape, DaVinci-class editors. The OS commits to providing a stable HolyC + C ABI for these.
- **The Camo Index applies.** Briefing Room (formal off-white serif) is the recommended theme for writing; Recon Night for code; Tactical Black for video grading; Olive for distraction-free.

---

## 10. Synthesis: tensions and resolutions

This brief draws from five sources whose values do not always agree. The resolutions are made explicit:

**Tension: TempleOS is ring-0-only and identity-mapped; modern desktops require user/kernel separation and per-process address spaces.**
*Resolution:* Keep the *aesthetic* of directness — one language, one compiler, source-as-doc, F5 reload — but adopt paged virtual memory and a real user/kernel boundary. The 100,000-line budget keeps the directness honest even with the additional structure.

**Tension: TempleOS's Brief-everywhere model collides with Big Sur's sidebar-and-toolbar app shell.**
*Resolution:* The Brief is the **content model**; the sidebar/toolbar/inset-selection chrome is the **app shell**. Apps render Briefs in their content area. The shell does not get embedded in Briefs; Briefs do not get embedded in the shell. Field Manual, Briefing, Operator, and Manual are the four canonical Brief-rendering apps.

**Tension: Big Sur's accent system assumes the iOS palette (blue, purple, pink, orange, yellow, green, graphite, multicolor); MGS3's register rejects most of those.**
*Resolution:* Replace the curated palette wholesale with the accents in §5.2. Apps may still declare any color as their accent; the *system* curated set is olive/jungle/brass/khaki/red/canvas/midnight/radio-blue.

**Tension: Snow Leopard's "no new features" disposition vs. the need for a v1.0 with a real feature set.**
*Resolution:* Field OS adopts the *disposition* (refinement over additions, smaller-on-disk over larger, fewer abstractions over more) on every release *after* v1.0. v1.0 itself is allowed to ship the foundational set; v1.1 onward must respect the disposition or fail review.

**Tension: HolyC is a niche language with no library ecosystem; modern apps depend on huge C/C++/Rust ecosystems.**
*Resolution:* The base system is HolyC. Apps that need a third-party engine (browser, video, ML, audio editing) ship that engine in their Cardboard Box, exposed to the rest of the system through a HolyC binding layer. The 100,000-line budget protects the base; the Cardboard Box protects the rest of the system from the engine.

**Tension: The MGS3 register is evocative but easily becomes cosplay (scanlines, CRT bloom, HUD brackets).**
*Resolution:* Reserve all visually loud MGS3 references for the optional **Field Standard** legacy theme. The default themes are precise modern Big-Sur-class rendering with warm palette and tactical naming. The Codec surface is the *only* place a user encounters anything that visually nods to MGS3 in the default theme — and it nods through layout, not through chrome decoration.

**Tension: Single-language simplicity vs. driver realism.**
*Resolution:* The driver layer is the only place C/C++/Rust is allowed inside the kernel. Drivers do not count against the 100,000-line budget. They are walled off behind a stable in-kernel C ABI and reviewed with a higher bar for safety.

**Tension: Live coding (F5 hot-patch) vs. system stability.**
*Resolution:* F5 hot-patch is a developer feature. End-user apps run from precompiled, signed Brief bundles. F5 is permitted on user-owned source files only; it cannot patch the running kernel, Patrol, or Stage from a non-developer Calling Card.

---

## 11. Operating Principles (the public design document)

The document formerly called the Charter is now the **Operating Principles**. It is short, secular, and engineering-first. It reads, in full:

> **Operating Principles of Field OS**
>
> 1. *Refinement over addition.* Every release must end smaller, faster, or more legible than the previous one in at least one measurable dimension, and worse in none.
> 2. *One language, one compiler, one document.* The base system is HolyC. The base document format is the Brief. The base shell is the compiler. Where this constraint hurts, we prefer to feel the hurt and write better HolyC.
> 3. *Deference.* The interface must disappear behind user content. If the system feels like the subject, the system is wrong.
> 4. *Accessibility is a load-bearing wall.* Reduce Motion, Reduce Transparency, Dynamic Type, and the screen reader are shipped in v1.0 or the release is held.
> 5. *Source is the documentation.* Every public function carries a `#help_index` directive. The help system is a derived view.
> 6. *The line-count budget is law.* Base system ≤ 100,000 lines of HolyC + assembly. Drivers and apps live outside.
> 7. *Sandboxed by default.* Every third-party binary runs inside a Cardboard Box. A missing manifest gets a Box with no capabilities.
> 8. *Local-first.* No telemetry, sync, or remote service is enabled without an explicit, recent user gesture.
> 9. *Tactical restraint.* No flourish, no skin, no theme-park overlay. The system is professional infrastructure.
> 10. *The user is an operator.* Tools are tunable, settings are honest, errors explain themselves. The system trusts the user and earns the trust back.

---

## 12. Phasing

A realistic plan for a small team (~6–10 contributors, mixed full- and part-time), modeled on the cadence small-team OS projects like SerenityOS, Haiku, Asahi Linux, and Redox have demonstrated.

**Phase 0 — Foundations (months 0–9)**
HolyC compiler hardening on x86_64; bring-up on QEMU and one reference laptop. Kernel boot to a serial console; paged VM; SMP; basic process model. Patrol v0 (single-user, no dependencies). A bring-up shell that compiles and runs Briefs.

**Phase 1 — Display & toolkit (months 9–18)**
Foundry on top of Vulkan; Stage compositor; the 8/12/20-radius window with full-height sidebar and inline toolbar; the four material classes; the three-faced Plex typography stack; Field Symbols v1 (Lucide fork + 100 first-pass custom glyphs); the Brief renderer. Demo-bar: Cache, Manual, Operator, Frequencies, Listening Post, Stamina at usable quality.

**Phase 2 — Daily-driver candidate (months 18–30)**
Comm Tower (TCP/IP, DNS, TLS); Wavelength (PipeWire-class via a single Patrol unit); Cardboard Box with capability broker; Stockpile v1; Patch (system update); Recon (browser via Servo-class engine in a Box); Dispatch, Roster, Schedule; Negatives, Frequency, Projector; Cure, Survival Kit. Accessibility: VoiceOver via a Patrol unit; Dynamic Type; Reduce Motion; Reduce Transparency. The 6 GB disk-footprint target is enforced from this phase forward.

**Phase 3 — Polish & ship v1.0 (months 30–36)**
Performance tuning to hit the §7.1 numbers. Camo Index theming. Codec interaction pattern shipping. The Operating Principles document published. Briefing first-run experience. Field Standard legacy theme bootable end-to-end (engineering test). v1.0 release notes — short, in the Snow Leopard register.

**Phase 4 — Refinement releases (1.x)**
Each minor must be smaller, faster, or clearer in at least one dimension and no worse in any. Driver portfolio expands with the Patrol-managed driver model. ARM64 bring-up begins (track Asahi's reverse-engineered M1/M2 work for Apple Silicon if pursued). Creative-tool ports follow.

**Phase 5 — Surface area (2.x and beyond)**
Tablet and detachable form-factor support. RISC-V bring-up. Optional second hardware reference platform. The 100,000-line budget is reviewed but, per principle 6, only relaxed if the relaxation produces a measurable correctness or performance win.

---

## 13. Cross-references to other small-team OS projects

Field OS is positioned in a real landscape; it is useful to be explicit about what it learns from and where it diverges.

- **Asahi Linux.** The reference for what a small team can do against undocumented hardware — Hector Martin, Alyssa Rosenzweig, Asahi Lina, Sven Peter, Janne Grunau, et al., reverse-engineered the M1/M2/M3 platform and upstreamed drivers including the world's first Rust GPU kernel driver. Field OS's lesson is the *upstreaming discipline* and the m1n1-style hypervisor-based reverse engineering technique, applicable when Field OS pursues ARM64 hardware in Phase 4.
- **SerenityOS.** Andreas Kling's project demonstrated that a single-language (C++) from-scratch desktop OS can reach the point of a self-hosted browser passing Acid3 with a small team. Field OS adopts the *taste* (single language, in-tree everything, no porting culture) and the *cadence* (regular monthly progress, public).
- **Haiku.** The longest-running of these efforts, the reimplementation of BeOS, is the canonical case study for "ship a coherent desktop slowly." Field OS learns from Haiku's commitment to API stability and from BeOS's pervasive multithreading — a thing Siracusa explicitly held up as the right mental model for GCD. The Haiku file manager (formerly "Tracker" in BeOS) is the model for **Cache**, but the name is changed.
- **Pop!_OS COSMIC and elementary OS.** Two Rust/Vala desktop environments built by small teams with strong design opinions on top of Linux. Field OS borrows from elementary the discipline of an HIG-as-living-document and a small curated default app set, and from COSMIC the willingness to write the toolkit and compositor from scratch.
- **Fuchsia / Zircon.** Google's microkernel-based OS gives Field OS the model for **Channel** as the universal IPC primitive (capability-bearing, typed handles, no global namespace). Field OS does not adopt Fuchsia's microkernel architecture wholesale — it stays monolithic for performance and 100K-line-budget reasons — but it adopts the IPC semantics.
- **Redox OS.** Rust microkernel project; the reference for a memory-safe systems language at the OS level. Field OS does not use Rust in the base (HolyC is the rule), but it borrows Redox's discipline of capability-passing IPC and the preference for in-tree everything.
- **Plan 9.** The reference for "everything is a file (or a name)" and for small, composable systems. Field OS is *not* file-namespace-everything (the Brief is the universal model, not the 9P filename), but it adopts Plan 9's preference for textual, inspectable, composable interfaces over opaque binary protocols.
- **Smalltalk / Pharo.** The reference for image-based live programming. Field OS's F5 hot-patch and the shell-as-compiler model are the closest a statically-typed system gets to Smalltalk's "the whole environment is alive." Pharo's Browser-and-Inspector workflow is the model for Armory's symbol-jump experience.
- **Genode.** A capability-based component framework. The reference for Cardboard Box's broker design.

The lesson aggregated from these projects is that a small team *can* ship a coherent OS if it commits hard to a small surface area, ships often, refuses to port for the sake of porting, and treats the documentation as part of the build.

---

## 14. The Codec interaction pattern, in detail

Because the Codec is the single most explicitly MGS3-referential element of Field OS, it warrants a closer specification.

**Purpose.** The Codec is the system's surface for out-of-band communication with the user that does not justify a sheet, an alert, or a window — but does justify more than a banner. It is used for: long-running task completion notices, system update offers, failed Patrol restarts, low-resource warnings, and inbound messages from any app that opts in to the system notification API.

**Geometry.** Bottom-left of the screen, anchored 24 pt from the dock and 24 pt from the screen edge. A 360 × 132 pt rounded-rect at 20 px corner radius. The surface is `Material.HUD` — high-blur warm vibrancy.

**Layout.** Two panes, separated by a 1 px hairline in `Color.Border.Subtle`. Left pane (96 pt wide): the sender's identity — an app icon at 64 pt, the app/system name in IBM Plex Sans Medium 13 pt, and the Codec frequency string in IBM Plex Mono 11 pt (e.g., `140.85`, the system having a deterministic frequency-to-source mapping that is purely decorative — the user can enable or disable it in Frequencies → Codec). Right pane: title in Plex Sans Semibold 13 pt, body in Plex Sans Regular 12 pt with up to two lines, action area beneath with up to two textual buttons.

**Behavior.** Slides in over 240 ms (cross-fades over 80 ms under Reduce Motion). Auto-dismisses after 6 s for low-priority Codecs, persists for high-priority. Clicking the surface opens the Codec inbox. Pressing ⌘⇧C from any app opens the Codec inbox directly.

**What it is NOT.** It is not a HUD-style angled-bracket overlay. It is not a CRT. There are no scanlines on the default-theme Codec. There is no animated waveform, no static, no analog-radio noise. Those affordances exist *only* in the **Field Standard** legacy theme as a respectful nod, never in the default. The Codec earns the name through layout and naming, not through skin.

---

## 15. Closing

Field OS is, at its heart, a refusal of three modern operating-system tendencies:

- the tendency to grow rather than refine,
- the tendency to abstract before earning the abstraction, and
- the tendency to dress up infrastructure as entertainment.

It refuses the first by adopting Snow Leopard's disposition and a 100,000-line budget. It refuses the second by adopting TempleOS's single-language, single-compiler, single-document discipline and Big Sur's small, fixed component library. It refuses the third by adopting the Apple Human Interface principle of deference and the MGS3 sensibility of tactical restraint — a kit, well-maintained, that the operator trusts.

The result, if executed, is a small, fast, beautiful, coherent desktop operating system that a competent operator can read end to end in an afternoon, ship to a stranger without apology, and keep using for a decade.

— *End of brief.*