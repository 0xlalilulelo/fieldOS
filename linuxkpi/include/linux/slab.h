/* SPDX-License-Identifier: BSD-2-Clause */
/*
 * <linux/slab.h> — BSD-2 Arsenal-authored reimplementation per
 * ADR-0006. The kernel allocator surface inherited drivers reach
 * when they #include <linux/slab.h>.
 *
 * The backing implementations shipped at M1-2-1 in
 * linuxkpi/src/slab.rs (kmalloc / kzalloc / krealloc / kfree over
 * arsenal-kernel's frame allocator) and are declared in shim_c.h
 * alongside the GFP_* flags; in Linux those flags live in
 * <linux/gfp_types.h>, pulled in transitively through slab.h —
 * this proxy is the include name balloon uses, and shim_c.h is
 * where both the allocator externs and the flag values resolve.
 *
 * balloon's slab.h use is kzalloc(sizeof(*vb), GFP_KERNEL) for its
 * per-device state (virtio_balloon.c:958) and the paired kfree on
 * the probe-failure and remove paths (1101, 1145); the GFP_NOWAIT
 * and __GFP_* modifier flags it also names are advisory at M1
 * (slab.rs ignores the flags argument).
 */

#ifndef ARSENAL_LINUXKPI_LINUX_SLAB_H
#define ARSENAL_LINUXKPI_LINUX_SLAB_H

#include "../shim_c.h"

#endif /* ARSENAL_LINUXKPI_LINUX_SLAB_H */
