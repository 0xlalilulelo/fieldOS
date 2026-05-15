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

use crate::{frames, paging, pci};
use x86_64::PhysAddr;
use x86_64::structures::paging::{PhysFrame, Size4KiB};

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
