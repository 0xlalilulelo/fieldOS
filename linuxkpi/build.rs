// SPDX-License-Identifier: BSD-2-Clause

//! M1-2-4 cc-build infrastructure for linuxkpi.
//!
//! Compiles BSD-2 smoke source under `linuxkpi/csrc/` and (from
//! M1-2-5 onward) GPLv2 inherited driver source under
//! `vendor/linux-6.12/`. Cross-compile flag set per
//! `docs/adrs/0005-linuxkpi-shim-layout.md` § 2.
//!
//! License-boundary enforcement: `check_path` refuses any source
//! file path outside `linuxkpi/csrc/` (BSD-2) or
//! `../vendor/linux-6.12*` (GPLv2 — the GPL fence). The
//! directory-based fence is the audit-friendly invariant
//! ADR-0005 § 4 commits to; a reviewer should be able to tell
//! by `ls` which license applies to any file.
//!
//! Why the direct-clang + Rust-`ar`-crate path instead of cc's
//! `Build::compile()`: macOS's system `ar`/`ranlib` are
//! Mach-O-only and silently produce ELF-archive-index-less `.a`
//! files that rust-lld then can't resolve symbols against.
//! `llvm-ar` is the conventional fix but isn't shipped under
//! that name in stock Apple Xcode toolchains and isn't bundled
//! with rustup. The pure-Rust `ar` crate writes a GNU-format
//! archive (no symbol index); we pair it with rustc's
//! `+whole-archive` link modifier which tells rust-lld to pull
//! every `.o` from the archive unconditionally — no index
//! required. The `+whole-archive` syntax is stable since Rust
//! 1.61 and is the canonical pattern for kernel-shaped C-side
//! integrations where you want every symbol exposed regardless
//! of dead-code analysis.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=csrc/smoke.c");
    println!("cargo:rerun-if-changed=include/shim_c.h");
    println!("cargo:rerun-if-changed=build.rs");

    // Source manifest. M1-2-4: BSD-2 smoke only. M1-2-5 will
    // append vendor/linux-6.12/drivers/virtio/virtio_balloon.c
    // to this list once the upstream subset is vendored.
    let sources: &[&str] = &["csrc/smoke.c"];

    for path in sources {
        check_path(path);
        if !Path::new(path).exists() {
            panic!("linuxkpi build: source {path} missing");
        }
    }

    let target = std::env::var("TARGET").unwrap_or_default();
    let is_kernel_target = target == "x86_64-unknown-none";

    if !is_kernel_target {
        // Host build (cargo check on macOS / Linux): skip C
        // compile entirely. linuxkpi-as-host-library never
        // links the .o; cargo check just wants the Rust compile
        // to pass. Re-runs covered by rerun-if-changed above.
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR unset"));

    // Resolve clang's resource-dir for the freestanding-safe
    // builtin headers (stddef.h, stdint.h, stdarg.h). Linux's
    // own Kbuild does the analogous dance:
    //   NOSTDINC_FLAGS := -nostdinc -isystem $(shell $(CC)
    //                                  -print-file-name=include)
    let resource_dir = Command::new("clang")
        .arg("-print-resource-dir")
        .output()
        .expect("clang -print-resource-dir failed; is clang on PATH?");
    let resource_dir = String::from_utf8(resource_dir.stdout)
        .expect("clang -print-resource-dir output was not UTF-8");
    let resource_include = format!("{}/include", resource_dir.trim());

    let mut object_paths: Vec<PathBuf> = Vec::new();

    for &src in sources {
        let stem = Path::new(src)
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("source path has no file stem");
        let obj_path = out_dir.join(format!("{stem}.o"));

        let status = Command::new("clang")
            .args([
                "-target", "x86_64-unknown-none",
                "-x", "c",
                "-nostdinc",
                "-isystem", &resource_include,
                "-ffreestanding",
                "-fno-stack-protector",
                "-mno-red-zone",
                "-mcmodel=kernel",
                "-fno-pic",
                "-fno-pie",
                "-Wno-unused-parameter",
                "-Wno-unused-function",
                "-O2",
                "-I", "include",
                "-I", "../vendor/linux-6.12/include",
                "-c",
                src,
                "-o",
            ])
            .arg(&obj_path)
            .status()
            .unwrap_or_else(|e| panic!("clang invocation failed: {e}"));
        if !status.success() {
            panic!("clang compile of {src} failed with status {status}");
        }
        object_paths.push(obj_path);
    }

    // Bundle the .o files into liblinuxkpi-drivers.a using the
    // pure-Rust ar crate (no symbol index — see module doc).
    let archive_path = out_dir.join("liblinuxkpi-drivers.a");
    let archive_file = std::fs::File::create(&archive_path)
        .unwrap_or_else(|e| panic!("create {} failed: {e}", archive_path.display()));
    let mut builder = ar::Builder::new(archive_file);
    for obj_path in &object_paths {
        let mut obj_file = std::fs::File::open(obj_path)
            .unwrap_or_else(|e| panic!("open {} failed: {e}", obj_path.display()));
        let name = obj_path
            .file_name()
            .and_then(|s| s.to_str())
            .expect("obj path has no file name");
        let mut header = ar::Header::new(name.as_bytes().to_vec(), file_size(obj_path));
        header.set_mode(0o644);
        builder
            .append(&header, &mut obj_file)
            .unwrap_or_else(|e| panic!("ar::Builder::append failed: {e}"));
    }
    drop(builder); // flushes + closes the archive file

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    // +whole-archive: pull every .o from the archive without
    // needing a symbol index (which Apple's system ar/ranlib
    // can't generate for ELF). Stable since Rust 1.61.
    println!("cargo:rustc-link-lib=static:+whole-archive=linuxkpi-drivers");
}

/// Reject any source path outside the BSD-2 (`linuxkpi/csrc/`)
/// or GPLv2 (`vendor/linux-6.12*/`) fences. Build-system mirror
/// of the directory-based license boundary in ADR-0005 § 4.
fn check_path(path: &str) {
    let ok = path.starts_with("csrc/")
        || path.starts_with("../vendor/linux-6.12");
    if !ok {
        panic!(
            "linuxkpi build: refuse to compile {path} — must live \
             under linuxkpi/csrc/ (BSD-2) or vendor/linux-6.12*/ \
             (GPLv2). See ADR-0005 § 4."
        );
    }
}

fn file_size(p: &Path) -> u64 {
    std::fs::metadata(p)
        .unwrap_or_else(|e| panic!("metadata({}) failed: {e}", p.display()))
        .len()
}
