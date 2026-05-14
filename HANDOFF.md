Kickoff for the next milestone — Arsenal M1, "real iron."

M0 closed at `arsenal-M0-complete` (commit 9793487, 2026-05-14)
across six steps and ~16 calendar days post-pivot. ARSENAL.md
M0 gates met: 96 ms boot→prompt vs 2000 ms target, zero unsafe
outside designated FFI boundaries, prompt keyboard-navigable
with `hw` summary. Working tree clean, branch in sync with
origin/main.

M1 per ARSENAL.md (months 9-24 of the original calendar plan)
is "real iron" — first boot on a Framework 13 AMD laptop, with
enough driver support (storage, USB, GPU framebuffer, wireless)
to be useful. Six concrete deliverables from § "Three Concrete
Starting Milestones":

  1. LinuxKPI shim layer in Rust, modeled on FreeBSD
     drm-kmod patterns. **ARSENAL.md flags this as the single
     largest engineering task — budget accordingly.**
  2. amdgpu driver under LinuxKPI, KMS only (no Vulkan).
  3. NVMe driver (native Rust, ~5K LOC per ARSENAL.md).
  4. xHCI USB driver (native Rust or LinuxKPI port —
     evaluate at start).
  5. iwlwifi + mac80211 via LinuxKPI.
  6. First boot on real Framework 13 AMD hardware.

Plus a Slint app running in a software-rendered framebuffer
(seventh deliverable, listed separately in ARSENAL.md).

ARSENAL.md M1 gates:
  - **Performance:** cold boot to login on Framework 13 AMD
    < 8 s.
  - **Security:** Linux drivers run with minimum required
    kernel capabilities; no shared kernel state beyond
    explicit shim interfaces.
  - **Usability:** Wi-Fi association via TUI works on first
    try.

This HANDOFF is *milestone-level* — it proposes a step
decomposition for all of M1, surfaces the trade-offs whose
resolution will shape the next 12+ months of work, and
recommends a starting point. Subsequent HANDOFFs (one per
step kickoff, following the M0 pattern) will be step-level
and overwrite this file.

read CLAUDE.md (peer concerns, Rust-only, BSD-2-Clause base,
build loop sacred, "use Limine"; the LinuxKPI / GPLv2 driver
boundary is the first M1 surface that exercises CLAUDE.md
§3's combined-work license discipline) → STATUS.md (M0
complete, M1 active with no sub-block yet; the five posture
changes carrying forward from M0 are load-bearing for any
M1 driver) → docs/plan/ARSENAL.md § "M1 — Real iron" → the
ADR-0004 pivot history at docs/adrs/0004-arsenal-pivot.md
(Rust-only commitment that constrains the LinuxKPI shim
shape; inherited driver C stays GPLv2 in the LinuxKPI shim
boundary, our shim is Rust + BSD-2) → arsenal-kernel/src/
(at M0 exit: 22 .rs files, ~5,900 LOC, ELF 1.52 MB; M1
step 1 adds ~1 file per driver subsystem, expect ELF to
2.5-4 MB range by M1 mid-point) → docs/devlogs/
2026-05-arsenal-smp.md § "What M1 looks like" (the M0
exit devlog enumerates the M1 surface with one sentence
per item; this HANDOFF expands each into a step) →
git log --oneline -20 → run the sanity check below →
propose step decomposition (or argue for a different
shape) → wait for me to pick the first step → "go step 1"
for code (or whatever the first step shorthand becomes),
"draft step 1 HANDOFF" for the step-level kickoff document.

