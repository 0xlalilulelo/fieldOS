# M1 step 3 — xHCI USB, native Rust

*May 29 – June 8, 2026. Five sub-blocks (3-0 through 3-4), five
feat commits plus this devlog. A native USB stack from nothing:
host-controller bring-up, device enumeration, and two device
classes — a HID boot keyboard and a BOT/SCSI mass-storage device.*

xHCI was the step the M1 checklist left open: "native Rust or
LinuxKPI port — evaluate at start." It is also the first M1 driver
where that question had a real answer either way, so 3-0 spiked
both before committing. The rest of the step is the native path,
and it exercises something NVMe did not — the full USB transfer-
type spread (control, interrupt, bulk) against one controller.

Three things the step shipped:

- **A working xHCI host controller.** Find it, reset it, build the
  command/event rings + DCBAA, run it, and complete a command via
  an MSI-X interrupt — the NVMe ring-driver shape, transposed onto
  xHCI's register file (3-1).

- **USB device enumeration.** Walk the root ports, and over each
  device's default control endpoint drive Enable Slot → Address
  Device → GET_DESCRIPTOR → SET_CONFIGURATION, parsing the
  descriptor tree (3-2). This is the part NVMe has no analogue for
  — a real bus with addressable devices and a configuration
  handshake.

- **Two device-class drivers.** A HID boot keyboard whose
  keystrokes reach the live shell (interrupt transfers, 3-3), and a
  BOT/SCSI mass-storage device whose sector 0 reads back with the
  hybrid-ISO 0xAA55 MBR signature (bulk transfers, 3-4) — the same
  property the NVMe and virtio-blk smokes assert against the same
  backing image.

The milestone HANDOFF flagged six spec-fragile pieces in xHCI
bring-up and enumeration. One tripped (the event-ring drain, 3-1);
the rest were right first try. QEMU's `qemu-xhci` is a clean,
quirk-free xHCI 1.0 — the spec-correct sequence works, the same way
NVMe's did. Real-hardware quirks (the Framework's actual xHCI, hub
topology, HID report-descriptor variation) land at step 7.

## The decision: native, on spike data (3-0)

The kickoff deferred the native-vs-port choice to a 3-0 gate that
ran two timeboxed spikes and decided on measured data, not opinion
— exactly the discipline ADR-0006 forced after the balloon header
closure came in 14× over estimate.

- *Port spike (measured).* An ADR-0006-style include-graph audit of
  `drivers/usb/host/xhci-hcd` + usbcore resolved **655 headers and
  was not complete** — ~2.5× balloon's 281 — with 52 Kbuild-
  generated misses, atop ~15k LOC of xhci `.c` plus the entire USB
  core (hub driver, device model, URB machinery). An amdgpu-scale
  shim explosion, front-loaded onto the step before the one that
  actually needs the shim de-risked.

- *Native spike (worked).* An NVMe-style bring-up of `qemu-xhci`
  (caps → reset → DCBAA → command ring → event ring/ERST → run →
  No-Op) got a Command Completion Event round-trip **first try**,
  ~340 LOC, none of the spec-fragile pieces needing debug.

The decision — **native Rust**, recorded in
[ADR-0009](../adrs/0009-xhci-native-rust.md) — is a documented
deviation from ARSENAL.md's "xhci shim-hosted" row. The reasoning:
the port budget belongs on amdgpu (step 5), which is where the shim
has to scale to a complex driver whether we like it or not.
Front-loading a USB-core shim explosion onto step 3 would de-risk
the wrong surface (USB-core's shim differs from amdgpu's DRM/DMA
surface) and spend the budget twice. The spike was promoted to
`arsenal-kernel/src/xhci.rs` as the 3-1 seed; it no-ops when no
xHCI controller is present, so the production smoke was unaffected
through the whole step.

## What landed

Five commits between the step-3 HANDOFF at `eda439e` and this
close-out:

- `59b23ed` *feat(xhci): M1-3-0 — decide native xHCI (ADR-0009) +
  HC bring-up seed.* The gate. Both spikes' findings, the ADR, and
  the promotion of the native spike to `xhci.rs` (361 LOC seed).
  The port path's sub-block sketch was retired in the same commit.

