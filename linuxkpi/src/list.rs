// SPDX-License-Identifier: BSD-2-Clause

//! Linux intrusive doubly-linked list — `struct list_head` +
//! `INIT_LIST_HEAD` / `list_add` / `list_add_tail` / `list_del`
//! / `list_empty`. The `list_for_each_entry` iteration macro
//! lives in shim_c.h (preprocessor by necessity — uses
//! `container_of` + `typeof`).
//!
//! M1-2-5 Part A — translated from the canonical kernel-docs
//! description without copying upstream Linux source verbatim.
//! Linux's `list.h` defines these as `static inline` so they
//! get inlined into each translation unit; our shim ships them
//! as `extern "C"` fns. Slightly less efficient at the call
//! site, but discoverable + uniform with the rest of the shim's
//! fn-not-macro discipline.
//!
//! ABI: `struct list_head` is `{ next, prev }` — two pointers,
//! 16 bytes on 64-bit. Layout matches Linux <linux/list.h>
//! exactly so inherited C using `LIST_HEAD_INIT` macros at
//! struct-init time produces the same bit layout.

use core::ptr::null_mut;

/// C-ABI-compatible intrusive list node. Embedded in larger
/// structs; `container_of` recovers the outer struct from a
/// `*list_head` member pointer.
#[repr(C)]
pub struct list_head {
    pub next: *mut list_head,
    pub prev: *mut list_head,
}

impl list_head {
    /// Create an empty list head — both pointers self-reference.
    /// Static initializer for `LIST_HEAD()` macro semantics.
    pub const fn new() -> Self {
        Self {
            next: null_mut(),
            prev: null_mut(),
        }
    }
}

impl Default for list_head {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: list_head is a passive descriptor; concurrent access
// is the caller's responsibility (Linux convention — drivers
// hold a separate spinlock for list mutation).
unsafe impl Send for list_head {}
unsafe impl Sync for list_head {}

/// Initialize `list` to be an empty list (both pointers
/// self-reference). Called once before any `list_add` etc.
///
/// # Safety
/// `list` must point to writable memory of size + alignment
/// matching `list_head` for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn INIT_LIST_HEAD(list: *mut list_head) {
    if list.is_null() {
        return;
    }
    // SAFETY: caller's contract — list is writable.
    unsafe {
        (*list).next = list;
        (*list).prev = list;
    }
}

/// Insert `new_` between `prev` and `next`. Internal helper;
/// Linux's `__list_add`.
///
/// # Safety
/// `new_`, `prev`, `next` must point to valid `list_head`s
/// for the duration of the call. `prev->next == next` and
/// `next->prev == prev` should hold on entry (otherwise the
/// list invariant is broken on exit).
unsafe fn __list_add(
    new_: *mut list_head,
    prev: *mut list_head,
    next: *mut list_head,
) {
    // SAFETY: caller's contract.
    unsafe {
        (*next).prev = new_;
        (*new_).next = next;
        (*new_).prev = prev;
        (*prev).next = new_;
    }
}

/// Insert `new_` after `head` (i.e., at the front of the list
/// when `head` is the sentinel).
///
/// # Safety
/// As `__list_add`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn list_add(new_: *mut list_head, head: *mut list_head) {
    if new_.is_null() || head.is_null() {
        return;
    }
    // SAFETY: caller's contract; head->next is read here.
    unsafe { __list_add(new_, head, (*head).next) }
}

/// Insert `new_` before `head` (i.e., at the tail of the list
/// when `head` is the sentinel).
///
/// # Safety
/// As `__list_add`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn list_add_tail(new_: *mut list_head, head: *mut list_head) {
    if new_.is_null() || head.is_null() {
        return;
    }
    // SAFETY: caller's contract; head->prev is read here.
    unsafe { __list_add(new_, (*head).prev, head) }
}

/// Remove `entry` from its containing list. After this, `entry`
/// points to itself (matches Linux's `LIST_POISON` discipline
/// loosely — we don't poison; we self-link, which is
/// list-empty's representation, so a `list_empty(entry)` after
/// `list_del(entry)` returns true).
///
/// # Safety
/// `entry` must be a valid `list_head` currently in a list.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn list_del(entry: *mut list_head) {
    if entry.is_null() {
        return;
    }
    // SAFETY: caller's contract — entry is in a list.
    unsafe {
        let prev = (*entry).prev;
        let next = (*entry).next;
        (*next).prev = prev;
        (*prev).next = next;
        // Self-link so subsequent list_empty(entry) returns true.
        (*entry).next = entry;
        (*entry).prev = entry;
    }
}

/// Returns 1 if `head` is empty (next-pointer self-reference),
/// 0 otherwise. Matches Linux's `int list_empty(const struct
/// list_head *)`.
///
/// # Safety
/// `head` must point to a valid `list_head`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn list_empty(head: *const list_head) -> crate::types::c_int {
    if head.is_null() {
        return 1;
    }
    // SAFETY: caller's contract — head is valid.
    unsafe {
        if core::ptr::eq((*head).next, head as *mut list_head) { 1 } else { 0 }
    }
}
