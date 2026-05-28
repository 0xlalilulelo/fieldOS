# ADR-0011: Deferred-work via a single cooperative workqueue runner

## Status

Accepted. 2026-05-28. Resolves the panic-stub state of
[`linuxkpi/src/workqueue.rs`](../../linuxkpi/src/workqueue.rs)
that [ADR-0005 § 6](0005-linuxkpi-shim-layout.md) deferred
("the deferred-init shim primitives [...] are exposed as `extern
"C"` stubs at M1-2-1 that **panic-on-call**"). The trigger is
virtio-balloon's probe at M1-2-5 closing-commit round 22a:
balloon's `probe` calls `INIT_WORK` × 3 and `queue_work` × 4
(stats / size-update / free-page work paths), and its
config-change irq enqueues `update_balloon_size_work` on
`system_freezable_wq` — the host-driven inflate/deflate cycle
depends on that work running.

Takes the ADR-0011 slot that [ADR-0008](0008-module-init-by-symbol-name.md)
provisionally reserved as "Deferred / event-driven module init via
kthread + workqueue, and initcall-style table for synchronous
init." Splits the reservation: **this ADR resolves the deferred-
work side only**. The initcall-style-table side stays provisional
under a renamed ADR-0012 reservation (triggered if and when the
inherited-driver count or init-ordering need crosses the
threshold ADR-0008 names). Recorded as one-line edits to ADR-0005
§ "Reserved successor ADRs" and ADR-0006/0008's tracking notes
in this ADR's accepting commit.

## Context

ADR-0005 § 6 committed Arsenal to **synchronous module init at
M1** and to **panic-on-call stubs for the deferred-work
primitives** (`schedule_work` / `queue_work` / `kthread_run`),
with the deferred path to land "when an inherited driver actually
needs it — likely M1 step 5 or step 6." virtio-balloon at M1-2-5
arrives earlier than that forecast: balloon's
`update_balloon_size_work` is the path that responds to a host
`balloon` QMP command (config-change → `queue_work(system_-
freezable_wq, ...)` → work body reads target, inflates/deflates
the balloon via the virtio queues). Without a real workqueue, the
ARSENAL_VIRTIO_BALLOON_OK sentinel cannot fire.

What balloon's probe + runtime touch in the deferred-work
surface (one-line each from
`vendor/linux-6.12/drivers/virtio/virtio_balloon.c`):

- `INIT_WORK(&vb->update_balloon_stats_work, ...)` (line 964)
- `INIT_WORK(&vb->update_balloon_size_work, ...)` (line 965)
- `INIT_WORK(&vb->report_free_page_work, ...)` (line 995, only
  if `VIRTIO_BALLOON_F_FREE_PAGE_HINT` negotiated — likely off
  for QEMU defaults but present in the symbol surface)
- `alloc_workqueue("balloon-wq", ...)` (line 989, same `#ifdef`
  as above)
- `queue_work(system_freezable_wq, &vb->update_balloon_stats_-
  work)` (line 441, stats-vq callback)
- `queue_work(vb->balloon_wq, &vb->report_free_page_work)`
  (line 510)
- `queue_work(system_freezable_wq, &vb->update_balloon_size_work)`
  (line 516, config-change → inflate/deflate)
- `cancel_work_sync` × 3 (lines 1136-1140, **remove path only —
  not reached at M1**, where balloon is initialized once and
  never exits)
- `destroy_workqueue` × 2 (lines 1097/1141, **remove path only**)

