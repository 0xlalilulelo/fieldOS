# M0 step 3A — memory subsystem completion

*May 9, 2026. Two sessions. Four commits.*

3A is the first of seven sub-blocks in M0 step 3 (memory, scheduler,
virtio, network, framebuffer, SMP, `>` prompt). The exit criterion is
narrow: the kernel owns every level of the page-table tree, has a
free-able heap with a real allocator, has a frame allocator over
Limine's memory map, and has reclaimed every `BOOTLOADER_RECLAIMABLE`
page into the frame pool. After 3A, no kernel address resolves
through pages the bootloader allocated — Limine's hand-off state is
fully consumed, and the kernel can grow its page-table tree freely
for 3B's per-CPU + scheduler work.

## What landed

Four commits across two sessions:

- `2719e3f` *feat(kernel): frame allocator over Limine memory map.*
  Stack-of-frames `Vec<u64>` of free 4-KiB physical addresses, behind
  a `spin::Mutex` (revisited at SMP). `frames::init` walks
  `MemoryMapRequest::entries()`, excludes the heap byte-range
  reserved by `heap::init`, pushes every USABLE chunk. One alloc /
  free self-test at end of init catches a broken alloc path before
  3-2's deep clone bets the kernel on it.
- `3135ad6` *feat(kernel): deep-clone page tables, take ownership of
  all levels.* Recursive walk: PML4 → PDPT → PD → PT, allocating
  fresh frames at each non-leaf level. Leaves (PT entries, or
  HUGE_PAGE-flagged entries at PDPT/PD level) copy verbatim — they
  describe what is mapped, and the same physical pages stay mapped.
  Replaces the shallow PML4 copy from M0 step 2-4. After this commit,
  every page-table page the kernel walks lives in USABLE memory the
  kernel allocated.
- `f947d04` *feat(kernel): linked-list allocator with free path.*
  `LockedHeap` from `linked_list_allocator` (Phil Opp's blog
  allocator) replaces the bump from M0 step 2-1. `Box::drop` and
  `Vec::drop` actually return memory now. Tried `talc` 5.0.3 first
  per the 3A trade-off resolution; talc's 5.x split into
  `talc::base::Talc` + `talc::sync::TalcLock` + `talc::source::*`
  was more API surface than this commit warranted. talc revisits if
  fragmentation or perf bites.
- `df16d9f` *feat(kernel): reclaim BOOTLOADER_RECLAIMABLE into frame
  pool.* Second pass over the memory map; pushes every
  `BOOTLOADER_RECLAIMABLE` region onto the frame allocator's free
  list. Safe because 3-2's deep clone severed the kernel's last
  read through Limine's tables. New `ARSENAL_FRAMES_OK` sentinel
  reports total / free counts; smoke now asserts all three sentinels.

## How long it took

Two evening sessions on Apple Silicon, 2026-05-09. Maybe two hours
of active time across both. ARSENAL.md budgets 6–8 calendar months
for the *whole* of M0 step 3; 3A landing in two sessions is the
expected asymmetry — memory primitives are well-trodden ground,
while 3B (scheduler) and 3C (virtio) are where bugs hide deeply.

The fast read on 3A doesn't generalize. The next sub-block (3B)
ships per-CPU data and the first scheduler with yield points;
that's "subtle bugs that surface much later" territory.

## Detours worth recording

**Duplicate `MemoryMapRequest` static.** The first version of
`frames.rs` declared its own
`#[unsafe(link_section = ".requests")] static MEMMAP: MemoryMapRequest`,
intending to read the memory map directly. Two requests with the
same Limine ID are not standard; v12 fills exactly one of them and
silently drops the other, including any cross-references. The
visible failure mode was that `BASE_REVISION.is_supported()`
returned `false` after the duplicate landed, the boot path halted
before serial init, and *nothing* printed — not even
`ARSENAL_BOOT_OK`. About 30 minutes of "wait, why is serial empty
now?" before bisecting back to the duplicate. The fix is
canonical: keep the single request static in `main.rs` and pass
`entries()` through.

**`talc` 5.x's expanded API surface.** HANDOFF said "try talc
first." talc 4.x had a small `Talc::new(oom_handler).lock()` API
with `claim(span)` for runtime regions. talc 5.x reorganized into
`talc::base::Talc<Source, Binning>`, `talc::sync::TalcLock<R, S, B>`,
and a separate `talc::source::{Manual, Claim, GlobalAllocSource,
AllocatorSource}` module. Five generic parameters across three
types is more design surface than one heap region warrants.
Switched to `linked_list_allocator` per the 3A fallback
resolution. talc revisits when scheduler / virtio churn produces
real fragmentation evidence.