Where the project is

  - HEAD: 9793487 (docs(devlogs): Arsenal SMP + M0 milestone
    exit). Tagged arsenal-M0-complete. Working tree clean.
    In sync with origin/main. The Field OS arc's
    M0-complete / M1-complete / M2-complete tags coexist on
    earlier commits (the C arc, preserved at field-os-v0.1).

  - Kernel: 22 .rs files (acpi, apic, cpu, fb, fb_font,
    frames, gdt, heap, idt, ioapic, irq, kbd, main, net,
    paging, pci, rand, sched, serial, shell, smp, task,
    virtio, virtio_blk, virtio_net) at ~5,900 LOC plus the
    Spleen font + small smoke harness. ELF release 1.52 MB,
    ISO 19.3 MB.

  - Smoke: 13 required sentinels, ~1.2-1.5 s on QEMU TCG
    with -smp 4. Boot→prompt 94 ms (budget 3000 ms). The
    sentinel list at M0 exit:
    ARSENAL_BOOT_OK / HEAP_OK / FRAMES_OK / BLK_OK /
    NET_OK / SCHED_OK / TCP_OK / TLS_OK / TIMER_OK /
    ACPI_OK / IOAPIC_OK / SMP_OK / PROMPT_OK.

  - Toolchain: nightly-2026-04-01 pinned in
    rust-toolchain.toml. M1's first new toolchain dependency
    is likely Slint at step 7; the rest of M1 is driver work
    that doesn't push the toolchain frontier.

  - Vendored crates at M0 exit: limine 0.5,
    linked_list_allocator 0.10, spin 0.10, x86_64 0.15,
    smoltcp 0.12, rustls 0.23, rustls-rustcrypto 0.0.2-alpha,
    getrandom 0.4 + 0.2, bitflags 2. All BSD / MIT / Apache-2.0
    / ISC — clear under CLAUDE.md §3. M1 grows this list:
    pci-types (BSD/MIT, structured PCIe config parsing) and a
    Slint runtime crate at step 7 are the likely first
    additions; per-driver shim source files for inherited
    Linux code retain GPLv2 in their original form per
    CLAUDE.md §3 / the FreeBSD drm-kmod pattern.

  - QEMU smoke command line at M0 exit:
    `-cdrom $ISO -m 256M -smp 4 -machine q35 -accel tcg
    -cpu max -device virtio-rng-pci
    -drive file=$ISO,if=none,id=blk0,format=raw,readonly=on
    -device virtio-blk-pci,drive=blk0
    -netdev user,id=net0
    -device virtio-net-pci,netdev=net0
    -display none -no-reboot -no-shutdown
    -serial file:$SERIAL_LOG -d guest_errors -D $QEMU_LOG`.
    M1 step 1 (NVMe) replaces `-device virtio-blk-pci` with
    `-device nvme,serial=arsenal0,drive=blk0`.

  - Real hardware: not yet purchased. ARSENAL.md commits to
    Framework 13 AMD as the v1.0 configuration target. The
    purchase timing is one of the milestone-level trade-offs
    below; recommend purchase mid-M1 (after step 1 NVMe is
    stable in QEMU) so the first real-hardware boot has a
    storage path that works.

M1 — proposed step decomposition

