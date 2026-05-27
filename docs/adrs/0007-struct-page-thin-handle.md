# ADR-0007: `struct page` is a thin per-frame handle, not a `mem_map` array

## Status

Accepted. 2026-05-27. First-use decision, forced by virtio-balloon's
`#include <linux/balloon_compaction.h>` during M1-2-5 Part B sub-task
3's compile-error iteration. Takes the ADR-0007 slot that
[ADR-0006](0006-linuxkpi-headers-are-shim.md) had provisionally
reserved for the three-crate split; that and the other two
provisional reservations shift up by one (three-crate split →
ADR-0008, cbindgen adoption → ADR-0009, deferred init via
kthread+workqueue → ADR-0010), recorded as a one-line edit to
ADR-0006 in this ADR's accepting commit. The reservations were
explicitly provisional and unwritten; `struct page` is a decision
being made now, so it takes the next live number.

## Context

virtio-balloon is the first inherited Linux driver to traffic in
`struct page`. Its `#include <linux/balloon_compaction.h>` requires:

- `struct balloon_dev_info` embedded **by value** in `struct
  virtio_balloon` (the driver's per-device state), which holds a
  `struct list_head pages` of balloon-owned pages.
- `balloon_page_alloc()` → `struct page *`, `balloon_page_enqueue(dev_info, page)`,
  `balloon_page_dequeue(dev_info)` → `struct page *`, and the inline
  `balloon_page_push(list, page)` / `balloon_page_pop(list)` which
  thread pages through `page->lru`.
- `page_to_pfn(page)` (in `page_to_balloon_pfn`) and `page_address(page)`
  (in the free-page-reporting path).

`struct page` is **foundational, not balloon-specific**: every
mm-touching inherited driver to come reaches it. amdgpu's GEM/TTM
allocators (M1 step 5) build on `struct page`; NVMe and xHCI DMA
buffers (already native, but their LinuxKPI-shim equivalents) would;
iwlwifi's RX rings (step 6) would. The representation chosen here is
the one those drivers inherit. Per [CLAUDE.md](../../CLAUDE.md) this
is an explicit design decision, recorded rather than picked silently.

### How Linux represents `struct page`

Linux keeps a global array (`mem_map`, or `vmemmap` on sparse-memory
configs) with one `struct page` per physical frame, indexed by page
frame number (pfn). `page_to_pfn(page)` is the pointer subtraction
`page - mem_map`; `pfn_to_page(pfn)` is `mem_map + pfn`. Drivers
exploit this: they do pointer arithmetic between `struct page`s, pass
pfns and recover the page, and assume `struct page` is a stable
identity for a physical frame for the machine's lifetime. The struct
itself is large (~64 bytes) and heavily unionized; drivers treat all
but a few fields opaquely.

### What balloon actually needs

balloon never calls `pfn_to_page`. It goes `page → pfn` (one
direction), `page → kernel virtual address`, and threads pages onto
its own lists via `page->lru`. It allocates pages with
`balloon_page_alloc` (one order-0 page at a time) and frees them on
the deflate path. With `CONFIG_BALLOON_COMPACTION` left undefined
(Arsenal has no page-migration/compaction subsystem at M1), the
isolation/migration surface — `balloon_page_isolate`,
`balloon_page_migrate`, `balloon_page_insert/delete`,
`->isolated_pages`, `->migratepage` — collapses out of balloon.c
(it is all under `#ifdef CONFIG_BALLOON_COMPACTION`, balloon.c:808-882
and 977-979).

Arsenal already has the substrate a thin handle needs:
`frames::FRAMES.alloc_frame()` (4-KiB page-aligned physical frames),
`paging::hhdm_offset()` (phys→virt for the kernel direct map), both
exposed to the shim as `linuxkpi_frames_alloc_frame` /
`linuxkpi_frames_free_frame` / `linuxkpi_paging_hhdm_offset` bridge
functions.

## Decision

`struct page` is a **thin per-frame handle**: a small descriptor
allocated alongside the physical frame it represents, carrying only
what the shim and inherited drivers touch.

```c
struct page {
    struct list_head lru;        /* driver list threading (balloon ->pages, etc.) */
    unsigned long    _phys;      /* backing physical address (4-KiB aligned)        */
    int              _refcount;  /* get_page / put_page                              */
    void            *_private;   /* Linux page.private — driver-opaque scratch       */
};
```

- `page_to_pfn(page)` = `(page)->_phys >> PAGE_SHIFT`. O(1), no array.
- `page_address(page)` = `hhdm_offset() + (page)->_phys`. O(1).
- `alloc_pages(gfp, order)` (order 0 only at M1) allocates one frame
  from `frames::FRAMES`, allocates a `struct page` descriptor, sets
  `_phys` + `_refcount = 1`, returns it. `__free_page` / `free_pages`
  reverse it.
- `get_page` / `put_page` adjust `_refcount`; `put_page` on the last
  reference frees the frame and the descriptor.
- The `_`-prefixed fields signal "shim-internal — inherited drivers
  do not touch these"; drivers only ever touch `lru` (and that only
  via the list helpers).

There is **no `mem_map` / `vmemmap` array**. `pfn_to_page` is
therefore unsupported and is declared as a panic-on-call stub: no
inherited driver Arsenal hosts at M1 needs the reverse mapping, and
fabricating one without the array would be a silent lie.

The descriptor's allocation backing is the existing kernel slab
(`kmalloc`), not a dedicated pool, until profiling says otherwise —
one small allocation per ballooned page is acceptable for balloon's
inflate/deflate cadence.