- `6f9208d` *feat(xhci): M1-3-1 — host controller bring-up, MSI-X-
  driven completion.* Builds the seed into a real (minimal) xHCI HC:
  the §4.2 sequence (read CAPLENGTH/RTSOFF/DBOFF + HCSPARAMS/
  HCCPARAMS, reset, allocate DCBAA + command ring + event ring +
  ERST, program the operational and interrupter-0 registers, run),
  PCI bus-master enable, interrupter 0 wired to an MSI-X vector,
  connected root ports reset, and a No-Op command completed via an
  MSI-X interrupt. `-device qemu-xhci` + `-device usb-kbd` and
  `ARSENAL_XHCI_OK` enter the smoke (18 sentinels).

  The one spec-fragile piece that tripped: the event-ring drain.
  The first cut read a fixed event-ring slot for the No-Op's
  Command Completion Event — but the port resets done at bring-up
  enqueue Port Status Change Events *ahead* of the command
  completion. The fix walks the ring honoring the consumer cycle
  bit (CCS, starts 1, flips on each 16-TRB wrap) until the command
  completion appears, logging and skipping the interleaved PSC
  events. This is xHCI's version of the NVMe phase tag, but per-ring
  and with the wrap semantics that make it the most common bring-up
  bug. The MSI-X / BME / interrupter wiring was right first try —
  the step-2 BME carry-forward (QEMU silently drops MSI writes when
  Bus Master Enable is clear) was set explicitly at bring-up, so the
  round-22d failure mode never recurred.

- `e6d55b7` *feat(xhci): M1-3-2 — device enumeration over the
  default control endpoint.* The part with no NVMe analogue. For
  each connected, enabled root port: Enable Slot (a command that
  returns a slot ID) → allocate the output device context + an
  input context (Input Control + Slot + EP0 blocks, `ctx_size`-
  strided per HCCPARAMS1.CSZ) → Address Device → and then over the
  default control endpoint's transfer ring, the standard USB
  control transfers: GET_DESCRIPTOR(device), GET_DESCRIPTOR(config
  header) for `wTotalLength`, GET_DESCRIPTOR(config full) parsed
  into its interface/endpoint tree, SET_CONFIGURATION. Reuses 3-1's
  command ring, event-ring drain, MSI-X interrupter, and DCBAA; adds
  a per-device EP0 transfer ring and the Setup/Data/Status control-
  TRB helpers. `ARSENAL_USB_ENUM_OK`; 19 sentinels.

  qemu-xhci's usb-kbd enumerates at High Speed: slot 1, maxpkt0=64,
  vid=0x0627, one interface reporting class 0x03/0x01/0x01 (HID /
  Boot / Keyboard), one endpoint. Two flagged shortcuts: Address
  Device runs single-phase (BSR=0 — qemu handles it; the two-phase
  BSR=1 form some real full-speed devices need is deferred to step
  7), and the per-device context frames are left allocated for the
  kernel lifetime (a bounded one-time boot allocation, like the
  rings).

- `40ed573` *feat(xhci): M1-3-3 — HID boot keyboard online, feeding
  the live shell.* Interrupt transfers. When enumeration finds a HID
  boot-protocol keyboard interface, `setup_hid_keyboard` issues a
  Configure Endpoint command for its interrupt IN endpoint (slot 1,
  EP DCI 3, maxpkt 8), arms a Normal TRB on a fresh transfer ring,
  and hands the live ring state to a `HID` static. The cooperative
  `hid_poll_task` (the ADR-0011 runner shape; the MSI-X handler
  stays thin, per the NVMe prior) drains interrupt Transfer Events,
  decodes the 8-byte boot report into ASCII (press-edge detection
  against the previous report; shift via the modifier byte), injects
  it into the shared `kbd::RING` the shell already drains, and re-
  arms — the transfer ring wraps via a Link TRB so the keyboard
  stays live indefinitely, not just for the first 255 keystrokes.

  The smoke's QMP helper injects a press+release of 'a' (input-send-
  event) once the "HID keyboard armed" marker appears; the guest
  decodes usage 0x04 → 0x61 and fires `ARSENAL_USB_HID_OK`. The
  keyboard ring now has two producers — the M0 PS/2 IRQ and the
  cooperative USB poller — converging on one input stream via
  `kbd::inject`. 20 sentinels.

