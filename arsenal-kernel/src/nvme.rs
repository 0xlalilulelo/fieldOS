// SPDX-License-Identifier: BSD-2-Clause
//
// NVMe driver — M1 step 1. Native Rust per ARSENAL.md (~5K LOC
// ceiling; target ~600-800 LOC at step exit). No LinuxKPI shim
// dependency. The first M1 driver and the first to exercise PCIe
// MSI-X paths every later driver (xHCI, virtio-gpu, amdgpu,
// iwlwifi) also needs.
//
// M1-1-1 (this commit) covers device discovery, BAR mapping, and
// controller-register read/write primitives. The Controller
// handle returned by `init` is what 1-2's reset + admin queue
// work consumes; 1-3 builds the I/O queue + first sector read on
// top of that; 1-4 converts to MSI-X interrupt-driven completion.
//
// Spec reference: NVMe 1.4 base specification, particularly §3.1
// (Register Definition), §7.6.1 (Controller Initialization), and
// §4 (Admin and NVM Command Set). The 1.4 spec is what QEMU's
// nvme device emulates by default and what every consumer SSD
// shipped in the last 5+ years implements.

use core::fmt::Write;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU64, Ordering};

use x86_64::structures::idt::InterruptStackFrame;

use crate::apic;
use crate::frames;
use crate::idt;
use crate::paging;
use crate::pci;
use crate::serial;

/// PCI class code for NVMe controllers: 01 (mass storage) :
/// 08 (NVMe) : 02 (NVMe I/O command set).
const NVME_CLASS: u8 = 0x01;
const NVME_SUBCLASS: u8 = 0x08;
const NVME_PROG_IF: u8 = 0x02;

/// Bytes of BAR0 mapping. Spec mandates controller registers at
/// offsets 0x00..=0x1000 plus per-queue doorbells starting at
/// 0x1000; with default DSTRD=0 (4-byte stride) and 32 queue
/// pairs, doorbells occupy 0x1000..=0x1100. Map 16 KiB to cover
/// that plus the optional Controller Memory Buffer header
/// registers up to ~0x4000. 1-2 widens this if CAP.DSTRD or a
/// large queue count demands more.
const BAR0_MAP_SIZE: u64 = 0x4000;

/// Spec register offsets (NVMe 1.4 §3.1, Table 27).
#[allow(dead_code)]
pub const REG_CAP: usize = 0x0000; // 64-bit (controller capabilities)
#[allow(dead_code)]
pub const REG_VS: usize = 0x0008; // 32-bit (version)
#[allow(dead_code)]
pub const REG_CC: usize = 0x0014; // 32-bit (controller configuration)
#[allow(dead_code)]
pub const REG_CSTS: usize = 0x001C; // 32-bit (controller status)
#[allow(dead_code)]
pub const REG_AQA: usize = 0x0024; // 32-bit (admin queue attributes)
#[allow(dead_code)]
pub const REG_ASQ: usize = 0x0028; // 64-bit (admin submission queue base)
#[allow(dead_code)]
pub const REG_ACQ: usize = 0x0030; // 64-bit (admin completion queue base)
#[allow(dead_code)]
pub const DOORBELL_BASE: usize = 0x1000;

/// Admin queue depth — 64 entries fits in one 4-KiB frame at the
/// configured 64-byte SQ entry size (64 × 64 = 4096) and 16-byte
/// CQ entry size (64 × 16 = 1024, well under 4 KiB).
const ADMIN_QUEUE_SIZE: u16 = 64;

/// CC register fields (NVMe 1.4 §3.1.5).
const CC_EN: u32 = 1 << 0;
const CC_CSS_NVM: u32 = 0 << 4;
const CC_MPS_4K: u32 = 0 << 7;
const CC_AMS_RR: u32 = 0 << 11;
const CC_IOSQES_64: u32 = 6 << 16;
const CC_IOCQES_16: u32 = 4 << 20;

/// CSTS register fields (NVMe 1.4 §3.1.6).
const CSTS_RDY: u32 = 1 << 0;
const CSTS_CFS: u32 = 1 << 1;

/// Admin command opcodes (NVMe 1.4 §5).
const OPC_CREATE_IO_SQ: u8 = 0x01;
const OPC_CREATE_IO_CQ: u8 = 0x05;
const OPC_IDENTIFY: u8 = 0x06;

/// NVM I/O command opcodes (NVMe 1.4 §6).
const OPC_READ: u8 = 0x02;

/// CNS values for the Identify command (NVMe 1.4 §5.21, Table 246).
const CNS_NAMESPACE: u32 = 0x00;
const CNS_CONTROLLER: u32 = 0x01;

/// First I/O queue ID. Admin is queue 0; M1 step 1 uses a single
/// I/O queue pair at QID=1 (HANDOFF.md "Number of I/O queue pairs"
/// trade-off resolved (i): single shared I/O queue).
const IO_QID: u16 = 1;

/// MSI-X table index that backs the I/O queue's IRQ. NVMe Create
/// I/O CQ's IV field names this index; the MSI-X table at this
/// index encodes the IDT vector + LAPIC destination.
const IO_MSIX_INDEX: u16 = 0;

/// MSI-X table entry layout (PCIe Base Spec §6.8.5, Table 6-9):
///   +0   Message Address Low  (bit 2 = dest mode 0=phys, bits 12..19 = dest ID)
///   +4   Message Address High (zero for our 32-bit MSI-X address)
///   +8   Message Data         (low 8 bits = vector; delivery / trigger zero)
///   +12  Vector Control       (bit 0 = mask; clear to enable)
const MSIX_ENTRY_BYTES: usize = 16;

/// MSI-X Message Control field bit 15 — MSI-X Enable. Setting this
/// in the device's PCI capability turns on MSI-X delivery for the
/// whole device. The Message Control field is a 16-bit RW
/// register that occupies bits 16..31 of the dword at cap_offset.
const MSIX_CTRL_ENABLE_DWORD_BIT: u32 = 1 << 31;

