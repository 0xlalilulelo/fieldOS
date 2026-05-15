// SPDX-License-Identifier: BSD-2-Clause

//! Linux slab allocator shim — `kmalloc` / `kzalloc` / `kfree` /
//! `krealloc` routed to the kernel's global allocator
//! (`linked_list_allocator` per `arsenal-kernel/src/heap.rs`).
//!
//! GFP semantics: in Linux, `GFP_KERNEL` allocations may sleep
//! (the allocator can wait for the memory subsystem to reclaim
//! pages); `GFP_ATOMIC` allocations must not sleep (callable
//! from IRQ context). At our M1 cooperative-only scheduler the
//! global allocator is non-sleeping by construction, so the
//! distinction is informational at M1-2-1. The `flags` argument
//! is recorded but not yet enforced; M1-2-2 (PCI bridge / IRQ
//! adapter) introduces the `IrqGuard`-aware enforcement that
//! catches `GFP_KERNEL`-from-IRQ-context bugs at the call site.
//!
//! Allocation header: `kmalloc` returns a pointer to the user
//! payload; the underlying `alloc::alloc::alloc` needs the
//! original `Layout` for `dealloc`. We prepend a 16-byte header
//! recording the user-requested size so `kfree` / `krealloc`
//! reconstruct the layout. The 16-byte header alignment is also
//! a natural payload alignment that covers any plain old data
//! type Linux drivers allocate.

extern crate alloc;

use crate::types::{c_void, gfp_t, size_t};
use alloc::alloc::{Layout, alloc, dealloc, realloc};

/// May sleep; not callable from IRQ context.
pub const GFP_KERNEL: gfp_t = 0x0000_0001;
/// Must not sleep; callable from IRQ context.
pub const GFP_ATOMIC: gfp_t = 0x0000_0002;
/// Zero-fill on allocation. OR'd with `GFP_KERNEL` / `GFP_ATOMIC`.
pub const __GFP_ZERO: gfp_t = 0x0000_0004;

#[repr(C)]
struct Header {
    size: usize,
    _padding: usize,
}

const HEADER_SIZE: usize = core::mem::size_of::<Header>();
const PAYLOAD_ALIGN: usize = 16;

const _: () = assert!(HEADER_SIZE == 16, "Header must be 16 bytes for payload alignment");

/// Allocate `size` bytes; return a pointer to the user payload.
///
/// Returns NULL on failure or if `size == 0`.
///
/// # Safety
/// The returned pointer (when non-NULL) must be released via
/// `kfree` or `krealloc`; using `core::alloc::dealloc` directly
/// will trip the global allocator's free-list bookkeeping
/// because the user pointer is offset past the header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kmalloc(size: size_t, _flags: gfp_t) -> *mut c_void {
    if size == 0 {
        return core::ptr::null_mut();
    }
    let total = match size.checked_add(HEADER_SIZE) {
        Some(t) => t,
        None => return core::ptr::null_mut(),
    };
    let layout = match Layout::from_size_align(total, PAYLOAD_ALIGN) {
        Ok(l) => l,
        Err(_) => return core::ptr::null_mut(),
    };
    // SAFETY: layout has non-zero size + valid alignment. The global
    // allocator is initialized by arsenal-kernel before linuxkpi
    // self-test runs.
    let raw = unsafe { alloc(layout) };
    if raw.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: raw is valid for HEADER_SIZE bytes of header + size
    // bytes of payload. Header is repr(C) and 16-byte aligned.
    unsafe {
        core::ptr::write(raw as *mut Header, Header { size, _padding: 0 });
        raw.add(HEADER_SIZE) as *mut c_void
    }
}

/// Allocate `size` bytes, zero-fill, return a pointer to the user
/// payload. Equivalent to `kmalloc(size, flags | __GFP_ZERO)`.
///
/// # Safety
/// Same as `kmalloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kzalloc(size: size_t, flags: gfp_t) -> *mut c_void {
    // SAFETY: delegating to kmalloc.
    let p = unsafe { kmalloc(size, flags) };
    if !p.is_null() {
        // SAFETY: kmalloc returned a valid `size`-byte buffer at p.
        unsafe { core::ptr::write_bytes(p as *mut u8, 0, size) }
    }
    p
}

/// Resize the allocation `p` to `new_size`. If `p` is NULL, behaves
/// like `kmalloc(new_size, flags)`. If `new_size` is 0, behaves
/// like `kfree(p)` and returns NULL.
///
/// # Safety
/// `p` must be NULL or a pointer previously returned by `kmalloc` /
/// `kzalloc` / `krealloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krealloc(p: *mut c_void, new_size: size_t, flags: gfp_t) -> *mut c_void {
    if p.is_null() {
        return unsafe { kmalloc(new_size, flags) };
    }
    if new_size == 0 {
        unsafe { kfree(p) };
        return core::ptr::null_mut();
    }
    let new_total = match new_size.checked_add(HEADER_SIZE) {
        Some(t) => t,
        None => return core::ptr::null_mut(),
    };
    // SAFETY: caller's contract — p was returned by kmalloc et al.
    // The header sits HEADER_SIZE bytes before p.
    unsafe {
        let header_ptr = (p as *mut u8).sub(HEADER_SIZE) as *mut Header;
        let old_size = (*header_ptr).size;
        let old_total = old_size + HEADER_SIZE;
        let old_layout = Layout::from_size_align_unchecked(old_total, PAYLOAD_ALIGN);
        let new_raw = realloc(header_ptr as *mut u8, old_layout, new_total);
        if new_raw.is_null() {
            return core::ptr::null_mut();
        }
        core::ptr::write(new_raw as *mut Header, Header { size: new_size, _padding: 0 });
        new_raw.add(HEADER_SIZE) as *mut c_void
    }
}

/// Free an allocation returned by `kmalloc` / `kzalloc` /
/// `krealloc`. NULL is a no-op (matches Linux semantics).
///
/// # Safety
/// `p` must be NULL or a pointer returned by `kmalloc` /
/// `kzalloc` / `krealloc` and not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kfree(p: *const c_void) {
    if p.is_null() {
        return;
    }
    // SAFETY: caller's contract — p was returned by our allocator.
    // Recover the header and the original layout to release the
    // full backing region.
    unsafe {
        let header_ptr = (p as *const u8).sub(HEADER_SIZE) as *mut Header;
        let size = (*header_ptr).size;
        let total = size + HEADER_SIZE;
        let layout = Layout::from_size_align_unchecked(total, PAYLOAD_ALIGN);
        dealloc(header_ptr as *mut u8, layout);
    }
}
