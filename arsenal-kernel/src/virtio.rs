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

/// Resolved virtio-modern transport pointers for one device.
/// Returned by `find_device`; consumed by drivers (3C-3 / 3C-4)
/// when initializing.
pub struct VirtioDevice {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    // 3C-4's virtio-net will read device_id (to distinguish 0x1000
    // legacy from 0x1041 modern at runtime) and device_cfg (for the
    // MAC address). 3F wires isr for IRQ acknowledgement.
    #[allow(dead_code)]
    pub device_id: u16,
    pub common_cfg: *mut u8,
    pub notify_base: *mut u8,
    pub notify_off_multiplier: u32,
    #[allow(dead_code)]
    pub isr: *mut u8,
    #[allow(dead_code)]
    pub device_cfg: *mut u8,
}

/// Find the first virtio device matching `device_id` and resolve
/// its transport pointers. Returns None if no matching device is
/// present. Returns the resolved common / notify / isr / device
/// MMIO pointers translated through HHDM. The caller treats the
/// returned struct as the handle for all subsequent register
/// accesses on that device.
pub fn find_device(device_id: u16) -> Option<VirtioDevice> {
    for bus in 0u16..=255 {
        for dev in 0u8..32 {
            if let Some(d) = try_resolve(bus as u8, dev, 0, device_id) {
                return Some(d);
            }
            // SAFETY: standard PCI dword read.
            let header_dword =
                unsafe { pci::config_read32(bus as u8, dev, 0, 0x0C) };
            if (header_dword >> 16) & 0x80 != 0 {
                for func in 1u8..8 {
                    if let Some(d) = try_resolve(bus as u8, dev, func, device_id) {
                        return Some(d);
                    }
                }
            }
        }
    }
    None
}

fn try_resolve(bus: u8, dev: u8, func: u8, want: u16) -> Option<VirtioDevice> {
    // SAFETY: standard PCI dword reads at dword-aligned offsets.
    let id = unsafe { pci::config_read32(bus, dev, func, 0x00) };
    if (id & 0xFFFF) as u16 != VIRTIO_VENDOR {
        return None;
    }
    if ((id >> 16) & 0xFFFF) as u16 != want {
        return None;
    }

    let mut common_cfg: *mut u8 = core::ptr::null_mut();
    let mut notify_base: *mut u8 = core::ptr::null_mut();
    let mut notify_off_multiplier: u32 = 0;
    let mut isr: *mut u8 = core::ptr::null_mut();
    let mut device_cfg: *mut u8 = core::ptr::null_mut();

    // Walk caps the same way print_virtio_cap does, but instead of
    // logging, store the resolved virtual addresses by cfg_type.
    // SAFETY: standard PCI dword reads.
    let status_dword = unsafe { pci::config_read32(bus, dev, func, 0x04) };
    if ((status_dword >> 16) & 0x10) == 0 {
        return None;
    }
    let cap_ptr_dword = unsafe { pci::config_read32(bus, dev, func, 0x34) };
    let mut cap_offset = (cap_ptr_dword & 0xFC) as u8;
    while cap_offset != 0 {
        let cap_header = unsafe { pci::config_read32(bus, dev, func, cap_offset) };
        let cap_id = (cap_header & 0xFF) as u8;
        let next = ((cap_header >> 8) & 0xFC) as u8;
        let cap_len = ((cap_header >> 16) & 0xFF) as u8;
        let cfg_type = ((cap_header >> 24) & 0xFF) as u8;

        if cap_id == PCI_CAP_ID_VENDOR && cap_len >= 16 {
            let bar_dword =
                unsafe { pci::config_read32(bus, dev, func, cap_offset + 4) };
            let bar = (bar_dword & 0xFF) as u8;
            let off_in_bar =
                unsafe { pci::config_read32(bus, dev, func, cap_offset + 8) };
            let length =
                unsafe { pci::config_read32(bus, dev, func, cap_offset + 12) };
            let bar_phys = unsafe { bar_address(bus, dev, func, bar) };
            let cap_phys = bar_phys + off_in_bar as u64;
            // Limine's HHDM doesn't cover device MMIO; map each
            // virtio cap's region into our page tables before any
            // driver dereferences a returned pointer.
            if length > 0 {
                paging::map_mmio(cap_phys, length as u64);
            }
            let virt = (cap_phys + paging::hhdm_offset()) as *mut u8;
            match cfg_type {
                VIRTIO_PCI_CAP_COMMON_CFG => common_cfg = virt,
                VIRTIO_PCI_CAP_NOTIFY_CFG => {
                    notify_base = virt;
                    notify_off_multiplier = unsafe {
                        pci::config_read32(bus, dev, func, cap_offset + 16)
                    };
                }
                VIRTIO_PCI_CAP_ISR_CFG => isr = virt,
                VIRTIO_PCI_CAP_DEVICE_CFG => device_cfg = virt,
                _ => {}
            }
        }
        cap_offset = next;
    }

    if common_cfg.is_null() || notify_base.is_null() || device_cfg.is_null() {
        return None;
    }

    Some(VirtioDevice {
        bus,
        dev,
        func,
        device_id: ((id >> 16) & 0xFFFF) as u16,
        common_cfg,
        notify_base,
        notify_off_multiplier,
        isr,
        device_cfg,
    })
}

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
// Transport-level helpers used by every virtio driver.
// ---------------------------------------------------------------
//
// COMMON_CFG register offsets (virtio v1.2 § 4.1.4.3) and the cc_*
// volatile read/write primitives that drivers use to read and write
// them. Lifted out of virtio_blk.rs in 3C-4 because virtio-net wants
// the same primitives — drivers should not be redefining the
// register map, and feature negotiation is identical across them.

