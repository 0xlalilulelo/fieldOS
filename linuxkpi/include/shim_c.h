/* SPDX-License-Identifier: BSD-2-Clause */

/*
 * LinuxKPI shim — C-callable declarations of Rust shim functions
 * that inherited Linux 6.12 LTS drivers under vendor/linux-6.12/
 * link against. See docs/adrs/0005-linuxkpi-shim-layout.md for the
 * bidirectional-FFI rationale and the hand-written-header decision.
 *
 * M1-2-1 surface: types + printk + slab + locks + atomics.
 * M1-2-2 surface: PCI bus adapter (pci_driver registration model)
 *   + DMA coherent + IRQ bridge (request_irq + free_irq +
 *   pci_alloc_irq_vectors + pci_irq_vector +
 *   pci_free_irq_vectors).
 * M1-2-3 surface: virtio bus adapter (virtio_driver registration
 *   model + virtio_cread / virtio_cwrite + virtqueue type with
 *   panic-on-call stubs for find_vqs / virtqueue_add_* /
 *   virtqueue_kick / virtqueue_get_buf — real impls at M1-2-5).
 *   Closes the "shim foundation" devlog cluster (2-1+2-2+2-3).
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

/* ---- <linux/pci.h> + <linux/mod_devicetable.h> ---- */

#define PCI_ANY_ID  0xFFFFFFFFU

typedef unsigned long kernel_ulong_t;

struct pci_device_id {
    __u32 vendor;
    __u32 device;
    __u32 subvendor;
    __u32 subdevice;
    __u32 class;
    __u32 class_mask;
    kernel_ulong_t driver_data;
};

struct pci_dev {
    __u16 vendor;
    __u16 device;
    __u16 subsystem_vendor;
    __u16 subsystem_device;
    __u32 class;
    __u8  bus_number;
    __u8  devfn;            /* (dev << 3) | func */
    __u64 bar_addr[6];      /* cached at probe-dispatch time */
    __u64 bar_len[6];
    void *driver_data;      /* opaque to shim; pci_set_drvdata */
    int   msix_first_slot;  /* set by pci_alloc_irq_vectors; -1 = none */
    int   msix_vector_count;
};

struct pci_driver {
    const char *name;
    const struct pci_device_id *id_table;
    int  (*probe)(struct pci_dev *dev, const struct pci_device_id *id);
    void (*remove)(struct pci_dev *dev);
};

extern int  pci_register_driver(struct pci_driver *drv);
extern void pci_unregister_driver(struct pci_driver *drv);
extern __u64 pci_resource_start(const struct pci_dev *dev, int bar);
extern __u64 pci_resource_len(const struct pci_dev *dev, int bar);
extern void *pci_iomap(const struct pci_dev *dev, int bar, __u64 max_len);
extern void  pci_iounmap(const struct pci_dev *dev, void *addr);
extern void  pci_set_master(struct pci_dev *dev);
extern int   pci_enable_device(struct pci_dev *dev);

/* MSI-X allocation. M1-2-2 supports PCI_IRQ_MSIX only; legacy
 * MSI / INTx returns negative if requested. */
#define PCI_IRQ_INTX       (1 << 0)
#define PCI_IRQ_MSI        (1 << 1)
#define PCI_IRQ_MSIX       (1 << 2)
#define PCI_IRQ_ALL_TYPES  (PCI_IRQ_INTX | PCI_IRQ_MSI | PCI_IRQ_MSIX)

extern int  pci_alloc_irq_vectors(struct pci_dev *dev,
                                  unsigned int min_vecs,
                                  unsigned int max_vecs,
                                  unsigned int flags);
extern int  pci_irq_vector(const struct pci_dev *dev, unsigned int idx);
extern void pci_free_irq_vectors(struct pci_dev *dev);

/* ---- <linux/interrupt.h> ---- */

#define IRQ_NONE         0
#define IRQ_HANDLED      1
#define IRQ_WAKE_THREAD  2