/// Count of MSI-X interrupts the I/O completion queue has fired.
/// Bumped by `nvme_io_handler` in IRQ context; cooperative code
/// snapshots before submit and spins until the count advances.
/// Single I/O queue at M1 step 1; per-queue counters would be the
/// shape for multi-queue at M2+.
static IO_IRQ_COUNT: AtomicU64 = AtomicU64::new(0);

/// IRQ handler for the I/O completion queue. The thin shape —
/// just a counter bump and EOI — keeps IRQ latency tiny; the
/// cooperative consumer drains the actual CQE state outside IRQ
/// context. EOI is required because MSI-X delivers through the
/// LAPIC just like every other vector in M0.
extern "x86-interrupt" fn nvme_io_handler(_frame: InterruptStackFrame) {
    IO_IRQ_COUNT.fetch_add(1, Ordering::Release);
    apic::send_eoi();
}

/// I/O queue depth — same 64 as admin. Far below QEMU's reported
/// MQES (2048); M1 single-block read is well within one queue
/// page's worth of entries.
const IO_QUEUE_SIZE: u16 = 64;

/// CSTS poll bound. QEMU TCG transitions in microseconds; real
/// hardware may take tens of milliseconds. 100M iterations of a
/// read + compare loop comfortably bounds either before the
/// smoke harness's 15-s timeout.
const CSTS_POLL_LIMIT: u64 = 100_000_000;

/// 64-byte Admin / I/O submission queue entry (NVMe 1.4 §4.2,
/// Figure 14). Layout matches the spec exactly; #[repr(C)] keeps
/// field order without packing — every field is naturally aligned.
#[repr(C)]
#[derive(Clone, Copy)]
struct SqEntry {
    /// bits 0..7 = opcode, bits 16..31 = Command Identifier.
    cdw0: u32,
    nsid: u32,
    _rsvd: u64,
    mptr: u64,
    prp1: u64,
    prp2: u64,
    cdw10: u32,
    cdw11: u32,
    cdw12: u32,
    cdw13: u32,
    cdw14: u32,
    cdw15: u32,
}

/// 16-byte Admin / I/O completion queue entry (NVMe 1.4 §4.6.1,
/// Figure 26). bit 0 of `status` is the Phase Tag; bits 1..15 are
/// the Status Field (SC + SCT + CRD + M + DNR).
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CqEntry {
    dw0: u32,
    _rsvd: u32,
    sq_head: u16,
    sq_id: u16,
    cid: u16,
    status: u16,
}

/// Per-queue state for an NVMe submission/completion queue pair.
/// Used for the admin queue (qid=0) and the I/O queue (qid=1) at
/// M1 step 1. The frames backing sq_phys / cq_phys are allocated
/// once at queue setup and live for the kernel's lifetime (no
/// Drop free path — M1's frame allocator doesn't have a use case
/// for "controller-shutdown" yet).
#[allow(dead_code)]
pub struct NvmeQueue {
    /// 0 for admin, 1+ for I/O queues. Determines the doorbell
    /// offsets in BAR0.
    pub qid: u16,
    /// Queue depth. 64 throughout M1 step 1 (one frame per queue);
    /// post-M1 may grow per-queue to amortize MMIO doorbell cost.
    pub size: u16,
    pub sq_phys: u64,
    pub cq_phys: u64,
    /// Index of the next SQ slot we'll write. Tail follows the
    /// driver; the controller reads up to (but not including) tail.
    pub sq_tail: u16,
    /// Index of the next CQ slot the controller will write. The
    /// driver consumes entries at cq_head and ACKs by writing
    /// updated cq_head into the CQ head doorbell.
    pub cq_head: u16,
    /// Phase tag we expect on the next completion entry. Initially
    /// 1 (queue memory is zero-initialized; first completion flips
    /// to phase=1). Flips on every CQ wrap.
    pub cq_phase: u8,
    pub next_cid: u16,
}

/// Identifies which queue a submit/poll helper acts on. submit_qe
/// uses this to look up the right field on the Controller; the
/// doorbell offsets fall out of the queue's qid.
#[derive(Clone, Copy, PartialEq, Eq)]
enum QueueKind {
    Admin,
    Io,
}

// CAP register field accessors (NVMe 1.4 §3.1.1, Table 28).
fn cap_mqes(cap: u64) -> u16 {
    (cap & 0xFFFF) as u16
}
fn cap_dstrd(cap: u64) -> u8 {
    ((cap >> 32) & 0xF) as u8
}
fn cap_css(cap: u64) -> u8 {
    ((cap >> 37) & 0xFF) as u8
}
fn cap_mpsmin(cap: u64) -> u8 {
    ((cap >> 48) & 0xF) as u8
}
fn cap_mpsmax(cap: u64) -> u8 {
    ((cap >> 52) & 0xF) as u8
}

/// Resolved NVMe controller handle. Step 1-2 consumes this for
/// the reset + admin-queue bring-up; 1-3 uses it for the I/O
/// queue; 1-4 for MSI-X. The fields above bar0_phys are read-only
/// snapshots captured at init time so cooperative-context code
/// doesn't need to re-issue the MMIO read for properties that
/// can't change at runtime (CAP and VS are spec-immutable).
#[allow(dead_code)]
pub struct Controller {
    pub bdf: pci::Bdf,
    pub bar0_phys: u64,
    pub bar0_virt: usize,
    pub cap: u64,
    pub version: u32,
    pub doorbell_stride: u8,
    /// None until `setup_admin` runs; Some thereafter. M1-1-3's
    /// I/O queue setup issues admin commands through this.
    pub admin: Option<NvmeQueue>,
    /// None until `setup_io_queue` runs; Some thereafter. M1-1-3+
    /// reads/writes flow through this. Single I/O queue at M1
    /// (HANDOFF trade-off (i)); per-CPU queues are M2 work.
    pub io: Option<NvmeQueue>,
}