pub(crate) const CC_DEVICE_FEATURE_SELECT: usize = 0x00;
pub(crate) const CC_DEVICE_FEATURE: usize = 0x04;
pub(crate) const CC_DRIVER_FEATURE_SELECT: usize = 0x08;
pub(crate) const CC_DRIVER_FEATURE: usize = 0x0C;
#[allow(dead_code)] // num_queues is informational; drivers read directly when interested
pub(crate) const CC_NUM_QUEUES: usize = 0x12;
pub(crate) const CC_DEVICE_STATUS: usize = 0x14;
pub(crate) const CC_QUEUE_SELECT: usize = 0x16;
pub(crate) const CC_QUEUE_SIZE: usize = 0x18;
pub(crate) const CC_QUEUE_ENABLE: usize = 0x1C;
pub(crate) const CC_QUEUE_NOTIFY_OFF: usize = 0x1E;
pub(crate) const CC_QUEUE_DESC: usize = 0x20;
pub(crate) const CC_QUEUE_DRIVER: usize = 0x28;
pub(crate) const CC_QUEUE_DEVICE: usize = 0x30;

pub(crate) const STATUS_ACKNOWLEDGE: u8 = 1;
pub(crate) const STATUS_DRIVER: u8 = 2;
pub(crate) const STATUS_DRIVER_OK: u8 = 4;
pub(crate) const STATUS_FEATURES_OK: u8 = 8;
#[allow(dead_code)]
pub(crate) const STATUS_FAILED: u8 = 0x80;

pub(crate) const VIRTIO_F_VERSION_1: u32 = 1; // bit 32 → bit 0 of upper dword

/// SAFETY for all cc_*: caller's contract — `common` is a valid
/// COMMON_CFG MMIO base (mapped via paging::map_mmio at find_device
/// time) and `off` is a dword/word/byte-aligned offset within the
/// 4 KiB region.
pub(crate) unsafe fn cc_read8(common: *mut u8, off: usize) -> u8 {
    unsafe { core::ptr::read_volatile(common.add(off)) }
}
pub(crate) unsafe fn cc_write8(common: *mut u8, off: usize, v: u8) {
    unsafe { core::ptr::write_volatile(common.add(off), v) }
}
pub(crate) unsafe fn cc_read16(common: *mut u8, off: usize) -> u16 {
    unsafe { core::ptr::read_volatile(common.add(off) as *const u16) }
}
pub(crate) unsafe fn cc_write16(common: *mut u8, off: usize, v: u16) {
    unsafe { core::ptr::write_volatile(common.add(off) as *mut u16, v) }
}
pub(crate) unsafe fn cc_read32(common: *mut u8, off: usize) -> u32 {
    unsafe { core::ptr::read_volatile(common.add(off) as *const u32) }
}
pub(crate) unsafe fn cc_write32(common: *mut u8, off: usize, v: u32) {
    unsafe { core::ptr::write_volatile(common.add(off) as *mut u32, v) }
}
pub(crate) unsafe fn cc_write64(common: *mut u8, off: usize, v: u64) {
    unsafe { core::ptr::write_volatile(common.add(off) as *mut u64, v) }
}

