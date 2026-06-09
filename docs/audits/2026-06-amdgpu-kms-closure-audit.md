# M1-5-0 closure audit — amdgpu KMS via the LinuxKPI shim

*June 9, 2026. Part 1 of the M1-5-0 gate (closure audit + hardware
recon + scope ADR). This is the measured input the scope ADR cites; it
does not make the scope decision. It runs the same instrument
[ADR-0009](../adrs/0009-xhci-native-rust.md) Spike 1 ran for the xHCI
port (655 headers, ~15.4k LOC) and [ADR-0006](../adrs/0006-linuxkpi-headers-are-shim.md)
ran for virtio-balloon (281-header closure against a ~20 estimate),
now against amdgpu's KMS path.*

## Method

A blobless sparse checkout of `torvalds/linux` at tag **v6.12**
(`git clone --filter=blob:none --depth 1 --branch v6.12`, sparse cone
`drivers/gpu/drm include arch/x86/include`), analyzed on-disk. LOC is
raw `wc -l` over `.c` / `.h` files (whole-file line count, comments and
blanks included — the vendoring surface, not SLOC). The API-header
surface is the distinct set of `#include <...>` angle-bracket
directives across the KMS source set; quoted (`"..."`) local includes
resolve inside the vendored tree and are not counted as shim surface.
On-disk static extraction rather than a network BFS because amdgpu's
closure is large enough that per-file fetches would run to thousands of
requests — the point ADR-0006 predicted ("amdgpu would pull headers
into the thousands").

**KMS-only scope** = `drivers/gpu/drm/amd/{amdgpu,display,pm,include}`
+ the DRM core a modeset driver registers against
(`drivers/gpu/drm/*.c`, `ttm/`, `scheduler/`, `include/drm`,
`include/uapi/drm`). **Excluded**: `amdkfd` (compute, 34,253 LOC),
`acp` (audio co-processor, 50 LOC), and — within `amdgpu/` itself —
nothing, because the directory is not cleanly separable (see Finding 4).

## Finding 1 — inherited `.c` source to vendor: ~925k LOC

| Subtree | `.c` files | `.c` LOC |
| --- | ---: | ---: |
| `amd/amdgpu` (device core + IP blocks) | 276 | 286,853 |
| `amd/display/dc` (Display Core) | 428 | 378,664 |
| `amd/pm` (power / clocks — needed for display) | 81 | 129,684 |
| `amd/display/amdgpu_dm` (DRM↔DC bridge) | 16 | 27,735 |
| `amd/display/modules` (freesync/color/hdcp) | 14 | 10,661 |
| `amd/display/dmub` (display microcontroller iface) | 17 | 4,795 |
| `drm` core (`*.c`, non-recursive) | 83 | 75,173 |
| `drm/ttm` (memory manager) | 20 | 9,559 |
| `drm/scheduler` | 3 | 2,317 |
| **Total (KMS-only)** | **938** | **925,441** |

For scale: this is **~755× virtio-balloon** (1,223 LOC, the shim's only
shipped inherited driver) and **~30× the xHCI LinuxKPI port** (~30k LOC
of `.c`) that ADR-0009 already rejected as too large and took native
instead. The xHCI port was deemed an "amdgpu-scale shim explosion"
not worth paying for a rewritable driver; this audit confirms amdgpu is
that scale, one order of magnitude up, and is *not* rewritable — which
is exactly why ADR-0009 concentrated the port budget here.

## Finding 2 — API-header surface to reimplement: ~200 distinct headers

The KMS source set issues **216 distinct `#include <...>` directives**.
Under ADR-0006 these are *not* vendored — each is reimplemented as a
BSD-2 header under `linuxkpi/include/`. By prefix:

| Prefix | distinct headers | shim has today |
| --- | ---: | --- |
| `<linux/*>` | 121 | 13 header files + `shim_c.h` symbols |
| `<drm/*>` | **71** | **0 — the entire DRM/KMS surface is absent** |
| `<asm/*>` | 8 | partial (via `shim_c.h`) |
| `<uapi/*>`, `<sound/*>`, `<acpi/*>`, `<xen/*>`, misc | 16 | 0 |

The shim's current `linuxkpi/include/linux/` is 13 files
(`slab`, `mm`, `workqueue`, `wait`, `virtio*`, `delay`, `module`,
`oom`, `swap`, `page_reporting`, `balloon_compaction`, `types`), plus
the inline declarations in `shim_c.h`. amdgpu needs **121** distinct
`<linux/*>` headers — nearly an order of magnitude more API breadth
than everything the shim has accumulated across M1 steps 1–4 combined,
and **71 `<drm/*>` headers for which there is no shim code at all**.

## Finding 3 — register-data headers: 446 files / 4.58M LOC (vendor, not reimplement)

`amd/include` is 515 header files totaling **4,717,699 LOC** — but this
splits cleanly:

- `asic_reg/`: **446 files, 4,583,001 LOC** — auto-generated per-IP-block
  register offset (`*_offset.h`) and shift-mask (`*_sh_mask.h`) tables.
  This is hardware *data*, not API. It falls under ADR-0006 §3's
  transcription-risk logic (magic numbers where a typo is a silent
  correctness bug) → vendored verbatim, not reimplemented. Critically,
  **only the target ASIC's subset is needed** — the 4.58M LOC spans
  every AMD GPU generation; a single-target KMS port vendors a small
  fraction.
- non-`asic_reg` (`69 files, 134,698 LOC`): IP-discovery, ABI,
  ObjectID, atombios interface headers — mixed data + interface, mostly
  vendored.

So the "4.7M LOC of headers" number is real but mostly inapplicable
register data for ASICs Arsenal does not target. The decision-relevant
header work is Finding 2 (the ~200 API surfaces), not this.

## Finding 4 — DC is monolithic: "minimal modeset" is largely a mirage

The most important finding for scope decision (a). The Display Core
(`dc/`, 378,664 LOC) does **not** decompose into "one display block per
ASIC." Its weight is shared infrastructure that `dc_create()` pulls in
regardless of how few ASICs you target:

| `dc/` sub | `.c` LOC | nature |
| --- | ---: | --- |
| `dml` + `dml2` | 129,845 | floating-point display-timing math (DML) |
| `resource` | 42,667 | shared resource/pipe management |
| `hwss` (shared) | 22,298 | hardware sequencer |
| `dce` | 18,477 | shared display-controller engine |
| `link` | 17,914 | link/DP training |
| `core` | 16,789 | dc state machine |
| `bios` | 13,059 | atombios command-table parser |
| `gpio` | 8,747 | |
| `irq` | 7,364 | |
| `basics` | 5,062 | fixed-point math, logging |
| `virtual` | 362 | |
| **shared subtotal** | **~282,584** | needed for *any* DCN target |
| per-target-DCN slice (`resource/` + `hwss/`) | **2,522–3,939** | dcn314 / dcn35 / dcn351 |

The shared:per-ASIC ratio is roughly **75:1**. Targeting one DCN
(Phoenix `dcn314` or Strix `dcn35`/`dcn351`) saves you the *other* ~16
DCN slices — a few thousand LOC each — but the ~283k-LOC shared DC core
+ the ~130k-LOC DML floating-point timing library are non-optional. DML
in particular is the notoriously hard part: dense FP math that must run
under the shim's existing `-msoft-float` posture (CR4 has SSE off at M1
— the balloon round-22d `xorps` `#UD` lesson), at far greater volume
than anything the shim has compiled.

