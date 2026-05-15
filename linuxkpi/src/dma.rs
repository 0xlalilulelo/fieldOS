// SPDX-License-Identifier: BSD-2-Clause

//! Linux DMA-coherent allocator shim — `dma_alloc_coherent` /
//! `dma_free_coherent` over arsenal-kernel's frame allocator,
//! plus no-op `dma_map_single` / `dma_unmap_single` /
//! `dma_sync_*` for x86_64.
//!
//! x86_64 cache coherency: per Intel SDM Vol. 3A § 11.3 ("Methods
//! of Caching Available"), x86 processors snoop DMA traffic against
//! their caches when memory is marked WB (write-back) — the
//! default for kernel RAM. CPU writes become visible to devices
//! and device writes become visible to the CPU without explicit
//! flush/invalidate. Linux's `dma_sync_*` family exists for non-
//! coherent architectures (some ARM SoCs, some PowerPC); on x86_64
//! they reduce to no-ops. Same for `dma_map_single` / `unmap` —
//! the streaming-DMA bookkeeping has no work to do.
//!
//! IOMMU: x86_64 with an active IOMMU (Intel VT-d / AMD-Vi) maps
//! DMA addresses through the IOMMU page tables; absent an IOMMU,
//! DMA address == physical address. M1 has no IOMMU support; the
//! shim assumes the absent-IOMMU case and returns physical
//! addresses directly. IOMMU integration arrives at M2 or later
//! when amdgpu's DMA reach justifies it.
//!
//! `dma_alloc_coherent` returns a (CPU-virtual, DMA-physical) pair.
//! On our HHDM mapping the CPU-virtual is `phys + hhdm_offset()`;
//! the DMA-physical is the raw frame address. Inherited drivers
//! consume the virtual for `memcpy` / `writel` / etc. and the
//! physical for device-facing register writes (queue head pointers,
//! descriptor PRPs, etc.).

use crate::types::{c_int, c_void, dma_addr_t, gfp_t, size_t};

unsafe extern "C" {
    fn linuxkpi_paging_hhdm_offset() -> u64;
    fn linuxkpi_frames_alloc_frame() -> u64;
    fn linuxkpi_frames_free_frame(phys: u64);
}

const FRAME_SIZE: u64 = 4096;

/// DMA mapping direction — Linux <linux/dma-direction.h>. M1-2-2
/// retains the values for shim_c.h ABI but the no-op map fns
/// don't actually consume them.
pub const DMA_BIDIRECTIONAL: c_int = 0;
pub const DMA_TO_DEVICE: c_int = 1;
pub const DMA_FROM_DEVICE: c_int = 2;
pub const DMA_NONE: c_int = 3;

/// A `struct device *` placeholder. Linux drivers receive a
/// `device` pointer for DMA bookkeeping (IOMMU domain selection,
/// streaming-DMA accounting). At M1 we ignore it — the absent-
/// IOMMU + cache-coherent x86_64 path needs no per-device state.
#[repr(C)]
pub struct device {
    pub _opaque: [u8; 8],
}

/// Allocate `size` bytes of DMA-coherent memory. Writes the DMA-
/// physical address to `*dma_handle` and returns the CPU-virtual
/// pointer. NULL on failure or if `size > 4096` (M1 single-frame
/// limit; multi-page coherent allocations land when an inherited
/// driver demands them).
///
/// # Safety
/// `dev` is ignored at M1 but must be a valid pointer per Linux
/// convention. `dma_handle` must point to writable storage for the
/// returned `dma_addr_t`. The returned virtual pointer is page-
/// aligned + valid until matched `dma_free_coherent`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dma_alloc_coherent(
    _dev: *mut device,
    size: size_t,
    dma_handle: *mut dma_addr_t,
    _flags: gfp_t,
) -> *mut c_void {
    if dma_handle.is_null() || size == 0 || size as u64 > FRAME_SIZE {
        // M1 single-frame limit; multi-frame coherent allocations
        // need contiguous physical pages (the frame allocator
        // hands out singletons), which arrives when an inherited
        // driver actually needs > 4 KiB.
        return core::ptr::null_mut();
    }
    // SAFETY: bridge fn — alloc_frame returns a 4-KiB-aligned
    // physical address or 0 on exhaustion.
    let phys = unsafe { linuxkpi_frames_alloc_frame() };
    if phys == 0 {
        return core::ptr::null_mut();
    }
    // SAFETY: dma_handle is non-null per the check.
    unsafe {
        *dma_handle = phys;
    }
    // SAFETY: bridge fn — hhdm_offset is a constant after paging::init.
    let hhdm = unsafe { linuxkpi_paging_hhdm_offset() };
    (phys + hhdm) as *mut c_void
}

