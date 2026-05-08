# CLAUDE.md

> This file is read by Claude Code at the start of every session.
> Keep it short, opinionated, and current. If a rule changes, change it here.

## What this project is

**Arsenal** — a from-scratch desktop operating system for commodity 2026 hardware (Framework 13 AMD/Intel, Snapdragon X laptops, Apple Silicon M1/M2 Macs, generic AMD/Intel desktops). Built by one person, evenings and weekends, on a multi-year arc — solo today, designed to support a small-team transition around year 3–4.

Arsenal commits explicitly that **performance, usability, and security are peer concerns**. None is subordinated to the others. When two pillars conflict, the resolution is an Architecture Decision Record, not a silent ranking.

The kernel is a Rust monolith with capability-secured userspace. Drivers are inherited from Linux 6.12 LTS via a LinuxKPI-style shim. The compositor is a custom wgpu/Skia "Stage" rendering an iDroid + Big Sur fusion identity. Applications ship in three tiers: native Rust, sandboxed Wasm components (WASI 0.2 → 0.3), and a curated POSIX subset (relibc-style) for ports of Firefox / mpv / git / foot.

The project pivoted from "Field OS" (a TempleOS-modernization in HolyC) on technical merit at `field-os-v0.1` (commit `dffe259`, 2026-05-08). See [`docs/adrs/0004-arsenal-pivot.md`](docs/adrs/0004-arsenal-pivot.md) for the rationale; the canonical plan is [`docs/plan/ARSENAL.md`](docs/plan/ARSENAL.md).

## Hard constraints — never violate without an explicit conversation first

1. **Peer concerns.** Performance, usability, and security are co-equals. When two conflict, write an ADR; do not silently rank one above the others. Concrete gates per milestone are in `docs/plan/ARSENAL.md` § "Three Concrete Starting Milestones."
2. **Memory-safe Rust as the primary language.** Rust end-to-end across the kernel and base. C is permitted **only** in inherited Linux drivers under the LinuxKPI shim (which retain GPLv2 in their original form). No `unsafe` Rust without a `// SAFETY:` invariant comment naming the precondition.
3. **License: BSD-2-Clause** for the Arsenal base (kernel, supervisor, compositor, system apps). MIT / BSD / Apache-2.0 / ISC / zlib / SIL-OFL acceptable for vendored Rust crates. **GPLv2 preserved on inherited Linux drivers** — non-negotiable; Arsenal ships as a *combined work* with explicit license boundaries (the FreeBSD / drm-kmod pattern).
4. **Visual identity.** iDroid + Big Sur fusion. Amber `#FFB200` (primary signal), cyan `#00C8E0` (secondary signal), navy `#0A1A2A` (chrome base). IBM Plex Mono 13 px chrome, Plex Sans 14 px body, Plex Serif long-form. 4 px spacing grid. 8 / 12 / 20 px corner radii. Big Sur translucent vibrancy via dual-pass blur. Holographic milspec scan-line shader on Stage chrome. No deviation without an ADR.
5. **MGS3-warm tactical naming throughout.** No religious framing — never use Cathedral, Solomon, Covenant, Tabernacle, Oracle, or biblical references. The naming catalog is in `docs/naming.md`; consult it before introducing any new system component name.
6. **`docs/plan/ARSENAL.md` is the canonical plan.** Always read the relevant section before suggesting a change of direction. The plan absorbs months of design discussion; do not reinvent decisions it has already made unless asked. ADRs document the deviations.

## Naming catalog (one-line summary)

The full canonical catalog lives in [`docs/naming.md`](docs/naming.md). Consult it before introducing any new system component name; the 3-to-5-candidate shortlist protocol is documented there. No religious framing, ever — never Cathedral, Solomon, Covenant, Tabernacle, Oracle, or biblical references.

The names you'll touch most in M0–M2: **Patrol** (init / service supervisor) · **Stage** (compositor) · **Cache** (file manager) · **Operator** (terminal) · **Cardboard Box** (per-app sandbox) · **Comm Tower** (network daemon) · **Inspector** (developer overlay).

## Where I am right now

See `STATUS.md` for the current milestone and active work.

The full plan lives in [`docs/plan/ARSENAL.md`](docs/plan/ARSENAL.md). Milestones are M0 (boot and breathe, 0–9 months), M1 (real iron, 9–24 months), M2 (it looks like Arsenal, 24–36 months), then v0.5 / v1.0 / v2.0. **Always read the relevant milestone section before suggesting a change of direction.**

