Kickoff for M1 step 3 — xHCI USB (host controller + enumeration +
HID keyboard + mass storage).

## Where we are

M1 step 2 (LinuxKPI shim foundation + first inherited driver)
closed on 2026-05-29. virtio-balloon is online end to end — the
first inherited GPLv2 Linux driver running against Arsenal's
BSD-2 shim, through its full lifecycle (init_transport with
feature intersection → validate → probe → virtqueues → workqueue
→ a config-changed MSI-X round-trip that inflates the balloon on a
host QMP command). The M1-2-6 paper shipped: three devlogs (shim
foundation, GPL boundary, virtio-balloon) plus the step-2
retrospective in STATUS.md.

HEAD is `7e7d48c` (docs: M1-2-6 close); working tree clean except
this file. Smoke is 17/17 (`ARSENAL_VIRTIO_BALLOON_INFLATE_OK` is
the newest sentinel). `main` is **42 commits ahead of
`origin/main`** — push before 3-0 kicks off so the entire M1 step
2 arc (shim + balloon + the BME fix + the paper + this HANDOFF)
lands on origin as one push.

## Read before proposing

read CLAUDE.md (peer concerns; Rust-only with the one exception of
inherited Linux drivers under the LinuxKPI boundary; BSD-2 base,
GPLv2 preserved on inherited drivers; build loop sacred; no
`unsafe` without a `// SAFETY:` comment) → STATUS.md (M1 step 2
complete, step 3 next; the two step-2 carry-forwards below are
load-bearing) → docs/plan/ARSENAL.md § "M1 — Real iron" (xHCI is
the step-3 checklist line "native Rust or LinuxKPI port — evaluate
at start"; § "Driver strategy" lists `xhci` shim-hosted but
`USB-HID` native — the host-controller choice is genuinely open)
→ docs/adrs/0005-linuxkpi-shim-layout.md + 0006-linuxkpi-headers-are-shim.md
(the shim layout + the headers-are-shim discipline a port path
would lean on heavily) → docs/devlogs/2026-05-arsenal-nvme.md
(the native-driver template: PCIe MSI-X, dynamic IDT vectors,
ring management, the thin-IRQ-handler + cooperative-drain pattern)
→ docs/devlogs/2026-05-arsenal-virtio-balloon.md (the BME bug, the
xp/ECAM debugging technique) → arsenal-kernel/src/{pci,idt,nvme}.rs
(the foundation step 3 reuses) → git log --oneline -10 → run the
sanity check below → propose the 3-0 spike shape (or argue for a
different decomposition) → wait for the pick.

Two step-2 carry-forwards that bite step 3 directly:

1. **PCI Bus Master Enable is a hard precondition for MSI
   delivery, not just ring DMA.** QEMU silently drops MSI writes
   when the COMMAND register's BME bit (bit 2) is clear; the
   round-22d bug was three sessions of "every precondition is
   correct and nothing delivers." xHCI is a bus-master device with
   MSI-X — set BME at controller bring-up. The native NVMe driver
   already does; whichever path step 3 takes must too.

2. **Measure shim closure, do not estimate it.** ADR-0006: the
   balloon header closure was 281 files against a ~20 estimate —
   off 14×. The 3-0 port spike exists precisely so the
   native-vs-port decision rests on a measured include-graph, not
   a guess.

## What step 3 is

A working USB stack: find the xHCI host controller, bring it up,
enumerate attached devices, and drive two device classes —

- a **HID boot-protocol keyboard** (interrupt transfers;
  keystrokes reach the shell — the path real Framework 13 boot at
  step 7 needs for input), and
- a **mass-storage device** (BOT/SCSI bulk transfers; read a
  sector and validate it, parallel to the NVMe / virtio-blk
  "sector 0, check 0xAA55" smoke).

Both targets were chosen at this kickoff (the question was HID,
mass storage, or both — answer: both). HID is the load-bearing one
for the Framework boot path; mass storage exercises bulk transfers
the HID path does not, so the two together prove the transfer-type
spread (control / interrupt / bulk) the USB stack has to handle.

## CI substrate

