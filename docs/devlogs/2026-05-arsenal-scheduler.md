# M0 step 3B — scheduler skeleton

*May 9, 2026. Three sessions. Six commits.*

3B is the second of seven sub-blocks in M0 step 3 (memory, scheduler,
virtio, network, framebuffer, SMP, `>` prompt). The exit criterion is
narrow: a cooperative single-CPU scheduler with at least two task
flows that round-robin through `yield_now` and a `switch_to` asm
primitive whose save area is stable enough that the next sub-block
can build on it without re-deriving register conventions. After 3B,
the kernel runs on its own heap-allocated stack, spawned tasks
exchange control voluntarily, and `ARSENAL_SCHED_OK` joins
`ARSENAL_BOOT_OK` / `ARSENAL_HEAP_OK` / `ARSENAL_FRAMES_OK` in the
smoke's required-sentinel list.

## What landed

Six commits across three sessions:

- `da3627e` *feat(kernel): panic handler prints to serial before
  halt.* Pre-3B hardening. The 3A reclaim OOM cost an hour to
  diagnose under a silent `hlt` loop; the 3B context-switch work is
  the next place a silent panic could hide. `PanicInfo`'s `Display`
  impl carries the message and (when known) the location, so a
  single `writeln!` is the entire diagnostic. Prefix is
  `ARSENAL_PANIC ` — disjoint from the sentinel namespace,
  greppable in CI logs.
- `7795073` *feat(kernel): per-CPU data area.* `[CpuLocal; 64]`
  static array indexed by CPU id, `current_cpu()` returns
  `&CPUS[0]` for now. SMP arrival in 3F replaces the body with a
  GS-relative fetch backed by `SWAPGS`. 3B-1 lit up just the `id`
  field; the `current` / `idle` / `runqueue` fields the HANDOFF
  spec named were deliberately deferred — adding them required a
  forward-declared empty `Task` type that 3B-2 would have
  rewritten, and the bisect cost of folding them in was real.
- `b2c748c` *feat(kernel): task struct + 16 KiB kernel stacks.*
  `Task` owns a saved RSP, a state enum (`Ready` for now;
  `Running`/`Blocked`/`Exited` carry `#[allow(dead_code)]` until
  the scheduler wires them), an entry function pointer, and a
  `Box<KernelStack>` carved from the linked-list heap.
  `Task::new(entry)` lays down a synthetic stack frame whose
  layout is the load-bearing contract with the asm in 3B-3 — six
  callee-save reg slots in pop order plus a return-address slot
  plus 8 bytes of alignment padding. The slot containing `entry`
  is 16-byte aligned so that `ret` lands at entry's first
  instruction with `RSP+8` 16-aligned per SysV.
