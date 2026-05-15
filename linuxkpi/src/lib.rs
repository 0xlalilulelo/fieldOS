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
//! Added at M1-2-2: PCI bus adapter (`pci`) — `struct pci_driver`
//! registration + `.probe` dispatch + `pci_resource_*` /
//! `pci_iomap` / `pci_set_master` / `pci_enable_device`. DMA
//! coherent (`dma`) — `dma_alloc_coherent` over the frame
//! allocator + no-op `dma_map_*` / `dma_sync_*` reflecting
//! x86_64's cache-coherent DMA model. IRQ bridge
//! (`pci_alloc_irq_vectors`, `request_irq`) is the next
//! follow-up in M1-2-2.
//!
//! Surfaces deferred to later sub-blocks:
//! - virtio bus adapter: M1-2-3.
//! - `cc`-driven build of inherited C + first `vendor/linux-6.12/`
//!   subset: M1-2-4.
//! - virtio-balloon online: M1-2-5.
//! - `container_of` / `BUG_ON` / `WARN_ON` macros, `jiffies` /
//!   `msleep` / `udelay`, `copy_*_user` stubs: lands when the first
//!   inherited driver demands them (typically M1-2-5).

#![no_std]

pub mod dma;
pub mod locks;
pub mod log;
pub mod pci;
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

    // M1-2-2: PCI bus walk. Validates the linuxkpi side sees the
    // same set of PCI functions arsenal-kernel's pci::scan saw at
    // boot — same brute-force walk, same CF8/CFC reads via the
    // bridge externs.
    let pci_count = pci::count_present();
    log::pr(b"linuxkpi: pci walk found ");
    log_decimal(pci_count);
    log::pr(b" present functions\n");
    assert!(pci_count > 0, "linuxkpi: pci walk found zero present functions; bridge externs may be misrouted");

    // M1-2-2: no-op pci_driver self-test. Registers a driver with
    // PCI_ANY_ID match, sees its .probe() called once per
    // discovered function (just counts), unregisters cleanly.
    {
        static MATCH_COUNT: locks::AtomicInt = locks::AtomicInt::new(0);
        unsafe extern "C" fn noop_probe(
            _dev: *mut pci::pci_dev,
            _id: *const pci::pci_device_id,
        ) -> types::c_int {
            MATCH_COUNT.inc();
            -1 // -1 = "did not bind" so the registry doesn't track
        }
        static ID_TABLE: [pci::pci_device_id; 2] = [
            pci::pci_device_id {
                vendor: pci::PCI_ANY_ID,
                device: pci::PCI_ANY_ID,
                subvendor: pci::PCI_ANY_ID,
                subdevice: pci::PCI_ANY_ID,
                class: 0,
                class_mask: 0,
                driver_data: 0,
            },
            pci::pci_device_id {
                vendor: 0, device: 0, subvendor: 0, subdevice: 0,
                class: 0, class_mask: 0, driver_data: 0,
            },
        ];
        // SAFETY: probe is non-null + extern "C"; id_table is
        // 'static + NULL-sentinel-terminated; name is 'static.
        let mut driver = pci::pci_driver {
            name: c"linuxkpi-self-test".as_ptr(),
            id_table: ID_TABLE.as_ptr(),
            probe: Some(noop_probe),
            remove: None,
        };
        // SAFETY: driver lives on this stack for the registration
        // window; pci_register_driver dispatches probe synchronously
        // and pci_unregister_driver clears the registry entry before
        // the stack frame ends.
        unsafe {
            let rc = pci::pci_register_driver(&mut driver as *mut pci::pci_driver);
            assert_eq!(rc, 0, "pci_register_driver returned non-zero");
        }
        let matches = MATCH_COUNT.read() as usize;
        assert_eq!(
            matches, pci_count,
            "no-op pci_driver should match every present function (got {matches}, expected {pci_count})"
        );
        // SAFETY: pairs with the register above.
        unsafe { pci::pci_unregister_driver(&mut driver as *mut pci::pci_driver) };
    }

    // M1-2-2: dma_alloc_coherent round-trip. Allocates one frame,
    // checks the dma_handle is page-aligned, writes a tag through
    // the CPU-virtual pointer, frees.
    unsafe {
        let mut handle: types::dma_addr_t = 0;
        let cpu_addr = dma::dma_alloc_coherent(
            core::ptr::null_mut(),
            512,
            &mut handle as *mut types::dma_addr_t,
            slab::GFP_KERNEL,
        );
        assert!(!cpu_addr.is_null(), "dma_alloc_coherent returned NULL");
        assert_eq!(handle & 0xFFF, 0, "dma_handle {handle:#x} is not page-aligned");
        let p = cpu_addr as *mut u32;
        core::ptr::write_volatile(p, 0xDEAD_BEEF);
        let v = core::ptr::read_volatile(p);
        assert_eq!(v, 0xDEAD_BEEF, "dma_alloc_coherent buffer is not coherent CPU<->CPU");
        dma::dma_free_coherent(core::ptr::null_mut(), 512, cpu_addr, handle);
    }

    log::pr(b"ARSENAL_LINUXKPI_OK\n");
}

/// Tiny decimal-printer for the self-test's "found N functions"
/// line. Avoids pulling alloc::format!'s machinery into the shim.
fn log_decimal(mut n: usize) {
    if n == 0 {
        log::pr(b"0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    log::pr(&buf[i..]);
}
