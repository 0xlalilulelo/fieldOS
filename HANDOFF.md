Kickoff for M1 step 4 — virtio-gpu (native Rust): a KMS-capable GPU
driver for QEMU CI that stabilizes the kernel-side GPU/display
abstraction before amdgpu (step 5) has to consume it.

## Where we are

M1 step 3 (xHCI USB, native Rust) closed on 2026-06-08. A native USB
stack from nothing: host-controller bring-up, device enumeration, and
two device-class drivers — a HID boot keyboard feeding the live shell
(interrupt transfers) and a BOT/SCSI mass-storage device reading
sector 0 (bulk transfers). ~1,590 LOC in `arsenal-kernel/src/xhci.rs`,
native per [ADR-0009](docs/adrs/0009-xhci-native-rust.md). The step
exercised the full USB transfer-type spread (control / interrupt /
bulk) one controller has to handle. The step-3 devlog
(`docs/devlogs/2026-06-arsenal-xhci.md`) tells the arc.

HEAD is the M1-3-final commit (this STATUS + devlog + HANDOFF);
working tree clean once it lands. Smoke is 21/21 (`ARSENAL_XHCI_OK`,
`ARSENAL_USB_ENUM_OK`, `ARSENAL_USB_HID_OK`, `ARSENAL_USB_STORAGE_OK`
are the step-3 sentinels). Push before 4-0 kicks off so the step-3
arc lands on origin as one push (the user pushes each sub-block).

## Read before proposing

read CLAUDE.md (peer concerns; Rust-only base with the one exception
of inherited Linux drivers under the LinuxKPI boundary; BSD-2 base;
build loop sacred; no `unsafe` without a `// SAFETY:` comment naming
the invariant) → STATUS.md (M1 step 3 complete, step 4 active; the
step-3 carry-forwards are not load-bearing for step 4 but the
post-pivot-velocity framing is) → docs/plan/ARSENAL.md § "M1 — Real
iron" (step 4 is the inserted virtio-gpu CI-substrate step; read the
STATUS "M1 step plan" note on *why* it precedes amdgpu) and § the
compositor/Stage rows (virtio-gpu's display abstraction is what Stage
eventually sits on, at M2 — so the abstraction shape matters beyond
this driver) → arsenal-kernel/src/virtio.rs (the modern virtio-PCI
transport + `Virtqueue` — `find_device`, `activate_queue`, `notify`,
`push_descriptor` / `push_chain` / `pop_used`, `num_free`) →
arsenal-kernel/src/virtio_blk.rs (the cleanest native virtio driver
template: find → init → one queue → submit a request → poll the used
ring → validate) → arsenal-kernel/src/fb.rs (the Limine linear
framebuffer the kernel draws to today; virtio-gpu's scanout is the
GPU-backed parallel, and the display abstraction should generalize
over both) → docs/devlogs/2026-05-arsenal-virtio.md (the M0 virtio
transport bring-up) → git log --oneline -10 → run the sanity check
below → propose the 4-0 sub-block shape (or argue a different
decomposition) → wait for the pick.

## Why virtio-gpu, and why now (before amdgpu)

This step was inserted into the M1 plan at the step-1 kickoff (see the
STATUS "M1 step plan" rationale). The problem it solves: **QEMU does
not emulate amdgpu.** Without virtio-gpu, the amdgpu step (5) would
have no per-commit CI substrate — amdgpu would develop against real
Framework hardware only, with no smoke validation between sessions,
which is exactly the regression-blind position the build-loop
discipline exists to avoid.

virtio-gpu fixes that two ways:

1. **A KMS-capable GPU driver QEMU smokes on every commit.** `-device
   virtio-gpu-pci` is a clean, faithful virtual GPU. The same
   "spec-correct path works" property that carried NVMe and xHCI
   applies.

2. **It forces the kernel-side GPU/display abstraction to exist and
   stabilize against a clean device first.** amdgpu (via the shim)
   then consumes an abstraction that already has one real
   implementation behind it, rather than co-designing the abstraction
   with the hardest driver. This is the same logic as 3-0's
   native-vs-port call: don't spend the hard design budget twice.

