// SPDX-License-Identifier: BSD-2-Clause

//! `struct page` — the thin per-frame handle per ADR-0007, plus the
//! page-lifecycle + balloon_compaction page-list shims. Created
//! during M1-2-5 Part B sub-task 3 (header phase) and given real
//! frame-allocator-backed bodies at the M1-2-5-closing commit.
//!
//! Arsenal does not keep Linux's `mem_map` array; a `struct page` is
//! a small descriptor `kmalloc`'d alongside the physical frame it
//! represents. `_phys` backs page_to_pfn / page_address, `_refcount`
//! backs put_page (the last drop frees the frame + the descriptor),
//! `_private` mirrors Linux's page.private. Inherited C touches only
//! `lru` via the list helpers; the rest is shim-internal. Layout
//! mirrors `struct page` in shim_c.h — keep the two in sync
//! (ADR-0007's named FFI risk).
//!
//! Only `order == 0` allocation is supported at M1: the frame
//! allocator hands out one 4-KiB frame at a time, and balloon's
//! inflate/deflate hot path uses order 0. `alloc_pages(order > 0)` +
//! the matching `free_pages(addr, order)` are reached only by
//! balloon's free-page-hint path (VIRTIO_BALLOON_F_FREE_PAGE_HINT),
//! which the M1 smoke device does not negotiate — they panic-on-call
//! so the unsupported path fails loudly rather than silently.
//!
//! `adjust_managed_page_count` is intentionally a no-op (not a
//! panic): Arsenal has no kernel-managed-page accounting at M1 and
//! balloon's call is informational. The honest M1 behavior is to do
//! nothing, documented here so the difference from the panic-on-call
//! stubs is deliberate.

use core::ffi::c_void;

use crate::list::{list_add_tail, list_del, list_head, INIT_LIST_HEAD};
use crate::locks::{spin_lock, spin_unlock, spinlock};
use crate::slab::{kfree, kmalloc, GFP_KERNEL};
use crate::types::{c_int, c_long, c_uint, c_ulong};

unsafe extern "C" {
    fn linuxkpi_frames_alloc_frame() -> u64;
    fn linuxkpi_frames_free_frame(phys: u64);
    fn linuxkpi_paging_hhdm_offset() -> u64;
}

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
    /// Reference count; put_page decrements, frees on zero.
    pub _refcount: c_int,
    /// Driver-opaque scratch (Linux's page.private).
    pub _private: *mut c_void,
}

/// Mirror of <linux/balloon_compaction.h>'s `struct balloon_dev_info`
/// (shim_c.h's BSD-2 reimpl: isolated_pages + pages_lock + pages, no
/// migratepage since CONFIG_BALLOON_COMPACTION is undefined). Layout
/// must match shim_c.h: a 16-byte spinlock storage (the opaque
/// `struct spinlock`) sits between `isolated_pages` and `pages`. The
/// in-storage `locks::spinlock` is much smaller than 16 bytes; the
/// trailing bytes are unused padding. We keep the 16-byte field as
/// `[u64; 2]` so it has 8-byte alignment matching the surrounding
/// fields.
#[repr(C)]
pub struct balloon_dev_info {
    pub isolated_pages: c_ulong,
    pub pages_lock: [u64; 2],
    pub pages: list_head,
}

/// `alloc_pages(gfp, order)` — allocate 2^order contiguous pages.
/// M1: only `order == 0` is supported (one frame from
/// frames::FRAMES); higher orders panic (free-page-hint path is
/// feature-gated off in the smoke build).
///
/// The descriptor is a small heap allocation (kmalloc); the frame
/// itself comes from the page-frame allocator via the bridge.
/// Returns NULL on frame-allocator or descriptor-allocator
/// exhaustion (matching Linux's NULL-on-failure contract).
///
/// # Safety
/// Returned page is owned by the caller; release via `put_page`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn alloc_pages(_gfp: c_uint, order: c_uint) -> *mut page {
    if order != 0 {
        panic!("linuxkpi: alloc_pages only supports order 0 at M1 (free-page-hint not negotiated)")
    }
    // SAFETY: bridge fn — returns a 4-KiB-aligned phys addr or 0.
    let phys = unsafe { linuxkpi_frames_alloc_frame() };
    if phys == 0 {
        return core::ptr::null_mut();
    }
    // SAFETY: kmalloc returns aligned writable storage of the
    // requested size, or null on exhaustion.
    let desc = unsafe { kmalloc(core::mem::size_of::<page>(), GFP_KERNEL) } as *mut page;
    if desc.is_null() {
        // SAFETY: phys came from alloc_frame; the free pairs it.
        unsafe { linuxkpi_frames_free_frame(phys) };
        return core::ptr::null_mut();
    }
    // SAFETY: desc is a fresh, properly-sized, non-null allocation.
    unsafe {
        (*desc)._phys = phys as c_ulong;
        (*desc)._refcount = 1;
        (*desc)._private = core::ptr::null_mut();
        // lru is a self-loop ("empty list") until enqueued.
        INIT_LIST_HEAD(&mut (*desc).lru);
    }
    desc
}

/// `free_pages(addr, order)` — free 2^order pages by virtual address
/// (Linux's free_pages contract). Reached only by balloon's
/// free-page-hint path; panic-on-call at M1 alongside alloc_pages
/// with order > 0.
///
/// # Safety
/// Calling this during M1 panics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free_pages(_addr: c_ulong, _order: c_uint) {
    panic!("linuxkpi: free_pages not yet implemented (free-page-hint path; closing commit)")
}

