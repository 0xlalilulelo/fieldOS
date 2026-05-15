/* SPDX-License-Identifier: BSD-2-Clause */

/*
 * M1-2-4 cc-build smoke. Validates that the cc-crate-driven C
 * compile path produces an object file that:
 *
 *   1. parses include/shim_c.h cleanly under the kernel cross-
 *      compile flag set (-nostdinc -ffreestanding -mno-red-zone
 *      -mcmodel=kernel -x c — see linuxkpi/build.rs);
 *
 *   2. references Rust shim symbols (printk + kmalloc + kfree)
 *      that arsenal-kernel's ELF link resolves to the
 *      #[unsafe(no_mangle)] pub extern "C" fn definitions in
 *      linuxkpi/src/{log,slab}.rs;
 *
 *   3. exposes one symbol the Rust self-test calls
 *      (`linuxkpi_cc_smoke`), proving the Rust<->C FFI loop is
 *      wired end-to-end.
 *
 * vendor/linux-6.12/drivers/virtio/virtio_balloon.c is NOT yet
 * in linuxkpi/build.rs's source manifest; it lands at M1-2-5
 * with the gap-filling sub-block per HANDOFF M1-2-5.
 */

#include "shim_c.h"

void linuxkpi_cc_smoke(void) {
    /* KERN_INFO + literal string: the C preprocessor concatenates
     * the SOH+"6" prefix with the message at compile time, so
     * printk receives a single NUL-terminated literal. The Rust
     * shim's strip_kern_level path detects the prefix and emits
     * the [INFO] tag. */
    printk(KERN_INFO "linuxkpi: cc-build smoke ok\n");

    /* kmalloc + kfree round-trip — exercises the slab shim's
     * full path through alloc::alloc::alloc + the Header layout
     * recovery in kfree. 128 bytes is arbitrary; the path is
     * size-independent. */
    void *p = kmalloc(128, GFP_KERNEL);
    if (p) {
        kfree(p);
    }
}