- `7eadc79` *feat(kernel): cooperative context switch via
  global_asm.* The 17-line asm primitive: `push rbx, rbp,
  r12-r15`, store RSP into `*prev`, load RSP from `next`, pop in
  reverse, `ret`. Both directions of every switch run through this
  same body; the asm makes no distinction between fresh tasks
  (Task::new's synthetic frame, 16-aligned saved_rsp) and
  suspended tasks (8-aligned saved_rsp from the call instruction's
  pushed return) because the layout above saved_rsp is identical.
  Self-test in `sched::switch_test()` builds one task, switches
  into it, that task switches back, main resumes. The
  three-line round-trip serial trace appeared on the first boot —
  the 30-line block comment over the asm paid off as authoritative
  reference, and there was no retry.
- `46b005f` *feat(kernel): scheduler init, spawn idle task.*
  `CpuLocal` grew the deferred fields from 3B-1 — `current`
  (`AtomicPtr<Task>`), `idle` (same), `runqueue`
  (`Mutex<VecDeque<Box<Task>>>`). `sched::yield_now` does the
  round-robin: pop runqueue front, `AtomicPtr::swap` that into
  current, push the swapped-out previous to the back, switch_to.
  The swap closes the brief window between "decided next" and
  "made next visible" that preemptive 3F will care about; today
  it's formally redundant but the shape costs nothing.
  `sched::init` builds the idle task, installs it as current, and
  switches into it from `_start`'s Limine boot stack via
  `switch_to` with a throwaway `u64` for the outgoing RSP. After
  this commit, the kernel is no longer running on Limine's stack.
- `1264c20` *test(smoke): ARSENAL_SCHED_OK after ping-pong demo.*
  Two cooperative tasks, ping and pong, each yield three times
  then hand off to a shared `finish()`. The last task to reach
  `finish()` prints `ARSENAL_SCHED_OK` via a `fetch_add` barrier;
  others enter a trailing yield-loop. Idle is in the rotation but
  silent after its initial entry. Smoke's `FINAL_SENTINEL` flips
  to `ARSENAL_SCHED_OK`; the four required sentinels are now the
  smoke's pass condition.

## How long it took

Three evening sessions on Apple Silicon, all on 2026-05-09 — the
same day 3A wrapped. Maybe four hours of active time across the
three sessions. ARSENAL.md budgets months for the *whole* of M0
step 3, and 3B compressing into a single calendar day was not
expected. Two factors:

The HANDOFF for 3B-3 had explicit register-by-register documentation
of the save-area layout — push order, pop order, alignment math.
The asm landed first try as a result. The HANDOFF estimated one
retry on the asm save area; that estimate would have held without
the upfront documentation.

The 3A→3B pivot day kept context warm. Re-loading the build / smoke
loop, the Limine + `x86_64` crate idiom, and the trace shape from
3A would have cost a session by itself; doing 3B in the same window
absorbed that cost into productive work.

The fast read on 3B doesn't generalize either. 3C (virtio) involves
PCI scanning, MMIO BARs, virtqueue setup, descriptor rings —
hardware-spec ground that's slower than register-level asm because
the spec is denser and the failure modes are quieter (a mis-set
feature bit can present as silent stalls).

## Detours worth recording

**The 3B-1 / 3B-2 chicken-and-egg.** The HANDOFF spec for 3B-1
named `current`, `idle`, and `runqueue` as `CpuLocal` fields. But
`Task` is the type those pointers point at, and `Task` lands in
3B-2. Threading `Task` through 3B-1 would have meant a
forward-declared empty struct that 3B-2 then rewrites, or a
`*mut ()` placeholder fixed up later. The actual resolution: 3B-1
landed only the `id` field, and 3B-4 (scheduler init) was where
all three deferred fields needed to exist anyway. The cost was
"3B-1's commit message has a deviation paragraph"; the bisect
benefit was real (when a 3B-3 context-switch bug shows up, "did
3B-1 misshape per-CPU storage" is trivially answerable against an
`id`-only struct).

**The State enum dead-code allow.** The HANDOFF spec named
`State::Ready / Running / Blocked / Exited`, but only `Ready` is
constructed in 3B-2 (Task::new). The other variants take
`#[allow(dead_code)]` until 3B-4 / 3B-5 wires state transitions.
CLAUDE.md disfavors speculative shape; the resolution was to
document the wiring milestone in the comment rather than build the
variants on-demand, because re-opening the enum mid-step would have
been more code than the allow attribute.

**Destructive task exit deferred.** The HANDOFF described 3B-5 as
"both tasks return; scheduler sees no runnable tasks, prints
`ARSENAL_SCHED_OK`, halts." Task return needs a deferred-reaping
path — free the dying task's stack only after the switch is off
it — which is real surface area (about 50 LOC of ownership-juggling)
that 3B doesn't need yet. The shipped form is functionally
equivalent from the smoke's perspective: workers loop-yield after
their N rounds, the last finisher prints the sentinel via
`AtomicUsize::fetch_add`. 3B-7 or 3C wires real exit when virtio
bring-up needs it.

**Idle's `hlt` was wrong for the cooperative case.** 3B-4 had the
idle loop call `yield_now` then `hlt`. That works when the runqueue
is empty (no work, halt forever, smoke times out). It breaks the
moment 3B-5 spawns ping/pong: idle yields out, gets scheduled back
in, hlts — and there are no IRQs to wake it. Cooperative-no-IRQ
plus hlt equals stuck CPU. 3B-5 removed the hlt; 3F's preemptive
LAPIC timer brings it back as a real power-save.

**The asm round-trip worked first try.** Worth recording because the
HANDOFF flagged 3B-3 as the single bug-prone surface in 3B and
budgeted one retry. The save-area documentation in
[`task.rs:55-78`](../../arsenal-kernel/src/task.rs) and at the top of
[`sched.rs`](../../arsenal-kernel/src/sched.rs) — six callee-save
slots in pop order, alignment math worked out for both fresh and
suspended tasks — was redundant against a careful manual derivation,
but redundancy is exactly what catches the kind of off-by-one that
silently corrupts r15. The cost of writing it (15 minutes) was a
fraction of the cost of debugging a wrong push order (HANDOFF's
estimate of one retry was probably an under-count for less
documented attempts).

## The numbers

- **6 commits.** Three pairs landed across three sessions —
  hardening + scaffolding (3B-0 / 3B-1 / 3B-2), the asm primitive
  (3B-3), wiring + demo (3B-4 / 3B-5).
- **1252 lines of Rust kernel code** in `arsenal-kernel/src/`, up
  from 725 at the end of 3A. Net +527 LOC. The three new modules:
  `cpu.rs` (70), `task.rs` (112), `sched.rs` (261). `main.rs` grew
  by ~85 lines for the demo + smokes; everything else unchanged.
- **~47 KB ELF**, up from ~37 KB at the end of 3A. The asm body
  itself is ~70 bytes; the bulk of the growth is Rust code for
  yield_now's runqueue manipulation, the switch test, and the
  ping-pong demo.
- **~1 second** local TCG smoke. Four sentinels: `ARSENAL_BOOT_OK`,
  `ARSENAL_HEAP_OK`, `ARSENAL_FRAMES_OK`, `ARSENAL_SCHED_OK`.
- **3 ping × 3 pong × 6 yields** under the demo round-robin. Idle
  takes turns silently between the workers; visible serial output
  shows pure alternation.

## What the boot looks like

The serial trace is now sixteen lines, ending at the new sentinel:

```
ARSENAL_BOOT_OK
mm: 2 usable regions; heap @ 0xffff800000100000 size 16384 KiB
EXCEPTION #BP at 0xffffffff800024f1
paging: deep-cloned cr3 -> 0x000000000ff48000 (all levels kernel-owned)
ARSENAL_HEAP_OK
frames: reclaimed 226 bootloader frames; 61273 free / 61283 total
ARSENAL_FRAMES_OK
cpu: id=0 (single-CPU stage)
task: built (entry=0xffffffff80002ae0, saved_rsp=0xffff800000103fc0, state=Ready, stack=16 KiB)
sched: switching to test task...
sched: switched INTO test task
sched: returned to main
sched: init complete; switching to idle
sched: idle running
ping
pong
ping
pong
ping
pong
ARSENAL_SCHED_OK
```

The three `sched: ... test task` lines are 3B-3's switch_to round-trip
self-test; the two `sched: init complete` / `sched: idle running`
lines mark the threshold where the kernel left Limine's boot stack
behind; the six `ping` / `pong` lines are the workers cooperatively
rotating; the final `ARSENAL_SCHED_OK` is `pong` (the second of two
to finish) calling `fetch_add` and seeing the final count match.

## What 3C looks like

Per ARSENAL.md M0, the next sub-block: virtio bring-up.

- **PCI bus scan.** Walk function 0 of every (bus, device) until we
  find vendor 0x1AF4, the virtio common ID. Probe the BAR layout to
  pull MMIO addresses; ignore PIO transport for now (modern QEMU
  defaults to MMIO).
- **virtqueue setup.** Three rings per queue (descriptor, available,
  used), 4-KiB-aligned, frame-allocator-backed. The descriptor ring
  is fixed-size; available + used are co-located.
- **virtio-blk.** Read blocks from the boot ISO via the standard
  request format. The smoke target is "read sector 0, see its
  magic match `0xAA55`". This is the first kernel-side I/O outside
  serial.
- **virtio-net.** Receive and transmit Ethernet frames on the QEMU
  user-mode network. Pairs with smoltcp in 3D.

The bug-prone moment in 3C is feature negotiation. virtio's modern
spec has dozens of feature bits; getting one wrong (especially
`VIRTIO_F_VERSION_1`) presents as silent stalls or unprocessed
descriptors. Expect 1–2 sessions of bisecting against `qemu-system`
trace output if the first attempt gets feature negotiation subtly
off.

## Cadence

This is the second sub-block devlog of M0 step 3 (3A was the first).
The 3A devlog flagged the cadence question — per-sub-block devlogs
through 3G versus a single step-3 wrap-up. The 3B devlog feels
genuinely useful as a record (the chicken-and-egg, the dead-code
allow, the no-retry asm) rather than rote bookkeeping, so I'm
keeping the per-sub-block cadence for now. If 3C's devlog ends up
mostly recapping the spec without project-specific judgment, that's
the signal to consolidate.

The Asahi cadence stays the model — calibrated, honest, never
marketing.

—
