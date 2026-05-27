/* SPDX-License-Identifier: BSD-2-Clause */
/*
 * <linux/delay.h> — BSD-2 Arsenal-authored reimplementation per
 * ADR-0006. The busy-wait / sleep surface inherited drivers reach
 * when they #include <linux/delay.h>.
 *
 * The backing implementations already shipped at M1-2-5 Part A in
 * linuxkpi/src/time.rs (msleep / udelay / ndelay over the LAPIC
 * TICKS counter at HZ=100), declared in shim_c.h; this proxy is
 * the include balloon (and future inherited drivers) names. It
 * adds mdelay — the one canonical delay.h member without its own
 * Rust symbol — as a pure macro over udelay, so the surface a
 * driver sees matches Linux without new impl work.
 *
 * balloon's only delay.h use is msleep(200) in the leak-balloon
 * retry path (virtio_balloon.c:261). At M1's cooperative scheduler
 * msleep busy-waits (time.rs documents the GFP_ATOMIC "must not
 * sleep" caveat the M1-2-5 (c) failure mode names); M2's sleep-
 * capable mutex adds IrqGuard-aware enforcement at the call site.
 */

#ifndef ARSENAL_LINUXKPI_LINUX_DELAY_H
#define ARSENAL_LINUXKPI_LINUX_DELAY_H

#include "../shim_c.h"

/* mdelay(n) — millisecond busy-delay. Linux loops udelay(1000) n
 * times to dodge udelay's per-call argument ceiling; at M1 udelay
 * is already coarse (one LAPIC jiffy = 10 ms minimum), so the loop
 * form is preserved for shape but resolves to the same spin. */
#define mdelay(n)                          \
    do {                                   \
        unsigned int __ms = (n);           \
        while (__ms--)                     \
            udelay(1000);                  \
    } while (0)

#endif /* ARSENAL_LINUXKPI_LINUX_DELAY_H */
