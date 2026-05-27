/* SPDX-License-Identifier: BSD-2-Clause */
/*
 * <linux/types.h> — BSD-2 Arsenal-authored reimplementation per
 * ADR-0006. Declares the fixed-width kernel integer aliases
 * (__u8/__u16/__u32/__u64 + signed variants) and protocol-related
 * typedefs (gfp_t, dma_addr_t, loff_t) that inherited drivers
 * reach when they #include <linux/types.h>.
 *
 * Substantive declarations live in linuxkpi/include/shim_c.h.
 */

#ifndef ARSENAL_LINUXKPI_LINUX_TYPES_H
#define ARSENAL_LINUXKPI_LINUX_TYPES_H

#include "../shim_c.h"

/* Sparse annotations — Linux uses these for static-analysis hints
 * (endianness tagging, userspace-pointer marking, integer-class
 * separation). The kernel itself defines them as no-ops outside
 * sparse builds; we follow the same convention. */
#ifndef __bitwise
#define __bitwise
#endif
#ifndef __force
#define __force
#endif
#ifndef __user
#define __user
#endif
#ifndef __kernel
#define __kernel
#endif
#ifndef __iomem
#define __iomem
#endif

#endif /* ARSENAL_LINUXKPI_LINUX_TYPES_H */
