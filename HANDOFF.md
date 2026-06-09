Kickoff for M1 step 5 — amdgpu KMS via the LinuxKPI shim: the
headlining M1 driver, the shim's first complex-driver port, and the
step the whole M1 calendar variance has been pointing at. This step
does not start with code. It starts with a gate.

## Where we are

M1 step 4 (virtio-gpu, native Rust) closed on 2026-06-08. The kernel's
first GPU: find the modern-only device (PCI id 0x1050), bring up the
2D command protocol over the control queue, and present a framebuffer
through a scanout — GET_DISPLAY_INFO → RESOURCE_CREATE_2D →
RESOURCE_ATTACH_BACKING → SET_SCANOUT → TRANSFER_TO_HOST_2D →
RESOURCE_FLUSH — in ~517 LOC (`virtio_gpu.rs` 481 + `display.rs` 36),
native, no shim. `ARSENAL_GPU_OK` fires on the flush. The step-4 devlog
(`docs/devlogs/2026-06-arsenal-virtio-gpu.md`) tells the arc.

The thing step 4 bought step 5: a kernel-side display vocabulary
(`display::DisplayInfo` / `PixelFormat`) that amdgpu's KMS output now
targets, rather than co-designing it with the hardest driver; and a
GPU that QEMU smokes on every commit, so amdgpu — which QEMU cannot
emulate — develops with the per-commit gate staying green underneath
it. Step 4 deliberately deferred the cross-backend `Display` trait to
step 5, when amdgpu makes it n=2 GPU backends. That trait is now due,
and designable against two real implementations.

HEAD is the M1-4-final commit (this STATUS + devlog + HANDOFF);
working tree clean once it lands. Smoke is **22/22** (`ARSENAL_GPU_OK`
is the step-4 sentinel). Push before 5-0 kicks off so the step-4 arc
lands on origin as one push (the user pushes each sub-block).

## The honest framing before anything else

The four fast M1 steps (NVMe, the shim foundation, xHCI, virtio-gpu)
all went fast for structural reasons: faithful QEMU devices, bounded
specs, a virtio transport reused three times, a native bring-up that
round-tripped first try. **None of those properties hold for amdgpu.**
This is the step the post-pivot concentration window does not cover.
Three things make it categorically different from everything M1 has
shipped:

1. **The shim has never hosted a complex driver.** It is proven for
   virtio-balloon — ~600 LOC of inherited C, pure virtio-bus
   interaction, one virtqueue, a stats report. amdgpu is ~10K+ LOC of
   *its own* code (the DC display core is hundreds of thousands more),
   pulling DRM/KMS, GEM/TTM memory management, dma-buf and fences,
   i2c/aux + EDID, firmware loading, and a large MMIO + interrupt
   (IH ring) surface. ADR-0006's central warning — header closure
   grows *super-linearly* with driver complexity (balloon alone pulled
   281 transitive headers before the BFS halted) — is about to be
   tested at its limit. This is exactly where ADR-0009 deliberately
   concentrated the LinuxKPI-port budget: xHCI went native so the
   shim's first hard port is amdgpu, where it is unavoidable, not
   spent twice.

2. **QEMU cannot smoke it.** There is no amdgpu emulation. The 22/22
   QEMU smoke stays green and keeps guarding everything *except*
   amdgpu — NVMe, xHCI, virtio-gpu, the shim, the scheduler, the net
   stack. **Do not conflate a passing smoke with a working amdgpu.**
   amdgpu gets no QEMU sentinel. Its validation is a manual
   observation on real Framework 13 AMD hardware — a picture on the
   panel, or a serial/console marker emitted from real silicon — and
   it is recorded in the step-5 devlog, the way the M0 framebuffer and
   SMP devlogs recorded manual display checks, never as a CI gate the
   cloud runner can assert.

