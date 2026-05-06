# STATUS

> What I am doing right now. Updated whenever the milestone changes,
> a major design decision lands, or a session leaves something
> mid-flight that future-me needs to know about.

## Current milestone

**M3 — HolyC Runtime on Bare Metal** *(not yet started; the
Big Risk milestone, est. 3–6 FT-weeks → ~7–14 calendar weeks
part-time)*

### Scope

A HolyC compiler running inside the kernel that can accept
source over serial, compile to native x86_64 in memory,
execute, and print the result. The tiniest possible REPL.
Bootstrap stays C; the C kernel calls `holyc_init()` once
memory and serial are up, then `holyc_repl()`. Do not boot
directly into HolyC (DWARF, incremental verification,
recovery, ABI clarity — see `docs/plan/phase-0.md §M3`).

### Exit criterion

`y = 6 * 7;` typed at the serial REPL prints `42`. The forked
`holyc-lang` backend emits position-independent x86_64 into a
JIT-allocated buffer; an ~20-function C → HolyC ABI surface
(`abi.h`) is documented and stable.

## Active work

**M2 — Memory Management — complete (2026-05-05 → 2026-05-06,
four commits, +1,814 LOC base-system).** The PMM landed first
as a packed bitmap with a uint64_t two-finger cursor over the
Limine memmap; the VMM followed as a 4-level walker over the
HHDM (no recursive mapping, 1 GiB self-test every boot
retaining 513 pages = 2052 KiB of page tables); the slab heap
closed with eight per-size caches plus a contiguous-page large
path and a 10,000 random-alloc / random-free no-leaks
self-test. M2-D consolidated four near-identical
`serial_print_dec` / `put_dec` helpers into
`kernel/lib/format.{h,c}::format_dec(uint64_t)` and bumped the
stage marker to `stage 2 reached`. Boot-to-sentinel ~2 s on
TCG. LOC: 1,980 / 100,000 (2 %), 23 base-system files.

**M3 preview.** First step is the JIT memory region: reserve
16 MiB of higher-half VA, back lazily, flip NX off only for
emitted pages via `vmm_remap` (no global W^X violation).
Second is the holyc-lang backend graft per
`docs/skills/holyc-lang-audit.md` — replace file-emit with
buffer-emit, resolve externs against a static ABI table.
Third is the smallest possible REPL on COM1. The audit's
six-step roadmap is the contract; deviations get an ADR.

## Last completed milestone

**M2 — Memory Management** (2026-05-05 → 2026-05-06, four
commits). Tag `M2-complete` on commit `6cd9855`.

**M1 — Boot to Long Mode** (2026-04-30 → 2026-05-04, four
commits, +700 LOC base-system). Boot path: GDT + TSS, IDT with
256 stubs and IST stacks for #DF/#NMI/#MC, software framebuffer
with TempleOS 8×8 font, "Hello, Field" rendered, zero
exceptions taken. Tag `M1-complete` on commit `c211cf8`.

**M0 — Tooling and Bootstrap** (2026-04-29 → 2026-04-30, six
commits, ~190 LOC of base-system C, 21,000 LOC vendored at
arm's length under `vendor/limine/` and `holyc/`). Tag
`M0-complete` on commit `60e1a48`.
