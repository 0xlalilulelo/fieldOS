/* SPDX-License-Identifier: BSD-2-Clause */
/*
 * <linux/module.h> — BSD-2 Arsenal-authored reimplementation per
 * ADR-0006. The module-machinery surface inherited drivers reach
 * when they #include <linux/module.h>.
 *
 * Arsenal has no loadable-module loader: drivers are inherited
 * statically and initialized synchronously at boot (ADR-0005 § 6
 * "synchronous module init at M1"). So the metadata macros
 * (MODULE_*, MODULE_DEVICE_TABLE) are no-ops — in Linux they emit
 * modpost-only sections — and the __init / __exit section markers
 * are empty (no .init.text discard at M1).
 *
 * module_init / module_exit are no-ops *here*: the module_driver /
 * module_virtio_driver chain still defines the standard static
 * <driver>_init / <driver>_exit wrapper functions (so the register
 * call's signature is validated at compile time and the eventual
 * init has a well-known shape), but nothing auto-invokes them yet.
 *
 * DEFERRED DESIGN DECISION (M1-2-5-closing commit): how the
 * synchronous boot actually reaches balloon's init — an explicit
 * call by symbol name from arsenal-kernel, or an initcall-style
 * table the kernel walks. That choice is what lights
 * ARSENAL_VIRTIO_BALLOON_OK; it is not made in this header.
 */

#ifndef ARSENAL_LINUXKPI_LINUX_MODULE_H
#define ARSENAL_LINUXKPI_LINUX_MODULE_H

#include "../shim_c.h"

/* Section markers — empty at M1 (no .init/.exit text discard). */
#define __init
#define __exit

/* Module metadata — modpost-only in Linux; no-ops here. The string
 * argument is evaluated for syntax but discarded. balloon uses
 * MODULE_DEVICE_TABLE / MODULE_DESCRIPTION / MODULE_LICENSE; the
 * sibling metadata macros round out the surface for future inherited
 * drivers, all identically no-op. (balloon's MODULE_LICENSE("GPL")
 * is honest metadata; Arsenal's GPL boundary is enforced by the
 * vendor/linux-6.12/ directory fence, not by this macro.) */
#define MODULE_DEVICE_TABLE(type, name)
#define MODULE_DESCRIPTION(desc)
#define MODULE_LICENSE(license)
#define MODULE_AUTHOR(author)
#define MODULE_VERSION(version)
#define MODULE_ALIAS(alias)

/* module_init / module_exit — no auto-invocation at M1. The wrapper
 * functions module_driver defines below are referenced only by the
 * closing-commit init mechanism (see the DEFERRED DESIGN DECISION
 * note above); marked here so -Wunused doesn't fire on them. */
#define module_init(initfn)
#define module_exit(exitfn)

/* module_driver(drv, register, unregister) — defines the standard
 * static <drv>_init / <drv>_exit wrappers that (un)register the
 * driver, then hands them to the no-op module_init / module_exit.
 * Mirrors Linux's macro shape so the wrapper names + signatures
 * match what the closing-commit init mechanism will look for. */
#define module_driver(__driver, __register, __unregister)            \
    static int __init __driver##_init(void)                          \
    {                                                                \
        return __register(&(__driver));                              \
    }                                                                \
    module_init(__driver##_init);                                    \
    static void __exit __driver##_exit(void)                         \
    {                                                                \
        __unregister(&(__driver));                                   \
    }                                                                \
    module_exit(__driver##_exit)

/* module_virtio_driver(drv) — the virtio-bus specialization balloon
 * uses (virtio_balloon.c:1220). Dispatches to module_driver with the
 * register_virtio_driver / unregister_virtio_driver entry points
 * declared in shim_c.h. */
#define module_virtio_driver(__virtio_driver) \
    module_driver(__virtio_driver, register_virtio_driver, unregister_virtio_driver)

#endif /* ARSENAL_LINUXKPI_LINUX_MODULE_H */
