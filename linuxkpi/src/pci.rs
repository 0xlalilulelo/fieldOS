// SPDX-License-Identifier: BSD-2-Clause

//! Linux PCI bus adapter — `struct pci_driver` registration model
//! over arsenal-kernel's PCI primitives (consumed via the bridge
//! externs in `arsenal-kernel/src/linuxkpi_bridge.rs`). Inherited
//! drivers register a `struct pci_driver` with an `id_table`; the
//! shim walks every present PCI function and dispatches `.probe`
//! for each match.
//!
//! M1-2-2 surface: registration + matching + `.probe` dispatch +
//! `pci_resource_*` + `pci_iomap` + `pci_set_master` /
//! `pci_enable_device`. IRQ allocation (`pci_alloc_irq_vectors`,
//! `request_irq`) lands at the next sub-block of M1-2-2; DMA
//! coherent (`dma_alloc_coherent`) lives in `dma.rs`.
//!
//! Match semantics: the driver's `id_table` is a NULL-sentinel
//! array of `pci_device_id`. PCI_ANY_ID (0xFFFFFFFF) in vendor /
//! device / subvendor / subdevice / class fields means "match
//! anything"; the class match is gated by `class_mask`.

extern crate alloc;

use alloc::vec::Vec;
use core::ptr::NonNull;
use spin::Mutex;

use crate::types::{c_char, c_int, c_uint, c_ulong, c_void};

unsafe extern "C" {
    fn linuxkpi_pci_config_read32(bus: u8, dev: u8, func: u8, offset: u8) -> u32;
    fn linuxkpi_pci_config_write32(bus: u8, dev: u8, func: u8, offset: u8, val: u32);
    fn linuxkpi_pci_bar_address(bus: u8, dev: u8, func: u8, bar: u8) -> u64;
    fn linuxkpi_paging_map_mmio(phys: u64, len: u64);
    fn linuxkpi_paging_hhdm_offset() -> u64;
    fn linuxkpi_pci_msix_info(bus: u8, dev: u8, func: u8, out: *mut MsixInfoRaw);
}

/// Mirror of `arsenal_kernel::linuxkpi_bridge::LinuxkpiMsixInfo`.
/// Populated by the bridge's `linuxkpi_pci_msix_info` extern.
#[repr(C)]
struct MsixInfoRaw {
    present: u32,
    cap_offset: u32,
    table_size: u32,
    table_bar: u32,
    table_offset: u32,
}

// =====================================================================
// Driver-facing types — match Linux's <linux/pci.h> layouts.
// =====================================================================

/// PCI_ANY_ID — wildcard for vendor / device / subvendor / subdevice
/// fields in `pci_device_id`. Class match is gated by class_mask.
pub const PCI_ANY_ID: u32 = 0xFFFF_FFFF;

/// `struct pci_device_id` from Linux <linux/mod_devicetable.h>.
/// Drivers declare an `id_table` as a NULL-sentinel array of these
/// (sentinel = `vendor == 0`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct pci_device_id {
    pub vendor: u32,
    pub device: u32,
    pub subvendor: u32,
    pub subdevice: u32,
    pub class: u32,
    pub class_mask: u32,
    pub driver_data: c_ulong,
}

/// `struct pci_dev` from Linux <linux/pci.h>. The Linux struct is
/// huge (~2 KiB on 6.12); we expose only the fields drivers
/// actually touch at probe time. Cached BAR addresses + lengths
/// are populated by `dispatch_probe` so drivers can read
/// `pci_resource_start` / `pci_resource_len` without re-hitting
/// config space.
#[repr(C)]
pub struct pci_dev {
    pub vendor: u16,
    pub device: u16,
    pub subsystem_vendor: u16,
    pub subsystem_device: u16,
    pub class: u32,
    pub bus_number: u8,
    pub devfn: u8,
    /// Cached BAR physical addresses. Zero for absent / I/O BARs.
    /// Indexed 0..=5; 64-bit BARs occupy two consecutive slots
    /// (the upper-half slot is zero by convention).
    pub bar_addr: [u64; 6],
    /// Cached BAR lengths in bytes. 0 for absent BARs. M1-2-2
    /// fills these via the BAR-sizing dance (write 0xFFFFFFFF,
    /// read back, restore) at probe-dispatch time.
    pub bar_len: [u64; 6],
    /// Per-driver state pointer — `pci_set_drvdata` /
    /// `pci_get_drvdata` manipulate this. The shim leaves it for
    /// the driver; we never inspect it.
    pub driver_data: *mut c_void,
    /// First slot allocated by `pci_alloc_irq_vectors` for this
    /// device, or -1 if no IRQ vectors have been allocated. The
    /// allocation is contiguous: the device owns slots
    /// `[msix_first_slot, msix_first_slot + msix_vector_count)`.
    /// `pci_irq_vector(dev, idx)` returns `msix_first_slot + idx`.
    pub msix_first_slot: c_int,
    /// Number of vectors allocated by `pci_alloc_irq_vectors`. 0
    /// when no allocation has been made.
    pub msix_vector_count: c_int,
}

