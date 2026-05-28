/* SPDX-License-Identifier: BSD-2-Clause */
/*
 * <linux/mm.h> — BSD-2 Arsenal-authored reimplementation per
 * ADR-0006. The page-allocation + page-accessor surface inherited
 * drivers reach when they #include <linux/mm.h>.
 *
 * Everything here works over the ADR-0007 thin-handle struct page
 * (defined in shim_c.h). page_to_pfn is a pure inline accessor
 * (_phys >> PAGE_SHIFT); page_address and the page lifecycle
 * (alloc_pages / free_pages / put_page / adjust_managed_page_count)
 * are Rust shims in linuxkpi/src/page.rs. The lifecycle entries
 * ship as panic-on-call stubs through this iteration arc; their
 * real frame-allocator-backed bodies + a self-test land at the
 * M1-2-5-closing commit (ADR-0007), alongside the virtqueue impls.
 *
 * get_page is omitted — balloon's only use (virtio_balloon.c:845)
 * is under #ifdef CONFIG_BALLOON_COMPACTION, which Arsenal leaves
 * undefined, so it has no caller.
 */

#ifndef ARSENAL_LINUXKPI_LINUX_MM_H
#define ARSENAL_LINUXKPI_LINUX_MM_H

#include "../shim_c.h"

/* Page frame number of a struct page. Pure accessor over the
 * thin handle's backing physical address. */
static inline unsigned long page_to_pfn(const struct page *page)
{
    return page->_phys >> PAGE_SHIFT;
}

/* Kernel virtual address of a page's contents (HHDM + _phys). */
extern void *page_address(const struct page *page);

/* Allocate 2^order contiguous pages; returns the head struct page
 * or NULL. balloon uses order VIRTIO_BALLOON_HINT_BLOCK_ORDER for
 * free-page hinting and order 0 elsewhere. */
extern struct page *alloc_pages(gfp_t gfp, unsigned int order);

/* Free 2^order pages previously returned (by virtual address, the
 * Linux free_pages contract — balloon passes page_address(page)). */
extern void free_pages(unsigned long addr, unsigned int order);

/* Drop a balloon reference on a page; frees on the last reference. */
extern void put_page(struct page *page);

/* Adjust the kernel's managed-page accounting by `count` pages as
 * balloon inflates (negative) / deflates (positive). */
extern void adjust_managed_page_count(struct page *page, long count);

#endif /* ARSENAL_LINUXKPI_LINUX_MM_H */
