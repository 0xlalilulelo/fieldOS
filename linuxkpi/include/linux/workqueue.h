/* SPDX-License-Identifier: BSD-2-Clause */
/*
 * <linux/workqueue.h> — BSD-2 Arsenal-authored reimplementation
 * per ADR-0006. Declares the deferred-work surface (struct
 * work_struct, struct workqueue_struct, INIT_WORK, alloc_workqueue,
 * destroy_workqueue, queue_work, cancel_work) that inherited
 * Linux drivers reach when they #include <linux/workqueue.h>.
 *
 * Per ADR-0005 § 6, M1 ships these as panic-on-call stubs —
 * synchronous module init only; no deferred-path scheduler
 * integration until the workqueue subsystem becomes its own
 * design decision at M1 step 5 (amdgpu) or step 6 (iwlwifi).
 * balloon's workqueue use is in the inflate/deflate hot path
 * (BALLOON_F_DEFERRED_REPORTING and stats reporting); if balloon
 * probe reaches a queue_work() call at runtime, it panics
 * loudly rather than silently dropping the work.
 *
 * struct work_struct's opaque buffer is sized generously
 * (8 pointers worth) so that a future real impl can extend the
 * field set without breaking the C ABI inherited drivers see.
 * Linux's actual struct work_struct is ~24 bytes on 64-bit
 * (atomic_long data + struct list_head entry + work_func_t func);
 * our 64-byte opaque has room for any reasonable extension.
 */

#ifndef ARSENAL_LINUXKPI_LINUX_WORKQUEUE_H
#define ARSENAL_LINUXKPI_LINUX_WORKQUEUE_H

#include "../shim_c.h"

struct work_struct;

typedef void (*work_func_t)(struct work_struct *work);

struct work_struct {
    unsigned char _opaque[64];
};

struct workqueue_struct;  /* opaque to inherited C; impl-defined */

/* INIT_WORK is a macro in Linux. We dispatch to a Rust extern
 * that records the work_func_t in the opaque buffer for the
 * eventual real impl to find. */
extern void linuxkpi_work_init(struct work_struct *work, work_func_t func);

#define INIT_WORK(_work, _func) linuxkpi_work_init((_work), (_func))

/* Workqueue lifecycle + dispatch — extern, panic-on-call at M1. */
extern struct workqueue_struct *alloc_workqueue(const char *fmt,
                                                unsigned int flags,
                                                int max_active);
extern void destroy_workqueue(struct workqueue_struct *wq);
extern bool queue_work(struct workqueue_struct *wq, struct work_struct *work);
extern bool cancel_work(struct work_struct *work);
/* cancel_work_sync — cancel + wait for the work to finish. M1
 * panic-on-call (the deferred-work path doesn't run yet). */
extern bool cancel_work_sync(struct work_struct *work);

/* System-wide shared workqueues. M1: declared so inherited drivers
 * can name them in queue_work calls; queue_work is itself panic-on-
 * call, so the pointer is never dereferenced yet (defined NULL in
 * workqueue.rs). balloon queues its stats / size work on
 * system_freezable_wq. */
extern struct workqueue_struct *system_freezable_wq;

/* Workqueue creation flags — values match Linux 6.12 LTS so any
 * future real impl honors the same semantics. balloon passes
 * WQ_FREEZABLE | WQ_MEM_RECLAIM. */
#define WQ_UNBOUND       (1 << 1)
#define WQ_FREEZABLE     (1 << 2)
#define WQ_MEM_RECLAIM   (1 << 3)
#define WQ_HIGHPRI       (1 << 4)
#define WQ_CPU_INTENSIVE (1 << 5)

#endif /* ARSENAL_LINUXKPI_LINUX_WORKQUEUE_H */
