# M0 step 2 — paging, GDT, IDT

*May 9, 2026. Two sessions. Five commits + a toolchain pin.*

Step 2 of M0 was the milestone where the kernel stopped trusting
Limine's hand-off state and built its own. After step 1, every
architectural primitive — page tables, segment registers, interrupt
descriptors, the allocator — was Limine's; the kernel was a guest in
its own boot. After step 2, all four are kernel-owned, and a regression
in any of them trips the smoke. The exit criterion: a heap round trip
runs *after* the kernel's PML4 takes CR3, and `ARSENAL_HEAP_OK` fires
only on success. That criterion is now met.

This is M0 step 2 of ARSENAL.md's M0 milestone. Step 3 — toward the
`>` prompt — is the bulk of M0: deep-clone page tables, real frame
allocator, linked-list (or buddy) allocator with a free path, basic
scheduler, virtio drivers, smoltcp + rustls, framebuffer console,
basic SMP. Per ARSENAL.md, ~6–8 calendar months of part-time work.

## What landed

Five substantive commits plus the nightly toolchain pin, in order:

- `f2663b5` *feat(kernel): ingest Limine memory map + bump allocator.*
  Parse `MemoryMapRequest` and `HhdmRequest` responses, pick the
  largest USABLE region, install a `#[global_allocator]` backed by
  a non-CAS bump allocator over the first 1 MiB. SMP arrival in M0
  step 5+ converts the bump to a `fetch_update` CAS loop. No free
  path — replace with linked-list / buddy when subsystems with
  churn (scheduler, virtio, sockets) start needing it.
- `ca6a390` *feat(kernel): GDT + TSS with IST stack reservations.*
  Replace Limine's GDT with kernel CS/DS + TSS. Reserve three
  20-KiB IST stacks for #DF, #NMI, #MC — the faults that cannot
  share the kernel's normal stack. `spin = "0.10"` for `Lazy<T>`
  globals; `x86_64 = "0.15"` (default-features off, `instructions`
  on) for the structs.
- `8bfa5f2` *chore: pin nightly Rust toolchain for x86-interrupt
  ABI.* The `extern "x86-interrupt" fn` calling convention required
  by `InterruptDescriptorTable::set_handler_fn` is gated behind
  the `abi_x86_interrupt` nightly feature. `rust-toolchain.toml`
  flips to `nightly-2026-04-01`. The HANDOFF predicted this moment
  ("switching later is a one-line file change") and step 2-3 was
  the natural inflection.