**Implication:** "minimal amdgpu modeset" as a small carve-out of DC
does not exist. The floor for a real amdgpu picture is ~283k LOC of
shared DC + DML + the amdgpu device core + amdgpu_dm. This is the
single strongest argument for the **intermediate GOP/simpledrm** scope
option getting first-light on real hardware cheaply, with the full DC
port as a separable, honestly-multi-month follow-on.

## Finding 5 — missing shim subsystems (the new-surface list)

Subsystems amdgpu's KMS path requires that the shim has **no code for
today**, grouped:

- **DRM/KMS core (all 71 `<drm/*>` absent):** `drm_drv`, `drm_device`,
  `drm_atomic` + `drm_atomic_helper` + `drm_atomic_state_helper`,
  `drm_crtc`/`drm_plane`/`drm_connector`/`drm_encoder` (+ their
  `_helper`s), `drm_framebuffer`, `drm_gem` + the GEM helpers,
  `drm_fb_helper`/`drm_fbdev_ttm`, `drm_edid`, `drm_vblank`,
  `drm_managed`, `drm_mm`/`drm_buddy`/`drm_suballoc`,
  `drm_print`/`drm_debugfs`, `drm_ioctl`/`drm_file`/`drm_auth`,
  `drm_syncobj`/`drm_exec`. This is the compositor-facing surface the
  M2 Stage Wayland path will also sit above — designed once, here.
