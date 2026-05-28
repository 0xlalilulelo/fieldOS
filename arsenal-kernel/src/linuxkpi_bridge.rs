// SPDX-License-Identifier: BSD-2-Clause

//! Bridge fns the linuxkpi crate consumes via `extern "C"` to reach
//! kernel-side primitives. The cross-crate dependency is one-way
//! (arsenal-kernel depends on linuxkpi, never the reverse), so the
//! linuxkpi side cannot `use crate::pci` directly. Each kernel
//! primitive linuxkpi needs gets a `linuxkpi_<subsystem>_<op>`
//! `#[unsafe(no_mangle)]` extern fn here that delegates.
//!
//! The serial sink (`linuxkpi_serial_sink`) lives in serial.rs
//! because it predates this module (M1-2-1) and is the single
//! exception. New bridge fns land here.
//!
//! Adding a bridge fn:
//!   1. Define the `extern "C"` declaration in the consuming
//!      linuxkpi module (linuxkpi/src/pci.rs etc.).
//!   2. Add the matching `#[unsafe(no_mangle)] pub unsafe extern
//!      "C" fn` here that delegates to the kernel primitive.
//!   3. Document the safety contract on both sides — they must
//!      match.

use crate::{apic, frames, paging, pci, virtio};
use alloc::boxed::Box;

/// Read the global LAPIC tick counter. M1-2-5 Part A: backs
/// linuxkpi's `jiffies` / `msleep` / `udelay` over apic::ticks().
#[unsafe(no_mangle)]
pub extern "C" fn linuxkpi_jiffies() -> u64 {
    apic::ticks() as u64
}
use x86_64::PhysAddr;
use x86_64::structures::paging::{PhysFrame, Size4KiB};

/// Flat C-shaped MSI-X capability descriptor for `linuxkpi_pci_
/// msix_info`. Mirrors `pci::MsixInfo`'s shape but lives at the
/// bridge boundary so linuxkpi can declare the same `#[repr(C)]`
/// struct without depending on arsenal-kernel's types.
#[repr(C)]
pub struct LinuxkpiMsixInfo {
    /// 1 if MSI-X capability is present + parsed; 0 otherwise.
    pub present: u32,
    pub cap_offset: u32,
    pub table_size: u32,
    pub table_bar: u32,
    pub table_offset: u32,
}

/// Flat C-shaped virtio-modern transport descriptor for
/// `linuxkpi_virtio_resolve`. Mirrors `virtio::VirtioDevice`'s
/// shape with raw u64 pointer values so linuxkpi can declare
/// the same `#[repr(C)]` struct without dragging in
/// arsenal-kernel types.
#[repr(C)]
pub struct LinuxkpiVirtioDev {
    /// 1 if the function at (bus, dev, func) is a virtio device
    /// with valid modern transport caps; 0 otherwise.
    pub present: u32,
    pub device_id: u16,
    pub _pad0: u16,
    pub common_cfg: u64,
    pub notify_base: u64,
    pub notify_off_multiplier: u32,
    pub _pad1: u32,
    pub isr: u64,
    pub device_cfg: u64,
}

/// PCI config-space dword read. Delegates to `pci::config_read32`.
///
/// # Safety
/// `(bus, dev, func)` must reference a present PCI function;
/// `offset` must be dword-aligned and < 0x100. Standard PCI
/// config-read invariants — see `pci::config_read32` SAFETY.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_pci_config_read32(
    bus: u8,
    dev: u8,
    func: u8,
    offset: u8,
) -> u32 {
    // SAFETY: caller's contract — see fn doc.
    unsafe { pci::config_read32(bus, dev, func, offset) }
}

/// PCI config-space dword write. Delegates to `pci::config_write32`.
///
/// # Safety
/// As `linuxkpi_pci_config_read32`, plus the caller must
/// understand the hardware effect of the write (command/status,
/// BARs, capability state all have side effects).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_pci_config_write32(
    bus: u8,
    dev: u8,
    func: u8,
    offset: u8,
    val: u32,
) {
    // SAFETY: caller's contract.
    unsafe { pci::config_write32(bus, dev, func, offset, val) }
}

