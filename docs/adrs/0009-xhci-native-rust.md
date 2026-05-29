# ADR-0009: The xHCI host controller is native Rust, not a LinuxKPI port

## Status

Accepted. 2026-05-29. Resolves the M1 step-3 kickoff's deferred
"native Rust vs LinuxKPI port" decision (HANDOFF.md, M1-3-0), which
ARSENAL.md's M1 checklist left explicitly open ("xHCI USB driver —
native Rust or LinuxKPI port, evaluate at start"). Subordinate to
[ADR-0004](0004-arsenal-pivot.md) (Rust-only base + selective native
rewrites for low-complexity drivers) and informed by
[ADR-0006](0006-linuxkpi-headers-are-shim.md) (the shim's header
surface is reimplemented, not vendored — so a port's header closure
is reimplementation work, not transcription).

This is a **documented deviation** from ARSENAL.md's "Driver
strategy" row, which lists `xhci` in the LinuxKPI-shim-hosted column.
The same row lists `USB-HID` as a native Rust rewrite; this ADR
extends "native" to the host controller as well, for M1, on the
strength of the M1-3-0 spike data below. ARSENAL.md does not need
editing — the row's intent ("LinuxKPI for the high-complexity
drivers, native for the low-complexity ones") is preserved; the spike
reclassified xHCI's host-controller layer as low-complexity for
Arsenal's narrow needs.

## Context

M1 step 2 closed with virtio-balloon online — the shim proven for a
*simple* inherited driver. The open question the step-2
retrospective named is whether the shim scales to a *complex* one;
that question is deferred to amdgpu (step 5). xHCI is the
intermediate driver, and the M1-3 HANDOFF deferred its architecture
to a two-spike gate (M1-3-0) rather than pre-deciding on prose — per
ADR-0006's lesson that closure size must be measured, not estimated
(the balloon header closure was 281 against a ~20 estimate, off 14×).

Both spikes ran on the `spike/xhci-native` branch.

### Spike 1 — port closure audit (measured)

A BFS include-graph audit of `drivers/usb/host/xhci-hcd` + the USB
core, against Linux v6.12, resolving includes by Kbuild's search
order:

- **655 transitive headers resolved, and the closure was not
  complete** (18 still queued at a 700-fetch cap) — ~2.5× balloon's
  281.
- **52 missing / Kbuild-generated headers** (`asm/hash.h`,
  `asm/bootparam.h`, `asm/byteorder.h`, …) — the same
  build-time-synthesis impossibility ADR-0006 identified, at larger
  scale.
- The `.c` to vendor is ~15,400 lines across just five xhci host
  files (`xhci.c`, `xhci-mem.c`, `xhci-ring.c`, `xhci-hub.c`,
  `xhci-pci.c`), **plus the entire USB core** a host-controller
  driver must register against (`drivers/usb/core/*.c` —
  hcd/hub/message/driver/config/urb/usb, ~15-20k lines more).
- Under ADR-0006, those ~655 headers are not vendored — they are
  reimplemented in BSD-2 `linuxkpi/include/`. That is an
  amdgpu-scale shim expansion, front-loaded onto step 3.

### Spike 2 — native bring-up (worked)

A native NVMe-style bring-up of QEMU's `qemu-xhci`: read the
capability registers, reset, allocate the DCBAA + command ring +
event ring (+ ERST), run the controller, post a No-Op command TRB,
and observe its Command Completion Event on the event ring.

```
xhci-spike: caplength=0x40 max_slots=64 max_ports=8 max_scratch=0 ctx_size=32B
xhci-spike: op_base=+0x40 rt_base=+0x1000 db_base=+0x2000
xhci-spike: reset complete (CNR clear)
xhci-spike: controller running (HCH clear)
xhci-spike: No-Op command posted, doorbell rung
xhci-spike: event ring TRB type=33 completion_code=1 (spins=0)
xhci-spike: SPIKE OK — command-ring No-Op round-trip succeeded
```

Succeeded on the first real run, ~340 LOC, reading like the NVMe
driver. None of the spec-fragile pieces the HANDOFF flagged
(CAPLENGTH/RTSOFF/DBOFF offset math, 32-vs-64-byte contexts, ring
cycle bits, DCBAA, ERST) needed debugging. The controller completed
the command immediately (`spins=0`).

## Decision

**The xHCI host controller, USB enumeration, and the M1 class
drivers (HID keyboard, mass storage) are native Rust**, following the
NVMe template. No `drivers/usb/host/xhci*.c` or `drivers/usb/core/*.c`
is vendored; no USB-core shim surface is added to `linuxkpi/`.

The reasoning, weighing the two spikes:

1. **Native is demonstrably tractable.** The spike got a command-ring
   round-trip first try. The xHCI register/ring model is NVMe-shaped,
   and NVMe is the template Arsenal already executed well.