3. **It entangles with step 7.** Step 7 is "first boot on real
   Framework 13 AMD hardware" — the M1 exit criterion. amdgpu cannot
   be validated without running on that hardware, which means step 5's
   validation *needs* step-7 infrastructure (UEFI boot off USB on the
   Framework, and a console channel off real hardware — the Framework
   13 has no RS-232 header, so this is USB-serial, a framebuffer
   console, or a network console, none of which exist yet). How much
   of step 7 pulls forward into step 5 is a real sequencing decision,
   not a detail. It is one of the three questions the 5-0 scope ADR
   puts to you.

## Read before proposing

read CLAUDE.md (peer concerns; Rust-only base with the **one
exception** that matters most here — inherited Linux drivers in their
original C under the LinuxKPI boundary, GPLv2 preserved, shipped as a
combined work with explicit license boundaries, the FreeBSD drm-kmod
pattern; BSD-2 base; no `unsafe` without a `// SAFETY:` comment; build
loop sacred) → STATUS.md (M1 step 4 complete, step 5 active; the
step-4 design decision — shared `display.rs` data vocabulary, trait
deferred to step 5 — is load-bearing for 5-0; the step-3 carry-forward
#1 about explicit PCI Bus Master Enable on real hardware and the
balloon round-22d BME trap both apply to amdgpu on real silicon) →
docs/plan/ARSENAL.md § "M1 — Real iron" (amdgpu KMS is the milestone's
named GPU deliverable; read the performance gate — cold boot to login
< 8 s — which is asserted at step 7 on real hardware, and amdgpu modeset
time is part of that budget) and § the compositor/Stage rows (the
`Display` trait designed here is what Stage sits on at M2, so its shape
outlives this driver) → **ADR-0006** (`docs/adrs/0006-*.md` — headers
are the shim; closure grows super-linearly; the `shim_c.h`-is-the-
surface discipline; this is the single most important prior for the
5-0 closure audit) → **ADR-0009** (`docs/adrs/0009-xhci-native-rust.md`
— the native-vs-port spike methodology and *why the port budget was
concentrated on amdgpu*) → ADR-0005 (the shim's structural decisions:
single `linuxkpi/` member, `cc`-crate compile from `build.rs`,
directory-based GPL/BSD-2 boundary) → linuxkpi/include/ (the BSD-2
Arsenal-authored Linux API surface as it stands after balloon — every
header amdgpu needs that is absent is step-5 work) → linuxkpi/src/
(pci.rs, irq.rs, virtio.rs, mm.rs, page.rs, workqueue.rs, locks.rs —
the shim modules; amdgpu will demand large extensions to most and
several wholly new ones: dma-buf, fence/dma_resv, i2c/aux, the DRM/KMS
core) → linuxkpi/build.rs (the source manifest + the Kbuild flag set:
`-nostdinc`, the clang resource-dir `-isystem`, `-mno-sse -mno-mmx
-mno-avx -msoft-float`, per-TU `-DKBUILD_MODNAME`, the `ar`-crate +
`+whole-archive` link path) → arsenal-kernel/src/display.rs (the
vocabulary the trait extends) + virtio_gpu.rs + fb.rs (the two
existing display backends the trait must generalize over) →
docs/devlogs/2026-05-arsenal-linuxkpi-shim.md + the GPL-boundary devlog
(how the combined-work model actually works in this tree) → git log
--oneline -12 → run the sanity check below → **produce the 5-0 gate
deliverables (audit + recon + scope ADR), do not write driver code** →
bring the three scope decisions back for the pick.

## What step 5 is — and the gate it starts with

amdgpu KMS, modeset only (no Vulkan, no 3D, no compute — ARSENAL.md
scopes amdgpu KMS-only at M1, the same 2D-scanout posture virtio-gpu
held). The end state is: the Framework 13's internal panel lit by
Arsenal's own kernel, driving the AMD display controller, at native
resolution.

But the first deliverable is **not code**. It is a gate — call it
**M1-5-0** — because the wrong scope decision here is the difference
between a 4-week step and a 6-month one, and that decision cannot be
made responsibly without two pieces of recon the tree does not yet
have. The gate has three parts:

