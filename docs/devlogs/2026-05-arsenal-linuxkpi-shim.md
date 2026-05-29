# M1 step 2 — the LinuxKPI shim foundation

*May 29, 2026. The first of the three-devlog cluster closing M1
step 2. Covers the shim infrastructure — sub-blocks 2-0 through
2-5 Part A — that virtio-balloon then ran on. The driver story is
its own devlog; the license story is a third. This one is the
layer underneath both.*

ARSENAL.md calls M1 step 2 "the single largest engineering task"
of M1. It is also the one that ships nothing a user can see. The
shim is the layer that lets Arsenal inherit Linux's driver corpus
instead of writing every driver from scratch — amdgpu (step 5),
iwlwifi (step 6), and the long tail after. Until a driver runs on
top of it, the shim is pure infrastructure: a contract with no
caller.

The discipline that kept that bearable, per the step-2 HANDOFF,
was to make the contract concrete one surface at a time. Build
the smallest viable shim surface, give it a smoke test, commit,
repeat. Each commit is a bisect-rich checkpoint, and the shim
self-test (`ARSENAL_LINUXKPI_OK`) grows an assertion per surface
so "the shim still works" is a CI fact, not a hope. This devlog
walks those surfaces in the order they landed.

## The model — a two-way bridge

[ADR-0005](../adrs/0005-linuxkpi-shim-layout.md) fixed the shape
before any code. The shim is a single Cargo workspace member,
`linuxkpi/`, peer to `arsenal-kernel/` and `xtask/`. It is a
two-way FFI bridge:

- Inherited C calls *into* Rust shim functions — `printk`,
  `kmalloc`, `pci_register_driver`, `request_irq` — declared
  `extern "C"` and surfaced through a hand-written
  `linuxkpi/include/shim_c.h`.
- Arsenal Rust calls *into* inherited C entry points
  (`virtio_balloon_driver_init`) by their exported symbol names.

One subtlety the layout had to respect: the crate dependency is
one-way. `linuxkpi` cannot depend on `arsenal-kernel` (that would
be circular — the kernel links the shim). So kernel-side
primitives the shim needs — the frame allocator, the HHDM offset,
the LAPIC EOI, the jiffies counter — are reached through a
dedicated `extern "C"` surface in
`arsenal-kernel/src/linuxkpi_bridge.rs`. The bridge is the
pattern: anything the shim needs from the kernel proper crosses
that file as a named C function. By step-2 exit it is the
single chokepoint for shim↔kernel coupling, which keeps the
coupling auditable.

ADR-0005 also chose the `cc` build-dep crate to compile inherited
C (over a bespoke xtask harness or host Kbuild — the latter
rejected because CI runs on macOS runners), a directory-based
GPL/BSD-2 license fence, a hand-written `shim_c.h` (cbindgen
deferred), and synchronous module init with the deferred-work
primitives stubbed panic-on-call. Those last two — the header
strategy and the deferred-work path — both grew their own ADRs
later (0006 and 0011); the license-fence story is the third
devlog in this cluster.

## What landed — surface by surface

- `4b1f88e` *(M1-2-1)* — the foundational shim. Four modules and
  the wiring to smoke them. `types.rs` (FFI typedefs), `log.rs`
  (`printk` with `KERN_*` level detection, routed to serial via a
  `linuxkpi_serial_sink` extern), `slab.rs` (`kmalloc` / `kzalloc`
  / `kfree` / `krealloc` with a 16-byte header for layout
  recovery on free), `locks.rs` (`atomic_t`, mutex, spinlock with
  `repr(C)` layouts plus Rust-friendly `Mutex<T>`). The self-test
  exercises every one and emits `ARSENAL_LINUXKPI_OK`; the smoke
  went from 14 to 15 sentinels.

  Two bugs caught in-session, both worth their comments. The
  `KERN_INFO` prefix was encoded `\x01\x06` (SOH then integer 6)
  instead of `\x016` (SOH then ASCII '6'), so the level-strip fell
  through silently and `[INFO]` never appeared. And a later
  sub-block surfaced that static `atomic_t` declarations landed in
  `.rodata` because `atomic_t { counter: i32 }` had no interior-
  mutability marker — the first `.inc()` page-faulted on a
  read-only kernel address. The fix, `counter: UnsafeCell<i32>`,
  preserves the C ABI (`int counter`) while making the statics
  writable. A trap worth remembering for any Rust type meant to
  live in a `static`.