#[allow(dead_code)]
impl Controller {
    /// Read a 32-bit MMIO register at offset `reg`.
    ///
    /// # Safety
    /// `reg` must be within the BAR0 region mapped by `init`
    /// (0..BAR0_MAP_SIZE) and 4-byte aligned. The 64-bit CAP at
    /// offset 0 should use `read64`; reading it as two 32-bit
    /// halves under-the-hood works but loses the
    /// natural-alignment atomicity the spec relies on.
    pub unsafe fn read32(&self, reg: usize) -> u32 {
        // SAFETY: caller asserts reg is within the mapped BAR0;
        // bar0_virt comes from paging::map_mmio at init.
        unsafe { read_volatile((self.bar0_virt + reg) as *const u32) }
    }

    /// Read a 64-bit MMIO register at offset `reg`. Required for
    /// CAP (offset 0) — the spec specifies an atomic 64-bit read.
    ///
    /// # Safety
    /// As `read32`, plus `reg` must be 8-byte aligned and the
    /// register must be defined as 64-bit by the spec.
    pub unsafe fn read64(&self, reg: usize) -> u64 {
        // SAFETY: as read32.
        unsafe { read_volatile((self.bar0_virt + reg) as *const u64) }
    }

    /// Write a 32-bit MMIO register at offset `reg`.
    ///
    /// # Safety
    /// As `read32`, plus the value must be a spec-legal bit
    /// pattern for the register; writes to CC, AQA, ASQ, ACQ,
    /// and the doorbells have hardware side effects.
    pub unsafe fn write32(&self, reg: usize, val: u32) {
        // SAFETY: caller's contract.
        unsafe { write_volatile((self.bar0_virt + reg) as *mut u32, val) };
    }

    /// Write a 64-bit MMIO register at offset `reg`.
    ///
    /// # Safety
    /// As `write32`, plus `reg` must be 8-byte aligned.
    pub unsafe fn write64(&self, reg: usize, val: u64) {
        // SAFETY: caller's contract.
        unsafe { write_volatile((self.bar0_virt + reg) as *mut u64, val) };
    }
}

/// Find the first NVMe controller and probe it. Maps BAR0 via
/// paging::map_mmio, reads CAP / VS, asserts the spec features
/// M1 step 1 relies on, and logs a one-line summary. Returns a
/// Controller handle ready for 1-2's reset + admin-queue work.
///
/// Panics if no NVMe controller is found — M1 step 1 assumes
/// exactly one, attached via QEMU's `-device nvme` (or a real
/// Framework 13 AMD's onboard NVMe at step 7).
pub fn init() -> Controller {
    let (bdf, bar0_phys) = find_controller().expect(
        "nvme: no controller found — pass -device nvme,drive=nvme0 to QEMU",
    );

    // Limine's HHDM doesn't cover device MMIO; map the BAR before
    // the first dereference. Same chokepoint as 3C virtio BARs,
    // 3F LAPIC MMIO, 4-0 ACPI tables, 4-3 IOAPIC MMIO.
    paging::map_mmio(bar0_phys, BAR0_MAP_SIZE);
    let bar0_virt = bar0_phys as usize + paging::hhdm_offset() as usize;

    // SAFETY: BAR0 is mapped above; CAP at offset 0 is 64-bit and
    // 8-byte aligned (the BAR base is page-aligned); VS at offset
    // 8 is 32-bit and 4-byte aligned.
    let cap = unsafe { read_volatile(bar0_virt as *const u64) };
    let version = unsafe { read_volatile((bar0_virt + REG_VS) as *const u32) };

    let mqes = cap_mqes(cap);
    let dstrd = cap_dstrd(cap);
    let css = cap_css(cap);
    let mpsmin = cap_mpsmin(cap);
    let mpsmax = cap_mpsmax(cap);

    let v_major = (version >> 16) as u16;
    let v_minor = ((version >> 8) & 0xFF) as u8;
    let v_tertiary = (version & 0xFF) as u8;

    let _ = writeln!(
        serial::Writer,
        "nvme: controller at {bdf:?} bar0={bar0_phys:#018x} \
         version={v_major}.{v_minor}.{v_tertiary} cap={cap:#018x}",
    );
    let _ = writeln!(
        serial::Writer,
        "nvme: cap.mqes={mqes} (max queue entries {}) \
         cap.dstrd={dstrd} (doorbell stride {} bytes) \
         cap.css={css:#04x} cap.mpsmin={mpsmin} (min host page {} bytes) \
         cap.mpsmax={mpsmax}",
        mqes as u32 + 1,
        4u32 << dstrd,
        1u32 << (mpsmin + 12),
    );

    // M1 step 1 uses 4-KiB host pages exclusively; if CAP.MPSMIN
    // demands a larger minimum we'd need to re-think the frame
    // allocator's 4-KiB unit. Real-hardware controllers report
    // MPSMIN = 0 (4 KiB) universally; the assert protects against
    // a future virtual / hypothetical-hardware quirk.
    assert!(
        mpsmin <= 12,
        "nvme: CAP.MPSMIN={mpsmin} demands host page size > 4 KiB; \
         M1 step 1 uses 4-KiB pages exclusively"
    );
    // M1 step 1 uses the NVM command set; CAP.CSS bit 0 advertises
    // its support. Newer controllers may add ZNS (bit 1) or admin-
    // only (bit 7) modes; we don't consume those.
    assert!(
        css & 0x01 != 0,
        "nvme: CAP.CSS bit 0 (NVM command set) not advertised; \
         M1 step 1 uses NVM exclusively"
    );

    Controller {
        bdf,
        bar0_phys,
        bar0_virt,
        cap,
        version,
        doorbell_stride: dstrd,
        admin: None,
        io: None,
    }
}