**(1) An ADR-0006-style header/LOC closure audit, amdgpu KMS-only.**
Run the same instrument ADR-0006 ran on balloon, scoped to the KMS
path (drivers/gpu/drm/amd/amdgpu + drivers/gpu/drm/amd/display, minus
the GFX/compute/VCN/Vulkan subtrees). Produce: the transitive header
closure size, the inherited-`.c` LOC under a KMS-only build config, and
the list of shim subsystems amdgpu needs that do not exist yet
(DRM/KMS core, GEM/TTM, dma-buf, dma_resv/fence, i2c/aux, the IH
interrupt model, firmware request). This is the number that sizes the
step. ADR-0006 says it will be large; quantify *how* large before
committing to a path.

**(2) Hardware + firmware recon on the actual Framework 13 AMD.**
Which APU is in the target unit — Ryzen 7040 "Phoenix" (RDNA3, DCN
3.1.4, gfx1103) or Ryzen AI 300 "Strix Point" (RDNA 3.5)? The DCN
version determines which DC code paths and which firmware blobs are in
play. Enumerate the required `amdgpu/*.bin` firmware for a *modeset*
(at minimum the DMCUB display microcontroller, the PSP sos/ta secure-
boot firmware, SMU/PMFW for clocks, plus GC/SDMA as the closure
demands) — these are GPLv2/redistributable-firmware blobs that ship
*alongside* the combined work, not inside it; their licensing and
packaging is part of the recon. Confirm the panel's native mode and
the connector topology (the internal eDP).

**(3) A scope ADR putting three decisions to the user.** Do not pick
any of these silently — each is a peer-concerns-weighted call:

  - **(a) How much amdgpu to inherit.** Three shapes:
    - **Full DC.** Inherit drivers/gpu/drm/amd/display/dc + amdgpu_dm
      and run amdgpu's real atomic modeset. Maximum fidelity (every
      panel, DP-MST, HDR, PSR), maximum shim surface — the DC core is
      the largest single body of display code in the kernel. This is
      "amdgpu, properly," and the multi-month tail lives here.
    - **Minimal modeset.** Inherit only enough amdgpu to program the
      display controller for one fixed mode on the internal eDP. The
      honest catch: amdgpu's DC is monolithic and was not built to be
      sliced this way — "minimal amdgpu modeset" may not be a small
      thing, and the closure audit (part 1) is what tells us whether
      this option is real or a mirage.
    - **Intermediate — native KMS over the firmware framebuffer
      (GOP/simpledrm-style).** Get a picture on the Framework's panel
      *cheaply and first* by taking over the framebuffer UEFI GOP
      already set up at boot (the efifb/simpledrm pattern: the
      firmware lit the panel; Arsenal inherits that linear framebuffer
      and draws to it natively, no amdgpu, no DC, no shim), then pursue
      real amdgpu modeset as a separable follow-on. This de-risks
      "Arsenal draws on real Framework hardware" to near-term and
      isolates the amdgpu-DC question from the first-light question.
      It is the smaller version worth surfacing per CLAUDE.md, and it
      changes what step 7 needs. Its cost: GOP gives you the
      firmware's mode, no mode-setting, no second display, no power
      management — a picture, not a driver.

  - **(b) The shape of the `Display` trait, now due.** Step 4 deferred
    it deliberately to n=2 GPU backends. It is now designable against
    the real set: the Limine LFB (write-through, no flush — fb.rs),
    the virtio-gpu scanout (explicit transfer + flush), and amdgpu
    (atomic page-flip) — and, if option (a)-intermediate is chosen, the
    GOP/simpledrm framebuffer (write-through again). Propose the
    minimal trait all real backends satisfy — resolution + a writable
    framebuffer + a present/flush call — and explicitly *not* a
    KMS-like surface (page-flip queues, damage, multi-plane); that is
    M2 Stage speculation, the same over-abstraction step 4's option C
    rejected. This is the durable deliverable; flag it for design
    review the way 4-0's vocabulary was flagged.

  - **(c) How much step-7 infrastructure pulls forward.** amdgpu's
    validation needs real-hardware boot + a console off the Framework.
    Decide: does step 5 build the minimum step-7 boot/console infra it
    needs to validate (USB-serial or a framebuffer console), accepting
    that step 5 and step 7 partially merge — or does step 5 develop
    against the closure/compile gate alone and defer *all* on-silicon
    validation to step 7, accepting that amdgpu ships "compiles and is
    wired" but unproven until then? The intermediate option (a) makes
    this cheaper (a GOP framebuffer console is itself first-light); the
    full-DC option makes it heavier.

