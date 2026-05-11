// SPDX-License-Identifier: BSD-2-Clause
//
// virtio-net driver. Provides a reusable VirtioNet struct that
// owns the RX and TX queues and their buffer pools, plus an impl
// of smoltcp's phy::Device trait that 3D-2's Interface drives.
//
// Header format (virtio v1.2 § 5.1.6 with VIRTIO_F_VERSION_1):
// 12-byte virtio_net_hdr precedes every TX and RX frame, even
// without VIRTIO_NET_F_MRG_RXBUF. We zero every header field on
// transmit (no GSO, no checksum offload, num_buffers=0).
//
// Buffer ownership:
//
//  - RX is a pre-allocated, fixed-size pool indexed by buffer
//    index. Each Box<RxBuffer> stays alive in the pool for the
//    driver's lifetime; when the device fills one and we hand it
//    to a smoltcp RxToken, the slice borrowed for `consume()` is
//    a view into the still-owned Box. After consume() returns we
//    re-post the same Box to the device — no allocation churn.
//    The `desc_to_buf` table maps descriptor id → buffer index;
//    push_descriptor returns the desc id, which we record on each
//    re-post.
//
//  - TX allocates per-call. transmit() returns a TxToken whose
//    consume() Box::news a TxBuffer, fills it, push_descriptor's
//    it, leaves it in `tx_in_flight` indexed by desc id. tx_reap
//    runs at the start of every receive()/transmit() and frees
//    completed TX buffers (Box drop). No long-lived TX pool keeps
//    the structure simple; the cost is one heap alloc + free per
//    transmitted frame, which is acceptable for handshake-class
//    workloads (3D-3/4) and revisitable when throughput-class
//    work arrives.
//
// References:
//   virtio v1.2 § 3.1   (device init)
//   virtio v1.2 § 5.1   (network device specifics)

use core::fmt::Write;

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use smoltcp::phy::{self, Device, DeviceCapabilities, Medium};
use smoltcp::time::Instant;

use crate::paging;
use crate::sched;
use crate::serial;
use crate::virtio;

const VIRTIO_NET_DEVICE_ID: u16 = 0x1000;

const RX_QUEUE_IDX: u16 = 0;
const TX_QUEUE_IDX: u16 = 1;
const QUEUE_SIZE: u16 = 16;
const QUEUE_SIZE_USIZE: usize = QUEUE_SIZE as usize;

const NET_HDR_LEN: usize = 12;
const MTU: usize = 1514;
const RX_BUFFER_LEN: usize = NET_HDR_LEN + MTU;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct VirtioNetHdr {
    flags: u8,
    gso_type: u8,
    hdr_len: u16,
    gso_size: u16,
    csum_start: u16,
    csum_offset: u16,
    num_buffers: u16,
}

#[repr(C, align(8))]
struct RxBuffer {
    bytes: [u8; RX_BUFFER_LEN],
}

#[repr(C, align(8))]
struct TxBuffer {
    hdr: VirtioNetHdr,
    payload: [u8; MTU],
}

struct RxState {
    queue: virtio::Virtqueue,
    notify: *mut u16,
    /// Pool of receive buffers, indexed by buffer index. Vec<Box<_>>
    /// rather than Vec<_> because DMA needs stable addresses: each
    /// Box's heap pointer is fixed, while a Vec<RxBuffer> would move
    /// its elements on resize, invalidating any device-held physical
    /// pointer. clippy's vec_box lint doesn't account for this.
    #[allow(clippy::vec_box)]
    bufs: Vec<Box<RxBuffer>>,
    /// Map from descriptor id → buffer index. push_descriptor returns
    /// a desc id chosen by the virtqueue free chain; we record which
    /// buffer is at that desc so pop_used can find it back.
    desc_to_buf: [u8; QUEUE_SIZE_USIZE],
}

struct TxState {
    queue: virtio::Virtqueue,
    notify: *mut u16,
    /// In-flight TX buffers, indexed by descriptor id. Some until the
    /// device completes the descriptor (drop frees the heap memory).
    in_flight: Vec<Option<Box<TxBuffer>>>,
}

pub struct VirtioNet {
    #[allow(dead_code)] // dev kept for future re-init / IRQ wiring (3F)
    dev: virtio::VirtioDevice,
    rx: RxState,
    tx: TxState,
}

// SAFETY: VirtioNet contains raw pointers (notify regs, virtqueue
// NonNulls, VirtioDevice MMIO bases) that are !Send by default. The
// pointee MMIO regions and queue frames are pinned by paging::map_mmio
// and the frame allocator respectively, so the addresses are stable.
// The single-CPU cooperative scheduler plus 3D-2's enclosing Mutex
// (net::NET) guarantees no concurrent access from another CPU thread
// today. 3F's preemptive + SMP migration revisits this when more than
// one CPU can attempt a poll.
unsafe impl Send for VirtioNet {}

