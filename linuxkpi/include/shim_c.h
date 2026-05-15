/* SPDX-License-Identifier: BSD-2-Clause */

/*
 * LinuxKPI shim — C-callable declarations of Rust shim functions
 * that inherited Linux 6.12 LTS drivers under vendor/linux-6.12/
 * link against. See docs/adrs/0005-linuxkpi-shim-layout.md for the
 * bidirectional-FFI rationale and the hand-written-header decision.
 *
 * M1-2-1 surface: types + printk + slab + locks + atomics.
 *   - PCI bus + request_irq + DMA arrives at M1-2-2.
 *   - virtio bus arrives at M1-2-3.
 *   - cc-driven compilation of inherited C against this header
 *     lands at M1-2-4 — until then this header is consumed only
 *     by the Rust shim's own type-shape declarations.
 */

#ifndef ARSENAL_LINUXKPI_SHIM_C_H
#define ARSENAL_LINUXKPI_SHIM_C_H

#include <stddef.h>  /* size_t */
#include <stdint.h>  /* fixed-width integers */

/* ---- <linux/types.h> aliases ---- */

typedef uint8_t  __u8;
typedef uint16_t __u16;
typedef uint32_t __u32;
typedef uint64_t __u64;
typedef int8_t  __s8;
typedef int16_t __s16;
typedef int32_t __s32;
typedef int64_t __s64;

typedef uint32_t gfp_t;
typedef uint64_t dma_addr_t;
typedef int64_t  loff_t;

/* ---- <linux/gfp.h> ---- */

#define GFP_KERNEL  0x00000001U  /* may sleep; not from IRQ context */
#define GFP_ATOMIC  0x00000002U  /* must not sleep */
#define __GFP_ZERO  0x00000004U  /* zero-fill on alloc */

/* ---- <linux/printk.h> + <linux/kern_levels.h> ---- */

#define KERN_SOH        "\001"
#define KERN_EMERG      KERN_SOH "0"
#define KERN_ALERT      KERN_SOH "1"
#define KERN_CRIT       KERN_SOH "2"
#define KERN_ERR        KERN_SOH "3"
#define KERN_WARNING    KERN_SOH "4"
#define KERN_NOTICE     KERN_SOH "5"
#define KERN_INFO       KERN_SOH "6"
#define KERN_DEBUG      KERN_SOH "7"

/*
 * pr_* convenience macros. M1-2-1 ships printk without varargs
 * (literal-string callers only); pr_info("foo: %d\n", x) with
 * format specifiers becomes valid at M1-2-4 when printk grows the
 * varargs signature against the first inherited driver's needs.
 */
#define pr_emerg(fmt)   printk(KERN_EMERG fmt)
#define pr_alert(fmt)   printk(KERN_ALERT fmt)
#define pr_crit(fmt)    printk(KERN_CRIT fmt)
#define pr_err(fmt)     printk(KERN_ERR fmt)
#define pr_warn(fmt)    printk(KERN_WARNING fmt)
#define pr_notice(fmt)  printk(KERN_NOTICE fmt)
#define pr_info(fmt)    printk(KERN_INFO fmt)
#define pr_debug(fmt)   printk(KERN_DEBUG fmt)

extern int printk(const char *fmt);

/* ---- <linux/slab.h> ---- */

extern void *kmalloc(size_t size, gfp_t flags);
extern void *kzalloc(size_t size, gfp_t flags);
extern void *krealloc(void *p, size_t new_size, gfp_t flags);
extern void  kfree(const void *p);

/* ---- <linux/atomic.h> ---- */

typedef struct { int counter; } atomic_t;

extern void atomic_inc(atomic_t *v);
extern void atomic_dec(atomic_t *v);
extern int  atomic_read(const atomic_t *v);
extern void atomic_set(atomic_t *v, int i);

/* ---- <linux/mutex.h> ---- */

/*
 * struct mutex / struct spinlock are declared opaque-with-
 * placeholder-size for inherited C consumers. The Rust shim
 * (linuxkpi/src/locks.rs) defines the actual layout via repr(C);
 * the placeholder bytes here cover spin::Mutex<()>'s footprint
 * with margin. M1-2-4 will static_assert the size match between
 * Rust and C at the cc build step.
 */
struct mutex { unsigned char _opaque[16]; };
struct spinlock { unsigned char _opaque[16]; };

extern void mutex_init(struct mutex *m);
extern void mutex_lock(struct mutex *m);
extern void mutex_unlock(struct mutex *m);

/* ---- <linux/spinlock.h> ---- */

extern void spin_lock_init(struct spinlock *s);
extern void spin_lock(struct spinlock *s);
extern void spin_unlock(struct spinlock *s);

#endif /* ARSENAL_LINUXKPI_SHIM_C_H */
