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
#include <stdint.h>   /* fixed-width integers */
#include <stdbool.h>  /* `bool` — used across the shim regardless of
                       * which header an inherited driver includes
                       * first (order-independent). */

/* ---- <linux/types.h> aliases ---- */

typedef uint8_t  __u8;
typedef uint16_t __u16;
typedef uint32_t __u32;
typedef uint64_t __u64;
typedef int8_t  __s8;
typedef int16_t __s16;
typedef int32_t __s32;
typedef int64_t __s64;

/* Kernel short-form scalar aliases (the in-tree names inherited
 * drivers use, distinct from the __-prefixed UAPI forms above). */
typedef uint8_t  u8;
typedef uint16_t u16;
typedef uint32_t u32;
typedef uint64_t u64;
typedef int8_t  s8;
typedef int16_t s16;
typedef int32_t s32;
typedef int64_t s64;

typedef uint32_t gfp_t;
typedef uint64_t dma_addr_t;
typedef int64_t  loff_t;

/* ---- <linux/gfp.h> ---- */

#define GFP_KERNEL  0x00000001U  /* may sleep; not from IRQ context */
#define GFP_ATOMIC  0x00000002U  /* must not sleep */
#define __GFP_ZERO  0x00000004U  /* zero-fill on alloc */
/*
 * Allocator modifier flags + GFP_NOWAIT. Arsenal-local bit values
 * (not Linux's), distinct so OR'd combinations stay unambiguous;
 * all advisory at M1 — slab.rs ignores the flags argument (the
 * GFP_KERNEL-from-IRQ "may sleep" distinction is documented there
 * and enforced at M2). balloon reaches these via slab.h: GFP_NOWAIT
 * on a fill-balloon inbuf, the three __GFP_* in its free-page
 * alloc-flag macro (virtio_balloon.c:33).
 */
#define __GFP_NORETRY    0x00000008U  /* don't retry / loop on failure */
#define __GFP_NOWARN     0x00000010U  /* suppress allocation-failure warning */
#define __GFP_NOMEMALLOC 0x00000020U  /* never dip into emergency reserves */
#define GFP_NOWAIT       0x00000040U  /* atomic, no reclaim, no warn-on-fail */

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

/* ---- <linux/dev_printk.h> ---- */

/* Device-scoped diagnostics. M1's printk takes no varargs (M1-2-1
 * deferred that to the first inherited driver demanding it), so
 * these discard the format + arguments and keep only the
 * side-effect-free device reference; the messages return once printk
 * grows a varargs entry point. */
#define dev_err(dev, ...)              ((void)(dev))
#define dev_warn(dev, ...)             ((void)(dev))
#define dev_info(dev, ...)             ((void)(dev))
#define dev_info_ratelimited(dev, ...) ((void)(dev))

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

typedef struct spinlock spinlock_t;

extern void spin_lock_init(struct spinlock *s);
extern void spin_lock(struct spinlock *s);
extern void spin_unlock(struct spinlock *s);

/* IRQ-saving variants. M1 simplification: virtio is polled (vq
 * callbacks run in process/polling context, not hard IRQ) and each
 * inherited driver's locks are private to it, so the local-IRQ
 * disable these provide in Linux is not needed for correctness yet —
 * the underlying spin_lock supplies the mutual exclusion. `flags` is
 * cleared rather than holding a saved IRQ state. Documented
 * simplification in the spirit of time.rs's busy-wait msleep; real
 * local_irq_save / _restore lands when an inherited driver takes a
 * lock in hard-IRQ context (a bridge fn, then). */
#define spin_lock_irqsave(lock, flags) \
    do { (flags) = 0; spin_lock(lock); } while (0)
#define spin_unlock_irqrestore(lock, flags) \
    do { (void)(flags); spin_unlock(lock); } while (0)
#define spin_lock_irq(lock)   spin_lock(lock)
#define spin_unlock_irq(lock) spin_unlock(lock)

/* ---- <linux/pm_wakeup.h> + <linux/device.h> wakeup ---- */

/* Power-management wakeup sources. Arsenal has no PM / suspend
 * subsystem at M1, so these are no-ops; balloon uses them to keep
 * the device awake while adjusting the balloon (a suspend-time
 * concern that does not exist yet). */
#define pm_stay_awake(dev) ((void)(dev))
#define pm_relax(dev)      ((void)(dev))
#define device_set_wakeup_capable(dev, capable) \
    do { (void)(dev); (void)(capable); } while (0)

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

struct virtio_config_ops;  /* defined just below virtio_device */

/* Trimmed virtio_device. Fields are the ones balloon (and the M1
 * inherited driver fleet) actually reach for. Layout matches
 * linuxkpi/src/virtio.rs's `pub struct virtio_device`. */
