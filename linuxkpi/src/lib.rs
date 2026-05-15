// SPDX-License-Identifier: BSD-2-Clause

//! LinuxKPI shim — Rust adapters that present a Linux-kernel-shaped
//! API surface to inherited Linux 6.12 LTS drivers vendored under
//! `vendor/linux-6.12/`. See `docs/adrs/0005-linuxkpi-shim-layout.md`
//! for the structural commitments: single-crate layout, BSD-2 /
//! GPLv2 directory boundary, `cc`-crate-driven C build, minimal
//! hand-curated header subset, bidirectional FFI with hand-written
//! `include/shim_c.h`, synchronous module init at M1.
//!
//! Surfaces present at M1-2-1: foundational types (`types`), `printk`
//! + `pr_*` (`log`), `kmalloc` / `kfree` / `kzalloc` / `krealloc`
//!   (`slab`), `atomic_t` / `mutex` / `spinlock` (`locks`).
//!
//! Surfaces deferred to later sub-blocks:
//! - PCI bus adapter + `request_irq` + DMA: M1-2-2.
//! - virtio bus adapter: M1-2-3.
//! - `cc`-driven build of inherited C + first `vendor/linux-6.12/`
//!   subset: M1-2-4.
//! - virtio-balloon online: M1-2-5.
//! - `container_of` / `BUG_ON` / `WARN_ON` macros, `jiffies` /
//!   `msleep` / `udelay`, `copy_*_user` stubs: lands when the first
//!   inherited driver demands them (typically M1-2-2 / M1-2-5).

#![no_std]

pub mod locks;
pub mod log;
pub mod slab;
pub mod types;

/// Self-test exercising the foundational shim primitives. Called
/// from `arsenal-kernel/src/main.rs` during boot. Touches `printk`
/// (Rust-side and C-callable variants), `kmalloc`/`kfree` round-
/// trip, `Mutex<T>::lock`, and `AtomicInt::inc`/`read` in sequence.
/// Emits `ARSENAL_LINUXKPI_OK` on success; panics on any check
/// failure (panic propagates to the kernel's panic handler).
pub fn self_test() {
    log::pr(b"linuxkpi: self-test starting\n");

    // printk via the C-callable entry point with a literal C string.
    // The b"...\0" sigil + as_ptr() dance is how a Rust caller
    // hands a C-shaped string to extern "C" fn printk; inherited C
    // will call this same entry point with format strings whose
    // KERN_* prefix we detect.
    //
    // Linux's KERN_INFO is the two-byte sequence SOH (0x01) +
    // ASCII '6' (0x36) — the ASCII digit, not the integer 6.
    // shim_c.h's `#define KERN_INFO KERN_SOH "6"` gives the C side
    // the same bytes via string concatenation; the Rust literal
    // here writes them inline as `\x016`.
    let info_msg = b"\x016linuxkpi: printk via C entry\n\0";
    // SAFETY: info_msg is a NUL-terminated byte string with the
    // KERN_INFO prefix; printk's contract requires NUL-termination,
    // which holds.
    unsafe {
        log::printk(info_msg.as_ptr() as *const types::c_char);
    }

    // kmalloc / kfree round-trip with a small allocation.
    // SAFETY: kmalloc/kfree are paired; the buffer is written
    // before the read, and freed exactly once.
    unsafe {
        let p = slab::kmalloc(64, slab::GFP_KERNEL);
        if p.is_null() {
            panic!("linuxkpi self-test: kmalloc(64, GFP_KERNEL) returned NULL");
        }
        // Touch the memory to validate it's writable.
        let bytes = p as *mut u8;
        for i in 0..64u8 {
            core::ptr::write(bytes.add(i as usize), i);
        }
        for i in 0..64u8 {
            let v = core::ptr::read(bytes.add(i as usize));
            assert_eq!(v, i, "kmalloc payload mismatch at offset {i}");
        }
        slab::kfree(p);
    }

    // kzalloc — zero-fill validation.
    // SAFETY: kzalloc/kfree are paired.
    unsafe {
        let p = slab::kzalloc(32, slab::GFP_KERNEL) as *const u8;
        if p.is_null() {
            panic!("linuxkpi self-test: kzalloc(32, GFP_KERNEL) returned NULL");
        }
        for i in 0..32 {
            let v = core::ptr::read(p.add(i));
            assert_eq!(v, 0, "kzalloc returned non-zero byte at offset {i}");
        }
        slab::kfree(p as *const types::c_void);
    }

    // Mutex<T>::lock round-trip — drop releases.
    {
        let m = locks::Mutex::new(0u32);
        let mut g = m.lock();
        *g = 0xCAFE_F00D;
        assert_eq!(*g, 0xCAFE_F00D);
    }

    // AtomicInt — inc/read/dec.
    let a = locks::AtomicInt::new(0);
    a.inc();
    a.inc();
    a.inc();
    assert_eq!(a.read(), 3, "AtomicInt::inc x3 should yield 3");
    a.dec();
    assert_eq!(a.read(), 2, "AtomicInt::dec should yield 2");

    // C-callable mutex round-trip — initialize, lock, unlock.
    let mut cm = locks::mutex::new();
    // SAFETY: cm is on this stack frame for the duration of the
    // lock/unlock pair.
    unsafe {
        locks::mutex_lock(&mut cm);
        locks::mutex_unlock(&mut cm);
    }

    log::pr(b"ARSENAL_LINUXKPI_OK\n");
}
