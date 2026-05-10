// SPDX-License-Identifier: BSD-2-Clause
//
// virtio modern PCI transport (virtio v1.2 § 4.1). For each PCI
// device with vendor 0x1AF4, walks the capability list at config
// offset 0x34, picks out VIRTIO_PCI_CAP_* entries (common cfg,
// notify, isr, device cfg, pci-cfg-window), resolves their BAR /
// offset / length to kernel virtual addresses through HHDM, and
// logs each.
//
// Modern only. Legacy (pre-1.0) devices used PIO-mapped registers
// in BAR 0; transitional devices expose both interfaces. Our
// driver-time acceptance of VIRTIO_F_VERSION_1 (3C-3) forces the
// modern path; this transport probe doesn't try to support legacy
// register layouts. Capability layout reference: virtio v1.2
// § 4.1.4.3 (struct virtio_pci_cap, virtio_pci_notify_cap).
//
// 3C-1 prints what it found and exits. 3C-2 builds virtqueue
// infrastructure on top of the resolved pointers; 3C-3 / 3C-4 add
// the actual blk and net drivers and read/write through the
// common-cfg pointer.

use core::fmt::Write;
use core::ptr::NonNull;

use x86_64::structures::paging::{PhysFrame, Size4KiB};

use crate::frames;
use crate::paging;
use crate::pci;
use crate::serial;

const VIRTIO_VENDOR: u16 = 0x1AF4;
const PCI_CAP_ID_VENDOR: u8 = 0x09;

const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;
const VIRTIO_PCI_CAP_ISR_CFG: u8 = 3;
const VIRTIO_PCI_CAP_DEVICE_CFG: u8 = 4;
const VIRTIO_PCI_CAP_PCI_CFG: u8 = 5;

/// Walk every PCI BDF, probe virtio devices, log each resolved
/// capability. Idempotent — safe to call multiple times.
pub fn probe() {
    for bus in 0u16..=255 {
        for dev in 0u8..32 {
            probe_function(bus as u8, dev, 0);
            // Multifunction handling.
            // SAFETY: (bus, dev, 0, 0x0C) is dword-aligned; an
            // absent function-0 returned 0xFFFF earlier and we
            // would have skipped. We don't recheck here because
            // probe_function already filtered to virtio vendor.
            let header_dword =
                unsafe { pci::config_read32(bus as u8, dev, 0, 0x0C) };
            let header_type = ((header_dword >> 16) & 0xFF) as u8;
            if header_type & 0x80 != 0 {
                for func in 1u8..8 {
                    probe_function(bus as u8, dev, func);
                }
            }
        }
    }
}

fn probe_function(bus: u8, dev: u8, func: u8) {
    // SAFETY: bus / dev / func legal for PCI; offset 0 dword-aligned.
    let id = unsafe { pci::config_read32(bus, dev, func, 0x00) };
    let vendor = (id & 0xFFFF) as u16;
    if vendor != VIRTIO_VENDOR {
        return;
    }
    let device_id = ((id >> 16) & 0xFFFF) as u16;

    let _ = writeln!(
        serial::Writer,
        "virtio: probing {bus:02x}:{dev:02x}.{func} device={device_id:#06x}"
    );

    walk_caps(bus, dev, func);
}

fn walk_caps(bus: u8, dev: u8, func: u8) {
    // Status register at config offset 0x06 (high half of dword
    // at 0x04). Bit 4 indicates a capabilities list.
    // SAFETY: standard PCI config read.
    let status_dword = unsafe { pci::config_read32(bus, dev, func, 0x04) };
    let status = ((status_dword >> 16) & 0xFFFF) as u16;
    if status & 0x10 == 0 {
        let _ = writeln!(serial::Writer, "  no capabilities list");
        return;
    }

    // Caps pointer at offset 0x34 (low byte of the dword at 0x34).
    // SAFETY: standard PCI config read.
    let cap_ptr_dword = unsafe { pci::config_read32(bus, dev, func, 0x34) };
    let mut cap_offset = (cap_ptr_dword & 0xFC) as u8;

    while cap_offset != 0 {
        // SAFETY: cap pointers within the spec-mandated 0x40..0xFF
        // window of the config space. Misbehaving devices could
        // return arbitrary nexts; we don't bound-check because a
        // bad next eventually walks into 0xFF / 0x00 terminators.
        let cap_header = unsafe { pci::config_read32(bus, dev, func, cap_offset) };
        let cap_id = (cap_header & 0xFF) as u8;
        let next = ((cap_header >> 8) & 0xFC) as u8;
        let cap_len = ((cap_header >> 16) & 0xFF) as u8;
        let cfg_type = ((cap_header >> 24) & 0xFF) as u8;

        if cap_id == PCI_CAP_ID_VENDOR && cap_len >= 16 {
            print_virtio_cap(bus, dev, func, cap_offset, cfg_type);
        }

        cap_offset = next;
    }
}