// SAFETY: pci_dev is a passive descriptor; concurrent access is
// the caller's responsibility (Linux requires drivers to serialize
// pci_dev access themselves).
unsafe impl Send for pci_dev {}
unsafe impl Sync for pci_dev {}

/// `struct pci_driver` from Linux <linux/pci.h>. Drivers declare a
/// static instance + register at module init. The function pointers
/// are nullable per Linux convention; missing `.probe` is a no-op
/// driver (registers but never claims devices).
#[repr(C)]
pub struct pci_driver {
    pub name: *const c_char,
    pub id_table: *const pci_device_id,
    pub probe: Option<unsafe extern "C" fn(*mut pci_dev, *const pci_device_id) -> c_int>,
    pub remove: Option<unsafe extern "C" fn(*mut pci_dev)>,
}

// SAFETY: pci_driver instances are typically `static` in Linux
// drivers; the function pointers + name + id_table are all
// 'static. Concurrent registration is serialized by REGISTRY's
// mutex.
unsafe impl Send for pci_driver {}
unsafe impl Sync for pci_driver {}

// =====================================================================
// Registry — drivers register here at module init; pci_register_driver
// walks the registry against every present PCI function.
// =====================================================================

struct RegisteredDriver {
    drv: NonNull<pci_driver>,
}

// SAFETY: NonNull<pci_driver> is shared across cores via REGISTRY's
// mutex; pci_driver itself is Send + Sync (see above).
unsafe impl Send for RegisteredDriver {}

static REGISTRY: Mutex<Vec<RegisteredDriver>> = Mutex::new(Vec::new());

/// Register `drv` and dispatch its `.probe` against every
/// currently-present PCI function whose vendor/device matches the
/// driver's `id_table`. Returns 0 on success.
///
/// Linux's `pci_register_driver` is reentrant in the kernel build
/// configuration that supports module loading; M1's shim is single-
/// threaded at boot time so the mutex is sufficient.
///
/// # Safety
/// `drv` must point to a valid `pci_driver` whose `name`,
/// `id_table`, and function pointers remain valid for the
/// lifetime of the registration (typically `'static`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pci_register_driver(drv: *mut pci_driver) -> c_int {
    let drv = match NonNull::new(drv) {
        Some(d) => d,
        None => return -1,
    };

    // SAFETY: drv is non-null per the check above; caller's contract
    // ensures id_table points at a valid NULL-sentinel array (vendor
    // == 0 sentinel) and probe / remove are valid fn pointers.
    let id_table = unsafe { (*drv.as_ptr()).id_table };
    let probe = unsafe { (*drv.as_ptr()).probe };

    pci_walk(|bus, dev, func, vendor, device, class, subsys_vendor, subsys_device| {
        if let Some(matched_id) =
            match_id_table(id_table, vendor, device, subsys_vendor, subsys_device, class)
            && let Some(probe_fn) = probe
        {
            let mut pdev = build_pci_dev(
                bus, dev, func, vendor, device, subsys_vendor, subsys_device, class,
            );
            // SAFETY: probe_fn is a valid extern "C" fn per the
            // driver's declaration; pdev lives on this stack frame
            // for the duration of the call; matched_id points into
            // the driver's static id_table.
            unsafe {
                let _ = probe_fn(&mut pdev as *mut pci_dev, matched_id);
            }
        }
    });

    REGISTRY.lock().push(RegisteredDriver { drv });
    0
}

