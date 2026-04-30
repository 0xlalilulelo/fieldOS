# STATUS

> What I am doing right now. Updated whenever the milestone changes,
> a major design decision lands, or a session leaves something
> mid-flight that future-me needs to know about.

## Current milestone

**M0 — Tooling and Bootstrap** *(in progress)*

Started: 2026-04-29

### M0 deliverables

- [x] Step 1 — Repo skeleton, license, naming catalog
- [x] Step 2 — Cross-compiler toolchain (`tools/build-toolchain.sh`,
  `tools/toolchain.mk`, `tools/fetch-toolchain.sh`)
- [x] Step 2.5 — Top-level `Makefile` + `tools/qemu-run.sh`
  (pulled forward from step 3 to validate toolchain integration)
- [x] Step 3 — Limine v12.0.2 boot path + ~120 lines of C kernel;
  serial prints `Field OS: stage 0 reached` and `FIELD_OS_BOOT_OK`
  under QEMU TCG
- [x] Step 4 — CI smoke loop. `ci/qemu-smoke.sh`, `tools/count-loc.sh`,
  `.github/workflows/ci.yml` with three jobs: `build-iso`, `smoke`,
  `loc-budget`. The `reproducibility` job is deferred to M10 (we don't
  yet have byte-identical builds and known-failing CI noise erodes
  signal).
- [x] Step 5 — `holyc-lang` beta-v0.0.10 vendored at `holyc/`
  (BSD-2-Clause, 21,497 LOC across 42 src/ files); audit notes at
  `docs/skills/holyc-lang-audit.md` covering architecture, libc
  surface, ABI, and a six-step M3 graft roadmap.

### Exit criterion

`make iso && ci/qemu-smoke.sh` is green locally and in GitHub Actions.
Serial prints `Field OS: stage 0 reached` followed by the sentinel
`FIELD_OS_BOOT_OK`. The toolchain rebuilds reproducibly from a clean
checkout on Debian 12 / Ubuntu 24.04 / Fedora 41.

## Active work

M0 step 5 just landed: vendored `holyc-lang` beta-v0.0.10
(BSD-2-Clause, 21,497 LOC) into `holyc/`; audit at
`docs/skills/holyc-lang-audit.md` documents architecture, libc
deps, ABI assumptions, and a concrete six-step roadmap for the
M3 freestanding-backend graft.

**M0 is complete.** Final exit criterion verified post-vendor:
`make clean && make iso && ci/qemu-smoke.sh field-os-poc.iso`
is green (FIELD_OS_BOOT_OK on serial in 2 s). Line-count budget
consumed: 166/100,000 (0%). Five commits across two evenings.

Next: M1 — boot to long mode in earnest. GDT (5 entries + TSS),
IDT with 256 stub handlers and IST stacks for #DF/#NMI/#MC,
framebuffer-backed `Println` alongside serial, "Hello, Field"
on the framebuffer with a PSF bitmap font deferred to M5.
Estimated 1–2 FT-weeks per phase-0.md §M1, ~3 weeks part-time.

## Last completed milestone

**M0 — Tooling and Bootstrap** (2026-04-29 → 2026-04-30, five
commits, ~190 LOC of base-system C, 21,000 LOC vendored at
arm's length under `vendor/limine/` and `holyc/`).

## Last completed milestone

None — M0 is the first.