Notably *absent* from balloon's surface: `kthread_run` /
`kthread_create` / `kthread_stop`. Future inherited drivers (a
USB hub poller, iwlwifi's rfkill thread) will want kthreads;
this ADR scopes the decision to **workqueues only** and leaves
kthreads as the same provisional stub state ADR-0005 § 6
established. The pattern this ADR sets is what kthreads will
re-use when they arrive.

The cooperative scheduler Arsenal runs on (single runqueue,
yield-driven, M0 step 4-4 hard-preemption layer on top) is the
substrate. There is **no concept of "irq context" vs "process
context"** distinct from cooperative-task context at M1; every
shim entry runs from a cooperative task (or from a hardware-irq
handler that runs to completion and re-enters scheduling). The
freezable / unfrozen / system / per-driver workqueue distinctions
Linux carries collapse to "the work runs from a single cooperative
runner task at M1."

## Decision

**Single cooperative workqueue runner.** All deferred work goes
through one shared queue, drained by one cooperative task.

```rust
// linuxkpi/src/workqueue.rs

#[repr(C)] pub struct work_struct {
    pub list:    list_head,                    // freezable/system list link
    pub data:    AtomicUsize,                  // PENDING / RUNNING flags + WQ id
    pub func:    Option<unsafe extern "C" fn(*mut work_struct)>,
}

static PENDING: Mutex<VecDeque<*mut work_struct>> = ...;
```

- `INIT_WORK(work, func)` zeroes the struct then writes `func`.
  Idempotent across re-init (Linux semantics).
- `queue_work(_wq, work)` pushes `work` onto `PENDING` iff the
  PENDING flag wasn't already set; sets the flag. Returns
  `true` for "newly queued" / `false` for "already queued"
  (matches Linux). The `_wq` argument is ignored — every queue
  is the same queue at M1.
- `cancel_work_sync(work)` removes `work` from `PENDING` if
  present; the "sync wait" is a no-op (the cooperative runner
  is the only thing that ever runs work bodies, so if the work
  isn't in PENDING and isn't currently executing, no wait is
  needed; if it IS currently executing, the canceller is by
  definition not the runner, so the canceller must be a
  cooperative peer that has yielded the CPU to let the runner
  run — and the runner runs each body to completion before
  returning).
- `alloc_workqueue(name, ...)` returns the same singleton
  sentinel as `system_freezable_wq`; `destroy_workqueue` is a
  no-op (single shared queue, no per-wq state to release).
- `system_freezable_wq` becomes a non-null sentinel pointer
  (currently the null placeholder linuxkpi/src/workqueue.rs:112).
  balloon enqueues to it directly; the sentinel survives any
  cast balloon does without observable difference.

A new cooperative task `workqueue::run_loop` is spawned from
arsenal-kernel's boot **after** `sched::init` but **before**
any inherited driver's init runs. It loops:

```
loop {
    if let Some(work) = PENDING.lock().pop_front() {
        clear PENDING flag on `work`; set RUNNING flag.
        invoke work.func(work);
        clear RUNNING flag.
    } else {
        sched::yield_now();
    }
}
```

Work bodies run to completion before the next work is pulled.
Concurrent work is **not supported at M1**; a body that needs
to block waits on the cooperative scheduler's yield primitives
(spinlocks, `wait_event` busy-polling). This matches the
single-CPU runqueue model M0 step 4-4 established and is
sufficient for balloon's three work types (stats, size, free-
page) which are independent in time and inexpensive per
invocation.

The runner is one task; there is no work-stealing, no per-CPU
workqueue, no priority. The pattern is "one cooperative drain
task per shared work source" — when a future driver needs
work-isolation (e.g., a slow driver shouldn't starve a fast
one), the right move is **a second runner task drained by a
distinct PENDING queue**, gated on a real measured starvation
event rather than speculative parallelism.

## Alternatives rejected

- **Real per-workqueue task + Linux-faithful semantics.** Spawn
  one cooperative task per `alloc_workqueue` call; carry
  freezable / unbound / per-CPU flags. **Rejected at M1** as
  speculative — balloon uses exactly one freezable system queue
  and (under one `#ifdef`) one driver-allocated queue; the
  semantic distinctions don't observably matter in cooperative
  scheduling without preemption-context constraints. The
  one-task-shared-queue model is the [CLAUDE.md](../../CLAUDE.md)
  "smaller version first" choice; the larger model is the right
  successor when a driver actually depends on the semantics
  (likely amdgpu's display tasks or iwlwifi's per-vif workers
  at M1 step 5 / step 6). Trigger for the successor: any
  observed starvation event between two inherited drivers'
  work bodies, OR an inherited driver that depends on freezable
  semantics for suspend/resume.

- **Synchronous "queue_work runs immediately on the caller's
  stack."** Inline the work body at the `queue_work` call site
  — turns deferred work into synchronous work. **Rejected**
  because balloon's config-change IRQ handler is the most
  common `queue_work` caller for the inflate/deflate path;
  running the inflate body from IRQ context would block the
  interrupt for the duration of the virtio round-trips, and
  violates Linux's well-established "irq handlers must not
  block" contract. Even if Arsenal's M1 model doesn't enforce
  the contract, balloon's source assumes it — running the work
  body inline would mean re-entering virtqueue locks the irq
  handler is supposed to leave to the work body. The cost of
  the proper deferred path is one cooperative task and one
  `VecDeque`; the savings from inlining don't justify the
  layering violation.

- **An Arsenal-native `Future`/executor and pretend `work_struct`
  is a future.** Rust-idiomatic; would let inherited drivers'
  deferred work compose with the kernel's existing `async`
  surface. **Rejected** because (a) Arsenal has no `async`
  executor at M1 — the cooperative scheduler is yield-based
  bare tasks, not poll-based futures; (b) `work_struct` is a
  C-side opaque type whose semantics are "function pointer
  + bookkeeping," not future-with-state; mapping one to the
  other would require allocating a `Pin<Box<dyn Future>>` per
  `INIT_WORK` plus a state machine the inherited driver can't
  see. The `VecDeque<*mut work_struct>` is a 30-line
  one-to-one match for Linux's semantics; the future-based
  shape would be 200-300 lines for no observable benefit at M1.
  The right successor (if Arsenal grows a future executor for
  native code) is a *separate* executor adjacent to the
  workqueue runner, not a replacement for it.