- **DisplayPort / HDMI / DSC helpers:** `drm/display/drm_dp_helper`,
  `drm_dp_mst_helper`, `drm_dsc{,_helper}`, `drm_hdmi_helper`,
  `drm_hdcp_helper` — DP aux/link training, EDID, stream compression.
- **GEM/TTM buffer management:** `drm/ttm/ttm_{bo,device,resource,tt,pool,placement,range_manager,execbuf_util,caching}`
  (9,559 LOC of TTM `.c` to vendor) + the GEM-TTM glue. The shim's mm
  is `struct page` thin-handle (ADR-0007) + `alloc_pages` order-0 —
  TTM wants pools, placements, and eviction.
- **dma-buf / fence / reservation:** `<linux/dma-buf.h>`,
  `<linux/dma-fence.h>` (+ `-array`, `-chain`), `<linux/dma-resv.h>`,
  `<linux/sync_file.h>`. Cross-driver buffer sharing + the fence model
  the GPU scheduler and KMS flips synchronize on. Entirely new.
- **GPU scheduler:** `drm/gpu_scheduler` + `drm/scheduler/*.c` (2,317
  LOC) + `spsc_queue`/`task_barrier`. amdgpu submits through it.
- **i2c / aux bus:** `<linux/i2c.h>`, `<linux/i2c-algo-bit.h>` — DDC/EDID
  read and DP aux. No i2c in the shim today.
