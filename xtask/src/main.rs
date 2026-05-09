// SPDX-License-Identifier: BSD-2-Clause
//
// xtask — Arsenal build helpers. Invoked as `cargo xtask <subcommand>`
// via the workspace alias in .cargo/config.toml.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("iso") => match build_iso() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("xtask: {e}");
                ExitCode::FAILURE
            }
        },
        Some("--help") | Some("-h") | None => {
            print_help();
            ExitCode::SUCCESS
        }
        Some(cmd) => {
            eprintln!("xtask: unknown subcommand `{cmd}`\n");
            print_help();
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!("xtask — Arsenal build helpers");
    println!();
    println!("Usage: cargo xtask <subcommand>");
    println!();
    println!("Subcommands:");
    println!("  iso    Assemble bootable arsenal.iso (BIOS + UEFI hybrid)");
}

fn build_iso() -> Result<(), String> {
    let project_root = project_root();
    let target_dir = project_root.join("target");
    let staging = target_dir.join("iso_root");
    let vendor = project_root.join("vendor/limine");

    // 1) Build the kernel for x86_64-unknown-none.
    run(
        Command::new("cargo")
            .current_dir(&project_root)
            .args([
                "build",
                "--release",
                "-p",
                "arsenal-kernel",
                "--target",
                "x86_64-unknown-none",
            ]),
        "cargo build (arsenal-kernel)",
    )?;

    let kernel_elf = target_dir.join("x86_64-unknown-none/release/arsenal-kernel");
    if !kernel_elf.exists() {
        return Err(format!(
            "kernel ELF not found at {}",
            kernel_elf.display()
        ));
    }

    // 2) Assemble the staging tree. Layout matches the field-os-v0.1
    //    recipe (limine binaries flat in /boot/, BOOTX64.EFI under
    //    /EFI/BOOT/) — proven to boot under Limine v12.0.2.
    let _ = fs::remove_dir_all(&staging);
    let staging_boot = staging.join("boot");
    let staging_efi = staging.join("EFI/BOOT");
    fs::create_dir_all(&staging_boot).map_err(|e| format!("mkdir boot: {e}"))?;
    fs::create_dir_all(&staging_efi).map_err(|e| format!("mkdir EFI/BOOT: {e}"))?;

    copy(&kernel_elf, &staging_boot.join("arsenal-kernel"))?;
    copy(&project_root.join("boot/limine.conf"), &staging_boot.join("limine.conf"))?;
    for f in ["limine-bios.sys", "limine-bios-cd.bin", "limine-uefi-cd.bin"] {
        copy(&vendor.join(f), &staging_boot.join(f))?;
    }
    copy(&vendor.join("BOOTX64.EFI"), &staging_efi.join("BOOTX64.EFI"))?;

    // 3) Run xorriso to produce the hybrid ISO.
    let iso = project_root.join("arsenal.iso");
    run(
        Command::new("xorriso").args([
            "-as", "mkisofs",
            "-b", "boot/limine-bios-cd.bin",
            "-no-emul-boot", "-boot-load-size", "4", "-boot-info-table",
            "--efi-boot", "boot/limine-uefi-cd.bin",
            "-efi-boot-part", "--efi-boot-image", "--protective-msdos-label",
        ])
        .arg(&staging)
        .arg("-o")
        .arg(&iso),
        "xorriso",
    )?;

    // 4) Install Limine BIOS stage 1/2 into the ISO. Without this step the
    //    BIOS boot path hangs in firmware — Limine's stage 2 isn't found
    //    in the El Torito record alone. UEFI boot would still work, but
    //    QEMU defaults to BIOS, and CI runners do too.
    let limine_host = vendor.join("limine");
    if !limine_host.exists() {
        run(
            Command::new("make").current_dir(&vendor),
            "make -C vendor/limine",
        )?;
    }
    run(
        Command::new(&limine_host).arg("bios-install").arg(&iso),
        "limine bios-install",
    )?;

    println!();
    println!("Arsenal ISO: {}", iso.display());
    Ok(())
}

fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask is a workspace member")
        .to_path_buf()
}

fn copy(src: &Path, dst: &Path) -> Result<(), String> {
    fs::copy(src, dst)
        .map(|_| ())
        .map_err(|e| format!("copy {} -> {}: {e}", src.display(), dst.display()))
}

fn run(cmd: &mut Command, label: &str) -> Result<(), String> {
    let status = cmd
        .status()
        .map_err(|e| format!("{label}: failed to spawn: {e}"))?;
    if !status.success() {
        return Err(format!("{label}: exited with {status}"));
    }
    Ok(())
}
