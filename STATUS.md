# STATUS

> What I am doing right now. Updated whenever the milestone changes,
> a major design decision lands, or a session leaves something
> mid-flight that future-me needs to know about.

## Current milestone

**Pre-M0 — Field OS → Arsenal transition** *(ADR-0004 landed
2026-05-08; transition runs ~2–3 sessions)*

### What's happening

The project pivoted from **Field OS** (TempleOS-modernization in HolyC,
M3 step 6-5 working REPL at the `field-os-v0.1` tag) to **Arsenal**
(Rust monolith with capability-secured userspace, LinuxKPI driver
inheritance, tri-modal app distribution). Rationale is in
[ADR-0004](docs/adrs/0004-arsenal-pivot.md); the canonical plan is
[`docs/plan/ARSENAL.md`](docs/plan/ARSENAL.md).

### Active work

**Phase A — Paper deliverables (complete).** Every document in the repo
describes Arsenal: ADR-0004, the CLAUDE.md rewrite, the naming.md merge,
the README / CHANGELOG / ARSENAL.md rewrites, three legacy archive
READMEs, and the pivot devlog landed across nine commits ending at
`4ecd0b4` on 2026-05-08.

**Phase B — Code archival (complete).** Removal commit `a35c473`
(2026-05-08) deleted `kernel/`, the vendored `holyc/` tree, `base/`,
`assets/`, the top-level `Makefile`, `ci/qemu-smoke.sh`, the cross-GCC
toolchain scripts, `tools/count-loc.sh`, and `tools/qemu-run.sh` —
196 files, 43,322 lines. Follow-up `0370b1f` stubbed
`.github/workflows/ci.yml` to a noop placeholder. Access path to the
C kernel preserved via `git checkout field-os-v0.1`.

**Phase C — Rust scaffolding (next session).** Cargo workspace +
`arsenal-kernel` crate + Limine boot + COM1 sentinel `ARSENAL_BOOT_OK`.
This is the first Arsenal commit that boots; ARSENAL.md M0 step 1.

### After the transition

Arsenal M0 in full per ARSENAL.md § "Three Concrete Starting Milestones"
→ M0: boot to a `>` prompt in QEMU, virtio block + virtio-net,
smoltcp + rustls, basic scheduler, framebuffer console, paging, SMP.
Performance gate: boot to prompt in < 2 s under QEMU. Security gate:
zero `unsafe` Rust outside designated FFI boundaries. Usability gate:
prompt is keyboard-navigable; shows hardware summary.

Estimated 9 calendar months part-time per the new timeline.

## Last completed milestone

**Field OS PoC v0.1** (tag `field-os-v0.1`, commit `dffe259`,
2026-05-08). M3 step 6-5: per-eval cctrl reset, the HolyC REPL working
in QEMU under `make repl-iso`. Encoder byte-equivalent with GAS across
a 63-instruction corpus; JIT path landed `X` on serial through a
six-step pipeline (parse → codegen → encode → relocate → commit →
invoke); the M3 5-line exit-criterion session worked in miniature.
~6,274 LOC of base-system C across 56 files at the high-water mark.

The C kernel is preserved at the tag; `git checkout field-os-v0.1`
resurrects it. Bringing it back into `main` would require reverting
Phase B's removal commit.

## Earlier milestones

**M2 — Memory Management** (2026-05-05 → 2026-05-06, four commits,
+1,814 LOC). Tag `M2-complete` on commit `6cd9855`. PMM + VMM + slab.

**M1 — Boot to Long Mode** (2026-04-30 → 2026-05-04, four commits,
+700 LOC). Tag `M1-complete` on commit `c211cf8`. GDT + TSS, IDT, BGA
framebuffer with 8×8 font, "Hello, Field" rendered.

**M0 — Tooling and Bootstrap** (2026-04-29 → 2026-04-30, six commits,
~190 LOC base-system C, ~21,000 LOC vendored). Tag `M0-complete` on
commit `60e1a48`. Cross-GCC toolchain, Limine vendored, `make iso`
producing a bootable ISO.

These tags remain in place; the work is preserved at `field-os-v0.1`
along with everything else from the Field OS arc.