struct virtio_device {
    __u32 id_device;
    __u32 id_vendor;
    void *priv;
    /* PCI transport address (shim-internal). `pci_dev`, not `dev`,
     * so it doesn't collide with the embedded `struct device dev`
     * below that inherited drivers reach via `&vdev->dev`. */
    __u8  bus;
    __u8  pci_dev;
    __u8  func;
    __u8  _pad;
    void *common_cfg;
    void *notify_base;
    __u32 notify_off_multiplier;
    void *isr;
    void *device_cfg;
    /* virtio_config_ops vtable. The shim populates it at probe time
     * (M1-2-5-closing); balloon reads ->get + ->del_vqs. */
    const struct virtio_config_ops *config;
    /* Linux's embedded device. balloon takes `&vdev->dev`. */
    struct device dev;
    /* Negotiated feature bits. Populated by init_transport (the
     * M1-2-5 closing-commit lifecycle); virtio_has_feature reads it,
     * virtio_clear_bit / __virtio_clear_bit clear bits in it
     * (validate-time bit drops). */
    __u64 features;
};

/* Config-ops vtable. Trimmed to what balloon dereferences: ->get
 * (validate checks it is non-NULL) and ->del_vqs (remove teardown).
 * The full Linux vtable is ~15 ops; future drivers extend this. */
struct virtio_config_ops {
    void (*get)(struct virtio_device *vdev, unsigned int offset,
                void *buf, unsigned int len);
    void (*del_vqs)(struct virtio_device *vdev);
};

/* virtio_driver — Linux shape: name lives in the embedded
 * struct device_driver (.driver.name), per balloon's initializer.
 * Trimmed to balloon's used fields; the PM freeze/restore ops are
 * #ifdef CONFIG_PM_SLEEP in balloon and omitted here. */
struct device_driver {
    const char *name;
};

struct virtio_driver {
    struct device_driver driver;
    const struct virtio_device_id *id_table;
    const unsigned int *feature_table;
    unsigned int feature_table_size;
    int  (*validate)(struct virtio_device *dev);
    int  (*probe)(struct virtio_device *dev);
    void (*remove)(struct virtio_device *dev);
    void (*config_changed)(struct virtio_device *dev);
};

extern int  register_virtio_driver(struct virtio_driver *drv);
extern void unregister_virtio_driver(struct virtio_driver *drv);

extern __u8  virtio_cread8(const struct virtio_device *vdev, unsigned int offset);
extern __u16 virtio_cread16(const struct virtio_device *vdev, unsigned int offset);
extern __u32 virtio_cread32(const struct virtio_device *vdev, unsigned int offset);
extern void  virtio_cwrite8(struct virtio_device *vdev, unsigned int offset, __u8 val);
extern void  virtio_cwrite16(struct virtio_device *vdev, unsigned int offset, __u16 val);
extern void  virtio_cwrite32(struct virtio_device *vdev, unsigned int offset, __u32 val);

/* virtio feature bits (the ones balloon's validate touches; the
 * device-specific VIRTIO_BALLOON_F_* live in the UAPI header). */
#define VIRTIO_F_ACCESS_PLATFORM 33

/* Host/virtio endian conversion. Modern virtio (VIRTIO_F_VERSION_1)
 * is little-endian and Arsenal's targets are little-endian x86, so
 * these are identity; the vdev argument (which would select legacy
 * byte order) is ignored. Real on a big-endian port would branch on
 * vdev's negotiated VERSION_1 bit. */
#define cpu_to_virtio16(vdev, v) ((__u16)(v))
#define cpu_to_virtio32(vdev, v) ((__u32)(v))
#define cpu_to_virtio64(vdev, v) ((__u64)(v))
#define virtio16_to_cpu(vdev, v) ((__u16)(v))
#define virtio32_to_cpu(vdev, v) ((__u32)(v))
#define virtio64_to_cpu(vdev, v) ((__u64)(v))

/* Virtqueue. balloon reads ->vdev (owning device) and ->num_free
 * (available descriptors); priv holds shim-internal vring state the
 * M1-2-5-closing virtqueue impl populates. */
struct virtqueue {
    struct virtio_device *vdev;
    unsigned int          num_free;
    void                 *priv;
};

/* virtqueue_info + virtio_find_vqs live in <linux/virtio_config.h>
 * (their Linux home); the closing-commit impl + the
 * struct virtqueue_info definition are declared there. */

/* Virtqueue entry points — panic-on-call stubs (M1-2-3 / 2-5
 * iteration); real virtqueue machinery lands at the M1-2-5-closing
 * commit when virtio-balloon online demands it. */