/// Resolve BAR `bar` of `(bus, dev, func)` to a physical address.
/// Returns 0 for I/O BARs and for absent BARs. Delegates to
/// `pci::bar_address`.
///
/// # Safety
/// `(bus, dev, func)` must reference a present PCI function;
/// `bar` in 0..=5; for 64-bit BARs caller should not pass 5.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_pci_bar_address(
    bus: u8,
    dev: u8,
    func: u8,
    bar: u8,
) -> u64 {
    // SAFETY: caller's contract.
    unsafe { pci::bar_address(bus, dev, func, bar) }
}

/// Map device MMIO at `[phys, phys+len)` into the kernel's HHDM
/// alias with NO_CACHE. Delegates to `paging::map_mmio`. Idempotent
/// on overlap with existing mappings.
#[unsafe(no_mangle)]
pub extern "C" fn linuxkpi_paging_map_mmio(phys: u64, len: u64) {
    paging::map_mmio(phys, len);
}

/// HHDM physical-to-virtual offset. The shim's `pci_iomap` and
/// `dma_alloc_coherent` use this for the virt = phys + hhdm
/// translation that x86_64 cache-coherent DMA assumes.
#[unsafe(no_mangle)]
pub extern "C" fn linuxkpi_paging_hhdm_offset() -> u64 {
    paging::hhdm_offset()
}

/// Allocate one 4-KiB physical frame. Returns the physical address
/// or 0 on exhaustion. The shim's `dma_alloc_coherent` wraps this
/// — frame addresses are page-aligned by construction so they
/// satisfy `dma_addr_t` alignment requirements directly.
#[unsafe(no_mangle)]
pub extern "C" fn linuxkpi_frames_alloc_frame() -> u64 {
    frames::FRAMES
        .alloc_frame()
        .map(|f| f.start_address().as_u64())
        .unwrap_or(0)
}

/// Free a physical frame previously returned by
/// `linuxkpi_frames_alloc_frame`.
///
/// # Safety
/// `phys` must be a 4-KiB-aligned physical address obtained from
/// `linuxkpi_frames_alloc_frame` and not yet freed. Double-free
/// will corrupt the frame allocator's free-list.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_frames_free_frame(phys: u64) {
    let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(phys));
    frames::FRAMES.free_frame(frame);
}

/// Send LAPIC end-of-interrupt. The shim's per-slot dispatcher
/// calls this after every IRQ handler invocation.
#[unsafe(no_mangle)]
pub extern "C" fn linuxkpi_lapic_eoi() {
    apic::send_eoi();
}

/// Resolve the virtio-modern transport at `(bus, dev, func)`
/// into `*out`. Sets `out.present = 0` when the function is not
/// a virtio device or lacks the modern capability set; sets
/// `present = 1` and populates the rest when present. Mirrors
/// `virtio::try_resolve` semantics — `want` is the PCI device_id
/// to match (the caller has already filtered by virtio vendor).
///
/// # Safety
/// `out` must point to writable storage of size + alignment
/// matching `LinuxkpiVirtioDev`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_virtio_resolve(
    bus: u8,
    dev: u8,
    func: u8,
    want_device_id: u16,
    out: *mut LinuxkpiVirtioDev,
) {
    if out.is_null() {
        return;
    }
    match virtio::try_resolve(bus, dev, func, want_device_id) {
        Some(d) => {
            // SAFETY: out is non-null per the check; caller's
            // contract ensures correct alignment + size.
            unsafe {
                (*out).present = 1;
                (*out).device_id = d.device_id;
                (*out)._pad0 = 0;
                (*out).common_cfg = d.common_cfg as u64;
                (*out).notify_base = d.notify_base as u64;
                (*out).notify_off_multiplier = d.notify_off_multiplier;
                (*out)._pad1 = 0;
                (*out).isr = d.isr as u64;
                (*out).device_cfg = d.device_cfg as u64;
            }
        }
        None => {
            // SAFETY: see above.
            unsafe { (*out).present = 0 }
        }
    }
}

