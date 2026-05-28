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

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::ptr::NonNull;
use spin::Mutex;

use crate::types::{c_char, c_int, c_uint, c_ulong, c_void};

unsafe extern "C" {
    fn linuxkpi_pci_config_read32(bus: u8, dev: u8, func: u8, offset: u8) -> u32;
    fn linuxkpi_virtio_resolve(
        bus: u8,
        dev: u8,
        func: u8,
        want_device_id: u16,
        out: *mut VirtioDevRaw,
    );

    // Virtqueue + transport bridge (M1-2-5 closing-commit work).
    // `linuxkpi_virtqueue_free` + `linuxkpi_virtqueue_info` are
    // declared now but consumed in a later round (del_vqs wiring);
    // keeping them in the same bridge-extern block as the rest keeps
    // the bridge surface adjacent.
    fn linuxkpi_virtqueue_new(size: u16) -> *mut c_void;
    #[allow(dead_code)]
    fn linuxkpi_virtqueue_free(handle: *mut c_void);
    #[allow(dead_code)]
    fn linuxkpi_virtqueue_info(handle: *const c_void, out: *mut LinuxkpiVqInfo);
    fn linuxkpi_virtqueue_push_descriptor(
        handle: *mut c_void,
        addr: u64,
        len: u32,
        flags: u16,
    ) -> i32;
    fn linuxkpi_virtqueue_push_chain(
        handle: *mut c_void,
        parts: *const LinuxkpiVqChainPart,
        nparts: u32,
    ) -> i32;
    fn linuxkpi_virtqueue_pop_used(
        handle: *mut c_void,
        out_id: *mut u32,
        out_len: *mut u32,
    ) -> bool;
    fn linuxkpi_virtio_read_queue_size(common_cfg: *mut u8, idx: u16) -> u16;
    fn linuxkpi_virtio_activate_queue(
        common_cfg: *mut u8,
        notify_base: *mut u8,
        notify_off_multiplier: u32,
        queue_idx: u16,
        queue_handle: *const c_void,
    ) -> *mut c_void;
    fn linuxkpi_virtio_set_driver_ok(common_cfg: *mut u8);
    fn linuxkpi_virtio_notify(notify_ptr: *mut c_void, queue_idx: u16);
    fn linuxkpi_virtio_init_transport(common_cfg: *mut u8, driver_features: u64) -> u64;
    fn linuxkpi_virtio_reset_device(common_cfg: *mut u8);
}

/// Mirror of arsenal-kernel's `LinuxkpiVqInfo` (linuxkpi_bridge.rs).
/// Read by the round-21 del_vqs path (paired with the matching
/// allow(dead_code) on the linuxkpi_virtqueue_info extern above).
#[allow(dead_code)]
#[repr(C)]
struct LinuxkpiVqInfo {
    size: u16,
    _pad: [u8; 6],
    desc_phys: u64,
    avail_phys: u64,
    used_phys: u64,
}

/// Mirror of arsenal-kernel's `LinuxkpiVqChainPart`.
#[repr(C)]
struct LinuxkpiVqChainPart {
    addr: u64,
    len: u32,
    flags: u16,
    _pad: u16,
}

/// Mirror of <linux/virtio_config.h>'s `struct virtqueue_info` (the
/// per-vq setup descriptor balloon passes to virtio_find_vqs).
#[repr(C)]
struct VirtqueueInfoC {
    name: *const c_char,
    callback: *const c_void, // vq_callback_t* — stored opaquely; not invoked at M1
    ctx: bool,
}

/// VIRTQ descriptor flags (subset). Linux's <linux/virtio_ring.h>
/// + virtio v1.2 §2.6.13.1. F_WRITE = "device writes" (inbuf).
const VIRTQ_DESC_F_WRITE: u16 = 2;