## Validation model (read carefully — it is unlike every prior step)

Every M1 step so far had a QEMU sentinel: the device round-tripped, the
smoke asserted it, the cloud runner gated on it. **amdgpu has none of
that.** The model splits cleanly and the two halves must not be
conflated:

- **The QEMU smoke (22/22) stays green and guards everything except
  amdgpu.** It is the regression net for the rest of the kernel while
  amdgpu is built. A passing smoke says nothing about amdgpu. amdgpu
  adds *no* sentinel to REQUIRED_SENTINELS. If the closure-audit path
  produces compile-time shim self-tests (the way balloon's shim
  primitives got `ARSENAL_LINUXKPI_OK` coverage), those *can* smoke —
  but they assert the shim, not the driver.

- **amdgpu's validation is manual, on real Framework 13 AMD hardware,
  recorded in the devlog.** The observable is a picture on the panel
  (a known pattern, the navy field + amber band virtio-gpu used is the
  natural reuse) or a serial/console line from real silicon confirming
  modeset completed. This is the honest limit and it is stated plainly,
  the way the headless virtio-gpu smoke's "command pipeline, not
  pixels" caveat was stated in qemu-smoke.sh and the step-4 devlog.

## Sub-block plan (proposed — argue with it at 5-0, and expect it to
change based on the scope pick)

**(M1-5-0) The gate.** Closure audit + hardware/firmware recon + scope
ADR with the three decisions above. **No driver code.** Output is a
committed ADR (`docs/adrs/NNNN-amdgpu-kms-scope.md`) and the three
picks. This is the design-review seam, and the most important one in
M1. Bisect seam.

Everything below 5-0 is *conditional on the scope pick* and is sketched,
not committed — the real decomposition is written after 5-0:

**If (a)-intermediate (GOP/simpledrm) is picked first:**
  - 5-1: inherit the UEFI GOP framebuffer at boot, native Rust, draw
    the known pattern on real hardware → first light on the Framework.
  - 5-2: land the `Display` trait (decision b) with fb.rs, virtio-gpu,
    and the GOP backend as its three implementors.
  - then the amdgpu-DC port (full or minimal per a) proceeds as 5-3+
    with first-light already in hand and the trait already shaped.

**If full-DC or minimal-modeset amdgpu is picked directly:**
  - 5-1: the shim closure foundation — the new subsystems the audit
    found (DRM/KMS skeleton, GEM/TTM, dma-buf, fence, i2c/aux), each
    landing + compiling + self-testing the way the balloon shim
    rounds did. This is the multi-session, unbudgetable compile-error
    iteration ADR-0006 describes, at amdgpu scale.
  - 5-N: amdgpu.c family into build.rs's manifest; compile-error
    iteration until it builds; firmware loading; modeset against the
    real panel; the `Display` trait (decision b) landed once amdgpu is
    the second real GPU backend.

**(M1-5-final)** STATUS refresh + step-5 devlog + step-6 HANDOFF
(iwlwifi + mac80211 via LinuxKPI). Per the established close.

## Foundation step 5 reuses

- **The entire LinuxKPI shim** (`linuxkpi/`): the PCI bus adapter,
  IRQ dispatcher pool, DMA-coherent allocator, the virtio bus, the mm
  surface (struct page per ADR-0007, alloc_pages, page_address), the
  cooperative workqueue runner (ADR-0011), locks, lists, err, time,
  the `cc`-crate build path, and the `shim_c.h`-is-the-surface
  discipline (ADR-0006). amdgpu extends all of it heavily and adds new
  subsystems, but the foundation, the build loop, and the GPL/BSD-2
  boundary are proven.