fn print_virtio_cap(bus: u8, dev: u8, func: u8, cap_off: u8, cfg_type: u8) {
    // virtio_pci_cap (v1.2 § 4.1.4.3):
    //   off+0  cap_vndr|cap_next|cap_len|cfg_type   (the header dword)
    //   off+4  bar|id|padding[2]
    //   off+8  offset (LE u32, within the BAR)
    //   off+12 length (LE u32)
    // Notify caps (cfg_type=2) extend by:
    //   off+16 notify_off_multiplier (LE u32)
    //
    // SAFETY: all reads at dword-aligned offsets within the cap.
    // cap_len ≥ 16 was verified by walk_caps for the vendor cap;
    // notify reads off+16 only when cfg_type guarantees the field
    // exists.
    let bar_dword = unsafe { pci::config_read32(bus, dev, func, cap_off + 4) };
    let bar = (bar_dword & 0xFF) as u8;
    let off_in_bar = unsafe { pci::config_read32(bus, dev, func, cap_off + 8) };
    let length = unsafe { pci::config_read32(bus, dev, func, cap_off + 12) };

    let cfg_name = match cfg_type {
        VIRTIO_PCI_CAP_COMMON_CFG => "common ",
        VIRTIO_PCI_CAP_NOTIFY_CFG => "notify ",
        VIRTIO_PCI_CAP_ISR_CFG => "isr    ",
        VIRTIO_PCI_CAP_DEVICE_CFG => "device ",
        VIRTIO_PCI_CAP_PCI_CFG => "pci-cfg",
        _ => "unknown",
    };

    // SAFETY: bar must be in 0..6 for PCI; virtio devices in
    // practice use bar 0 or 4 for their MMIO region. A bogus value
    // gets a config read that returns 0 and bar_address yields a
    // zero physical address, which translates to a kernel virtual
    // address that resolves to the start of HHDM — readable but
    // meaningless. We don't dereference here; 3C-3 / 3C-4 do, and
    // they validate the common-cfg signature first.
    let bar_phys = unsafe { bar_address(bus, dev, func, bar) };
    let mmio_phys = bar_phys + off_in_bar as u64;
    let mmio_virt = mmio_phys + paging::hhdm_offset();

    let _ = writeln!(
        serial::Writer,
        "  cap {cfg_name} bar={bar} off={off_in_bar:#x} len={length:#x} -> phys={mmio_phys:#018x} virt={mmio_virt:#018x}"
    );

    if cfg_type == VIRTIO_PCI_CAP_NOTIFY_CFG {
        // SAFETY: notify caps are at least 20 bytes per spec; we
        // got cfg_type=2 from the cap header, so the field exists.
        let mult = unsafe { pci::config_read32(bus, dev, func, cap_off + 16) };
        let _ = writeln!(serial::Writer, "    notify_off_multiplier={mult}");
    }
}

/// Resolve BAR `bar` of (bus, dev, func) to a physical address.
/// Handles both 32-bit and 64-bit memory BARs. Returns 0 for I/O
/// BARs (which virtio modern doesn't use).
///
/// # Safety
/// `bar` should be 0..6; for 64-bit BARs the caller should not pass
/// an index of 5 (the upper-half BAR would read off the end of the
/// BAR window, returning 0xFFFF_FFFF, yielding a nonsense address).
unsafe fn bar_address(bus: u8, dev: u8, func: u8, bar: u8) -> u64 {
    // SAFETY: caller's contract; offset 0x10 + bar*4 is dword-aligned
    // for bar in 0..=5 and lies within the legacy config space.
    let lo = unsafe { pci::config_read32(bus, dev, func, 0x10 + bar * 4) };
    if lo & 0x01 != 0 {
        // I/O BAR — virtio modern doesn't use these. Return 0.
        return 0;
    }
    if (lo & 0x06) == 0x04 {
        // 64-bit memory BAR. Low 32 bits in this BAR (mask off type
        // bits in [3:0]); high 32 bits in the next BAR slot.
        // SAFETY: same constraints, bar+1 in range when bar < 5.
        let hi = unsafe { pci::config_read32(bus, dev, func, 0x10 + (bar + 1) * 4) };
        ((hi as u64) << 32) | ((lo & 0xFFFF_FFF0) as u64)
    } else {
        // 32-bit memory BAR.
        (lo & 0xFFFF_FFF0) as u64
    }
}