/// Per-virtqueue shim state. The shim's `struct virtqueue.priv_`
/// stores a `Box::into_raw`'d pointer to one of these. Each entry
/// tracks the underlying arsenal-kernel Virtqueue handle, the
/// per-descriptor token balloon passed to `virtqueue_add_*` (so
/// `virtqueue_get_buf` can return it), the notify pointer activate_
/// queue handed back, and the queue index.
struct ShimVirtqueueState {
    bridge_handle: *mut c_void,
    tokens: Box<[*mut c_void]>,
    notify_ptr: *mut c_void,
    queue_idx: u16,
    size: u16,
}

/// Largest power-of-two ≤ `n` (capped at our 128 max). 0 if n is 0.
fn pow2_at_most(n: u16) -> u16 {
    if n == 0 {
        return 0;
    }
    let capped = n.min(128);
    1u16 << (15 - capped.leading_zeros() as u16)
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
    /// PCI device number (transport address). Named `pci_dev`, not
    /// `dev`, so it does not collide with the embedded `dev`
    /// (`struct device`) below that inherited drivers reach via
    /// `&vdev->dev`. Matches shim_c.h.
    pub pci_dev: u8,
    pub func: u8,
    pub _pad: u8,
    pub common_cfg: *mut u8,
    pub notify_base: *mut u8,
    pub notify_off_multiplier: u32,
    pub isr: *mut u8,
    pub device_cfg: *mut u8,
    /// virtio_config_ops vtable. `register_virtio_driver` installs
    /// the bus-side `CONFIG_OPS` table on every matched device
    /// before validate / probe (so balloon's validate-time null-check
    /// of `vdev->config->get` resolves). Stays NULL on directly
    /// stack-constructed vdevs (e.g., the self-test bit-op vdev).
    /// Layout-matches shim_c.h's `config`.
    pub config: *const c_void,
    /// Linux's embedded `struct device` (8-byte opaque in shim_c.h;
    /// the DMA section's `struct device`). balloon takes `&vdev->dev`
    /// for dev_* logging + PM, all no-ops at M1.
    pub dev: [u8; 8],
    /// Negotiated feature bits. Populated by the bus-side lifecycle
    /// from init_transport; virtio_has_feature reads it,
    /// virtio_clear_bit / __virtio_clear_bit clear bits in it (the
    /// validate-time bit drops). Layout-matches shim_c.h's `features`.
    pub features: u64,
}

// SAFETY: virtio_device is a passive descriptor; concurrent
// access is the driver's responsibility (Linux convention).
unsafe impl Send for virtio_device {}
unsafe impl Sync for virtio_device {}

/// `struct device_driver` — Linux's base driver struct. balloon
/// (and the Linux virtio_driver shape) put the driver name here,
/// reached as `.driver.name`. Only `name` is mirrored; the rest of
/// Linux's device_driver is bus/PM machinery the shim doesn't model.
#[repr(C)]
pub struct device_driver {
    pub name: *const c_char,
}

/// `struct virtio_driver` — Linux shape (matches shim_c.h). `name`
/// moved into the embedded `driver` (device_driver); feature_table /
/// feature_table_size / validate / config_changed added for
/// balloon's initializer. Field order + types mirror shim_c.h
/// exactly (the registration code below reads id_table + probe).
#[repr(C)]
pub struct virtio_driver {
    pub driver: device_driver,
    pub id_table: *const virtio_device_id,
    pub feature_table: *const c_uint,
    pub feature_table_size: c_uint,
    pub validate: Option<unsafe extern "C" fn(*mut virtio_device) -> c_int>,
    pub probe: Option<unsafe extern "C" fn(*mut virtio_device) -> c_int>,
    pub remove: Option<unsafe extern "C" fn(*mut virtio_device)>,
    pub config_changed: Option<unsafe extern "C" fn(*mut virtio_device)>,
}

unsafe impl Send for virtio_driver {}
unsafe impl Sync for virtio_driver {}

