# CLAUDE.md

> This file is read by Claude Code at the start of every session.
> Keep it short, opinionated, and current. If a rule changes, change it here.

## What this project is

**Field OS** — a from-scratch desktop operating system for commodity x86_64 PCs (and later ARM64 Snapdragon X and Apple Silicon M1/M2). Built by one person, evenings and weekends, on a multi-year arc.

It is inspired by the *technical primitives* of TempleOS — HolyC as the universal language, the Brief executable-document format, the shell-is-the-compiler REPL, the F5 hot-patch live-coding, the source-as-documentation `#help_index` model, the line-count discipline — and the *visual identity* of macOS Big Sur (translucent vibrancy, 8/12/20 px corner radii, 4 px spacing grid, IBM Plex SIL OFL typography).

It is **not** TempleOS. It carries forward none of TempleOS's religious framing or ring-0 / identity-mapped / no-protection architecture. Field OS is paged, user/kernel-separated, preemptively scheduled. It happens to feel as immediate and direct as Terry Davis's original.

## Hard constraints — never violate without an explicit conversation first

1. **Single-language base system.** HolyC for kernel, supervisor, compositor, runtime, document format, shell, file manager. C / C++ / Rust permitted **only** for hardware drivers and ported third-party libraries.
2. **100,000-line budget** for the base system (excludes drivers and ported libraries). CI tracks this and fails the build at 95% consumed. Every line costs.
3. **License: BSD-2-Clause** for everything written here. Ported drivers keep their original license at the LinuxKPI shim boundary; vendored libraries live in Cardboard Boxes with documented licensing.
4. **Visual identity.** Big Sur translucent vibrancy / 8, 12, or 20 px corner radii (small/medium/large) / 4 px spacing grid / IBM Plex Sans, Mono, Serif (SIL OFL) / Field Symbols icons (Lucide ISC fork + custom). No deviation without an architecture decision record.
5. **MGS3-warm tactical naming throughout.** No religious framing — never use Cathedral, Solomon, Covenant, Tabernacle, Oracle, or biblical references. The naming catalog is in `docs/naming.md`; consult it before introducing any new system component name.
6. **TempleOS technical core preserved.** HolyC universal language, Brief executable documents, F5 hot-patch, source-as-documentation. These are not negotiable.

## Naming catalog (one-line summary)

The full canonical catalog lives in [`docs/naming.md`](docs/naming.md). Consult it before introducing any new system component name; the 3-to-5-candidate shortlist protocol is documented there. No religious framing, ever — never Cathedral, Solomon, Covenant, Tabernacle, Oracle, or biblical references.

The six names you'll touch most in M0–M2: **Patrol** (service supervisor / scheduler) · **Stage** (compositor) · **Channel** (IPC) · **Cache** (file manager) · **Operator** (shell) · **Brief** (executable document format).

## Where I am right now

See `STATUS.md` for the current milestone and active work. As of this commit, the project is at: **`<update STATUS.md when state changes>`**.

The full multi-year plan lives in `docs/plan/` — Phase 0 (`docs/plan/phase-0.md`) through Phase 3 (`docs/plan/phase-3.md`). Each phase has milestones M*N*. **Always read the relevant phase plan and the active milestone section before suggesting a change of direction.** The plan absorbs months of design discussion; do not reinvent decisions it has already made unless asked.

## How to work with me on this repo

### Read before writing

1. Read `STATUS.md` to know the current milestone.
2. Read the relevant section of `docs/plan/phase-N.md`.
3. Read any `SKILL.md`-equivalent in `docs/skills/` that applies (e.g., `docs/skills/holyc.md` before generating HolyC; `docs/skills/limine.md` before touching boot config).
4. *Then* read the user's request and propose work.

### Edit, don't rewrite

This codebase is mine. I have opinions about every line. When I ask for a change:
- Make the smallest change that satisfies the request.
- Preserve existing style (naming, indentation, comment density).
- Never reformat unrelated code. Never drive-by-clean adjacent files.
- If you think a refactor is warranted, **ask first** with a specific proposal, not a sweeping rewrite.

### The build loop is sacred

The full build is `make iso && tools/qemu-run.sh`. Every change must keep this green or have a clear, stated reason for being intermediate work. If you make changes and the smoke test stops printing `FIELD_OS_BOOT_OK` on serial, that is a regression and must be flagged.

When you finish a unit of work, **always run** `make iso && ci/qemu-smoke.sh` and report the result. Don't claim a milestone exit criterion is met without seeing the smoke test pass.

### Commit hygiene

- One concern per commit. Conventional Commits format: `feat(stage): floating window snap-to-grid`, `fix(mm): off-by-one in slab cache resize`, `docs(plan): tighten M3 exit criterion`.
- If you propose a commit, write the full message, including a body that explains the *why*, not just the *what*.
- Squash-merge to `main`. Even as a solo dev. Bisect time matters more than a clean linear log costs.

### When you don't know

- HolyC's exact syntax for a construct → grep the existing codebase first; if absent, ask me.
- A spec detail (Limine protocol, NVMe register, ACPI method) → cite the spec section you're working from.
- A design call I haven't made yet → propose 2–3 options with trade-offs and ask. Do not silently pick one.
- The right MGS3-warm name for a new thing → propose 3–5 candidates from the conceptual space and let me choose. Do not invent without a shortlist.

## Style — code