- **ADR-0006's methodology**: the recursive vendor-fetch closure walker
  (preserved in git history at `b2dd46f`) is the exact instrument for
  the 5-0 audit's part 1.
- **The PCI MSI-X + dynamic IDT vector path** (from NVMe 1-0, reused by
  every M1 driver) for amdgpu's interrupt (IH ring) wiring.
- **The step-3/balloon BME trap**: PCI Bus Master Enable is a hard
  precondition for MSI delivery *and* for any DMA. amdgpu does heavy
  DMA; the shim's `register_*`/probe path already sets BME (balloon
  round-22d fix) — confirm amdgpu's path inherits it.
- **`display::DisplayInfo` / `PixelFormat`** (step 4) — the vocabulary
  amdgpu's modeset output populates and the trait (decision b)
  extends.
- **`fb::NAVY` / `fb::AMBER`** for the on-hardware test pattern, the
  same visual identity virtio-gpu's scanout used.

## Spec-fragile / risk pieces to watch

- **The closure is the risk, not any single command.** Unlike
  virtio-gpu (a bounded 2D command set), amdgpu has no small spec to
  transcribe correctly. The risk is the *size and shape* of the
  inherited surface, which is why 5-0 quantifies it before any code.
- **DC is monolithic.** "Minimal amdgpu modeset" may not exist as a
  small thing; the audit decides whether option (a)-minimal is real.
- **Firmware loading is a new shim subsystem.** amdgpu requires signed
  firmware blobs (DMCUB, PSP, SMU); the shim has never loaded firmware.
  The request_firmware path + where the blobs live in the Arsenal image
  + their redistribution licensing is net-new and recon'd at 5-0.
- **i2c/aux + EDID** to read the panel's mode is a subsystem the shim
  lacks. The intermediate GOP option sidesteps it (firmware already
  read the EDID); the amdgpu path does not.
- **The IH (interrupt handler) ring model** differs from the per-vector
  MSI-X the shim does today; amdgpu multiplexes many interrupt sources
  over a ring. New shim shape.