/// Disable the controller (if enabled), allocate and program admin
/// queues, enable, wait ready. Leaves the Controller with
/// `admin = Some(_)` and CSTS.RDY = 1 — ready to submit Identify.
///
/// Failure modes flagged in the M1 step 1 HANDOFF and guarded
/// against here: CC.EN must transition 1→0 with CSTS.RDY=0
/// confirmation before re-programming AQA/ASQ/ACQ; all three
/// register writes must happen before CC.EN flips to 1; CSTS.CFS
/// (Controller Fatal Status) panics rather than spinning forever.
pub fn setup_admin(ctrl: &mut Controller) {
    assert!(ctrl.admin.is_none(), "nvme: setup_admin called twice");

    // 1. If the controller is currently enabled (firmware left it
    // running, or this is a re-init path), disable and wait for
    // ready=0 before touching AQA/ASQ/ACQ. The spec is explicit
    // that programming admin-queue registers while EN=1 is
    // undefined.
    // SAFETY: BAR0 mapped at init(); REG_CC is a 32-bit register.
    let cc = unsafe { ctrl.read32(REG_CC) };
    if cc & CC_EN != 0 {
        // SAFETY: same; writing CC.EN=0 initiates the disable.
        unsafe { ctrl.write32(REG_CC, cc & !CC_EN) };
        wait_csts(ctrl, CSTS_RDY, 0);
    }

    // 2. Allocate the two queue frames. 4-KiB frames are page-
    // aligned by definition of the frame allocator — exactly the
    // alignment ASQ / ACQ register writes require.
    let sq_frame = frames::FRAMES
        .alloc_frame()
        .expect("nvme: OOM allocating admin SQ frame");
    let cq_frame = frames::FRAMES
        .alloc_frame()
        .expect("nvme: OOM allocating admin CQ frame");
    let sq_phys = sq_frame.start_address().as_u64();
    let cq_phys = cq_frame.start_address().as_u64();

    // SAFETY: freshly-allocated frames at known physical addresses;
    // Limine's HHDM covers RAM so the virt mapping exists. We hold
    // exclusive ownership.
    unsafe {
        core::ptr::write_bytes(phys_to_virt(sq_phys) as *mut u8, 0, 4096);
        core::ptr::write_bytes(phys_to_virt(cq_phys) as *mut u8, 0, 4096);
    }

    // 3. Program admin queue registers. AQA encodes both queue
    // depths as (size - 1); ADMIN_QUEUE_SIZE=64 → 63 in both
    // fields. ASQ / ACQ are 64-bit physical pointers.
    let aqa = ((ADMIN_QUEUE_SIZE as u32 - 1) << 16) | (ADMIN_QUEUE_SIZE as u32 - 1);
    // SAFETY: 32 / 64-bit reg writes at dword/qword-aligned BAR
    // offsets; values are spec-legal.
    unsafe {
        ctrl.write32(REG_AQA, aqa);
        ctrl.write64(REG_ASQ, sq_phys);
        ctrl.write64(REG_ACQ, cq_phys);
    }

    // 4. Configure + enable. CC.EN is the last bit set after every
    // other field is locked in.
    let new_cc =
        CC_IOCQES_16 | CC_IOSQES_64 | CC_AMS_RR | CC_MPS_4K | CC_CSS_NVM | CC_EN;
    // SAFETY: BAR0 mapped; CC accepts the value above per
    // NVMe 1.4 §3.1.5.
    unsafe { ctrl.write32(REG_CC, new_cc) };

    // 5. Spin until the controller signals ready.
    wait_csts(ctrl, CSTS_RDY, CSTS_RDY);

    ctrl.admin = Some(NvmeQueue {
        qid: 0,
        size: ADMIN_QUEUE_SIZE,
        sq_phys,
        cq_phys,
        sq_tail: 0,
        cq_head: 0,
        cq_phase: 1,
        next_cid: 0,
    });

    let _ = writeln!(
        serial::Writer,
        "nvme: admin queue up (sq_phys={sq_phys:#018x} cq_phys={cq_phys:#018x} depth={ADMIN_QUEUE_SIZE})",
    );
}

/// Submit Identify Controller via the admin queue, poll for the
/// completion, parse and log the response.
pub fn identify_controller(ctrl: &mut Controller) {
    let buf_frame = frames::FRAMES
        .alloc_frame()
        .expect("nvme: OOM for Identify Controller buffer");
    let buf_phys = buf_frame.start_address().as_u64();
    let buf_virt = phys_to_virt(buf_phys);
    // SAFETY: freshly-allocated frame; we own it exclusively.
    unsafe { core::ptr::write_bytes(buf_virt as *mut u8, 0, 4096) };

    let cid = submit_qe(ctrl, QueueKind::Admin, OPC_IDENTIFY, 0, buf_phys, CNS_CONTROLLER, 0, 0);
    let status = poll_qe(ctrl, QueueKind::Admin, cid);
    assert_eq!(
        status & 0xFFFE,
        0,
        "nvme: Identify Controller returned non-zero status {status:#06x}",
    );

    // Identify Controller response layout (NVMe 1.4 §5.21, Fig 247).
    // SAFETY: buf_virt holds 4 KiB of valid response data; the
    // slices we project are within that region.
    let bytes = unsafe { core::slice::from_raw_parts(buf_virt as *const u8, 4096) };
    let sn = ascii_trim(&bytes[4..24]);
    let mn = ascii_trim(&bytes[24..64]);
    let fr = ascii_trim(&bytes[64..72]);
    let nn = u32::from_le_bytes(bytes[516..520].try_into().unwrap_or([0; 4]));
    let _ = writeln!(
        serial::Writer,
        "nvme: ident-ctrl sn=\"{sn}\" mn=\"{mn}\" fr=\"{fr}\" nn={nn}",
    );

    frames::FRAMES.free_frame(buf_frame);
}

