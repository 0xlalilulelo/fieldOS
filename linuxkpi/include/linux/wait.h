/* SPDX-License-Identifier: BSD-2-Clause */
/*
 * <linux/wait.h> — BSD-2 Arsenal-authored reimplementation per
 * ADR-0006. The wait-queue surface inherited drivers reach when
 * they #include <linux/wait.h>.
 *
 * M1 model: wait_event busy-polls its condition. Arsenal's M1
 * scheduler is cooperative with no sleep/wake-on-waitqueue
 * machinery, so the wait_queue_head_t holds no waiter list,
 * init_waitqueue_head and wake_up are no-ops, and wait_event spins
 * on the condition with a PAUSE hint. This is the same busy-wait
 * posture time.rs already takes for msleep / udelay; it is correct
 * here because balloon's wait_event conditions are
 * virtqueue_get_buf() calls that read the virtqueue used ring
 * directly (the device marks the buffer used asynchronously), so
 * the condition becomes true without the wake_up path — exactly the
 * polled-completion model M0's virtio-blk / virtio-net used.
 *
 * balloon's wait.h use: wait_queue_head_t acked (embedded in struct
 * virtio_balloon), init_waitqueue_head(&vb->acked) in probe,
 * wake_up(&vb->acked) in the balloon_ack vq callback, and
 * wait_event(vb->acked, virtqueue_get_buf(...)) in tell_host /
 * stats (virtio_balloon.c:91, 968, 180, 195, 221).
 */

#ifndef ARSENAL_LINUXKPI_LINUX_WAIT_H
#define ARSENAL_LINUXKPI_LINUX_WAIT_H

#include "../shim_c.h"

/* Opaque at M1 — busy-poll wait_event keeps no waiter list. Sized
 * to give a future real waitqueue (spinlock + list_head) room
 * without changing the ABI inherited drivers embed by value. */
typedef struct { unsigned char _opaque[16]; } wait_queue_head_t;

/* PAUSE spin-wait hint (Intel SDM Vol. 2B, "PAUSE"). Local to
 * wait.h until a second consumer earns a promotion to shim_c.h. */
#define cpu_relax() __asm__ __volatile__("pause" ::: "memory")

#define init_waitqueue_head(wq) ((void)(wq))
#define wake_up(wq)             ((void)(wq))

/* Block until `condition` is true. M1: busy-poll (see file header).
 * `condition` is re-evaluated each spin — balloon's is a
 * virtqueue_get_buf() that polls the used ring. */
#define wait_event(wq, condition)        \
    do {                                 \
        (void)(wq);                      \
        while (!(condition))             \
            cpu_relax();                 \
    } while (0)

#endif /* ARSENAL_LINUXKPI_LINUX_WAIT_H */
