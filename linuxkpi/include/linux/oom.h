/* SPDX-License-Identifier: BSD-2-Clause */
/*
 * <linux/oom.h> — BSD-2 Arsenal-authored reimplementation per
 * ADR-0006. The out-of-memory-notifier surface inherited drivers
 * reach when they #include <linux/oom.h>.
 *
 * Arsenal has no OOM killer / OOM-notifier subsystem at M1: the
 * frame allocator returns failure on exhaustion and the caller
 * handles it; there is no notifier chain to drive on memory
 * pressure. struct notifier_block + NOTIFY_OK live in shim_c.h
 * (general notifier surface); this header adds the oom-specific
 * register/unregister entry points, bodies in linuxkpi/src/mm.rs
 * as panic-on-call stubs.
 *
 * balloon's only oom.h use is register_oom_notifier(&vb->oom_nb) /
 * unregister_oom_notifier, gated behind VIRTIO_BALLOON_F_DEFLATE_ON_OOM
 * feature negotiation (virtio_balloon.c:1015, 1091, 1130). If a
 * balloon device negotiates that feature at runtime the stub panics
 * loudly rather than silently dropping the deflate-on-OOM contract;
 * the M1 smoke device does not negotiate it.
 */

#ifndef ARSENAL_LINUXKPI_LINUX_OOM_H
#define ARSENAL_LINUXKPI_LINUX_OOM_H

#include "../shim_c.h"

extern int register_oom_notifier(struct notifier_block *nb);
extern int unregister_oom_notifier(struct notifier_block *nb);

#endif /* ARSENAL_LINUXKPI_LINUX_OOM_H */