/// Read the MSI-X capability of `(bus, dev, func)` into `*out`.
/// Sets `out.present = 0` when the function does not have an
/// MSI-X capability; sets `present = 1` and populates the rest
/// when present.
///
/// # Safety
/// `out` must point to writable storage of size + alignment
/// matching `LinuxkpiMsixInfo`. `(bus, dev, func)` is treated
/// as a probe (absent functions return `present = 0`); no
/// validity precondition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_pci_msix_info(
    bus: u8,
    dev: u8,
    func: u8,
    out: *mut LinuxkpiMsixInfo,
) {
    if out.is_null() {
        return;
    }
    match pci::msix_info(bus, dev, func) {
        Some(info) => {
            // SAFETY: out is non-null per the check; caller's
            // contract ensures it is properly aligned + sized.
            unsafe {
                (*out).present = 1;
                (*out).cap_offset = info.cap_offset as u32;
                (*out).table_size = info.table_size;
                (*out).table_bar = info.table_bar as u32;
                (*out).table_offset = info.table_offset;
            }
        }
        None => {
            // SAFETY: see above.
            unsafe {
                (*out).present = 0;
            }
        }
    }
}

// =====================================================================
// Virtqueue + virtio-transport bridge (M1-2-5 closing-commit work).
// Wraps the arsenal-kernel split-virtqueue machinery (Virtqueue +
// init_transport / activate_queue / set_driver_ok / notify) so the
// linuxkpi shim's virtqueue_add_* / kick / get_buf / find_vqs panic-
// stubs can drive real I/O without reimplementing the vring.
// =====================================================================

use crate::virtio::{
    Virtqueue, VirtioDevice, activate_queue, set_driver_ok,
    notify as virtio_notify, cc_read8, cc_write8, cc_read16, cc_write16,
    cc_read32, cc_write32,
    CC_QUEUE_SELECT, CC_QUEUE_SIZE, CC_DEVICE_STATUS, CC_MSIX_CONFIG,
    CC_DEVICE_FEATURE_SELECT, CC_DEVICE_FEATURE,
    CC_DRIVER_FEATURE_SELECT, CC_DRIVER_FEATURE,
    STATUS_ACKNOWLEDGE, STATUS_DRIVER, STATUS_FEATURES_OK,
};

/// Ring-physical-address descriptor for `linuxkpi_virtqueue_info`,
/// the values the linuxkpi shim hands to `activate_queue` and the
/// queue-size cap it negotiated.
#[repr(C)]
pub struct LinuxkpiVqInfo {
    pub size: u16,
    pub _pad: [u8; 6],
    pub desc_phys: u64,
    pub avail_phys: u64,
    pub used_phys: u64,
}

/// Flat C-shaped chain segment for `linuxkpi_virtqueue_push_chain`.
#[repr(C)]
pub struct LinuxkpiVqChainPart {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub _pad: u16,
}

/// Allocate a Virtqueue of `size` descriptors and return a leaked
/// Box pointer. Caller stores it in the shim's `struct virtqueue.priv`
/// and pairs with `linuxkpi_virtqueue_free`.
#[unsafe(no_mangle)]
pub extern "C" fn linuxkpi_virtqueue_new(size: u16) -> *mut core::ffi::c_void {
    Box::into_raw(Box::new(Virtqueue::new(size))) as *mut core::ffi::c_void
}

/// Release a Virtqueue previously returned by `linuxkpi_virtqueue_new`.
///
/// # Safety
/// `handle` must be a pointer returned by `linuxkpi_virtqueue_new`
/// and must not have been freed already.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_virtqueue_free(handle: *mut core::ffi::c_void) {
    if handle.is_null() {
        return;
    }
    // SAFETY: forwarded caller contract.
    drop(unsafe { Box::from_raw(handle as *mut Virtqueue) });
}

