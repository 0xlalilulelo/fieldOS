# ADR-0004: Pivot from Field OS to Arsenal

## Status

Accepted. Supersedes [ADR-0001](0001-holyc-graft.md),
[ADR-0002](0002-holyc-xmm.md), [ADR-0003](0003-x86c-untouched-corpus-driven-encoder.md).

## Context

Field OS was a from-scratch desktop OS modeled on the technical primitives
of TempleOS — HolyC as the universal language for kernel + supervisor +
compositor + apps, Brief as the executable-document format, F5 hot-patch
live-coding, source-as-documentation, a 100,000-line budget for the base
system. The plan ran from M0 (toolchain bring-up, 2026-04-29) through M3
step 6-5 (per-eval cctrl reset, 2026-05-08). The HolyC REPL was working in
QEMU; the encoder was byte-equivalent with GAS across a 63-instruction
corpus; the JIT path landed `X` on serial through a six-step pipeline.

The audit-driven HolyC graft proved technically sound. The pivot is not a
retreat from a failed experiment; it is a re-evaluation of the multi-year
arc to v1.0. Three findings drove the decision:

1. **The single-language HolyC commitment locks the project out of the
   LinuxKPI driver-inheritance path** that every solo / small-team OS
   eventually depends on. Redox, Asterinas, Genode-DDE, and FreeBSD all
   inherit Linux drivers via a shim layer because writing amdgpu / iwlwifi
   / mac80211 / sof-audio from scratch is a multi-decade effort with paid
   teams. The previous CLAUDE.md hard constraint 1 ("HolyC for kernel,
   supervisor, compositor, runtime, document format, shell, file manager")
   forecloses this. Allowing C-via-LinuxKPI as a permitted exception (the
   "kernel/drivers/" carve-out) was technically feasible but
   philosophically incoherent with the single-language identity. Better to
   re-architect with driver inheritance as a first-class concern.

2. **The 100,000-line budget was achievable but only by deferring everything
   that makes a desktop OS desktop-grade.** Sandboxing (per-app capabilities,
   not user-trust ambient authority), accessibility (screen reader, keyboard
   navigation, high-contrast — shipped before v1.0, not retrofitted), modern
   app distribution (Wasm components, signed updates, repository), and
   internationalization all consume significant LOC. Hitting 100K with these
   omitted, then claiming "v1.0" later when they were added, would either
   blow the budget (discipline broken) or ship a v1.0 that fails its own
   usability mission.

3. **The peer-concerns framing (performance / usability / security as
   co-equals) does not translate cleanly into a TempleOS-derived value
   system.** TempleOS's primary axes are simplicity and immediacy
   (one-language, no-protection, identity-mapped, no-network, F5 to patch
   running code). Field OS preserved those technical primitives but
   committed paged + user/kernel separated + preemptive scheduling +
   sandboxing — a combination that is internally consistent but thematically
   strained. Arsenal frames performance / usability / security as peer
   concerns from the start, with documented ADRs when they conflict, and
   accepts the language and architecture choices that follow from that
   commitment rather than from a TempleOS lineage.

## Decision

Pivot to **Arsenal** per [`docs/plan/ARSENAL.md`](../plan/ARSENAL.md). The
new direction:

- **Kernel architecture**: Rust monolithic kernel with capability-secured
  userspace IPC. Not seL4, not pure microkernel, not SASOS. The Redox /
  Asterinas pattern.
- **Primary language**: Rust, end-to-end. C is permitted *only* in
  inherited Linux drivers under the LinuxKPI shim (which retain GPLv2 in
  their original form).
- **Driver strategy**: LinuxKPI-style shim hosting Linux 6.12 LTS drivers
  (amdgpu, i915/xe, iwlwifi, mac80211, sof-audio, xhci, nvme, bluetooth).
  Selected native Rust rewrites for low-complexity drivers (NVMe queueing,
  USB-HID, virtio).
- **Application distribution**: three-tier — native Rust binaries for
  first-party apps, Wasm components (WASI 0.2 → 0.3) for sandboxed
  third-party apps, POSIX subset (relibc-style) for ports of Firefox,
  mpv, git, foot.
- **Sandboxing**: capability-based per-app container ("Cardboard Box")
  with declared capabilities at install time, granted per-capability at
  first launch.
- **License**: BSD-2-Clause for the Arsenal base. GPLv2 preserved on
  inherited Linux drivers (combined-work model, the FreeBSD / drm-kmod
  pattern).
- **Project name**: Arsenal. Replaces "Field OS." Consistent with the
  MGS3 vocabulary (Arsenal Gear, Metal Gear Solid 2). Single word, easy
  to say, no religious or TempleOS associations.

The full plan is in `docs/plan/ARSENAL.md`. This ADR records the *decision
to pivot*; the plan document records *what Arsenal is*.

### What carries forward from Field OS

- BSD-2-Clause license.
- Vendored Limine bootloader (`vendor/limine/`); Arsenal still uses Limine.
- Visual identity: amber `#FFB200`, cyan `#00C8E0`, navy `#0A1A2A`; IBM Plex
  Mono / Sans / Serif; 4 px spacing grid; 8 / 12 / 20 px corner radii;
  Big Sur translucent vibrancy; holographic milspec scan-line shader on
  Stage chrome.
- MGS3-warm tactical naming catalog, expanded — see `docs/naming.md` and
  ARSENAL.md § "Naming Catalog (Preserved)".
- The Brief executable-document format concept (re-justified as a generic
  notebook concept, not TempleOS-specific).
- Commit hygiene (Conventional Commits, one concern per commit), ADR
  discipline (Michael Nygard format, monotonic numbering), devlog cadence
  (Asahi-style monthly wraps).
- The `docs/devlogs/` directory and the qemu test-harness shape.
- The historical record at the `field-os-v0.1` tag (commit `dffe259`,
  2026-05-08) — the high-water mark of the Field OS arc, preserved so
  `git checkout field-os-v0.1` resurrects the C kernel any time.

### What is discarded

- HolyC as a language. The single-language commitment.
- The 100,000-LOC budget for the base system. (Arsenal is not budget-free;
  it is just not budget-disciplined the same way. Each subsystem has its
  own performance and binary-size targets in ARSENAL.md.)
- F5 hot-patch live-coding as a primary mechanism. Arsenal's developer UX
  is the Inspector overlay (Genode Leitzentrale pattern), not in-place
  source patching of running kernels.
- Source-as-documentation as the primary documentation model. Arsenal
  ships rendered docs (Manual app + Field Manual help system); source
  comments are about *why*, not *how to read the program*.
- The C kernel (`kernel/main.c`) and the M0 / M1 / M2 / M3 work it grew
  through. Preserved at the `field-os-v0.1` tag.
- The vendored `holyc/` upstream tree. Removed in Phase B of the
  transition.
- The cross-GCC `x86_64-elf` toolchain. Replaced by stable Rust + the
  `x86_64-unknown-none` target.
- The `docs/plan/phase-{0..3}.md` Field OS plan. Archived to
  `docs/plan/legacy/` in Phase A.

## Consequences

**Easier:**

- Driver inheritance unblocks commodity hardware support; Framework 13 AMD
  / Intel, Snapdragon X laptops, and Apple Silicon (via Asahi) all become
  reachable through the LinuxKPI shim rather than from-scratch driver
  development.
- Capability sandboxing becomes architecturally native. Cardboard Box's
  per-app capability model is the canonical sandbox; not a retrofit.
- The Rust ecosystem unlocks: smoltcp + rustls for the network stack,
  wgpu / Skia for the compositor, Slint for the UI toolkit, Wasmtime for
  the Wasm component runtime. None of these would have been reachable
  from a single-language HolyC base.
- Memory safety as a kernel-level guarantee (Rust's type system across the
  base) replaces HolyC's "trust the language" model. `unsafe` blocks become
  the audit surface, with `// SAFETY:` invariant comments mandatory.
- ARSENAL.md's peer-concerns framing matches what the project actually
  cares about. The CLAUDE.md hard constraints become a single-page
  description of three peers plus the visual / naming identity.

**Harder:**

- Rust toolchain bring-up replaces a working cross-GCC + Limine path. The
  M0 step-1 work re-derives Limine integration in Rust (limine-rs crate
  or hand-rolled). One session of work, not a decade — but real work
  nonetheless.
- The C kernel's M0 / M1 / M2 / M3 work is sunk cost. The M3 step 6-5
  REPL that just landed will not boot Arsenal. The lessons (paging, GDT /
  IDT setup, slab allocation, JIT page management) carry forward as
  understanding; the code does not.
- The "TempleOS modernization" framing's 6-month-of-positioning is gone.
  Arsenal positions itself fresh, against Redox / SerenityOS / Genode /
  Asahi — peers it must compare honestly to, not a single ancestor it
  modernizes.
- The visual mockup work and Stitch prompts referenced "Field OS" in
  wordmarks; one-line replacement to "Arsenal" each.

**New risks:**

- **Slint accessibility** is the single biggest UX risk per ARSENAL.md §
  Caveats. If a screen-reader user cannot navigate Arsenal, the project
  fails its usability mission. Either contribute upstream to Slint's
  AccessKit integration or write a dedicated a11y compatibility layer in
  Stage. Budget 2–4 person-months in Phase 1; v1.0 release blocker.
- **WASI 0.3 timing** historically slips. ARSENAL.md does not commit to
  async-Wasm-as-primary-IPC until WASI 1.0 ships. The risk is that
  third-party Wasm app developers want patterns WASI 0.2 cannot express;
  mitigation is the POSIX-subset tier as an escape hatch.
- **Asahi Linux governance flux** (Asahi Lina paused March 2025; Hector
  Martin stepped down February 2025; seven-person shared governance
  since). The Apple Silicon port (Phase 3 / v2.0) depends on Asahi's
  continued upstreaming progress, which is not guaranteed. ARSENAL.md
  mandates a Phase 3 contingency that does not require Asahi.
- **Snapdragon X Linux support regressed in Q4 2025** (Tuxedo cancelled
  their X1 Elite laptop, November 2025). The Snapdragon X commitment may
  need a fallback target — Framework 13 Intel Core Ultra is the safer
  second platform.
- **Solo-builder framing is generous.** Every comparable project that
  shipped — Redox, SerenityOS, Genode, Asahi — eventually became
  multi-person efforts. Plan for solo → small-team transition around year
  3–4; the BSD-2 license is designed to make that transition possible.

## Follow-up work

This ADR's decision triggers the transition plan documented at
`/Users/silmaril/.claude/plans/smooth-discovering-muffin.md` (a
session-local plan file; the canonical record is the commits this ADR
lands alongside). The transition runs:

- **Phase A** (this commit + 6–7 more in this session): paper deliverables
  — CLAUDE.md rewrite, naming.md merge, STATUS.md rewrite, phase-*.md
  archival, README / PLAN / CHANGELOG updates, pivot devlog.
- **Phase B** (next session, one commit): code archival — single removal
  commit takes out `kernel/`, `holyc/`, the cross-GCC toolchain, the
  Field OS Makefile. The `field-os-v0.1` tag remains the access path.
- **Phase C** (one or two sessions after B): first Rust scaffolding —
  Cargo workspace, `arsenal-kernel` crate, Limine boot, COM1 sentinel
  `ARSENAL_BOOT_OK`. Phase C ends at the first Arsenal commit booting
  to its sentinel.

ARSENAL.md M0 in full — the boot to `>` prompt with virtio drivers,
smoltcp + rustls, basic scheduler — is 9 calendar months of part-time
work per the new timeline. That work is tracked session by session
against ARSENAL.md M0's bullet list, not against this ADR.

## References

- [`docs/plan/ARSENAL.md`](../plan/ARSENAL.md) — the canonical Arsenal
  plan. This ADR's decision is "do that."
- The `field-os-v0.1` tag (commit `dffe259`) — the Field OS PoC at M3
  step 6-5, the high-water mark of the prior arc.
- [ADR-0001](0001-holyc-graft.md) — HolyC graft strategy, now superseded.
- [ADR-0002](0002-holyc-xmm.md) — HolyC subset xmm activation, now
  superseded.
- [ADR-0003](0003-x86c-untouched-corpus-driven-encoder.md) — corpus-driven
  encoder, now superseded.
- Comparable projects: Redox (https://www.redox-os.org/) at year 11
  pre-1.0, SerenityOS (https://serenityos.org/) at year 7 daily-driver,
  Genode (https://genode.org/) at year 19 with paid team, Asahi Linux
  (https://asahilinux.org/) at year 5 daily-driver M1.
- Michael Nygard, "Documenting Architecture Decisions" (2011).