/// Unregister `drv`. M1-2-2 commit A: removes from the registry
/// without calling `.remove` (no driver currently exits, and the
/// remove path needs the bound `pci_dev` table that lands with the
/// IRQ bridge).
///
/// # Safety
/// `drv` must have been previously registered via
/// `pci_register_driver` and must remain valid through the
/// unregister call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pci_unregister_driver(drv: *mut pci_driver) {
    let Some(target) = NonNull::new(drv) else { return; };
    let mut reg = REGISTRY.lock();
    reg.retain(|entry| entry.drv.as_ptr() != target.as_ptr());
}

// =====================================================================
// Resource + iomap + bus-master / enable-device.
// =====================================================================

/// Return the cached BAR physical address for `bar` of `dev`.
/// Matches Linux's `pci_resource_start`.
///
/// # Safety
/// `dev` must point to a `pci_dev` populated by the shim's
/// `dispatch_probe`; `bar` must be < 6.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pci_resource_start(dev: *const pci_dev, bar: c_int) -> u64 {
    if dev.is_null() || !(0..6).contains(&bar) {
        return 0;
    }
    // SAFETY: caller's contract — dev is valid + populated.
    unsafe { (*dev).bar_addr[bar as usize] }
}

/// Return the cached BAR length for `bar` of `dev`. Matches
/// Linux's `pci_resource_len`.
///
/// # Safety
/// As `pci_resource_start`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pci_resource_len(dev: *const pci_dev, bar: c_int) -> u64 {
    if dev.is_null() || !(0..6).contains(&bar) {
        return 0;
    }
    // SAFETY: caller's contract.
    unsafe { (*dev).bar_len[bar as usize] }
}

/// Map `bar` of `dev` for MMIO access; returns the HHDM-virtual
/// address (CPU-reachable pointer). `max_len` caps the mapping
/// length; pass 0 to map the whole BAR. Returns NULL if the BAR
/// is absent / I/O-typed.
///
/// # Safety
/// `dev` must point to a `pci_dev` populated by the shim's
/// `dispatch_probe`; `bar` must be < 6. The returned pointer is
/// valid for the lifetime of the kernel (we don't unmap MMIO).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pci_iomap(dev: *const pci_dev, bar: c_int, max_len: u64) -> *mut c_void {
    if dev.is_null() || !(0..6).contains(&bar) {
        return core::ptr::null_mut();
    }
    // SAFETY: caller's contract.
    let phys = unsafe { (*dev).bar_addr[bar as usize] };
    let len = unsafe { (*dev).bar_len[bar as usize] };
    if phys == 0 || len == 0 {
        return core::ptr::null_mut();
    }
    let map_len = if max_len == 0 || max_len > len { len } else { max_len };
    // SAFETY: bridge fns are link-time-resolved; map_mmio is
    // idempotent on overlap.
    unsafe {
        linuxkpi_paging_map_mmio(phys, map_len);
        let hhdm = linuxkpi_paging_hhdm_offset();
        (phys + hhdm) as *mut c_void
    }
}

/// Counterpart to `pci_iomap`. M1-2-2 commit A: no-op (we don't
/// unmap MMIO at M1; reclaim arrives with M2 or later).
///
/// # Safety
/// Linux convention requires the args mirror `pci_iomap`'s output;
/// the no-op shim ignores them.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pci_iounmap(_dev: *const pci_dev, _addr: *mut c_void) {
    // intentionally empty
}

/// Set the bus-master enable bit (bit 2) of the command register
/// (config offset 0x04) for `dev`. Required before any DMA.
///
/// # Safety
/// `dev` must point to a populated `pci_dev`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pci_set_master(dev: *mut pci_dev) {
    if dev.is_null() {
        return;
    }
    // SAFETY: caller's contract — dev is valid + populated.
    unsafe {
        let bus = (*dev).bus_number;
        let devnum = (*dev).devfn >> 3;
        let func = (*dev).devfn & 0x07;
        let cur = linuxkpi_pci_config_read32(bus, devnum, func, 0x04);
        linuxkpi_pci_config_write32(bus, devnum, func, 0x04, cur | (1 << 2));
    }
}

