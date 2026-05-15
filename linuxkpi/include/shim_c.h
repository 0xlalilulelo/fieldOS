/* SPDX-License-Identifier: BSD-2-Clause */

/*
 * LinuxKPI shim — C-callable declarations of Rust shim functions
 * that inherited Linux 6.12 LTS drivers under vendor/linux-6.12/
 * link against. See docs/adrs/0005-linuxkpi-shim-layout.md for the
 * bidirectional-FFI rationale and the hand-written-header decision.
 *
 * At M1-2-0 this header is an empty placeholder. Foundational
 * declarations (printk, kmalloc, kfree, mutex_lock, mutex_unlock,
 * spin_lock, spin_unlock, atomic_*) land at M1-2-1; PCI bus +
 * request_irq + DMA at M1-2-2; virtio bus at M1-2-3; cc-driven
 * compilation of inherited C against this header at M1-2-4.
 */

#ifndef ARSENAL_LINUXKPI_SHIM_C_H
#define ARSENAL_LINUXKPI_SHIM_C_H

#endif /* ARSENAL_LINUXKPI_SHIM_C_H */
