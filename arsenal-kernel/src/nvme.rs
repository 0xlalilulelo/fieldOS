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

use crate::frames;
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
const OPC_IDENTIFY: u8 = 0x06;

/// CNS values for the Identify command (NVMe 1.4 §5.21, Table 246).
const CNS_NAMESPACE: u32 = 0x00;
const CNS_CONTROLLER: u32 = 0x01;

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

/// Admin queue state, owned by the Controller after `setup_admin`
/// returns. The frames backing sq_phys / cq_phys are allocated
/// once at admin-queue setup and live for the kernel's lifetime
/// (no Drop free path — M1's frame allocator doesn't have a use
/// case for "controller-shutdown" yet).
#[allow(dead_code)]
pub struct AdminQueue {
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
    /// None until `setup_admin` runs; Some thereafter. 1-3's I/O
    /// queue setup uses the admin queue from here.
    pub admin: Option<AdminQueue>,
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

    ctrl.admin = Some(AdminQueue {
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

    let cid = submit_admin(ctrl, OPC_IDENTIFY, 0, buf_phys, CNS_CONTROLLER);
    let status = poll_admin(ctrl, cid);
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

    let cid = submit_admin(ctrl, OPC_IDENTIFY, nsid, buf_phys, CNS_NAMESPACE);
    let status = poll_admin(ctrl, cid);
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

/// Write the SQE, advance the local tail, ring the SQ0 tail
/// doorbell. Returns the assigned CID so the caller can match
/// the completion.
fn submit_admin(ctrl: &mut Controller, opc: u8, nsid: u32, prp1: u64, cdw10: u32) -> u16 {
    // Stage the SQE + advance the local tail inside a short
    // admin-borrow scope, then release before touching MMIO via
    // ctrl.write32 (which needs &Controller, conflicting with
    // the &mut admin borrow if held across).
    let (cid, new_tail) = {
        let admin = ctrl
            .admin
            .as_mut()
            .expect("nvme: submit_admin without setup_admin");
        let cid = admin.next_cid;
        admin.next_cid = admin.next_cid.wrapping_add(1);

        let sqe = SqEntry {
            cdw0: (opc as u32) | ((cid as u32) << 16),
            nsid,
            _rsvd: 0,
            mptr: 0,
            prp1,
            prp2: 0,
            cdw10,
            cdw11: 0,
            cdw12: 0,
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        };

        let slot = admin.sq_tail as usize;
        let sq_base = phys_to_virt(admin.sq_phys) as *mut SqEntry;
        // SAFETY: sq_base points at the admin SQ frame (4 KiB,
        // 64 entries × 64 bytes). slot < ADMIN_QUEUE_SIZE.
        unsafe { sq_base.add(slot).write_volatile(sqe) };

        admin.sq_tail = (admin.sq_tail + 1) % ADMIN_QUEUE_SIZE;
        (cid, admin.sq_tail)
    };

    // SAFETY: doorbell at 0x1000 is inside the BAR0 mapping (16
    // KiB). The write tells the controller about the new SQE.
    // SQ0 tail doorbell is at DOORBELL_BASE + 0, independent of
    // DSTRD (the stride only affects subsequent doorbell offsets).
    unsafe { ctrl.write32(DOORBELL_BASE, new_tail as u32) };

    cid
}

/// Spin until a completion appears at admin.cq_head with the
/// expected phase tag. Return the (post-phase-strip) status field.
/// Panics if the completion's CID doesn't match `expected_cid` or
/// if the poll exceeds CSTS_POLL_LIMIT iterations.
fn poll_admin(ctrl: &mut Controller, expected_cid: u16) -> u16 {
    // Spec section §3.1.10: CQ0 head doorbell at
    // 0x1000 + (4 << CAP.DSTRD). Computed up front so the
    // admin-borrow scope inside the poll loop stays narrow.
    let cq_head_db = DOORBELL_BASE + (4usize << ctrl.doorbell_stride);

    for _ in 0..CSTS_POLL_LIMIT {
        // Narrow admin-borrow scope: read the candidate CQE,
        // advance bookkeeping if it's ours. Releasing the borrow
        // before ctrl.write32 keeps the doorbell write outside
        // the borrow.
        let outcome = {
            let admin = ctrl
                .admin
                .as_mut()
                .expect("nvme: poll_admin without setup_admin");
            let cq_base = phys_to_virt(admin.cq_phys) as *const CqEntry;
            // SAFETY: cq_base points at the admin CQ frame; cq_head
            // < ADMIN_QUEUE_SIZE.
            let cqe = unsafe { cq_base.add(admin.cq_head as usize).read_volatile() };
            let phase = (cqe.status & 1) as u8;
            if phase != admin.cq_phase {
                None
            } else {
                assert_eq!(
                    cqe.cid, expected_cid,
                    "nvme: admin CQE cid={} but expected {expected_cid} (status={:#06x})",
                    cqe.cid, cqe.status,
                );
                let status = cqe.status >> 1;
                admin.cq_head = (admin.cq_head + 1) % ADMIN_QUEUE_SIZE;
                if admin.cq_head == 0 {
                    admin.cq_phase ^= 1;
                }
                Some((status, admin.cq_head))
            }
        };

        if let Some((status, new_head)) = outcome {
            // SAFETY: cq_head_db is inside the BAR0 mapping; the
            // write ACKs consumption of the entry to the controller.
            unsafe { ctrl.write32(cq_head_db, new_head as u32) };
            return status;
        }
        core::hint::spin_loop();
    }
    panic!("nvme: timed out polling for admin CID {expected_cid}");
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