## How to work with me on this repo

### Read before writing

1. Read `STATUS.md` to know the current milestone.
2. Read the relevant section of `docs/plan/ARSENAL.md`.
3. Read any `SKILL.md`-equivalent in `docs/skills/` that applies (e.g., `docs/skills/limine.md` before touching boot config; future skill files for LinuxKPI / Slint / wgpu / Wasmtime as those layers come online).
4. *Then* read the user's request and propose work.

### Edit, don't rewrite

This codebase is mine. I have opinions about every line. When I ask for a change:
- Make the smallest change that satisfies the request.
- Preserve existing style (naming, indentation, comment density).
- Never reformat unrelated code. Never drive-by-clean adjacent files.
- If you think a refactor is warranted, **ask first** with a specific proposal, not a sweeping rewrite.

### The build loop is sacred

The full build is `cargo build --release && cargo xtask iso && ci/qemu-smoke.sh`. Every change must keep this green or have a clear, stated reason for being intermediate work. If you make changes and the smoke test stops printing `ARSENAL_BOOT_OK` on serial, that is a regression and must be flagged.

When you finish a unit of work, **always run** the build + smoke and report the result. Don't claim a milestone exit criterion is met without seeing the smoke test pass.

(M0 step 1 is where the Cargo + xtask + smoke pipeline first lands; until then the build commands above are aspirational targets. The Field OS C build at `field-os-v0.1` is the historical reference for how the smoke test shape worked.)

### Commit hygiene

- One concern per commit. Conventional Commits format: `feat(stage): floating window snap-to-grid`, `fix(mm): off-by-one in slab cache resize`, `docs(plan): tighten M3 exit criterion`.
- If you propose a commit, write the full message, including a body that explains the *why*, not just the *what*.
- Squash-merge to `main`. Even as a solo dev. Bisect time matters more than a clean linear log costs.

### When you don't know

- A spec detail (Limine protocol, NVMe register, ACPI method, Wayland protocol) → cite the spec section you're working from.
- A Rust idiom in a context you haven't seen — `tokio` vs `smol`, `parking_lot` vs `spin`, `vulkano` vs `ash` — grep the existing codebase first; if absent, propose 2–3 with trade-offs and ask.
- A design call I haven't made yet → propose 2–3 options with trade-offs and ask. Do not silently pick one.
- The right MGS3-warm name for a new thing → propose 3–5 candidates from the conceptual space and let me choose. Do not invent without a shortlist.

## Style — code

- **Rust formatting:** rustfmt defaults. `cargo clippy --deny warnings` clean. No `unsafe` block without a `// SAFETY:` comment naming the invariant the caller must uphold and why this site upholds it.
- **C formatting (driver / shim code only, under the LinuxKPI boundary):** Linux kernel style (8-wide tabs, opening brace on same line for functions but on the next line for control flow — match `linux/Documentation/process/coding-style.rst`). 80-column soft limit, 100-column hard limit.
- **No clever code in kernel paths.** Clarity over brevity. A junior reader six months from now should understand it without context.
- **Assertions liberally.** `assert!`, `debug_assert!`, `panic!` with informative messages in Rust. `BUG_ON(cond)` in C driver shims.
- **Comments explain *why*, not *what*.** The code already says what.

## Style — prose (devlogs, Field Manual chapters, README)

- Match the tone of `docs/plan/ARSENAL.md` and the existing devlogs at `docs/devlogs/`: serious, calibrated, honest about timelines, never marketing, never hyperbolic.
- No exclamation points. No emoji except in user-facing UI strings explicitly marked for translation.
- "Ship" not "deliver." "Build" not "implement" when describing creative work; "implement" is fine for spec-defined work.
- One paragraph per idea. Short paragraphs.
- Cite primary sources (OSDev wiki, Limine docs, Rust embedded book, Asahi blog, kernel.org Documentation/, WASI specs, Wayland protocol docs) by URL, not by name alone.

## What you should *not* do

