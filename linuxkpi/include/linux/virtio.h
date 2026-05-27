/* SPDX-License-Identifier: BSD-2-Clause */
/*
 * <linux/virtio.h> — BSD-2 Arsenal-authored reimplementation per
 * ADR-0006. Declares the virtio bus surface (struct virtio_device,
 * struct virtio_driver, virtqueue, register_virtio_driver, the
 * virtio_cread/cwrite/find_vqs entry points) that inherited Linux
 * drivers reach when they #include <linux/virtio.h>.
 *
 * The substantive declarations live in linuxkpi/include/shim_c.h
 * (the M1-2-1-era catch-all); this per-header file delegates so
 * that include-path discipline matches what balloon.c expects.
 * A later refactor will split shim_c.h into per-header files; the
 * per-header API surface seen from inherited C is unchanged.
 */

#ifndef ARSENAL_LINUXKPI_LINUX_VIRTIO_H
#define ARSENAL_LINUXKPI_LINUX_VIRTIO_H

#include "../shim_c.h"

#endif /* ARSENAL_LINUXKPI_LINUX_VIRTIO_H */
