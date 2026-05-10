// SPDX-License-Identifier: BSD-2-Clause
//
// virtio-blk driver, single-shot sector-0 read smoke (3C-3).
//
// Initializes the first virtio-blk device found, accepts only
// VIRTIO_F_VERSION_1 (forces modern transport, declines every
// optional feature), activates queue 0 with 16 descriptors,
// submits one VIRTIO_BLK_T_IN read request as a 3-descriptor chain
// (header → data → status), polls via sched::yield_now until the
// used ring advances, and asserts the hybrid-ISO MBR boot
// signature 0xAA55 at offset 510..512 of the returned data.
//
// Polling-not-IRQ is deliberate. 3F brings the LAPIC timer + IRQ
// delivery; until then every "wait for completion" is a yield
// loop. CPU is pegged during the wait, but a single 512-byte read
// completes in microseconds under TCG and the cooperative
// scheduler routes the polling time back to the idle task, so
// nothing else stalls.
//
// Feature negotiation logs every accepted/rejected bit so a
// future "the device is silently stalling" bug can be bisected
// against the trace.
//
// References:
//   virtio v1.2 § 3.1 (device init)
//   virtio v1.2 § 4.1.4 (modern PCI transport — common cfg layout)
//   virtio v1.2 § 5.2   (block device specifics)

use core::fmt::Write;
use core::ptr::{read_volatile, write_volatile};

use alloc::boxed::Box;

use crate::paging;
use crate::sched;
use crate::serial;
use crate::virtio;

const VIRTIO_BLK_DEVICE_ID: u16 = 0x1001;

// virtio_pci_common_cfg field offsets (virtio v1.2 § 4.1.4.3).
const CC_DEVICE_FEATURE_SELECT: usize = 0x00;
const CC_DEVICE_FEATURE: usize = 0x04;
const CC_DRIVER_FEATURE_SELECT: usize = 0x08;
const CC_DRIVER_FEATURE: usize = 0x0C;
const CC_NUM_QUEUES: usize = 0x12;
const CC_DEVICE_STATUS: usize = 0x14;
const CC_QUEUE_SELECT: usize = 0x16;
const CC_QUEUE_SIZE: usize = 0x18;
const CC_QUEUE_ENABLE: usize = 0x1C;
const CC_QUEUE_NOTIFY_OFF: usize = 0x1E;
const CC_QUEUE_DESC: usize = 0x20;
const CC_QUEUE_DRIVER: usize = 0x28;
const CC_QUEUE_DEVICE: usize = 0x30;

// device_status bits (virtio v1.2 § 2.1).
const STATUS_ACKNOWLEDGE: u8 = 1;
const STATUS_DRIVER: u8 = 2;
const STATUS_DRIVER_OK: u8 = 4;
const STATUS_FEATURES_OK: u8 = 8;
#[allow(dead_code)]
const STATUS_FAILED: u8 = 0x80;

// Block request types.
const VIRTIO_BLK_T_IN: u32 = 0;

// Block status codes.
const VIRTIO_BLK_S_OK: u8 = 0;

// Feature bits.
const VIRTIO_F_VERSION_1: u32 = 1; // bit 32 → bit 0 of the upper dword.

const QUEUE_SIZE: u16 = 16;
const SECTOR_SIZE: usize = 512;

#[repr(C)]
struct VirtioBlkReqHeader {
    req_type: u32,
    reserved: u32,
    sector: u64,
}

#[repr(C)]
struct ReadRequest {
    header: VirtioBlkReqHeader,
    data: [u8; SECTOR_SIZE],
    status: u8,
}

unsafe fn cc_read8(common: *mut u8, off: usize) -> u8 {
    // SAFETY: caller's contract — offset within COMMON_CFG region
    // (1 frame = 4 KiB; all fields here are < 0x40).
    unsafe { read_volatile(common.add(off)) }
}
unsafe fn cc_write8(common: *mut u8, off: usize, v: u8) {
    // SAFETY: caller's contract.
    unsafe { write_volatile(common.add(off), v) }
}
unsafe fn cc_read16(common: *mut u8, off: usize) -> u16 {
    // SAFETY: caller's contract; 2-byte aligned offsets here.
    unsafe { read_volatile(common.add(off) as *const u16) }
}
unsafe fn cc_write16(common: *mut u8, off: usize, v: u16) {
    // SAFETY: caller's contract.
    unsafe { write_volatile(common.add(off) as *mut u16, v) }
}
unsafe fn cc_read32(common: *mut u8, off: usize) -> u32 {
    // SAFETY: caller's contract; 4-byte aligned offsets.
    unsafe { read_volatile(common.add(off) as *const u32) }
}
unsafe fn cc_write32(common: *mut u8, off: usize, v: u32) {
    // SAFETY: caller's contract.
    unsafe { write_volatile(common.add(off) as *mut u32, v) }
}
unsafe fn cc_write64(common: *mut u8, off: usize, v: u64) {
    // SAFETY: caller's contract; 8-byte aligned offsets for the
    // queue_desc/driver/device fields.
    unsafe { write_volatile(common.add(off) as *mut u64, v) }
}