- `8fcd986` *feat(xhci): M1-3-4 — USB mass storage, BOT/SCSI sector-
  0 read.* Bulk transfers, and the transfer-type spread completed.
  A SCSI-transparent Bulk-Only-Transport interface (class 0x08 /
  subclass 0x06 / protocol 0x50) gets a Configure Endpoint for its
  bulk endpoint pair (EP Type 2 = Bulk OUT, EP Type 6 = Bulk IN),
  then one BOT transaction runs synchronously inside the enumeration
  `sti` window:

  - **CBW** (31-byte Command Block Wrapper, signature 0x43425355,
    bmCBWFlags=0x80 data-IN, carrying a SCSI READ(10) for LBA 0,
    1 block) on bulk OUT,
  - **512 bytes** of sector data on bulk IN,
  - **CSW** (13-byte Command Status Wrapper, signature 0x53425355)
    on bulk IN.

  Unlike the HID keyboard, the read self-completes — real data, no
  external stimulus — so this mirrors the NVMe / virtio-blk sector-0
  smoke shape rather than handing off to a poller. The CSW signature,
  echoed tag, and bStatus are validated, then the MBR boot signature
  0xAA55 at byte 510. `-device usb-storage,bus=xhci.0` backed by the
  ISO (the same readonly-ISO trick NVMe and virtio-blk use);
  `ARSENAL_USB_STORAGE_OK`; **21 sentinels**.

  The trap the HANDOFF flagged and that materialized: the bulk IN
  and OUT endpoints carry *different* endpoint numbers (QEMU: IN
  0x81 → DCI 3, OUT 0x02 → DCI 4), so each DCI is computed from its
  own bEndpointAddress, not assumed adjacent. The trap that did
  *not* materialize: QEMU answers READ(10) directly with no power-on
  unit-attention, so a non-zero CSW status is treated as a hard
  failure. A real device may surface a CHECK CONDITION that needs a
  TEST UNIT READY / REQUEST SENSE retry before READ(10) succeeds;
  that path is flagged in-code and deferred to the step-7 real-
  hardware checklist.

## The transfer-type spread