/// Submit Identify Namespace for `nsid` via the admin queue, poll,
/// parse, log NSZE + LBADS.
pub fn identify_namespace(ctrl: &mut Controller, nsid: u32) {
    let buf_frame = frames::FRAMES
        .alloc_frame()
        .expect("nvme: OOM for Identify Namespace buffer");
    let buf_phys = buf_frame.start_address().as_u64();
    let buf_virt = phys_to_virt(buf_phys);
    // SAFETY: freshly-allocated frame; we own it exclusively.
    unsafe { core::ptr::write_bytes(buf_virt as *mut u8, 0, 4096) };

    let cid = submit_qe(ctrl, QueueKind::Admin, OPC_IDENTIFY, nsid, buf_phys, CNS_NAMESPACE, 0, 0);
    let status = poll_qe(ctrl, QueueKind::Admin, cid);
    assert_eq!(
        status & 0xFFFE,
        0,
        "nvme: Identify Namespace returned non-zero status {status:#06x}",
    );

    // Identify Namespace response (NVMe 1.4 §5.21, Fig 245).
    // SAFETY: as identify_controller.
    let bytes = unsafe { core::slice::from_raw_parts(buf_virt as *const u8, 4096) };
    let nsze = u64::from_le_bytes(bytes[0..8].try_into().unwrap_or([0; 8]));
    let flbas = bytes[26];
    let lbaf_index = (flbas & 0xF) as usize;
    // Each LBAF entry is 4 bytes at offsets 128, 132, 136, ...
    let lbaf_off = 128 + lbaf_index * 4;
    let lbaf =
        u32::from_le_bytes(bytes[lbaf_off..lbaf_off + 4].try_into().unwrap_or([0; 4]));
    let lbads = ((lbaf >> 16) & 0xFF) as u8;
    let lba_bytes = 1u64 << lbads;
    let _ = writeln!(
        serial::Writer,
        "nvme: ident-ns nsid={nsid} nsze={nsze} blocks lba_size={lba_bytes} bytes (lbads={lbads}, flbas={flbas:#04x})",
    );

    frames::FRAMES.free_frame(buf_frame);
}

/// Write the SQE, advance the local tail, ring the SQ tail
/// doorbell for the named queue. Returns the assigned CID so the
/// caller can match the completion. Works for both admin (qid=0)
/// and the I/O queue (qid=1+); the doorbell offset is computed
/// from queue.qid and CAP.DSTRD.
#[allow(clippy::too_many_arguments)] // NVMe SQEs natively pack a u8 opcode plus six CDWs;
// taking each as a typed parameter is more legible than a builder
// pattern at M1 step 1's call-site count (~4 submitters total).
fn submit_qe(ctrl: &mut Controller, kind: QueueKind, opc: u8, nsid: u32, prp1: u64,
             cdw10: u32, cdw11: u32, cdw12: u32) -> u16 {
    let dstrd = ctrl.doorbell_stride;
    // Stage the SQE + advance the local tail inside a short
    // borrow scope, then release before touching MMIO via
    // ctrl.write32 (which needs &Controller, conflicting with
    // the &mut queue borrow if held across).
    let (cid, new_tail, sq_tail_db) = {
        let q = queue_mut(ctrl, kind);
        let cid = q.next_cid;
        q.next_cid = q.next_cid.wrapping_add(1);

        let sqe = SqEntry {
            cdw0: (opc as u32) | ((cid as u32) << 16),
            nsid,
            _rsvd: 0,
            mptr: 0,
            prp1,
            prp2: 0,
            cdw10,
            cdw11,
            cdw12,
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        };

        let slot = q.sq_tail as usize;
        let sq_base = phys_to_virt(q.sq_phys) as *mut SqEntry;
        // SAFETY: sq_base points at the SQ frame (4 KiB, 64
        // entries × 64 bytes). slot < q.size.
        unsafe { sq_base.add(slot).write_volatile(sqe) };

        q.sq_tail = (q.sq_tail + 1) % q.size;
        let db = DOORBELL_BASE + (2 * q.qid as usize) * (4 << dstrd);
        (cid, q.sq_tail, db)
    };

    // SAFETY: sq_tail_db is at DOORBELL_BASE + 2*qid*(4<<DSTRD),
    // inside the 16 KiB BAR0 mapping. The write tells the
    // controller about the new SQE.
    unsafe { ctrl.write32(sq_tail_db, new_tail as u32) };

    cid
}

/// Spin until a completion appears at queue.cq_head with the
/// expected phase tag. Returns the (post-phase-strip) status
/// field. Panics if the completion's CID doesn't match
/// `expected_cid` or if the poll exceeds CSTS_POLL_LIMIT iters.
fn poll_qe(ctrl: &mut Controller, kind: QueueKind, expected_cid: u16) -> u16 {
    let dstrd = ctrl.doorbell_stride;
    for _ in 0..CSTS_POLL_LIMIT {
        // Narrow borrow scope: read the candidate CQE, advance
        // bookkeeping if it's ours. Releasing the borrow before
        // ctrl.write32 keeps the doorbell write outside the borrow.
        let outcome = {
            let q = queue_mut(ctrl, kind);
            let cq_base = phys_to_virt(q.cq_phys) as *const CqEntry;
            // SAFETY: cq_base points at the CQ frame; cq_head < q.size.
            let cqe = unsafe { cq_base.add(q.cq_head as usize).read_volatile() };
            let phase = (cqe.status & 1) as u8;
            if phase != q.cq_phase {
                None
            } else {
                assert_eq!(
                    cqe.cid, expected_cid,
                    "nvme: CQE cid={} but expected {expected_cid} (status={:#06x}) on qid={}",
                    cqe.cid, cqe.status, q.qid,
                );
                let status = cqe.status >> 1;
                q.cq_head = (q.cq_head + 1) % q.size;
                if q.cq_head == 0 {
                    q.cq_phase ^= 1;
                }
                let db = DOORBELL_BASE + (2 * q.qid as usize + 1) * (4 << dstrd);
                Some((status, q.cq_head, db))
            }
        };

        if let Some((status, new_head, db)) = outcome {
            // SAFETY: db is inside the BAR0 mapping; write ACKs
            // consumption of the entry to the controller.
            unsafe { ctrl.write32(db, new_head as u32) };
            return status;
        }
        core::hint::spin_loop();
    }
    panic!("nvme: timed out polling for CID {expected_cid} on {kind:?}",
           kind = match kind { QueueKind::Admin => "admin", QueueKind::Io => "io" });
}