The abstraction is the higher-stakes deliverable than the driver. The
driver is ~1000–1500 LOC of well-understood virtio work; the
display/GPU trait the compositor (Stage, M2) and amdgpu (step 5) both
sit on is the thing to get right. Propose its shape explicitly at 4-0
and flag it for review — it is the part worth slowing down on.

## What step 4 is

A working virtio-gpu driver that brings up a 2D scanout and presents a
framebuffer through it:

- Find the virtio-gpu device, init the virtio transport (the M0
  modern-PCI path), set up the control queue (and the cursor queue if
  needed — likely not for the smoke).
- Drive the virtio-gpu 2D command protocol over the control queue:
  GET_DISPLAY_INFO → RESOURCE_CREATE_2D → RESOURCE_ATTACH_BACKING (a
  guest framebuffer) → SET_SCANOUT → TRANSFER_TO_HOST_2D →
  RESOURCE_FLUSH. (Names are VIRTIO_GPU_CMD_*; spec is the virtio 1.2
  spec § "GPU Device".)
- Expose a kernel-side display abstraction (resolution + a writable
  framebuffer + a flush/present call) that generalizes over the
  Limine LFB fb.rs uses today and the virtio-gpu scanout — the
  surface amdgpu's KMS and, later, Stage will consume.
- Smoke: present a known pattern through the scanout and assert the
  command round-trips completed (the GPU has no "read it back" the way
  a block device does, so the sentinel asserts the command-queue
  responses, not pixel contents — see CI substrate below).

## CI substrate

QEMU emulates virtio-gpu faithfully:

- `-device virtio-gpu-pci` — the GPU. Virtio device type 16 → modern
  PCI device id **0x1050** (virtio-gpu is modern-only; there is no
  transitional id, unlike blk's 0x1001 / net's 0x1000 — verify the id
  `find_device` matches against as a first 4-0 step).
- The smoke runs headless (`-display none`), so the assertion is that
  the virtio-gpu *command protocol* round-trips: each control-queue
  command gets a VIRTIO_GPU_RESP_OK_* response in the used ring.
  GET_DISPLAY_INFO returning a valid scanout rect + the scanout/flush
  commands returning OK is the observable "the GPU works" property —
  the parallel to NVMe/xHCI reading sector 0 back. The sentinel
  (`ARSENAL_GPU_OK` or similar — name it at 4-0) fires on the
  completed flush.
- Real GPU bring-up (amdgpu on the Framework) is step 5; virtio-gpu's
  faithful emulation is the per-commit gate until then.

A note on the headless assertion: there is no pixel read-back, so the
smoke proves the command pipeline, not that pixels reached a display.
That is the honest limit of a headless GPU smoke and should be stated
in the sentinel's comment (like the perf-gate resolution caveat in
qemu-smoke.sh). Manual `-display gtk` verification of an actual drawn
pattern belongs in the step-4 devlog, the way the M0 framebuffer +
SMP devlogs recorded manual display checks.

## Sub-block plan (proposed — argue with it at 4-0)

**(M1-4-0) Device find + transport init + display abstraction shape.**
  Find virtio-gpu (id 0x1050), init the modern-PCI transport, set up
  the control queue. Propose the kernel display/GPU abstraction trait
  + flag it for review *before* building the command protocol on top —
  this is the design-review seam. Bisect seam.

**(M1-4-1) Display info + resource create + attach backing.**
  GET_DISPLAY_INFO (read the scanout geometry), RESOURCE_CREATE_2D,
  RESOURCE_ATTACH_BACKING (a guest framebuffer from FRAMES). Each
  command's control-queue round-trip validated against its
  VIRTIO_GPU_RESP_OK response.

**(M1-4-2) Set scanout + transfer + flush → sentinel.**
  SET_SCANOUT (bind the resource to scanout 0), TRANSFER_TO_HOST_2D,
  RESOURCE_FLUSH. Present a known pattern; assert the flush completed;
  land the GPU sentinel. `-device virtio-gpu-pci` enters the smoke.