typedef int (*irq_handler_t)(int irq, void *dev_id);

extern int        request_irq(unsigned int irq, irq_handler_t handler,
                              unsigned long flags, const char *name,
                              void *dev_id);
extern void      *free_irq(unsigned int irq, void *dev_id);

/* ---- <linux/dma-mapping.h> + <linux/dma-direction.h> ---- */

struct device { unsigned char _opaque[8]; };

#define DMA_BIDIRECTIONAL  0
#define DMA_TO_DEVICE      1
#define DMA_FROM_DEVICE    2
#define DMA_NONE           3

extern void *dma_alloc_coherent(struct device *dev, size_t size,
                                dma_addr_t *dma_handle, gfp_t flags);
extern void  dma_free_coherent(struct device *dev, size_t size,
                               void *cpu_addr, dma_addr_t dma_handle);
extern dma_addr_t dma_map_single(struct device *dev, void *cpu_addr,
                                 size_t size, int dir);
extern void  dma_unmap_single(struct device *dev, dma_addr_t dma_handle,
                              size_t size, int dir);
extern void  dma_sync_single_for_device(struct device *dev,
                                        dma_addr_t dma_handle,
                                        size_t size, int dir);
extern void  dma_sync_single_for_cpu(struct device *dev,
                                     dma_addr_t dma_handle,
                                     size_t size, int dir);
extern int   dma_set_mask(struct device *dev, __u64 mask);
extern int   dma_set_coherent_mask(struct device *dev, __u64 mask);

/* ---- <linux/virtio.h> + <uapi/linux/virtio_ids.h> +
 *      <linux/mod_devicetable.h> ---- */

#define VIRTIO_DEV_ANY_ID  0xFFFFFFFFU

#define VIRTIO_ID_NET      1
#define VIRTIO_ID_BLOCK    2
#define VIRTIO_ID_CONSOLE  3
#define VIRTIO_ID_RNG      4
#define VIRTIO_ID_BALLOON  5

struct virtio_device_id {
    __u32 device;
    __u32 vendor;
};

/* Trimmed virtio_device. Fields are the ones balloon (and the M1
 * inherited driver fleet) actually reach for. Layout matches
 * linuxkpi/src/virtio.rs's `pub struct virtio_device`. */
struct virtio_device {
    __u32 id_device;
    __u32 id_vendor;
    void *priv;
    __u8  bus;
    __u8  dev;
    __u8  func;
    __u8  _pad;
    void *common_cfg;
    void *notify_base;
    __u32 notify_off_multiplier;
    void *isr;
    void *device_cfg;
};

/* Trimmed virtio_driver. M1-2-4 / 2-5 will surface missing fields
 * (feature_table, validate, scan, config_changed) when balloon's
 * compile demands them; we add then. */
struct virtio_driver {
    const char *name;
    const struct virtio_device_id *id_table;
    int  (*probe)(struct virtio_device *dev);
    void (*remove)(struct virtio_device *dev);
};

extern int  register_virtio_driver(struct virtio_driver *drv);
extern void unregister_virtio_driver(struct virtio_driver *drv);

extern __u8  virtio_cread8(const struct virtio_device *vdev, unsigned int offset);
extern __u16 virtio_cread16(const struct virtio_device *vdev, unsigned int offset);
extern __u32 virtio_cread32(const struct virtio_device *vdev, unsigned int offset);
extern void  virtio_cwrite8(struct virtio_device *vdev, unsigned int offset, __u8 val);
extern void  virtio_cwrite16(struct virtio_device *vdev, unsigned int offset, __u16 val);
extern void  virtio_cwrite32(struct virtio_device *vdev, unsigned int offset, __u32 val);

/* Virtqueue surface — opaque type + entry-point declarations.
 * M1-2-3 ships these as panic-on-call stubs; real virtqueue
 * machinery lands at M1-2-5 when virtio-balloon online demands
 * them (the gap-filling sub-block per HANDOFF). */
