// SPDX-License-Identifier: BSD-2-Clause
//
// Custom getrandom backends for getrandom 0.4 and getrandom 0.2. The
// RustCrypto provider rustls-rustcrypto (3D-4) pulls both crate
// versions through its tree; bare-metal x86_64-unknown-none has no
// system RNG, so both must dispatch to a kernel-provided source.
//
// The source: try RDRAND if CPUID advertises it; fall back to a TSC
// xor-jumble. Neither is cryptographically defensible alone — RDRAND
// is fine in isolation (NIST SP 800-90B HW source) but the TSC fallback
// is purely "good enough for a TLS handshake smoke under TCG QEMU,"
// where the TCG hypervisor doesn't synthesize RDRAND on the boot CPU
// in every configuration. Real entropy for v0.5+ comes from a proper
// kernel RNG (jitter + HW + reseed) once the IRQ and timer subsystems
// give us collection points.

use core::arch::x86_64::{__cpuid, _rdtsc};

const CPUID_RDRAND_BIT: u32 = 1 << 30;

/// Probe CPUID leaf 1, ECX bit 30 for RDRAND availability. Result is
/// stable for the life of the boot.
fn has_rdrand() -> bool {
    let info = __cpuid(1);
    (info.ecx & CPUID_RDRAND_BIT) != 0
}

/// Read 64 random bits via RDRAND, retrying on the architectural
/// failure path (CF=0 after RDRAND means "no entropy this cycle";
/// Intel guidance is up to 10 retries).
fn rdrand64() -> Option<u64> {
    for _ in 0..10 {
        let value: u64;
        let success: u8;
        // SAFETY: rdrand is a defined x86_64 instruction; on CPUs
        // without it (filtered by has_rdrand), execution would #UD.
        // We test the carry flag (set on success, clear on failure)
        // via setc, returning success in `success`.
        unsafe {
            core::arch::asm!(
                "rdrand {value}",
                "setc {success}",
                value = out(reg) value,
                success = out(reg_byte) success,
                options(nomem, nostack),
            );
        }
        if success != 0 {
            return Some(value);
        }
    }
    None
}

/// Fill `dest` with bytes. Uses RDRAND when available; otherwise a
/// TSC xor-jumble that's adequate for the smoke and explicitly not
/// for cryptographic use beyond M0.
pub fn fill_bytes(dest: &mut [u8]) {
    let use_rdrand = has_rdrand();
    let mut i = 0;
    while i < dest.len() {
        let word = if use_rdrand {
            rdrand64()
        } else {
            None
        }
        .unwrap_or_else(|| {
            // SAFETY: _rdtsc is unconditionally available on x86_64;
            // no side effects beyond reading the TSC.
            let a = unsafe { _rdtsc() };
            let b = unsafe { _rdtsc() };
            // Hash two TSC reads with a splitmix64-flavored mix so
            // adjacent words don't visibly correlate.
            let mut x = a ^ b.rotate_left(31);
            x = x.wrapping_mul(0x9E37_79B9_7F4A_7C15);
            x ^= x >> 30;
            x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x ^= x >> 27;
            x
        });
        let bytes = word.to_le_bytes();
        let chunk = (dest.len() - i).min(8);
        dest[i..i + chunk].copy_from_slice(&bytes[..chunk]);
        i += chunk;
    }
}

// --- getrandom 0.4 custom backend -----------------------------------
//
// getrandom 0.3+ uses --cfg getrandom_backend="custom" (set in
// .cargo/config.toml) and dispatches to an extern Rust fn with the
// fixed name __getrandom_v04_custom.

/// # Safety
/// Caller passes a valid `dest` pointer + `len` describing a writable
/// region of bytes. getrandom 0.4's contract.
#[unsafe(no_mangle)]
unsafe extern "Rust" fn __getrandom_v04_custom(
    dest: *mut u8,
    len: usize,
) -> Result<(), getrandom::Error> {
    // SAFETY: caller's contract per getrandom's custom backend API.
    let slice = unsafe { core::slice::from_raw_parts_mut(dest, len) };
    fill_bytes(slice);
    Ok(())
}

// --- getrandom 0.2 custom backend -----------------------------------
//
// getrandom 0.2 uses the register_custom_getrandom! macro with a
// function returning Result<(), getrandom02::Error>.

fn getrandom_v02(dest: &mut [u8]) -> Result<(), getrandom02::Error> {
    fill_bytes(dest);
    Ok(())
}

getrandom02::register_custom_getrandom!(getrandom_v02);