/// Set memory-space + I/O-space enable bits of the command
/// register. Required before any BAR access through MMIO or PIO.
///
/// # Safety
/// `dev` must point to a populated `pci_dev`. Returns 0 on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pci_enable_device(dev: *mut pci_dev) -> c_int {
    if dev.is_null() {
        return -1;
    }
    // SAFETY: caller's contract.
    unsafe {
        let bus = (*dev).bus_number;
        let devnum = (*dev).devfn >> 3;
        let func = (*dev).devfn & 0x07;
        let cur = linuxkpi_pci_config_read32(bus, devnum, func, 0x04);
        // Memory-space (bit 1) + I/O-space (bit 0). Inherited
        // drivers expect both — Linux's pci_enable_device toggles
        // them together for symmetry across BAR types.
        linuxkpi_pci_config_write32(bus, devnum, func, 0x04, cur | 0x03);
        0
    }
}

// =====================================================================
// Internal helpers.
// =====================================================================

/// Walk every (bus, dev, func) and invoke `f` for each present
/// function. Mirrors arsenal-kernel's `pci::scan` shape but is
/// linuxkpi-local because cross-crate FFI doesn't easily carry
/// closures. Costs: ~256 * 32 = 8192 config reads per walk; on
/// QEMU TCG this is microseconds.
fn pci_walk<F>(mut f: F)
where
    F: FnMut(u8, u8, u8, u16, u16, u32, u16, u16),
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
            visit_func(bus as u8, dev, 0, &mut f);
            // Multi-function bit (bit 7 of header type at offset
            // 0x0E within dword 0x0C).
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
                    visit_func(bus as u8, dev, func, &mut f);
                }
            }
        }
    }
}

fn visit_func<F>(bus: u8, dev: u8, func: u8, f: &mut F)
where
    F: FnMut(u8, u8, u8, u16, u16, u32, u16, u16),
{
    // SAFETY: caller verified the function is present.
    let id = unsafe { linuxkpi_pci_config_read32(bus, dev, func, 0x00) };
    let class = unsafe { linuxkpi_pci_config_read32(bus, dev, func, 0x08) };
    let subsys = unsafe { linuxkpi_pci_config_read32(bus, dev, func, 0x2C) };
    f(
        bus,
        dev,
        func,
        (id & 0xFFFF) as u16,
        ((id >> 16) & 0xFFFF) as u16,
        class,
        (subsys & 0xFFFF) as u16,
        ((subsys >> 16) & 0xFFFF) as u16,
    );
}

/// Match a discovered (vendor, device, subsys_vendor, subsys_device,
/// class) against a NULL-sentinel `id_table` array. Returns the
/// matching `pci_device_id` pointer or `None`.
///
/// PCI_ANY_ID matches any value for vendor / device / subvendor /
/// subdevice fields; class matching is gated by `class_mask` (zero
/// mask = don't care).
fn match_id_table(
    id_table: *const pci_device_id,
    vendor: u16,
    device: u16,
    subsys_vendor: u16,
    subsys_device: u16,
    class: u32,
) -> Option<*const pci_device_id> {
    if id_table.is_null() {
        return None;
    }
    // SAFETY: caller's contract — id_table is a NULL-sentinel
    // array (vendor == 0 sentinel). Bound the walk at 1024 to
    // avoid runaway loops on malformed tables.
    unsafe {
        for i in 0..1024 {
            let entry = id_table.add(i);
            let e = &*entry;
            if e.vendor == 0 && e.device == 0 {
                return None; // sentinel
            }
            if (e.vendor == PCI_ANY_ID || e.vendor == vendor as u32)
                && (e.device == PCI_ANY_ID || e.device == device as u32)
                && (e.subvendor == PCI_ANY_ID || e.subvendor == subsys_vendor as u32)
                && (e.subdevice == PCI_ANY_ID || e.subdevice == subsys_device as u32)
                && (e.class_mask == 0 || (class & e.class_mask) == (e.class & e.class_mask))
            {
                return Some(entry);
            }
        }
    }
    None
}