/// Free a region previously returned by `dma_alloc_coherent`.
///
/// # Safety
/// `cpu_addr` + `dma_handle` must be the pair returned by a prior
/// `dma_alloc_coherent` and not yet freed. `size` must match the
/// original allocation size (informational at M1; the frame
/// allocator releases the entire 4-KiB frame regardless).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dma_free_coherent(
    _dev: *mut device,
    _size: size_t,
    cpu_addr: *mut c_void,
    dma_handle: dma_addr_t,
) {
    if cpu_addr.is_null() {
        return;
    }
    // SAFETY: bridge fn — caller's contract guarantees dma_handle
    // is a frame-allocator-issued physical address.
    unsafe { linuxkpi_frames_free_frame(dma_handle) };
}

/// Map a virtually-contiguous CPU buffer for streaming DMA.
/// Returns the DMA-physical address. On x86_64 with no IOMMU this
/// is a translation of the CPU-virtual back to physical via the
/// HHDM offset — only valid for HHDM-mapped pointers. Heap-
/// allocated buffers from `kmalloc` live in HHDM-mapped pages so
/// this works; stack buffers do not.
///
/// # Safety
/// `cpu_addr` must point to `size` bytes of valid HHDM-mapped
/// memory; `dir` is the streaming direction (informational on
/// x86_64).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dma_map_single(
    _dev: *mut device,
    cpu_addr: *mut c_void,
    _size: size_t,
    _dir: c_int,
) -> dma_addr_t {
    if cpu_addr.is_null() {
        return 0;
    }
    // SAFETY: bridge fn — hhdm_offset is constant.
    let hhdm = unsafe { linuxkpi_paging_hhdm_offset() };
    (cpu_addr as u64).wrapping_sub(hhdm)
}

/// Unmap a streaming-DMA region. No-op on x86_64 with no IOMMU.
///
/// # Safety
/// Linux convention requires the args mirror `dma_map_single`'s
/// returned handle + size; the no-op shim ignores them.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dma_unmap_single(
    _dev: *mut device,
    _dma_handle: dma_addr_t,
    _size: size_t,
    _dir: c_int,
) {
    // intentionally empty — see module doc.
}

/// Sync a streaming-DMA region for device access. No-op on x86_64
/// (cache-coherent DMA, see module doc).
///
/// # Safety
/// As `dma_unmap_single`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dma_sync_single_for_device(
    _dev: *mut device,
    _dma_handle: dma_addr_t,
    _size: size_t,
    _dir: c_int,
) {
    // intentionally empty.
}

/// Sync a streaming-DMA region for CPU access. No-op on x86_64.
///
/// # Safety
/// As `dma_unmap_single`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dma_sync_single_for_cpu(
    _dev: *mut device,
    _dma_handle: dma_addr_t,
    _size: size_t,
    _dir: c_int,
) {
    // intentionally empty.
}

/// Set the DMA mask for `dev`. M1 has no IOMMU and the frame
/// allocator hands out 64-bit-addressable RAM; any mask succeeds.
/// Returns 0 (success).
///
/// # Safety
/// Linux convention is `dev` non-null; the no-op shim ignores it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dma_set_mask(_dev: *mut device, _mask: u64) -> c_int {
    0
}

/// Set the coherent DMA mask for `dev`. As `dma_set_mask`.
///
/// # Safety
/// As `dma_set_mask`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dma_set_coherent_mask(_dev: *mut device, _mask: u64) -> c_int {
    0
}