extern int  virtqueue_add_outbuf(struct virtqueue *vq, const void *sg,
                                 unsigned int num, void *data,
                                 unsigned int gfp);
extern int  virtqueue_add_inbuf(struct virtqueue *vq, const void *sg,
                                unsigned int num, void *data,
                                unsigned int gfp);
extern int  virtqueue_kick(struct virtqueue *vq);
extern void *virtqueue_get_buf(struct virtqueue *vq, unsigned int *len);
extern unsigned int virtqueue_get_vring_size(const struct virtqueue *vq);

/* __virtio_clear_bit — clear a driver-side feature bit during
 * validate (lower-level than virtio_clear_bit). */
extern void __virtio_clear_bit(struct virtio_device *vdev, unsigned int fbit);

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

/* BUILD_BUG_ON — compile-time assertion. A true `cond` makes the
 * array size negative, which fails to compile. Pure compile-time,
 * no runtime cost. balloon: BUILD_BUG_ON(PAGE_SHIFT <
 * VIRTIO_BALLOON_PFN_SHIFT). */
#define BUILD_BUG_ON(cond) ((void)sizeof(char[1 - 2 * !!(cond)]))

/* ---- <linux/minmax.h> + <linux/kernel.h> + <linux/align.h> ---- */

/* Simple (non-type-checked) min/max — sufficient for the integer
 * comparisons inherited drivers use; Linux's strict-type versions
 * exist for safety, not behavior. */
#define min(a, b) ((a) < (b) ? (a) : (b))
#define max(a, b) ((a) > (b) ? (a) : (b))

#define ARRAY_SIZE(arr) (sizeof(arr) / sizeof((arr)[0]))

/* ALIGN / round_up assume `a` is a power of two (Linux's contract). */
#define ALIGN(x, a)    (((x) + (a) - 1) & ~((typeof(x))(a) - 1))
#define round_up(x, y) ((((x) + (y) - 1) / (y)) * (y))

/* ---- <linux/compiler.h> ---- */

#define likely(x)   __builtin_expect(!!(x), 1)
#define unlikely(x) __builtin_expect(!!(x), 0)

/* ---- <linux/string.h> ---- */

/* memset / memcpy / memmove resolve to the freestanding intrinsics
 * the kernel already provides (compiler_builtins for the Rust base);
 * declared here so inherited C can call them. */
extern void *memset(void *s, int c, size_t n);
extern void *memcpy(void *dst, const void *src, size_t n);
extern void *memmove(void *dst, const void *src, size_t n);

/* ---- <linux/bitops.h> + <asm/bitops.h> ---- */

/* Atomic test-and-set / test-and-clear of bit `nr` in the bitmap at
 * `addr`; return the previous bit value. Bodies in linuxkpi/src/
 * bitops.rs (panic-on-call this iteration; real atomic ops land at
 * the M1-2-5-closing commit when balloon's config-read path runs). */
extern int test_and_set_bit(long nr, volatile unsigned long *addr);
extern int test_and_clear_bit(long nr, volatile unsigned long *addr);

/* ---- <uapi/asm-generic/errno-base.h> + errno.h ---- */

/* Standard errno values (asm-generic, the x86 set). Inherited
 * drivers return these negated (`return -ENOMEM;`). */
#define EPERM    1
#define ENOENT   2
#define EINTR    4
#define EIO      5
#define EAGAIN  11
#define ENOMEM  12
#define EFAULT  14
#define EBUSY   16
#define ENODEV  19
#define EINVAL  22
#define ENOSPC  28
#define ENOSYS  38

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
#define list_first_entry(ptr, type, member) \
    list_entry((ptr)->next, type, member)
/* list_first_entry_or_null — first entry, or NULL on an empty list.
 * Evaluates `ptr` once (the statement-expression head__ binding).
 * balloon_page_pop relies on this. */
#define list_first_entry_or_null(ptr, type, member) ({ \
    struct list_head *head__ = (ptr); \
    struct list_head *first__ = head__->next; \
    first__ != head__ ? list_entry(first__, type, member) : NULL; })
/* Iterate safely against removal of the current entry — `n` holds
 * the next entry so `pos` can be deleted mid-loop. balloon uses it
 * to drain its page lists. */
#define list_for_each_entry_safe(pos, n, head, member) \
    for (pos = list_entry((head)->next, typeof(*pos), member), \
         n = list_entry(pos->member.next, typeof(*pos), member); \
         &pos->member != (head); \
         pos = n, n = list_entry(n->member.next, typeof(*n), member))

/* ---- <linux/limits.h> ---- */

#define ULONG_MAX (~0UL)

