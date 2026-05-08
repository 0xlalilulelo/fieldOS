# Arsenal — pivot from Field OS

*May 8, 2026. ADR-0004 + Phase A paper deliverables.*

The Field OS arc closed at the `field-os-v0.1` tag (commit `dffe259`)
with the HolyC REPL working in QEMU under `make repl-iso`. M3 step 6-5
landed clean: encoder byte-equivalent with GAS across a 63-instruction
corpus; JIT path landed `X` on serial through a six-step pipeline
(parse → codegen → encode → relocate → commit → invoke); per-eval
cctrl reset proved out via a four-line REPL session that exercised
top-level accumulation, parse-error recovery, and post-error continued
operation.

The C kernel works. The graft worked. The plan was holding.

Then we pivoted.

## Why

Three findings drove the decision, all surfacing not from any one
session's friction but from re-evaluating the multi-year arc against
what Arsenal's mission actually demands.

**The single-language HolyC commitment locks the project out of
LinuxKPI driver inheritance.** Every solo / small-team OS that has
shipped commodity-hardware support — Redox, Asterinas, Genode-DDE,
FreeBSD — does it by inheriting Linux drivers via a shim layer.
Writing amdgpu, iwlwifi, mac80211, sof-audio from scratch is a
multi-decade effort with paid teams. Field OS would have allowed
C-via-LinuxKPI as a permitted exception (the "kernel/drivers/"
carve-out), but doing so meaningfully would mean a substantial portion
of the running system is C, which is philosophically incoherent with a
single-language HolyC identity. Better to re-architect with driver
inheritance as a first-class concern.

**The 100,000-line budget was achievable but only by deferring
everything that makes a desktop OS desktop-grade.** Sandboxing (per-app
capabilities, not user-trust ambient authority), accessibility (screen
reader, keyboard navigation, high-contrast — shipped before v1.0, not
retrofitted), modern app distribution (Wasm components, signed updates,
repository), and internationalization all consume significant LOC.
Hitting 100K with these omitted, then claiming "v1.0" later when they
were added, would either blow the budget (discipline broken) or ship a
v1.0 that fails its own usability mission.

**The peer-concerns framing does not translate cleanly into a
TempleOS-derived value system.** TempleOS's primary axes are simplicity
and immediacy — one language, no protection rings, identity-mapped
memory, no network, F5 to patch a running kernel. Field OS preserved
those technical primitives but committed paged + user/kernel separated
+ preemptive scheduling + sandboxing — a combination that is
internally consistent but thematically strained. Arsenal frames
performance / usability / security as co-equal peers from the start,
with documented ADRs when they conflict, and accepts the language and
architecture choices that follow from that commitment rather than from
a TempleOS lineage.

None of these findings is fatal in isolation. Each is a re-architecture
hint that compounds with the others.

## What was learned

The Field OS arc was not wasted. The lessons that carry forward into
Arsenal are concrete:

- **Limine is the right bootloader.** Vendored at v9.x; works; ships;
  the protocol is small enough to fit in a session of reading. No
  reason to re-evaluate.
- **The "audit before grafting" discipline.** The
  [`docs/skills/holyc-lang-audit.md`](../skills/holyc-lang-audit.md)
  document — six steps from "fork in place" to "the REPL" — was the
  right shape. Phase 0 of Arsenal will probably need a similar
  audit-before-grafting pass if any large vendored crate (limine-rs,
  smoltcp, rustls) needs structural changes.
- **The encoder corpus pattern.** ADR-0003 established that the in-tree
  encoder consumes a corpus produced by the upstream's text emit, and
  the harness verifies byte-equivalence with GAS on every CI run. That
  pattern — vendored upstream emits structured input, our consumer
  treats it as a closed-set parser — generalizes to anywhere we wrap
  vendored code.
- **The line walker's quote-aware mode.** 5-4c-prep surfaced that the
  most likely surprise in a vendored consumer is a *missing parser
  mode*, not a missing API entry. Arsenal's LinuxKPI shim work will
  encounter the same shape: not "this kernel API is missing" but "this
  pattern in the upstream's source assumes context our shim doesn't
  reproduce."
- **The setjmp / longjmp recovery pattern.** Arsenal's parse-error
  recovery in any future scripting layer (Brief macros, an Operator
  shell parser, future scripted-config loaders) will reach for the
  same shape: arm the longjmp before entering the vendored panic-
  reachable surface; clear state on entry to every call so the next
  invocation starts clean.