- **Real-hardware DMA is not QEMU DMA.** Cache coherency, IOMMU, and
  the BME precondition all bite on silicon in ways QEMU forgives
  (step-3 carry-forward #1, the balloon BME trap). The first time
  Arsenal's shim DMA runs on real AMD silicon is in this step.
- **The `Display` trait at n=2 GPUs is still a design risk**, just a
  smaller one than at n=1. Over-abstracting toward a KMS surface
  (step 4's rejected option C) is the trap; under-abstracting leaks
  amdgpu specifics into the trait. Minimal common surface, flagged for
  review.

## Estimates and cadence

This is where the M1 calendar variance lives, and it always has. The
milestone budget (~67 part-time weeks across 9 steps; ARSENAL.md months
9-24) was structured with the harder steps — shim, **amdgpu**, real-
hardware bring-up — holding the variance precisely because the early
steps could not. **Four fast steps do not shrink amdgpu's estimate.**
ADR-0006 already proved the closure grows super-linearly: balloon's 281
headers are the floor, not the model, for a ~10K+ LOC driver pulling
DRM/KMS/DC.

The compile-error iteration (the balloon sub-task-3 pattern: each round
resolves one missing type/macro/function by extending a BSD-2 header or
shim module, `main` green at every commit) is the unbudgetable part, at
a scale balloon only previewed. Treat 5-1+ as multi-month, not multi-
session. Per CLAUDE.md's working-hours posture: this is the step where
"a single bug owns multiple sessions" is the *expected* texture, not the
exception — and the explicit cue applies: when it does, write up what
was tried and step away for a day. Every gap-filling sub-block carries a
`wip:` branch as the partial-work checkpoint.

The intermediate (GOP) scope option exists partly as morale and risk
management: it puts a real picture on real Framework hardware *early*,
which is both a genuine milestone and a hedge against the amdgpu-DC tail
being as long as ADR-0006 predicts. Surface it as the smaller-first
option; let the user weigh it against full-fidelity amdgpu.

## Sanity check before kicking off

    git tag --list | grep arsenal     # arsenal-M0-complete
    git log --oneline -6              # M1-4-final (HEAD), f6c6da3,
                                      # dd96e50, 0ea2814, c3258bd,
                                      # 8fcd986
    git status --short                # clean (or ?? HANDOFF.md while drafting)
    cargo build -p arsenal-kernel --target x86_64-unknown-none --release
    cargo clippy -p arsenal-kernel --target x86_64-unknown-none --release -- -D warnings
    cargo xtask iso                   # arsenal.iso ~19.4 MB
    ci/qemu-smoke.sh                  # ==> PASS (22 sentinels)

Expected: smoke PASSes with 22 sentinels; boot→prompt ~110-235 ms;
`ARSENAL_GPU_OK` fires. (This is the regression net for the rest of the
kernel; it does not and will not assert amdgpu.)

## Out of scope for step 5 specifically

- **Vulkan / 3D / compute / VCN (video).** amdgpu is KMS modeset only
  at M1, per ARSENAL.md — the same scope-discipline virtio-gpu held
  (2D only, no virgl). The GFX/compute/Vulkan/VCN subtrees are
  explicitly *excluded* from the 5-0 closure audit.
- **DP-MST / external displays / multi-monitor.** The internal eDP
  panel only for the step. (Full-DC inherits the capability; minimal/
  GOP do not target it.)
- **HDR / PSR / variable refresh / display power management.** Modeset
  to a stable native picture is the bar.
- **The compositor (Stage).** M2. Step 5 ships the `Display` trait
  Stage sits on, not Stage.
- **GPU acceleration of the existing fb/shell.** As in step 4, no
  consumer demands it at M1; M2 Stage work.

## Permanently out of scope (do not propose)

- Any `unsafe` block without a `// SAFETY:` comment naming the
  invariant. amdgpu DMA + MMIO + firmware is unsafe-dense; every site
  needs it.
- Reverting any closed/tagged M0 or merged M1 commit. Force-pushing to
  origin.
- Dropping the BSD-2 SPDX header from any Arsenal-base file, or the
  GPLv2 header from any inherited `.c`. The combined-work boundary is
  non-negotiable (CLAUDE.md §3; the drm-kmod pattern). amdgpu's C stays
  GPLv2 under `vendor/`; the shim stays BSD-2 under `linuxkpi/`.
- Pulling amdgpu's C into the Rust base, or "rewriting amdgpu in Rust."
  It is inherited under the LinuxKPI boundary; that is the whole point
  of the shim (ADR-0004, ADR-0005).
- Conflating a passing QEMU smoke with a working amdgpu. They are
  independent; say so wherever it could be misread.
- Religious framing; reintroducing HolyC; going back to stable Rust.
- Skipping the build + smoke loop on any commit that touches kernel or
  shim code (amdgpu's own C is the exception that proves it — it cannot
  smoke, hence the manual-validation model above).

## First action

**Start M1-5-0: the gate, not the driver.** Run the ADR-0006 closure
instrument against amdgpu's KMS path and produce the header/LOC closure
number and the missing-shim-subsystem list. Do the Framework 13 AMD
hardware + firmware recon (APU/DCN version, the modeset `amdgpu/*.bin`
set, the eDP native mode). Then write the scope ADR
(`docs/adrs/NNNN-amdgpu-kms-scope.md`) laying out the three decisions —
(a) full-DC vs minimal-modeset vs intermediate-GOP, (b) the `Display`
trait shape at n=2 backends, (c) how much step-7 boot/console infra
pulls forward — each with options and a recommendation, none picked
silently. Bring all three back for the design review before any driver
or shim code is written. The QEMU smoke stays 22/22 and green
throughout; it is the net under the rest of the kernel, not a gate on
amdgpu. This is the step where the estimate discipline matters most:
quantify the closure first, then decide the path, then build.