impl VirtioNet {
    /// Initialize a virtio-net device for driving by smoltcp.
    /// Performs the v1.2 § 3.1.1 init dance, allocates and activates
    /// RX (queue 0) + TX (queue 1), pre-populates the RX queue with
    /// QUEUE_SIZE buffers, and writes DRIVER_OK so the device is
    /// live before this returns.
    pub fn new(dev: virtio::VirtioDevice) -> Self {
        let driver_features = (virtio::VIRTIO_F_VERSION_1 as u64) << 32;
        let device_features = virtio::init_transport(&dev, driver_features);
        let _ = writeln!(
            serial::Writer,
            "net: features dev={device_features:#018x} drv={driver_features:#018x}"
        );

        let rx_queue = virtio::Virtqueue::new(QUEUE_SIZE);
        let tx_queue = virtio::Virtqueue::new(QUEUE_SIZE);
        let rx_notify = virtio::activate_queue(&dev, RX_QUEUE_IDX, &rx_queue);
        let tx_notify = virtio::activate_queue(&dev, TX_QUEUE_IDX, &tx_queue);
        let _ = writeln!(
            serial::Writer,
            "net: rx_q desc_phys={:#018x} tx_q desc_phys={:#018x}",
            rx_queue.desc_phys, tx_queue.desc_phys
        );

        let mut bufs: Vec<Box<RxBuffer>> = Vec::with_capacity(QUEUE_SIZE_USIZE);
        for _ in 0..QUEUE_SIZE_USIZE {
            bufs.push(Box::new(RxBuffer {
                bytes: [0u8; RX_BUFFER_LEN],
            }));
        }
        let mut rx = RxState {
            queue: rx_queue,
            notify: rx_notify,
            bufs,
            desc_to_buf: [0u8; QUEUE_SIZE_USIZE],
        };
        // Post every buffer up front. Each push_descriptor returns
        // the desc id chosen from the queue's free chain (0..size on
        // a fresh queue, in order); record it so pop_used can map
        // back to the buffer index.
        let hhdm = paging::hhdm_offset();
        for bufidx in 0..QUEUE_SIZE_USIZE {
            let phys = (&*rx.bufs[bufidx] as *const _ as u64) - hhdm;
            let desc_id = rx
                .queue
                .push_descriptor(phys, RX_BUFFER_LEN as u32, virtio::VIRTQ_DESC_F_WRITE)
                .expect("net: RX queue full pre-populating");
            rx.desc_to_buf[desc_id as usize] = bufidx as u8;
        }
        virtio::notify(rx.notify, RX_QUEUE_IDX);

        let in_flight: Vec<Option<Box<TxBuffer>>> = (0..QUEUE_SIZE).map(|_| None).collect();
        let tx = TxState {
            queue: tx_queue,
            notify: tx_notify,
            in_flight,
        };

        virtio::set_driver_ok(&dev);

        Self { dev, rx, tx }
    }

    /// Submit a payload to the TX queue. Returns Err if the queue is
    /// full or the payload exceeds MTU. Notifies the device.
    pub fn tx_submit(&mut self, payload: &[u8]) -> Result<(), TxError> {
        if payload.len() > MTU {
            return Err(TxError::TooLarge);
        }
        let mut buf = Box::new(TxBuffer {
            hdr: VirtioNetHdr::default(),
            payload: [0u8; MTU],
        });
        buf.payload[..payload.len()].copy_from_slice(payload);

        let phys = (&*buf as *const _ as u64) - paging::hhdm_offset();
        let total = (NET_HDR_LEN + payload.len()) as u32;
        let desc_id = self
            .tx
            .queue
            .push_descriptor(phys, total, 0)
            .ok_or(TxError::QueueFull)?;
        self.tx.in_flight[desc_id as usize] = Some(buf);
        virtio::notify(self.tx.notify, TX_QUEUE_IDX);
        Ok(())
    }

    /// Reap completed TX buffers, freeing them back to the heap.
    /// Idempotent — safe to call any time before submitting.
    pub fn tx_reap(&mut self) {
        while let Some(elem) = self.tx.queue.pop_used() {
            self.tx.in_flight[elem.id as usize] = None;
        }
    }

    /// Number of TX buffers currently in flight (submitted but not
    /// yet completed by the device). Useful for spin-wait loops in
    /// tests; production code drives via the smoltcp Interface.
    pub fn tx_inflight(&self) -> usize {
        self.tx.in_flight.iter().filter(|s| s.is_some()).count()
    }
}

#[derive(Debug)]
pub enum TxError {
    TooLarge,
    QueueFull,
}

// ---- smoltcp phy::Device adapter ------------------------------

pub struct VirtioNetRxToken<'a> {
    rx: &'a mut RxState,
    bufidx: usize,
    len: usize,
}

impl<'a> phy::RxToken for VirtioNetRxToken<'a> {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        // The device wrote (virtio_net_hdr || frame); smoltcp wants
        // just the frame. Slice past the 12-byte header.
        let buf = &self.rx.bufs[self.bufidx].bytes[NET_HDR_LEN..NET_HDR_LEN + self.len];
        let result = f(buf);
        // Re-post the buffer to the device for another receive.
        let phys = (&*self.rx.bufs[self.bufidx] as *const _ as u64)
            - paging::hhdm_offset();
        let desc_id = self
            .rx
            .queue
            .push_descriptor(phys, RX_BUFFER_LEN as u32, virtio::VIRTQ_DESC_F_WRITE)
            .expect("net: RX queue full repushing");
        self.rx.desc_to_buf[desc_id as usize] = self.bufidx as u8;
        virtio::notify(self.rx.notify, RX_QUEUE_IDX);
        result
    }
}

