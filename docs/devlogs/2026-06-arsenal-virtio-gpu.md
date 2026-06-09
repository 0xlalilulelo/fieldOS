# M1 step 4 — virtio-gpu, native Rust

*June 8, 2026. Three sub-blocks (4-0 through 4-2), three feat
commits plus this devlog. The kernel's first GPU: find a virtio-gpu
device, bring up its 2D command protocol, and present a framebuffer
through a scanout — ~517 LOC, native, no shim.*

virtio-gpu was inserted into the M1 plan at the step-1 kickoff, and
it is the one step that exists for a structural reason rather than a
hardware one. **QEMU does not emulate amdgpu.** Without virtio-gpu,
the amdgpu step (5) would have no per-commit CI substrate — it would
develop against real Framework hardware only, regression-blind
between sessions. virtio-gpu fixes that: a clean, faithful virtual
GPU QEMU smokes on every commit, and a kernel-side display
vocabulary that amdgpu's KMS output will target.

Two things the step shipped:

- **A working 2D GPU driver.** Find the device (modern PCI id
  0x1050), run the virtio init dance, and drive the virtio-gpu 2D
  command protocol over the control queue: `GET_DISPLAY_INFO` →
  `RESOURCE_CREATE_2D` → `RESOURCE_ATTACH_BACKING` → `SET_SCANOUT` →
  `TRANSFER_TO_HOST_2D` → `RESOURCE_FLUSH`. A painted framebuffer
  reaches the scanout; `ARSENAL_GPU_OK` fires on the flush.

- **The display vocabulary amdgpu will consume.** `display.rs` —
  `DisplayInfo` (geometry + format) and `PixelFormat` — plain data
  any display backend populates. This is the higher-stakes
  deliverable than the driver, and the one the step deliberately
  kept small. The decision on *how much* abstraction to commit to is
  the most interesting thing about the step; it gets its own section
  below.

The driver came in at ~517 LOC against a ~1000–1500 budget. Three
reasons: the virtio transport was already built (this is the third
native virtio driver, after virtio-blk and virtio-net at M0), the 2D
command subset is bounded, and the abstraction stayed minimal by
design.

## What landed

Three commits between the step-4 HANDOFF (in the M1-3-final commit
`c3258bd`) and this close-out:

- `0ea2814` *feat(gpu): M1-4-0 — virtio-gpu transport bring-up +
  GET_DISPLAY_INFO.* Find the modern-only device (PCI id 0x1050 —
  virtio device type 16 + 0x1040; unlike blk's 0x1001 / net's 0x1000
  there is no transitional id), run `virtio::init_transport`
  declining every optional feature (no VIRGL/3D, no EDID — VERSION_1
  only, the virtio-blk posture), activate the control virtqueue
  (queue 0), bring the device `DRIVER_OK`, then issue the first
  command — `GET_DISPLAY_INFO` — and parse the first enabled
  scanout's rectangle. Observed: scanout 0 enabled at 1280×800,
  `num_scanouts=1`.

  `display.rs` lands here with `DisplayInfo` + `PixelFormat`.
  `GET_DISPLAY_INFO` was folded into 4-0 (the HANDOFF sketch had it
  at 4-1): defining the vocab type with no consumer would trip
  `clippy -D warnings` dead-code, and a bring-up that sends zero
  commands is a hollow bisect seam — reading the display info makes
  the vocab *used* and the control-queue round-trip *verified*.
  Smoke stays 21/21 (no new sentinel until 4-2, by design).