/// Program the device's MSI-X table entry IO_MSIX_INDEX to deliver
/// IDT vector `vector` to APIC ID `bsp_apic_id` (fixed delivery,
/// physical destination mode, edge-triggered). Enable MSI-X on the
/// device by setting bit 15 of the cap's Message Control. Returns
/// `(table_index, vector)` for use as the IO CQ's IV / IDT
/// dispatch pair.
///
/// MSI message format (Intel SDM Vol. 3A §10.11.1, PCIe §6.1.4):
///   Message Address[31:20] = 0xFEE (LAPIC region prefix)
///   Message Address[19:12] = Destination APIC ID
///   Message Address[3]     = 0 (no redirection hint)
///   Message Address[2]     = 0 (physical destination mode)
///   Message Data[7:0]      = IDT vector
///   Message Data[10:8]     = 0 (fixed delivery)
///   Message Data[14]       = 0 (edge)
///   Message Data[15]       = 0 (edge)
fn program_msix_entry(ctrl: &mut Controller, vector: u8, bsp_apic_id: u8) {
    let msix = pci::msix_info(ctrl.bdf.bus, ctrl.bdf.dev, ctrl.bdf.func)
        .expect("nvme: device advertises no MSI-X capability");

    // NVMe puts the MSI-X table inside BAR0 at offset 0x2000 on
    // QEMU q35; our 16 KiB BAR0 mapping (BAR0_MAP_SIZE = 0x4000)
    // already covers that. Hard-asserting keeps a future surprise
    // (a real-iron controller with the table in a different BAR)
    // obvious instead of silently corrupting an unmapped page.
    assert_eq!(
        msix.table_bar, 0,
        "nvme: MSI-X table_bar={}, expected BAR 0 (M1 step 1 maps BAR0 only)",
        msix.table_bar,
    );
    assert!(
        (msix.table_offset as u64) + ((IO_MSIX_INDEX as u64 + 1) * MSIX_ENTRY_BYTES as u64)
            <= BAR0_MAP_SIZE,
        "nvme: MSI-X table entry {IO_MSIX_INDEX} at offset {:#x} past BAR0 map",
        msix.table_offset + (IO_MSIX_INDEX as u32) * MSIX_ENTRY_BYTES as u32,
    );

    let entry_virt = ctrl.bar0_virt
        + msix.table_offset as usize
        + IO_MSIX_INDEX as usize * MSIX_ENTRY_BYTES;
    let addr_lo = 0xFEE0_0000u32 | ((bsp_apic_id as u32) << 12);
    let data = vector as u32;

    // SAFETY: entry_virt is inside BAR0's HHDM-mapped MMIO; the
    // four 32-bit writes target the spec-defined MSI-X table layout
    // for entry IO_MSIX_INDEX. Vector Control bit 0 = 0 unmasks
    // the entry; with the device's overall MSI-X Enable still 0
    // at this moment, no MSI can fire yet.
    unsafe {
        write_volatile(entry_virt as *mut u32, addr_lo);
        write_volatile((entry_virt + 4) as *mut u32, 0);
        write_volatile((entry_virt + 8) as *mut u32, data);
        write_volatile((entry_virt + 12) as *mut u32, 0);
    }

    // Enable MSI-X on the device. Read-modify-write the dword at
    // cap_offset (Cap ID + Next in low 16, Message Control in high
    // 16). Setting bit 15 of Message Control == bit 31 of the
    // dword. Cap ID + Next + RO bits in Message Control (Table
    // Size, etc.) are read-only and write-as-read.
    // SAFETY: standard PCI config dword RMW; the bit pattern is
    // spec-legal per PCIe §6.8.6.
    let dw = unsafe {
        pci::config_read32(ctrl.bdf.bus, ctrl.bdf.dev, ctrl.bdf.func, msix.cap_offset)
    };
    let new_dw = dw | MSIX_CTRL_ENABLE_DWORD_BIT;
    // SAFETY: as above.
    unsafe {
        pci::config_write32(
            ctrl.bdf.bus,
            ctrl.bdf.dev,
            ctrl.bdf.func,
            msix.cap_offset,
            new_dw,
        )
    };

    let _ = writeln!(
        serial::Writer,
        "nvme: msix entry {IO_MSIX_INDEX} -> vector {vector:#04x} apic_id {bsp_apic_id}; \
         msix-enabled (cap_offset={:#x})",
        msix.cap_offset,
    );
}