**(M1-4-3) Wire the display abstraction to the existing fb path
  (optional, scope at 4-0).** Make the kernel's framebuffer drawing
  (fb.rs) able to target the virtio-gpu scanout, so the prompt /
  shell can render through the GPU instead of only the Limine LFB.
  May be deferred to M2 (Stage) if 4-2 is a clean enough exit; decide
  at 4-0 whether the abstraction needs a consumer now or just needs
  to exist for amdgpu.

**(M1-4-final) STATUS refresh + step-4 devlog + step-5 HANDOFF.**
  Per the established close: flip STATUS to step 4 complete, write the
  devlog (single arc, like step 3 — virtio-gpu is one cohesive
  driver), and kick off M1 step 5 (amdgpu KMS via the LinuxKPI shim —
  the headlining driver, and the shim's hard scaling test the 3-0
  native-vs-port decision deliberately concentrated here).

## Foundation step 4 reuses

- **The whole M0 virtio transport** (`virtio.rs`): `find_device`,
  `activate_queue`, `notify`, and `Virtqueue` (`push_descriptor` /
  `push_chain` / `pop_used` / `num_free`). virtio-gpu is the third
  native virtio driver — the transport is well-worn. The new surface
  is the GPU command protocol on top, not the transport.
- **virtio_blk.rs as the template**: find → init → one queue → submit
  a descriptor chain → poll the used ring → validate. virtio-gpu's
  control queue is the same shape (request buffer OUT + response
  buffer IN as a two-part chain), just with a richer command set.
- **`frames::FRAMES.alloc_frame`** for the guest framebuffer backing
  + command/response buffers (page-aligned by construction).
- **fb.rs** as the display the abstraction must generalize over —
  read how it stashes width/height/pitch and writes pixels; the GPU
  abstraction is the superset.
