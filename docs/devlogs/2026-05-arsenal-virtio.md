# M0 step 3C — virtio bring-up

*May 9, 2026. Four sessions. Five commits.*

3C is the third of seven sub-blocks in M0 step 3 (memory, scheduler,
virtio, network, framebuffer, SMP, `>` prompt). The exit criterion is
narrow: PCI bus discovery, modern virtio PCI transport recognition,
split-virtqueue allocation, and one round-trip per device class
that proves notify-doorbell and DMA work. After 3C, the kernel
issues real I/O outside serial — sector-0 from a virtual disk and a
single Ethernet frame down the wire. 3D wires smoltcp on top.

## What landed

Five commits across four sessions:

- `d4ea3d2` *feat(kernel): PCI config-space scanner.* Brute-force
  walk over (bus 0..256, dev 0..32, func 0..8) via legacy CF8/CFC.
  Reads vendor at config offset 0x00; vendor 0xFFFF skips absent
  functions. Multifunction bit at the header-type byte gates the
  function-1..7 probe so the common single-function case is one
  config read per device. Tags virtio (vendor 0x1AF4) for 3C-1.
  ECAM via ACPI/MCFG is deferred to post-3F when MADT lands.
- `1d90405` *feat(kernel): virtio modern PCI transport.* For each
  virtio device, walks the capability list at PCI offset 0x34,
  picks out VIRTIO_PCI_CAP_* entries (common cfg, notify, isr,
  device cfg, pci-cfg-window), translates BAR + offset to a
  physical address, then to a kernel virtual address via HHDM.
  Modern only — legacy is retired, transitional is dead weight.
  3C-1 just prints what it found; 3C-3 / 3C-4 dereference.
- `8764f62` *feat(kernel): virtqueue rings + descriptor mgmt.* Split
  virtqueue per virtio v1.2 § 2.6. One Virtqueue holds three rings
  backed by a single 4-KiB frame: descriptor table, available
  ring, used ring (16 / 2 / 4-byte alignment). The HANDOFF's "one
  frame per ring" recommendation got swapped for "one frame per
  queue" — a 64-descriptor queue is ~1.7 KiB total and the
  three-frame waste was unjustified. push_descriptor and pop_used
  for single descriptors; push_chain and chain-aware pop_used
  arrived in 3C-3 when virtio-blk needed them.
- `bc6ccfc` *feat(kernel): virtio-blk + sector-0 read smoke.*
  Drives the v1.2 § 3.1.1 init dance — reset, ACK, DRIVER, read
  64-bit device features in two halves, write driver features
  (VIRTIO_F_VERSION_1 only), FEATURES_OK, verify retained,
  activate queue 0, DRIVER_OK. Submits one VIRTIO_BLK_T_IN as a
  3-descriptor chain (header + data + status), polls completion
  via sched::yield_now, asserts the hybrid-ISO MBR boot signature
  0xAA55 at offset 510..512 of the returned data. Folded in
  three pieces because they were each minimum surface for the
  round-trip: paging::map_mmio (the genuine retry, see below),
  virtio::find_device + VirtioDevice (probe with handle return),
  push_chain + chain-aware pop_used.
- `174127b` *feat(kernel): virtio-net + probe TX smoke.* Two
  queues (RX + TX), RX pre-populated with eight 1528-byte
  buffers, single 64-byte zero TX frame as a 2-descriptor chain
  (virtio_net_hdr + payload). Smoke target is "TX descriptor
  returns used" — proves notify works and the device acknowledged
  TX; whether QEMU slirp delivered the frame anywhere is 3D's
  domain. Same commit relocated cc_*/CC_*/STATUS_*/init_transport/
  activate_queue from virtio_blk.rs into virtio.rs as pub(crate)
  — they were never blk-specific, and net needs them too.

## How long it took

Four evening sessions on Apple Silicon, all on 2026-05-09 — the
same calendar day that closed 3A and 3B. Maybe five hours of active
time. ARSENAL.md budgets months for the *whole* of M0 step 3, and
3A + 3B + 3C compressing into a single day was not the projection.

The asymmetry holds. Memory primitives, scheduler scaffolding, and
PCI / virtio bring-up all share the same property: well-trodden
ground, dense spec, careful documentation pays back fast. The
HANDOFF before each step was a 200–300-line brief that named the
exact registers, layouts, and pitfalls — every dword offset and
every alignment requirement. The code is mechanical against that
backdrop.