- **A single-shot panic-on-`queue_work` that prints a backtrace
  + halts.** The current state. **Rejected as the round-22
  destination** because the user's milestone exit criterion
  (ARSENAL_VIRTIO_BALLOON_OK on a real device round-trip — see
  ADR-0005 § 6's "M1-2-5-closing commit alongside the virtqueue
  impls that ARSENAL_VIRTIO_BALLOON_OK forces") *cannot* be met
  with the stub. The stub was right for M1-2-1 through M1-2-4
  (link-clean, defer the implementation); the M1-2-5 closing
  commit is where the deferral resolves, and ADR-0011 records
  the resolution.

## Consequences

**Easier:**

- **balloon's probe + config-change → inflate/deflate cycle
  works.** With one cooperative runner draining `PENDING`,
  every `queue_work(system_freezable_wq, ...)` call from
  balloon's hot path is observable to the smoke as a real
  virtqueue round-trip. The QMP-driven inflate cycle is round
  22b's responsibility; round 22a establishes that probe
  returns 0 and the static work_structs are initialized.
- **Future workqueue-touching drivers slot in for free.** Any
  inherited driver that uses `INIT_WORK` + `queue_work` works
  against this surface with no new shim code; the next
  `alloc_workqueue` from a different driver shares the same
  runner task.
- **The pattern generalizes to kthreads when they arrive.** A
  cooperative task drains a queue, the queue is named-but-
  semantics-collapsed at M1 — kthreads will follow the same
  shape: one or more cooperative tasks per `kthread_run` call,
  each driving the kthread's body.

**Harder / deferred:**

- **No work-isolation between inherited drivers.** A long-
  running balloon work body delays every other driver's work.
  At M1 (single inherited driver — balloon) this is moot;
  becomes a concern at M1 step 5 (amdgpu's display work) and
  step 6 (iwlwifi's per-vif workers). **Mitigation:** the
  successor model (per-workqueue runner) is pre-identified
  with concrete triggers.
- **Freezable semantics are unimplemented.** `system_freezable_-
  wq` is freezable in Linux because it's drained before
  suspend-to-RAM. Arsenal has no suspend/resume at M1; the
  semantic is a no-op. **Mitigation:** if a future inherited
  driver depends on the freeze barrier for correctness (rather
  than for power-management ergonomics), that surfaces as a
  visible regression in that driver's smoke — fail-loud is the
  established discipline.
- **`cancel_work_sync` correctness is bounded by "the canceller
  yields before observing the cancel."** A canceller that calls
  `cancel_work_sync` and immediately reads state the work body
  was supposed to update will observe stale state if the runner
  task hasn't been scheduled in. balloon's `cancel_work_sync`
  calls are all in the **remove path** (which M1 doesn't
  reach), so this is theoretical at M1. **Mitigation:** the
  hard correctness contract on `cancel_work_sync` lives in
  ADR-0011's successor when a driver's remove path actually
  runs at M1+.

**New risks:**

- **The runner task is a single point of liveness.** If a
  buggy work body panics, the runner task panics, the cooperative
  scheduler observes the task gone, and all future deferred
  work stalls silently (rather than fail-louding). **Mitigation:**
  panic-on-work-body in arsenal-kernel re-enters the kernel
  panic handler that already prints ARSENAL_PANIC and halts,
  so silent-stall is not observable in practice — the smoke
  catches it as a missing sentinel.

## References

- [ADR-0005 § 6: Synchronous module init/exit at M1; deferred path stubbed](0005-linuxkpi-shim-layout.md)
  — the panic-on-call commitment this ADR replaces
- [ADR-0008: Inherited-driver module init by explicit symbol-name call](0008-module-init-by-symbol-name.md)
  — the provisional ADR-0011 reservation this ADR claims
- [`linuxkpi/src/workqueue.rs`](../../linuxkpi/src/workqueue.rs)
  — the panic-stub surface this ADR makes real (the
  implementation lands in the same commit chain as this ADR)
- [`vendor/linux-6.12/drivers/virtio/virtio_balloon.c`](../../vendor/linux-6.12/drivers/virtio/virtio_balloon.c)
  — the trigger: `INIT_WORK` × 3, `queue_work` × 4,
  `alloc_workqueue` × 1 in probe + the config-change path
- [Linux 6.12 LTS `include/linux/workqueue.h`](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/tree/include/linux/workqueue.h?h=linux-6.12.y)
  — upstream `work_struct` + workqueue API the shim mirrors
- [Linux 6.12 LTS `kernel/workqueue.c`](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/tree/kernel/workqueue.c?h=linux-6.12.y)
  — upstream implementation; the source-of-truth for the
  semantics this ADR collapses to a single cooperative runner
- **ADR-0012 (provisional):** "Initcall-style table for
  synchronous inherited-driver init." The other half of ADR-
  0008's previously-combined ADR-0011 reservation; stays
  provisional, triggered per ADR-0008's "5+ inherited drivers
  or any cross-driver init-ordering requirement."
- **ADR-0013 (provisional):** "Per-workqueue cooperative
  runner + freezable semantics." Successor to *this* ADR,
  triggered by an observed starvation event between two
  inherited drivers' work bodies OR an inherited driver
  dependent on freezable semantics for suspend/resume.
- Michael Nygard, "Documenting Architecture Decisions" (2011)
  — ADR template authority
