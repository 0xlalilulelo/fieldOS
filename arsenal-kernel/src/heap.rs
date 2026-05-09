// SPDX-License-Identifier: BSD-2-Clause
//
// Kernel heap backed by `linked_list_allocator` (Phil Opp's blog
// allocator). Free-list, O(N) allocation in the worst case but
// O(1) for the small/medium-block hot paths. Replaces the M0
// step 2 bump now that subsystems with churn — frame-allocator
// Vec growth, scheduler task structs in step 3B — start needing
// dealloc that returns memory.
//
// Initialized once from the same heap region the bump used (see
// main.rs init_heap). Tried talc 5.x first; the new
// Source + Binning + TalcLock split was too much API surface for
// step 3-3, and Phil Opp's lineage is the better fit alongside
// the existing limine + x86_64 + spin stack. talc revisits if
// fragmentation or perf bites.

use linked_list_allocator::LockedHeap;

#[global_allocator]
static HEAP: LockedHeap = LockedHeap::empty();

/// Hand the heap region `[base, base + size)` to the global allocator.
///
/// # Safety
/// `[base, base + size)` must be valid, exclusively owned, writable
/// memory for the kernel's lifetime. Must be called exactly once
/// before any allocation reaches the heap.
pub unsafe fn init(base: usize, size: usize) {
    // SAFETY: caller's contract — the region is exclusively ours.
    // LockedHeap::init lays out the free-list bookkeeping inside
    // the region itself.
    unsafe { HEAP.lock().init(base as *mut u8, size) };
}