- `f61c1a0` + `911518f` *(M1-2-2)* — the PCI / IRQ / DMA surface,
  in two commits. The PCI bus adapter mirrors Linux's
  `<linux/pci.h>`: `pci_register_driver` walks every (bus, dev,
  func), matches against a NULL-sentinel `id_table` (honoring
  `PCI_ANY_ID` and `class_mask`), and dispatches `.probe` with
  cached BAR addresses (the BAR-sizing dance per PCI Local Bus
  Spec 3.0 §6.2.5.1). DMA-coherent helpers are no-ops on x86_64
  per the cache-coherent-DMA guarantee (Intel SDM Vol. 3A §11.3).

  The IRQ bridge is a 16-slot dispatcher pool: pre-generated
  `dispatch_0..dispatch_15` functions, each an
  `extern "x86-interrupt"` that calls a common path indexing a
  static slot table, installed in the IDT through
  `idt::register_vector` (the foundation laid by NVMe at M1 step
  1-0). `request_irq` populates a slot; the dispatcher invokes the
  registered Linux handler and sends LAPIC EOI through a bridge
  fn. `pci_alloc_irq_vectors` reads the MSI-X capability and
  programs the table. This is the surface virtio-balloon's config-
  changed interrupt eventually rode — and the surface where the
  round-22d bus-master bug lived (see the balloon devlog).

- `2fed90c` *(M1-2-3)* — the virtio bus adapter, `virtio.rs`.
  `virtio_driver` registration, `.probe` dispatch, `virtio_cread`
  / `virtio_cwrite` over device config, and the
  PCI-device-id → VIRTIO_ID translation. The virtqueue surface
  (`virtio_find_vqs`, `virtqueue_add_*`, `virtqueue_kick`,
  `virtqueue_get_buf`) shipped here as panic-on-call stubs — the
  functional implementations landed at M1-2-5 when balloon's
  actual calls dictated their shape. The self-test found the three
  QEMU virtio devices (blk, net, rng) and matched a no-op driver
  against all of them.

- `6880b01` *(M1-2-4)* — the C build loop wired end to end.
  `linuxkpi/build.rs` drives `cc` against a tiny `csrc/smoke.c`;
  the `[INFO] linuxkpi: cc-build smoke ok` line proves the full
  round-trip: clang cross-compiles → an archive is built → rust-lld
  pulls it → inherited C calls the Rust shim → the shim calls back
  into C → returns.

  Two macOS-specific decisions the HANDOFF had not anticipated.
  Apple's `ar` / `ranlib` produce Mach-O archives with no ELF
  symbol index, so rust-lld cannot resolve symbols; the fix is the
  pure-Rust `ar` crate (GNU-format archives) paired with rustc's
  `+whole-archive` link modifier. And `-nostdinc` blocks clang's
  own freestanding-safe `<stddef.h>` / `<stdint.h>`; the fix is
  `-isystem $(clang -print-resource-dir)/include`, the canonical
  Kbuild dance.

- `40176cd` *(M1-2-5 Part A)* — the gap-fill surfaces balloon's
  compile would need: `container_of!` and `BUG_ON` (`macros.rs`),
  `IS_ERR` / `ERR_PTR` / `PTR_ERR` with `MAX_ERRNO=4095`
  (`err.rs`), intrusive `list_head` (`list.rs`), `jiffies` /
  `msleep` / `udelay` over a `linuxkpi_jiffies` bridge reading
  `apic::ticks()` at HZ=100 (`time.rs`), and the `copy_*_user`
  family as panic-on-call stubs (no userspace until M2, per
  ADR-0005 §6).

  A scoping bug worth recording: the `time` self-test originally
  asserted `jiffies() > 0` and post-`msleep` advancement, but
  `linuxkpi::self_test` runs before the scheduler's `sti`, so the
  LAPIC timer has never fired and `TICKS` is still 0 — the assert
  would have panicked, and if it hadn't, `msleep`'s busy-wait would
  have spun forever. The self-test was scoped to a callable-smoke
  only; real timer-advance coverage arrived when balloon called
  these post-`sti`. The lesson generalizes: a shim primitive that
  depends on IRQ delivery cannot be exercised in the pre-`sti`
  self-test.

## The self-test as the load-bearing invariant

The thing that made building a caller-less contract sane is that
every surface got a smoke test in the same commit that added it,
all funneled through one sentinel. `ARSENAL_LINUXKPI_OK` asserts:
`printk` from both Rust and C, a `kmalloc`/`kfree` round-trip,
`kzalloc` zero-fill, `Mutex<T>` lock, atomic inc/read/dec, a
C-callable mutex round-trip, a PCI walk finding the expected
device count, a `dma_alloc_coherent` round-trip with a page-
aligned handle assertion, an `err` round-trip, a three-element
list build/iterate/delete, `jiffies` callable, `container_of`
recovery, the bit-op state machine, and the workqueue round-trip
(added at 22a). Each line is one surface that cannot silently
regress.