/// Construct a `pci_dev` for `(bus, dev, func)`. Cached BAR
/// addresses + lengths are read here so probe-time
/// `pci_resource_start` / `pci_iomap` calls work without
/// re-hitting config space. The BAR-sizing dance (write
/// 0xFFFFFFFF, read back length, restore) is per the PCI Local
/// Bus Spec rev 3.0 § 6.2.5.1.
#[allow(clippy::too_many_arguments)]
fn build_pci_dev(
    bus: u8,
    dev: u8,
    func: u8,
    vendor: u16,
    device: u16,
    subsys_vendor: u16,
    subsys_device: u16,
    class: u32,
) -> pci_dev {
    let mut bar_addr = [0u64; 6];
    let mut bar_len = [0u64; 6];
    let mut bar = 0u8;
    while bar < 6 {
        // SAFETY: bridge fns; bar in 0..6.
        let lo_orig = unsafe { linuxkpi_pci_config_read32(bus, dev, func, 0x10 + bar * 4) };
        if lo_orig & 0x01 != 0 {
            // I/O BAR — M1 drivers don't use these. Skip.
            bar += 1;
            continue;
        }
        let is_64bit = (lo_orig & 0x06) == 0x04;
        // Sizing dance.
        // SAFETY: the original BAR value is restored at the end
        // of the dance; the all-1s probe is the standard PCI BAR-
        // sizing protocol.
        unsafe {
            linuxkpi_pci_config_write32(bus, dev, func, 0x10 + bar * 4, 0xFFFF_FFFF);
            let lo_size = linuxkpi_pci_config_read32(bus, dev, func, 0x10 + bar * 4) & 0xFFFF_FFF0;
            linuxkpi_pci_config_write32(bus, dev, func, 0x10 + bar * 4, lo_orig);
            if is_64bit && bar < 5 {
                let hi_orig = linuxkpi_pci_config_read32(bus, dev, func, 0x10 + (bar + 1) * 4);
                linuxkpi_pci_config_write32(bus, dev, func, 0x10 + (bar + 1) * 4, 0xFFFF_FFFF);
                let hi_size = linuxkpi_pci_config_read32(bus, dev, func, 0x10 + (bar + 1) * 4);
                linuxkpi_pci_config_write32(bus, dev, func, 0x10 + (bar + 1) * 4, hi_orig);
                let size_mask = ((hi_size as u64) << 32) | (lo_size as u64);
                bar_len[bar as usize] = (!size_mask).wrapping_add(1);
                bar_addr[bar as usize] = linuxkpi_pci_bar_address(bus, dev, func, bar);
                // Skip the upper half BAR slot.
                bar += 2;
                continue;
            }
            bar_len[bar as usize] = (!(lo_size as u64) & 0xFFFF_FFFF).wrapping_add(1);
            bar_addr[bar as usize] = linuxkpi_pci_bar_address(bus, dev, func, bar);
        }
        bar += 1;
    }

    pci_dev {
        vendor,
        device,
        subsystem_vendor: subsys_vendor,
        subsystem_device: subsys_device,
        class,
        bus_number: bus,
        devfn: (dev << 3) | (func & 0x07),
        bar_addr,
        bar_len,
        driver_data: core::ptr::null_mut(),
        msix_first_slot: -1,
        msix_vector_count: 0,
    }
}

// =====================================================================
// MSI-X allocation — pci_alloc_irq_vectors / pci_irq_vector /
// pci_free_irq_vectors. Programs the device's MSI-X table inline;
// IRQ handler dispatch is the slot pool in linuxkpi/src/irq.rs.
// =====================================================================

/// Linux PCI IRQ allocation flag bits — Linux <linux/pci.h>.
pub const PCI_IRQ_INTX: c_int = 1 << 0;
pub const PCI_IRQ_MSI: c_int = 1 << 1;
pub const PCI_IRQ_MSIX: c_int = 1 << 2;
pub const PCI_IRQ_ALL_TYPES: c_int = PCI_IRQ_INTX | PCI_IRQ_MSI | PCI_IRQ_MSIX;