- **HolyC formatting:** match `Compiler.HC` ancestors. Tab indent (8-wide visual). Opening brace on the same line. K&R-ish but with the TempleOS preference for short, dense functions.
- **C formatting (driver / shim code only):** Linux kernel style (8-wide tabs, opening brace on same line for functions but on the next line for control flow — actually match `linux/Documentation/process/coding-style.rst`). 80-column soft limit, 100-column hard limit.
- **No clever code in kernel paths.** Clarity over brevity. A junior reader six months from now should understand it without context.
- **Assertions liberally.** `Bt(cond, "panic message")` in HolyC, `BUG_ON(cond)` in C.
- **Comments explain *why*, not *what*.** The code already says what.

## Style — prose (devlogs, Field Manual chapters, README)

- Match the tone of `docs/plan/phase-*.md`: serious, calibrated, honest about timelines, never marketing, never hyperbolic.
- No exclamation points. No emoji except in user-facing UI strings explicitly marked for translation.
- "Ship" not "deliver." "Build" not "implement" when describing creative work; "implement" is fine for spec-defined work.
- One paragraph per idea. Short paragraphs.
- Cite primary sources (OSDev wiki, Limine docs, Phil Opp, Asahi blog, kernel.org Documentation/) by URL, not by name alone.

## What you should *not* do

- **Do not** generate large amounts of speculative code. If it's more than ~150 lines and it's not directly requested, stop and confirm.
- **Do not** edit `docs/plan/phase-*.md` unless I explicitly ask. The plan is canonical; deviation from it requires a conversation, not a patch.
- **Do not** add dependencies without checking the license against §3 of this file. BSD/MIT/ISC/Apache-2.0/zlib/SIL-OFL are fine. LGPL-2.1+ is fine via Cardboard Box. GPL is fine *only* via the LinuxKPI driver boundary. Anything else, ask.
- **Do not** suggest "modernizing" HolyC into a different language. The single-language commitment is the project's identity. If a piece of code is genuinely better as C (driver shim, ported library), it lives in `kernel/drivers/` or `vendor/`, not in the HolyC base.
- **Do not** suggest dropping the line-count budget. The discipline is the point.
- **Do not** introduce religious framing, biblical naming, the Oracle, or the word "Cathedral." This is non-negotiable.
- **Do not** generate marketing copy unprompted. Devlogs are fine; press releases are not, until I ask.

## What you *should* do

- **Catch me when I'm about to violate the plan.** "This change adds 2,000 lines to the base system; we're at 78% of budget. Worth it?" is exactly the prompt I want from you.
- **Notice when a task is bigger than I think it is.** Estimate in full-time-equivalent weeks, multiply by ~2.3 for my part-time real-time, and tell me before we start.
- **Suggest the smaller version first.** If I ask for X and Y is 70% of X for 30% of the work, surface Y as an option.
- **Maintain devlog momentum.** Once a month, when I'm wrapping a milestone, draft the devlog post in `docs/devlogs/` for me to edit. The Asahi cadence is the model.
- **Hold the test rig honest.** When we add a feature, add a smoke test. When we touch a hot path, add a perf assertion in CI.
- **Read commit history before re-deriving.** `git log --oneline -- <path>` will save you (and me) from repeating decisions.

## Working hours and pace

I work on this **part-time, ~15 hours per week**, evenings and weekends. Sometimes a sabbatical week happens. The plan is calibrated against this; do not pressure faster.

If you notice I've been heads-down on a single bug for multiple sessions, the right move is: *"This has been the active issue for three sessions. Want to write up what we've tried and step away for a day?"* Burnout avoidance is a real engineering concern on a 6-year solo project.

## When the plan must change

The plan is a strong prior, not a contract. When reality argues with it — a milestone is bigger than estimated, a library we counted on is dead, a hardware choice doesn't survive contact with silicon — the right response is:

1. Write up what changed and why, in `docs/adrs/NNNN-title.md` (Architecture Decision Record format — Michael Nygard's template).
2. Update the affected `docs/plan/phase-*.md` section with a marked revision.
3. Update `STATUS.md`.
4. Commit all three together: `docs(plan): ADR-0007, defer S0ix to v0.2 on Tier-2 hardware`.

Decisions get recorded. The plan stays current. Future-me reads ADRs and understands.

## Phase summaries (one-line each — see `docs/plan/` for the full text)

- **Phase 0 (M0–M10)**: QEMU PoC. HolyC compiler on bare metal, Brief renderer, software compositor, PS/2 input, BGA framebuffer. **12–18 months part-time.**
- **Phase 1 (M11–M50)**: Real hardware on Framework 13 AMD. LinuxKPI shim, AMDGPU/i915, Foundry GPU compositor, Comm Tower, Wavelength, Cardboard Box, Stockpile, Patch, accessibility v1.0, launch app suite. **v0.1 release. 18–30 months part-time.**
- **Phase 2 (M51–M90)**: Snapdragon X bring-up. WASM Tabernacles. Manual (Pages-class), Armory (VS Code-class), Cassette (Logic-class DAW), Negatives v2 (Capture One-class). Stable ABI, SDK, remote Stockpile, 11 localizations. **v1.0 release. 24–36 months part-time.**
- **Phase 3 (M91–M130)**: Apple Silicon M1/M2 via Asahi collaboration. Full tablet experience. Stencil (Illustrator-class), Sequence (Resolve-class). Cellular, server profile. **v2.0 release. 24–36 months part-time.**

Total to v2.0: **6–9 calendar years** from M0.

---

*If something in this file is wrong or outdated, fix it in the same PR as the change that made it wrong. The file is small. Keep it small.*