- **MSI-X / `idt::register_vector`** if step 4 drives the control
  queue by interrupt rather than polling — but a boot-time scanout
  setup can poll the used ring (virtio_blk's smoke does), and the
  control queue is low-frequency, so polling is likely simplest for
  M1. Decide at 4-0. (If interrupt-driven: BME is already the virtio
  path's known precondition — see the round-22d carry-forward.)

## Spec-fragile pieces to watch

virtio-gpu is far lighter than xHCI, but the traps:

- **Device id 0x1050, modern-only.** No transitional id; confirm
  `find_device` matches it (the existing constants are blk 0x1001 /
  net 0x1000 — gpu is the 0x1040+type modern form).
- **The control-queue command/response chain.** Each command is a
  descriptor chain: a device-readable request header (+ payload) and
  a device-writable response. Getting the OUT/IN descriptor flags and
  the two-part chain right is the equivalent of NVMe's PRP setup.
- **VIRTIO_GPU_CMD_* / RESP_* enum values + struct layouts.** The
  command structs (virtio_gpu_ctrl_hdr + per-command bodies) are
  fixed-layout little-endian; transcribe them from the virtio 1.2
  spec § GPU Device carefully (the ADR-0006 "transcription risk"
  applies to spec structs too).
- **Scanout geometry from GET_DISPLAY_INFO.** Don't hardcode a
  resolution; read the enabled scanout's rect (QEMU defaults are
  typically 1280×800 but read it).
- **RESOURCE_ATTACH_BACKING scatter-gather.** The backing is a list of
  (addr, length) entries; a single contiguous frame-backed buffer is
  the simple case, but the command still takes the SG-entry form.
- **TRANSFER_TO_HOST_2D before FLUSH.** The 2D model requires copying
  guest resource data to the host before flushing to the scanout;
  skipping the transfer shows nothing. (Headless, "shows nothing" is
  invisible — hence the manual `-display gtk` check in the devlog.)
- **The display abstraction is the real design risk, not the
  commands.** Over-abstracting (a full DRM-like KMS surface) is
  premature at M1; under-abstracting (virtio-gpu-specific calls
  leaking into fb/shell) makes amdgpu's consumption painful. Propose
  the minimal trait that both a scanout-GPU and a Limine LFB satisfy,
  and flag it.

## Estimates and cadence

ARSENAL.md budgets virtio-gpu at ~1000–1500 LOC and the STATUS plan
restructuring put M1 at ~67 part-time weeks across 9 steps. virtio-gpu
is genuinely lighter than xHCI (the transport is already built; the
command protocol is bounded), so the *driver* should go quickly the
way the third instance of a known pattern does. The *abstraction* is
where the time should go — it is the deliverable that pays off at
step 5 and again at M2, and the one worth a design-review pause at
4-0.

Per CLAUDE.md's working-hours posture: three M1 steps have now closed
inside the post-pivot concentration window. It will close. amdgpu
(step 5) is the ~10k-LOC inherited-C shim test where the calendar
variance has always lived; do not let three fast steps shrink the
estimate for the hard one. If 4-1's command transcription or the
abstraction design becomes the active issue for multiple sessions,
write up what was tried and step away for a day. Every gap-filling
sub-block carries a `wip:` branch as the partial-work checkpoint.

## Sanity check before kicking off

    git tag --list | grep arsenal     # arsenal-M0-complete
    git log --oneline -6              # M1-3-final (HEAD), 8fcd986,
                                      # 40ed573, e6d55b7, 6f9208d,
                                      # 59b23ed
    git status --short                # clean (or ?? HANDOFF.md while drafting)
    cargo build -p arsenal-kernel --target x86_64-unknown-none --release
    cargo clippy -p arsenal-kernel --target x86_64-unknown-none --release -- -D warnings
    cargo xtask iso                   # arsenal.iso ~19.4 MB
    ci/qemu-smoke.sh                  # ==> PASS (21 sentinels)

Expected: smoke PASSes with 21 sentinels; boot→prompt ~110–235 ms;
all four step-3 USB sentinels fire.

## Out of scope for step 4 specifically

- **3D / virgl / Vulkan.** virtio-gpu has a 3D mode (virgl); M1 is 2D
  scanout only. amdgpu is KMS-only at M1 too (no Vulkan) per ARSENAL.md
  — 3D is post-M1.
- **Multiple scanouts / multi-monitor.** One scanout for the step.
- **Hardware cursor / cursor queue.** Unless the smoke needs it
  (it shouldn't); software cursor is M2 Stage work.
- **Display hotplug / resolution change at runtime.** Read the
  geometry once at bring-up.
- **The compositor (Stage).** That is M2. Step 4 ships the
  abstraction Stage will sit on, not Stage.
- **Real GPU (amdgpu).** Step 5. virtio-gpu validates in QEMU only.

## Permanently out of scope (do not propose)

- Any `unsafe` block without a `// SAFETY:` comment naming the
  invariant. virtio-gpu is MMIO + DMA-buffer + ring work; every unsafe
  site needs it.
- Reverting any closed/tagged M0 or merged M1 commit.
- Force-pushing to origin.
- Dropping the BSD-2 SPDX header from any new Arsenal-base file.
- Pulling a GPL Rust crate into the base. (virtio-gpu is native Rust;
  no shim, no GPL surface — keep it that way.)
- Religious framing; reintroducing HolyC; going back to stable Rust.
  CLAUDE.md hard rules / ADR-0004.
- Skipping the build + smoke loop on a feat commit.

## First action

**Start 4-0: find virtio-gpu + init the transport + propose the
display abstraction.** Confirm `find_device(0x1050)` resolves QEMU's
`virtio-gpu-pci` (add `-device virtio-gpu-pci` to the smoke first so
the device is present), init the modern-PCI transport and the control
queue the virtio_blk way, and — before writing any GPU command — write
up the proposed kernel display/GPU abstraction trait (resolution +
writable framebuffer + flush/present) that generalizes over fb.rs's
Limine LFB and the virtio-gpu scanout, with 2–3 options if the shape
is genuinely open, and flag it for the design review. That trait is
the step's real deliverable; the commands hang off it. Keep the build
loop green; the GPU device is additive and the driver no-ops when
absent, like xhci.rs does.
