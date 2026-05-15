// SPDX-License-Identifier: BSD-2-Clause

//! Linux virtio bus adapter — `struct virtio_driver` registration
//! model + virtio-device discovery over arsenal-kernel's virtio
//! transport (consumed via the bridge externs in
//! `arsenal-kernel/src/linuxkpi_bridge.rs`). Closes the "shim
//! foundation" devlog cluster (M1-2-1 + 2-2 + 2-3) per the
//! M1-2 HANDOFF.
//!
//! M1-2-3 surface: registration + .probe dispatch + virtio_cread/
//! cwrite over device_cfg + the struct shape that lets balloon's
//! C source compile against shim_c.h. The virtqueue surface
//! (find_vqs / virtqueue_add_outbuf / virtqueue_kick /
//! virtqueue_get_buf) lands as functional implementations at
//! M1-2-5 when virtio-balloon online demands them; M1-2-3 ships
//! the symbols as panic-on-call stubs so any inherited driver
//! that needs them at link time fails loudly rather than
//! silently returning bad values.
//!
//! Match semantics: Linux's virtio_device_id is `{ device, vendor }`
//! where `device` is the VIRTIO_ID_* constant (e.g.,
//! VIRTIO_ID_BALLOON = 5), not the PCI device_id (e.g., 0x1045
//! for modern virtio-balloon-pci). The shim translates PCI device
//! IDs to VIRTIO_ID_* per virtio v1.2 § 4.1.2 ("PCI Device IDs"):
//!   - 0x1000..=0x103F: legacy.   VIRTIO_ID = pci_id - 0x1000
//!   - 0x1040..=0x107F: modern.   VIRTIO_ID = pci_id - 0x1040
//!
//! VIRTIO_DEV_ANY_ID (0xFFFFFFFF) in the driver's id_table matches
//! anything.

extern crate alloc;

use alloc::vec::Vec;
use core::ptr::NonNull;
use spin::Mutex;

use crate::types::{c_char, c_int, c_uint, c_void};

unsafe extern "C" {
    fn linuxkpi_pci_config_read32(bus: u8, dev: u8, func: u8, offset: u8) -> u32;
    fn linuxkpi_virtio_resolve(
        bus: u8,
        dev: u8,
        func: u8,
        want_device_id: u16,
        out: *mut VirtioDevRaw,
    );
}

/// Mirror of `arsenal_kernel::linuxkpi_bridge::LinuxkpiVirtioDev`.
#[repr(C)]
struct VirtioDevRaw {
    present: u32,
    device_id: u16,
    _pad0: u16,
    common_cfg: u64,
    notify_base: u64,
    notify_off_multiplier: u32,
    _pad1: u32,
    isr: u64,
    device_cfg: u64,
}

const VIRTIO_VENDOR: u16 = 0x1AF4;

// Subset of <uapi/linux/virtio_ids.h>.
pub const VIRTIO_ID_NET: u32 = 1;
pub const VIRTIO_ID_BLOCK: u32 = 2;
pub const VIRTIO_ID_CONSOLE: u32 = 3;
pub const VIRTIO_ID_RNG: u32 = 4;
pub const VIRTIO_ID_BALLOON: u32 = 5;

/// Match-any wildcard for `virtio_device_id::device` /
/// `virtio_device_id::vendor` — Linux <linux/mod_devicetable.h>.
pub const VIRTIO_DEV_ANY_ID: u32 = 0xFFFFFFFF;

// =====================================================================
// Driver-facing types — match Linux's <linux/virtio.h> +
// <linux/mod_devicetable.h> layouts. Trimmed to the fields
// inherited drivers actually consume at M1; balloon at M1-2-5
// will surface any missing fields and we add them then.
// =====================================================================

#[repr(C)]
#[derive(Clone, Copy)]
pub struct virtio_device_id {
    /// VIRTIO_ID_* per <uapi/linux/virtio_ids.h>. Use
    /// VIRTIO_DEV_ANY_ID to wildcard.
    pub device: u32,
    /// Vendor; Linux uses VIRTIO_DEV_ANY_ID for nearly all drivers.
    pub vendor: u32,
}

/// `struct virtio_device`. Linux's full struct is large (~ KB
/// with embedded device + config_ops table); we mirror the fields
/// inherited drivers reach for, plus the resolved transport
/// pointers the shim populates.
#[repr(C)]
pub struct virtio_device {
    /// Negotiated VIRTIO_ID (translated from PCI device_id at
    /// match time). Inherited drivers compare against
    /// VIRTIO_ID_*.
    pub id_device: u32,
    pub id_vendor: u32,
    /// Per-driver state (`dev_set_drvdata` / `dev_get_drvdata`).
    /// Opaque to the shim.
    pub priv_data: *mut c_void,
    /// Resolved transport pointers from arsenal-kernel's modern
    /// PCI transport. The shim's virtio_cread / virtio_cwrite
    /// access device_cfg directly; find_vqs / virtqueue_*
    /// (deferred to M1-2-5) will use common_cfg + notify_base.
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub _pad: u8,
    pub common_cfg: *mut u8,
    pub notify_base: *mut u8,
    pub notify_off_multiplier: u32,
    pub isr: *mut u8,
    pub device_cfg: *mut u8,
}