/// `put_page(page)` — drop a reference; on the last reference,
/// free the frame and the descriptor.
///
/// # Safety
/// `page` must be a valid pointer returned by `alloc_pages` /
/// `balloon_page_alloc`; the caller must not access it after the
/// last reference is dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn put_page(page: *mut page) {
    if page.is_null() {
        return;
    }
    // SAFETY: page is non-null per the check + caller's contract.
    let new_count = unsafe {
        (*page)._refcount -= 1;
        (*page)._refcount
    };
    if new_count > 0 {
        return;
    }
    if new_count < 0 {
        panic!("linuxkpi: put_page underflow — refcount went negative");
    }
    // Last reference: free the backing frame, then the descriptor.
    // SAFETY: _phys came from linuxkpi_frames_alloc_frame; the free
    // pairs it. The descriptor was kmalloc'd in alloc_pages.
    unsafe {
        linuxkpi_frames_free_frame((*page)._phys);
        kfree(page as *const c_void);
    }
}

/// `page_address(page)` — kernel virtual address of a page's
/// contents. HHDM + _phys, per ADR-0007.
///
/// # Safety
/// `page` must point to a valid `struct page` whose `_phys` is a
/// real allocated frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn page_address(page: *const page) -> *mut c_void {
    if page.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: bridge fn + caller's contract.
    let hhdm = unsafe { linuxkpi_paging_hhdm_offset() };
    // SAFETY: page is non-null.
    let phys = unsafe { (*page)._phys };
    (hhdm + phys) as *mut c_void
}

/// `adjust_managed_page_count(page, count)` — adjust the kernel's
/// managed-page accounting by `count` pages. Arsenal has no
/// kernel-managed-page accounting at M1; balloon's call is
/// informational, so this is intentionally a no-op (NOT a panic —
/// the call is in balloon's inflate/deflate hot path and a panic
/// would block ARSENAL_VIRTIO_BALLOON_OK; the "missing" accounting
/// is honest at M1).
///
/// # Safety
/// `page` may be any pointer; never dereferenced.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adjust_managed_page_count(_page: *mut page, _count: c_long) {
    // intentional no-op — see fn doc.
}

// =====================================================================
// balloon_compaction.h — page-list helpers over struct balloon_dev_info.
// =====================================================================

/// `balloon_page_alloc()` — allocate one balloon page. Equivalent to
/// `alloc_pages(GFP_HIGHUSER_MOVABLE, 0)` in Linux; at M1 the flags
/// are ignored and we just go through alloc_pages.
///
/// # Safety
/// Returned page is owned by the caller; release via put_page.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn balloon_page_alloc() -> *mut page {
    // SAFETY: alloc_pages's contract.
    unsafe { alloc_pages(0, 0) }
}

/// `balloon_page_enqueue(b_dev_info, page)` — take pages_lock, add
/// `page` at the tail of `b_dev_info->pages`, release the lock.
///
/// # Safety
/// `b_dev_info` and `page` must be valid non-null pointers; the
/// pages_lock must have been initialized via spin_lock_init (which
/// balloon's `balloon_devinfo_init` does at probe).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn balloon_page_enqueue(
    b_dev_info: *mut balloon_dev_info,
    page: *mut page,
) {
    if b_dev_info.is_null() || page.is_null() {
        return;
    }
    // SAFETY: pages_lock is the embedded 16-byte storage for a
    // `locks::spinlock` (which fits in 16 bytes per shim_c.h's
    // opaque sizing). The cast targets the same memory the
    // C-callable spin_lock_init / spin_lock / spin_unlock operate on.
    let lock = unsafe { &mut (*b_dev_info).pages_lock as *mut [u64; 2] as *mut spinlock };
    // SAFETY: lock is valid (initialized by balloon_devinfo_init);
    // list_add_tail expects non-null pointers, which is true here.
    unsafe {
        spin_lock(lock);
        list_add_tail(&mut (*page).lru, &mut (*b_dev_info).pages);
        spin_unlock(lock);
    }
}

/// `balloon_page_dequeue(b_dev_info)` — take pages_lock, pop the
/// first page from `b_dev_info->pages` (NULL if empty), release the
/// lock. The returned page's `lru` is re-initialized to a self-loop
/// (matching `list_del` semantics).
///
/// # Safety
/// `b_dev_info` must be a valid non-null pointer with an initialized
/// pages_lock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn balloon_page_dequeue(b_dev_info: *mut balloon_dev_info) -> *mut page {
    if b_dev_info.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: pages_lock storage holds an initialized locks::spinlock
    // (see balloon_page_enqueue safety note for the cast rationale).
    let lock = unsafe { &mut (*b_dev_info).pages_lock as *mut [u64; 2] as *mut spinlock };
    // SAFETY: pages_head is valid; list helpers handle empty list
    // (head.next == &head means empty in Linux's list convention).
    unsafe {
        spin_lock(lock);
        let pages_head = &mut (*b_dev_info).pages as *mut list_head;
        let first = (*pages_head).next;
        let result = if first.is_null() || first == pages_head {
            core::ptr::null_mut()
        } else {
            list_del(first);
            // `lru` is the first field of `struct page` (offset 0
            // per the shim_c.h definition + the Rust mirror above),
            // so the list_head pointer is also the page pointer.
            first as *mut page
        };
        spin_unlock(lock);
        result
    }
}