/// `struct virtio_config_ops` — Linux shape (matches shim_c.h:339).
/// Trimmed to what balloon dereferences: ->get (which validate
/// null-checks) and ->del_vqs (remove teardown). Future inherited
/// drivers extend this; the full Linux vtable is ~15 ops.
#[repr(C)]
pub struct virtio_config_ops {
    pub get: Option<unsafe extern "C" fn(*mut virtio_device, c_uint, *mut c_void, c_uint)>,
    pub del_vqs: Option<unsafe extern "C" fn(*mut virtio_device)>,
}

// SAFETY: virtio_config_ops is a static vtable of plain fn pointers.
unsafe impl Sync for virtio_config_ops {}

/// `get` slot — balloon's validate checks this is non-NULL but never
/// calls it during init. A real call would mean a driver reaching for
/// device-config data that the shim doesn't yet route; fail-loud per
/// the ADR-0005 § 6 deferred-path discipline.
unsafe extern "C" fn config_ops_get(
    _vdev: *mut virtio_device,
    _offset: c_uint,
    _buf: *mut c_void,
    _len: c_uint,
) {
    panic!("linuxkpi: virtio_config_ops.get not yet implemented (M1-2-5+ when a driver actually calls it)")
}

/// `del_vqs` slot — tears down the per-device virtqueue table on
/// driver remove. At M1 each inherited driver is initialized once and
/// is never removed (no module unload), so this is a no-op and the
/// virtqueue allocations leak by design (one balloon, bounded). The
/// successor with real teardown lands when an inherited driver
/// actually exits — beyond M1's scope.
unsafe extern "C" fn config_ops_del_vqs(_vdev: *mut virtio_device) {
    // No-op at M1 — see fn doc.
}

