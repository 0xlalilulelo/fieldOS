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
- [x] Step B — VMM (4-level walker, `vmm_map`/`vmm_unmap`/
  `vmm_translate`); `vmm_new_address_space` helper; 1 GiB
  map/unmap self-test runs every boot
- [ ] Step C — Slab heap (per-size caches at 16/32/64/128/256/
  512/1024/2048, large-page fallback); 10,000 random-sized
  allocations with random-order frees, zero leaks
- [ ] Step D — M2 wrap; stage 2 reached; full M2 verification

### Exit criterion

10,000 random-sized allocations / random-order frees with zero
leaks; map-and-unmap of 1 GiB of test pages with `vmm_translate`
round-trips; smoke prints free-RAM count on serial.

## Active work

M2-B just landed: 4-level page-table walker over Limine's HHDM,
no recursive mapping. New `kernel/mm/{vmm.h,vmm.c}`. `kernel/main.c`
calls `vmm_init()` after `pmm_init()` to capture the kernel master
PML4 from CR3, and `vmm_self_test()` after `pmm_print_stats()`.

API: `vmm_map`/`vmm_unmap`/`vmm_translate` for the universal
walker; `vmm_new_address_space()` returns a fresh PML4 with
entries 256..511 cloned from the master so user processes inherit
the kernel mapping; `vmm_kernel_pml4()` exposes the master.

The self-test runs every boot: 256K iterations of map, translate,
unmap covering 1 GiB of virtual address space, halts on any
failure so smoke catches regressions. Verified output:

  Memory: 254 MiB free of 254 MiB total
  VMM: 1 GiB map/unmap... OK (PMM retained 513 pages = 2052 KiB
       for page tables)
  Field OS: stage 1 reached
  FIELD_OS_BOOT_OK

The 513 pages retained match the predicted overhead exactly:
1 PD page + 512 PT pages for a contiguous 1 GiB span. Boot to
sentinel completes within 1 second on TCG.

Decisions taken in M2-B: 4 KiB pages only (no huge pages yet),
intermediate tables not torn down on unmap, `invlpg` after both
map and unmap, NX bit honored when `VMM_FLAG_NOEXEC` is passed,
single-CPU (no locks; SMP-safety lands at M11).

Next: M2-C — slab heap. Per-size caches at 16/32/64/128/256/512/
1024/2048 bytes; large allocations (>2 KiB) fall through to a
buddy/large-page path. `kmalloc(size)` / `kfree(ptr)`. 10,000
random-sized allocations / random-order frees / zero leaks as
the exit criterion. M2-D: stage 2 reached + final wrap.

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