/* ---- <asm/page.h> ---- */

/* 4-KiB base page. Matches arsenal-kernel's frame size (the only
 * granule frames::FRAMES hands out). page_to_pfn shifts a struct
 * page's _phys by PAGE_SHIFT. */
#define PAGE_SHIFT 12
#define PAGE_SIZE  (1UL << PAGE_SHIFT)

/* Largest order the page allocator hands out (Linux 6.12 renamed
 * MAX_ORDER → MAX_PAGE_ORDER). balloon clamps its free-page hint
 * block order against this. */
#define MAX_PAGE_ORDER 10

/* ---- <linux/vmstat.h> ---- */

/* No VM event / node-state accounting at M1 — the stats balloon
 * reports to the host are informational, so global_node_page_state
 * returns 0 (NR_FILE_PAGES is the only item balloon reads). */
#define NR_FILE_PAGES 0
#define global_node_page_state(item) ((unsigned long)0)

/* ---- <linux/mm.h> page-init / poisoning ---- */

/* Arsenal has neither init-on-free nor page poisoning at M1; both
 * report disabled. PAGE_POISON is the byte balloon would memset with
 * if poisoning were on (Linux's default 0xaa), kept for the
 * sizeof/memset call shape. */
#define want_init_on_free()             0
#define page_poisoning_enabled_static() 0
#define PAGE_POISON 0xaa

/* ---- <linux/mm_types.h> ---- */

/* struct page — thin per-frame handle per ADR-0007. NOT Linux's
 * mem_map array element; a small descriptor allocated alongside the
 * physical frame it represents. Inherited drivers touch only `lru`
 * (via the list helpers above); the _-prefixed fields are shim-
 * internal — _phys backs page_to_pfn / page_address, _refcount backs
 * get_page / put_page, _private mirrors Linux's page.private. Filled
 * by linuxkpi/src/page.rs; layout must stay in sync with the
 * #[repr(C)] mirror there. */
struct page {
    struct list_head lru;
    unsigned long    _phys;
    int              _refcount;
    void            *_private;
};

/* ---- <linux/scatterlist.h> ---- */

/* Linux's scatterlist shape (field names preserved for future
 * drivers using sg_set_page / sg_page); balloon only ever builds a
 * single-entry list via sg_init_one. The body of sg_init_one lives
 * in linuxkpi/src/virtio.rs as a panic-on-call stub — the M1-2-5-
 * closing commit settles the scatterlist representation together
 * with the real virtqueue_add_* it feeds. */
struct scatterlist {
    unsigned long page_link;
    unsigned int  offset;
    unsigned int  length;
    dma_addr_t    dma_address;
    unsigned int  dma_length;
};

extern void sg_init_one(struct scatterlist *sg, const void *buf,
                        unsigned int buflen);

/* ---- <linux/shrinker.h> ---- */

/* Memory-reclaim shrinker. Arsenal has no reclaim subsystem at M1;
 * shrinker_alloc / _free / _register are panic-on-call stubs in
 * linuxkpi/src/mm.rs. balloon registers a shrinker only under
 * VIRTIO_BALLOON_F_FREE_PAGE_HINT, which the M1 smoke device does
 * not negotiate. struct shrinker carries the callbacks + private
 * data balloon assigns after alloc. */
struct shrink_control {
    gfp_t         gfp_mask;
    unsigned long nr_to_scan;
};

struct shrinker {
    unsigned long (*count_objects)(struct shrinker *s, struct shrink_control *sc);
    unsigned long (*scan_objects)(struct shrinker *s, struct shrink_control *sc);
    void *private_data;
};

/* shrinker_alloc is __printf(2,3) varargs in Linux; balloon passes
 * only the bare name, so the non-varargs declaration matches its
 * call site. */
extern struct shrinker *shrinker_alloc(unsigned int flags, const char *name);
extern void shrinker_free(struct shrinker *shrinker);
extern void shrinker_register(struct shrinker *shrinker);

/* ---- <linux/notifier.h> ---- */

/* Notifier-chain callback. Forward-declare struct notifier_block
 * ahead of the typedef so the function-pointer parameter doesn't
 * trip clang's -Wvisibility (cf. the workqueue.h fix). */
struct notifier_block;
typedef int (*notifier_fn_t)(struct notifier_block *nb,
                             unsigned long action, void *data);

struct notifier_block {
    notifier_fn_t          notifier_call;
    struct notifier_block *next;   /* chain linkage; managed by register */
    int                    priority;
};

/* Notifier return codes — balloon's OOM callback returns NOTIFY_OK. */
#define NOTIFY_DONE 0x0000
#define NOTIFY_OK   0x0001

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