/// The single bus-side config-ops table the shim hands every
/// registered driver. balloon's validate dereferences vdev->config
/// (so it must be non-NULL by the time validate runs); register_-
/// virtio_driver installs this on every matched device before
/// validate / probe.
static CONFIG_OPS: virtio_config_ops = virtio_config_ops {
    get: Some(config_ops_get),
    del_vqs: Some(config_ops_del_vqs),
};

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
    let validate = unsafe { (*drv.as_ptr()).validate };
    let feature_table = unsafe { (*drv.as_ptr()).feature_table };
    let feature_table_size = unsafe { (*drv.as_ptr()).feature_table_size };

    // Fold the driver's feature_table (array of bit numbers) into a
    // u64 mask. Linux convention: drivers advertise *supported*
    // bits; the bus AND-intersects with what the device offers.
    // VIRTIO_F_VERSION_1 (bit 32) is mandatory for v1.0 devices and
    // is added unconditionally if the table doesn't already include
    // it (Linux does the same in drivers/virtio/virtio.c). Bits >=
    // 64 are silently skipped — Arsenal stores the negotiated set
    // in a u64; the upper-half feature space (e.g.,
    // VIRTIO_F_ACCESS_PLATFORM) is out of M1's scope.
    let mut driver_features: u64 = 1u64 << 32; // VIRTIO_F_VERSION_1
    if !feature_table.is_null() && feature_table_size > 0 {
        // SAFETY: caller's contract — feature_table points to
        // feature_table_size readable c_uint entries.
        unsafe {
            for i in 0..feature_table_size as usize {
                let fbit = *feature_table.add(i);
                if fbit < 64 {
                    driver_features |= 1u64 << fbit;
                }
            }
        }
    }

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
        let common_cfg = raw.common_cfg as *mut u8;
        // Drive the v1.2 § 3.1.1 init dance with bus-side feature
        // intersection. negotiated = device-offered AND
        // driver-supported, written back as the device's
        // driver_features and stored on vdev for validate / probe
        // to read via virtio_has_feature.
        // SAFETY: common_cfg came from linuxkpi_virtio_resolve, a
        // mapped MMIO region.
        let negotiated = unsafe {
            linuxkpi_virtio_init_transport(common_cfg, driver_features)
        };
        let mut vdev = virtio_device {
            id_device: virtio_id,
            id_vendor: VIRTIO_VENDOR as u32,
            priv_data: core::ptr::null_mut(),
            bus,
            pci_dev: dev,
            func,
            _pad: 0,
            common_cfg,
            notify_base: raw.notify_base as *mut u8,
            notify_off_multiplier: raw.notify_off_multiplier,
            isr: raw.isr as *mut u8,
            device_cfg: raw.device_cfg as *mut u8,
            config: &CONFIG_OPS as *const virtio_config_ops as *const c_void,
            dev: [0u8; 8],
            features: negotiated,
        };
        // validate() runs after feature negotiation but before
        // probe(). It may drop more bits via virtio_clear_bit /
        // __virtio_clear_bit; a non-zero return is a refusal —
        // reset the device and continue to the next match.
        if let Some(validate_fn) = validate {
            // SAFETY: validate_fn comes from the registered
            // driver; vdev lives on this stack frame.
            let rc = unsafe { validate_fn(&mut vdev as *mut virtio_device) };
            if rc != 0 {
                // SAFETY: common_cfg is the live MMIO region.
                unsafe { linuxkpi_virtio_reset_device(common_cfg) };
                return;
            }
        }
        // SAFETY: probe_fn is a valid extern "C" fn per the
        // driver's declaration; vdev lives on this stack frame
        // for the duration of the call.
        let rc = unsafe { probe_fn(&mut vdev as *mut virtio_device) };
        if rc < 0 {
            // Probe declined (Linux convention: negative = did not
            // bind). Return the device to RESET so a later driver
            // (or virtio_blk::smoke / virtio_net::smoke) can
            // re-initialize it from a clean state.
            // SAFETY: common_cfg is the live MMIO region.
            unsafe { linuxkpi_virtio_reset_device(common_cfg) };
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

/// `struct virtqueue`. Mirrors shim_c.h: balloon reads `vdev` (the
/// owning device) and `num_free` (available descriptors); `priv`
/// holds shim-internal vring state the M1-2-5-closing impl fills.
/// Inherited drivers only hold `*mut virtqueue`; the shim allocates
/// and populates instances at find-vqs time.
#[repr(C)]
pub struct virtqueue {
    pub vdev: *mut virtio_device,
    pub num_free: c_uint,
    pub priv_: *mut c_void,
}

/// `virtio_find_vqs` — discover and configure `nvqs` virtqueues for
/// `vdev` per the `vqs_info` descriptors, storing the results in
/// `vqs`. Linux 6.12 shape (replaced the older names-array find_vqs).
///
/// For each requested queue with a non-null `vqs_info[i].name`,
/// allocates a Virtqueue via the bridge (capped at the largest
/// power-of-two ≤ device max, ≤ 128), activates it on the device
/// (writes queue_select / size / ring physical addresses / enable),
/// boxes a shim `struct virtqueue` carrying the bridge handle in
/// `priv_`, and stores its pointer in `vqs[i]`. Per Linux semantics
/// NULL-named entries are skipped (vqs[i] = NULL) — balloon uses
/// this to leave optional vqs unallocated.
///
/// **Does NOT call set_driver_ok** — balloon's probe finalizes the
/// device via `virtio_device_ready` (the bridge for set_driver_ok)
/// after configuring the rest of its state. The device must already
/// be in DRIVER + FEATURES_OK status (init_transport done by the
/// shim's bus-side lifecycle, M1-2-5 next round).
///
/// # Safety
/// `vdev` is a live `virtio_device` whose transport pointers
/// (common_cfg / notify_base / notify_off_multiplier) resolve to
/// mapped MMIO. `vqs` points to `nvqs` writable `*mut virtqueue`
/// slots. `vqs_info` is an array of `nvqs` `struct virtqueue_info`.
/// `desc` (irq_affinity) is ignored at M1.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtio_find_vqs(
    vdev: *mut virtio_device,
    nvqs: c_uint,
    vqs: *mut *mut virtqueue,
    vqs_info: *mut c_void,
    _desc: *mut c_void,
) -> c_int {
    if vdev.is_null() || vqs.is_null() || vqs_info.is_null() || nvqs == 0 {
        return -1;
    }
    // SAFETY: caller guarantees vdev is live.
    let (common_cfg, notify_base, multiplier) = unsafe {
        (
            (*vdev).common_cfg,
            (*vdev).notify_base,
            (*vdev).notify_off_multiplier,
        )
    };
    let info_arr = vqs_info as *const VirtqueueInfoC;

    for i in 0..nvqs {
        // SAFETY: vqs_info[i] is in bounds per the caller's contract.
        let info = unsafe { &*info_arr.add(i as usize) };
        // Linux convention: NULL name → skip this slot (balloon uses
        // it to leave optional vqs unallocated).
        if info.name.is_null() {
            // SAFETY: vqs[i] is in bounds per the caller.
            unsafe {
                *vqs.add(i as usize) = core::ptr::null_mut();
            }
            continue;
        }
        // SAFETY: bridge fn — common_cfg is live MMIO.
        let max = unsafe { linuxkpi_virtio_read_queue_size(common_cfg, i as u16) };
        let size = pow2_at_most(max);
        if size == 0 {
            return -1;
        }
        // SAFETY: bridge fn.
        let handle = unsafe { linuxkpi_virtqueue_new(size) };
        if handle.is_null() {
            return -1;
        }
        // SAFETY: bridge fn — writes COMMON_CFG queue_* + returns
        // the notify-doorbell pointer.
        let notify_ptr = unsafe {
            linuxkpi_virtio_activate_queue(
                common_cfg,
                notify_base,
                multiplier,
                i as u16,
                handle,
            )
        };
        let tokens: Vec<*mut c_void> = vec![core::ptr::null_mut(); size as usize];
        let state = Box::new(ShimVirtqueueState {
            bridge_handle: handle,
            tokens: tokens.into_boxed_slice(),
            notify_ptr,
            queue_idx: i as u16,
            size,
        });
        let state_ptr = Box::into_raw(state);

        let vq = Box::new(virtqueue {
            vdev,
            num_free: size as c_uint,
            priv_: state_ptr as *mut c_void,
        });
        // SAFETY: vqs[i] is in bounds; the Box leak is paired with
        // the closing commit's del_vqs.
        unsafe {
            *vqs.add(i as usize) = Box::into_raw(vq);
        }
    }
    0
}

