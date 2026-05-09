// SPDX-License-Identifier: BSD-2-Clause

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use limine::BaseRevision;

mod serial;

#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    // If the bootloader doesn't support the base revision the limine
    // crate was compiled against, hang silently — emitting the sentinel
    // would lie about success.
    if !BASE_REVISION.is_supported() {
        halt();
    }

    serial::init();
    serial::write_str("ARSENAL_BOOT_OK\n");

    halt();
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    halt();
}

fn halt() -> ! {
    loop {
        // SAFETY: `hlt` is a privileged instruction with no side effects beyond
        // halting the CPU until the next interrupt. We are in ring 0 (entered
        // from Limine) and the loop ensures we re-halt on spurious wakes.
        unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)) }
    }
}
