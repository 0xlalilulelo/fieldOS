/* SPDX-License-Identifier: BSD-2-Clause */
/*
 * <linux/page_reporting.h> — BSD-2 Arsenal-authored reimplementation
 * per ADR-0006. The free-page-reporting surface inherited drivers
 * reach when they #include <linux/page_reporting.h>.
 *
 * Free-page reporting lets a guest hand the host hints about which
 * guest pages are free so the host can reclaim them. Arsenal has no
 * such subsystem at M1; page_reporting_register / _unregister are
 * panic-on-call stubs in linuxkpi/src/mm.rs. balloon reaches them
 * only under VIRTIO_BALLOON_F_REPORTING feature negotiation
 * (virtio_balloon.c:1067, 1128), which the M1 smoke device does not
 * enable.
 *
 * struct page_reporting_dev_info is embedded by value in struct
 * virtio_balloon; balloon touches only ->report (its callback) and
 * ->order (the free-page block order it reports). The report
 * callback takes a struct scatterlist *, forward-declared here —
 * the full <linux/scatterlist.h> definition lands when balloon's
 * body compile reaches the sg_* uses.
 */

#ifndef ARSENAL_LINUXKPI_LINUX_PAGE_REPORTING_H
#define ARSENAL_LINUXKPI_LINUX_PAGE_REPORTING_H

#include "../shim_c.h"

/* Max scatterlist entries a single report batch carries (Linux's
 * value). balloon sizes its reporting sg array to this. */
#define PAGE_REPORTING_CAPACITY 32

struct scatterlist;

struct page_reporting_dev_info {
    int (*report)(struct page_reporting_dev_info *prdev,
                  struct scatterlist *sgl, unsigned int nents);
    unsigned int order;
};

extern int  page_reporting_register(struct page_reporting_dev_info *prdev);
extern void page_reporting_unregister(struct page_reporting_dev_info *prdev);

#endif /* ARSENAL_LINUXKPI_LINUX_PAGE_REPORTING_H */