The plan below is the kickoff proposal, not gospel. The user
picks; deviations get justified before code lands. M1's eight
steps roughly correspond to the seven ARSENAL.md deliverables
plus a milestone-close step at the end. The first three steps
are QEMU-only (no real-hardware dependency); steps 4-7 need
the Framework 13 AMD; step 8 is the milestone exit.

  **Step 1 — NVMe native Rust (~5K LOC).** First M1 driver.
  Native Rust per ARSENAL.md; no LinuxKPI dependency. The
  smallest useful M1 driver and the first to exercise the
  PCIe configuration / MSI-X / DMA paths every other driver
  also needs. Outcome: kernel can boot from a real NVMe disk
  in QEMU. Smoke gains ARSENAL_NVME_OK and the QEMU command
  line swaps virtio-blk for `-device nvme`. Calendar budget:
  4-6 weeks at part-time pace. Use **go m1-1** for kickoff.

  **Step 2 — LinuxKPI shim foundation + tiny inherited
  driver.** Build the smallest viable shim surface that
  satisfies one inherited driver — recommend a simple Linux
  driver as the first target (a virtio-balloon driver, or a
  small Intel NIC driver from net/ethernet/intel/e1000 —
  decide at the step kickoff). The shim covers printk-style
  logging, kmalloc / kfree against our heap, GFP_KERNEL
  flags as no-ops, struct device + driver registration,
  pci_register_driver / pci_unregister_driver, IRQ
  registration via our IDT / IOAPIC, basic locking
  (spinlock_t / mutex_t mapping to spin::Mutex). DMA bounce
  buffers wait for amdgpu / iwlwifi. The driver source files
  retain GPLv2; the shim is BSD-2; the combined work ships
  with explicit license boundaries (the FreeBSD drm-kmod
  precedent). Calendar budget: 12-20 weeks — this is
  ARSENAL.md's "single largest engineering task" and the
  estimate is loose. Use **go m1-2**.

  **Step 3 — xHCI USB driver.** Per ARSENAL.md "evaluate at
  start" — the native-Rust-vs-LinuxKPI choice is the
  load-bearing trade-off below. Native Rust is the cleaner
  shape; the Linux xhci-hcd port via the shim from step 2
  is the broader-shim-validation shape. Recommend native
  Rust at step kickoff; revisit if scope blows up. Outcome:
  USB keyboard works post-Limine, USB mass storage works
  for installer / live USB. Calendar budget: 6-10 weeks
  native, 4-8 weeks LinuxKPI port (less code, more shim
  surface). Use **go m1-3**.

  **Step 4 — amdgpu KMS via LinuxKPI shim.** The headlining
  driver. Brings up Framework 13 AMD's integrated GPU
  (Phoenix / Hawk Point / Strix Point depending on SKU)
  enough to produce a framebuffer at native resolution.
  KMS only — no Vulkan, no DRI, no 3D acceleration. amdgpu
  also needs DMA bounce buffers, scatter-gather lists,
  i2c bus access, ACPI methods (AML interpretation) for
  ATIF / WMI handshake — the shim grows significantly at
  this step. Firmware blobs (sienna_cichlid_sdma.bin etc.)
  require provenance documentation. Calendar budget: 12-16
  weeks. **This is the step most likely to surface
  fundamental shim design issues that ripple back to step 2.**
  Use **go m1-4**.

  **Step 5 — iwlwifi + mac80211 via LinuxKPI.** Wireless.
  mac80211 is Linux's 802.11 stack (~50K LOC); iwlwifi is
  Intel's wifi driver (~30K LOC). Plus the firmware blob.
  Note: Framework 13 AMD ships with either an AMD-branded
  Mediatek MT7921 (mt76 driver) or an Intel AX210 (iwlwifi)
  depending on configuration; the step-level HANDOFF picks
  one based on the actual purchased SKU. WPA2/3 supplicant
  (wpa_supplicant) is itself ~100K LOC in C; for M1 we
  port the minimum subset needed for "associate to a
  preconfigured network" (the ARSENAL.md M1 usability
  gate). Calendar budget: 12-20 weeks. Use **go m1-5**.

  **Step 6 — First boot on real Framework 13 AMD.** USB
  installer, UEFI boot, Limine, kernel, all four drivers
  from steps 1-5 running on real silicon. Expect a 2-4
  week stabilization period for real-hardware quirks not
  seen in QEMU. The ARSENAL.md performance gate (cold
  boot to login < 8 s) is asserted here. Calendar budget:
  4-8 weeks. **This step ends most ambiguity about the
  hardware purchase timing — see logistics below.** Use
  **go m1-6**.

  **Step 7 — Slint app on software-rendered framebuffer.**
  First "modern UI" on top of M0's fb console. Slint is
  the Rust UI framework ARSENAL.md commits to (MIT/Apache
  dual-licensed; commercial license for proprietary apps
  also available — our use is open-source so MIT path).
  Software-rendered means no GPU acceleration — paint
  pixels to our fb directly. Likely a single "settings
  app" or "system info" widget at M1; richer apps wait
  for M2's Stage compositor. Calendar budget: 4-8 weeks.
  Use **go m1-7**.

  **Step 8 — M1 milestone exit.** STATUS retrospective,
  devlog at docs/devlogs/2026-NN-arsenal-real-iron.md (or
  per-step devlogs aggregated; pattern decided at step
  kickoff), arsenal-M1-complete tag. The M1 retrospective
  documents the LinuxKPI shim's final shape (how big it
  grew, which Linux APIs were stubbed vs full vs not-
  needed), what surprises showed up on real hardware, and
  what posture changes carry to M2 (Stage compositor will
  consume the M1 amdgpu KMS output through DRM dumb
  buffers, or via the framebuffer path if KMS doesn't pan
  out). Use **go m1-8** for STATUS + tag, **draft
  m1-8-devlog** for the devlog.

