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
- [x] Step C — Slab heap (per-size caches at 16/32/64/128/256/
  512/1024/2048, large-page fallback); 10,000 random-sized
  allocations with random-order frees, zero leaks
- [ ] Step D — M2 wrap; stage 2 reached; full M2 verification

### Exit criterion

10,000 random-sized allocations / random-order frees with zero
leaks; map-and-unmap of 1 GiB of test pages with `vmm_translate`
round-trips; smoke prints free-RAM count on serial.

## Active work

M2-C just landed: slab heap with eight per-size caches (16, 32,
64, 128, 256, 512, 1024, 2048) over 4 KiB PMM pages plus a
contiguous-page large path. New `kernel/mm/{slab.h,slab.c}`.
`pmm.{h,c}` gain `pmm_alloc_pages(n)` (naive linear contiguous
scan from page 0) and `pmm_free_pages(pa, n)`. `kernel/main.c`
calls `slab_init()` after `vmm_init()` and `slab_self_test()`
after `vmm_self_test()`.

Slab page layout: 32-byte header at offset 0 (magic 0x5A1B,
cache_id, free_count, total_slots, first_free, prev/next), slots
packed after the header for sizes <= 512 or starting at offset
cache_size for the 1024 / 2048 caches. Each free slot's first 2
bytes thread the freelist as a uint16_t page offset. Large allocs
(>2 KiB) take a contiguous N-page run from the PMM with a 16-byte
header at offset 0 (magic 0x1A1B, page count); payload at offset
16. `kmalloc(size)` dispatches by size; `kfree(ptr)` masks to the
4 KiB page boundary, reads the magic, and routes to slab-free or
large-free. When a slab becomes fully empty it unlinks from its
cache list and returns its page to the PMM — without that the
no-leaks self-test fails.

Self-test runs every boot: 10,000 LCG-sized allocations
(1..4096 bytes), each writes its low byte to offset 0 to sanity-
check overlap, then a Fisher-Yates shuffle and free-in-shuffled-
order, then assert PMM free-page count returned to baseline.
Verified output:

  Memory: 254 MiB free of 254 MiB total
  VMM: 1 GiB map/unmap... OK (PMM retained 513 pages = 2052 KiB
       for page tables)
  Slab: 10K random alloc/free... OK (no leaks)
  Field OS: stage 1 reached
  FIELD_OS_BOOT_OK

Boot to sentinel completes in ~2 s on TCG. LOC: 2,014 / 100,000
(2%), 21 base-system files.

Decisions taken in M2-C: kmalloc/kfree are ptr-only at the C
level (HolyC bindings deferred to M3); slab pages return to the
PMM when fully empty; naive linear contiguous-page scan for
large allocs (no buddy yet); bad-pointer kfree panics via
cli;hlt with no graceful recovery.

Next: M2-D — final M2 wrap. Print "stage 2 reached" before the
sentinel, tag `M2-complete`, optionally consolidate the
`serial_print_dec` triplicates (idt.c::put_dec, pmm.c, vmm.c,
now slab.c too) into a shared `kernel/lib/format.{h,c}`.

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
