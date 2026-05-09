// SPDX-License-Identifier: BSD-2-Clause
//
// Bump allocator backing the kernel's GlobalAlloc. No free path —
// allocations live for the kernel's lifetime. Replace with a real
// allocator (linked-list or buddy) when subsystems with churn — the
// scheduler, virtio queues, network sockets — start landing in M0
// step 3+. The bump shape is deliberate: smallest thing that lets
// `core::alloc::GlobalAlloc` exist so subsequent steps can rely on it.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};

pub struct BumpAllocator {
    next: AtomicUsize,
    end: AtomicUsize,
}

impl BumpAllocator {
    pub const fn new() -> Self {
        Self {
            next: AtomicUsize::new(0),
            end: AtomicUsize::new(0),
        }
    }
}

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Non-CAS bump: at M0 step 2 we are single-CPU and Limine
        // hands control over with IF=0. There is no preemption and
        // no other CPU. SMP arrival in M0 step 5+ converts this to
        // a fetch_update CAS loop.
        let end = self.end.load(Ordering::Relaxed);
        let next = self.next.load(Ordering::Relaxed);
        let aligned = next.next_multiple_of(layout.align());
        let Some(new_next) = aligned.checked_add(layout.size()) else {
            return ptr::null_mut();
        };
        if new_next > end {
            return ptr::null_mut();
        }
        self.next.store(new_next, Ordering::Relaxed);
        aligned as *mut u8
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator has no free.
    }
}

#[global_allocator]
static HEAP: BumpAllocator = BumpAllocator::new();

/// Initialize the global bump allocator with the heap region
/// `[base, base + size)`.
///
/// # Safety
/// `[base, base + size)` must be valid, exclusively owned, writable
/// memory for the kernel's lifetime — no other reference, no other
/// CPU, no DMA peer. Must be called exactly once before any
/// allocation.
pub unsafe fn init(base: usize, size: usize) {
    let end = base.checked_add(size).expect("heap range overflow");
    // SAFETY: caller's contract is that no other code observes HEAP
    // before this call returns; the stores below establish the
    // initial state under that exclusive-access assumption.
    HEAP.next.store(base, Ordering::Release);
    HEAP.end.store(end, Ordering::Release);
}