Calendar arithmetic. ARSENAL.md M1 budget is months 9-24 —
15 months. Step budgets sum: 4+12+6+12+12+4+4 = 54 weeks of
focused work, ~62 weeks at part-time (CLAUDE.md ×2.3) = 14
months. Sits inside the budget with a 1-month slack for
real-hardware surprises and revision cycles. Loose budgets
(amdgpu 12-16, iwlwifi 12-20, shim 12-20) absorb most of
the variance; expect one of those three to blow out by
50-100% and not be alarming.

M1 sub-block-vs-step-level cadence. M0 had a HANDOFF per
step with sub-block-per-devlog cadence inside. M1 steps are
larger and probably want internal sub-block decomposition
in their per-step HANDOFFs (M1-1 NVMe will decompose into:
PCIe config parsing, MSI-X setup, admin queue, I/O queues,
IRQ wiring, smoke validation — 5-6 sub-blocks; M1-2 LinuxKPI
shim will decompose into: types, allocators, IRQ, locks,
PCI, device — 6-8 sub-blocks). The HANDOFF.md file follows
the M0 pattern: overwritten at each step kickoff.

First step — recommendation: M1 step 1 (NVMe)

Why NVMe first, not amdgpu (the headliner) or the LinuxKPI
shim (the foundational piece)?

  - **Bounded scope.** ARSENAL.md says ~5K LOC. NVMe spec
    is public, well-documented, with mature reference
    implementations (Linux, FreeBSD, Redox). The driver
    is fundamentally a queue-pair I/O scheduler — much
    smaller than amdgpu (~500K LOC in Linux).
  - **Zero shim dependency.** Native Rust, no LinuxKPI.
    Step 1 unblocks itself; the shim shape can wait until
    we have at least one driver's worth of "what does a
    Rust driver in Arsenal actually look like" experience.
  - **Validates PCIe + MSI-X paths.** Every other M1
    driver needs these. NVMe is the smallest surface to
    exercise them.
  - **Useful outcome.** Kernel can boot from real disk
    (via QEMU's nvme device first; via real Framework
    NVMe at step 6). Replaces M0's virtio-blk dependence
    on QEMU.
  - **No real-hardware blocker.** All of step 1 happens
    in QEMU. Hardware purchase can wait until step 4 or
    later.

What step 1 would touch:

  - `arsenal-kernel/src/nvme.rs` (new, ~5K LOC target;
    smaller is better — Linux's nvme-core.c is ~6K LOC
    and supports far more than M1 needs).
  - `arsenal-kernel/src/pci.rs` (existing PCI scanner
    needs to grow MSI-X capability discovery; M0 PCI only
    enumerates and pretty-prints).
  - `arsenal-kernel/src/apic.rs` (IRQ vector allocation
    becomes dynamic; NVMe wants per-queue vectors, no
    longer a fixed 0xEF / 0xFF / 0x21 set).
  - `arsenal-kernel/src/ioapic.rs` (MSI-X bypasses IOAPIC
    — MSI-X messages go directly to LAPIC. IOAPIC stays
    for legacy IRQs only).
  - `arsenal-kernel/src/idt.rs` (vectors allocated at
    runtime; the Lazy IDT needs a registration API for
    M1+).
  - `ci/qemu-smoke.sh` (swap virtio-blk for nvme; add
    ARSENAL_NVME_OK sentinel).
  - `Cargo.toml` (likely no new deps for step 1; NVMe
    structs are small enough to hand-write).

Milestone-level trade-off pairs

