// SPDX-License-Identifier: BSD-2-Clause

//! LinuxKPI shim — Rust adapters that present a Linux-kernel-shaped
//! API surface to inherited Linux 6.12 LTS drivers vendored under
//! `vendor/linux-6.12/`. See `docs/adrs/0005-linuxkpi-shim-layout.md`
//! for the structural commitments: single-crate layout, BSD-2 /
//! GPLv2 directory boundary, `cc`-crate-driven C build, minimal
//! hand-curated header subset, bidirectional FFI with hand-written
//! `include/shim_c.h`, synchronous module init at M1.
//!
//! At M1-2-0 this crate is an empty skeleton — no shim API surface
//! is implemented yet. Foundational types + printk + slab + locks +
//! atomics land at M1-2-1.

#![no_std]