// SAFETY: virtio_device is a passive descriptor; concurrent
// access is the driver's responsibility (Linux convention).
unsafe impl Send for virtio_device {}
unsafe impl Sync for virtio_device {}

/// `struct virtio_driver`. M1-2-3 trimmed surface: name +
/// id_table + probe + remove. The full Linux struct includes
/// feature_table / validate / scan / config_changed / freeze /
/// restore — added when balloon's compile (M1-2-4 / 2-5)
/// demands them.
#[repr(C)]
pub struct virtio_driver {
    pub name: *const c_char,
    pub id_table: *const virtio_device_id,
    pub probe: Option<unsafe extern "C" fn(*mut virtio_device) -> c_int>,
    pub remove: Option<unsafe extern "C" fn(*mut virtio_device)>,
}

unsafe impl Send for virtio_driver {}
unsafe impl Sync for virtio_driver {}

// =====================================================================
// Registry — drivers register here at module init; the shim's
// virtio_register_driver walks every present virtio device + the
// driver's id_table.
// =====================================================================

struct RegisteredDriver {
    drv: NonNull<virtio_driver>,
}

unsafe impl Send for RegisteredDriver {}

static REGISTRY: Mutex<Vec<RegisteredDriver>> = Mutex::new(Vec::new());

/// Register `drv` and dispatch its `.probe` against every
/// currently-present virtio device whose VIRTIO_ID matches the
/// driver's id_table. Returns 0 on success.
///
/// # Safety
/// `drv` must point to a valid `virtio_driver` whose `name`,
/// `id_table`, and function pointers remain valid for the
/// lifetime of the registration (typically `'static`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn register_virtio_driver(drv: *mut virtio_driver) -> c_int {
    let drv = match NonNull::new(drv) {
        Some(d) => d,
        None => return -1,
    };

    // SAFETY: drv is non-null per the check above; caller's
    // contract guarantees the function pointers + id_table.
    let id_table = unsafe { (*drv.as_ptr()).id_table };
    let probe = unsafe { (*drv.as_ptr()).probe };

    walk_virtio_devices(|bus, dev, func, pci_device_id| {
        let virtio_id = pci_to_virtio_id(pci_device_id);
        if !match_id_table(id_table, virtio_id) {
            return;
        }
        let Some(probe_fn) = probe else { return };
        // Resolve the transport pointers via the bridge; skip
        // if resolution fails (device-cfg cap missing, etc.).
        let mut raw = VirtioDevRaw {
            present: 0,
            device_id: 0,
            _pad0: 0,
            common_cfg: 0,
            notify_base: 0,
            notify_off_multiplier: 0,
            _pad1: 0,
            isr: 0,
            device_cfg: 0,
        };
        // SAFETY: bridge fn — out points to writable stack storage.
        unsafe {
            linuxkpi_virtio_resolve(bus, dev, func, pci_device_id, &mut raw as *mut VirtioDevRaw)
        };
        if raw.present == 0 {
            return;
        }
        let mut vdev = virtio_device {
            id_device: virtio_id,
            id_vendor: VIRTIO_VENDOR as u32,
            priv_data: core::ptr::null_mut(),
            bus,
            dev,
            func,
            _pad: 0,
            common_cfg: raw.common_cfg as *mut u8,
            notify_base: raw.notify_base as *mut u8,
            notify_off_multiplier: raw.notify_off_multiplier,
            isr: raw.isr as *mut u8,
            device_cfg: raw.device_cfg as *mut u8,
        };
        // SAFETY: probe_fn is a valid extern "C" fn per the
        // driver's declaration; vdev lives on this stack frame
        // for the duration of the call.
        unsafe {
            let _ = probe_fn(&mut vdev as *mut virtio_device);
        }
    });

    REGISTRY.lock().push(RegisteredDriver { drv });
    0
}

/// Unregister `drv`. M1-2-3: removes from the registry without
/// calling `.remove` (no driver currently exits, and the remove
/// path needs the bound virtio_device table that lands when
/// per-driver state tracking arrives).
///
/// # Safety
/// `drv` must have been previously registered via
/// `register_virtio_driver` and must remain valid through this
/// call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unregister_virtio_driver(drv: *mut virtio_driver) {
    let Some(target) = NonNull::new(drv) else {
        return;
    };
    let mut reg = REGISTRY.lock();
    reg.retain(|entry| entry.drv.as_ptr() != target.as_ptr());
}