`balloon_page_alloc` / `balloon_page_enqueue` / `balloon_page_dequeue`
ship as panic-on-call stubs at the header-resolution commit (the
M1-2-5 Part B sub-task 3 iteration discipline — link-clean now,
fail-loud on the deferred path), and get their real `struct
page`-backed implementations at the M1-2-5-closing commit alongside
the virtqueue impls that `ARSENAL_VIRTIO_BALLOON_OK` forces.

## Alternatives rejected

- **Full `mem_map` / `vmemmap` array now.** Allocate a global
  `struct page` array indexed by pfn at boot, covering all RAM — the
  real Linux model, maximally compatible with arbitrary inherited
  drivers (page-pointer arithmetic and `pfn_to_page` both work).
  **Rejected for M1** because it is a real mm-subsystem addition
  (boot-time array sizing over the memory map, sparse-memory handling
  for the framebuffer/MMIO holes) costing `sizeof(struct page) ×
  nframes` of RAM upfront — on the order of megabytes per gigabyte —
  built speculatively before any hosted driver needs the reverse
  mapping or inter-page arithmetic. balloon needs neither. The thin
  handle satisfies balloon at a fraction of the work; the mem_map
  array is the right call **when a driver actually needs it** — most
  likely amdgpu's GEM/TTM at M1 step 5 — and that is the moment to
  write the successor ADR with concrete requirements in hand rather
  than guessing them now. This is the [CLAUDE.md](../../CLAUDE.md)
  "suggest the smaller version first" posture: the thin handle is
  ~30% of the work for the 100% of balloon's need.

- **Make `struct page` fully opaque (no fields visible to C).**
  Impossible: `balloon_page_pop`'s `list_first_entry_or_null(pages,
  struct page, lru)` does `container_of` from a `list_head` back to
  `struct page`, which needs `offsetof(struct page, lru)` — the
  layout must be visible to the C compiler.

- **Vendor `mm/balloon_compaction.c` (GPLv2) for the real
  enqueue/dequeue/alloc bodies.** Consistent with vendoring the
  driver `.c`, but `balloon_compaction.c` is a **core mm helper**,
  not the driver — and its non-compaction bodies are short (allocate
  a page, list_add/list_del on `->lru`, refcount). Reimplementing
  them in BSD-2 Rust fits [ADR-0006](0006-linuxkpi-headers-are-shim.md)
  § 1's "the shim is the surface" gravity better than dragging a
  second GPLv2 translation unit into the build. **Rejected** in favor
  of BSD-2 Rust shims.

## Consequences

**Easier:**

- balloon's `struct page` need is met with a ~4-field struct + four
  O(1) accessors over substrate that already exists. No boot-time
  array, no memory-map walk, no sparse-memory handling.
- `page_to_pfn` / `page_address` are trivially correct (shift / add)
  and need no global state — easy to reason about and test.

**Harder / deferred:**

- **`pfn_to_page` and inter-`struct page` pointer arithmetic are
  unsupported.** A future inherited driver that assumes `page ==
  &mem_map[pfn]` (common in DMA scatter-gather and huge-page paths)
  will hit the `pfn_to_page` panic-stub or compute wrong addresses.
  **Mitigation:** the panic-on-call stub is fail-loud; the successor
  ADR (mem_map array) is pre-identified, with amdgpu step 5 the
  likely trigger. The shim self-test discipline (ADR-0006 § Consequences)
  covers the thin-handle accessors in the same commit as their impl.

- **One small `kmalloc` per ballooned page.** Acceptable at balloon's
  cadence; a dedicated descriptor pool is a later optimization gated
  on profiling, not a correctness concern.

**New risks:**

- **Two definitions of `struct page` to keep in sync** — the C view
  in `shim_c.h` and the `#[repr(C)]` Rust mirror in
  `linuxkpi/src/page.rs`. **Mitigation:** the same hand-maintained-FFI
  risk ADR-0005 § 5 already accepts for every shim struct; a
  layout-mismatch surfaces as a self-test failure or a wrong
  `page_address`, both fail-loud. cbindgen (deferred to ADR-0009)
  would eliminate the duplication if the FFI surface grows enough to
  justify it.

## References

- [ADR-0005: LinuxKPI shim layout and GPL/BSD-2 boundary](0005-linuxkpi-shim-layout.md)
  — hand-maintained FFI (§ 5), synchronous init + panic-on-call
  deferred-path stubs (§ 6)
- [ADR-0006: LinuxKPI headers are the shim](0006-linuxkpi-headers-are-shim.md)
  — "the shim is the surface" (§ 1); the provisional ADR-0007/8/9
  reservations this ADR shifts up by one
- [`linuxkpi/include/linux/balloon_compaction.h`](../../linuxkpi/include/linux/balloon_compaction.h)
  — the header this decision lets compile
- [`linuxkpi/src/page.rs`](../../linuxkpi/src/page.rs)
  — the thin-handle implementation + `#[repr(C)]` Rust mirror
- [Linux 6.12 LTS `include/linux/mm_types.h`](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/tree/include/linux/mm_types.h?h=linux-6.12.y)
  — upstream `struct page` (the large unionized array element this
  ADR deliberately does not reproduce)
- [Linux 6.12 LTS `include/linux/balloon_compaction.h`](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/tree/include/linux/balloon_compaction.h?h=linux-6.12.y)
  — the API surface reimplemented in BSD-2
- Michael Nygard, "Documenting Architecture Decisions" (2011) — ADR
  template authority
