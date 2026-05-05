# STATUS

> What I am doing right now. Updated whenever the milestone changes,
> a major design decision lands, or a session leaves something
> mid-flight that future-me needs to know about.

## Current milestone

**M2 — Memory Management** *(in progress)*

Started: 2026-05-05

### M2 deliverables

- [x] Step A — PMM (bitmap, two-finger cursor) over Limine
  memmap + HHDM; free-RAM count printed on serial
- [ ] Step B — VMM (4-level walker, `vmm_map`/`vmm_unmap`/
  `vmm_translate`); `vmm_new_address_space` helper; 1 GiB
  map/unmap round-trip test
- [ ] Step C — Slab heap (per-size caches at 16/32/64/128/256/
  512/1024/2048, large-page fallback); 10,000 random-sized
  allocations with random-order frees, zero leaks
- [ ] Step D — M2 wrap; stage 2 reached; full M2 verification

### Exit criterion

10,000 random-sized allocations / random-order frees with zero
leaks; map-and-unmap of 1 GiB of test pages with `vmm_translate`
round-trips; smoke prints free-RAM count on serial.

## Active work

M2-A just landed: bitmap physical-memory manager bootstrapped over
the Limine memmap and HHDM responses. New `kernel/mm/{pmm.h,
pmm.c}`. `kernel/main.c` adds the Limine `MEMMAP` and `HHDM`
requests, calls `pmm_init()` between `idt_init` and `fb_init`, and
`pmm_print_stats()` between `fb_puts` and the stage indicator.

Implementation: one bit per 4 KiB frame, packed bitmap, two-finger
cursor over uint64_t words with `__builtin_ctzll` for bit-scan.
Bitmap is bootstrapped inside the largest USABLE region; its own
pages are marked used immediately. USABLE-only — bootloader-
reclaimable regions stay used to avoid stomping on Limine's
response data.

Verified under QEMU `-m 256M`: serial prints
`Memory: 254 MiB free of 254 MiB total` (the missing 2 MiB is
low-memory BIOS/BDA + framebuffer + Limine bookkeeping). The
allocator path is implemented but not yet stressed; M2-C's
10,000-random-alloc/free test is the principled exercise.

Next: M2-B — VMM. 4-level page-table walker
(`vmm_map`/`vmm_unmap`/`vmm_translate`), `vmm_new_address_space`
helper that clones the upper-half kernel mappings into a fresh
PML4, and a 1 GiB map/unmap round-trip test.

## Last completed milestone

**M1 — Boot to Long Mode** (2026-04-30 → 2026-05-04, four
commits, +700 LOC base-system). Boot path: GDT + TSS, IDT with
256 stubs and IST stacks for #DF/#NMI/#MC, software framebuffer
with TempleOS 8×8 font, "Hello, Field" rendered, zero
exceptions taken.

**M0 — Tooling and Bootstrap** (2026-04-29 → 2026-04-30, six
commits, ~190 LOC of base-system C, 21,000 LOC vendored at
arm's length under `vendor/limine/` and `holyc/`). Tag:
`M0-complete` on commit `60e1a48`.