/// Run the 3C-3 smoke: locate virtio-blk, init, read sector 0,
/// assert the boot signature, print ARSENAL_BLK_OK. If no
/// virtio-blk device is attached, log and return — the rest of
/// the boot continues so smoke tests other sentinels still
/// complete.
pub fn smoke() {
    let Some(dev) = virtio::find_device(VIRTIO_BLK_DEVICE_ID) else {
        let _ = writeln!(serial::Writer, "blk: no virtio-blk device found");
        return;
    };

    let _ = writeln!(
        serial::Writer,
        "blk: device at {:02x}:{:02x}.{} common={:p} notify={:p} mult={}",
        dev.bus, dev.dev, dev.func, dev.common_cfg, dev.notify_base,
        dev.notify_off_multiplier
    );

    init_device(&dev);

    let mut queue = virtio::Virtqueue::new(QUEUE_SIZE);
    activate_queue(&dev, 0, &queue);

    let queue_notify_off = unsafe { cc_read16(dev.common_cfg, CC_QUEUE_NOTIFY_OFF) };
    let _ = writeln!(
        serial::Writer,
        "blk: queue 0 desc_phys={:#018x} notify_off={}",
        queue.desc_phys, queue_notify_off
    );

    // Set DRIVER_OK — device is now live.
    let final_status = STATUS_ACKNOWLEDGE | STATUS_DRIVER
        | STATUS_FEATURES_OK | STATUS_DRIVER_OK;
    // SAFETY: cc_write8 contract; offset is in COMMON_CFG range.
    unsafe { cc_write8(dev.common_cfg, CC_DEVICE_STATUS, final_status) };

    // Build the read request on the heap; device DMAs by physical
    // address derived from the heap virtual address via HHDM.
    // Bound is immutable from Rust's view because the compiler
    // doesn't model device DMA — we re-read the status / data
    // bytes via read_volatile after pop_used so the compiler can't
    // constant-fold them back to the initialization values.
    let req = Box::new(ReadRequest {
        header: VirtioBlkReqHeader {
            req_type: VIRTIO_BLK_T_IN,
            reserved: 0,
            sector: 0,
        },
        data: [0u8; SECTOR_SIZE],
        status: 0xFF, // sentinel: device overwrites with VIRTIO_BLK_S_*
    });

    let req_virt = &*req as *const _ as u64;
    let hhdm = paging::hhdm_offset();
    let header_phys = req_virt - hhdm;
    let data_phys = (&req.data as *const _ as u64) - hhdm;
    let status_phys = (&req.status as *const _ as u64) - hhdm;

    let head = queue
        .push_chain(&[
            (header_phys, 16, 0),                       // header, device-read
            (data_phys, SECTOR_SIZE as u32, virtio::VIRTQ_DESC_F_WRITE), // data, device-write
            (status_phys, 1, virtio::VIRTQ_DESC_F_WRITE),  // status, device-write
        ])
        .expect("blk: queue full on first request");

    let _ = writeln!(serial::Writer, "blk: submitted request, head desc={head}");

    // Notify the device. Notify address = notify_base +
    // queue_notify_off * notify_off_multiplier. Any 16-bit write
    // to that address triggers; we write the queue index (0).
    // SAFETY: notify_base + computed offset lies within the
    // notify-cap-mapped MMIO region (0x1000 bytes per QEMU).
    unsafe {
        let off = (queue_notify_off as usize) * (dev.notify_off_multiplier as usize);
        let p = dev.notify_base.add(off) as *mut u16;
        write_volatile(p, 0);
    }

    // Poll for completion. yield_now returns control to other
    // tasks (idle, ping/pong) while we wait, but on QEMU TCG the
    // device completes the request before any other task gets a
    // turn — our first pop_used succeeds.
    let mut spins = 0u64;
    let elem = loop {
        if let Some(e) = queue.pop_used() {
            break e;
        }
        sched::yield_now();
        spins += 1;
        if spins > 1_000_000 {
            panic!("blk: device did not complete sector-0 read after {spins} polls");
        }
    };

    let _ = writeln!(
        serial::Writer,
        "blk: completed; used.id={} used.len={} spins={}",
        elem.id, elem.len, spins
    );

    // SAFETY: req is a live Box; we read each byte via volatile so
    // the compiler can't constant-fold to the initialization
    // values. The device finished writing before pop_used returned
    // (used.idx advancing is the device's "I'm done" signal).
    let status = unsafe { read_volatile(&req.status as *const u8) };
    let b510 = unsafe { read_volatile(&req.data[510] as *const u8) };
    let b511 = unsafe { read_volatile(&req.data[511] as *const u8) };

    assert_eq!(
        status, VIRTIO_BLK_S_OK,
        "blk: device returned non-OK status {status:#x}"
    );

    // El Torito / hybrid-ISO MBR boot signature: bytes [510, 511]
    // are 0x55 0xAA respectively, which reads as 0xAA55 LE.
    let sig = u16::from_le_bytes([b510, b511]);
    assert_eq!(
        sig, 0xAA55,
        "blk: expected boot signature 0xAA55 at offset 510, got {sig:#06x}"
    );

    let _ = writeln!(serial::Writer, "ARSENAL_BLK_OK");
}

