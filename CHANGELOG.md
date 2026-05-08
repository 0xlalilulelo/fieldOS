# Changelog

All notable changes to **Arsenal** are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Semantic
versioning applies once a public release is cut (M2 first public alpha;
v1.0 daily-driver maturity per `docs/plan/ARSENAL.md`).

The project was previously named **Field OS** and tracked changes through
M0 / M1 / M2 / M3 in the entries below. The pivot to Arsenal is recorded
in [`docs/adrs/0004-arsenal-pivot.md`](docs/adrs/0004-arsenal-pivot.md);
the Field OS work is preserved at the `field-os-v0.1` tag.

## [Unreleased]

### Changed

- **Project pivoted from Field OS to Arsenal** on 2026-05-08
  (ADR-0004). HolyC monolith → Rust monolith with capability-secured
  userspace, LinuxKPI driver inheritance, tri-modal app distribution
  (native Rust + Wasm components + POSIX subset). Field OS PoC at
  M3 step 6-5 preserved at the `field-os-v0.1` tag. Naming catalog
  and visual identity carry forward; the language and architecture
  do not.
- Documentation rewritten for Arsenal: CLAUDE.md, STATUS.md,
  README.md, docs/naming.md, docs/plan/ARSENAL.md (now canonical;
  Field OS phase docs archived to docs/plan/legacy/).

## [Field OS — archived at field-os-v0.1]

The entries below describe the Field OS arc (2026-04-29 through
2026-05-08, M0 through M3 step 6-5). They are preserved unchanged as
historical record. The code these entries describe is reachable via
`git checkout field-os-v0.1`.

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
- M1-D: `kmain` final order — `serial_init` → `gdt_init` →
  `idt_init` → `fb_init` → `fb_puts("Hello, Field\n")` →
  `serial_puts("Field OS: stage 1 reached\n")` → sentinel →
  halt. The stage indicator now lands AFTER all M1
  initialisation, so its presence on serial certifies every
  init step succeeded. (M1 step D)
- **M1 — Boot to Long Mode complete.** `make iso &&
  ci/qemu-smoke.sh` green; serial prints `Field OS: stage 1
  reached` then `FIELD_OS_BOOT_OK`; framebuffer shows
  `Hello, Field` in the TempleOS 8×8 font; QEMU
  `-d int,cpu_reset` log shows zero exception/interrupt
  deliveries from the kernel. Tag `M1-complete` on commit
  `c211cf8`.
- M2-A: bitmap physical-memory manager. New
  `kernel/mm/{pmm.h,pmm.c}`. One bit per 4 KiB frame; packed
  bitmap; two-finger cursor over uint64_t words using
  `__builtin_ctzll` for bit-scan. `pmm_init` walks the Limine
  memmap, sizes the bitmap from the highest USABLE address,
  bootstraps it inside the largest USABLE region (bitmap pages
  self-mark as used). `pmm_alloc_page` / `pmm_free_page` /
  `pmm_stats` / `pmm_print_stats` / `pmm_hhdm_offset` form the
  external API. `kernel/main.c` adds the
  `LIMINE_MEMMAP_REQUEST_ID` and `LIMINE_HHDM_REQUEST_ID`
  requests, calls `pmm_init` immediately after `idt_init`, and
  `pmm_print_stats` between `fb_puts` and the stage line.
  Verified under `-m 256M`: serial prints
  `Memory: 254 MiB free of 254 MiB total`. (M2 step A)
