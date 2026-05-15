// SPDX-License-Identifier: BSD-2-Clause

//! C-FFI integer typedefs matching Linux's `<linux/types.h>` shape.
//! These names match the kernel's u8/u16/u32/u64 + __s* + size_t
//! aliases so inherited C compiles against `shim_c.h` without per-
//! driver typedef customization.
//!
//! ABI invariants:
//!
//! - `__u*` / `__s*` are fixed-width by definition; they alias
//!   Rust's primitives 1:1.
//! - `size_t` / `ssize_t` follow the platform `usize` / `isize`.
//!   On x86_64 both are 64-bit; future aarch64 / riscv64 ports
//!   inherit the same shape from Rust's targeting.
//! - `gfp_t` is u32 to match Linux's `typedef unsigned int gfp_t`.
//! - `dma_addr_t` is u64; we are 64-bit-only at M1.

#![allow(non_camel_case_types)]

use core::ffi;

pub type c_void = ffi::c_void;
pub type c_char = ffi::c_char;
pub type c_int = ffi::c_int;
pub type c_uint = ffi::c_uint;
pub type c_long = ffi::c_long;
pub type c_ulong = ffi::c_ulong;

pub type __u8 = u8;
pub type __u16 = u16;
pub type __u32 = u32;
pub type __u64 = u64;
pub type __s8 = i8;
pub type __s16 = i16;
pub type __s32 = i32;
pub type __s64 = i64;

pub type gfp_t = u32;
pub type dma_addr_t = u64;

pub type size_t = usize;
pub type ssize_t = isize;
pub type loff_t = i64;