QEMU 11.0 emulates all three pieces (confirmed available):

- `-device qemu-xhci` — the host controller (a clean spec-
  compliant xHCI 1.0, no vendor quirks; the spec-correct path
  works, like NVMe did).
- `-device usb-kbd` — a HID boot keyboard on the xHCI bus.
- `-device usb-storage,drive=...` — a BOT/SCSI mass-storage
  device; back it with the ISO (readonly), the same trick NVMe and
  virtio-blk use, so the sector-0 read sees the hybrid-ISO 0xAA55
  MBR signature the other block smokes already validate.

Real-hardware quirks (the Framework's actual xHCI, hub topology,
HID report-descriptor variation) land at step 7; QEMU's faithful
emulation is the per-commit gate until then.

## The architecture decision — spiked, then recorded (3-0 is the gate)

The native-vs-port choice shapes everything downstream, and the
step-2 retrospective leaves a real open question: the shim is
proven for a *simple* driver (balloon, pure virtio-bus), but
whether it scales to a *complex* one is deferred to amdgpu (step
5). xHCI is the intermediate test. Per the kickoff decision, **3-0
runs two timeboxed spikes and decides on data**, then records the
decision in an ADR. The two candidates, stated so the spike has
something to falsify:

**Native Rust (the NVMe template).** Native xHCI host controller
(register interface + command/event/transfer rings — structurally
NVMe-like), native minimal USB enumeration, native HID and BOT
class drivers. *For:* the NVMe precedent worked well; full control;
no new GPL surface; matches ARSENAL.md's "USB-HID native" line;
avoids front-loading a USB-core shim explosion before the one that
actually matters (amdgpu). *Against:* more native LOC than NVMe
(rough order 1.5–3k for HC + enumeration + two class drivers);
reinvents a slice of usbcore; USB enumeration has real sequencing
complexity (addressing, descriptor parsing, configuration select).