/// Read a Virtqueue's size + ring physical addresses into `out`
/// (the values `activate_queue` will write into COMMON_CFG).
///
/// # Safety
/// `handle` must be a live `linuxkpi_virtqueue_new` pointer; `out`
/// must point to a writable `LinuxkpiVqInfo`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_virtqueue_info(
    handle: *const core::ffi::c_void,
    out: *mut LinuxkpiVqInfo,
) {
    // SAFETY: forwarded caller contract.
    unsafe {
        let vq = &*(handle as *const Virtqueue);
        (*out).size = vq.size;
        (*out).desc_phys = vq.desc_phys;
        (*out).avail_phys = vq.avail_phys;
        (*out).used_phys = vq.used_phys;
    }
}

/// Push one descriptor onto a Virtqueue. Returns the descriptor index
/// (≥ 0), or -1 if the queue is full.
///
/// # Safety
/// `handle` must be a live `linuxkpi_virtqueue_new` pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_virtqueue_push_descriptor(
    handle: *mut core::ffi::c_void,
    addr: u64,
    len: u32,
    flags: u16,
) -> i32 {
    // SAFETY: forwarded caller contract.
    unsafe {
        match (*(handle as *mut Virtqueue)).push_descriptor(addr, len, flags) {
            Some(idx) => idx as i32,
            None => -1,
        }
    }
}

/// Push 1..=8 chained descriptors via `parts[0..nparts]`. Returns
/// the head descriptor index (≥ 0), or -1 on too-many / empty / full.
/// arsenal-kernel's `push_chain` caps the chain at 8.
///
/// # Safety
/// `handle` is a live Virtqueue pointer; `parts` points to `nparts`
/// readable `LinuxkpiVqChainPart` entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_virtqueue_push_chain(
    handle: *mut core::ffi::c_void,
    parts: *const LinuxkpiVqChainPart,
    nparts: u32,
) -> i32 {
    if nparts == 0 || nparts > 8 || parts.is_null() {
        return -1;
    }
    let mut buf: [(u64, u32, u16); 8] = [(0, 0, 0); 8];
    // SAFETY: forwarded caller contract.
    unsafe {
        for i in 0..nparts as usize {
            let p = &*parts.add(i);
            buf[i] = (p.addr, p.len, p.flags);
        }
        let slice = &buf[..nparts as usize];
        match (*(handle as *mut Virtqueue)).push_chain(slice) {
            Some(idx) => idx as i32,
            None => -1,
        }
    }
}

/// Pop one used buffer from a Virtqueue. Returns `true` if one was
/// dequeued (writes the descriptor head index to `*out_id` and the
/// device-reported bytes-used to `*out_len`); `false` if the used
/// ring is empty.
///
/// # Safety
/// `handle` is a live Virtqueue pointer; `out_id` + `out_len` are
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_virtqueue_pop_used(
    handle: *mut core::ffi::c_void,
    out_id: *mut u32,
    out_len: *mut u32,
) -> bool {
    // SAFETY: forwarded caller contract.
    unsafe {
        match (*(handle as *mut Virtqueue)).pop_used() {
            Some(elem) => {
                *out_id = elem.id;
                *out_len = elem.len;
                true
            }
            None => false,
        }
    }
}

/// Read the device-reported max queue size for queue `idx` (the value
/// at CC_QUEUE_SIZE after CC_QUEUE_SELECT = idx). 0 means the device
/// doesn't implement that queue.
///
/// # Safety
/// `common_cfg` must be the mapped COMMON_CFG region for a live virtio
/// device.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_virtio_read_queue_size(
    common_cfg: *mut u8,
    idx: u16,
) -> u16 {
    // SAFETY: forwarded caller contract.
    unsafe {
        cc_write16(common_cfg, CC_QUEUE_SELECT, idx);
        cc_read16(common_cfg, CC_QUEUE_SIZE)
    }
}