/// Run the v1.2 § 3.1.1 init dance: reset, ACKNOWLEDGE, DRIVER, read
/// device features, write driver features (low + high halves),
/// FEATURES_OK, verify retained. Returns the device's offered
/// feature set so the caller can log it. Panics if the device clears
/// FEATURES_OK after our write — driver features were unacceptable.
///
/// `driver_features` is the full 64-bit set the driver wants. The
/// caller almost always includes VIRTIO_F_VERSION_1 (bit 32).
pub(crate) fn init_transport(dev: &VirtioDevice, driver_features: u64) -> u64 {
    // SAFETY: dev.common_cfg is a 4-KiB MMIO region mapped by
    // virtio::find_device; CC_* offsets are < 0x40.
    unsafe {
        cc_write8(dev.common_cfg, CC_DEVICE_STATUS, 0);
        // Spec recommends waiting until status reads 0 post-reset;
        // QEMU acks immediately. Bound the loop so a misbehaving
        // device can't hang us forever here.
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

        cc_write32(dev.common_cfg, CC_DEVICE_FEATURE_SELECT, 0);
        let dev_lo = cc_read32(dev.common_cfg, CC_DEVICE_FEATURE);
        cc_write32(dev.common_cfg, CC_DEVICE_FEATURE_SELECT, 1);
        let dev_hi = cc_read32(dev.common_cfg, CC_DEVICE_FEATURE);
        let device_features = ((dev_hi as u64) << 32) | dev_lo as u64;

        let drv_lo = (driver_features & 0xFFFF_FFFF) as u32;
        let drv_hi = (driver_features >> 32) as u32;
        cc_write32(dev.common_cfg, CC_DRIVER_FEATURE_SELECT, 0);
        cc_write32(dev.common_cfg, CC_DRIVER_FEATURE, drv_lo);
        cc_write32(dev.common_cfg, CC_DRIVER_FEATURE_SELECT, 1);
        cc_write32(dev.common_cfg, CC_DRIVER_FEATURE, drv_hi);

        cc_write8(
            dev.common_cfg,
            CC_DEVICE_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK,
        );
        let after = cc_read8(dev.common_cfg, CC_DEVICE_STATUS);
        assert_eq!(
            after & STATUS_FEATURES_OK,
            STATUS_FEATURES_OK,
            "virtio: device cleared FEATURES_OK — driver features unacceptable"
        );

        device_features
    }
}

/// Activate queue `idx` with the rings of `queue`. Sets queue_select,
/// validates the device's max queue size against ours, writes the
/// three ring physical addresses, and enables the queue. Returns the
/// notify pointer (*mut u16) the caller writes to to alert the device
/// of new available descriptors.
pub(crate) fn activate_queue(
    dev: &VirtioDevice,
    idx: u16,
    queue: &Virtqueue,
) -> *mut u16 {
    // SAFETY: same as init_transport — common_cfg is the mapped MMIO
    // region; queue_* fields are within COMMON_CFG; notify_base
    // points at the mapped notify region.
    unsafe {
        cc_write16(dev.common_cfg, CC_QUEUE_SELECT, idx);
        let max = cc_read16(dev.common_cfg, CC_QUEUE_SIZE);
        assert!(
            max >= queue.size,
            "virtio: queue {idx} max size {max} < requested {}",
            queue.size
        );
        cc_write16(dev.common_cfg, CC_QUEUE_SIZE, queue.size);
        cc_write64(dev.common_cfg, CC_QUEUE_DESC, queue.desc_phys);
        cc_write64(dev.common_cfg, CC_QUEUE_DRIVER, queue.avail_phys);
        cc_write64(dev.common_cfg, CC_QUEUE_DEVICE, queue.used_phys);

        let queue_notify_off = cc_read16(dev.common_cfg, CC_QUEUE_NOTIFY_OFF);
        let notify_off =
            (queue_notify_off as usize) * (dev.notify_off_multiplier as usize);
        let notify_ptr = dev.notify_base.add(notify_off) as *mut u16;

        cc_write16(dev.common_cfg, CC_QUEUE_ENABLE, 1);

        notify_ptr
    }
}

/// Set DEVICE_STATUS to ACK | DRIVER | FEATURES_OK | DRIVER_OK,
/// bringing the device live for I/O.
pub(crate) fn set_driver_ok(dev: &VirtioDevice) {
    let v = STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK;
    // SAFETY: caller holds a valid VirtioDevice from find_device.
    unsafe { cc_write8(dev.common_cfg, CC_DEVICE_STATUS, v) };
}