// =====================================================================
// virtio_cread / virtio_cwrite — typed accessors over device_cfg.
// =====================================================================

/// Read a u8 from `vdev`'s device_cfg at `offset`.
///
/// # Safety
/// `vdev` must be populated by `register_virtio_driver`'s probe
/// dispatch; `offset` must be within the device's published
/// device_cfg region.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtio_cread8(vdev: *const virtio_device, offset: c_uint) -> u8 {
    if vdev.is_null() {
        return 0;
    }
    // SAFETY: caller's contract — vdev valid + offset in bounds;
    // device_cfg points to MMIO mapped by paging::map_mmio.
    unsafe { core::ptr::read_volatile((*vdev).device_cfg.add(offset as usize)) }
}

/// Read a u16 from `vdev`'s device_cfg at `offset`.
///
/// # Safety
/// As `virtio_cread8`. `offset` should be 2-byte aligned per
/// the virtio spec for typed reads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtio_cread16(vdev: *const virtio_device, offset: c_uint) -> u16 {
    if vdev.is_null() {
        return 0;
    }
    // SAFETY: see virtio_cread8.
    unsafe {
        let p = (*vdev).device_cfg.add(offset as usize) as *const u16;
        core::ptr::read_volatile(p)
    }
}

/// Read a u32 from `vdev`'s device_cfg at `offset`.
///
/// # Safety
/// As `virtio_cread8`. `offset` should be 4-byte aligned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtio_cread32(vdev: *const virtio_device, offset: c_uint) -> u32 {
    if vdev.is_null() {
        return 0;
    }
    // SAFETY: see virtio_cread8.
    unsafe {
        let p = (*vdev).device_cfg.add(offset as usize) as *const u32;
        core::ptr::read_volatile(p)
    }
}

/// Write a u8 to `vdev`'s device_cfg at `offset`.
///
/// # Safety
/// As `virtio_cread8`. Caller must understand the device's
/// expected response to the write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtio_cwrite8(vdev: *mut virtio_device, offset: c_uint, val: u8) {
    if vdev.is_null() {
        return;
    }
    // SAFETY: see virtio_cread8.
    unsafe { core::ptr::write_volatile((*vdev).device_cfg.add(offset as usize), val) }
}

/// Write a u16 to `vdev`'s device_cfg at `offset`.
///
/// # Safety
/// As `virtio_cwrite8`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtio_cwrite16(vdev: *mut virtio_device, offset: c_uint, val: u16) {
    if vdev.is_null() {
        return;
    }
    // SAFETY: see virtio_cread8.
    unsafe {
        let p = (*vdev).device_cfg.add(offset as usize) as *mut u16;
        core::ptr::write_volatile(p, val);
    }
}

/// Write a u32 to `vdev`'s device_cfg at `offset`.
///
/// # Safety
/// As `virtio_cwrite8`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtio_cwrite32(vdev: *mut virtio_device, offset: c_uint, val: u32) {
    if vdev.is_null() {
        return;
    }
    // SAFETY: see virtio_cread8.
    unsafe {
        let p = (*vdev).device_cfg.add(offset as usize) as *mut u32;
        core::ptr::write_volatile(p, val);
    }
}

// =====================================================================
// Virtqueue — M1-2-3 ships the type + entry-point symbols as
// panic-on-call stubs. Functional implementations land at M1-2-5
// when virtio-balloon's first call demands real virtqueue
// machinery (the "gap-filling" sub-block per HANDOFF).
// =====================================================================

/// Linux's `struct virtqueue` is opaque to drivers; they only
/// hold pointers. M1-2-3 declares it as an opaque placeholder so
/// inherited driver code that does `struct virtqueue *vq;`
/// compiles. Actual layout populated at M1-2-5.
#[repr(C)]
pub struct virtqueue {
    _opaque: [u8; 16],
}

/// `find_vqs` — discover and configure virtqueues for `vdev`.
/// Stubbed at M1-2-3; lands at M1-2-5 when balloon demands.
///
/// # Safety
/// Calling this at M1-2-3 panics; never reached under the
/// existing self-test path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn find_vqs(
    _vdev: *mut virtio_device,
    _nvqs: c_uint,
    _vqs: *mut *mut virtqueue,
    _names: *const *const c_char,
) -> c_int {
    panic!("linuxkpi: find_vqs not yet implemented (lands at M1-2-5)")
}

/// `virtqueue_add_outbuf` — submit an outbound buffer to a vq.
/// Stubbed at M1-2-3.
///
/// # Safety
/// Calling this at M1-2-3 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtqueue_add_outbuf(
    _vq: *mut virtqueue,
    _sg: *const c_void,
    _num: c_uint,
    _data: *mut c_void,
    _gfp: u32,
) -> c_int {
    panic!("linuxkpi: virtqueue_add_outbuf not yet implemented (lands at M1-2-5)")
}

