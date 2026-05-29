# M1 step 2 (sub-blocks 2-5 / 2-6) — virtio-balloon: the first inherited Linux driver

*May 29, 2026. The closing-commit chain of M1-2-5 (rounds 18
through 22d) plus the 2-6 paper. The first piece of inherited
GPLv2 Linux driver code running inside Arsenal, against a
BSD-2-licensed shim its authors never saw.*

This is the devlog the whole shim was for. M1 step 2's premise —
ARSENAL.md's "single largest engineering task" — is that Arsenal
inherits Linux's driver corpus through a LinuxKPI-style shim
instead of writing every driver from scratch. virtio-balloon is
the proof. `vendor/linux-6.12/drivers/virtio/virtio_balloon.c`
is upstream Linux, GPL-2.0-or-later, unmodified. It compiles
against `linuxkpi/include/*.h` (BSD-2, Arsenal-authored), links
against the Rust shim, and at boot it probes a real QEMU
virtio-balloon-pci device, negotiates features, allocates
virtqueues, and reports memory statistics to the host. When the
host asks it to give memory back, it does.

It is a small driver and it does a small thing. That is the
point: it is the smallest honest end-to-end demonstration that
the combined-work model works — inherited C calling Arsenal
Rust, Arsenal Rust calling back into inherited C, across a
license boundary that stays clean.

This devlog is one of three closing M1 step 2. The shim-
foundation and GPL-boundary writeups are separate (the latter
records the ADR-0006 finding that `linuxkpi/include/` *is* the
Linux header surface, BSD-2-reimplemented, not vendored). This
one is the driver story: how balloon was brought online, and the
one bug that took most of the closing sessions to find.

## Two phases: compile, then make-real

The work split cleanly along a seam the step-2 HANDOFF
predicted. First, get balloon.c to *compile* — every `#include`
resolved, every referenced symbol declared. Then make those
symbols *real* — swap the panic-on-call stubs for working
implementations, one at a time, keeping `main` green at every
commit per the build-loop-is-sacred discipline.

The compile phase (sub-task 3, rounds 1 through 17) is its own
story and is mostly told in the STATUS history. The shape: each
round resolved one more of balloon's includes by writing a
BSD-2 proxy header under `linuxkpi/include/linux/`, growing
`shim_c.h` and the Rust shim as the body errors demanded. The
error stream *was* the work — `clang` told us exactly which type,
macro, or function balloon needed next, and balloon.c stayed out
of the `build.rs` source manifest (probed via a direct `clang`
invocation) until it compiled clean. It reached `clang` exit 0 at
round 17 (`99e2959`), with only three residual upstream
`-Wpointer-sign` warnings in balloon's own int-vs-unsigned
`virtqueue_get_buf` calls.

Two first-use decisions during the compile phase were large
enough to be ADRs rather than code:

- **[ADR-0007](../adrs/0007-struct-page-thin-handle.md): `struct
  page` is a thin per-frame handle**, not a `mem_map` array.
  Forced by `balloon_compaction.h`. Foundational for every later
  mm-touching driver — Linux's `struct page` is an enormous
  union; Arsenal's is a small handle that maps to one physical
  frame.

- **[ADR-0008](../adrs/0008-module-init-by-symbol-name.md):
  inherited-driver module init by explicit symbol-name call**,
  not an initcall table. `module_driver`'s expansion drops
  `static __init` so the `virtio_balloon_driver_init` wrapper is
  externally callable; arsenal-kernel calls it by name after the
  shim self-test.

## What landed — the make-real chain

Rounds 18 through 22a turned the compiled-but-inert balloon.c
into a probing driver. Each commit swapped one cluster of stubs
for real implementations, driven by what balloon actually calls
during probe.

- `16070ec` *(round 18)* — real atomic `test_and_set_bit` /
  `test_and_clear_bit` over a LOCK-prefixed `AtomicU64` bitmap
  word. balloon's feature- and state-bit handling needs them.

- `c52ea00` *(round 19)* — real `struct page` lifecycle:
  `alloc_pages` (order 0) / `put_page` / `page_address` over the
  frame allocator and the HHDM bridge; `balloon_page_alloc` /
  `_enqueue` / `_dequeue` with a Rust mirror of `struct
  balloon_dev_info`; `sg_init_one` computing `dma_address = buf -
  hhdm_offset`. `adjust_managed_page_count` landed here as a
  deliberate no-op — it is on balloon's hot path and M1 has no
  managed-page accounting. (It became the natural sentinel hook
  later; see below.)