The reason HID *and* mass storage were both step-3 targets (the
kickoff's question was "HID, mass storage, or both" — answer: both)
is that together they prove the USB transfer-type coverage the stack
has to handle. NVMe was a single transfer model (submission/
completion queues, one DMA shape). xHCI's value as a step is that it
forces all three USB transfer types through one controller:

| Transfer | Sub-block | Ring shape | Completion |
| --- | --- | --- | --- |
| **Control** | 3-2 enum | EP0 ring, Setup/Data/Status TRBs | per-transfer event |
| **Interrupt** | 3-3 HID | per-endpoint ring, Normal TRB + Link wrap | recurring, poller re-arms |
| **Bulk** | 3-4 MSC | per-endpoint rings (IN+OUT), Normal TRB | self-completing read |

Control transfers have the three-stage Setup/Data/Status structure
unique to EP0. Interrupt transfers recur and need the ring to wrap
(the Link TRB) so the endpoint stays live. Bulk transfers move a
payload and complete on their own. One controller, three models —
the spread amdgpu and iwlwifi won't exercise but every real USB
device the Framework boots will.

## Numbers

| Sub | Commit  | xhci.rs Δ        | Smoke      |
| --- | ------- | ---------------- | ---------- |
| 3-0 | 59b23ed | +361 (seed)      | 17/17 (no-op) |
| 3-1 | 6f9208d | rewrite to ~480  | 18/18      |
| 3-2 | e6d55b7 | +~500            | 19/19      |
| 3-3 | 40ed573 | +~340            | 20/20      |
| 3-4 | 8fcd986 | +~250            | 21/21      |

`xhci.rs` totals ~1,590 LOC at step 3 exit. The HANDOFF's rough
order for the native path was "1.5–3k LOC for HC + enumeration +
two class drivers"; landed at the bottom of that range. The minimum-
viable shape skips the long tail (hubs, USB3 SuperSpeed specifics,
isochronous, hotplug, power management — all explicitly out of scope
per the HANDOFF), and the spec-correct qemu-xhci path needed no
quirk-workaround code.

Final smoke: 21/21 sentinels in ~1.5 s, boot→prompt ~211 ms (budget
3000 ms), stable across repeated runs. The four new step-3 sentinels
— `ARSENAL_XHCI_OK` (No-Op via MSI-X), `ARSENAL_USB_ENUM_OK` (a
device's descriptors read back + SET_CONFIGURATION accepted),
`ARSENAL_USB_HID_OK` (a decoded keystroke reached the ring), and
`ARSENAL_USB_STORAGE_OK` (sector 0 read back with 0xAA55) — each
assert a distinct observable property of the stack.

## Foundation reuse — and what step 3 added

xHCI consumed the step-1 NVMe foundation cleanly:

- **`pci::msix_info` + `pci::config_write32` + `pci::bar_address`**
  — same MSI-X table-programming path NVMe and balloon use, plus the
  BME write the step-2 retrospective made non-negotiable.
- **`idt::register_vector`** — the interrupter-0 vector, dynamically
  allocated from the same pool.
- **The thin-IRQ-handler + cooperative-drain pattern** — the MSI-X
  handler bumps a counter; the boot path (for commands) or the
  cooperative poller (for HID) drains the event ring. Same division
  of labor as `nvme_io_handler`.

What step 3 adds that later steps may reuse: the event-ring cycle-
bit drain (`next_event` / `wait_command` / `wait_transfer`), the
TRB-ring producer helpers with Link-TRB wrap, and the descriptor-
tree walkers (`first_interface`, `find_interrupt_in_endpoint`,
`find_bulk_endpoints`). None of it is abstracted into a reusable USB
core — it's a single-file driver, deliberately, because there is
exactly one USB controller and the abstraction would be speculative.

## What M1 step 4 looks like

Step 4 is **virtio-gpu, native Rust** — the CI-substrate step
inserted before amdgpu (per the STATUS plan restructuring at step-1
kickoff). The rationale: QEMU does not emulate amdgpu, so without
virtio-gpu the amdgpu step (5) would have no per-commit smoke
substrate and would develop against real Framework hardware only.
virtio-gpu (~1000–1500 LOC, no shim dependency) gives the kernel a
KMS-capable GPU driver QEMU smokes on every commit, and lets the
kernel-side GPU/display abstraction stabilize against a clean
virtual device before amdgpu has to consume it.

virtio-gpu is the third native virtio driver (after virtio-blk and
virtio-net from M0, virtio-balloon via the shim at step 2), so the
virtio transport surface is well-worn. The new work is the GPU
command protocol (VIRTIO_GPU_CMD_* over the control queue), a 2D
scanout (create resource → attach backing → set scanout → flush),
and the display abstraction the compositor will eventually sit on.
The next session writes the step-4 HANDOFF.

## Cadence note

This is the eleventh devlog of the post-pivot arc (M0's eight, NVMe's
one, the step-2 cluster's three, now step 3's one). Step 3 ran from
May 29 to June 8 — calendar-wider than NVMe's single day, narrower
than the step-2 shim's multi-week grind. The post-pivot
concentration window is still open but the texture is shifting: the
event-ring drain bug (3-1) and the BOT endpoint-DCI trap (3-4) were
the kind of spec-detail friction that, on real hardware, multiplies.

The honest read on velocity is unchanged from the NVMe devlog: xHCI
went well because qemu-xhci is faithful and the spec subset is
bounded, the same reasons NVMe went well. The M1 budget does not
shrink because three steps closed fast — the variance lives in steps
5 (amdgpu, ~10k LOC of inherited C through the shim), 6 (iwlwifi),
and 7 (first real-hardware boot), where it always lived. The native-
vs-port decision at 3-0 was made precisely to keep the shim's hard
scaling test concentrated on amdgpu, where it is unavoidable, rather
than spending it twice.

M1 step 3 is complete. Onward to virtio-gpu.
