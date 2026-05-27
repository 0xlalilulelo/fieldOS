/* SPDX-License-Identifier: BSD-2-Clause */
/*
 * <linux/virtio_config.h> — BSD-2 Arsenal-authored reimplementation
 * per ADR-0006. Declares the configuration / status / feature
 * helpers that inherited Linux drivers reach when they #include
 * <linux/virtio_config.h>.
 *
 * Upstream Linux ships most of these as `static inline` functions
 * dispatching through a `config_ops` vtable on `struct
 * virtio_device`. The Arsenal shim flattens the dispatch: the
 * byte-level cread/cwrite primitives are extern Rust functions
 * (declared in shim_c.h, implemented in linuxkpi/src/virtio.rs);
 * the type-erased cread_le / cwrite_le macros below dispatch on
 * sizeof(*ptr) to those primitives.
 *
 * The status/feature helpers (virtio_has_feature,
 * virtio_device_ready, virtio_reset_device, virtio_clear_bit) are
 * extern functions for now — panic-on-call stubs land at this
 * commit; real implementations land in the M1-2-5-closing commit
 * alongside balloon.c entering the build.rs source manifest. The
 * panic-on-call discipline (vs silent no-op) matches the
 * established shim pattern: a missing implementation is a link-
 * time presence + runtime panic, never a silent correctness bug.
 */

#ifndef ARSENAL_LINUXKPI_LINUX_VIRTIO_CONFIG_H
#define ARSENAL_LINUXKPI_LINUX_VIRTIO_CONFIG_H

#include "../shim_c.h"
#include <stdbool.h>  /* `bool` for virtio_has_feature's return type
                       * and the const bool *ctx parameter on
                       * virtio_find_vqs. */

/* Status / feature helpers — extern declarations. Implementations
 * live in linuxkpi/src/virtio.rs. */
extern bool virtio_has_feature(const struct virtio_device *vdev, unsigned int fbit);
extern void virtio_device_ready(struct virtio_device *vdev);
extern void virtio_reset_device(struct virtio_device *vdev);
extern void virtio_clear_bit(struct virtio_device *vdev, unsigned int fbit);

/* Type-erased config-space accessors. Dispatch on sizeof(*ptr) to
 * the size-specific extern primitives already in shim_c.h. The
 * little-endian flavor (_le) is the modern-virtio convention; on
 * x86_64 the dispatched primitives are already CPU-order so the
 * macro is endian-correct without further work. A future big-
 * endian port will add the byteswap inside the size cases. */
#define virtio_cread_le(vdev, structname, member, ptr) do {              \
    switch (sizeof(*(ptr))) {                                            \
    case 1: *(__u8  *)(ptr) =                                            \
        virtio_cread8 ((vdev), offsetof(structname, member)); break;     \
    case 2: *(__u16 *)(ptr) =                                            \
        virtio_cread16((vdev), offsetof(structname, member)); break;     \
    case 4: *(__u32 *)(ptr) =                                            \
        virtio_cread32((vdev), offsetof(structname, member)); break;     \
    default:                                                             \
        linuxkpi_bug(__FILE__, __LINE__,                                 \
            "virtio_cread_le: unsupported sizeof(*ptr) — "               \
            "add a virtio_cread64 extern if balloon needs 8-byte reads");\
    }                                                                    \
} while (0)

#define virtio_cwrite_le(vdev, structname, member, ptr) do {             \
    switch (sizeof(*(ptr))) {                                            \
    case 1: virtio_cwrite8 ((vdev), offsetof(structname, member),        \
                            *(const __u8  *)(ptr)); break;               \
    case 2: virtio_cwrite16((vdev), offsetof(structname, member),        \
                            *(const __u16 *)(ptr)); break;               \
    case 4: virtio_cwrite32((vdev), offsetof(structname, member),        \
                            *(const __u32 *)(ptr)); break;               \
    default:                                                             \
        linuxkpi_bug(__FILE__, __LINE__,                                 \
            "virtio_cwrite_le: unsupported sizeof(*ptr) — "              \
            "add a virtio_cwrite64 extern if balloon needs 8-byte writes");\
    }                                                                    \
} while (0)

/* virtio_find_vqs — Linux's signature has more parameters than the
 * M1-2-3 find_vqs panic-stub in shim_c.h (callbacks, ctx,
 * irq_affinity). Declared here in its Linux shape; the
 * M1-2-5-closing commit will reconcile the extern signature with
 * the implementation in linuxkpi/src/virtio.rs. For now, the
 * declaration lets balloon.c compile; the link-time symbol will
 * resolve once the closing commit lands the impl. */
typedef void (*vq_callback_t)(struct virtqueue *vq);
struct irq_affinity;
extern int virtio_find_vqs(struct virtio_device *vdev, unsigned int nvqs,
                           struct virtqueue *vqs[], vq_callback_t *callbacks[],
                           const char *const names[], const bool *ctx,
                           struct irq_affinity *desc);

#endif /* ARSENAL_LINUXKPI_LINUX_VIRTIO_CONFIG_H */