fn init_device(dev: &virtio::VirtioDevice) {
    // Reset, then drive the v1.2 § 3.1.1 init sequence.
    // SAFETY: dev.common_cfg is a 4-KiB MMIO region returned by
    // virtio::find_device with the COMMON_CFG capability; all
    // CC_* offsets above are < 0x40 (well within).
    unsafe {
        cc_write8(dev.common_cfg, CC_DEVICE_STATUS, 0);
        // The spec recommends waiting until status reads 0 after
        // reset; QEMU acks immediately. Loop at most 16 times.
        for _ in 0..16 {
            if cc_read8(dev.common_cfg, CC_DEVICE_STATUS) == 0 {
                break;
            }
        }
        cc_write8(dev.common_cfg, CC_DEVICE_STATUS, STATUS_ACKNOWLEDGE);
        cc_write8(
            dev.common_cfg,
            CC_DEVICE_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER,
        );

        // Read 64 bits of device features in two halves.
        cc_write32(dev.common_cfg, CC_DEVICE_FEATURE_SELECT, 0);
        let dev_lo = cc_read32(dev.common_cfg, CC_DEVICE_FEATURE);
        cc_write32(dev.common_cfg, CC_DEVICE_FEATURE_SELECT, 1);
        let dev_hi = cc_read32(dev.common_cfg, CC_DEVICE_FEATURE);
        let device_features = ((dev_hi as u64) << 32) | dev_lo as u64;

        // Accept VERSION_1 only. Refuse everything else — the
        // smoke target wants a single read, no fancy features.
        let driver_lo: u32 = 0;
        let driver_hi: u32 = VIRTIO_F_VERSION_1;
        let driver_features = ((driver_hi as u64) << 32) | driver_lo as u64;

        cc_write32(dev.common_cfg, CC_DRIVER_FEATURE_SELECT, 0);
        cc_write32(dev.common_cfg, CC_DRIVER_FEATURE, driver_lo);
        cc_write32(dev.common_cfg, CC_DRIVER_FEATURE_SELECT, 1);
        cc_write32(dev.common_cfg, CC_DRIVER_FEATURE, driver_hi);

        let _ = writeln!(
            serial::Writer,
            "blk: features dev={device_features:#018x} drv={driver_features:#018x}"
        );

        cc_write8(
            dev.common_cfg,
            CC_DEVICE_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK,
        );
        let after = cc_read8(dev.common_cfg, CC_DEVICE_STATUS);
        assert_eq!(
            after & STATUS_FEATURES_OK,
            STATUS_FEATURES_OK,
            "blk: device cleared FEATURES_OK — driver features unacceptable"
        );

        let n_queues = cc_read16(dev.common_cfg, CC_NUM_QUEUES);
        let _ = writeln!(serial::Writer, "blk: num_queues={n_queues}");
    }
}

fn activate_queue(dev: &virtio::VirtioDevice, idx: u16, queue: &virtio::Virtqueue) {
    // SAFETY: same justification as init_device — common_cfg is the
    // mapped 4-KiB MMIO region; the queue_* fields are 64-bit
    // aligned within COMMON_CFG.
    unsafe {
        cc_write16(dev.common_cfg, CC_QUEUE_SELECT, idx);
        let max = cc_read16(dev.common_cfg, CC_QUEUE_SIZE);
        assert!(
            max >= queue.size,
            "blk: queue {idx} max size {max} < requested {}",
            queue.size
        );
        cc_write16(dev.common_cfg, CC_QUEUE_SIZE, queue.size);
        cc_write64(dev.common_cfg, CC_QUEUE_DESC, queue.desc_phys);
        cc_write64(dev.common_cfg, CC_QUEUE_DRIVER, queue.avail_phys);
        cc_write64(dev.common_cfg, CC_QUEUE_DEVICE, queue.used_phys);
        cc_write16(dev.common_cfg, CC_QUEUE_ENABLE, 1);
    }
}
