// SPDX-License-Identifier: BSD-2-Clause

//! `struct page` — the thin per-frame handle per ADR-0007, plus the
//! balloon_compaction page-list shims. Created during M1-2-5 Part B
//! sub-task 3 when balloon.c's #include <linux/balloon_compaction.h>
//! forced a struct page representation into the shim.
//!
//! Arsenal does not keep Linux's `mem_map` array; a `struct page` is
//! a small descriptor allocated alongside the physical frame it
//! represents. `_phys` backs page_to_pfn / page_address, `_refcount`
//! backs get_page / put_page, `_private` mirrors Linux's
//! page.private. Inherited C touches only `lru` (via the list
//! helpers); the rest is shim-internal. Layout mirrors `struct page`
//! in shim_c.h — keep the two in sync (ADR-0007's named FFI risk).
//!
//! balloon_page_alloc / _enqueue / _dequeue (from
//! <linux/balloon_compaction.h>) and the <linux/mm.h> page lifecycle
//! (page_address / alloc_pages / free_pages / put_page /
//! adjust_managed_page_count) ship as panic-on-call stubs here per
//! the M1-2-5 Part B iteration discipline (link-clean now, fail-loud
//! on the deferred path). Their real struct page-backed bodies —
//! allocate a frame via the frames bridge, set _phys + _refcount,
//! thread page->lru onto the dev_info list under pages_lock, derive
//! page_address from the HHDM offset — land at the M1-2-5-closing
//! commit (+ a self-test) alongside the virtqueue implementations
//! ARSENAL_VIRTIO_BALLOON_OK forces. page_to_pfn is a pure C inline
//! in <linux/mm.h>, not a Rust symbol.

use core::ffi::c_void;

use crate::list::list_head;
use crate::types::{c_int, c_long, c_uint, c_ulong};

/// Mirror of <linux/mm_types.h>'s thin-handle `struct page` (the
/// shim_c.h definition, ADR-0007). `#[repr(C)]`; field order and
/// types must match shim_c.h exactly.
#[repr(C)]
pub struct page {
    /// Driver list threading (balloon's `->pages`, etc.).
    pub lru: list_head,
    /// Backing physical address (4-KiB aligned). page_to_pfn =
    /// `_phys >> PAGE_SHIFT`; page_address = `hhdm + _phys`.
    pub _phys: c_ulong,
    /// Reference count; get_page / put_page adjust it.
    pub _refcount: c_int,
    /// Driver-opaque scratch (Linux's page.private).
    pub _private: *mut c_void,
}

/// `balloon_page_alloc` — allocate one balloon page. M1-2-5 Part B:
/// panic-on-call.
///
/// # Safety
/// Calling this during the M1-2-5 Part B iteration arc panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn balloon_page_alloc() -> *mut page {
    panic!("linuxkpi: balloon_page_alloc not yet implemented (lands at M1-2-5 close)")
}

/// `balloon_page_enqueue` — add `page` to `b_dev_info`'s locked page
/// list. M1-2-5 Part B: panic-on-call.
///
/// # Safety
/// Calling this during the M1-2-5 Part B iteration arc panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn balloon_page_enqueue(
    _b_dev_info: *mut c_void,
    _page: *mut page,
) {
    panic!("linuxkpi: balloon_page_enqueue not yet implemented (lands at M1-2-5 close)")
}

/// `balloon_page_dequeue` — remove and return the first page from
/// `b_dev_info`'s locked page list, or NULL if empty. M1-2-5 Part B:
/// panic-on-call.
///
/// # Safety
/// Calling this during the M1-2-5 Part B iteration arc panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn balloon_page_dequeue(_b_dev_info: *mut c_void) -> *mut page {
    panic!("linuxkpi: balloon_page_dequeue not yet implemented (lands at M1-2-5 close)")
}

// ---- <linux/mm.h> page lifecycle (ADR-0007 thin handle) ----

/// `page_address` — kernel virtual address of a page's contents
/// (HHDM + `_phys`). M1-2-5 Part B: panic-on-call; real body (the
/// HHDM-offset add) lands with the page lifecycle at M1-2-5 close.
///
/// # Safety
/// Calling this during the M1-2-5 Part B iteration arc panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn page_address(_page: *const page) -> *mut c_void {
    panic!("linuxkpi: page_address not yet implemented (lands at M1-2-5 close)")
}

/// `alloc_pages` — allocate 2^`order` contiguous pages over
/// frames::FRAMES; returns the head `struct page` or NULL. M1-2-5
/// Part B: panic-on-call.
///
/// # Safety
/// Calling this during the M1-2-5 Part B iteration arc panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn alloc_pages(_gfp: c_uint, _order: c_uint) -> *mut page {
    panic!("linuxkpi: alloc_pages not yet implemented (lands at M1-2-5 close)")
}

/// `free_pages` — free 2^`order` pages by kernel virtual address
/// (the Linux free_pages contract). M1-2-5 Part B: panic-on-call.
///
/// # Safety
/// Calling this during the M1-2-5 Part B iteration arc panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free_pages(_addr: c_ulong, _order: c_uint) {
    panic!("linuxkpi: free_pages not yet implemented (lands at M1-2-5 close)")
}

/// `put_page` — drop a reference; frees the frame + descriptor on
/// the last reference. M1-2-5 Part B: panic-on-call.
///
/// # Safety
/// Calling this during the M1-2-5 Part B iteration arc panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn put_page(_page: *mut page) {
    panic!("linuxkpi: put_page not yet implemented (lands at M1-2-5 close)")
}

/// `adjust_managed_page_count` — adjust the kernel's managed-page
/// accounting by `count` pages as balloon inflates / deflates.
/// M1-2-5 Part B: panic-on-call.
///
/// # Safety
/// Calling this during the M1-2-5 Part B iteration arc panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adjust_managed_page_count(_page: *mut page, _count: c_long) {
    panic!("linuxkpi: adjust_managed_page_count not yet implemented (lands at M1-2-5 close)")
}