/// Create one I/O completion queue + one I/O submission queue at
/// QID=1 via admin Create-I/O-CQ + Create-I/O-SQ commands.
/// Interrupt-driven completion via MSI-X: a fresh IDT vector,
/// programmed into MSI-X table entry IO_MSIX_INDEX, named in the
/// Create-I/O-CQ CDW11 IV field. The I/O queue's CQE arrivals fire
/// `nvme_io_handler` which bumps `IO_IRQ_COUNT`; cooperative
/// callers (currently `smoke_read_sector_0`) wait on the counter.
///
/// CDW10 for both Create commands (NVMe 1.4 §5.4, §5.5): bits 0..15
/// = QID, bits 16..31 = QSIZE (zero-based).
/// CDW11 for Create I/O CQ: bit 0 = PC (physically contiguous),
/// bit 1 = IEN (interrupts enabled), bits 16..31 = IV (MSI-X
/// table index — NOT the IDT vector; the MSI-X table entry at
/// that index encodes the vector).
/// CDW11 for Create I/O SQ: bit 0 = PC, bits 1..2 = QPRIO,
/// bits 16..31 = CQID.
pub fn setup_io_queue(ctrl: &mut Controller) {
    assert!(ctrl.io.is_none(), "nvme: setup_io_queue called twice");

    // Allocate an IDT vector for the I/O completion handler, then
    // program MSI-X to deliver it to the BSP. Done before queue
    // creation so the controller's first IEN=1 CQ fires through
    // an already-armed path.
    let vector = idt::register_vector(nvme_io_handler);
    program_msix_entry(ctrl, vector, apic::lapic_id());

    let cq_frame = frames::FRAMES
        .alloc_frame()
        .expect("nvme: OOM allocating I/O CQ frame");
    let sq_frame = frames::FRAMES
        .alloc_frame()
        .expect("nvme: OOM allocating I/O SQ frame");
    let cq_phys = cq_frame.start_address().as_u64();
    let sq_phys = sq_frame.start_address().as_u64();

    // SAFETY: freshly-allocated frames; we own them exclusively.
    unsafe {
        core::ptr::write_bytes(phys_to_virt(cq_phys) as *mut u8, 0, 4096);
        core::ptr::write_bytes(phys_to_virt(sq_phys) as *mut u8, 0, 4096);
    }

    let qsize_field = (IO_QUEUE_SIZE as u32 - 1) << 16;
    let cdw10 = qsize_field | (IO_QID as u32);

    // Create I/O CQ first (must exist before its paired SQ).
    // CDW11 = (IV << 16) | (IEN << 1) | PC. IEN=1, IV=IO_MSIX_INDEX.
    let cdw11_cq = ((IO_MSIX_INDEX as u32) << 16) | 0b11; // PC=1, IEN=1
    let cid = submit_qe(
        ctrl,
        QueueKind::Admin,
        OPC_CREATE_IO_CQ,
        0,
        cq_phys,
        cdw10,
        cdw11_cq,
        0,
    );
    let status = poll_qe(ctrl, QueueKind::Admin, cid);
    assert_eq!(
        status & 0xFFFE,
        0,
        "nvme: Create I/O CQ returned non-zero status {status:#06x}",
    );

    // Create I/O SQ targeting CQID=IO_QID. CDW11 = PC=1, QPRIO=0,
    // CQID in upper 16 bits.
    let cdw11_sq = ((IO_QID as u32) << 16) | 0x0000_0001;
    let cid = submit_qe(
        ctrl,
        QueueKind::Admin,
        OPC_CREATE_IO_SQ,
        0,
        sq_phys,
        cdw10,
        cdw11_sq,
        0,
    );
    let status = poll_qe(ctrl, QueueKind::Admin, cid);
    assert_eq!(
        status & 0xFFFE,
        0,
        "nvme: Create I/O SQ returned non-zero status {status:#06x}",
    );

    ctrl.io = Some(NvmeQueue {
        qid: IO_QID,
        size: IO_QUEUE_SIZE,
        sq_phys,
        cq_phys,
        sq_tail: 0,
        cq_head: 0,
        cq_phase: 1,
        next_cid: 0,
    });

    let _ = writeln!(
        serial::Writer,
        "nvme: io queue up (qid={IO_QID} sq_phys={sq_phys:#018x} cq_phys={cq_phys:#018x} depth={IO_QUEUE_SIZE} ien=1 iv={IO_MSIX_INDEX})",
    );
}

/// Read one logical block from `nsid` at `slba` into a freshly-
/// allocated 4-KiB DMA buffer, asserts the MBR boot-signature
/// 0xAA55 at byte 510, emits ARSENAL_NVME_OK on success. M1-1-4
/// uses MSI-X interrupt-driven completion: snapshot IO_IRQ_COUNT,
/// submit, enable interrupts, spin until counter advances, then
/// drain the (already-pending) CQE via the existing poll_qe path.
///
/// Mirrors the 3C virtio-blk sector-0 smoke pattern. Runs at boot
/// before sched::init, so IF=0 throughout main except the brief
/// sti window for this submission's MSI delivery.
pub fn smoke_read_sector_0(ctrl: &mut Controller, nsid: u32) {
    let buf_frame = frames::FRAMES
        .alloc_frame()
        .expect("nvme: OOM allocating read buffer");
    let buf_phys = buf_frame.start_address().as_u64();
    let buf_virt = phys_to_virt(buf_phys);
    // SAFETY: freshly-allocated frame; we own it exclusively.
    unsafe { core::ptr::write_bytes(buf_virt as *mut u8, 0, 4096) };

    // Read command (NVMe 1.4 §6.9 Figure 391):
    //   CDW10 = SLBA[31:0]
    //   CDW11 = SLBA[63:32]
    //   CDW12 = NLB[15:0] zero-based + flags
    // NLB=0 → one block. PRP1 = buffer phys; PRP2 unused for a
    // single 4-KiB transfer covering ≤ one host page.
    let slba: u64 = 0;
    let cdw10 = slba as u32;
    let cdw11 = (slba >> 32) as u32;
    let cdw12 = 0; // NLB=0 (one block), no flags

    // Snapshot the IRQ counter before submission so the wait
    // is "count > snapshot" — guards against a delivered-but-
    // not-yet-observed MSI from any earlier-than-now event.
    let target = IO_IRQ_COUNT.load(Ordering::Acquire) + 1;
    let cid = submit_qe(
        ctrl,
        QueueKind::Io,
        OPC_READ,
        nsid,
        buf_phys,
        cdw10,
        cdw11,
        cdw12,
    );

    // Enable interrupts so the MSI can deliver. Main runs with
    // IF=0 throughout boot until idle's sti at sched::init; this
    // is the one boot-time IRQ-receive window we open before idle
    // takes over. The LAPIC timer may fire concurrently — its
    // handler increments TICKS, dispatches sched::preempt which
    // no-ops on an empty runqueue (no tasks spawned yet), and
    // returns. No interference with the MSI we're waiting for.
    // SAFETY: sti / cli are privileged in ring 0, which we have;
    // no memory or stack effect beyond IF toggling.
    unsafe { core::arch::asm!("sti", options(nomem, nostack)) };
    while IO_IRQ_COUNT.load(Ordering::Acquire) < target {
        core::hint::spin_loop();
    }
    // SAFETY: see above.
    unsafe { core::arch::asm!("cli", options(nomem, nostack)) };

    // The CQE is now present in the I/O CQ (the MSI fired because
    // the controller wrote it). poll_qe spins on the phase tag,
    // finds the match on iteration 1, advances bookkeeping, and
    // writes the CQ head doorbell — same path the polled version
    // at 1-3 used, just guaranteed to return immediately.
    let status = poll_qe(ctrl, QueueKind::Io, cid);
    assert_eq!(
        status & 0xFFFE,
        0,
        "nvme: Read sector 0 returned non-zero status {status:#06x}",
    );

    // SAFETY: buf_virt holds the 4-KiB read result; the bytes
    // [510..512] are within the first sector regardless of LBA
    // size (M1 step 1 asserts 512-byte LBAs at identify_ns).
    let sig: u16 = unsafe { core::ptr::read_unaligned((buf_virt + 510) as *const u16) };
    assert_eq!(
        sig, 0xAA55,
        "nvme: sector 0 MBR signature mismatch (got {sig:#06x}); the QEMU NVMe backing should be the same hybrid ISO virtio-blk reads at 3C-3"
    );

    let _ = writeln!(
        serial::Writer,
        "nvme: sector 0 read OK (status={status:#06x}, sig=0xaa55, irq_count={})",
        IO_IRQ_COUNT.load(Ordering::Relaxed),
    );
    serial::write_str("ARSENAL_NVME_OK\n");

    frames::FRAMES.free_frame(buf_frame);
}