/// `virtqueue_add_inbuf` — submit an inbound buffer to a vq.
/// Stubbed at M1-2-3.
///
/// # Safety
/// Calling this at M1-2-3 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtqueue_add_inbuf(
    _vq: *mut virtqueue,
    _sg: *const c_void,
    _num: c_uint,
    _data: *mut c_void,
    _gfp: u32,
) -> c_int {
    panic!("linuxkpi: virtqueue_add_inbuf not yet implemented (lands at M1-2-5)")
}

/// `virtqueue_kick` — notify the device that buffers are
/// available. Stubbed at M1-2-3.
///
/// # Safety
/// Calling this at M1-2-3 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtqueue_kick(_vq: *mut virtqueue) -> c_int {
    panic!("linuxkpi: virtqueue_kick not yet implemented (lands at M1-2-5)")
}

/// `virtqueue_get_buf` — drain one completed buffer from a vq.
/// Stubbed at M1-2-3.
///
/// # Safety
/// Calling this at M1-2-3 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtqueue_get_buf(
    _vq: *mut virtqueue,
    _len: *mut c_uint,
) -> *mut c_void {
    panic!("linuxkpi: virtqueue_get_buf not yet implemented (lands at M1-2-5)")
}

// =====================================================================
// Internal helpers.
// =====================================================================

/// Walk every (bus, dev, func) and invoke `f` for each present
/// function whose vendor is the virtio vendor (0x1AF4). Mirrors
/// the pattern in pci.rs::pci_walk + filters early.
fn walk_virtio_devices<F>(mut f: F)
where
    F: FnMut(u8, u8, u8, u16),
{
    for bus in 0u16..=255 {
        for dev in 0u8..32 {
            // SAFETY: bridge fn — valid for any (bus, dev, 0,
            // dword-aligned offset); absent devices return
            // 0xFFFF_FFFF.
            let id = unsafe { linuxkpi_pci_config_read32(bus as u8, dev, 0, 0x00) };
            if (id & 0xFFFF) as u16 == 0xFFFF {
                continue;
            }
            visit_func(bus as u8, dev, 0, &id, &mut f);
            // SAFETY: same as above.
            let header_dword = unsafe { linuxkpi_pci_config_read32(bus as u8, dev, 0, 0x0C) };
            let header_type = ((header_dword >> 16) & 0xFF) as u8;
            if header_type & 0x80 != 0 {
                for func in 1u8..8 {
                    // SAFETY: func in 0..8.
                    let id = unsafe { linuxkpi_pci_config_read32(bus as u8, dev, func, 0x00) };
                    if (id & 0xFFFF) as u16 == 0xFFFF {
                        continue;
                    }
                    visit_func(bus as u8, dev, func, &id, &mut f);
                }
            }
        }
    }
}

fn visit_func<F>(bus: u8, dev: u8, func: u8, id_dword: &u32, f: &mut F)
where
    F: FnMut(u8, u8, u8, u16),
{
    let vendor = (id_dword & 0xFFFF) as u16;
    if vendor != VIRTIO_VENDOR {
        return;
    }
    let pci_device_id = ((id_dword >> 16) & 0xFFFF) as u16;
    f(bus, dev, func, pci_device_id);
}

/// Translate a PCI device_id to a VIRTIO_ID_* constant per
/// virtio v1.2 § 4.1.2 ("PCI Device IDs"). Returns 0 for an
/// out-of-range device_id (caller then fails the match).
fn pci_to_virtio_id(pci_device_id: u16) -> u32 {
    match pci_device_id {
        0x1000..=0x103F => (pci_device_id - 0x1000) as u32,
        0x1040..=0x107F => (pci_device_id - 0x1040) as u32,
        _ => 0,
    }
}

/// Match a discovered virtio_id against a NULL-sentinel id_table
/// (sentinel = `device == 0 && vendor == 0`). VIRTIO_DEV_ANY_ID
/// in either field wildcards. Returns true on match.
fn match_id_table(id_table: *const virtio_device_id, virtio_id: u32) -> bool {
    if id_table.is_null() {
        return false;
    }
    // SAFETY: caller's contract — id_table is a NULL-sentinel
    // array. Bound the walk at 256 to avoid runaway loops.
    unsafe {
        for i in 0..256 {
            let entry = &*id_table.add(i);
            if entry.device == 0 && entry.vendor == 0 {
                return false; // sentinel
            }
            let device_match = entry.device == VIRTIO_DEV_ANY_ID || entry.device == virtio_id;
            if device_match {
                return true;
            }
        }
    }
    false
}

/// Count present virtio devices. Used by the self-test.
pub fn count_present() -> usize {
    let mut n = 0usize;
    walk_virtio_devices(|_, _, _, _| n += 1);
    n
}