- `556bcd2` *feat(kernel): IDT with stub handlers, IST routing for
  #DF/#NMI/#MC.* Per-exception `extern "x86-interrupt"` handlers
  for #DE, #UD, #BP, #GP, #PF, #DF, #NMI, #MC. Each prints a one-
  line diagnostic via `writeln!(serial::Writer, ...)` and halts
  (#BP returns, so `int3` works as a recoverable trap).
  `#![feature(abi_x86_interrupt)]` in the kernel crate. An `int3`
  self-test in `_start` confirms the IDT routes cleanly.
- `9c38083` *feat(kernel): 4-level paging, take ownership of CR3.*
  Allocate a fresh 4-KiB PML4 from the heap, `copy_nonoverlapping`
  Limine's PML4 contents into it, write CR3. Lower-level tables
  remain Limine's (in `BOOTLOADER_RECLAIMABLE` memory) — a future
  step deep-clones when we actually want to reclaim that physical
  RAM. The shallow clone is deliberate: the smallest correct change
  that puts the top-level table under kernel control.
- `8cd0186` *test(smoke): require ARSENAL_HEAP_OK sentinel after
  paging.* `heap_round_trip()` does Box<u32> + Vec<u32> after CR3
  swap; `ARSENAL_HEAP_OK` prints only on success. `ci/qemu-smoke.sh`
  grows a `REQUIRED_SENTINELS` array and now asserts both sentinels
  appear before declaring PASS. A future paging regression that
  breaks HHDM after CR3 swap would page-fault on `Box::new`,
  print `#PF cr2=...`, and fail smoke red.

## How long it took

Two evening sessions on Apple Silicon, both 2026-05-09. On the order
of three hours of active time. ARSENAL.md calibrated step 2 as 2–4
FT-weeks (~5–10 calendar weeks part-time). It landed in a single
calendar day, mostly because the bug-prone step (paging) didn't
actually bite — see the detours below.

The fast-than-calibrated read isn't a forecast about step 3. Step 3
is the bulk of M0: the scheduler, virtio drivers, smoltcp, framebuffer
console, SMP — work where the bugs hide deeper than in the well-trodden
GDT/IDT/paging primitives. ARSENAL.md's calendar estimate for the rest
of M0 (~6–8 months) is the calibration to trust.

## Detours worth recording

**The "bug-prone" paging step wasn't bug-prone.** ARSENAL.md M0's
warning about subtle bugs that surface much later applies to deep
page-table operations — building tables from scratch, walking them,
mutating leaf entries. The shallow-clone strategy (allocate one fresh
PML4, `copy_nonoverlapping` Limine's verbatim, write CR3) sidesteps
all of that. Every lower-level table is still Limine's; every
transitive mapping resolves identically; the kernel just owns the
top-level page. The bug-prone work is *deferred*, not avoided — when
step 3 (or wherever) needs to reclaim `BOOTLOADER_RECLAIMABLE` memory,
the deep clone has to happen and the calibration applies in full.
About ten minutes of sketching, ~50 LOC, first try worked.

**The `static mut` lint dance.** The IST stacks in `gdt.rs` are
big-block static buffers (3 × 20 KiB) the CPU writes to during
exception delivery. In Rust 2024 `&mut STATIC_MUT_VAR` triggers a
warn-by-default `static_mut_refs` lint; the modern pattern is the
`&raw mut` operator (stable since 1.82). The fix was syntactic but
worth recording — every kernel will hit this exact shape (statically-
allocated hardware-touched buffers) and the pattern is `static mut`
+ `&raw mut` + a SAFETY comment about the absent Rust-side aliasing.

**Nightly toolchain — when "stable suffices" stopped sufficing.**
The HANDOFF anticipated this in step 1's "Trade-off pairs" section:
stable Rust was sufficient for boot+serial+hlt, and probably
sufficient for GDT (which step 2-2 confirmed — x86_64 crate's
`instructions` feature works on stable). The IDT, on the other
hand, requires `extern "x86-interrupt" fn` for handler signatures,
and that ABI is nightly-only. We pinned `nightly-2026-04-01`. The
toolchain commit is its own concern, separated from the IDT work
that motivates it, so a future bisect on a different question can
cleanly identify it.

## The numbers

- **6 commits** including the toolchain pin. Each is a self-contained
  step-2 piece. Bisect surface clean.
- **583 lines of Rust kernel code** in `arsenal-kernel/src/`. Up from
  ~140 at step 1. Breakdown: `main.rs` 151, `idt.rs` 124, `serial.rs`
  92, `gdt.rs` 86, `heap.rs` 70, `paging.rs` 60.
- **36 KB ELF**, up from 9 KB at step 1. Most of the growth is
  `.bss` (60 KB before strip — the three IST stacks live there) and
  `.text` (~13 KB of code, bumped by the IDT handler bodies that
  pretty-print to serial via `core::fmt`).
- **17.9 MB ISO**, essentially unchanged from step 1 — the kernel
  ELF is dwarfed by the El Torito UEFI image.
- **~1 second** wall time from QEMU launch to `ARSENAL_HEAP_OK` on
  serial under headless TCG locally. CI on `ubuntu-24.04` runs the
  full pipeline (apt → rustup → clippy → xtask iso → smoke) in ~45
  seconds.

## What the boot looks like

The serial output is now five lines, the trace of the step-2 boot
sequence:

```
ARSENAL_BOOT_OK
mm: 2 usable regions; heap @ 0xffff800000100000 size 1024 KiB
EXCEPTION #BP at 0xffffffff80000c65
paging: cr3 -> 0x0000000000100000 (kernel-owned PML4, shallow clone)
ARSENAL_HEAP_OK
```

Each line is a step 2 sub-system asserting its own correctness:

- `ARSENAL_BOOT_OK`: COM1 init still works (step 1).
- `mm: ...`: Limine's memory map parsed; bump allocator armed
  against the largest USABLE region; the heap landed at the
  HHDM-translated address shown.
- `EXCEPTION #BP at ...`: the IDT routed an `int3` to the breakpoint
  handler, which printed and returned to `_start`. A mis-loaded
  IDT would have triple-faulted here.
- `paging: cr3 -> ...`: `Cr3::write` succeeded with the kernel's own
  PML4. The fact that the *next* instruction (the `writeln!`)
  executed is the load-bearing proof that the shallow clone
  preserved every mapping the running code needed.
- `ARSENAL_HEAP_OK`: a `Box<u32>` and a `Vec<u32>` round-tripped
  through the global allocator after the CR3 swap. The HHDM
  mapping survived the page-table swap; the heap is reachable
  through the kernel-owned tables.

## What M0 step 3 looks like

Per ARSENAL.md M0 the remaining bullets, ordered roughly by
prerequisite chain:

- **Deep-clone page tables.** Walk Limine's PML4 → PDPT → PD → PT,
  allocate fresh frames at each level, copy entries down. After
  this, `BOOTLOADER_RECLAIMABLE` physical RAM is reusable.
- **Real frame allocator.** Stack-of-frames or bitmap over the
  memory map. Replaces "allocate 4 KiB pages from the heap" with
  "allocate them from the frame pool."
- **Linked-list (or buddy) allocator.** Adds a free path. Replace
  the bump allocator's no-op `dealloc` with real free-list
  insertion. The `linked_list_allocator` crate is the first thing
  to try; buddy if fragmentation surfaces.
- **Basic scheduler.** Cooperative at first. Per-CPU current task,
  yield-points at I/O boundaries.
- **Virtio block + virtio-net.** The minimal driver stack the
  scheduler can demonstrate I/O against.
- **smoltcp + rustls.** Network stack. The `>` prompt is keyboard-
  driven via serial first; this is the prerequisite for the
  hardware-summary bullet ARSENAL.md flags as the usability gate.
- **Framebuffer console.** Limine's `FramebufferRequest` exposes
  a typed framebuffer; we render an 8×8 bitmap font onto it. The
  Stage compositor (M2) is the eventual replacement.
- **Basic SMP.** The Limine `MpRequest` enumerates other cores;
  we boot them, give each a per-CPU stack and a copy of the GDT/
  IDT, and the cooperative scheduler becomes preemptive (or at
  least multi-core).
- **Boot to a `>` prompt.** All the above land into a serial-
  driven shell that reports hardware and waits for input.

ARSENAL.md's M0 calendar estimate is 9 months total. After 1 day of
step 1 + 1 day of step 2, ~9 months remain — the asymmetry is the
shape of this work. Step 3 sub-step decomposition gets a fresh
HANDOFF kickoff at the next session start.

## Cadence

This devlog is the once-a-step artifact for M0. Step 3's wrap will
get its own. Bi-weekly progress notes between sub-steps if anything
notable surfaces. The pattern remains the Asahi blog cadence — the
prior art that fits Arsenal's solo, part-time shape.

—