/// `virtqueue_get_vring_size` — number of descriptor slots in `vq`'s
/// vring. balloon reads it to size its free-page-reporting batches.
///
/// # Safety
/// `vq` must be a live virtqueue returned by virtio_find_vqs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtqueue_get_vring_size(vq: *const virtqueue) -> c_uint {
    if vq.is_null() {
        return 0;
    }
    // SAFETY: vq is non-null per the check + caller's contract; priv_
    // points to a ShimVirtqueueState set up in virtio_find_vqs.
    unsafe {
        let state = (*vq).priv_ as *const ShimVirtqueueState;
        if state.is_null() {
            return 0;
        }
        (*state).size as c_uint
    }
}

/// `__virtio_clear_bit` — clear driver-side feature bit `fbit` on
/// `vdev`. Drops the bit from `vdev.features`; the device-side
/// state remains FEATURES_OK with the original negotiation, but
/// the driver agrees not to use this feature (Linux semantics —
/// the validate path drops bits the driver/platform can't honor).
///
/// # Safety
/// `vdev` must be a valid `*mut virtio_device`. `fbit` must be in
/// [0, 64) (Arsenal stores the negotiated features in a u64).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __virtio_clear_bit(vdev: *mut virtio_device, fbit: c_uint) {
    if vdev.is_null() || fbit >= 64 {
        return;
    }
    // SAFETY: caller's contract; non-atomic — only the owning driver
    // touches features (Linux convention).
    unsafe {
        (*vdev).features &= !(1u64 << fbit);
    }
}

