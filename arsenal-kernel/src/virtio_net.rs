// SPDX-License-Identifier: BSD-2-Clause
//
// virtio-net driver, single-shot probe-TX smoke (3C-4).
//
// Initializes the first virtio-net device, accepts only
// VIRTIO_F_VERSION_1 (declines mergeable RX bufs, TSO, csum offload,
// MAC, link status — every optional feature), activates the receive
// queue (idx 0) and transmit queue (idx 1), pre-populates the
// receive queue with a small pool of empty buffers, and submits one
// 64-byte all-zero Ethernet frame on the TX queue. The smoke target
// is "TX descriptor returns used" — proves notify-doorbell works
// and the device processed our frame. Whether the frame actually
// went anywhere on QEMU's slirp net is 3D's concern (smoltcp
// drives RX correctness then).
//
// Header format (virtio v1.2 § 5.1.6 with VIRTIO_F_VERSION_1):
// 12-byte virtio_net_hdr precedes every TX and RX frame, even
// without VIRTIO_NET_F_MRG_RXBUF. We zero every header field on
// transmit (no GSO, no checksum offload, num_buffers=0).
//
// References:
//   virtio v1.2 § 3.1   (device init)
//   virtio v1.2 § 5.1   (network device specifics)

use core::fmt::Write;

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::paging;
use crate::sched;
use crate::serial;
use crate::virtio;

const VIRTIO_NET_DEVICE_ID: u16 = 0x1000;

const RX_QUEUE_IDX: u16 = 0;
const TX_QUEUE_IDX: u16 = 1;
const QUEUE_SIZE: u16 = 16;
const RX_BUFFER_COUNT: usize = 8;
const TX_PAYLOAD_LEN: usize = 64;

const NET_HDR_LEN: u32 = 12;

#[repr(C)]
#[derive(Default)]
struct VirtioNetHdr {
    flags: u8,
    gso_type: u8,
    hdr_len: u16,
    gso_size: u16,
    csum_start: u16,
    csum_offset: u16,
    num_buffers: u16,
}

#[repr(C)]
struct TxBuffer {
    hdr: VirtioNetHdr,
    payload: [u8; TX_PAYLOAD_LEN],
}

/// Receive buffer big enough for a max-MTU Ethernet frame plus the
/// 12-byte virtio_net_hdr. 1514 bytes payload + 12-byte header = 1526
/// bytes; round up to multiples of 4 bytes for cleanliness.
const RX_BUFFER_LEN: usize = 1528;

#[repr(C)]
struct RxBuffer {
    bytes: [u8; RX_BUFFER_LEN],
}

/// Run the 3C-4 smoke. If no virtio-net device is attached, log and
/// return — boot continues so other sentinels still complete.
pub fn smoke() {
    let Some(dev) = virtio::find_device(VIRTIO_NET_DEVICE_ID) else {
        let _ = writeln!(serial::Writer, "net: no virtio-net device found");
        return;
    };

    let _ = writeln!(
        serial::Writer,
        "net: device at {:02x}:{:02x}.{} common={:p}",
        dev.bus, dev.dev, dev.func, dev.common_cfg
    );

    let driver_features = (virtio::VIRTIO_F_VERSION_1 as u64) << 32;
    let device_features = virtio::init_transport(&dev, driver_features);
    let _ = writeln!(
        serial::Writer,
        "net: features dev={device_features:#018x} drv={driver_features:#018x}"
    );

    let mut rx_queue = virtio::Virtqueue::new(QUEUE_SIZE);
    let mut tx_queue = virtio::Virtqueue::new(QUEUE_SIZE);
    let rx_notify = virtio::activate_queue(&dev, RX_QUEUE_IDX, &rx_queue);
    let tx_notify = virtio::activate_queue(&dev, TX_QUEUE_IDX, &tx_queue);
    let _ = writeln!(
        serial::Writer,
        "net: rx_q desc_phys={:#018x} tx_q desc_phys={:#018x}",
        rx_queue.desc_phys, tx_queue.desc_phys
    );

    // Pre-populate the receive queue. Each buffer is one descriptor
    // marked F_WRITE; the device will fill (header + frame) when a
    // packet arrives. We hold the Boxes alive in a Vec for the
    // duration of the smoke; dropping the Vec would free the heap
    // memory while the device still references it.
    let hhdm = paging::hhdm_offset();
    let mut rx_bufs: Vec<Box<RxBuffer>> = Vec::with_capacity(RX_BUFFER_COUNT);
    for _ in 0..RX_BUFFER_COUNT {
        let buf: Box<RxBuffer> = Box::new(RxBuffer {
            bytes: [0u8; RX_BUFFER_LEN],
        });
        let buf_virt = &*buf as *const _ as u64;
        let buf_phys = buf_virt - hhdm;
        rx_queue
            .push_descriptor(buf_phys, RX_BUFFER_LEN as u32, virtio::VIRTQ_DESC_F_WRITE)
            .expect("net: RX queue full pre-populating");
        rx_bufs.push(buf);
    }
    virtio::notify(rx_notify, RX_QUEUE_IDX);

    virtio::set_driver_ok(&dev);

    // Build a single transmit. Two-descriptor chain: header
    // (device-read) + payload (device-read). All zeros — the smoke
    // target is "device acknowledged we transmitted", not "QEMU
    // forwarded the frame anywhere".
    let tx = Box::new(TxBuffer {
        hdr: VirtioNetHdr::default(),
        payload: [0u8; TX_PAYLOAD_LEN],
    });
    let tx_virt = &*tx as *const _ as u64;
    let hdr_phys = tx_virt - hhdm;
    let payload_phys = (&tx.payload as *const _ as u64) - hhdm;

    tx_queue
        .push_chain(&[
            (hdr_phys, NET_HDR_LEN, 0),
            (payload_phys, TX_PAYLOAD_LEN as u32, 0),
        ])
        .expect("net: TX queue full on first frame");

    virtio::notify(tx_notify, TX_QUEUE_IDX);

    // Poll for TX completion. The device marks the descriptor used
    // when it has finished reading our frame and handed it to the
    // network backend — for QEMU's user-mode slirp, that's nearly
    // immediate.
    let mut spins = 0u64;
    let elem = loop {
        if let Some(e) = tx_queue.pop_used() {
            break e;
        }
        sched::yield_now();
        spins += 1;
        if spins > 1_000_000 {
            panic!("net: TX did not complete after {spins} polls");
        }
    };

    let _ = writeln!(
        serial::Writer,
        "net: TX completed; used.id={} used.len={} spins={}",
        elem.id, elem.len, spins
    );

    // The device may have written into RX buffers (e.g., DHCP
    // discovery from QEMU's slirp). We don't check RX correctness
    // — that's 3D's concern. Just log how many RX descriptors came
    // back used so future-me has a hint.
    let mut rx_completions = 0u32;
    while let Some(_e) = rx_queue.pop_used() {
        rx_completions += 1;
    }
    if rx_completions > 0 {
        let _ = writeln!(
            serial::Writer,
            "net: incidental RX completions: {rx_completions}"
        );
    }

    // Keep rx_bufs and tx alive past this point — the device may
    // still be writing into RX buffers we provided. Dropping them
    // here is fine because we never call DRIVER_OK -> reset; the
    // smoke just exits. Real drivers would unbind in reverse.
    drop(rx_bufs);
    drop(tx);

    let _ = writeln!(serial::Writer, "ARSENAL_NET_OK");
}