/// Activate `queue_idx` on the device: write select / size / ring
/// physical addresses / ENABLE = 1, then return the notify-doorbell
/// pointer the linuxkpi shim uses for subsequent `kick` calls.
///
/// # Safety
/// All pointer arguments must be live MMIO; `queue_handle` is a live
/// Virtqueue.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_virtio_activate_queue(
    common_cfg: *mut u8,
    notify_base: *mut u8,
    notify_off_multiplier: u32,
    queue_idx: u16,
    queue_handle: *const core::ffi::c_void,
) -> *mut core::ffi::c_void {
    // arsenal-kernel's activate_queue takes a &VirtioDevice for the
    // (common_cfg, notify_base, notify_off_multiplier) tuple; the
    // other fields aren't read. Construct a thin shim VirtioDevice
    // for the call.
    let vdev = VirtioDevice {
        bus: 0,
        dev: 0,
        func: 0,
        device_id: 0,
        common_cfg,
        notify_base,
        notify_off_multiplier,
        isr: core::ptr::null_mut(),
        device_cfg: core::ptr::null_mut(),
    };
    // SAFETY: forwarded caller contract.
    let queue = unsafe { &*(queue_handle as *const Virtqueue) };
    activate_queue(&vdev, queue_idx, queue) as *mut core::ffi::c_void
}

/// Set DEVICE_STATUS to ACK | DRIVER | FEATURES_OK | DRIVER_OK,
/// bringing the device live for I/O after all queues are activated.
///
/// # Safety
/// `common_cfg` is the mapped COMMON_CFG region for a live virtio
/// device whose feature negotiation already completed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_virtio_set_driver_ok(common_cfg: *mut u8) {
    let vdev = VirtioDevice {
        bus: 0,
        dev: 0,
        func: 0,
        device_id: 0,
        common_cfg,
        notify_base: core::ptr::null_mut(),
        notify_off_multiplier: 0,
        isr: core::ptr::null_mut(),
        device_cfg: core::ptr::null_mut(),
    };
    set_driver_ok(&vdev);
}

/// Notify the device that queue `queue_idx` has new available
/// descriptors. `notify_ptr` is the pointer returned by
/// `linuxkpi_virtio_activate_queue`.
///
/// # Safety
/// `notify_ptr` is a live notify-region pointer for `queue_idx`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_virtio_notify(
    notify_ptr: *mut core::ffi::c_void,
    queue_idx: u16,
) {
    virtio_notify(notify_ptr as *mut u16, queue_idx);
}