/// Allocate `min_vecs..=max_vecs` IRQ vectors for `dev`. M1's
/// shim supports `PCI_IRQ_MSIX` only; legacy MSI / INTx return
/// -ENOSYS-style failure even if requested.
///
/// On success, returns the number of vectors allocated (in
/// `min_vecs..=max_vecs`); the device's MSI-X table is programmed
/// + enabled, and the per-slot dispatchers are wired through the
///   shim's IDT pool. On failure returns negative.
///
/// # Safety
/// `dev` must point to a populated `pci_dev` (i.e., one delivered
/// to a driver's `.probe` callback by `pci_register_driver`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pci_alloc_irq_vectors(
    dev: *mut pci_dev,
    min_vecs: c_uint,
    max_vecs: c_uint,
    flags: c_uint,
) -> c_int {
    if dev.is_null() {
        return -1;
    }
    if (flags as c_int) & PCI_IRQ_MSIX == 0 {
        // Legacy MSI / INTx is post-M1; fail loudly.
        return -1;
    }
    if min_vecs == 0 || max_vecs < min_vecs {
        return -1;
    }

    // SAFETY: caller's contract — dev is populated.
    let (bus, devnum, func) = unsafe {
        (
            (*dev).bus_number,
            (*dev).devfn >> 3,
            (*dev).devfn & 0x07,
        )
    };

    // Read the MSI-X capability via the bridge.
    let mut info = MsixInfoRaw {
        present: 0,
        cap_offset: 0,
        table_size: 0,
        table_bar: 0,
        table_offset: 0,
    };
    // SAFETY: bridge fn — out points to writable stack storage.
    unsafe { linuxkpi_pci_msix_info(bus, devnum, func, &mut info as *mut MsixInfoRaw) };
    if info.present == 0 {
        return -1;
    }

    // Clamp the vector count to: requested max, MSI-X table size,
    // and the dispatcher pool's free capacity (handled by alloc_slots).
    let table_size = info.table_size as c_uint;
    let want = max_vecs.min(table_size);
    if want < min_vecs {
        return -1;
    }
    let count = want as usize;

    let first_slot = match crate::irq::alloc_slots(count) {
        Some(s) => s,
        None => return -1,
    };

    // Map the MSI-X table BAR. The BAR address is the BAR base;
    // table_offset is the byte offset within the BAR. Per the
    // PCIe spec the table is `count * 16` bytes long; we map
    // exactly that.
    let table_phys_base = unsafe {
        linuxkpi_pci_bar_address(bus, devnum, func, info.table_bar as u8)
    };
    if table_phys_base == 0 {
        return -1;
    }
    let table_phys = table_phys_base + info.table_offset as u64;
    let table_len = (count as u64) * 16;
    // SAFETY: bridge fns — map_mmio is idempotent on overlap.
    let table_virt = unsafe {
        linuxkpi_paging_map_mmio(table_phys, table_len);
        let hhdm = linuxkpi_paging_hhdm_offset();
        (table_phys + hhdm) as *mut u32
    };

    // Program each table entry. Entry layout (16 bytes):
    //   off 0..3   Message Address Low  (LAPIC fixed-delivery)
    //   off 4..7   Message Address High (0 for 32-bit address)
    //   off 8..11  Message Data         (vector + delivery mode)
    //   off 12..15 Vector Control       (bit 0 = mask)
    //
    // BSP destination: APIC ID 0. M1 single-CPU MSI-X delivery;
    // multi-CPU IRQ steering arrives at M2 when the scheduler's
    // per-CPU runqueue justifies it.
    for i in 0..count {
        let entry_idx = i * 4;
        let idt_vec = crate::irq::slot_idt_vector(first_slot + i) as u32;
        // SAFETY: table_virt + (i*4)..(i*4 + 4) is in bounds for
        // count entries; map_mmio covered exactly that range.
        unsafe {
            core::ptr::write_volatile(table_virt.add(entry_idx), 0xFEE0_0000); // addr low: APIC ID 0
            core::ptr::write_volatile(table_virt.add(entry_idx + 1), 0);       // addr high
            core::ptr::write_volatile(table_virt.add(entry_idx + 2), idt_vec); // data: vector
            core::ptr::write_volatile(table_virt.add(entry_idx + 3), 0);       // vector control: unmasked
        }
    }

    // Enable MSI-X in the capability's Message Control register.
    // The cap dword: [0..7] cap ID, [8..15] next, [16..31] msg_ctrl.
    // Set bit 31 (msg_ctrl[15] = MSI-X Enable); clear bit 30
    // (msg_ctrl[14] = Function Mask) so individual entries are
    // governed by their own vector_control mask bits.
    // SAFETY: bridge fns — capability offset is dword-aligned per
    // PCI spec.
    unsafe {
        let cur = linuxkpi_pci_config_read32(bus, devnum, func, info.cap_offset as u8);
        let new = (cur | (1u32 << 31)) & !(1u32 << 30);
        linuxkpi_pci_config_write32(bus, devnum, func, info.cap_offset as u8, new);
    }

    // SAFETY: caller's contract — dev is writable.
    unsafe {
        (*dev).msix_first_slot = first_slot as c_int;
        (*dev).msix_vector_count = count as c_int;
    }

    count as c_int
}

