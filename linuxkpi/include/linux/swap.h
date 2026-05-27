/* SPDX-License-Identifier: BSD-2-Clause */
/*
 * <linux/swap.h> — BSD-2 Arsenal-authored reimplementation per
 * ADR-0006. Declares the minimum balloon needs from Linux's
 * swap / memory-info surface: struct sysinfo + si_meminfo + the
 * si_mem_available helper that balloon's stats path calls.
 *
 * Arsenal has no swap and no kernel-style memory accounting
 * (file-page caches, dirty / writeback counters, etc.) at M1; the
 * extern functions land as panic-on-call stubs in
 * linuxkpi/src/mm.rs (created at this iteration). Real impls land
 * in the M1-2-5-closing commit, sourced from arsenal-kernel's
 * frame allocator (`frames::FRAMES.free_count()` etc.) — balloon
 * only ever reads them informationally, so returning honest
 * free / total frame counts (and zero for swap / file-page caches)
 * is enough to satisfy the host's stats reporting.
 *
 * The heavy VM-event counter machinery balloon's update_balloon_
 * vm_stats reaches (PSWPIN / PSWPOUT / PGMAJFAULT / PGSCAN_* /
 * PGSTEAL_* / OOM_KILL / etc., all in NR_VM_EVENT_ITEMS) is gated
 * by CONFIG_VM_EVENT_COUNTERS in balloon.c. Arsenal leaves that
 * symbol undefined; the entire #ifdef block collapses to the
 * inline-return-0 fallback at virtio_balloon.c:395.
 */

#ifndef ARSENAL_LINUXKPI_LINUX_SWAP_H
#define ARSENAL_LINUXKPI_LINUX_SWAP_H

#include "../shim_c.h"

/* Linux's struct sysinfo (from kernel.h / sysinfo.h depending on
 * version) — fields balloon reaches: freeram, totalram. Other
 * fields kept to match the Linux ABI shape so future inherited
 * drivers compile against the same layout. */
struct sysinfo {
    long uptime;
    unsigned long loads[3];
    unsigned long totalram;
    unsigned long freeram;
    unsigned long sharedram;
    unsigned long bufferram;
    unsigned long totalswap;
    unsigned long freeswap;
    unsigned short procs;
    unsigned short pad;
    unsigned long totalhigh;
    unsigned long freehigh;
    unsigned int mem_unit;
    char _f[20 - 2 * sizeof(long) - sizeof(int)];
};

extern void si_meminfo(struct sysinfo *info);
extern long si_mem_available(void);

#endif /* ARSENAL_LINUXKPI_LINUX_SWAP_H */