- **Firmware loading:** `<linux/firmware.h>` — `request_firmware` for the
  DMCUB / PSP / SMU blobs (recon'd in part 2). The shim has never loaded
  firmware; where the blobs live in the Arsenal image and their
  redistribution licensing is net-new.
- **Interrupt model (IH ring):** `<linux/irq.h>`, `<linux/irqdomain.h>` —
  amdgpu multiplexes many sources over an interrupt-handler ring, unlike
  the per-vector MSI-X the shim does today.
- **Supporting core breadth:** `<linux/{xarray,rbtree,kref,kthread,completion,scatterlist,vmalloc,dma-mapping,seq_file,debugfs,pm_runtime,suspend,reboot,backlight,component,hmm,mmu_notifier,iommu,acpi}.h>`
  — most needing real implementations, a few stubbable.

## Comparison across the M1 driver decisions

| Driver | inherited `.c` | API headers to reimplement | outcome |
| --- | ---: | ---: | --- |
| virtio-balloon (shipped) | 1,223 | ~30–50 (281 pre-ADR-0006 closure) | shim, online M1-2 |
| xHCI LinuxKPI **port** (rejected) | ~30,000 | 655 closure / ~200+ API | rejected → native (ADR-0009) |
| **amdgpu KMS-only (this step)** | **~925,000** | **~200 API + 446 register-data files** | the step 5 decision |

amdgpu is ~30× the port ADR-0009 already judged too large to take —
and it is the one driver that is genuinely *not* rewritable, which is
the whole reason the port budget was banked here. The audit's job is
not to relitigate native-vs-port (amdgpu is a port) but to size which
*port* — full DC, or a GOP-first intermediate.

## What this means for the scope ADR (not decided here)

The audit informs but does not settle the three M1-5-0 decisions:

- **(a) How much to inherit.** Finding 4 makes **minimal-modeset**
  largely illusory (DC's shared core + DML dominate; you cannot slice
  to one block). The real choice narrows to **full DC** (~925k LOC,
  multi-month, full fidelity) vs **intermediate GOP/simpledrm**
  (near-zero inherited `.c` — take over the firmware-initialized
  framebuffer for first-light, defer DC). The intermediate option's
  appeal rises sharply given Finding 4.
- **(b) The `Display` trait shape.** Now designable against the real
  backend set — Limine LFB (write-through), virtio-gpu scanout
  (transfer+flush), amdgpu (atomic flip), and, under option (a)-GOP, the
  GOP framebuffer (write-through). The 71-header DRM/KMS surface is the
  *Linux* abstraction; Arsenal's trait stays the minimal common one
  (resolution + writable framebuffer + present), not a DRM clone.
- **(c) Step-7 pull-forward.** Validation needs real Framework hardware
  + a console; the GOP option doubles as first-light and makes that
  cheap, the full-DC option makes it heavy.

## Part 2 (first cut) — target pinned: Ryzen AI 300 / Strix Point

Target ASIC confirmed as **Strix Point** (RDNA 3.5): GC 11.5.0, **DCN
3.5 / 3.5.1**, PSP 14.0.x, SDMA 6.1.x, SMU 14.0.x. Two consequences for
scope:

- **Strix is on the DML2 path, not legacy DML.** `dcn35_resource.c` and
  `dcn351_resource.c` both drive `dc/dml2` (the newer 51,309-LOC timing
  library), not the older `dc/dml`. DML2 is the larger and less-aged of
  the two FP timing libraries — a point *against* under-estimating the
  DC port, and a newer-silicon reminder that Strix's amdgpu paths have
  had fewer years of bug-shaking than Phoenix's `dcn314`.
- **A modeset on Strix needs the full GPU firmware set, not a display
  subset.** The `MODULE_FIRMWARE` strings the Strix IP versions request:
  `dcn_3_5_dmcub.bin` / `dcn_3_5_1_dmcub.bin` (display microcontroller),
  `psp_14_0_0_{toc,ta}.bin` (PSP secure-boot — gates loading everything
  else), the **full GC graphics microcode** `gc_11_5_0_{imu,me,mec,mes1,mes_2,pfp,rlc}.bin`,
  `sdma_6_1_0.bin` (paging DMA), and `smu_14_0_*.bin` (PMFW — display
  clocks/DPM). That is ~15 signed blobs. The GFX microcode is required
  even for pure display because amdgpu device init brings up the GFX
  engine unconditionally — a firmware-level confirmation of Finding 4's
  monolith result. The `.bin` files live in `linux-firmware` (a separate
  repo, redistributable under its own license); their packaging into the
  Arsenal image, and a new `request_firmware` + PSP secure-boot path the
  shim has never built, are net-new step-5 work.

  By contrast, the **GOP/simpledrm intermediate needs zero firmware** —
  UEFI lit the panel before hand-off. This is now the third independent
  axis (LOC, DC-monolith, firmware) all pointing the same way: GOP-first
  gets a picture on the Strix panel cheaply; full DC is the multi-month
  tail.

Still open in part 2: confirming the exact DCN minor (3.5.0 vs 3.5.1) and
`gc_11_5_0` vs `_1`/`_2` variant on the physical unit, the `.bin`
sizes/licenses in `linux-firmware`, and the eDP native mode/topology.

## Limits of this audit

- LOC is whole-file `wc -l` (the vendoring surface), not SLOC; the
  build-relevant subset under a real `.config` is smaller but not
  knowable without a working Kconfig resolution, which the shim does not
  emulate.
- The 216 API headers are the *directly-included* set; the transitive
  closure through the reimplemented headers is what actually gets
  written, bounded by ADR-0006's "shim grows by reading compile errors"
  discipline — i.e., the real shim-LOC number emerges from the
  compile-error loop, not from this static count.
- Part 2 (hardware/firmware recon) has a first cut above — target pinned
  to Strix Point (`dcn35`/`dcn351`, GC 11.5, DML2), firmware request set
  enumerated from source. What remains open needs the physical unit and
  the `linux-firmware` repo: exact DCN/GC minor variant, `.bin`
  sizes/licenses, eDP native mode/topology.

## Reproduce

```
git clone --filter=blob:none --no-checkout --depth 1 --branch v6.12 \
    https://github.com/torvalds/linux.git /tmp/linux-612
cd /tmp/linux-612
git sparse-checkout init --cone
git sparse-checkout set drivers/gpu/drm include arch/x86/include
git checkout
# LOC per subtree: find <path> -name '*.c' -exec cat {} + | wc -l
# API surface:     grep -hoE '#include[[:space:]]*<[^>]+>' over the .c/.h set
```