**LinuxKPI port (xhci-hcd + usbcore).** Vendor Linux's
`drivers/usb/host/xhci*.c` + the USB core; write or inherit the
class drivers. *For:* de-risks the shim against a complex driver
before amdgpu; inherits Linux's mature quirk handling; matches
ARSENAL.md's "xhci shim-hosted" line. *Against:* pulls the entire
USB core surface (hub driver, device model, URB machinery) into
the shim — a large, amdgpu-scale expansion front-loaded onto step
3; the de-risk is only partial (USB-core's shim surface differs
from amdgpu's DRM/DMA surface); and ADR-0006's
header-reimplementation burden applies to all of it.

The spike must produce, for each candidate, a *measured* number
and a *worked* fragment — not an opinion. A prior lean either way
is fine, but 3-0's data is what decides.

## Sub-block plan

**(M1-3-0) Spike both, decide, record. The gate — COMPLETE (2026-05-29,
decided NATIVE).** Both spikes ran on `spike/xhci-native`:
  - *Port spike (measured):* the ADR-0006-style include-graph audit
    of `drivers/usb/host/xhci-hcd` + usbcore resolved **655 headers
    and was not complete** (~2.5× balloon's 281), with 52
    Kbuild-generated misses, atop ~15k LOC of xhci `.c` plus the full
    USB core. An amdgpu-scale shim explosion, front-loaded.
  - *Native spike (worked):* an NVMe-style bring-up of `qemu-xhci`
    (caps → reset → DCBAA → command ring → event ring/ERST → run →
    No-Op command) got a Command Completion Event round-trip **first
    try**, ~340 LOC, none of the spec-fragile pieces needing debug.
  - *Decision:* **native Rust**, recorded in
    [ADR-0009](docs/adrs/0009-xhci-native-rust.md) (a documented
    deviation from ARSENAL.md's "xhci shim-hosted" line; the port
    budget belongs on amdgpu). The spike was promoted to
    `arsenal-kernel/src/xhci.rs` as the 3-1 seed — it no-ops when no
    xHCI controller is present, so the production smoke is unaffected
    (still 17/17). ADR-0005's provisional reservations shifted up one.

The native path is now canonical:

  - **3-1 xHCI HC bring-up.** Build on the `xhci.rs` seed: keep the
    caps/reset/DCBAA/command-ring/event-ring/ERST sequence the spike
    proved, add port reset/enable, convert the polled event-ring
    observation to MSI-X-driven completion (the seed already sets
    BME), add `-device qemu-xhci` + `ARSENAL_XHCI_OK` to the smoke.
    The seed's `run()` becomes the real bring-up entry point.
  - **3-2 Enumeration.** Enable Slot → Address Device →
    GET_DESCRIPTOR → SET_CONFIGURATION over the default control
    endpoint; descriptor parsing. (`Address Device` with/without BSR
    is the trap to watch — HANDOFF spec-fragile list.)
  - **3-3 HID keyboard.** Interrupt endpoint + transfer ring,
    boot-protocol report parsing, keystrokes into the existing
    kbd/shell path. `-device usb-kbd`; sentinel `ARSENAL_USB_HID_OK`.
  - **3-4 Mass storage.** Bulk in/out endpoints, BOT/SCSI READ(10),
    sector-0 + 0xAA55 check (parallel to the NVMe/virtio-blk block
    smoke). `-device usb-storage` backed by the ISO; sentinel
    `ARSENAL_USB_STORAGE_OK`.

(The LinuxKPI-port sub-block sketch is retired — see ADR-0009 for the
data that ruled it out. A future complex USB need could revisit it as
a successor ADR, but M1 step 3 is native.)

**(M1-3-final) STATUS refresh + step-3 devlog(s) + step-4 HANDOFF.**
  Per the established close: flip STATUS to step 3 complete, write
  the devlog(s) (one if the step is single-block enough, a small
  cluster if not), and kick off M1 step 4 (virtio-gpu native Rust,
  the inserted CI-substrate step before amdgpu).

## Foundation step 3 reuses

- `idt::register_vector` + `pci::msix_info` + `pci::config_write32`
  (NVMe step 1-0) — xHCI is MSI-X-driven; the table-programming
  path is the same one NVMe and balloon use. **Remember BME.**
- `pci::bar_address` + `paging::map_mmio` — xHCI's registers live
  in a single MMIO BAR; map it like NVMe's BAR0.
- The cooperative scheduler + the ADR-0011 workqueue runner — USB
  enumeration and transfer completion are event-ring-driven;
  whether step 3 polls the event ring or drives it from the IRQ
  handler + a cooperative drain is a 3-1 decision (NVMe's "thin IRQ
  handler bumps a counter, cooperative consumer drains" is the
  prior).

## Spec-fragile pieces to watch

xHCI is spec-heavy (xHCI 1.0/1.2, ~400 pages). The pieces most
likely to bite, flagged so they are not a surprise:

- **64-byte vs 32-byte contexts** (HCCPARAMS1.CSZ) — slot/endpoint
  context size is controller-reported; hardcoding 32 breaks on a
  CSZ=1 controller.
- **Ring cycle-bit semantics** — command/event/transfer rings use
  a producer/consumer cycle bit that flips on wrap (the NVMe phase
  tag, but per-ring and with link TRBs). The most common bring-up
  bug.
- **DCBAA + scratchpad buffers** (HCSPARAMS2 Max Scratchpad) — the
  controller may require scratchpad pages allocated before it runs.
- **Port routing + reset sequencing** (USB2 vs USB3 port sets, the
  PORTSC reset/enable handshake) — enumeration won't start until a
  port is reset and enabled correctly.
- **Address Device with/without BSR** — the two-phase addressing
  (Block Set Address Request) trips early enumeration.
- **MSI-X + BME** — see the step-2 carry-forward; the silent-drop
  failure mode is now known. xp/ECAM the COMMAND register if an
  interrupt "should" arrive and doesn't.

## Estimates and cadence

The M1 milestone budget (ARSENAL.md months 9–24, ~67 part-time
weeks across 9 steps) is unchanged by step 2 closing fast. xHCI is
genuinely harder than NVMe whichever path it takes — NVMe was ~880
LOC against a small faithful-QEMU spec; xHCI's enumeration + two
device classes is a larger surface, and the port path's USB-core
shim is an unknown the 3-0 spike exists to size. Treat 3-0 as the
de-risking gate: the spikes either confirm a tractable path or
surface that step 3 is bigger than budgeted, early, while it is
cheap to know. Do not let the post-pivot concentration window's
optimism shrink the estimate before 3-0 produces numbers.

Per CLAUDE.md's working-hours posture: the concentration window
carried through step 2 but will close. If 3-2 (native enumeration)
or the port's compile-error iteration becomes the active issue for
multiple sessions — as the balloon BME bug did — write up what was
tried and step away for a day. Every gap-filling sub-block carries
a `wip:` branch as the partial-work checkpoint.

## Sanity check before kicking off

    git tag --list | grep arsenal     # arsenal-M0-complete
    git log --oneline -6              # 7e7d48c (HEAD), 610c93d,
                                      # 5b3c773, ddd79e8, 4179259,
                                      # 730f21a
    git status --short                # ?? HANDOFF.md (while drafting)
                                      # or clean once committed
    cargo build -p arsenal-kernel --target x86_64-unknown-none --release
    cargo clippy -p arsenal-kernel --target x86_64-unknown-none --release -- -D warnings
    cargo xtask iso                   # arsenal.iso ~19.4 MB
    ci/qemu-smoke.sh                  # ==> PASS (17 sentinels)

Expected: smoke PASSes with 17 sentinels; boot→prompt ~110-215 ms;
ARSENAL_VIRTIO_BALLOON_OK + _INFLATE_OK both fire.

## Out of scope for step 3 specifically

- **USB hubs / multi-device topology.** QEMU attaches usb-kbd +
  usb-storage to root ports; a single-tier topology is enough for
  the step-3 target. External-hub enumeration (the TT/MTT
  machinery) waits until a real device needs it.
- **USB3 SuperSpeed specifics beyond what qemu-xhci presents.**
  Stream protocol, bandwidth negotiation — not needed for HID +
  BOT.
- **Isochronous transfers** (audio/video). No step-3 device needs
  them; the transfer-type coverage is control + interrupt + bulk.
- **Runtime device hotplug.** Step 3 enumerates devices present at
  boot. Hotplug event handling waits for a driver that needs it.
- **Real-hardware boot.** Step 3 validates in QEMU only; the
  Framework's xHCI + real HID report descriptors are step 7.
- **Power management (suspend/resume, USB selective suspend).**
  Out per ARSENAL.md M1's no-S3/S4 note.

## Permanently out of scope (do not propose)

- Any `unsafe` block without a `// SAFETY:` comment naming the
  invariant. The xHCI driver is heavy on MMIO + ring pointer
  wrangling; every unsafe site needs it.
- Modifying inherited Linux `.c` source (if the port path is
  chosen) without forking into `vendor/linux-6.12-arsenal/` and
  documenting the diff. CLAUDE.md §3.
- Reverting any closed/tagged M0 or merged M1 commit.
- Force-pushing to origin.
- Dropping the BSD-2 SPDX header from any new Arsenal-base file;
  inherited `.c` retains its original GPLv2 SPDX.
- Pulling a GPL Rust crate into the base or shim.
- Religious framing; reintroducing HolyC; going back to stable
  Rust. CLAUDE.md hard rules / ADR-0004.
- Skipping the build + smoke loop on a feat commit.

## First action

**3-0 is complete (native, ADR-0009); start 3-1.** Build the real
HC bring-up on the `arsenal-kernel/src/xhci.rs` seed: keep the
caps/reset/DCBAA/command-ring/event-ring sequence the spike proved,
add port reset/enable, convert the polled event-ring observation to
MSI-X-driven completion, and add `-device qemu-xhci` +
`ARSENAL_XHCI_OK` to `ci/qemu-smoke.sh`. Keep the build loop green;
the seed already builds and round-trips a No-Op under qemu-xhci. Then
3-2 (enumeration) → 3-3 (HID) → 3-4 (mass storage) per the plan
above. The architecture section and spike framing above are kept as
the rationale of record for ADR-0009.