2. **The port's cost is amdgpu-scale and front-loaded.** Porting
   xhci-hcd drags in the full USB device model and core — ~655
   reimplemented headers and ~30k LOC of vendored GPL `.c`. That
   shim-expansion budget is better spent on amdgpu (step 5), the
   driver that genuinely *must* be ported because no one rewrites a
   modern GPU driver. Spending it on USB, which is rewritable, buys a
   partial de-risk (USB-core's shim surface differs from amdgpu's
   DRM/DMA surface) at disproportionate cost.

3. **Arsenal's USB needs are narrow.** M1 wants a HID keyboard (for
   the Framework boot path) and mass storage. That is the
   control/interrupt/bulk transfer spread plus single-tier
   enumeration — not the hub topology, runtime PM, isochronous, and
   device-class breadth Linux's USB core generalizes for. Native code
   sized to the narrow need is smaller than the general core's shim.

4. **It matches the spirit of ARSENAL.md's driver split** (native for
   low-complexity, LinuxKPI for high-complexity) once the spike
   reclassified the host-controller layer as low-complexity here.

The boundary of this decision: it covers M1's xHCI + USB. It does
**not** preclude a future LinuxKPI USB port if a later need (a
complex USB device class, real-hardware quirk density) makes native
maintenance unattractive — that would be a successor ADR with its own
evidence. And it does not weaken the combined-work commitment for the
drivers that genuinely need it (amdgpu, iwlwifi).

## Consequences

**Easier:**

- No USB-core shim explosion in step 3; the shim grows only for
  amdgpu/iwlwifi where the port is non-negotiable.
- The xHCI driver reuses the existing native foundation (NVMe's
  `pci::msix_info` + `idt::register_vector` + `pci::bar_address` +
  `paging::map_mmio` + the thin-IRQ-handler + cooperative-drain
  pattern). No new license boundary.
- Full control over the transfer model; no impedance-matching between
  Linux's URB lifecycle and Arsenal's cooperative scheduler.

**Harder:**

- Arsenal owns USB enumeration correctness (addressing, descriptor
  parsing, configuration selection) and the two class drivers — work
  Linux's usbcore would otherwise have provided. Larger native LOC
  than NVMe (rough order 1.5–3k across HC + enumeration + HID + BOT).
- Real-hardware quirks (the Framework's xHCI, HID report-descriptor
  variation, hub topology) are Arsenal's to handle at step 7, without
  Linux's accumulated quirk table. Mitigation: QEMU's spec-faithful
  `qemu-xhci` is the per-commit gate; step 7 surfaces the real-iron
  delta, as it will for every native driver.

**New risks / notes:**

- **A latent BME finding surfaced during the spike, unrelated to the
  decision but worth recording:** the native NVMe driver does not set
  PCI Bus Master Enable anywhere in `arsenal-kernel/src/`, yet its
  MSI delivers — QEMU's nvme model appears to default BME on (or
  Limine sets it). On real hardware, NVMe MSI may require an explicit
  BME write that nvme.rs lacks — a potential step-7 bug. The xHCI
  spike sets BME explicitly (the balloon round-22d lesson). Flagged
  for the M1 step-7 real-hardware checklist; tracked in STATUS.md.

## Reserved-ADR renumbering

This first-use decision claims the **0009** slot per the established
convention (accepted first-use ADRs take precedence over provisional
reservations and bump them — see ADR-0006/0007/0008's shuffles).
[ADR-0005](0005-linuxkpi-shim-layout.md)'s "Reserved successor ADRs"
list shifts accordingly: three-crate split → ADR-0010, cbindgen →
ADR-0012, initcall-style table → ADR-0013, per-workqueue runner →
ADR-0014 (0011 remains the accepted deferred-work cooperative runner).
The edit is applied to ADR-0005 in the same commit.

## Primary references

- M1-3-0 spike (this commit's parent state on `spike/xhci-native`):
  the port closure audit + the native bring-up serial transcript.
- [ADR-0006: LinuxKPI headers are the shim](0006-linuxkpi-headers-are-shim.md)
  — why a port's header closure is reimplementation, not vendoring.
- [docs/plan/ARSENAL.md § "Driver strategy"](../plan/ARSENAL.md) — the
  native-vs-shim split this ADR classifies xHCI within.
- [docs/devlogs/2026-05-arsenal-nvme.md](../devlogs/2026-05-arsenal-nvme.md)
  — the native-driver template the xHCI driver follows.
- [xHCI 1.2 specification](https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf)
  § 4.2 (Host Controller Initialization) — the bring-up sequence the
  spike followed.
- [Linux 6.12 `drivers/usb/host/`](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/tree/drivers/usb/host?h=linux-6.12.y)
  + `drivers/usb/core/` — the port-path source the closure audit
  measured.

---

*The shim earns its keep where the driver cannot be rewritten;
xHCI's host controller can be, and the spike proved it cheaply.
amdgpu is where the port budget belongs.*