- `dd96e50` *feat(gpu): M1-4-1 — virtio-gpu 2D resource create +
  backing attach.* `RESOURCE_CREATE_2D` (resource id 1, format
  `B8G8R8X8_UNORM` matching `PixelFormat::Xrgb8888`, sized to the
  scanout) then `RESOURCE_ATTACH_BACKING` with one `mem_entry`. The
  framebuffer is a single `Box<[u32]>` heap allocation. Because the
  kernel heap is one physically-contiguous Limine region — the
  property virtio_blk already relies on for DMA — a single heap
  allocation is one contiguous backing, so one `mem_entry` suffices
  and there is no scatter-gather list. 1280×800×4 = 4,096,000 bytes,
  a quarter of the 16 MiB heap, allocated before smoltcp/rustls
  churn, so headroom is fine.

  This commit refactored 4-0's inline command path into the two
  helpers the rest of the driver rides: `submit()` (push the
  request/response descriptor chain, notify, poll the used ring) and
  `cmd_nodata<R>()` (box a `repr(C)` request body contiguous with its
  response header, submit, assert `RESP_OK_NODATA`). `DisplayInfoXfer`
  generalized to `Xfer<R, P>`. Smoke 21/21.

- `f6c6da3` *feat(gpu): M1-4-2 — virtio-gpu scanout + transfer +
  flush; ARSENAL_GPU_OK.* The present pipeline completes. Paint a
  known pattern into the backing (an Arsenal-navy field with a
  centered amber band, reusing `fb::NAVY` / `fb::AMBER` for visual
  identity), then `SET_SCANOUT` (bind resource 1 to scanout 0),
  `TRANSFER_TO_HOST_2D` (copy the backing to the host resource), and
  `RESOURCE_FLUSH` (present). All three return `OK_NODATA`.
  `ARSENAL_GPU_OK` fires after the flush; smoke is **22/22**.

  A note on the assertion, already in-code: a scanout has no
  read-back the way a block device reads a sector, so the sentinel
  asserts the command pipeline *completed*, not pixel contents. That
  is the honest limit of a headless GPU smoke. Manual `-display gtk`
  verification of the drawn pattern is this devlog's job, not CI's —
  flagged for a windowed run.

## The design decision: how much abstraction at n=1

The HANDOFF framed the display/GPU abstraction as "the step's real
deliverable" and asked for it to be proposed at 4-0, with options, as
a design review. With the code in front of me, the genuine tension
surfaced: the HANDOFF's stated reason for inserting step 4 was to
*stabilize the abstraction before amdgpu*, but CLAUDE.md says no
speculative abstraction — and at M1 the Limine LFB (write-through, no
flush) and a GPU (explicit transfer + flush) have genuinely different
present models. A trait papering over them with only one GPU in hand
would be designed against too little; it might not survive contact
with amdgpu.

Three shapes were on the table:

- **A.** A `Display` trait now, with `fb.rs`'s Limine LFB and
  virtio-gpu both refactored to implement it behind a global
  `dyn Display`. Most literally honors "abstraction before amdgpu,"
  but refactors working M0 framebuffer code and introduces dyn
  dispatch the headless smoke doesn't need.
- **B.** Shared *data* vocabulary — `DisplayInfo` + `PixelFormat` —
  with the driver concrete and standalone, `fb.rs` untouched, and the
  unifying trait deferred to step 5 when amdgpu makes it n=2 GPUs.
- **C.** A full KMS-like surface (multiple framebuffers, page-flip,
  damage rects) modeling the eventual compositor. Speculation against
  M2 requirements not yet pinned.

The call (recorded as the 4-0 decision) was **B**. The reasoning: the
*vocabulary* is the durable part and the *trait* is the cheap part to
add later, so the right move is to stabilize the vocabulary now and
design the trait when it is no longer designed against n=1. amdgpu
arriving as a second real GPU backend at step 5 is exactly that
moment. `fb.rs` — green, working M0 code — was left alone.

A consequence worth stating plainly: this means the HANDOFF's
"stabilize the abstraction before amdgpu" goal is *partially*
deferred. What stabilized is the vocabulary (`DisplayInfo`,
`PixelFormat`, and the create→attach→scanout→transfer→flush command
sequence as the worked reference). What waits is the trait. That is
the honest scope, and it is the more defensible engineering line than
shipping a trait fitted to one driver.

## 4-3 was skipped

The plan floated a 4-3 to wire `fb.rs` / the shell to render through
the virtio-gpu scanout instead of only the Limine LFB. It was
skipped. The CI-substrate goal (a KMS-capable GPU QEMU smokes every
commit) and the abstraction-vocabulary goal are both met at 4-2, and
a real `fb`→scanout consumer is genuinely M2 Stage work — there is no
consumer demanding it at M1. Building it now would be a feature
beyond what the step needs. Logged for M2.