/// Run the virtio v1.2 § 3.1.1 init dance with **bus-side feature
/// intersection**: reset, ACKNOWLEDGE, DRIVER, read device features,
/// AND with `driver_features` (what the driver supports), write the
/// negotiated set back as driver features, FEATURES_OK, verify
/// retained. Returns the negotiated u64 — the bits the device offered
/// AND the driver claimed, which the linuxkpi shim stores in
/// `vdev.features`. Panics if the device clears FEATURES_OK (per the
/// existing assert in `virtio::init_transport`); the intersection
/// guarantees we never ask for an unsupported bit, so the assertion
/// fires only on a broken transport.
///
/// Distinct from `virtio::init_transport`, which requires the caller
/// to pre-intersect (the native blk/net drivers know their feature
/// requirements at compile time). Linux convention is that drivers
/// advertise *supported* features and the bus computes the
/// intersection; this bridge is that bus side.
///
/// # Safety
/// `common_cfg` is the mapped COMMON_CFG region for a live virtio
/// device (returned by `linuxkpi_virtio_resolve`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_virtio_init_transport(
    common_cfg: *mut u8,
    driver_features: u64,
) -> u64 {
    // SAFETY: common_cfg is a 4-KiB MMIO region per the caller's
    // contract; all CC_* offsets are < 0x40.
    unsafe {
        cc_write8(common_cfg, CC_DEVICE_STATUS, 0);
        for _ in 0..16 {
            if cc_read8(common_cfg, CC_DEVICE_STATUS) == 0 {
                break;
            }
        }
        cc_write8(common_cfg, CC_DEVICE_STATUS, STATUS_ACKNOWLEDGE);
        cc_write8(
            common_cfg,
            CC_DEVICE_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER,
        );

        cc_write32(common_cfg, CC_DEVICE_FEATURE_SELECT, 0);
        let dev_lo = cc_read32(common_cfg, CC_DEVICE_FEATURE);
        cc_write32(common_cfg, CC_DEVICE_FEATURE_SELECT, 1);
        let dev_hi = cc_read32(common_cfg, CC_DEVICE_FEATURE);
        let device_features = ((dev_hi as u64) << 32) | dev_lo as u64;

        let negotiated = driver_features & device_features;
        let drv_lo = (negotiated & 0xFFFF_FFFF) as u32;
        let drv_hi = (negotiated >> 32) as u32;
        cc_write32(common_cfg, CC_DRIVER_FEATURE_SELECT, 0);
        cc_write32(common_cfg, CC_DRIVER_FEATURE, drv_lo);
        cc_write32(common_cfg, CC_DRIVER_FEATURE_SELECT, 1);
        cc_write32(common_cfg, CC_DRIVER_FEATURE, drv_hi);

        cc_write8(
            common_cfg,
            CC_DEVICE_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK,
        );
        let after = cc_read8(common_cfg, CC_DEVICE_STATUS);
        assert_eq!(
            after & STATUS_FEATURES_OK,
            STATUS_FEATURES_OK,
            "linuxkpi: device cleared FEATURES_OK after intersection write — transport broken"
        );

        negotiated
    }
}

/// Write the MSI-X vector index for the virtio config-change irq
/// into `CC_MSIX_CONFIG` (offset 0x10 in COMMON_CFG). Per virtio
/// v1.2 § 4.1.5.1.2, the device fires its config-change irq via
/// the MSI-X vector at the written index after this write; the
/// special value `VIRTIO_MSI_NO_VECTOR` (0xFFFF) disables the
/// irq. Read back the same offset to confirm the write (the spec
/// requires the device echo the written value if the vector is
/// valid; reading back 0xFFFF after writing a real index would
/// signal allocation failure). Round-22c wiring: balloon's
/// `virtballoon_changed` becomes reachable from QMP-driven
/// config updates after this write.
///
/// # Safety
/// `common_cfg` is the mapped COMMON_CFG region for a live virtio
/// device.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_virtio_set_msix_config_vector(
    common_cfg: *mut u8,
    vector_idx: u16,
) -> u16 {
    // SAFETY: caller's contract; CC_MSIX_CONFIG is at offset 0x10
    // (within the 4-KiB COMMON_CFG region).
    unsafe {
        cc_write16(common_cfg, CC_MSIX_CONFIG, vector_idx);
        cc_read16(common_cfg, CC_MSIX_CONFIG)
    }
}

/// Reset a virtio device by writing DEVICE_STATUS = 0 and waiting
/// (bounded) for the device to acknowledge per v1.2 § 2.1.2. The
/// device returns to RESET; subsequent re-initialization must go
/// through `linuxkpi_virtio_init_transport` again. Used when a
/// driver's probe declines (returns negative) so the device is left
/// in a clean state for a later driver to re-claim — the linuxkpi
/// self-test's no-op driver leans on this for blk/net/rng, which
/// `virtio_blk::smoke` / `virtio_net::smoke` then re-initialize.
///
/// # Safety
/// `common_cfg` is the mapped COMMON_CFG region for a live virtio
/// device.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linuxkpi_virtio_reset_device(common_cfg: *mut u8) {
    // SAFETY: caller's contract.
    unsafe {
        cc_write8(common_cfg, CC_DEVICE_STATUS, 0);
        for _ in 0..16 {
            if cc_read8(common_cfg, CC_DEVICE_STATUS) == 0 {
                break;
            }
        }
    }
}
