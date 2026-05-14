# M1 step 1 — NVMe native Rust

*May 14, 2026. Same calendar day M0 closed. Six sub-blocks
(1-0 through 1-5), four feat commits, two paper deliverables
this devlog ships in.*

NVMe was the first M1 driver. Native Rust per ARSENAL.md (no
LinuxKPI dependency); the smallest useful M1 driver and the
first to exercise PCIe MSI-X. Two things the step had to ship
and did:

- A working NVMe driver — find the controller, reset, build
  admin queues, identify, build I/O queues, read a sector,
  validate the data — first end-to-end through polled
  completion at 1-3, then through MSI-X at 1-4.

- Foundation work the rest of M1 will reuse: PCIe MSI-X
  capability parsing in `pci.rs`, dynamic IDT vector
  allocation in `idt.rs`, the `pub unsafe fn pci::bar_address`
  helper, `apic::send_eoi` (already from 4-5, first reused
  by a non-LAPIC handler here).

The M1 milestone HANDOFF flagged seven spec-fragile pieces in
NVMe controller initialization. None tripped on first run.
QEMU's NVMe emulation is faithful enough that the spec-
correct sequence works without hardware-quirk workarounds;
real-hardware quirks land at M1 step 7 (first Framework
boot).

This devlog also closes M1 step 1 (paper sub-block 1-5 is
this devlog + the STATUS update). After 1-5 lands, M1 step 2
(LinuxKPI shim foundation) is active. Per the milestone
HANDOFF, step 2 is the "single largest engineering task" of
M1; budget 12-20 part-time weeks. Cadence note at the
bottom — the M0 → M1 transition is happening at a velocity
ARSENAL.md didn't anticipate, and the honest framing matters.

## What landed

Six commits between the step-1 HANDOFF at `077961d` and the
paper close-out below:

- `dd9f4a6` *feat(kernel): PCIe MSI-X capability + dynamic
  IDT vector allocation.* 1-0. Foundation step that the rest
  of step 1 plus every subsequent M1 driver consumes.

  Two pieces in one commit:

  *(a) PCIe MSI-X capability parsing in `pci.rs`.* Walks the
  capability list at config offset 0x34 looking for cap ID
  0x11. Returns `MsixInfo { bdf, cap_offset, table_size,
  table_bar, table_offset, pba_bar, pba_offset }`. Pattern
  mirrors 3C's virtio modern PCI transport walk (cap ID
  0x09). Legacy MSI (cap ID 0x05) deliberately NOT matched
  — M1 assumes MSI-X exclusively per ARSENAL.md's PCIe-first
  stance; legacy MSI is post-M1 if a real-hardware quirk
  ever demands it. Walk is bounded to 64 iterations against
  malformed capability chains.

  Added `Bdf` newtype (bus/dev/func triple) with a Debug
  impl that prints in the conventional `bb:dd.f` form.

  `pci::scan` now logs MSI-X info per device that has it.
  QEMU q35 observation: virtio-rng has 2 vectors, virtio-blk
  5, virtio-net 4 — all with table at BAR 1 offset 0, PBA
  at BAR 1 offset 0x800. The Intel host-bridge / BGA-graphics
  / ISA-bridge / IDE / SMBus devices have no MSI-X (legacy
  chipset devices, no PCIe IRQ need).

  *(b) Dynamic IDT vector allocation in `idt.rs`.* Replaces
  the `spin::Lazy<InterruptDescriptorTable>` storage from
  M0 with `spin::Mutex<InterruptDescriptorTable>` plus an
  `IDT_POPULATED` one-shot guard so populate-on-first-init
  runs exactly once across BSP + AP boots. Adds
  `register_vector(handler) -> u8` that allocates from a
  175-vector pool (0x40..=0xEE), holding an `irq::IrqGuard`
  for the brief critical section around the entry write.

  Pool layout pads against the M0 keyboard / timer /
  spurious vectors (0x21 / 0xEF / 0xFF). 175 vectors is far
  above what M1's driver fleet (NVMe ~5, xHCI ~4, virtio-gpu
  ~2, amdgpu ~8, iwlwifi ~4) will consume.

  `init()` switched to `load_unsafe` because the IDT lives
  behind a Mutex (not exposed as `&'static`) so the
  `load()` variant's `&'static self` requirement isn't
  satisfiable. The Mutex itself is static; the table
  memory outlives every IRQ; the load_unsafe contract is
  met. Each core LIDTs independently — BSP at boot, each
  AP from `ap_entry`.

  Cross-core visibility of post-init IDT changes
  (`register_vector` called after BSP init / AP boot) relies
  on the implicit happens-before chain on the device-wiring
  side: register_vector returns vector → caller writes
  vector into MSI-X Message Data → caller unmasks the entry
  → device may now fire. The IDT write strictly precedes
  the device-side enable, so a fired IRQ always finds the
  populated entry.

  ELF -8 KB (removing the `Lazy` wrapper saved more than
  the new surface added).

- `bc6ddac` *feat(kernel): NVMe device discovery + BAR
  mapping.* 1-1. Scans for the NVMe controller (PCI class
  01:08:02), maps BAR0 to a 16-KiB MMIO window via
  `paging::map_mmio` (covers controller registers + per-
  queue doorbells at 0x1000..=0x1100 with default DSTRD=0
  and 64 queue pairs of room), reads CAP and VS, asserts
  the spec features M1 step 1 relies on:

    - CAP.MPSMIN ≤ 12 → 4-KiB host pages supported
    - CAP.CSS bit 0 → NVM command set advertised

  Exposes a `Controller` handle with typed `read32` /
  `read64` / `write32` / `write64` MMIO helpers that 1-2's
  reset + admin-queue work consumes.

  Small refactor: lifted `bar_address` from `virtio.rs` to
  `pub unsafe fn pci::bar_address` since NVMe needs the
  same BAR resolution as virtio, and step 3 (xHCI) / step
  5 (amdgpu) will need it too. virtio.rs's two call sites
  updated; behavior unchanged. PCI is the right layer to
  own BAR resolution.

  QEMU q35 + `-device nvme` observation:
  - Controller at 00:04.0, BAR0 at 0xFEBD0000.
  - Version 1.4.0, CAP `0x004008200f0107ff`.
  - CAP.MQES=2047 (max queue entries 2048).
  - CAP.DSTRD=0 (doorbell stride 4 bytes).
  - CAP.CSS=0x41 (NVM command set + admin-only optional).
  - CAP.MPSMIN=0 (host page 4 KiB).
  - CAP.MPSMAX=4.

  The NVMe device appeared in the PCI scan with MSI-X
  `table_size:65` (admin + 64 I/O queues) at BAR0 offset
  0x2000, PBA at BAR0 offset 0x3000. The MSI-X table
  living inside BAR0 (not a separate BAR like virtio puts
  it) is what makes step 1-4's table programming cheap —
  the 16-KiB BAR0 mapping from 1-1 already covers it.

  Note: virtio-net shifted from 00:04.0 to 00:05.0 in the
  PCI scan because the new NVMe device inserts before it
  in slot order. No driver references hardcoded BDFs, so
  the shift is invisible to virtio_blk / virtio_net /
  smoltcp.

  `ci/qemu-smoke.sh` grew two lines: a `-drive
  file=$ISO,id=nvme0,readonly=on` and `-device
  nvme,serial=arsenal0,drive=nvme0`. Re-uses the ISO file
  as the NVMe backing so 1-3's sector 0 read sees the
  same hybrid-ISO MBR signature 0xAA55 that virtio-blk's
  smoke validates. QEMU allows the same file across
  multiple drives when both are readonly=on.

- `061e3cb` *feat(kernel): NVMe controller reset + admin
  queue + Identify.* 1-2. The spec-rich block. The full
  canonical NVMe controller-initialization sequence (NVMe
  1.4 §7.6.1) plus admin SQ/CQ allocation plus polled
  command submission plus the two Identify commands the
  HANDOFF specified.

  Sequence:

    1. Read CC; if `CC.EN == 1`, write `CC.EN = 0` and spin
       until `CSTS.RDY == 0`. Programming AQA/ASQ/ACQ
       while enabled is undefined per the spec.
    2. Allocate two 4-KiB frames from FRAMES for admin SQ
       + CQ. Frame-allocator alignment is the 4-KiB page
       alignment ASQ/ACQ register writes require — no
       manual alignment math. Zero both frames.
    3. Write AQA = (63 << 16) | 63 (queue depth 64, zero-
       based in both fields). Write ASQ + ACQ as 64-bit
       physical pointers.
    4. Write CC with `IOCQES=4` (16-byte CQEs), `IOSQES=6`
       (64-byte SQEs), `AMS=0` (round-robin), `MPS=0`
       (4-KiB pages), `CSS=0` (NVM command set), `EN=1`.
       The EN bit transitions last; spec-strict.
    5. Spin until `CSTS.RDY == 1`. `CSTS_POLL_LIMIT` (100M
       iterations) bounds the wait against a wedged
       controller; `CSTS.CFS` (Controller Fatal Status)
       check inside the loop panics rather than spinning
       forever.

  Submit/poll helpers:

  - `submit_admin` writes the SQE at the local tail,
    advances the tail, rings the SQ0 tail doorbell at
    offset 0x1000. The doorbell write is the load-bearing
    "tell the controller about new work" signal the
    HANDOFF specifically flagged in its failure-mode list.

  - `poll_admin` spins on the CQ entry at cq_head's phase
    tag. Initial expected phase is 1 (queue zero-filled;
    first completion writes phase=1). Phase flips on every
    CQ wrap. CID match assertion catches out-of-order
    completions. CQ head doorbell at `0x1000 + (4 <<
    DSTRD)` ACKs consumption.

  Borrow-checker dance worth noting: `ctrl.admin.as_mut()`
  can't coexist with `ctrl.write32` (the latter needs
  `&Controller`). Both `submit_admin` and `poll_admin`
  scope the `&mut admin` borrow narrowly, copy what's
  needed into stack locals, then release before the
  doorbell MMIO write. This same pattern carried into 1-3
  and 1-4 when the helpers got generalized.

  Two Identify commands per HANDOFF:
  - Identify Controller (CNS=0x01, NSID=0): parses
    SN/MN/FR/NN from the 4-KiB response per NVMe 1.4
    §5.21 Fig 247.
  - Identify Namespace (CNS=0x00, NSID=1): parses NSZE +
    FLBAS-indexed LBAF entry for LBA size per §5.21 Fig
    245.

  QEMU q35 + `-device nvme,serial=arsenal0` observation:
  ```
  nvme: admin queue up (sq_phys=0x000000000fc10000
                        cq_phys=0x000000000fc0f000 depth=64)
  nvme: ident-ctrl sn="arsenal0" mn="QEMU NVMe Ctrl"
                   fr="11.0.0" nn=256
  nvme: ident-ns nsid=1 nsze=37800 blocks lba_size=512 bytes
                 (lbads=9, flbas=0x00)
  ```

  37,800 × 512 = 19.3 MB, matching the ISO file's size
  (same ISO attached as the NVMe backing).

  None of the HANDOFF's seven flagged failure modes
  tripped: CAP read endianness right, CC.EN disable-before-
  enable right, AQA/ASQ/ACQ ordered before CC.EN=1, queue
  alignment from FRAMES, SQ doorbell on submission, CQ
  phase tag flip on wrap, MSI-X table BAR location (1-4's
  surface, also right).

- `a75541c` *feat(kernel): NVMe I/O queue + sector 0 read
  (polled).* 1-3. The cathartic block — first real NVMe
  I/O.

  Refactor in service of the I/O queue: `AdminQueue` →
  `NvmeQueue` with `qid: u16` + `size: u16` fields.
  `submit_admin` / `poll_admin` → `submit_qe` / `poll_qe`
  taking a `QueueKind` (Admin or Io). Doorbell offsets now
  derived from `queue.qid` and `CAP.DSTRD`
  (`DOORBELL_BASE + 2*qid*(4<<DSTRD)` for SQ tail; `+1`
  for CQ head) — works for any queue ID without per-queue
  specialized helpers. Controller grew an `io: Option<
  NvmeQueue>` field alongside the existing `admin`.

  I/O queue creation via the admin queue:
  - Allocate two 4-KiB frames for the I/O CQ + SQ.
  - Submit Create I/O CQ (opcode 0x05). `CDW10 = (size-1
    << 16) | qid`; `CDW11 = PC=1, IEN=0` (polled at 1-3;
    IEN=1 + IV at 1-4). PRP1 = cq_phys.
  - Submit Create I/O SQ (opcode 0x01). `CDW10` same;
    `CDW11 = (cqid << 16) | PC=1` (QPRIO=0 → urgent at
    lowest prio band). PRP1 = sq_phys.
  - Poll each admin completion; assert `status & 0xFFFE
    == 0`.

  Sector 0 read smoke:
  - Allocate one 4-KiB DMA buffer; zero it.
  - Submit NVM Read (opcode 0x02) via the I/O queue.
    `NSID=1, PRP1=buf_phys, PRP2=0` (single-PRP transfer
    fits in one host page), `CDW10=SLBA[31:0]=0,
    CDW11=SLBA[63:32]=0, CDW12=NLB=0` (one block, zero-
    based).
  - Poll I/O completion; assert status code.
  - Read u16 at buf+510 via `read_unaligned`; assert
    `== 0xAA55` (the hybrid-ISO MBR boot signature, same
    property 3C-3's virtio-blk smoke validates against
    the same ISO).
  - Emit ARSENAL_NVME_OK; free the buffer frame.

  This is the property M1 step 1 actually delivers
  ("first M1 driver works"): kernel finds the controller,
  brings it online, reads a block, validates the data.

- `dcd9ed1` *feat(kernel): NVMe MSI-X interrupts.* 1-4.
  Converts the I/O queue's polled completion (from 1-3)
  to MSI-X interrupt-driven. First consumer of 1-0's
  PCIe MSI-X capability parsing and dynamic IDT vector
  allocation.

  Pipeline (per HANDOFF §M1-1-4):
  1. `idt::register_vector(handler)` allocates the next
     vector from the 0x40..=0xEE pool. First call returns
     0x40.
  2. `program_msix_entry` writes the device's MSI-X table
     entry at index `IO_MSIX_INDEX`:
     ```
     +0   Message Address Low  = 0xFEE0_0000 | (apic_id << 12)
     +4   Message Address High = 0
     +8   Message Data         = vector (delivery=fixed, edge)
     +12  Vector Control       = 0 (unmasked)
     ```
     NVMe puts the MSI-X table at BAR0 offset 0x2000;
     the 16-KiB BAR0 mapping from 1-1 already covers it.
     Hard-asserts `table_bar == 0` + that the entry fits
     in the BAR0 map — a real-iron controller with the
     table in a different BAR would surface immediately.
  3. MSI-X enable via PCI config-space RMW: set bit 15
     of Message Control (== bit 31 of the dword at
     `cap_offset`). Adds `pci::config_write32`, the
     natural companion to the existing `config_read32`.
  4. Create-I/O-CQ admin command's `CDW11` changed from
     `PC=1/IEN=0/IV=0` (polled at 1-3) to
     `PC=1/IEN=1/IV=IO_MSIX_INDEX`. The controller now
     sends an MSI-X message for every CQE write to this
     queue.
  5. New IRQ handler `nvme_io_handler` is thin: bump
     `IO_IRQ_COUNT` (Release-ordered) + `apic::send_eoi`.
     No CQE drain in IRQ context — the cooperative
     consumer does that via the existing `poll_qe` (which
     spins on phase tag and finds it immediately on
     iteration 1 after the MSI fires).
  6. `smoke_read_sector_0` wraps the submit + wait in a
     brief sti/cli window. Main runs with IF=0 throughout
     boot until idle's sti at sched::init; this is the
     one boot-time IRQ-receive window opened before idle
     takes over. The LAPIC timer may fire concurrently —
     its handler increments TICKS, dispatches
     `sched::preempt` which no-ops on the empty runqueue
     (no tasks spawned yet), returns. No interference.

  Subtle correctness detail: NVMe's IV field in Create
  I/O CQ CDW11 is the **MSI-X table index**, NOT the IDT
  vector. The MSI-X table entry at that index encodes the
  IDT vector via the Message Data field. Easy to confuse;
  the comment in `program_msix_entry` makes this
  explicit.

  Observation:
  ```
  idt: registered vector 0x40
  nvme: msix entry 0 -> vector 0x40 apic_id 0; msix-enabled
  nvme: io queue up (qid=1 ... ien=1 iv=0)
  nvme: sector 0 read OK (status=0x0000, sig=0xaa55, irq_count=1)
  ```

  The IRQ counter incremented exactly once — clean edge-
  triggered MSI delivery, no spurious wakeups.

## Numbers

| Sub | Commit  | ELF (bytes)         | Smoke (ms)    | LOC delta |
| --- | ------- | ------------------- | ------------- | --------- |
| —   | 1b316c9 | 1,518,768 (M0 exit) | ~1.2-1.5 s   | —         |
| 1-0 | dd9f4a6 | 1,510,448 (-8 KB)   | ~1185-1255 ms | +247 / -22 |
| 1-1 | bc6ddac | 1,514,560 (+4 KB)   | ~1235-1825 ms | +313 / -29 |
| 1-2 | 061e3cb | 1,515,144 (+600 B)  | ~1354-1569 ms | +399 / -6  |
| 1-3 | a75541c | 1,523,600 (+8 KB)   | ~1229-1453 ms | +279 / -75 |
| 1-4 | dcd9ed1 | 1,523,856 (+256 B)  | ~1322-2024 ms | +205 / -14 |

`nvme.rs` totals ~880 LOC at step 1 exit. ARSENAL.md said
"~5K LOC" as a ceiling; the M1 step 1 HANDOFF estimated
"600-800 LOC actual." Landed in range, leaning slightly
above the upper bound but well under the ARSENAL.md
ceiling. The minimum-viable shape doesn't need the long
tail of features (write paths, multi-namespace, AEN, NVMe
Set Features past bring-up, NVMe-MI, NVMe-oF) the spec
ceiling anticipated.

Smoke pass time grew slightly because of the witness task
from 4-4 plus the new NVMe init work; deterministic at 14
sentinels across all five runs. ELF +5 KB net across the
six sub-blocks (the 1-0 `Lazy → Mutex` refactor saved
8 KB; the NVMe driver itself added ~13 KB; net +5 KB).

## Trade-offs that resolved in flight

The step-1 HANDOFF surfaced seven trade-off pairs. All
seven resolved as the HANDOFF recommended; none flipped
mid-flight.

- *Polled completion at 1-3, MSI-X follow-up at 1-4.* Did
  it this way — bisect-rich; 1-3's commit verifiably shows
  end-to-end NVMe I/O working before the MSI-X wiring
  complicates the picture.
- *Single shared I/O queue.* One queue at QID=1 with depth
  64. Per-CPU queues are M2 work.
- *DMA buffer source: frame allocator.* Page-aligned by
  definition. The pattern carries forward through every
  M1 driver.
- *Single nvme.rs file.* Stayed at ~880 LOC — comfortably
  under the 1500-LOC threshold where the directory-per-
  driver shape would start to pay off.
- *Single ARSENAL_NVME_OK sentinel.* NVMe-as-a-whole is
  one observable property; same sentinel fires through
  the polled path (1-3) and the IRQ-driven path (1-4)
  because the same property is asserted.
- *Additive `-device nvme` alongside virtio-blk.* Both
  drives reference the same ISO. virtio-blk + ARSENAL_
  BLK_OK still fire; NVMe is additive. Step 7 (real
  Framework boot) is the natural deletion point for
  virtio-blk.
- *Target NVMe 1.4.* QEMU reports 1.4.0 in VS. Real
  Framework NVMe will report 1.4 or 2.0; the M1 step 1
  spec-feature set is the 1.0+ baseline either way.

## Foundation work step 3+ will consume

1-0's two pieces are the highest-leverage parts of step 1
because they're reused by every later M1 driver:

- **`pci::msix_info` + `pci::config_write32`** — every
  PCIe driver needs to find its MSI-X capability and
  enable MSI-X. xHCI (step 3) and virtio-gpu (step 4)
  consume directly; amdgpu (step 5) and iwlwifi (step 6)
  will too via the LinuxKPI shim's `pci_*` family.

- **`idt::register_vector`** — every IRQ-driven driver
  needs a dynamically-allocated IDT vector. Same consumer
  list. The Lazy → Mutex IDT refactor was small (50 LOC)
  but its absence would have forced a re-design at the
  first driver-side IRQ-handler installation.

The `pub unsafe fn pci::bar_address` lift from `virtio.rs`
is a third foundation piece, smaller in scope. xHCI and
amdgpu will use it to resolve their controller BARs.

## What M1 step 2 looks like

Per the milestone HANDOFF (commit 9df4682, in git history
since it got overwritten by the step-1 kickoff):

> Build the smallest viable shim surface that satisfies
> one inherited driver — recommend a simple Linux driver
> as the first target (a virtio-balloon driver, or a small
> Intel NIC driver from net/ethernet/intel/e1000 — decide
> at the step kickoff). The shim covers printk-style
> logging, kmalloc/kfree against our heap, GFP_KERNEL
> flags as no-ops, struct device + driver registration,
> pci_register_driver / pci_unregister_driver, IRQ
> registration via our IDT / IOAPIC, basic locking
> (spinlock_t / mutex_t mapping to spin::Mutex).

ARSENAL.md flags step 2 as "the single largest engineering
task" of M1. The morale-load-bearing dimension is real:
12-20 part-time weeks of work that doesn't ship anything
user-visible on its own. The HANDOFF specifically called
out that the step-2 plan should include explicit
intermediate milestones ("one shim API surface lands +
compiles + has a smoke test, repeat") so progress is
visible week-over-week, not just at the end.

First driver target for step 2: virtio-balloon was the
HANDOFF recommendation (~600 LOC inherited C, pure virtio-
bus interaction). The alternative was e1000 (~3000 LOC,
more shim surface tested early). Final decision at the
step-2 HANDOFF kickoff.

The next session writes that step-2 HANDOFF.

## Cadence note — being honest about velocity

The M0 milestone retrospective (devlog at `2026-05-arsenal-
smp.md`) closed with "Calendar pace, honestly" — the
observation that M0 finished in 16 calendar days post-
pivot against ARSENAL.md's 0-9 month budget, and that this
pace was *initial-condition* concentration, not the
sustainable cadence. The recommendation was: don't project
the M0 pace forward to M1.

M1 step 1 then proceeded to finish in **one calendar day**
on the same day M0 closed. Six sub-blocks. Four feat
commits plus the M1 milestone HANDOFF, the step-1 HANDOFF,
and these paper deliverables.

This is not what ARSENAL.md projected. The honest framing:

- The post-pivot concentration has not actually let up.
  The "rest week before M1" the SMP devlog speculated
  about did not happen.
- NVMe is well-suited to fast implementation: the spec
  is small (the relevant 1.0 subset is ~50 pages of dense
  reading), QEMU's emulation is faithful, and there's
  prior Rust art (Redox, Theseus, the `nvme` crate) that
  validates approach without our needing to vendor it.
- The M1 milestone HANDOFF estimated 4-6 weeks at part-
  time pace for step 1. Step 1 took ~8 focused hours.
- The remaining steps are not all going to be like this.
  Step 2 (LinuxKPI shim) does not have a clean spec to
  follow — it's "build whatever the next inherited
  driver needs," iterated. Step 5 (amdgpu) is ~10K LOC
  of inherited C glued through the shim; the shim
  design's quality determines how much pain that step
  surfaces. Step 7 (real Framework boot) is the first
  real-hardware contact and will surface bugs QEMU
  doesn't show.

So: M1 step 1's pace is real data about NVMe specifically
and about the post-pivot concentration window. It is *not*
a basis for projecting M1's total calendar. The ARSENAL.md
month-9-to-month-24 budget for M1 remains the honest
projection; the concentration window will close eventually
(burnout, other life, calendar attrition). The right
posture is gratitude for the data and continued discipline
against the budget.

The M1 plan still says ~67 part-time weeks summed across
all 9 steps. Even if step 1 closed in 8 hours, the *plan*
doesn't shrink — the variance is now concentrated in
later steps where it always lived.

## Cadence

This is the ninth devlog of the post-pivot project arc.
M0's eight (one per step through 3A-3G plus the step 4
wrap), now M1 step 1's one. Same pattern: detail-rich
while the work is fresh, milestone-level summary
absorbed into the step-exit devlog when the step is
single-author / single-block enough to warrant it.

M1 step 2's cadence will be different. Per the milestone
HANDOFF: "one devlog per cluster of related sub-blocks"
since LinuxKPI's sub-blocks span weeks each. The shim
isn't a single-cadence story; each API surface (printk,
heap, IRQ, locks, PCI, device) is its own writeup
naturally.

M1 step 1 is complete. The next session writes the
step-2 HANDOFF. Onward.