- **The cctrl-state research.** What we learned from 6-5 — that "leave
  state alone" hypotheses about long-lived vendored handles are
  usually wrong, and the research before code is cheap insurance —
  applies to any long-lived Rust state object Arsenal owns (Limine
  request bundles, Wasmtime engine handles, vulkan device contexts).
- **The Asahi-cadence devlog.** Once-a-month-ish, calibrated and
  honest, never marketing. This devlog is part of that cadence.

## What carries forward verbatim

- **License**: BSD-2-Clause.
- **Vendored Limine**: Same tree at `vendor/limine/`. Same boot path.
  The kernel binary it loads changes; the loader does not.
- **Visual identity**: amber `#FFB200`, cyan `#00C8E0`, navy
  `#0A1A2A`; IBM Plex Mono / Sans / Serif; 4 px grid; 8 / 12 / 20 px
  corner radii; Big Sur translucent vibrancy; holographic milspec
  scan-line shader on Stage chrome. The Stitch prompts that produced
  the M2-era mockups remain valid; only the wordmark changes from
  "Field OS" to "Arsenal."
- **MGS3-warm naming catalog**: Patrol, Stage, Cache, Operator,
  Cardboard Box, Comm Tower, Brief, and the rest. Catalog at
  `docs/naming.md`; ARSENAL.md § "Naming Catalog (Preserved)" lists
  the same names with one-line role descriptions.
- **Commit hygiene** (Conventional Commits, one concern per commit),
  **ADR discipline** (Michael Nygard format, monotonic numbering,
  paired with code in the same commit when behavior changes), and the
  **devlog cadence** (this file's tone).
- **The historical record** at `docs/devlogs/2026-04-m0.md`,
  `2026-05-m1.md`, `2026-05-m2.md`, `2026-05-m3-step5.md`, and
  `2026-05-m3b.md`. Untouched. Future archaeologists will read them
  to understand how Field OS was shaped before it became Arsenal.
- **The qemu test-harness shape.** `ci/qemu-smoke.sh` will be rewritten
  for Arsenal in Phase C, but the structure (headless QEMU + serial
  grep + distinct exit codes for failure modes + JIT-witness bracket
  pattern) is the right shape and ports directly.

## What is being let go

The C kernel itself, the M0 / M1 / M2 / M3 work it grew through, the
vendored holyc/ tree (~21,000 LOC of upstream), the cross-GCC
toolchain, and the four-phase plan documents (now at
`docs/plan/legacy/`). All preserved at the `field-os-v0.1` tag for any
future archaeology, but not in the working tree after Phase B closes.

The TempleOS-derived primitives — F5 hot-patch live-coding,
source-as-documentation, the source-is-the-program model — are dropped
as primary mechanisms. Inspector overlay (Genode Leitzentrale pattern)
replaces F5; rendered docs (Manual app + Field Manual help system)
replace source-as-documentation. The substance of immediate-feedback
development survives — Inspector lets you watch every IPC and capability
in real time — but the mechanism shifts.

The 100,000-LOC budget is dropped. Arsenal's discipline becomes
performance / usability / security peer-concerns gates per milestone,
not a global LOC ceiling. Whether this proves harder or easier remains
to be seen; it is at least more honest about what a desktop OS actually
costs.

## What's next

This session lands Phase A of the transition: ADR-0004, CLAUDE.md
rewrite, naming.md merge, STATUS.md update, phase-doc archival, README
/ CHANGELOG, and this devlog. Eight commits including this one.

Next session: Phase B. A single removal commit takes out `kernel/`,
the vendored `holyc/` tree, the cross-GCC toolchain helpers, and the
top-level Makefile. The working tree becomes Arsenal-shaped.

Subsequent sessions: Phase C. Cargo workspace at the repo root.
`arsenal-kernel` crate (no_std, x86_64-unknown-none target). Limine
boot. COM1 sentinel `ARSENAL_BOOT_OK`. The first Arsenal commit that
boots.

After Phase C, the transition is over. The remaining work is
ARSENAL.md M0 in full — paging, scheduler, virtio drivers, smoltcp +
rustls, the `>` prompt — and the rest of the multi-year arc to v1.0
on Framework 13 AMD.

## A note on the timeline

Field OS's plan estimated 6–9 calendar years to v2.0. Arsenal's plan
estimates 7–10. The half-year-to-year increase is realistic: ARSENAL.md
ships sandboxing, accessibility, and modern app distribution as v1.0
release-blockers rather than v1.5+ deferrals. The total work goes up.
The honesty about what "desktop OS" means goes up the same amount.

The plan is holding. The shape changed. Onward.

—
