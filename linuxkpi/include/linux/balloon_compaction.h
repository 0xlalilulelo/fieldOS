/* SPDX-License-Identifier: BSD-2-Clause */
/*
 * <linux/balloon_compaction.h> — BSD-2 Arsenal-authored
 * reimplementation per ADR-0006. The balloon-page-list surface
 * virtio-balloon reaches when it #includes
 * <linux/balloon_compaction.h>.
 *
 * Arsenal leaves CONFIG_BALLOON_COMPACTION undefined — there is no
 * page-migration / compaction subsystem at M1. So balloon.c's entire
 * migration path (virtballoon_migratepage, balloon_page_insert /
 * _delete, ->isolated_pages--, ->migratepage assignment, the
 * spin_lock_irqsave isolation dance — all under
 * #ifdef CONFIG_BALLOON_COMPACTION at virtio_balloon.c:808-882 and
 * 977-979) collapses out of the compile. This header therefore omits
 * the migratepage callback, enum migrate_mode, and the isolate /
 * putback / migrate surface: they have no caller. struct
 * balloon_dev_info carries only the three fields the non-compaction
 * path uses.
 *
 * struct page is the thin per-frame handle from shim_c.h (ADR-0007).
 * balloon threads pages through page->lru via the inline push/pop
 * below; the alloc/enqueue/dequeue bodies are Rust shims in
 * linuxkpi/src/page.rs (panic-on-call until the M1-2-5-closing commit
 * gives them real struct page-backed implementations).
 */

#ifndef ARSENAL_LINUXKPI_LINUX_BALLOON_COMPACTION_H
#define ARSENAL_LINUXKPI_LINUX_BALLOON_COMPACTION_H

#include "../shim_c.h"

/* Per-device balloon page bookkeeping, embedded by value in struct
 * virtio_balloon. isolated_pages is written by balloon_devinfo_init
 * (and decremented only on the compaction path, which is compiled
 * out); pages_lock + pages guard and hold the balloon's page list. */
struct balloon_dev_info {
    unsigned long    isolated_pages;
    struct spinlock  pages_lock;
    struct list_head pages;
};

/* Initialize a balloon_dev_info to empty. Inline — matches Linux's
 * non-compaction balloon_devinfo_init. */
static inline void balloon_devinfo_init(struct balloon_dev_info *balloon)
{
    balloon->isolated_pages = 0;
    spin_lock_init(&balloon->pages_lock);
    INIT_LIST_HEAD(&balloon->pages);
}

/* Push a page onto the head of a bare page list (no dev_info / lock;
 * used to stage a batch before enqueue). Inline list op on page->lru. */
static inline void balloon_page_push(struct list_head *pages, struct page *page)
{
    list_add(&page->lru, pages);
}

/* Pop the first page off a bare page list, or NULL if empty. */
static inline struct page *balloon_page_pop(struct list_head *pages)
{
    struct page *page = list_first_entry_or_null(pages, struct page, lru);

    if (!page)
        return NULL;
    list_del(&page->lru);
    return page;
}

/* Allocate one balloon page, add a page to / remove a page from a
 * balloon_dev_info's locked list. Bodies in linuxkpi/src/page.rs. */
extern struct page *balloon_page_alloc(void);
extern void balloon_page_enqueue(struct balloon_dev_info *b_dev_info,
                                 struct page *page);
extern struct page *balloon_page_dequeue(struct balloon_dev_info *b_dev_info);

#endif /* ARSENAL_LINUXKPI_LINUX_BALLOON_COMPACTION_H */
