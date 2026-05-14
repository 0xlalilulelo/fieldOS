Kickoff for M1 step 1 — NVMe native Rust.

M0 closed at arsenal-M0-complete (9793487, 2026-05-14). M1's
milestone-level HANDOFF landed at 9df4682; the eight-step plan
puts NVMe as step 1 — smallest useful M1 driver, native Rust,
~5K LOC target per ARSENAL.md, zero dependency on the
LinuxKPI shim, the first PCIe driver to exercise MSI-X. The
M0 PCI scanner from 3C was enumerate-and-print only; M1
step 1 grows it into "find the NVMe controller, parse its
capability list, map its BARs, talk to it."

The natural outcome at step exit: the QEMU smoke gains an
`-device nvme` and reads sector 0 of the emulated NVMe via
our driver. ARSENAL_NVME_OK joins the sentinel set. Per the
milestone HANDOFF's recommendation, virtio-blk stays in the
QEMU command line — it satisfies the boot path until step 7
(real hardware) makes it unnecessary. The smoke's
ARSENAL_BLK_OK keeps firing; ARSENAL_NVME_OK is additive.

read CLAUDE.md (peer concerns, Rust-only, BSD-2-Clause base,
build loop sacred, "no unsafe without // SAFETY: comment
naming the invariant"; NVMe is the first M1 step and our
unsafe blocks will multiply — MMIO reads, PRP physical-
address conversions, DMA buffer transmutes) → STATUS.md
(M0 complete, M1 active; the five posture changes from M0
are load-bearing — especially "MMIO pages need explicit
`paging::map_mmio` before access" which step 1 will repeat
for the NVMe BAR) → docs/plan/ARSENAL.md § "M1 — Real iron"
(third bullet: "NVMe driver (native Rust, ~5 K LOC)") →
M1 milestone HANDOFF at git show 9df4682 § "First step —
recommendation: M1 step 1 (NVMe)" + § "First driver target"
trade-off (the milestone-level resolutions step 1 inherits) →
NVMe spec 1.4 base (https://nvmexpress.org/specifications/);
the M1-step-1 implementation targets 1.4 deliberately —
broadly supported by every NVMe device in commodity hardware
and what QEMU emulates by default → arsenal-kernel/src/pci.rs
(M0 enumerate-and-print path; step 1 grows it to walk the
PCIe capability list, find MSI-X, expose MSI-X table base +
count to callers; pattern mirrors 3C's virtio modern PCI
capability walk) → arsenal-kernel/src/virtio_blk.rs (the
3C-3 model — single device, single queue, smoke reads sector
0, asserts 0xAA55; M1-1's NVMe smoke follows the same shape
at higher abstraction) → arsenal-kernel/src/idt.rs (M0's IDT
is Lazy; step 1 adds a `register_vector` helper for dynamic
vector allocation — NVMe wants one MSI-X vector per queue,
not statically known at compile time) → arsenal-kernel/src/
apic.rs (LAPIC EOI path; MSI-X delivers directly to LAPIC,
bypassing IOAPIC — apic::send_eoi from 4-5 is reusable) →
arsenal-kernel/src/frames.rs (DMA buffers need page-aligned
physical memory; frame allocator hands out 4-KiB frames at
known physical addresses) → arsenal-kernel/src/paging.rs
(map_mmio for the NVMe BAR; hhdm_offset for the data buffer
virt-to-phys conversion since heap-allocated buffers live in
HHDM-mapped RAM) → arsenal-kernel/src/main.rs (boot order;
nvme::smoke fits between virtio_net::smoke and net::init,
matching virtio_blk's position) → ci/qemu-smoke.sh (add
`-device nvme,serial=arsenal0,drive=nvme0`, a small raw
backing file for nvme0, ARSENAL_NVME_OK sentinel) →
Cargo.toml (no new dependencies at step 1; NVMe structs
are register-shaped and ~200 LOC of bindings; we hand-write)
→ git log --oneline -10 → run the sanity check below →
propose 1-N commit shape (or argue for a different
decomposition) → wait for me to pick → "go m1-1-N" for
code, "draft m1-1-N" for paper deliverables.

Where the project is

  - HEAD: 9df4682 (docs(handoff): kick off M1 (real iron)).
    Working tree is clean except this file. main is one
    commit ahead of origin/main; this HANDOFF makes it two.
    Push when 1-0 is about to kick off so the milestone +
    step HANDOFFs both land together on origin.

  - Kernel: 22 .rs files at M0 exit. Step 1 adds two new
    files (nvme.rs, plus a small extension to pci.rs that
    grows enough to deserve its own section but probably
    stays in pci.rs). Expected LOC growth: ~600-800 LOC in
    nvme.rs at step exit (well under the ~5K LOC ARSENAL.md
    estimate — M1 step 1 targets *minimal correct NVMe*,
    not feature-complete; more features accrue in later
    steps if needed for amdgpu/iwlwifi storage paths or
    M1 step 7's real-hardware install medium). pci.rs
    grows ~150 LOC for the MSI-X capability walker.
    idt.rs grows ~50 LOC for vector registration.

  - ELF: 1.52 MB at M0 exit. Step 1 likely adds ~30-50 KB
    (mostly nvme.rs's lookup tables and the new IRQ
    handler).

  - Smoke at M0 exit: 13 sentinels, ~1.2-1.5 s on QEMU
    TCG with -smp 4. Boot→prompt 94-108 ms. Step 1 adds
    one sentinel (ARSENAL_NVME_OK), brings total to 14.

  - Toolchain: nightly-2026-04-01. No changes for step 1.

  - QEMU command line at step 1 exit (proposed):
    `-cdrom $ISO -m 256M -smp 4 -machine q35 -accel tcg
    -cpu max -device virtio-rng-pci
    -drive file=$ISO,if=none,id=blk0,format=raw,readonly=on
    -device virtio-blk-pci,drive=blk0
    -drive file=$NVME_BACKING,if=none,id=nvme0,format=raw
    -device nvme,serial=arsenal0,drive=nvme0
    -netdev user,id=net0
    -device virtio-net-pci,netdev=net0
    -display none -no-reboot -no-shutdown
    -serial file:$SERIAL_LOG -d guest_errors -D $QEMU_LOG`.
    The `$NVME_BACKING` file is a small (1 MiB) raw image
    the smoke creates via `dd if=/dev/zero ... bs=1M count=1`
    with the MBR signature written at byte 510 — same
    pattern 3C used to validate virtio-blk's sector 0 read.

M1 step 1 — proposed sub-block decomposition

The plan below is the kickoff proposal, not gospel. The user
picks; deviations get justified before code lands. Step 1
decomposes into six sub-blocks (the M0 step 3 cadence —
~one per week at part-time pace, six weeks total fits inside
the milestone HANDOFF's 4-6 week budget with one slack week
for real-hardware-style surprises QEMU surfaces).

  **(M1-1-0) PCIe MSI-X capability enumeration.** Extend
  `arsenal-kernel/src/pci.rs` to walk the PCIe capability
  list and recognize MSI-X (capability ID 0x11). For each
  device with MSI-X, decode the Message Control field (table
  size, function mask, enable), record the table BAR + offset,
  and expose a `pci::msix_info(bdf) -> Option<MsixInfo>`
  getter. The capability walk pattern is already established
  by 3C's virtio modern PCI transport (vendor-specific
  capability ID 0x09); MSI-X is a different ID but the same
  walk shape. Also adds `idt::register_vector(handler) -> u8`
  — a small dynamic allocator over a fixed range of vectors
  (recommend 0x40..0xEF, leaving the M0 ones 0x21/0xEF/0xFF
  unchanged). Step exit observation: pci scan logs every
  device's MSI-X presence + table size. No new sentinel.
  ~150 LOC pci.rs + ~50 LOC idt.rs + ~10 LOC main.rs. One
  commit: `feat(kernel): PCIe MSI-X capability + dynamic
  IDT vector allocation`. Use **go m1-1-0**.

  **(M1-1-1) NVMe device discovery + BAR mapping + register
  primitives.** Adds `arsenal-kernel/src/nvme.rs`. Scans for
  the NVMe controller (class code 0x01:0x08:0x02 — mass
  storage / NVMe / NVMe I/O); BAR0 (64-bit MMIO) is mapped
  via `paging::map_mmio` to 0x4000 bytes (covers the
  controller registers + first 31 sets of queue doorbells —
  doorbell stride DSTRD from CAP.DSTRD widens this if needed
  at 1-2). Defines the spec-required register offsets (CAP
  0x00 64-bit, VS 0x08 32-bit, CC 0x14 32-bit, CSTS 0x1C
  32-bit, AQA 0x24 32-bit, ASQ 0x28 64-bit, ACQ 0x30 64-bit,
  doorbell base 0x1000) and read/write helpers. Reads
  + logs CAP (MQES = max queue entries supported, DSTRD =
  doorbell stride, CSS = command set support, MPSMIN /
  MPSMAX = page size support range) and VS (NVMe spec
  version — expect 1.4 from QEMU's default). Asserts
  MPSMIN ≤ 12 (so 4-KiB host pages are supported) and CSS
  bit 0 set (NVM command set supported). No queues built
  yet. ~200 LOC. One commit: `feat(kernel): NVMe device
  discovery + BAR mapping`. Use **go m1-1-1**.

  **(M1-1-2) Controller reset + admin queue + Identify.**
  Disables the controller (CC.EN = 0; spin on CSTS.RDY → 0),
  allocates two physically-contiguous 4-KiB pages from the
  frame allocator for the admin submission queue (64 entries
  × 64 bytes) and admin completion queue (64 entries × 16
  bytes), writes ASQ + ACQ + AQA (queue sizes, both 63 in
  the zero-based queue-depth fields), configures CC
  (IOSQES = 6 → 64-byte SQ entries, IOCQES = 4 → 16-byte CQ
  entries, MPS = 0 → 4-KiB pages, CSS = 0 → NVM command set,
  AMS = 0 → round-robin arbitration), enables (CC.EN = 1;
  spin on CSTS.RDY → 1). Then submits Identify Controller
  (CNS = 1) and Identify Namespace (CNS = 0, NSID = 1) via
  the admin queue using polled completion (the I/O queue
  IRQ wiring is at 1-4; admin queue completion polling
  is fine at this stage — admin commands are rare and
  blocking). Logs the disk serial, model, FR (firmware
  revision), NN (namespace count), and the namespace 1's
  NSZE (size in logical blocks) + LBADS (LBA data size,
  typically 9 for 512-byte sectors). ~300 LOC. One commit:
  `feat(kernel): NVMe controller reset + admin queue +
  Identify`. Use **go m1-1-2**.

  **(M1-1-3) I/O queue creation + first sector read (polled).**
  Creates one I/O completion queue (CID = 1, size 64, IRQ
  vector field zero — interrupts come at 1-4; this stage
  uses polled completion) via the admin Create-I/O-CQ
  command (opcode 0x05), then one I/O submission queue
  (QID = 1, CQID = 1, size 64) via Create-I/O-SQ (opcode
  0x01). Submits a Read command (opcode 0x02) on the I/O
  queue with NSID = 1, SLBA = 0, NLB = 0 (one block, NLB
  is zero-based), PRP1 = physical address of a 4-KiB
  frame-allocated buffer. Polls the I/O completion queue
  for the doorbell-updated phase tag flip, reads the
  buffer, asserts the MBR signature 0xAA55 at byte offset
  510 (same as 3C's virtio-blk smoke). Emits
  ARSENAL_NVME_OK. ~250 LOC. One commit:
  `feat(kernel): NVMe I/O queue + sector 0 read (polled)`.
  Use **go m1-1-3**. **This is the sub-block that closes
  the ARSENAL.md step-1 outcome ("first M1 driver works").**
  1-4 / 1-5 are quality-of-implementation follow-ups.

  **(M1-1-4) MSI-X interrupt wiring for the I/O queue.**
  Converts the I/O queue's polled completion to MSI-X
  interrupt-driven. Allocates one IDT vector via
  `idt::register_vector` (from 1-0), writes the
  corresponding MSI-X table entry (address = LAPIC fixed-
  delivery address 0xFEE00000 with the BSP's APIC ID in
  bits 12-19, data = vector + delivery mode 0 + level 0
  + edge), unmasks the entry (clear bit 0 of vector
  control), re-creates the I/O CQ with the vector field
  set (the 1-3 path used vector=0 which is "no IRQ"; a
  new Create-I/O-CQ command with the right vector is the
  spec-clean reconfigure). Handler does the same work
  the polled path did plus an apic::send_eoi at the end.
  ~150 LOC. One commit: `feat(kernel): NVMe MSI-X
  interrupts`. Use **go m1-1-4**.

  **(M1-1-5) STATUS refresh + step 1 devlog + step 2
  HANDOFF kickoff.** STATUS flips step 1 from "next" to
  "complete," promotes step 2 (LinuxKPI shim foundation)
  to "active," and writes the step 1 retrospective
  sub-section (what NVMe quirks QEMU surfaced, what the
  M0 → M1 pattern shifts looked like in practice, how
  close the LOC came to ARSENAL.md's ~5K estimate).
  Devlog at `docs/devlogs/2026-NN-arsenal-nvme.md` (NN =
  whatever month the step actually exits) records the
  controller-reset sequence's delicate moments (the
  CSTS.RDY polling timing, the AQA / ASQ / ACQ write
  ordering subtleties), the polled-vs-MSI-X trade-off
  resolution (polled first at 1-3, MSI-X at 1-4 — the
  HANDOFF's recommendation), the PCIe MSI-X capability
  walk addition to pci.rs as a foundation step 3+ will
  also consume, and the "this is what M1 step 1 looked
  like" summary. Two commits: `docs(status): M1 step 1
  complete, step 2 (LinuxKPI shim foundation) next` and
  `docs(devlogs): Arsenal NVMe`. Use **go m1-1-5** for
  STATUS, **draft m1-1-5-devlog** for the devlog.

Realistic session-count estimate. M1's cadence is week-
scale, not day-scale (M1 milestone HANDOFF note #3). 1-0
is half-a-week to a week (the MSI-X capability walk is
mechanical against the spec but the dynamic IDT vector
allocator is one of those "easy to get right and
catastrophic to get wrong" pieces — get it reviewed before
shipping). 1-1 is one focused session if the BAR mapping
goes cleanly; two if NVMe's CAP register reads zero (a
common bring-up bug — the BAR is 64-bit so the cap-read
needs `lapic_read`-shaped 64-bit MMIO access, not 32-bit
× 2 with the wrong byte order). 1-2 is the spec-rich
session — most of the M1 step 1 code lands here. 1-3 is
the cathartic session — first real NVMe I/O. 1-4 is one
session (MSI-X is straightforward once 1-0's foundations
are in). 1-5 is the milestone-style paper session. Calendar
budget per the milestone HANDOFF: 4-6 weeks at part-time.

Step-level trade-off pairs

  **MSI-X first vs polled completion first.**
  (i) **Polled completion at 1-3, MSI-X follow-up at 1-4.**
  Smaller surface per sub-block; 1-3 is independently
  smoke-verifiable (ARSENAL_NVME_OK fires off the polled
  path); 1-4 converts to interrupt-driven without
  rebuilding the queue from scratch (Create-I/O-CQ is
  re-issuable per the NVMe spec). Bisect-rich.
  (ii) **MSI-X from the start.** 1-3 lands the I/O queue
  with MSI-X already wired; 1-4 doesn't exist; the step
  becomes 5 sub-blocks instead of 6. Slightly less code
  total but a larger 1-3 sub-block (combining queue setup +
  IRQ wiring + sector read in one commit).
  Recommend (i). Polled-first is the conventional NVMe
  bring-up pattern; Linux's nvme-pci.c does the same in
  its setup path. The bisect granularity from splitting
  matters more than the small commit-count win from
  combining.

  **Number of I/O queue pairs.**
  (i) **Single shared I/O queue.** One submission + one
  completion queue, all CPUs submit through it. Single
  MSI-X vector. Simple. M0/M1-scale workload doesn't
  saturate; Linux's nvme-pci defaults to one queue per
  online CPU but that's overkill until a workload needs
  parallel disk I/O.
  (ii) **One I/O queue per CPU.** SMP-friendly; matches
  Linux's default. Requires per-CPU vector allocation,
  per-CPU SQ tail tracking, per-CPU CQ phase tag bits.
  Bigger surface; not justified at M1.
  Recommend (i) at step 1. The HANDOFF for step 2+ can
  revisit if amdgpu / iwlwifi's storage paths surface a
  parallel-I/O demand. M2's Stage compositor will need
  the per-CPU queues for graceful UI under load, but
  that's M2.

  **DMA buffer source.**
  (i) **Frame allocator** (page-aligned 4-KiB frames at
  known physical addresses; HHDM-mapped virtual access
  for the kernel side). Step 1's queues and sector-read
  buffer all come from FRAMES.
  (ii) **Heap-allocated** (variable size, alignment via
  `core::alloc::Layout`). Sufficient for queues but the
  NVMe spec requires page-aligned buffers for PRP1; heap
  alignment would need explicit support.
  Recommend (i). Frame allocator is the right primitive
  for DMA — that's what frame allocators exist for. Heap
  for everything else; frames for DMA / queues. Pattern
  carries forward through every M1 driver.

  **Driver file layout.**
  (i) **Single nvme.rs file.** All step 1 code lives in
  one file (~600-800 LOC at step exit). Easy to navigate.
  (ii) **nvme/ module directory** with admin.rs, io.rs,
  registers.rs. More organized; over-organized at the
  M1-step-1 LOC scale.
  Recommend (i). M0 pattern is one file per subsystem;
  preserve until a file genuinely outgrows 1500-2000 LOC.
  At M1 step 5 (amdgpu) and step 6 (iwlwifi) the
  directory-per-driver shape will be appropriate.

  **Sentinel granularity.**
  (a) **Single ARSENAL_NVME_OK** at 1-3 (polled sector
  read completes). 1-4's MSI-X conversion uses the same
  sentinel — same property asserted, different path.
  (b) **Two sentinels**: ARSENAL_NVME_OK at 1-3 (polled),
  ARSENAL_NVME_IRQ_OK at 1-4 (MSI-X). Per-sub-block
  granularity in CI.
  (c) **Three sentinels**: add ARSENAL_NVME_ADMIN_OK at
  1-2 (admin queue + Identify complete).
  Recommend (a). NVMe-as-a-whole has one observable
  property at M1 step 1: "kernel can read sector 0 of an
  emulated NVMe disk." That maps to one sentinel. 1-4's
  conversion is a code-path change, not a new property —
  the same sentinel firing through a different path is
  equivalent evidence.

  **Replace virtio-blk in the smoke vs additive.**
  (i) **Additive** — keep virtio-blk for boot + the BLK_OK
  sentinel; add NVMe alongside. Smoke command line grows.
  (ii) **Replace** — drop virtio-blk, NVMe handles both the
  driver demonstration and (eventually) the boot path.
  Need to remove the BLK_OK sentinel + virtio_blk::smoke
  call site.
  Recommend (i) at step 1. virtio-blk works; removing it
  is unmotivated at this point. Real-hardware Framework 13
  AMD doesn't have virtio-blk, so step 7 (real-iron boot)
  is the natural deletion point — but step 7 also needs
  the install-medium boot path which Limine handles, not
  virtio-blk. Defer the virtio-blk deletion to a polish
  commit post-step-1, not blocking on it.

  **NVMe spec version target.**
  (i) **1.4** — broadly supported in commodity hardware
  (2019-spec, every current consumer NVMe SSD shipped in
  the last 5+ years implements 1.4 or later). QEMU
  emulates 1.4 by default. Most spec features M1 step 1
  needs are 1.0-era; 1.4 brings only features we don't
  consume (Persistent Event Log, Sanitize, etc.).
  (ii) **2.0** — newer spec; some additions M2+ might
  want (Zoned Namespaces, Endurance Groups). Adds parser
  complexity for an M1 step that doesn't need it.
  Recommend (i). Target 1.4 explicitly; fall back to 1.0
  patterns if QEMU's emulated controller reports an older
  version. M1 step 1 reads the VS register and asserts
  ≥ 1.0; no behavior change based on version.

  **Sub-block granularity.**
  (a) **Six-commit shape** above (MSI-X foundation + IDT
  vector / device + BAR / reset + admin / I/O queue read /
  MSI-X conversion / STATUS+devlog). Bisect-rich.
  (b) **Five-commit shape** combining 1-0 with 1-1 (MSI-X
  + IDT-vector tooling + NVMe device discovery in one
  commit). Smaller history; harder to bisect if MSI-X
  parsing has a subtle bug that surfaces at step 3 (xHCI)
  three weeks later.
  (c) **Four-commit shape** also combining 1-3 with 1-4
  (single commit ships polled + MSI-X). Saves a commit
  but loses the "polled smoke green before MSI-X
  conversion" bisect point.
  Recommend (a). MSI-X capability parsing in pci.rs is a
  foundation step 3+ also consumes; making it a standalone
  commit lets future bisection isolate "did the MSI-X
  walker change?" cleanly. The 1-3 / 1-4 split similarly
  protects the "polled NVMe works" property as a
  standalone milestone.

Sanity check before kicking off

    git tag --list | grep arsenal             # arsenal-M0-complete
    git log --oneline -10                     # 9df4682 (HEAD),
                                              # 9793487, b535195,
                                              # e2057de, 6a69383,
                                              # 78b38e2, b6b3785,
                                              # b70f0f2, f3f431e,
                                              # 8b20132
    git status --short                        # ?? HANDOFF.md (only,
                                              # while drafting this)
                                              # or clean once committed
    cargo build -p arsenal-kernel --target x86_64-unknown-none --release
                                              # clean, ~1.52 MB ELF
    cargo clippy -p arsenal-kernel --target x86_64-unknown-none --release -- -D warnings
                                              # clean
    cargo xtask iso                           # arsenal.iso ~19.3 MB
    ci/qemu-smoke.sh                          # ==> PASS (13 sentinels)

Expected: HEAD as above; smoke PASSes with 13 sentinels;
boot→prompt around 100 ms.

If 1-1 or 1-2 fails to make progress (controller doesn't go
from disabled to ready), the likely culprits are:

  (a) **CAP register read returns 0.** NVMe's CAP at offset
  0x00 is 64-bit. If we read it as two 32-bit halves with
  the wrong endianness or with the lapic_read-style 32-bit
  helper that doesn't span 64-bit reads cleanly, the result
  comes out garbage. The MMIO BAR for NVMe is mapped at the
  HHDM offset like every other MMIO; the read pattern is
  `core::ptr::read_volatile::<u64>` against the HHDM-virtual
  base.

  (b) **CC.EN write doesn't take effect.** CC at offset 0x14
  is 32-bit; the EN bit (bit 0) is the controller's
  enable / disable signal. Writing CC.EN = 1 while CC.EN was
  already 1 (residual from firmware) is undefined per the
  spec — must explicitly disable first (CC.EN = 0; spin
  CSTS.RDY → 0; configure; CC.EN = 1; spin CSTS.RDY → 1).
  The CSTS.RDY → 0 spin has no real-world timeout (QEMU
  ~1ms, real hardware ~tens of ms); a `while_until_with_
  bound` loop with a generous 1-second cap surfaces a wedged
  controller before the smoke times out.

  (c) **AQA / ASQ / ACQ written after CC.EN = 1.** Spec is
  explicit: AQA, ASQ, ACQ must be programmed *before* CC.EN
  flips to 1. The bring-up sequence is fixed: disable →
  program admin queue → enable.

  (d) **Admin queue physical address misaligned.** ASQ
  must point at a 4-KiB-aligned physical address; same for
  ACQ. The frame allocator hands out 4-KiB-aligned frames
  by definition, so just pass `frames::FRAMES.alloc_frame
  ().start_address().as_u64()` straight through — no
  manual alignment.

  (e) **Doorbell write missing on submission.** SQ tail
  doorbell at offset 0x1000 + 2 * SQID * (4 << DSTRD) must
  be written after staging a command in the SQ — that's the
  "tell the controller new work is available" signal. The
  classic bring-up bug: stage the command, never doorbell,
  controller idles forever. Polled completion checks the
  CQ head doorbell shape, *not* whether the controller
  noticed the SQ update.

  (f) **CQ phase tag flip not handled.** Each I/O completion
  flips a "phase" bit so the driver can tell new completions
  from stale buffer contents. Initial phase is 1; first
  completion sets phase to 1 (queue zero-initialized);
  subsequent wrap-arounds flip phase to 0, 1, 0, etc. The
  polled completion loop must track the expected phase per
  queue and compare against the CQ entry's phase field.

  (g) **MSI-X table BAR confusion.** MSI-X tables live in
  one of the device's BARs (NVMe usually puts them in BAR0
  with an offset). The capability structure encodes which
  BAR + what offset; 1-0's parser must extract both, not
  assume BAR0+0. Step 1 logs the BAR + offset at 1-0 so
  any mismatch surfaces in the boot log.

Out of scope for step 1 specifically

  - **Write paths.** Step 1 reads sector 0 only. Write
    (opcode 0x01) and Flush (opcode 0x00) aren't needed
    until step 7 (real-hardware install medium); add then.
  - **Multi-namespace support.** Step 1 hardcodes NSID = 1.
    NVMe spec allows up to 2^32 - 1 namespaces; commodity
    consumer SSDs typically expose 1.
  - **Multiple controllers.** Step 1 finds the first NVMe
    PCI device and uses it; additional NVMe controllers
    (multi-disk systems, NVMe-of-fabrics) are post-M1.
  - **PCIe Hotplug.** Step 1 enumerates at boot only.
  - **NVMe Set Features / Get Features past the bring-up
    set.** Step 1 uses the default arbitration, default
    power state, no power management. PM at M1 step 7+ or
    M2 when power matters.
  - **Asynchronous Event Notifications.** AEN polling is
    Linux's monitor for SMART warnings, error log entries,
    namespace attach/detach. Post-M1.
  - **NVMe-MI** (Management Interface). BMC / out-of-band
    management. Not consumer-hardware territory; permanently
    out of scope.
  - **NVMe-oF** (over Fabrics). Network NVMe. Not commodity
    hardware; permanently out of scope.
  - **Filesystem on the NVMe device.** Step 1 reads raw
    sector 0. A filesystem driver (FAT32 read, ext2 read)
    is M1 step 7 territory at the earliest, more likely
    v0.5.

Permanently out of scope (do not propose)

  - Any unsafe block without a // SAFETY: comment naming the
    invariant the caller must uphold. CLAUDE.md hard rule.
  - Reverting any M0 commit. M0 closed and tagged.
  - Force-pushing to origin. Branch is in sync; preserve
    history.
  - Dropping BSD-2-Clause license header from any new file.
    nvme.rs is BSD-2; nvme spec is openly available with
    no copyright on protocol details.
  - Pulling a GPL crate into the kernel base for NVMe. Linux
    drivers via LinuxKPI is the only GPL path; NVMe is
    native Rust per ARSENAL.md.
  - Religious framing. CLAUDE.md hard rule.
  - Reintroducing HolyC. ADR-0004's discard is final.
  - Going back to stable Rust.
  - Skipping the build + smoke loop on a feat(kernel)
    commit.

Three notes worth flagging before you go

  1. **NVMe's controller-reset sequence is the most
     spec-fragile piece of step 1.** The order of writes
     (disable, program admin queue, enable) and the polling
     of CSTS.RDY across the transitions has half a dozen
     subtle wrong-order failure modes. Read the M1 step 1
     HANDOFF's failure-mode list above before kicking off
     1-2; better still, sketch the sequence as inline ASCII
     in nvme.rs's `init` function before writing any code.
     The NVMe 1.4 spec § 7.6.1 (Controller Initialization)
     is the canonical reference; § 3.5.1 (Memory-Based
     Transport Model Initialization) and § 5.21.1.7 (Feature
     Identifier 11 — Arbitration) are secondary. Half-
     spec'd implementations work on QEMU and fail on real
     hardware where timing matters; full-spec implementations
     work on both. M1 step 7's real-hardware bring-up will
     reward over-specification at step 1.

  2. **MSI-X programming is the moment the IRQ model evolves
     past M0's IDT.** The M0 IDT is `spin::Lazy<...>` —
     initialized once, used forever. M1 step 1 introduces
     `idt::register_vector(handler)` for dynamic allocation.
     The Lazy initializer at idt.rs runs ONCE; subsequent
     `register_vector` calls must mutate the IDT after the
     initial load. The x86_64 crate's `InterruptDescriptor
     Table` is mutable through `&mut`, but our IDT lives
     behind Lazy which only exposes `&IDT`. The natural
     refactor: replace Lazy with a `Mutex<Option<Inter
     ruptDescriptorTable>>` + a `register_vector(handler)`
     that locks, updates, and re-loads IDTR via `LIDT`.
     This is one of those "easy to get subtly wrong" pieces;
     the step 1-0 HANDOFF or commit body should document
     the locking discipline explicitly.

  3. **Step 1 is M1's velocity-establishment sub-step.** The
     M0 step 4 cadence (six sub-blocks in one calendar day)
     does not apply. M1 step 1 lands in 4-6 calendar weeks.
     If 1-2 (controller reset + admin queue) takes more
     than two sessions of grinding, that's the moment to
     pause, write up what's been tried, and step away for
     a day (CLAUDE.md cue: "This has been the active issue
     for three sessions. Want to write up what we've tried
     and step away for a day?"). Don't drive through. NVMe
     controller-reset failures have a stubborn habit of
     yielding to a fresh look after rest, and they rarely
     yield to grinding.

Wait for the pick. Do not pick silently. The natural first
split is 1-0 as a standalone session (MSI-X capability +
IDT vector tooling — foundational infrastructure with no
NVMe content yet), 1-1 in one focused session (NVMe device
discovery + BAR + register primitives), 1-2 as the longest
single sub-block of step 1 (controller reset + admin queue +
Identify, the spec-rich piece), 1-3 in one session that ends
with ARSENAL_NVME_OK firing, 1-4 as a short follow-up
(MSI-X conversion), 1-5 as the paper session. Use **go
m1-1-0** to start. Happy to combine 1-3 + 1-4 if you want
the "interrupt-driven NVMe" milestone in one push, or to
defer 1-0 and let 1-1 use a hand-coded MSI-X parser inline
in nvme.rs — the latter would couple MSI-X to NVMe though,
which 1-0's recommendation is specifically against. Your
call.