// ---------------------------------------------------------------
// 3C-2 — Split virtqueues (virtio v1.2 § 2.6).
// ---------------------------------------------------------------
//
// Three rings per queue, all backed by a single 4-KiB frame:
//
//   offset 0                     — descriptor table (16 * size bytes)
//   offset desc_size             — available ring header + ring
//   offset (rounded up to 4)     — used ring header + ring
//
// Modern alignment is per-ring (16 / 2 / 4 bytes for desc / avail /
// used). The HANDOFF mentioned 64/2/4; the v1.2 spec is 16/2/4 and
// QEMU enforces 16. We pack all three into one frame because a
// 16-descriptor queue is ~424 bytes total and even 64-descriptor
// queues fit; the HANDOFF's "one frame per ring" would waste two
// thirds of the allocated frames per queue. If a future queue
// needs more than ~128 descriptors, the layout grows to multi-
// frame and this code revisits the per-ring split.
//
// Memory ordering: x86 is TSO so plain writes to avail.idx and
// reads from used.idx are correctly ordered against the ring
// stores. SMP / weakly-ordered targets (3F / Apple Silicon)
// will need acquire/release on the index fields.

pub const VIRTQ_DESC_F_NEXT: u16 = 1;
pub const VIRTQ_DESC_F_WRITE: u16 = 2;
#[allow(dead_code)] // indirect descriptors are a 3C-3+ optimization
pub const VIRTQ_DESC_F_INDIRECT: u16 = 4;

#[repr(C)]
#[derive(Copy, Clone, Default, Debug)]
pub struct VirtqDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

#[repr(C)]
#[derive(Copy, Clone, Default, Debug)]
pub struct VirtqUsedElem {
    pub id: u32,
    pub len: u32,
}

#[repr(C)]
struct VirtqAvailHeader {
    flags: u16,
    idx: u16,
    // ring: [u16; size] follows immediately
}

#[repr(C)]
struct VirtqUsedHeader {
    flags: u16,
    idx: u16,
    // ring: [VirtqUsedElem; size] follows immediately
}

pub struct Virtqueue {
    pub size: u16,
    desc: NonNull<VirtqDesc>,
    avail: NonNull<VirtqAvailHeader>,
    used: NonNull<VirtqUsedHeader>,

    pub desc_phys: u64,
    // 3C-3 writes these into the device's COMMON_CFG queue_desc /
    // queue_driver / queue_device registers when activating a queue.
    #[allow(dead_code)]
    pub avail_phys: u64,
    #[allow(dead_code)]
    pub used_phys: u64,

    free_head: u16,
    num_free: u16,
    last_used_idx: u16,

    backing_frame: PhysFrame<Size4KiB>,
}

impl Virtqueue {
    /// Allocate a virtqueue of `size` descriptors. `size` must be a
    /// power of two and small enough that desc + avail + used fit
    /// in a single 4-KiB frame (size ≤ 128 in practice).
    pub fn new(size: u16) -> Self {
        assert!(
            size > 0 && size.is_power_of_two(),
            "virtq: size must be a positive power of two; got {size}"
        );
        let n = size as usize;
        let desc_size = 16 * n;
        let avail_size = 4 + 2 * n;
        let used_offset = (desc_size + avail_size + 3) & !3;
        let used_size = 4 + 8 * n;
        let total = used_offset + used_size;
        assert!(
            total <= 4096,
            "virtq: layout {total} bytes exceeds single 4-KiB frame for size {size}"
        );

        let frame = frames::FRAMES.alloc_frame().expect("virtq: OOM");
        let phys = frame.start_address().as_u64();
        let virt = phys + paging::hhdm_offset();

        // SAFETY: virt is HHDM-mapped to a frame we just allocated
        // and exclusively own. Zeroing initializes all three rings'
        // header fields (flags=0, idx=0) and clears the descriptor
        // table to a known state.
        unsafe { core::ptr::write_bytes(virt as *mut u8, 0, 4096) };

        let desc_ptr = virt as *mut VirtqDesc;
        let avail_ptr = (virt + desc_size as u64) as *mut VirtqAvailHeader;
        let used_ptr = (virt + used_offset as u64) as *mut VirtqUsedHeader;

        // Build the free-descriptor chain. Each desc.next points to
        // the following slot; the last desc terminates with next=0,
        // which is also the head — the chain is never circular and
        // num_free guards exhaustion.
        // SAFETY: desc_ptr points to n freshly-zeroed descriptors
        // we exclusively own; n descriptors fit within desc_size.
        unsafe {
            for i in 0..n {
                let next = if i == n - 1 { 0 } else { (i + 1) as u16 };
                (*desc_ptr.add(i)).next = next;
            }
        }

        Self {
            size,
            // SAFETY: pointers derived from a 4-KiB frame just
            // allocated and zeroed; non-null by construction.
            desc: unsafe { NonNull::new_unchecked(desc_ptr) },
            avail: unsafe { NonNull::new_unchecked(avail_ptr) },
            used: unsafe { NonNull::new_unchecked(used_ptr) },
            desc_phys: phys,
            avail_phys: phys + desc_size as u64,
            used_phys: phys + used_offset as u64,
            free_head: 0,
            num_free: size,
            last_used_idx: 0,
            backing_frame: frame,
        }
    }