pub struct VirtioNetTxToken<'a> {
    tx: &'a mut TxState,
}

impl<'a> phy::TxToken for VirtioNetTxToken<'a> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        // Per-call heap allocation for the TX frame. smoltcp's
        // Interface fills the bytes inside the closure; we then
        // submit the boxed buffer to the device.
        let mut payload = vec![0u8; len];
        let result = f(&mut payload);

        // Push directly without going through tx_submit's bounds
        // check (we already know len ≤ MTU because smoltcp's
        // capabilities() advertised MTU).
        let mut buf = Box::new(TxBuffer {
            hdr: VirtioNetHdr::default(),
            payload: [0u8; MTU],
        });
        buf.payload[..len].copy_from_slice(&payload);

        let phys = (&*buf as *const _ as u64) - paging::hhdm_offset();
        let total = (NET_HDR_LEN + len) as u32;
        if let Some(desc_id) = self.tx.queue.push_descriptor(phys, total, 0) {
            self.tx.in_flight[desc_id as usize] = Some(buf);
            virtio::notify(self.tx.notify, TX_QUEUE_IDX);
        }
        // If push_descriptor returns None the queue is full; we
        // silently drop the frame. smoltcp will retry on its next
        // poll. Buf goes out of scope and the heap memory is freed.
        result
    }
}

impl Device for VirtioNet {
    type RxToken<'a> = VirtioNetRxToken<'a>;
    type TxToken<'a> = VirtioNetTxToken<'a>;

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ethernet;
        caps.max_transmission_unit = MTU;
        caps
    }

    fn receive(&mut self, _ts: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        // Reap finished TX so transmits backed up behind a slow
        // device get unblocked.
        self.tx_reap();
        let elem = self.rx.queue.pop_used()?;
        let bufidx = self.rx.desc_to_buf[elem.id as usize] as usize;
        // virtio reports total bytes the device wrote (hdr + frame).
        // Strip the 12-byte hdr to get the frame length smoltcp wants.
        let frame_len = (elem.len as usize).saturating_sub(NET_HDR_LEN);
        Some((
            VirtioNetRxToken {
                rx: &mut self.rx,
                bufidx,
                len: frame_len,
            },
            VirtioNetTxToken { tx: &mut self.tx },
        ))
    }

    fn transmit(&mut self, _ts: Instant) -> Option<Self::TxToken<'_>> {
        self.tx_reap();
        // We never actually run out of TX descriptors in 3D's
        // workloads, but be honest about it.
        if self.tx.queue.num_free() == 0 {
            return None;
        }
        Some(VirtioNetTxToken { tx: &mut self.tx })
    }
}

// ---- 3C-4 smoke (kept; rewired to use VirtioNet driver) -------

/// 3C-4 smoke target: locate the device, init, send one zero-filled
/// 64-byte frame, observe TX completion, drain incidental RX, print
/// ARSENAL_NET_OK. Returns the live VirtioNet for 3D-2's smoltcp
/// Interface to take ownership of. Panics if no virtio-net is found
/// — the smoke gate requires it.
pub fn smoke() -> VirtioNet {
    let dev = virtio::find_device(VIRTIO_NET_DEVICE_ID)
        .expect("net: no virtio-net device found");

    let _ = writeln!(
        serial::Writer,
        "net: device at {:02x}:{:02x}.{} common={:p}",
        dev.bus, dev.dev, dev.func, dev.common_cfg
    );

    let mut net = VirtioNet::new(dev);

    net.tx_submit(&[0u8; 64])
        .expect("net: TX queue full on first frame");

    let mut spins = 0u64;
    loop {
        net.tx_reap();
        if net.tx_inflight() == 0 {
            break;
        }
        sched::yield_now();
        spins += 1;
        if spins > 1_000_000 {
            panic!("net: TX did not complete after {spins} polls");
        }
    }
    let _ = writeln!(serial::Writer, "net: TX completed; spins={spins}");

    // Drain any incidental RX queued before smoltcp takes over the
    // Device. slirp doesn't push unsolicited frames here, but a stale
    // completion from the probe TX (BROADCAST echo on some setups)
    // could be sitting in the used ring.
    let mut rx_completions = 0u32;
    while let Some((rx, _tx)) = net.receive(Instant::ZERO) {
        use phy::RxToken;
        rx.consume(|_buf| ());
        rx_completions += 1;
    }
    if rx_completions > 0 {
        let _ = writeln!(
            serial::Writer,
            "net: incidental RX completions: {rx_completions}"
        );
    }

    let _ = writeln!(serial::Writer, "ARSENAL_NET_OK");
    net
}