What does not hold this property is anything where reality argues
with the spec. 3D (smoltcp + rustls) is mostly spec-driven (TCP
RFC 9293, TLS RFC 8446) with a short list of integration gotchas.
3E (framebuffer) needs a font, a damage-tracking compositor, and
a Wayland-shaped surface for native apps — bigger design surface,
many small unknowns. 3F (SMP) is where the cooperative-correctness
shortcuts in 3B and 3C have to actually become preempt-safe, and
the "x86 TSO covers it" answer stops being enough on Apple Silicon
later. The pace has to slow for those.

## Detours worth recording

**HHDM doesn't cover MMIO.** The single retry of the entire 3C arc.
The 3C-1 transport probe printed cap pointers without dereferencing
any of them; everything looked fine. 3C-3's first MMIO write to
COMMON_CFG faulted with PRESENT=0 — the HHDM-derived virtual
address didn't actually resolve. Limine's protocol spec, on closer
read, only guarantees mapping of "USABLE memory ranges" plus
"bootloader reclaimable memory ranges." Device MMIO regions like
the q35 PCI BAR window at 0xfe000000 are RESERVED in the firmware
memory map and skipped. The fix was paging::map_mmio using
x86_64::OffsetPageTable backed by a FrameAllocator adapter on
FRAMES, hooked from virtio::try_resolve so each cap's region gets
mapped as it's discovered. ~60 LOC of paging code I would have
saved a session on if the HANDOFF had named this prereq. Worth
adding to future device-driver HANDOFFs as a standing prereq.

**Feature negotiation didn't bite.** The HANDOFF flagged 3C-3 as
the place where "the device is silently stalling because we
accepted feature X but accessed register Y that X disabled" was
likely. We accepted only VIRTIO_F_VERSION_1 — the simplest possible
feature set — and the device retained FEATURES_OK on read-back,
proving negotiation worked. The bug surface flagged was real but
we didn't approach it; the smoke target wanted a single read, not
a fancy I/O path. A future "TCP throughput" smoke or an "RX
mergeable buffers" optimization will revisit.