struct virtqueue { unsigned char _opaque[16]; };

extern int  find_vqs(struct virtio_device *vdev, unsigned int nvqs,
                     struct virtqueue **vqs, const char *const *names);
extern int  virtqueue_add_outbuf(struct virtqueue *vq, const void *sg,
                                 unsigned int num, void *data,
                                 unsigned int gfp);
extern int  virtqueue_add_inbuf(struct virtqueue *vq, const void *sg,
                                unsigned int num, void *data,
                                unsigned int gfp);
extern int  virtqueue_kick(struct virtqueue *vq);
extern void *virtqueue_get_buf(struct virtqueue *vq, unsigned int *len);

/* ---- <linux/kernel.h> + <linux/bug.h> macros ---- */

/* container_of — recover the containing struct pointer from a
 * member pointer. Linux <linux/kernel.h> idiom. Requires GNU
 * typeof() extension which clang supports under -x c. */
#define container_of(ptr, type, member) \
    ((type *)((char *)(ptr) - offsetof(type, member)))

/* BUG_ON / WARN_ON dispatch through the Rust shim; the helper
 * receives __FILE__ + __LINE__ + the stringified condition for
 * diagnostic output. WARN_ON returns the predicate value so
 * inherited C can `if (WARN_ON(cond)) return -EINVAL;`. */
extern void linuxkpi_bug(const char *file, int line, const char *cond)
    __attribute__((noreturn));
extern void linuxkpi_warn(const char *file, int line, const char *cond);

#define BUG_ON(cond) \
    do { if (cond) linuxkpi_bug(__FILE__, __LINE__, #cond); } while (0)
#define WARN_ON(cond) \
    ((cond) ? (linuxkpi_warn(__FILE__, __LINE__, #cond), 1) : 0)
/* M1: no once-tracking; degenerates to WARN_ON. */
#define WARN_ON_ONCE(cond) WARN_ON(cond)
#define WARN_ONCE(cond, ...) WARN_ON(cond)

/* ---- <linux/err.h> ---- */

#define MAX_ERRNO 4095

extern void *ERR_PTR(long error);
extern long  PTR_ERR(const void *ptr);
extern int   IS_ERR(const void *ptr);
extern int   IS_ERR_OR_NULL(const void *ptr);

/* ---- <linux/list.h> ---- */

struct list_head {
    struct list_head *next, *prev;
};

#define LIST_HEAD_INIT(name) { &(name), &(name) }
#define LIST_HEAD(name) struct list_head name = LIST_HEAD_INIT(name)

extern void INIT_LIST_HEAD(struct list_head *list);
extern void list_add(struct list_head *new_, struct list_head *head);
extern void list_add_tail(struct list_head *new_, struct list_head *head);
extern void list_del(struct list_head *entry);
extern int  list_empty(const struct list_head *head);

#define list_entry(ptr, type, member) container_of(ptr, type, member)
#define list_for_each_entry(pos, head, member) \
    for (pos = list_entry((head)->next, typeof(*pos), member); \
         &pos->member != (head); \
         pos = list_entry(pos->member.next, typeof(*pos), member))

/* ---- <linux/jiffies.h> + <linux/delay.h> ---- */

#define HZ 100UL  /* arsenal-kernel LAPIC calibration; see time.rs */

extern unsigned long jiffies(void);
extern void msleep(unsigned int msecs);
extern void udelay(unsigned int usecs);
extern void ndelay(unsigned int nsecs);

/* ---- <linux/uaccess.h> ---- */

#define __user  /* nothing — Linux annotation kept for source compat */

extern unsigned long copy_to_user(void *to, const void *from, unsigned long n);
extern unsigned long copy_from_user(void *to, const void *from, unsigned long n);

#endif /* ARSENAL_LINUXKPI_SHIM_C_H */
