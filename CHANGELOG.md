# Changelog

All notable changes to Field OS are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Semantic
versioning applies once a public release is cut (v0.1 at the end of
Phase 1).

## [Unreleased]

### Added

- Repository scaffolding: directory tree, BSD-2-Clause license,
  README, naming catalog at `docs/naming.md`, ADR template at
  `docs/adrs/0000-template.md`. (M0 step 1)
- Cross-compiler toolchain build script
  (`tools/build-toolchain.sh`) with pinned binutils 2.42 and
  gcc 14.2.0 + SHA-256 hashes; companion CI fetch-script stub
  (`tools/fetch-toolchain.sh`); shared pin file
  (`tools/toolchain.mk`). (M0 step 2)
- Top-level `Makefile` (`toolchain-check`, `help`, `clean`,
  `distclean`); portable `tools/qemu-run.sh` launcher with
  HVF/KVM/TCG auto-select. (M0 step 2.5)
- Limine v12.0.2 vendored at `vendor/limine/` (BSD-2 binaries,
  0BSD header), trimmed to x86_64. ~120 lines of C kernel
  (`kernel/main.c`, `kernel/arch/x86_64/{io.h,serial.h,serial.c,
  linker.ld}`, `kernel/kernel.mk`), `boot/limine.conf`, and a
  rewritten top-level `Makefile` `iso` target. Kernel boots to
  long mode via Limine, initializes COM1 (16550 @ 115200 8N1),
  prints `Field OS: stage 0 reached` then `FIELD_OS_BOOT_OK`,
  and halts under cli/hlt. Includes a portability fix to
  `tools/qemu-run.sh` so x86_64 guests on Apple Silicon
  correctly fall through to TCG (HVF requires host arch ==
  guest arch). (M0 step 3)
- CI smoke loop: `ci/qemu-smoke.sh` (headless QEMU + serial
  grep, distinct exit codes for failure modes),
  `tools/count-loc.sh` (reports base-system LOC vs the
  100,000-line budget; warn at 90%, hard fail at 95%),
  `.github/workflows/ci.yml` with `build-iso` (toolchain
  cached on `tools/toolchain.mk` hash), `smoke` (boot the
  artifact and grep), and `loc-budget`. `reproducibility`
  job deferred to M10. (M0 step 4)
