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
- `holyc-lang` beta-v0.0.10 vendored at `holyc/` (BSD-2,
  21,497 LOC across 42 src/ files); audit at
  `docs/skills/holyc-lang-audit.md` covering architecture,
  libc/host-assumption surface, ABI, and a six-step roadmap
  for the M3 freestanding-backend graft. Recon only — the
  graft itself is M3 work. (M0 step 5)
- **M0 — Tooling and Bootstrap complete.** `make iso &&
  ci/qemu-smoke.sh field-os-poc.iso` boots through Limine
  v12.0.2, prints `Field OS: stage 0 reached` then
  `FIELD_OS_BOOT_OK` on COM1 serial, halts cleanly. 166 LOC
  consumed of the 100,000-line base-system budget. Tag
  `M0-complete` on commit `60e1a48`.
- M1-A: x86_64 GDT (5 entries — null + kernel code/data + user
  code/data) plus a 16-byte TSS descriptor with empty IST
  entries (M1-B fills them). `gdt_init()` loads the GDT via
  `lgdt`, reloads CS through a far-return trick, reloads
  SS/DS/ES/FS/GS to the kernel data selector, and runs `ltr`
  on the TSS selector. New: `kernel/arch/x86_64/{gdt.h,gdt.c,
  gdt_load.S}`. `kernel/kernel.mk` learns to compile `.S`
  sources. (M1 step A)
- M1-B: 256-entry IDT with per-vector asm stubs (synthetic zero
  error code pushed for vectors where the CPU does not),
  common `isr_common` dispatcher saving all 15 GPRs into a
  `struct regs`, C `exception_handler` printing
  `PANIC: vec=N err=0x... rip=0x... rsp=0x...` on serial, and
  4 KiB IST stacks for #DF (IST1) / #NMI (IST2) / #MC (IST3).
  New: `kernel/arch/x86_64/{idt.h,idt.c,exceptions.S}`.
  `gdt.h/.c` gain `gdt_set_ist(index, top)` so `idt.c` can
  populate `tss.ist[]` without exposing the TSS globally.
  Fault path verified by a temporary `ud2` after `idt_init`
  yielding `PANIC: vec=6 err=0x0 rip=0xffffffff80001... rsp=...`
  on serial; the `ud2` was removed before the commit. (M1 step B)
- M1-C: Limine framebuffer wired in. New
  `kernel/arch/x86_64/{framebuffer.h,framebuffer.c}` — `fb_init`
  reads the Limine response, captures pointer / pitch /
  dimensions, clears to black; `fb_putc_at` / `fb_puts` blit the
  TempleOS/ZealOS 8×8 console font vendored at
  `kernel/arch/x86_64/{font_8x8.h,font_8x8.c}` (Unlicense /
  public domain — origin recorded per CLAUDE.md hard
  constraint #6). `kernel/main.c` adds the framebuffer request,
  initialises the framebuffer, and prints `Hello, Field` after
  `idt_init`. Glyph shapes verified by QEMU monitor screendump
  showing recognisable H-e-l-l-o-,- -F-i-e-l-d in the top-left
  8×96 pixel region of the 1280×800 framebuffer. (M1 step C)