    pub fn num_free(&self) -> u16 {
        self.num_free
    }

    /// Push a single-descriptor request onto the available ring.
    /// Returns the descriptor index, or None if the queue is full.
    /// Caller is responsible for notifying the device afterward
    /// (3C-3 wires the notify register write).
    pub fn push_descriptor(&mut self, addr: u64, len: u32, flags: u16) -> Option<u16> {
        if self.num_free == 0 {
            return None;
        }
        let idx = self.free_head;
        // SAFETY: idx < size (free_head is always a valid descriptor
        // index in the chain we set up at new()); desc covers size
        // entries.
        unsafe {
            let d = self.desc.as_ptr().add(idx as usize);
            self.free_head = (*d).next;
            (*d).addr = addr;
            (*d).len = len;
            (*d).flags = flags & !VIRTQ_DESC_F_NEXT;
            (*d).next = 0;
        }
        self.num_free -= 1;

        // SAFETY: avail header + ring fit in the allocated frame.
        // Ring slot index is masked into [0, size) by % size.
        unsafe {
            let avail_idx = (*self.avail.as_ptr()).idx;
            let ring_slot = (avail_idx % self.size) as usize;
            let ring_ptr = (self.avail.as_ptr() as *mut u8).add(4) as *mut u16;
            *ring_ptr.add(ring_slot) = idx;
            (*self.avail.as_ptr()).idx = avail_idx.wrapping_add(1);
        }

        Some(idx)
    }

    /// Pop the next completed descriptor from the used ring,
    /// returning its id + length and freeing the descriptor back
    /// to the pool. Returns None if no new completions since the
    /// last call.
    pub fn pop_used(&mut self) -> Option<VirtqUsedElem> {
        // SAFETY: used header is a 4-byte struct in our allocated
        // frame; reading idx is a simple aligned u16 load.
        let used_idx = unsafe { (*self.used.as_ptr()).idx };
        if used_idx == self.last_used_idx {
            return None;
        }
        let ring_slot = (self.last_used_idx % self.size) as usize;
        // SAFETY: used header + ring entries fit in the frame; each
        // slot is an 8-byte VirtqUsedElem.
        let elem = unsafe {
            let ring_ptr =
                (self.used.as_ptr() as *mut u8).add(4) as *mut VirtqUsedElem;
            *ring_ptr.add(ring_slot)
        };
        self.last_used_idx = self.last_used_idx.wrapping_add(1);

        // Return the descriptor to the free chain. A misbehaving
        // device could write any id; we don't validate id < size
        // because the ensuing add() would be out-of-bounds and
        // panic, which is the right failure mode for a corrupted
        // ring.
        // SAFETY: elem.id < size if device is well-behaved.
        unsafe {
            let d = self.desc.as_ptr().add(elem.id as usize);
            (*d).next = self.free_head;
        }
        self.free_head = elem.id as u16;
        self.num_free += 1;

        Some(elem)
    }
}

impl Drop for Virtqueue {
    fn drop(&mut self) {
        // Return the backing frame to the global pool. PhysFrame is
        // Copy; the value is copied here, so the field is unchanged.
        frames::FRAMES.free_frame(self.backing_frame);
    }
}