fn queue_mut(ctrl: &mut Controller, kind: QueueKind) -> &mut NvmeQueue {
    match kind {
        QueueKind::Admin => ctrl
            .admin
            .as_mut()
            .expect("nvme: admin queue not set up"),
        QueueKind::Io => ctrl
            .io
            .as_mut()
            .expect("nvme: I/O queue not set up"),
    }
}

/// Spin until CSTS bits under `mask` match `expected`. Panics on
/// CSTS.CFS (Controller Fatal Status) — the spec recommends
/// resetting and giving up, but at M1 step 1 a fatal controller
/// is a real bug, not a graceful-degradation case.
fn wait_csts(ctrl: &Controller, mask: u32, expected: u32) {
    for _ in 0..CSTS_POLL_LIMIT {
        // SAFETY: REG_CSTS at offset 0x1C is 32-bit aligned MMIO.
        let csts = unsafe { ctrl.read32(REG_CSTS) };
        assert!(
            csts & CSTS_CFS == 0,
            "nvme: CSTS.CFS (Controller Fatal Status) set: csts={csts:#010x}",
        );
        if (csts & mask) == expected {
            return;
        }
        core::hint::spin_loop();
    }
    panic!("nvme: timed out waiting for CSTS mask={mask:#x} expected={expected:#x}");
}

fn phys_to_virt(phys: u64) -> usize {
    (phys + paging::hhdm_offset()) as usize
}

/// Trim trailing ASCII spaces from a fixed-width spec field and
/// return as &str (with non-UTF8 fallback).
fn ascii_trim(bytes: &[u8]) -> &str {
    let end = bytes
        .iter()
        .rposition(|&b| b != b' ' && b != 0)
        .map(|p| p + 1)
        .unwrap_or(0);
    core::str::from_utf8(&bytes[..end]).unwrap_or("(non-ascii)")
}

/// Scan the PCI bus for the first NVMe controller and return its
/// BDF + BAR0 physical address. Returns None if no NVMe device is
/// present. Walks the same brute-force bus/dev/func space pci::scan
/// uses; class-code match means we don't need a vendor allowlist
/// (every NVMe spec-compliant controller reports 01:08:02).
fn find_controller() -> Option<(pci::Bdf, u64)> {
    for bus in 0u16..=255 {
        for dev in 0u8..32 {
            if let Some(found) = check_function(bus as u8, dev, 0) {
                return Some(found);
            }
            // SAFETY: standard PCI dword read at the
            // multi-function header offset.
            let header_dword = unsafe { pci::config_read32(bus as u8, dev, 0, 0x0C) };
            if (header_dword >> 16) & 0x80 != 0 {
                for func in 1u8..8 {
                    if let Some(found) = check_function(bus as u8, dev, func) {
                        return Some(found);
                    }
                }
            }
        }
    }
    None
}

fn check_function(bus: u8, dev: u8, func: u8) -> Option<(pci::Bdf, u64)> {
    // SAFETY: standard PCI dword reads at dword-aligned offsets;
    // 0xFFFF vendor short-circuits absent functions.
    let id = unsafe { pci::config_read32(bus, dev, func, 0x00) };
    if (id & 0xFFFF) as u16 == 0xFFFF {
        return None;
    }
    let class_dword = unsafe { pci::config_read32(bus, dev, func, 0x08) };
    let class_code = ((class_dword >> 24) & 0xFF) as u8;
    let subclass = ((class_dword >> 16) & 0xFF) as u8;
    let prog_if = ((class_dword >> 8) & 0xFF) as u8;
    if class_code != NVME_CLASS || subclass != NVME_SUBCLASS || prog_if != NVME_PROG_IF
    {
        return None;
    }
    // SAFETY: BAR 0 is in range; NVMe specs that BAR0 holds the
    // 64-bit MMIO controller-register region.
    let bar0_phys = unsafe { pci::bar_address(bus, dev, func, 0) };
    if bar0_phys == 0 {
        return None;
    }
    Some((pci::Bdf { bus, dev, func }, bar0_phys))
}
