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

M0 step 2.5 just landed: top-level `Makefile` with
`toolchain-check`/`help`/`clean`/`distclean` targets; portable
`tools/qemu-run.sh` with HVF/KVM/TCG auto-select. `make
toolchain-check` is green: x86_64-elf-gcc 14.2.0, GNU ld 2.42.

Next: M0 step 3 — vendor Limine v9.x, write the ~50-line C kernel
that prints the boot sentinel, top-level `make iso` target,
`kernel/arch/x86_64` subsystem skeleton.

## Last completed milestone

None — M0 is the first.