The 10 trade-offs whose resolution shapes M1 most. Steps 1-7
each have step-level trade-off pairs in their own HANDOFFs;
these are the milestone-spanning ones.

  **LinuxKPI shim strategy.**
  (i) **Incremental per-driver-need.** Build only what the
  current driver target requires; grow when the next driver
  exercises new API. Smaller shim surface at any point in
  time; lower upfront cost; per-driver scope-creep risk.
  (ii) **Structural FreeBSD-modeled foundation.** Port the
  drm-kmod shim's structural skeleton (types, headers, the
  20-30 most-used APIs) up front; per-driver work fills in
  details. Larger upfront cost; lower per-driver scope-creep.
  (iii) **Hybrid.** Structural for the foundational types
  (printk, kmalloc, gfp_t, struct device, dma_addr_t) and
  the PCIe / IRQ / locking APIs; incremental for everything
  else (i2c, ACPI methods, scatter-gather, bouncing).
  Recommend (iii). The "load-bearing 30 APIs every driver
  uses" come up front; the long tail of per-driver-specific
  APIs grow as needed. FreeBSD's drm-kmod is the precedent;
  Asahi's m1n1 + the Asahi kernel team's documentation of
  Linux driver porting is the secondary reference. This is
  the step-2 trade-off but recording the milestone-level
  decision here.

  **First driver target.**
  (i) **NVMe (native Rust, ~5K LOC).** Recommended above.
  (ii) **amdgpu (LinuxKPI port).** Headlining; biggest
  proof-of-concept for the shim. But it requires step 2's
  shim first, and the failure-mode-surface area is huge.
  (iii) **xHCI (either native Rust or LinuxKPI port).**
  Useful — unlocks USB keyboards and mass storage for the
  install-on-real-hardware path. But xHCI's spec is more
  complex than NVMe's; ARSENAL.md flags the "evaluate at
  start" decision specifically.
  Recommend (i). NVMe is the smallest useful driver with
  zero dependencies on the rest of M1. amdgpu first would
  collapse the shim into amdgpu's specific needs; xHCI
  first overlaps with the keyboard story (M0's PS/2 still
  works on QEMU and on real hardware Framework offers via
  legacy controller).

  **xHCI shape — native Rust vs LinuxKPI port.**
  (i) **Native Rust.** Cleaner — direct against our IRQ /
  DMA / device-registration primitives. ~10-15K LOC. The
  Redox xhci crate (MIT) and Theseus's xhci module are
  prior Rust art; vendoring isn't out of the question if
  CLAUDE.md §3 license checks pass.
  (ii) **LinuxKPI port of xhci-hcd.** ~20K LOC of C goes
  unchanged; the shim covers it. Faster to get working;
  exposes more shim surface (USB-specific APIs like
  usb_hcd_ops which amdgpu / NVMe don't exercise).
  Recommend evaluation at step 3 kickoff, not here. Both
  are viable; the choice depends on how step 2 leaves the
  shim shaped. If the shim has clean USB-bus support
  already from some other driver pulled in at step 2, (ii)
  is cheap. If not, (i) is the cleaner path. Defer.

  **Real-hardware purchase timing.**
  (i) **Now (start of M1).** Buy the Framework 13 AMD
  before step 1 begins. Use it for cross-checking each
  step's QEMU output against real silicon as we go.
  Highest cost (sits idle through steps 1-3); highest
  early-failure-mode visibility.
  (ii) **End of step 3** (post-NVMe, post-shim foundation,
  post-xHCI). Buy when we have a kernel that's at least
  plausibly bootable on real iron. Most of M1's surface
  still ahead.
  (iii) **End of step 5** (post-iwlwifi). Buy when the
  driver lineup is complete and we just need to validate
  real-iron boot.
  Recommend (ii). End-of-step-3 gives us NVMe (storage),
  the shim foundation (whatever it ends up looking like),
  and xHCI (USB for the install medium). amdgpu KMS and
  iwlwifi are the final steps that benefit most from
  real-hardware feedback. Buying earlier than step 3 risks
  the laptop sitting idle while we work in QEMU; buying
  later than step 3 means amdgpu development is
  QEMU-only-with-poor-fidelity (QEMU's amdgpu emulation
  doesn't exercise real GPU hardware). Specific SKU:
  Framework 13 AMD Ryzen AI 7 350 (Strix Point) at the
  expected purchase date in mid-2026; choose lid color
  and memory at order time.

  **Continuous integration on real iron.**
  (i) **Manual sessions, document each.** When real
  hardware arrives, each step's exit criterion includes
  a "boots on real hardware and prints the expected
  output" manual checkpoint, recorded in the step's
  devlog. No automated CI on real hardware.
  (ii) **Dedicated runner.** Set up the Framework as a
  CI runner with network boot + serial console. Run the
  smoke automatically on every push.
  (iii) **Hybrid:** QEMU CI on every push (today's
  setup), real-iron CI nightly or per-merge.
  Recommend (i) at M1 start, revisit at step 6 when the
  Framework arrives. CI infrastructure on the Framework
  takes its own engineering surface (PXE boot, serial
  console capture, power cycling, fault recovery) and
  isn't justified until M1 has a stable real-iron boot
  path. Manual sessions are honest: real-iron tests in
  the M1 devlogs are user-runnable, not pretending to
  be CI.

  **MSI-X / IRQ model evolution.**
  (i) **Static IDT, dynamic vector allocation.** Keep
  M0's Lazy<IDT> static; add a `idt::allocate_vector()`
  helper that returns the next unused vector + installs
  the handler. NVMe asks for one vector per queue; the
  allocator hands them out. Simple, fits M0's shape.
  (ii) **Per-CPU IDT.** Each CPU has its own IDT (Linux
  does this). More flexible for SMP IRQ routing; bigger
  refactor; not justified for M1's single-machine
  workload.
  (iii) **MSI-X routes to specific LAPIC vectors only;
  IOAPIC stays for legacy IRQs.** Already implied by 4-3;
  here we just confirm.
  Recommend (i) at M1 — keeps the M0 shape and adds the
  smallest helper that NVMe needs. Step 1 HANDOFF will
  spec this concretely.

  **First inherited driver at step 2.**
  (i) **virtio-balloon.** Tiny (~600 LOC in Linux); pure
  virtio-bus interaction; lets us validate the shim's
  PCIe + IRQ + device-registration paths without any
  fancy DMA / scatter-gather / firmware loading.
  (ii) **e1000 (Intel gigabit Ethernet).** Bigger (~3000
  LOC); needs DMA descriptor rings, ethtool stubs,
  netdev registration. Better stress test of the shim.
  (iii) **8250 serial driver from Linux.** Tiny but
  conflicts with our existing serial.rs; not useful as a
  shim validator.
  Recommend (i). virtio-balloon is the smallest useful
  inherited driver that exists in the Linux tree; it
  exercises the shim without taking on stream-of-data
  surface that's better validated at step 3 (xHCI) or
  step 4 (amdgpu). Pick at step-2 kickoff.

  **Boot loader on real iron.**
  (i) **Limine continues.** ARSENAL.md commits to Limine
  for the BSP boot path; M0 confirmed it works for SMP
  bring-up via MpRequest. Real-iron Limine is the same
  binary as QEMU-Limine.
  (ii) **Switch to systemd-boot or rEFInd.** Mature; well-
  understood UEFI loaders. Loses our SMP integration via
  MpRequest.
  (iii) **Direct UEFI boot (no second-stage loader).**
  Smallest dependency; biggest engineering cost (we'd be
  writing our own bootloader).
  Recommend (i). The M0 commitment stands; M1 inherits it
  unchanged.

  **Slint shape.**
  (i) **Pure Slint software renderer.** ARSENAL.md
  commits; Slint's software-rendered mode paints to a
  pixel buffer that we route to our fb. ~50K LOC of
  vendored Slint runtime (MIT/Apache, clear under
  CLAUDE.md §3).
  (ii) **Slint via direct DRM (when amdgpu KMS is up).**
  Software-rendered through KMS framebuffer at native
  resolution. Slightly more code but uses the GPU's
  scanout path.
  (iii) **Defer Slint entirely.** Stage at M2 absorbs the
  UI surface; M1 Slint app is a "we proved Slint works"
  exercise that doesn't ship to users.
  Recommend (i) at step 7. ARSENAL.md commits to Slint;
  software-renderer is the simplest M1 path; KMS-routed
  rendering arrives naturally at M2 when Stage takes
  over.

  **Sub-step granularity within M1 steps.**
  (a) **One devlog per sub-block** (M0 step 3 pattern —
  3A through 3G each got a devlog).
  (b) **One devlog per step** with sub-block detail
  inside (M0 step 4 pattern — single devlog for the
  whole step).
  (c) **One devlog per cluster of related sub-blocks**
  (e.g., the LinuxKPI shim at step 2 has 6-8 sub-blocks;
  cluster as "shim foundation" / "PCI bridge" / "IRQ
  bridge" / "DMA bridge" with one devlog each).
  Recommend (c) for M1. Per-sub-block was right for M0's
  daily-or-hourly cadence; M1 sub-blocks span weeks each
  and the devlog cadence should match. Each step's
  HANDOFF will set its own sub-block-to-devlog mapping;
  step 8 (M1 retrospective) wraps the milestone-level
  story.

Sanity check before kicking off

    git tag --list                          # arsenal-M0-complete now present
    git log --oneline -10                   # 9793487 (HEAD), b535195, e2057de,
                                            # 6a69383, 78b38e2, b6b3785,
                                            # b70f0f2, f3f431e, 8b20132,
                                            # 1b316c9
    git status --short                      # clean except this file
    cargo build -p arsenal-kernel --target x86_64-unknown-none --release
                                            # clean, ~1.52 MB ELF
    cargo clippy -p arsenal-kernel --target x86_64-unknown-none --release -- -D warnings
                                            # clean
    cargo xtask iso                         # arsenal.iso ~19.3 MB
    ci/qemu-smoke.sh                        # ==> PASS (13 sentinels in ~1.2-1.5 s)

Expected: HEAD as above; smoke PASSes with 13 sentinels;
boot→prompt around 94 ms.

If the sanity check fails before the first M1 step kicks off,
the likely culprits are toolchain (nightly-2026-04-01 still
available?), CI environment (the smoke harness's Python TLS
listener requires openssl in PATH on macOS), or a regression
between M0 close and M1 start that the deferred bootloader
reclaim from 4-2 surfaced under some new condition. Walk the
M0 retrospective in STATUS § "Last completed milestone" for
the load-bearing invariants.

Out of scope for M1 specifically

  - **Vulkan / 3D acceleration.** amdgpu at M1 is KMS only.
    Vulkan via radv (mesa) is M2 or v0.5.
  - **Multi-monitor / multi-GPU.** Framework 13 AMD has one
    GPU and one display; M1 is single-pipe single-monitor.
  - **Bluetooth.** Framework 13 AMD ships with the WiFi
    module also supporting Bluetooth; M1 wires Wi-Fi only.
    BT stack is post-M1.
  - **Touchpad gestures / multitouch.** Single-touch tap +
    motion at M1 (via xHCI / HID); gesture recognition is
    post-M1.
  - **Suspend / resume.** Power-off and cold-boot are M1;
    ACPI S3/S4 sleep and modern S0ix idle are M2+.
  - **Battery / charge / thermal management.** Reads via
    ACPI _BST / _BIF only at M1; thermal throttling
    feedback into the scheduler is post-M1.
  - **Audio.** No HDA driver at M1; Stage at M2 includes
    audio routing.
  - **File systems.** A read-only FAT32 / ext2 for the
    install medium at M1 maximum; full-featured filesystems
    (ext4 write, btrfs, ZFS) wait for v0.5+. ARSENAL.md
    doesn't specify the M1 filesystem target; recommend
    deferring until M1 step 6 (real-iron boot) makes it
    necessary.
  - **Container / sandbox runtime.** Cardboard Box at M2.
  - **POSIX subset / libc compat.** relibc work is v0.5.
  - **WebKitGTK / Servo / browser.** M2 / v0.5.

Permanently out of scope (do not propose)

  - Any unsafe block without a // SAFETY: comment naming the
    invariant the caller must uphold. CLAUDE.md hard rule.
  - Reverting any M0 commit. M0 closed and tagged.
  - Force-pushing to origin. Branch is in sync; preserve
    history.
  - Dropping BSD-2-Clause license header from any new
    Arsenal-base file. Inherited Linux driver files retain
    GPLv2; the shim source files are BSD-2; explicit license
    boundaries documented per CLAUDE.md §3.
  - Pulling a GPL crate into the kernel base. Inherited
    Linux drivers via the LinuxKPI boundary are the only
    GPL path; vendored crates remain BSD / MIT / Apache /
    ISC / zlib / SIL-OFL.
  - Religious framing. CLAUDE.md hard rule.
  - Reintroducing HolyC. ADR-0004's discard is final.
  - Going back to stable Rust.
  - Skipping the build + smoke loop on a feat(kernel)
    commit.

Three notes worth flagging before you go

  1. **The LinuxKPI shim is a 12-20 week single block of
     work that doesn't ship anything user-visible on its
     own.** This is the morale-load-bearing piece of M1.
     The HANDOFF for step 2 should include explicit
     intermediate milestones (one shim API surface lands +
     compiles + has a smoke test, repeat) so progress is
     visible week-over-week, not just at the end. The
     FreeBSD drm-kmod team's pattern of "compile-only
     CI for the shim, full driver-integration CI when
     the shim is half-built" is the model.

  2. **Real-hardware purchase is a $1500-$2000 commitment
     for the Framework 13 AMD Ryzen AI 7 350 (Strix Point)
     with sensible memory (32 GiB) and storage (1 TB
     NVMe). Order lead time on Framework is typically 2-4
     weeks at announce-and-ship batch boundaries. Place the
     order at end of step 3 (post-xHCI) per the recommended
     trade-off; the laptop should arrive within 2-3 weeks
     of step 4 (amdgpu KMS) starting. Confirm pricing /
     availability at order time.

  3. **M1's cadence is genuinely different from M0's.**
     M0 step 4 ran in one calendar day; that's not what M1
     looks like. M1 step 1 (NVMe) is a 4-6 week effort; M1
     step 2 (shim) is 12-20 weeks. Sub-block-per-devlog at
     daily cadence collapses; one devlog per sub-block at
     weekly cadence is the right shape. The HANDOFF /
     commit cadence slows accordingly. CLAUDE.md's "noticing
     when I'm heads-down on a single bug for multiple
     sessions" cue applies *more* in M1 than M0; flag
     promptly if a single sub-block stretches past 2 weeks
     of grinding without visible progress.

Real-hardware logistics (M1 step 6 prep)

ARSENAL.md commits to Framework 13 AMD as the v1.0 hardware
target. M1's step 6 is "first boot on real iron." Practical
steps:

  - **SKU selection.** Framework 13 AMD Ryzen AI 7 350
    (Strix Point) is the current premium SKU as of
    mid-2026; the Ryzen AI 5 340 is the budget option.
    Either works for M1; Strix Point gives more headroom
    for M2 / Stage compositor work. Recommend the higher
    SKU.

  - **Memory and storage.** Framework offers 16 / 32 / 64
    GiB DDR5; 32 GiB is the M1 sweet spot. NVMe: any of
    the offered 500 GB / 1 TB / 2 TB; 1 TB is plenty for
    dev work without paying the 2 TB premium.

  - **WiFi module.** Framework 13 AMD ships with either
    Mediatek MT7921 (mt76 Linux driver) or Intel AX210
    (iwlwifi). Step 5 of M1 implements whichever ships;
    ARSENAL.md mentions iwlwifi specifically, so AX210 is
    the assumed target. Confirm at order time.

  - **Boot medium.** USB drive flashed with arsenal.iso
    (the same artifact the QEMU smoke runs). Limine boots
    on UEFI without intervention. Recommend a USB-C
    drive (the Framework's USB-A port is via the
    expansion-card system; USB-C is always-available).

  - **Recovery posture.** Keep a Linux live-USB nearby
    during step 6. Real-hardware bring-up will reveal
    panics we can't reproduce in QEMU; being able to
    boot Linux and pull serial logs / disk dumps is the
    fallback path.

Wait for the pick. Do not pick silently. The natural first
move is **go m1-1** for the NVMe-first plan above, or **draft
m1-1 HANDOFF** if you want the step-level kickoff document
written before any code lands (recommended for M1's first
step — the step-level HANDOFF is the planning artifact M0's
cadence depended on). Alternatives: **go m1-2** to start with
the shim foundation (riskier — no driver target to drive the
API surface, scope creep likely), or **defer** to first
re-read ARSENAL.md and revisit the milestone-level trade-offs
above (especially the LinuxKPI shim strategy and the first
driver target — these are the most consequential resolutions
of M1).