/// Shared body for virtqueue_add_outbuf / _inbuf. `sg_flags` is 0
/// for outbuf (host reads) and `VIRTQ_DESC_F_WRITE` for inbuf (host
/// writes). Reads `num` scatterlist entries' (dma_address,
/// dma_length), pushes a single descriptor (num=1) or a chain
/// (num=2..=8) via the bridge, records `data` as the token at the
/// head descriptor index, and decrements `num_free` accordingly.
///
/// Returns 0 on success, a negative errno-ish on failure.
///
/// # Safety
/// `vq` is a live shim virtqueue from virtio_find_vqs; `sg` points
/// to `num` scatterlist entries; `data` is the token balloon will
/// reclaim through virtqueue_get_buf.
unsafe fn vq_add(
    vq: *mut virtqueue,
    sg: *const scatterlist,
    num: c_uint,
    data: *mut c_void,
    sg_flags: u16,
) -> c_int {
    if vq.is_null() || sg.is_null() || num == 0 || num > 8 {
        return -1;
    }
    // SAFETY: vq is non-null + caller's contract.
    let state = unsafe { (*vq).priv_ as *mut ShimVirtqueueState };
    if state.is_null() {
        return -1;
    }
    // SAFETY: bridge call routes to arsenal-kernel's push_descriptor /
    // push_chain; sg array is read-only.
    let head_idx = unsafe {
        if num == 1 {
            let entry = &*sg;
            linuxkpi_virtqueue_push_descriptor(
                (*state).bridge_handle,
                entry.dma_address,
                entry.dma_length,
                sg_flags,
            )
        } else {
            let mut parts: [LinuxkpiVqChainPart; 8] = core::array::from_fn(|_| {
                LinuxkpiVqChainPart {
                    addr: 0,
                    len: 0,
                    flags: 0,
                    _pad: 0,
                }
            });
            let sg_slice = core::slice::from_raw_parts(sg, num as usize);
            for (part, e) in parts.iter_mut().zip(sg_slice.iter()) {
                *part = LinuxkpiVqChainPart {
                    addr: e.dma_address,
                    len: e.dma_length,
                    flags: sg_flags,
                    _pad: 0,
                };
            }
            linuxkpi_virtqueue_push_chain(
                (*state).bridge_handle,
                parts.as_ptr(),
                num,
            )
        }
    };
    if head_idx < 0 {
        return -1;
    }
    // SAFETY: head_idx is in [0, size); tokens is sized to .size.
    unsafe {
        (*state).tokens[head_idx as usize] = data;
        (*vq).num_free = (*vq).num_free.saturating_sub(num);
    }
    0
}

/// `virtqueue_add_outbuf` — submit one or more outbound (host-read)
/// scatterlist entries on `vq`. `data` is the token returned later
/// via virtqueue_get_buf.
///
/// # Safety
/// See `vq_add`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtqueue_add_outbuf(
    vq: *mut virtqueue,
    sg: *const c_void,
    num: c_uint,
    data: *mut c_void,
    _gfp: u32,
) -> c_int {
    // SAFETY: forwarded.
    unsafe { vq_add(vq, sg as *const scatterlist, num, data, 0) }
}

/// `virtqueue_add_inbuf` — submit one or more inbound (host-write)
/// scatterlist entries on `vq`. Flags each descriptor with F_WRITE.
///
/// # Safety
/// See `vq_add`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtqueue_add_inbuf(
    vq: *mut virtqueue,
    sg: *const c_void,
    num: c_uint,
    data: *mut c_void,
    _gfp: u32,
) -> c_int {
    // SAFETY: forwarded.
    unsafe { vq_add(vq, sg as *const scatterlist, num, data, VIRTQ_DESC_F_WRITE) }
}