- `b291b95` *(round 20)* — the virtqueue bridge. Ten bridge
  functions wrap arsenal-kernel's `Virtqueue` (`push_descriptor`
  / `push_chain` / `pop_used`) plus `activate_queue` / `notify` /
  `set_driver_ok`. M1-2-3's panic-on-call virtqueue stubs
  (`virtqueue_add_outbuf` / `_inbuf` / `kick` / `get_buf` /
  `virtio_find_vqs`) became real, routing through per-queue
  `ShimVirtqueueState`.

- `1deb3f1` *(round 21a)* — `virtio_device` gains a `features:
  u64` field; `virtio_has_feature` / `__virtio_clear_bit` /
  `virtio_clear_bit` go real over it.

- `1382919` *(round 21b)* — `register_virtio_driver` drives the
  full init lifecycle. A new `linuxkpi_virtio_init_transport`
  bridge runs the virtio v1.2 §3.1.1 init dance with **bus-side
  feature intersection**: read the device's offered features, AND
  with the driver's `feature_table`, write the result back, set
  FEATURES_OK. Per matched device the path is init_transport →
  validate (if present) → probe; a declining probe resets the
  device so a later driver can re-init it from RESET.

- `eba56c6` *(round 21c)* — a static `virtio_config_ops` table.
  balloon's validate null-checks `vdev->config->get` but does not
  call it during init, so `.get` is a panic-stub fn pointer and
  `.del_vqs` is a no-op (single balloon, bounded leak at M1).

- `06bbf85` *(round 21d)* — the ADR-0008 module-init change in
  `module_driver`'s expansion.

- `c2b9760` + `0e18ffb` *(round 22a, steps 1-2)* —
  [ADR-0011](../adrs/0011-deferred-work-cooperative-runner.md)
  and the real workqueue it specifies: a single cooperative
  runner task. `queue_work` / `cancel_work` / `alloc_workqueue`
  and friends go real; arsenal-kernel spawns a `workqueue_runner`
  before `sched::init`. balloon defers its inflate work onto a
  workqueue, so the shim needed one before balloon could do
  anything on a config change.

- `b1043fc` *(round 22a, step 3)* — **light balloon at boot.**
  virtio_balloon.c enters `build.rs`'s manifest with
  `-DKBUILD_MODNAME='"virtio_balloon"'` and `-Wno-pointer-sign`.
  Linux Kbuild's `-mno-sse -mno-mmx -mno-avx -msoft-float` were
  needed too: clang `-O2` emitted an `xorps` for stack zero-init
  that `#UD`'d on the first instruction, because CR4 has SSE off
  at M1. `-device virtio-balloon-pci` joined the smoke command
  line and `ARSENAL_VIRTIO_BALLOON_OK` joined the required
  sentinels. **First inherited Linux driver online in Arsenal.**

At 22a the driver probes, negotiates, allocates its three
virtqueues (inflate / deflate / stats), and reports DRIVER_OK.
The runner task sits in the runqueue. But it sat *idle* — nothing
was driving a config change, so the inflate path never ran. That
is what 22b through 22d were for, and where the interesting bug
lived.

## The config-changed interrupt that wasn't

balloon inflates on demand. The host (here, a QMP `balloon`
command) sets a new target; the device raises a **configuration-
change interrupt**; balloon's `virtballoon_changed` reads the new
`num_pages` from device config, queues the inflate work, and the
runner allocates pages and hands them back. The whole loop hinges
on that one interrupt arriving.

Round 22b/c (`4179259`) built the scaffolding: a QMP harness in
the smoke script that waits for `ARSENAL_VIRTIO_BALLOON_OK` then
sends `{"execute":"balloon","arguments":{"value":<bytes>}}`, plus
the config-changed MSI-X wiring on the guest side — a dedicated
IDT slot, a per-device handler context, and a write of the config
MSI-X vector into the virtio common-config `config_msix_vector`
field per the virtio 1.2 spec ([OASIS
v1.2](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html),
§4.1.4.3).

It did not deliver. The QMP command landed (QEMU's trace showed
`virtio_balloon_to_target ... num_pages: 2048`), but the guest's
handler never ran, and the balloon never inflated. The config-
change MSI-X for vector `0x41` was never reaching the LAPIC. This
was the active issue for most of the closing sessions.

