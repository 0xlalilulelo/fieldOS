// SPDX-License-Identifier: BSD-2-Clause
//
// Stack-of-frames physical frame allocator. Hands out 4-KiB physical
// frames; the deep page-table clone (3-2) and any later subsystem
// that needs raw RAM pulls from here. Backed by a heap-allocated
// Vec<u64> of free frame addresses — push to free, pop to allocate,
// O(1) both ways.
//
// Backed by the heap (so heap::init must run first). Under SMP
// contention (M0 step 5+) the spin::Mutex is the obvious choke
// point and gets revisited then — either by per-CPU caches or by
// switching to a free-list-on-frame data structure that needs no
// external bookkeeping.

use alloc::vec::Vec;

use limine::memory_map::{Entry, EntryType};
use spin::Mutex;
use x86_64::PhysAddr;
use x86_64::structures::paging::{PhysFrame, Size4KiB};

const FRAME_SIZE: u64 = 4096;

pub struct FrameAllocator {
    free: Mutex<Vec<u64>>,
}

impl FrameAllocator {
    pub const fn new() -> Self {
        Self {
            free: Mutex::new(Vec::new()),
        }
    }

    /// Push every 4-KiB frame in `[base, base+length)` (after frame
    /// alignment) onto the free pool.
    pub fn add_region(&self, base: u64, length: u64) {
        let aligned_base = (base + FRAME_SIZE - 1) & !(FRAME_SIZE - 1);
        let aligned_end = (base + length) & !(FRAME_SIZE - 1);
        if aligned_end <= aligned_base {
            return;
        }
        let count = ((aligned_end - aligned_base) / FRAME_SIZE) as usize;
        let mut frames = self.free.lock();
        frames.reserve(count);
        for i in 0..count as u64 {
            frames.push(aligned_base + i * FRAME_SIZE);
        }
    }

    pub fn alloc_frame(&self) -> Option<PhysFrame<Size4KiB>> {
        let addr = self.free.lock().pop()?;
        Some(PhysFrame::containing_address(PhysAddr::new(addr)))
    }

    pub fn free_frame(&self, frame: PhysFrame<Size4KiB>) {
        self.free.lock().push(frame.start_address().as_u64());
    }
}

pub static FRAMES: FrameAllocator = FrameAllocator::new();

/// Walk Limine's memory map (passed by the caller — main.rs owns the
/// single MemoryMapRequest static) and add every USABLE region to
/// the global frame allocator, excluding the byte range
/// `[heap_phys_start, heap_phys_end)` already reserved by `heap::init`.
/// Self-tests one alloc/free round trip before returning.
pub fn init(entries: &[&Entry], heap_phys_start: u64, heap_phys_end: u64) {
    for entry in entries {
        if entry.entry_type != EntryType::USABLE {
            continue;
        }
        let region_start = entry.base;
        let region_end = entry.base + entry.length;

        // Add the prefix of the region that ends at or before the heap.
        if region_start < heap_phys_start {
            let end = region_end.min(heap_phys_start);
            FRAMES.add_region(region_start, end - region_start);
        }
        // Add the suffix that starts at or after the heap.
        if region_end > heap_phys_end {
            let start = region_start.max(heap_phys_end);
            FRAMES.add_region(start, region_end - start);
        }
    }

    // Sanity self-test: pop one frame, push it back. Catches a broken
    // alloc path before the deep clone (3-2) bets the kernel on it.
    let test = FRAMES
        .alloc_frame()
        .expect("frames: alloc returned None on a freshly-populated pool");
    FRAMES.free_frame(test);
}