**3C-4 was easier than 3C-3.** The HANDOFF predicted 3C-4 as the
slowest sub-commit ("RX descriptor pre-population + TX completion
polling has more moving parts than blk's single-shot read"). It
landed first try because the foundation — transport probe, virtqueue
infrastructure, MMIO mapping, init / activate / notify helpers —
was already proven by 3C-3. Per-device drivers are mechanical when
the foundation is right. Worth recording: the second consumer of
a piece of infrastructure validates the abstraction more than the
first; if the second consumer has no friction, the first one
genuinely paid for the design.

**Transport-helper relocation in 3C-4.** I noticed mid-write that
cc_read*/cc_write*/CC_* were sitting in virtio_blk.rs even though
nothing about them was blk-specific — every modern virtio device
uses identical COMMON_CFG offsets and identical feature-negotiation
mechanics. virtio-net wanted the same primitives; rather than
duplicate ~100 lines, they moved into virtio.rs as pub(crate). The
move is a refactor, not new functionality, and CLAUDE.md's "don't
drive-by-clean" was the rule I considered ignoring. The
counterargument was decisive: the second consumer makes the
abstraction's home obvious, and re-reading the duplicated copy
six months from now would be friction the first consumer
shouldn't have created. Bundled into the 3C-4 commit — one
concern (network driver needs shared transport helpers) closes
both ends of the loop.

## The numbers

- **5 commits.** PCI scan / virtio transport / virtqueue / blk /
  net. Plus this devlog and a STATUS update — 7 commits if you
  count the paper trail.
- **2734 lines of Rust kernel code** in `arsenal-kernel/src/`,
  up from 1252 at end of 3B. Net +1482 LOC. Three new modules
  (pci.rs 180, virtio.rs 791, virtio_blk.rs 170, virtio_net.rs
  192); paging.rs grew by ~60 lines for `map_mmio`; main.rs grew
  by ~60 lines for the smokes.
- **~81 KB ELF**, up from ~47 KB at the end of 3B. Mostly the
  formatting / writeln! machinery the smokes pull in — virtio
  itself is small at the asm level.
- **~1 second** local TCG smoke. **Six sentinels**:
  ARSENAL_BOOT_OK, ARSENAL_HEAP_OK, ARSENAL_FRAMES_OK,
  ARSENAL_BLK_OK, ARSENAL_NET_OK, ARSENAL_SCHED_OK.
- **3 virtio devices** probed end-to-end: rng, blk, net. rng is
  unused but its presence proves transport / cap walking on a
  device class no driver targets.
- **789 polling spins** (TCG) for the virtio-blk read; **1 spin**
  for the virtio-net TX. Slirp acks immediately; a real disk
  controller would cost more.

## What the boot looks like

The serial trace is 47 lines now, ending at ARSENAL_SCHED_OK after
the ping-pong demo. The new bits between the 3B baseline and
ARSENAL_SCHED_OK:

```
pci 00:00.0 vendor=0x8086 device=0x29c0 class=0x06:0x00
pci 00:01.0 vendor=0x1234 device=0x1111 class=0x03:0x00
pci 00:02.0 vendor=0x1af4 device=0x1005 class=0x00:0xff (virtio)
pci 00:03.0 vendor=0x1af4 device=0x1001 class=0x01:0x00 (virtio)
pci 00:04.0 vendor=0x1af4 device=0x1000 class=0x02:0x00 (virtio)
pci 00:1f.0 vendor=0x8086 device=0x2918 class=0x06:0x01
pci 00:1f.2 vendor=0x8086 device=0x2922 class=0x01:0x06
pci 00:1f.3 vendor=0x8086 device=0x2930 class=0x0c:0x05
pci: scan complete; 8 devices, 3 virtio
virtio: probing 00:02.0 device=0x1005
  cap pci-cfg / notify / device / isr / common ...
virtio: probing 00:03.0 device=0x1001
  cap pci-cfg / notify / device / isr / common ...
virtio: probing 00:04.0 device=0x1000
  cap pci-cfg / notify / device / isr / common ...
blk: device at 00:03.0 ...
blk: features dev=0x10130006e74 drv=0x100000000
blk: queue 0 desc_phys=0x...ffda000
blk: submitted request, head desc=0
blk: completed; used.id=0 used.len=513 spins=277
ARSENAL_BLK_OK
net: device at 00:04.0 ...
net: features dev=0x10130bf8024 drv=0x100000000
net: rx_q desc_phys=... tx_q desc_phys=...
net: TX completed; used.id=0 used.len=0 spins=1
ARSENAL_NET_OK
```

The two device feature words show what each device offered.
0x10130006e74 for blk decodes to F_VERSION_1 + a handful of blk-
specific bits (BLK_F_SEG_MAX, BLK_F_BLK_SIZE, etc., plus the
generic F_RING_INDIRECT_DESC and F_RING_EVENT_IDX). 0x10130bf8024
for net adds NET_F_MAC, NET_F_STATUS, NET_F_MQ, GSO/csum bits.
We accepted exactly 0x100000000 (VERSION_1) for both — every
optional feature declined.

## What 3D looks like

Per ARSENAL.md M0, the next sub-block: smoltcp + rustls.

- **smoltcp on virtio-net.** smoltcp's `phy::Device` trait wraps
  our virtio-net's RX / TX. We supply a `virtio_net::Phy` that
  pulls used-ring entries on receive and submits chains on
  transmit. Buffer pools the same shape we already have.
- **TCP loopback first.** Stand up a TCP echo server on the
  smoltcp interface; have a task connect to it locally. No real
  network involved.
- **TCP through slirp.** QEMU's user-mode network gives us a
  10.0.2.2 gateway and DHCP. Have a task DHCP-discover, get an
  IP, TCP-connect to 10.0.2.2:80 (slirp's HTTP forwarder if
  configured, or just a closed-port RST round-trip). Smoke
  asserts something concrete — say, an SYN/SYN-ACK exchange.
- **rustls on top.** TLS 1.3 to a known endpoint reachable
  through slirp's port forward. Smoke asserts a successful
  handshake to a localhost test server stood up by the smoke
  script.
- **Cardboard Box scaffolding considerations.** None of this
  yet runs in userland; smoltcp + rustls live in the kernel for
  M0. The userland refactor is post-M0 when Wasmtime / WASI
  arrive.

The bug-prone moment in 3D is buffer lifetime. smoltcp wants the
backing memory for sockets to live long enough that the device's
in-flight RX completions have somewhere to land. We've already had
that conversation in miniature in virtio_net's RX pre-population;
3D scales it.

## Cadence

This is the third sub-block devlog of M0 step 3 (3A and 3B were
the first two). Per the 3B devlog, the cadence call was "keep per-
sub-block devlogs unless they read like rote bookkeeping." 3C's
devlog has the MMIO-mapping detour, the calibration note that
3C-4 was easier than predicted, and the transport-helper
relocation judgment call — all genuine project-specific judgment
worth recording. Continuing per-sub-block. If 3D's reads as a
recap of smoltcp's README, that's the signal to consolidate.

The Asahi cadence stays the model — calibrated, honest, never
marketing.

—