- **Do not** generate large amounts of speculative code. If it's more than ~150 lines and it's not directly requested, stop and confirm.
- **Do not** edit `docs/plan/ARSENAL.md` unless I explicitly ask. The plan is canonical; deviation from it requires a conversation and an ADR, not a patch.
- **Do not** add dependencies without checking the license against §3 of this file. BSD/MIT/ISC/Apache-2.0/zlib/SIL-OFL are fine. LGPL-2.1+ is fine via Cardboard Box. GPL is fine *only* via the LinuxKPI driver boundary. Anything else, ask.
- **Do not** suggest "modernizing" Rust into a different language. The Rust commitment is post-pivot project identity; the rationale is in ADR-0004. If a piece of code is genuinely better as C (driver shim, vendored library), it lives under `kernel/drivers/` or `vendor/` with a documented license boundary, not in the Arsenal Rust base.
- **Do not** introduce religious framing, biblical naming, the Oracle, or the word "Cathedral." This is non-negotiable.
- **Do not** generate marketing copy unprompted. Devlogs are fine; press releases are not, until I ask.
- **Do not** silently rank performance, usability, or security above the others. When they conflict, surface the trade-off explicitly and propose an ADR.

## What you *should* do

- **Catch me when I'm about to violate the plan.** "This change adds the third option to a settings panel that ARSENAL.md said would have one toggle. Worth the surface area?" is exactly the prompt I want from you.
- **Notice when a task is bigger than I think it is.** Estimate in full-time-equivalent weeks, multiply by ~2.3 for my part-time real-time, and tell me before we start.
- **Suggest the smaller version first.** If I ask for X and Y is 70% of X for 30% of the work, surface Y as an option.
- **Maintain devlog momentum.** Once a month, when I'm wrapping a milestone, draft the devlog post in `docs/devlogs/` for me to edit. The Asahi cadence is the model.
- **Hold the test rig honest.** When we add a feature, add a smoke test. When we touch a hot path, add a perf assertion in CI. The performance gate at each milestone (boot time, frame budget, idle RAM) belongs in CI.
- **Read commit history before re-deriving.** `git log --oneline -- <path>` will save you (and me) from repeating decisions. The `field-os-v0.1` tag is the prior arc's reference point if a problem was solved there in C and we're re-solving it in Rust.

## Working hours and pace

I work on this **part-time, ~15 hours per week**, evenings and weekends. Sometimes a sabbatical week happens. The plan is calibrated against this; do not pressure faster.

If you notice I've been heads-down on a single bug for multiple sessions, the right move is: *"This has been the active issue for three sessions. Want to write up what we've tried and step away for a day?"* Burnout avoidance is a real engineering concern on a 7–10 year solo project.

## When the plan must change

The plan is a strong prior, not a contract. When reality argues with it — a milestone is bigger than estimated, a library we counted on is dead, a hardware choice doesn't survive contact with silicon — the right response is:

1. Write up what changed and why, in `docs/adrs/NNNN-title.md` (Architecture Decision Record format — Michael Nygard's template).
2. Update the affected section of `docs/plan/ARSENAL.md` with a marked revision.
3. Update `STATUS.md`.
4. Commit all three together: `docs(plan): ADR-0011, defer Snapdragon X to v1.5 after Tuxedo X1 cancellation`.

Decisions get recorded. The plan stays current. Future-me reads ADRs and understands.

## Milestone summaries (one-line each — see `docs/plan/ARSENAL.md` for the full text)

- **M0 — Boot and breathe (months 0–9)**: Rust kernel skeleton, UEFI via Limine, serial console, virtio drivers in QEMU, basic scheduler, smoltcp + rustls, simple shell. Boots to a `>` prompt in QEMU.
- **M1 — Real iron (months 9–24)**: LinuxKPI shim, amdgpu KMS, NVMe / xHCI / iwlwifi, first boot on real Framework 13 AMD hardware. Slint app in software-rendered framebuffer.
- **M2 — It looks like Arsenal (months 24–36)**: Stage compositor with iDroid/Big Sur identity, Wayland shim, first five native apps (Cache, Operator, Manual, Frequencies, Inspector), browser via Servo or WebKitGTK. **First public alpha.**
- **v0.5 (months 42–60)**: Wasm component runtime (Wasmtime), POSIX/relibc subset, ports of Firefox / mpv / foot, Brief notebook app, Cardboard Box sandbox.
- **v1.0 (months 60–84)**: Daily-driver maturity on Framework 13 AMD. Snapdragon X port. Cassette / Stencil / Sequence apps. Mail / music / video / IDE. Accessibility shipped. Stockpile remote repository.
- **v2.0 (months 84–120)**: Apple Silicon via Asahi collaboration. Tablet experience. CHERI experimental support if commodity silicon arrives.

Total to v1.0: **5–7 calendar years** from M0. Total to v2.0: **7–10 calendar years**.

---

*If something in this file is wrong or outdated, fix it in the same commit as the change that made it wrong. The file is small. Keep it small.*