/// Return the IRQ number for the `idx`-th vector of `dev`'s
/// MSI-X allocation. Equivalent in our shim to the slot index;
/// inherited drivers pass it to `request_irq`.
///
/// Returns negative if `dev` has no allocation or `idx` is out
/// of range.
///
/// # Safety
/// `dev` must point to a populated `pci_dev` whose
/// `pci_alloc_irq_vectors` call returned > 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pci_irq_vector(dev: *const pci_dev, idx: c_uint) -> c_int {
    if dev.is_null() {
        return -1;
    }
    // SAFETY: caller's contract.
    let (first, count) = unsafe { ((*dev).msix_first_slot, (*dev).msix_vector_count) };
    if first < 0 || (idx as c_int) >= count {
        return -1;
    }
    first + idx as c_int
}

/// Release all IRQ vectors previously allocated via
/// `pci_alloc_irq_vectors` for `dev`. Clears the corresponding
/// shim-side slot entries (drivers that already called
/// `request_irq` are silently unregistered) but does not reclaim
/// the slots in the pool — slot-pool reclaim arrives if real-
/// hardware driver churn exhausts the 16-slot capacity, which
/// won't happen at M1.
///
/// Also disables MSI-X in the device's Message Control register.
///
/// # Safety
/// `dev` must point to a populated `pci_dev`. Calling on a `dev`
/// that never had `pci_alloc_irq_vectors` called is a no-op.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pci_free_irq_vectors(dev: *mut pci_dev) {
    if dev.is_null() {
        return;
    }
    // SAFETY: caller's contract.
    let (first, count) = unsafe { ((*dev).msix_first_slot, (*dev).msix_vector_count) };
    if first < 0 || count <= 0 {
        return;
    }
    // Clear shim-side handler slots so any subsequent IRQ delivery
    // dispatches as a no-op (then sends EOI as usual).
    for i in 0..count {
        // SAFETY: free_irq is a pure shim fn; first + i is in
        // 0..SLOT_COUNT by construction (alloc_slots guaranteed it).
        unsafe { let _ = crate::irq::free_irq((first + i) as c_uint, core::ptr::null_mut()); }
    }
    // Disable MSI-X. Read the cap dword, clear bit 31 (Enable),
    // set bit 30 (Function Mask) for paranoia, write back.
    // SAFETY: caller's contract — dev is populated; cap_offset
    // was recorded by pci_alloc_irq_vectors via the bridge.
    unsafe {
        let bus = (*dev).bus_number;
        let devnum = (*dev).devfn >> 3;
        let func = (*dev).devfn & 0x07;
        // Re-read the MSI-X cap to find the cap_offset; cheaper
        // than caching it in pci_dev for one disable call.
        let mut info = MsixInfoRaw {
            present: 0,
            cap_offset: 0,
            table_size: 0,
            table_bar: 0,
            table_offset: 0,
        };
        linuxkpi_pci_msix_info(bus, devnum, func, &mut info as *mut MsixInfoRaw);
        if info.present == 1 {
            let cur = linuxkpi_pci_config_read32(bus, devnum, func, info.cap_offset as u8);
            let new = (cur & !(1u32 << 31)) | (1u32 << 30);
            linuxkpi_pci_config_write32(bus, devnum, func, info.cap_offset as u8, new);
        }
        (*dev).msix_first_slot = -1;
        (*dev).msix_vector_count = 0;
    }
}

// =====================================================================
// Self-test support — used by lib.rs::self_test.
// =====================================================================

#[cfg(any())]
const _UNUSED: () = (); // placeholder for cfg-gating future test support

/// Count discovered PCI functions. Used by self_test to validate
/// the walk machinery sees the same device count `pci::scan`
/// printed earlier in boot.
pub fn count_present() -> usize {
    let mut n = 0usize;
    pci_walk(|_, _, _, _, _, _, _, _| n += 1);
    n
}
