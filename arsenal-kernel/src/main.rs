// SPDX-License-Identifier: BSD-2-Clause

#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
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