The complementary discipline is fail-loud stubs. Every surface
the current driver does not exercise is a `panic!` with an
informative message, not a silent no-op returning a plausible
value. ADR-0005 §6 made this the standing posture: an inherited
driver that reaches an unimplemented path crashes at that call
site with a named reason, rather than misbehaving three layers
later. The panic-on-call stubs are a to-do list the next driver
populates by crashing into it.

## Numbers

The cumulative picture at M1-2-5 Part A exit, against the
step-2 HANDOFF's per-sub-block estimates:

| Sub-block | HANDOFF estimate | Actual |
| --- | --- | --- |
| 2-0 ADR + skeleton | 0.5 weeks | 0.5 sessions |
| 2-1 foundational shim | 2-3 sessions | 1 session |
| 2-2 PCI + IRQ + DMA | 4-5 sessions | 2 sessions |
| 2-3 virtio bus | 2-3 sessions | 1 session |
| 2-4 cc-build infra | 2-3 sessions | 1 session |
| 2-5 Part A gap-fill | (split) | 1 session |

Smoke at Part A exit: 15 sentinels, ~1.47 s, boot→prompt 184 ms.
The cadence rode the post-pivot concentration window the NVMe
devlog described — faster than the HANDOFF projected, with the
honest caveat that the variance was being deferred to the harder
sub-block ahead (2-5 Part B, the first driver's compile-error
iteration), not eliminated.

## Trade-offs that resolved in flight

- *Single workspace member, not a three-crate split.* ADR-0005
  reserved the `linuxkpi-headers` / `-shim` / `-drivers` split
  for when a second inherited driver (amdgpu) pulls a different
  subset and per-driver Cargo features start to pay off. At one
  driver it over-organizes. Still reserved.

- *`cc` build-dep over host Kbuild.* The deciding factor was
  cross-platform CI — the smoke harness runs on macOS runners, so
  binding to a host Linux source tree was a non-starter. `cc` is
  one MIT/Apache-2.0 build-dep half the ecosystem already uses.

- *Hand-written `shim_c.h`, cbindgen deferred.* At step-2 exit the
  header is small enough to maintain by hand; the cbindgen-revisit
  trigger (ADR-0005 §5, ~1500 lines) is now expected to arrive at
  amdgpu, sooner than first thought, because the header-strategy
  change in the GPL-boundary devlog grows `shim_c.h` substantially.

- *Synchronous init; deferred work stubbed.* `module_init` fires
  synchronously from `main.rs`; the `queue_work` / `kthread_run`
  family panicked until a driver needed them. balloon did, and
  that became ADR-0011's cooperative-runner workqueue.

## What this sets up

The shim foundation is the substrate, not the payoff. Its value
is measured entirely in how cheaply the next driver runs on it.
For virtio-balloon the answer was: the foundation surfaces
(log, slab, locks, PCI, IRQ, DMA, virtio bus, time, lists,
errors) carried unchanged, and the driver-specific work was the
virtqueue implementations, the feature/struct-page/workqueue
surfaces, and one bus-master bug. That is the balloon devlog.

The other thing the foundation forced into the open is the header
question. ADR-0005 §3 assumed Linux headers would be vendored
verbatim, ~20 of them. M1-2-5 Part B falsified that assumption
hard, and the correction — that `linuxkpi/include/` *is* the
Linux header surface, BSD-2-reimplemented — is the third devlog
in this cluster.

## Primary sources

- [ADR-0005: LinuxKPI shim layout and GPL/BSD-2 boundary](../adrs/0005-linuxkpi-shim-layout.md)
- [FreeBSD drm-kmod](https://github.com/freebsd/drm-kmod) — the
  combined-work shim precedent
- [`cc` crate](https://crates.io/crates/cc) — the cross-compile
  build dependency
- Linux kernel coding style
  ([kernel.org](https://www.kernel.org/doc/html/latest/process/coding-style.html))
  — the style any local driver fork follows
- Intel SDM Vol. 3A §11.3 — cache-coherent DMA, why the
  `dma_sync_*` shims are no-ops on x86_64

## Cadence

This is the shim-foundation entry in the three-devlog M1-2-6
cluster. The driver story (virtio-balloon) and the license story
(the GPL boundary, and ADR-0006's correction of the header
strategy) are the other two. Together with the step-2
retrospective they close the largest single block of M1, and the
shim is now a thing with a caller — which is the only proof a
contract layer ever really gets.
