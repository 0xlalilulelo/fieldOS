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
//! `pci_iomap` / `pci_set_master` / `pci_enable_device` /
//! `pci_alloc_irq_vectors` / `pci_irq_vector` /
//! `pci_free_irq_vectors`. IRQ bridge (`irq`) — 16-slot
//! pre-generated dispatcher pool, `request_irq` / `free_irq`,
//! `register_dispatchers` init for `arsenal-kernel` to call at
//! boot. DMA coherent (`dma`) — `dma_alloc_coherent` over the
//! frame allocator + no-op `dma_map_*` / `dma_sync_*` reflecting
//! x86_64's cache-coherent DMA model.
//!
//! Added at M1-2-3: virtio bus adapter (`virtio`) — `struct
//! virtio_driver` registration + .probe dispatch over the
//! virtio-modern transport in arsenal-kernel; virtio_cread /
//! virtio_cwrite typed accessors over device_cfg; virtqueue
//! type + find_vqs / virtqueue_add_outbuf / virtqueue_kick /
//! virtqueue_get_buf as panic-on-call stubs (real impls land
//! at M1-2-5 when virtio-balloon's first call demands them).
//! Closes the "shim foundation" devlog cluster.
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
#![feature(abi_x86_interrupt)]

pub mod dma;
pub mod irq;
pub mod locks;
pub mod log;
pub mod pci;
pub mod slab;
pub mod types;
pub mod virtio;

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

    // M1-2-4 cc-build smoke: call linuxkpi_cc_smoke (defined in
    // linuxkpi/csrc/smoke.c). Validates that the cc-crate-driven
    // C compile path produces a linkable object whose symbol
    // resolution against the Rust shim works end-to-end. The C
    // call into printk + kmalloc/kfree exercises the FFI loop in
    // both directions (Rust<->C<->Rust).
    unsafe extern "C" {
        fn linuxkpi_cc_smoke();
    }
    // SAFETY: linuxkpi_cc_smoke is the BSD-2 smoke harness from
    // linuxkpi/csrc/smoke.c; it has void(void) signature and
    // calls back into shim functions whose contracts hold here.
    unsafe { linuxkpi_cc_smoke() };

    // M1-2-2 IRQ bridge: request_irq / free_irq round-trip. The
    // dispatcher pool was registered in arsenal-kernel/src/main.rs
    // via linuxkpi::irq::register_dispatchers(idt::register_vector);
    // here we just exercise the slot bookkeeping (no actual IRQ
    // delivery — that lands at M1-2-5 with virtio-balloon's first
    // real interrupt).
    {
        static IRQ_FIRES: locks::AtomicInt = locks::AtomicInt::new(0);
        unsafe extern "C" fn noop_handler(
            _irq: types::c_int,
            _dev_id: *mut types::c_void,
        ) -> types::c_int {
            IRQ_FIRES.inc();
            irq::IRQ_HANDLED
        }
        // Use slot 15 (the highest pre-allocated slot) so we don't
        // collide with whatever pci_alloc_irq_vectors will hand out
        // at 2-5 (which starts from slot 0). This is a registration
        // round-trip, not an IRQ delivery test.
        let test_irq: types::c_uint = (irq::SLOT_COUNT - 1) as types::c_uint;
        let dummy_dev_id = 0xDEAD_BEEFu64 as *mut types::c_void;
        // SAFETY: noop_handler is extern "C" + the right signature;
        // dummy_dev_id is opaque. Slot 15 is in 0..SLOT_COUNT.
        let rc = unsafe {
            irq::request_irq(
                test_irq,
                noop_handler,
                0,
                c"linuxkpi-irq-self-test".as_ptr(),
                dummy_dev_id,
            )
        };
        assert_eq!(rc, 0, "request_irq failed with rc={rc}");
        // SAFETY: paired with the request_irq above.
        let returned_dev_id = unsafe { irq::free_irq(test_irq, dummy_dev_id) };
        assert_eq!(
            returned_dev_id, dummy_dev_id,
            "free_irq returned wrong dev_id (got {returned_dev_id:p}, expected {dummy_dev_id:p})"
        );
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

    // M1-2-3: virtio bus walk + no-op virtio_driver self-test.
    // QEMU's smoke command line has virtio-blk + virtio-net +
    // virtio-rng (3 functions); a no-op virtio_driver claiming
    // VIRTIO_DEV_ANY_ID should see .probe fire 3 times.
    let virtio_count = virtio::count_present();
    log::pr(b"linuxkpi: virtio walk found ");
    log_decimal(virtio_count);
    log::pr(b" present devices\n");
    assert!(
        virtio_count >= 3,
        "linuxkpi: expected >= 3 virtio devices (blk + net + rng); got {virtio_count}"
    );

    {
        static VIRTIO_MATCH_COUNT: locks::AtomicInt = locks::AtomicInt::new(0);
        unsafe extern "C" fn noop_virtio_probe(
            _vdev: *mut virtio::virtio_device,
        ) -> types::c_int {
            VIRTIO_MATCH_COUNT.inc();
            -1 // signal "did not bind" (Linux convention: negative
               // probe return means the driver declined)
        }
        static VIRTIO_ID_TABLE: [virtio::virtio_device_id; 2] = [
            virtio::virtio_device_id {
                device: virtio::VIRTIO_DEV_ANY_ID,
                vendor: virtio::VIRTIO_DEV_ANY_ID,
            },
            virtio::virtio_device_id { device: 0, vendor: 0 },
        ];
        let mut driver = virtio::virtio_driver {
            name: c"linuxkpi-virtio-self-test".as_ptr(),
            id_table: VIRTIO_ID_TABLE.as_ptr(),
            probe: Some(noop_virtio_probe),
            remove: None,
        };
        // SAFETY: driver lives on this stack for the registration
        // window; register_virtio_driver dispatches probe
        // synchronously and unregister clears the registry entry
        // before the stack frame ends.
        unsafe {
            let rc = virtio::register_virtio_driver(
                &mut driver as *mut virtio::virtio_driver,
            );
            assert_eq!(rc, 0, "register_virtio_driver returned non-zero");
        }
        let matches = VIRTIO_MATCH_COUNT.read() as usize;
        assert_eq!(
            matches, virtio_count,
            "no-op virtio_driver should match every present device (got {matches}, expected {virtio_count})"
        );
        // SAFETY: pairs with the register above.
        unsafe {
            virtio::unregister_virtio_driver(&mut driver as *mut virtio::virtio_driver);
        }
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
