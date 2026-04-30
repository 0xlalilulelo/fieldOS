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

M0 step 3 just landed. Vendored Limine v12.0.2 (binaries BSD-2,
header 0BSD; trimmed to x86_64). Wrote ~120 lines of kernel C
across `kernel/main.c`, `kernel/arch/x86_64/{io.h,serial.h,serial.c,
linker.ld}`, `kernel/kernel.mk`, `boot/limine.conf`. Top-level
`make iso` builds `field-os-poc.iso` (17.8 MB). Headless QEMU TCG
prints `Field OS: stage 0 reached` and `FIELD_OS_BOOT_OK` on COM1.

Discovered along the way: HVF can't virtualize x86_64 guests on
Apple Silicon (host/guest arch must match). `tools/qemu-run.sh`
now picks TCG on Apple Silicon, HVF on Intel macOS, KVM on Linux.

Next: M0 step 4 — CI smoke loop. `ci/qemu-smoke.sh` (headless
variant of qemu-run.sh) + `tools/count-loc.sh` + a four-job
`.github/workflows/ci.yml`: build-iso, smoke, loc-budget,
reproducibility.

## Last completed milestone

None — M0 is the first.