/// `virtqueue_kick` — notify the device that buffers are available.
/// Writes the queue index to the queue's notify-doorbell pointer.
///
/// # Safety
/// `vq` is a live shim virtqueue from virtio_find_vqs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtqueue_kick(vq: *mut virtqueue) -> c_int {
    if vq.is_null() {
        return -1;
    }
    // SAFETY: caller's contract; priv_ set by virtio_find_vqs.
    unsafe {
        let state = (*vq).priv_ as *const ShimVirtqueueState;
        if state.is_null() {
            return -1;
        }
        linuxkpi_virtio_notify((*state).notify_ptr, (*state).queue_idx);
    }
    1
}

/// `virtqueue_get_buf` — drain one completed buffer from `vq`.
/// Returns the original `data` token (NULL if the used ring is
/// empty) and writes the device-reported bytes-used to `*len` on
/// success. Restores `num_free` by 1 (chain reclaim is handled
/// inside arsenal-kernel's pop_used, which walks F_NEXT — but we
/// only know the head index, not the chain length, so the
/// num_free accounting here is approximate at M1 and will be
/// tightened when arsenal-kernel's pop_used reports the chain
/// length back through the bridge).
///
/// # Safety
/// `vq` is a live shim virtqueue from virtio_find_vqs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtqueue_get_buf(
    vq: *mut virtqueue,
    len: *mut c_uint,
) -> *mut c_void {
    if vq.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: caller's contract.
    unsafe {
        let state = (*vq).priv_ as *mut ShimVirtqueueState;
        if state.is_null() {
            return core::ptr::null_mut();
        }
        let mut out_id: u32 = 0;
        let mut out_len: u32 = 0;
        if !linuxkpi_virtqueue_pop_used((*state).bridge_handle, &mut out_id, &mut out_len) {
            return core::ptr::null_mut();
        }
        if !len.is_null() {
            *len = out_len as c_uint;
        }
        let token = (*state).tokens[out_id as usize];
        (*state).tokens[out_id as usize] = core::ptr::null_mut();
        // See fn doc — chain length not reported back; bump by 1.
        (*vq).num_free = (*vq).num_free.saturating_add(1);
        token
    }
}

/// Mirror of <linux/scatterlist.h>'s `struct scatterlist`. Layout +
/// field types match shim_c.h exactly (the C side and inherited
/// drivers see this shape). The shim's simplified DMA model uses
/// `dma_address` as the physical address the virtqueue descriptor
/// will carry; `page_link` / `offset` are unused at M1 (Linux uses
/// page_link to encode struct page + flags, but Arsenal's DMA is
/// HHDM-identity so the conversion happens in sg_init_one directly).
#[repr(C)]
pub struct scatterlist {
    pub page_link: c_ulong,
    pub offset: c_uint,
    pub length: c_uint,
    pub dma_address: u64,
    pub dma_length: c_uint,
}

unsafe extern "C" {
    fn linuxkpi_paging_hhdm_offset() -> u64;
}

/// `sg_init_one` (<linux/scatterlist.h>) — initialize `sg` as a
/// single-entry scatterlist pointing at `buf` for `buflen` bytes.
///
/// M1 model: `buf` is a kernel virtual address in the HHDM mapping
/// (everything the shim allocates — frames, kmalloc heap, page
/// descriptors — lives there). The physical address the virtqueue
/// descriptor needs is `buf - hhdm_offset()`, which we compute and
/// stash into `dma_address` directly; the eventual virtqueue_add_*
/// reads `dma_address` + `dma_length` from each sg entry. No
/// separate `dma_map_sg` step is needed (Arsenal's DMA is
/// cache-coherent + identity-mapped on x86_64).
///
/// # Safety
/// `sg` must point to a writable `struct scatterlist`; `buf` should
/// be a kernel virtual address in the HHDM mapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sg_init_one(
    sg: *mut scatterlist,
    buf: *const c_void,
    buflen: c_uint,
) {
    if sg.is_null() {
        return;
    }
    // SAFETY: bridge fn — read-only.
    let hhdm = unsafe { linuxkpi_paging_hhdm_offset() };
    let virt = buf as u64;
    let phys = virt.wrapping_sub(hhdm);
    // SAFETY: sg is non-null per the check; the writes are within
    // the struct's bounds (no aliasing beyond what the caller already
    // owns).
    unsafe {
        (*sg).page_link = 0;
        (*sg).offset = 0;
        (*sg).length = buflen;
        (*sg).dma_address = phys;
        (*sg).dma_length = buflen;
    }
}