What made it hard is that every guest-side and device-state
precondition was provably correct, verified directly against
QEMU's internal state through the QMP monitor (`xp` to read the
device's MSI-X table and PBA, and the q35 ECAM window at
`0xb0000000` to read live PCI config space):

- The MSI-X table entry was programmed and unmasked
  (`addr=0xfee00000, data=0x41, vctrl=0`).
- MSI-X was enabled at the capability, with the function-mask bit
  clear.
- `config_msix_vector` read back as 0 — QEMU had accepted the
  vector.
- DRIVER_OK was set and stayed set.
- The Pending Bit Array never went pending, so the device was not
  deferring a masked interrupt.
- The vdev was not marked broken (no `NEEDS_RESET` in status, no
  guest-error log).

Reading QEMU 11's source confirmed the delivery path: balloon's
`virtio_notify_config` → `virtio_notify_vector` → `virtio_pci_notify`
→ `msix_notify`. `msix_notify`
([hw/pci/msix.c](https://gitlab.com/qemu-project/qemu/-/blob/v11.0.0/hw/pci/msix.c))
has exactly two early-return guards: the vector's use-count being
zero, and the vector being masked (which sets a pending bit). The
PBA was clear, ruling out the masked path; every other
precondition ruled out the use-count path. By elimination
`msix_notify` should have called `msi_send_message` and the
interrupt should have arrived. It did not. A logical impossibility
on the black-box evidence.

The resolution was in the one link the black box could not show:
where `msi_send_message`
([hw/pci/msi.c](https://gitlab.com/qemu-project/qemu/-/blob/v11.0.0/hw/pci/msi.c))
actually writes. An MSI is not a special signal — it is an
ordinary memory write to the LAPIC's address at `0xFEE00000`,
performed by the device *as a bus master*
([OSDev: MSI](https://wiki.osdev.org/MSI)). QEMU routes that write
through the device's bus-master address space, which is gated by
the **Bus Master Enable** bit (bit 2) of the PCI COMMAND register
([OSDev: PCI](https://wiki.osdev.org/PCI)). When BME is clear,
QEMU silently drops the write. `msix_notify` ran, found nothing to
gate on, called `msi_send_message`, and the message evaporated
into a disabled address space — no delivery, no pending bit, no
diagnostic.

The differential that isolated it: reading every device's COMMAND
register over ECAM showed nvme and virtio-blk with BME set
(`0x0107`) and balloon, virtio-net, and virtio-rng with it clear
(`0x0103`). nvme delivered MSI-X fine (its native Rust driver sets
BME); balloon did not. The reason the gap had survived this long
is that balloon's config interrupt was the *first MSI any virtio
device in Arsenal had ever tried to send* — net and blk poll their
queues and never raised one, so the missing bus-master enable had
simply never mattered.

The fix (`ddd79e8`) is three lines. `register_virtio_driver` sets
Bus Master Enable during transport bring-up, mirroring what
Linux's own virtio-pci core does in `vp_modern_probe` via
`pci_set_master`. It belongs in the shared transport path, not the
driver, because it is a precondition for *all* device-initiated
DMA — virtqueue descriptor access as much as MSI delivery. With
it, `apic_deliver_irq ... vector 65` appears in the trace, the
handler runs, `virtballoon_changed` fires, the runner inflates,
and QEMU emits a real `BALLOON_CHANGE` event as pages begin
moving back toward the 8 MiB / 2048-page target.
`adjust_managed_page_count` fires
`ARSENAL_VIRTIO_BALLOON_INFLATE_OK` on the first non-zero reclaim,
and it joined the required sentinels.

The trap is worth stating plainly for every future MSI-X driver,
inherited or native: **PCI Bus Master Enable is a hard
precondition for MSI delivery, not just for ring DMA, and QEMU
gives no diagnostic when it is missing.** The native NVMe driver
set it by hand; the shim path had to learn to. It is recorded in
STATUS.md and in a comment at the enable site.

## The debugging technique, since it generalizes

The thing that eventually cracked this was reading QEMU's actual
state instead of trusting the guest's view of it. Two tools did
the work, both over the QMP `human-monitor-command`:

- `xp /Nx <phys>` reads guest physical memory, which for MMIO
  regions routes through the device model — so reading the MSI-X
  table BAR shows QEMU's *committed* table, and reading the PBA
  shows whether an interrupt is pending.

- The q35 ECAM/MMCONFIG window at `0xb0000000` maps PCI config
  space into physical memory, so `xp 0xb0000000 + (dev << 15) +
  offset` reads a device's live config registers — including the
  COMMAND register that held the answer.

When the guest insists everything is correct and the device still
does nothing, the host's own view of the device is the tiebreaker.
That is the lesson worth carrying to step 5 (amdgpu), where the
black box will be far larger.

## Numbers

| Property | Value |
| --- | --- |
| Inherited C (balloon.c) | 1,223 lines, GPL-2.0-or-later, unmodified |
| Smoke sentinels | 17 (was 16 at 22a; `…_INFLATE_OK` added at 22d) |
| Smoke pass time | ~1.4 s on QEMU TCG, `-smp 4` |
| boot→prompt | ~110-215 ms (budget 3000 ms) |
| ELF (release) | ~1.57 MB |
| ISO | ~19.4 MB |

The shim Rust the driver consumes is spread across
`linuxkpi/src/` (virtio, irq, page, slab, workqueue, mm, and the
rest) plus the bridge in `arsenal-kernel/src/linuxkpi_bridge.rs`;
no single "balloon driver" file exists on the Arsenal side, by
design — balloon is the inherited C, and everything Arsenal wrote
is general shim surface the next inherited driver reuses.

## Trade-offs that resolved in flight

- *Fire-and-yield QMP harness.* Per ADR-0011's "no concurrent
  work at M1," the harness sends the balloon command and does not
  block on the cycle; the main smoke poll-loop waits for the
  sentinel instead. The cooperative runner does the inflate when
  it next gets CPU.

- *The sentinel hook lives in `adjust_managed_page_count`.* That
  function is on balloon's inflate path and fires once per
  reclaim; making it a one-shot sentinel emitter means the
  sentinel proves real pages moved, not just that an interrupt
  arrived.

- *Single config MSI-X vector, table entry 0.* balloon at M1 uses
  one config-change vector; the per-queue vectors stay
  unprogrammed (the queues are drained by polling). The shared
  transport enables bus mastering for all of it.

- *BME in the transport, not the driver.* The alternative was a
  narrow fix in the MSI-X setup path. Enabling it in
  `register_virtio_driver` is both more correct (it is what Linux
  does) and covers balloon's inflate-vq DMA, not only the
  interrupt.

## What this does and does not prove

It proves the combined-work model end to end: GPLv2 driver
source, BSD-2 shim headers and Rust, a clean directory-enforced
license boundary, and a working device. It proves the shim
surface built across rounds 1-22 is sufficient for one real
driver's full lifecycle — probe, feature negotiation, virtqueues,
deferred work, and a config interrupt round-trip.

It does not prove the shim scales to amdgpu. balloon is ~1,200
lines of pure virtio-bus interaction with no DMA-engine, no
firmware load, no interrupt storm, no real-hardware quirks. The
GPL-boundary devlog argues the *legal* model scales (the FreeBSD
drm-kmod precedent); whether the *engineering* scales is what step
5 will answer. The honest claim here is narrow and real: the first
inherited driver is online, and the path to it is now a worked
example.

## What's next

M1-2-6 closes M1 step 2 with this devlog, the shim-foundation
devlog, the GPL-boundary devlog, and the step retrospective. Then
M1 step 3 (xHCI USB) — native Rust versus a LinuxKPI port is the
first decision at its kickoff, and balloon is the data point that
makes the port option credible.

## Cadence

This is the virtio-balloon entry in the three-devlog M1-2-6
cluster the step-2 HANDOFF planned: one writeup per major theme
rather than one per sub-block, because the shim's sub-blocks span
many sessions each and the coherent stories are thematic (the
foundation, the license boundary, the first driver).

The closing-commit chain ran long. Rounds 18-22a moved at the
post-pivot concentration pace; 22b-22d did not, because a single
silent-drop bug absorbed most of several sessions. That is the
honest texture of inherited-driver work and a preview of the
later steps: the spec-correct code is the fast part, and the gap
between "every precondition is correct" and "it works" is where
the calendar goes. The discipline that paid off was refusing to
accept the logical impossibility and going to read the host's
state directly.

The first inherited Linux driver is online. Onward to the step-2
retrospective.