/// Notify the device that a queue has new available descriptors.
/// `notify_ptr` is the pointer returned by `activate_queue` for that
/// queue. Writes the queue index (any 16-bit value works without
/// the NOTIFY_DATA feature; we use the index for diagnostics).
pub(crate) fn notify(notify_ptr: *mut u16, queue_idx: u16) {
    // SAFETY: notify_ptr is the mapped notify region pointer for a
    // specific queue, returned by activate_queue.
    unsafe { core::ptr::write_volatile(notify_ptr, queue_idx) };
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

    /// Push a chained request (multiple descriptors linked via
    /// VIRTQ_DESC_F_NEXT) onto the available ring. Each tuple is
    /// (physical address, length, flags). The F_NEXT bit is added
    /// automatically between consecutive parts; do not include it
    /// in the caller's flags. Returns the head descriptor index, or
    /// None if the queue can't fit `parts.len()` descriptors.
    pub fn push_chain(&mut self, parts: &[(u64, u32, u16)]) -> Option<u16> {
        if parts.is_empty() || (parts.len() as u16) > self.num_free {
            return None;
        }
        // Cap chain length at 8 — far past anything our drivers use,
        // keeps the temporary index array on the stack at fixed size.
        assert!(parts.len() <= 8, "virtq: chain longer than 8");

        // Pop n free descriptors off the head of the free chain,
        // recording their indices in order.
        let mut indices: [u16; 8] = [0; 8];
        let mut cur = self.free_head;
        for slot in indices.iter_mut().take(parts.len()) {
            *slot = cur;
            // SAFETY: cur < size — every value reachable via the
            // free chain is a valid descriptor index by construction.
            cur = unsafe { (*self.desc.as_ptr().add(cur as usize)).next };
        }
        self.free_head = cur;
        self.num_free -= parts.len() as u16;

        // Fill each descriptor; chain via F_NEXT to the next index.
        for i in 0..parts.len() {
            let (addr, len, flags) = parts[i];
            let idx = indices[i];
            let (next_idx, chain_flag) = if i + 1 < parts.len() {
                (indices[i + 1], VIRTQ_DESC_F_NEXT)
            } else {
                (0, 0)
            };
            // SAFETY: idx < size.
            unsafe {
                let d = self.desc.as_ptr().add(idx as usize);
                (*d).addr = addr;
                (*d).len = len;
                (*d).flags = (flags & !VIRTQ_DESC_F_NEXT) | chain_flag;
                (*d).next = next_idx;
            }
        }

        // SAFETY: avail header + ring fit in the allocated frame.
        unsafe {
            let avail_idx = (*self.avail.as_ptr()).idx;
            let ring_slot = (avail_idx % self.size) as usize;
            let ring_ptr = (self.avail.as_ptr() as *mut u8).add(4) as *mut u16;
            *ring_ptr.add(ring_slot) = indices[0];
            (*self.avail.as_ptr()).idx = avail_idx.wrapping_add(1);
        }

        Some(indices[0])
    }

    /// Pop the next completed (possibly-chained) request from the
    /// used ring. Walks the descriptor chain starting at elem.id,
    /// freeing every descriptor until the F_NEXT bit clears.
    /// Returns None if no new completions since the last call.
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

        // Walk and free the descriptor chain starting at elem.id.
        // F_NEXT means "more to follow"; the final descriptor has
        // flags & F_NEXT == 0, terminating the walk. A misbehaving
        // device that writes a bogus id would index out-of-bounds
        // on the next add() and panic, which is the right failure
        // mode for a corrupted used ring.
        let mut cur = elem.id as u16;
        loop {
            // SAFETY: cur < size if device + driver are consistent.
            let (flags, old_next) = unsafe {
                let d = self.desc.as_ptr().add(cur as usize);
                let f = (*d).flags;
                let n = (*d).next;
                (*d).next = self.free_head;
                (f, n)
            };
            self.free_head = cur;
            self.num_free += 1;
            if flags & VIRTQ_DESC_F_NEXT == 0 {
                return Some(elem);
            }
            cur = old_next;
        }
    }
}

impl Drop for Virtqueue {
    fn drop(&mut self) {
        // Return the backing frame to the global pool. PhysFrame is
        // Copy; the value is copied here, so the field is unchanged.
        frames::FRAMES.free_frame(self.backing_frame);
    }
}