**Silent OOM during reclaim.** First attempt at 3-4 hung after
`ARSENAL_HEAP_OK` with no diagnostic. The kernel's frame-allocator
`Vec` had grown to ~64 K entries (~512 KiB) during 3-1's USABLE
push; the reclaim's BOOTLOADER_RECLAIMABLE push triggered a
`Vec::reserve` doubling that asked for ~1 MiB transiently — old
+ new buffers held simultaneously during the copy. The 1 MiB heap
from M0 step 2 OOM'd. `linked_list_allocator::alloc` returned
null, `handle_alloc_error` panicked, and our `panic_handler`
halted silently — the worst kind of failure mode for a kernel.
Bumped `HEAP_CAP` from 1 MiB to 16 MiB; reclaim succeeded
immediately. The deeper fix is to make the panic handler print
the panic info to serial; logged for a future session.

## The numbers

- **4 commits.** Each ships a self-contained 3A piece.
- **725 lines of Rust kernel code** in `arsenal-kernel/src/`. Up
  from 583 at the end of step 2. Net +142 LOC for the four 3A
  commits — almost entirely in `frames.rs` (94 lines) and
  `paging.rs` (deep-clone walk).
- **~37 KB ELF**, essentially unchanged from step 2. The bump
  allocator was tiny; the linked-list allocator's overhead is
  comparable. Most growth from 3A is in `.bss` (the IST stacks
  from step 2 still dominate) and a handful of static items the
  frame allocator brings in.
- **61287 frames added to the pool** on QEMU 256 MB
  (USABLE + BOOTLOADER_RECLAIMABLE, ~239 MiB), of which 222 came
  from the bootloader reclaim. **10 frames in use** at exit
  (the deep-clone tree: PML4 + a few PDPTs + a handful of PDs
  + PTs).
- **~1 second** local TCG smoke. **~45 seconds** end-to-end on
  `ubuntu-24.04` runner.

## What the boot looks like

The serial trace is now seven lines, the trace of the step-3A boot
sequence:

```
ARSENAL_BOOT_OK
mm: 2 usable regions; heap @ 0xffff800000100000 size 16384 KiB
EXCEPTION #BP at 0xffffffff80000b48
paging: deep-cloned cr3 -> 0x000000000ff50000 (all levels kernel-owned)
ARSENAL_HEAP_OK
frames: reclaimed 222 bootloader frames; 61277 free / 61287 total
ARSENAL_FRAMES_OK
```

The two new lines (`paging: deep-cloned ...` and `frames: reclaimed
...`) carry every load-bearing assertion 3A makes. The `mm:` line's
heap size visibly grew from 1024 KiB at step 2 to 16384 KiB at 3-4
(the OOM detour above).

## What 3B looks like

Per ARSENAL.md M0 the next sub-block: scheduler skeleton.
Sub-commit decomposition belongs at the 3B session start, but the
shape is roughly:

- **Per-CPU data structure.** Just one CPU at this stage; the
  shape is correct so SMP arrival in 3F is a population, not a
  refactor. `current_task`, `idle_task`, runqueue head pointer.
- **Task struct.** Stack pointer, register save area, scheduler
  state. Stacks come from the frame allocator (3A's payoff).
- **Cooperative yield.** No timer interrupts yet — tasks call
  `yield_now()` to switch. Context-switch in inline assembly
  saving/restoring callee-save GP registers (and the IST RSP
  slot in TSS).
- **Two-task ping-pong demo.** Spawn two tasks that print
  "ping" and "pong" alternately, then halt. New
  `ARSENAL_SCHED_OK` sentinel after the demo completes.

3B's bug-prone moment is the context switch — register clobbering
and stack-pointer mishandling are the two ways this fails first.
Expect 1–2 sessions of debugging if the first attempt has subtle
issues with the MGS3-warm-named TSS interaction.

## Cadence

This devlog is the first sub-block devlog under M0 step 3. The
question of whether to keep this granularity (per sub-block) or
collapse the seven sub-block devlogs into one step-3 wrap-up is
worth revisiting after 3B lands. The Asahi cadence stays the
model — calibrated, honest, never marketing.

—
