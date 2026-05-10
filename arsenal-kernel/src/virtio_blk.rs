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
// 3C-4 lifted the COMMON_CFG register layout, cc_* primitives,
// init_transport, and activate_queue out of this file into
// virtio.rs so virtio-net can share them. This file is now the
// blk-specific bits only — request format and response checks.
//
// References:
//   virtio v1.2 § 3.1 (device init)
//   virtio v1.2 § 4.1.4 (modern PCI transport)
//   virtio v1.2 § 5.2   (block device specifics)

use core::fmt::Write;
use core::ptr::read_volatile;

use alloc::boxed::Box;

use crate::paging;
use crate::sched;
use crate::serial;
use crate::virtio;

const VIRTIO_BLK_DEVICE_ID: u16 = 0x1001;

const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_S_OK: u8 = 0;

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

    let driver_features = (virtio::VIRTIO_F_VERSION_1 as u64) << 32;
    let device_features = virtio::init_transport(&dev, driver_features);
    let _ = writeln!(
        serial::Writer,
        "blk: features dev={device_features:#018x} drv={driver_features:#018x}"
    );

    let mut queue = virtio::Virtqueue::new(QUEUE_SIZE);
    let notify_ptr = virtio::activate_queue(&dev, 0, &queue);
    let _ = writeln!(
        serial::Writer,
        "blk: queue 0 desc_phys={:#018x}",
        queue.desc_phys,
    );

    virtio::set_driver_ok(&dev);

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
            (header_phys, 16, 0),                                          // header, device-read
            (data_phys, SECTOR_SIZE as u32, virtio::VIRTQ_DESC_F_WRITE),   // data, device-write
            (status_phys, 1, virtio::VIRTQ_DESC_F_WRITE),                  // status, device-write
        ])
        .expect("blk: queue full on first request");

    let _ = writeln!(serial::Writer, "blk: submitted request, head desc={head}");

    virtio::notify(notify_ptr, 0);

    // Poll for completion. yield_now early-returns because sched
    // isn't init'd yet (runqueue empty); pop_used succeeds quickly
    // under TCG.
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
    // the compiler can't constant-fold to the initialization values.
    let status = unsafe { read_volatile(&req.status as *const u8) };
    let b510 = unsafe { read_volatile(&req.data[510] as *const u8) };
    let b511 = unsafe { read_volatile(&req.data[511] as *const u8) };

    assert_eq!(
        status, VIRTIO_BLK_S_OK,
        "blk: device returned non-OK status {status:#x}"
    );

    let sig = u16::from_le_bytes([b510, b511]);
    assert_eq!(
        sig, 0xAA55,
        "blk: expected boot signature 0xAA55 at offset 510, got {sig:#06x}"
    );

    let _ = writeln!(serial::Writer, "ARSENAL_BLK_OK");
}
