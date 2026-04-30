# STATUS

> What I am doing right now. Updated whenever the milestone changes,
> a major design decision lands, or a session leaves something
> mid-flight that future-me needs to know about.

## Current milestone

**M0 — Tooling and Bootstrap** *(in progress)*

Started: 2026-04-29

### M0 deliverables

- [x] Step 1 — Repo skeleton, license, naming catalog
- [ ] Step 2 — Cross-compiler toolchain script (`tools/build-toolchain.sh`,
  `tools/toolchain.mk`, `tools/fetch-toolchain.sh`)
- [ ] Step 3 — Limine boot path + ~50-line C kernel printing the
  `Field OS: stage 0 reached` line and the `FIELD_OS_BOOT_OK` sentinel
- [ ] Step 4 — CI smoke loop (`build-iso`, `smoke`, `loc-budget`,
  `reproducibility` jobs)
- [ ] Step 5 — `holyc-lang` vendored and audited (recon only; the
  freestanding-backend graft is M3)

### Exit criterion

`make iso && ci/qemu-smoke.sh` is green locally and in GitHub Actions.
Serial prints `Field OS: stage 0 reached` followed by the sentinel
`FIELD_OS_BOOT_OK`. The toolchain rebuilds reproducibly from a clean
checkout on Debian 12 / Ubuntu 24.04 / Fedora 41.

## Active work

M0 step 1 just landed: directory tree, BSD-2-Clause license, README,
CHANGELOG, .gitignore, naming catalog at `docs/naming.md` (lifted from
CLAUDE.md), ADR template at `docs/adrs/0000-template.md`.

Next: M0 step 2 — the cross-compiler toolchain script.

## Last completed milestone

None — M0 is the first.