## Numbers

| Sub | Commit  | Smoke | Note |
| --- | ------- | ----- | ---- |
| 4-0 | 0ea2814 | 21/21 | transport + GET_DISPLAY_INFO; display.rs vocab |
| 4-1 | dd96e50 | 21/21 | resource create + backing; submit/cmd_nodata helpers |
| 4-2 | f6c6da3 | 22/22 | scanout + transfer + flush; ARSENAL_GPU_OK |

`virtio_gpu.rs` 481 LOC + `display.rs` 36 LOC = ~517 at step exit,
against a ~1000–1500 budget. Final smoke 22/22, boot→prompt in the
~200 ms band, stable across repeated runs. The one new sentinel,
`ARSENAL_GPU_OK`, asserts the 2D present pipeline round-tripped end to
end.

## Foundation reuse

- **The whole M0 virtio transport** (`virtio.rs`): `find_device`,
  `init_transport`, `activate_queue`, `set_driver_ok`, `notify`, and
  `Virtqueue` (`push_chain` / `pop_used`). virtio-gpu added zero
  transport code — only the GPU command protocol on top. This is the
  dividend of the third virtio driver.
- **The heap as contiguous DMA memory** — a single `Box<[u32]>` as a
  physically-contiguous framebuffer backing, the property virtio_blk
  established.
- **`fb::NAVY` / `fb::AMBER`** for the test pattern's visual identity.

## What M1 step 5 looks like

Step 5 is **amdgpu KMS via the LinuxKPI shim** — the headlining M1
driver and the shim's hard scaling test. ADR-0009 (the xHCI
native-vs-port decision) deliberately concentrated the LinuxKPI-port
budget here: xHCI went native precisely so the shim's first
complex-driver port is amdgpu, where it is unavoidable, rather than
spent twice.

The shim is proven for a *simple* inherited driver — virtio-balloon,
~600 LOC, pure virtio-bus interaction. amdgpu is a different order of
magnitude: ~10K+ LOC of inherited C pulling DRM/KMS, GEM/TTM, dma-buf,
fences, i2c/aux + EDID, firmware loading, and a large MMIO/IRQ
surface. ADR-0006's warning — that the header closure grows
super-linearly with driver complexity — is about to be tested at its
limit. amdgpu is KMS-only at M1 (no Vulkan).

The thing step 4 bought step 5: amdgpu cannot smoke under QEMU, so it
develops against real Framework 13 AMD hardware, with the per-commit
QEMU gate staying green on virtio-gpu. And amdgpu's KMS output has a
kernel-side display vocabulary to target — `display::DisplayInfo` /
`PixelFormat` — rather than co-designing it with the hardest driver.
When amdgpu lands, it becomes the second GPU backend, and *that* is
when the `Display` trait deferred at 4-0 gets designed against two
real implementations. The next session writes the step-5 HANDOFF.

## Cadence note

This is the twelfth devlog of the post-pivot arc (M0's eight, NVMe's
one, the step-2 cluster's three, step 3's one, now step 4's one).
Step 4 closed in a single focused session — the lightest M1 step so
far, because it was the third instance of a known pattern (virtio)
plus a bounded command set.

Four M1 steps have now closed inside the post-pivot concentration
window. The honest read is unchanged: the steps that went fast went
fast for structural reasons (faithful QEMU devices, bounded specs,
reused transport), and the M1 budget does not shrink because of it.
Step 5 is where that stops applying. amdgpu is real-hardware work —
inherited C through a shim that has never hosted a complex driver,
debugged on actual silicon, with a header closure that ADR-0006
predicts will be large. It is the step the whole M1 calendar variance
has been pointing at. The right posture going in is the one CLAUDE.md
names: estimate honestly, treat the compile-error iteration as the
unbudgetable part, and when a single bug owns multiple sessions,
write up what was tried and step away for a day.

M1 step 4 is complete. Onward to amdgpu.