- M2-B: 4-level virtual memory manager. New
  `kernel/mm/{vmm.h,vmm.c}`. Page-table memory accessed through
  the Limine HHDM (no recursive mapping). API: `vmm_map`,
  `vmm_unmap`, `vmm_translate` over an arbitrary PML4;
  `vmm_new_address_space` clones entries 256..511 from the
  kernel master so user processes inherit kernel mappings;
  `vmm_kernel_pml4` exposes the master captured from CR3 at
  init. Walker allocates intermediate tables on demand (PMM
  pages zeroed and installed as `PRESENT|RW|USER`); `invlpg`
  fires on both map and unmap. Flags surface as
  `VMM_FLAG_PRESENT|RW|USER|GLOBAL|NOEXEC`. 4 KiB pages only
  in M2-B; huge pages may land later if profiling demands.
  `vmm_self_test()` runs every boot: 256K iterations of
  map/translate/unmap over 1 GiB of virtual addresses (16 TiB
  base), halts on failure. Verified: `OK (PMM retained 513
  pages = 2052 KiB for page tables)` — exact predicted overhead
  (1 PD + 512 PTs). Boot-to-sentinel <1 s on TCG. (M2 step B)
- M2-C: slab heap. New `kernel/mm/{slab.h,slab.c}`. Eight
  per-size caches at 16/32/64/128/256/512/1024/2048 bytes over
  4 KiB PMM pages with a 32-byte in-page header (magic 0x5A1B,
  cache_id, free_count, total_slots, first_free, prev/next).
  Slot freelist threaded as a uint16_t page offset in the first
  2 bytes of each free slot. Allocations >2 KiB take a
  contiguous-page large path with a 16-byte header (magic
  0x1A1B, page count) at offset 0 and payload at offset 16.
  `kmalloc(size)` / `kfree(ptr)` ptr-only at the C level
  (HolyC bindings deferred to M3); `kfree` masks to the 4 KiB
  page boundary, reads the magic, dispatches to slab-free or
  large-free; bad magic panics via `cli;hlt`. Fully-empty slab
  pages unlink from their cache list and return to the PMM —
  required by the no-leaks exit criterion. `pmm.{h,c}` extended
  with `pmm_alloc_pages(n)` (naive linear contiguous scan from
  page 0) and `pmm_free_pages(pa, n)`. `kernel/main.c` calls
  `slab_init()` after `vmm_init()` and `slab_self_test()` after
  `vmm_self_test()`. Self-test runs every boot: 10,000 LCG-
  sized allocations (1..4096 bytes), Fisher-Yates shuffle, free
  in shuffled order, assert PMM baseline. Verified: `Slab:
  10K random alloc/free... OK (no leaks)`. Boot-to-sentinel ~2 s
  on TCG. LOC: 2,014 / 100,000 (2%), 21 base-system files.
  (M2 step C)
- M2-D: M2 wrap. New `kernel/lib/{format.h,format.c}` with a
  single `format_dec(uint64_t)` that prints unsigned decimal to
  serial — no leading zeros, no separators, panic-path safe
  (single 21-byte stack buffer, no allocation, no globals).
  Replaces four near-identical helpers
  (`idt.c::put_dec`, `pmm.c`/`vmm.c`/`slab.c::serial_print_dec`)
  whose call sites all collapse to `format_dec`. `idt.c::put_hex64`
  stays put — single panic-path caller, moves the next time a
  non-panic caller needs hex. `kernel/main.c` bumps the stage
  marker from `stage 1 reached` to `stage 2 reached`, landing
  after every M2 init returns successfully so its appearance on
  serial certifies pmm/vmm/slab init plus the three self-tests.
  Net −34 LOC, +2 files. (M2 step D)
- **M2 — Memory Management complete.** `make iso &&
  ci/qemu-smoke.sh` green; serial prints
  `Memory: 254 MiB free of 254 MiB total`,
  `VMM: 1 GiB map/unmap... OK (PMM retained 513 pages = 2052
  KiB for page tables)`,
  `Slab: 10K random alloc/free... OK (no leaks)`,
  `Field OS: stage 2 reached`, then `FIELD_OS_BOOT_OK`.
  Boot-to-sentinel ~2 s on TCG. 1,980 LOC consumed of the
  100,000-line base-system budget (2 %), 23 files. Tag
  `M2-complete` on commit `6cd9855`.
