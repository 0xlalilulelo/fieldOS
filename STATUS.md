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
- [ ] Step 5 — `holyc-lang` vendored and audited (recon only; the
  freestanding-backend graft is M3)

### Exit criterion

`make iso && ci/qemu-smoke.sh` is green locally and in GitHub Actions.
Serial prints `Field OS: stage 0 reached` followed by the sentinel
`FIELD_OS_BOOT_OK`. The toolchain rebuilds reproducibly from a clean
checkout on Debian 12 / Ubuntu 24.04 / Fedora 41.

## Active work

M0 step 4 just landed. `ci/qemu-smoke.sh` (headless QEMU + serial
grep, distinct exit codes for missing-ISO / timeout / startup-fail
/ guest-CPU-fault), `tools/count-loc.sh` (reports against the
100k-line budget; soft-pass at 0% / 166 LOC currently),
`.github/workflows/ci.yml` (three jobs on Ubuntu 24.04: `build-iso`
with toolchain caching, `smoke` consuming the artifact,
`loc-budget`). The fourth `reproducibility` job from the original
plan is deferred to M10 alongside the SOURCE_DATE_EPOCH and
xorriso-determinism work.

Local verification: `tools/count-loc.sh` reports 166/100,000;
`ci/qemu-smoke.sh field-os-poc.iso` is green in 2 seconds.

Next: M0 step 5 — vendor `holyc-lang` (Jamesbarford) into `holyc/`
and write the audit notes for the M3 freestanding-backend graft
at `docs/skills/holyc-lang-audit.md`. Recon only; the actual graft
is M3.

## Last completed milestone

None — M0 is the first.