// =====================================================================
// virtio_config.h helpers — panic-on-call stubs landed during
// M1-2-5 Part B sub-task 3's iteration arc. Real implementations
// land in the M1-2-5-closing commit alongside balloon.c being
// added to the build.rs manifest end-to-end. Per the established
// shim discipline: declared symbols fail loudly at runtime rather
// than silently returning bad values.
// =====================================================================

/// `virtio_has_feature` — test whether the negotiated feature set
/// for `vdev` has bit `fbit` set. Reads `vdev.features`, which the
/// bus-side lifecycle (init_transport, M1-2-5 closing-commit round
/// 21b) populates with the bits the device offered AND the driver
/// claimed. False for any `fbit >= 64` (Arsenal's storage is u64).
///
/// # Safety
/// `vdev` must be a valid `*const virtio_device`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtio_has_feature(
    vdev: *const virtio_device,
    fbit: c_uint,
) -> bool {
    if vdev.is_null() || fbit >= 64 {
        return false;
    }
    // SAFETY: caller's contract.
    unsafe { (*vdev).features & (1u64 << fbit) != 0 }
}

/// `virtio_device_ready` — set the DRIVER_OK status bit on `vdev`,
/// completing the v1.2 § 3.1.1 init dance. Forwards to the bridge's
/// `linuxkpi_virtio_set_driver_ok`, which writes ACK | DRIVER |
/// FEATURES_OK | DRIVER_OK to CC_DEVICE_STATUS.
///
/// # Safety
/// `vdev` must be a valid `*mut virtio_device` whose `common_cfg`
/// points at a mapped COMMON_CFG region and whose feature
/// negotiation has completed (init_transport already ran).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtio_device_ready(vdev: *mut virtio_device) {
    if vdev.is_null() {
        return;
    }
    // SAFETY: caller's contract.
    unsafe { linuxkpi_virtio_set_driver_ok((*vdev).common_cfg) };
}

/// `virtio_reset_device` — clear DEVICE_STATUS to 0, returning the
/// device to RESET per v1.2 § 2.1.2. Forwards to the bridge's
/// `linuxkpi_virtio_reset_device`, which writes 0 and bounded-waits
/// for the device to ack. Subsequent re-init must go through
/// init_transport again.
///
/// # Safety
/// `vdev` must be a valid `*mut virtio_device` whose `common_cfg`
/// is a mapped COMMON_CFG region.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtio_reset_device(vdev: *mut virtio_device) {
    if vdev.is_null() {
        return;
    }
    // SAFETY: caller's contract.
    unsafe { linuxkpi_virtio_reset_device((*vdev).common_cfg) };
}

/// `virtio_clear_bit` — clear feature bit `fbit` on `vdev`'s
/// negotiated-features storage. Alias for `__virtio_clear_bit` at
/// M1 (no separate driver-features register; validate clears bits
/// the driver agrees not to use, and the change is shim-local).
///
/// # Safety
/// `vdev` must be a valid `*mut virtio_device`. `fbit` must be in
/// [0, 64).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn virtio_clear_bit(vdev: *mut virtio_device, fbit: c_uint) {
    // SAFETY: forwarded — __virtio_clear_bit has the same contract.
    unsafe { __virtio_clear_bit(vdev, fbit) }
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
