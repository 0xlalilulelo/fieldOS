// SPDX-License-Identifier: BSD-2-Clause
//
// Interactive shell task — M0 step 3G-1. Spawned from `_start`
// before `sched::init` takes over, the shell prints the initial
// `> ` prompt, emits the ARSENAL_PROMPT_OK sentinel, and loops
// reading single bytes from the PS/2 driver (kbd::poll). Each
// non-control byte echoes to serial — and via 3E's mirror, to
// the framebuffer console — and accumulates into a 256-byte line
// buffer. Backspace pops one byte and emits the classic VT100
// destructive sequence "\b \b" on serial. Newline calls
// `dispatch(&buf[..len])` (a structural stub at 3G-1; 3G-2 lands
// the `help` / `hw` / `panic` commands).
//
// Cooperative-only: the shell yields between every poll, so a
// task that wants CPU time gets it. With idle's 100 Hz hlt-wake
// from 3F-3 and the round-robin scheduler from 3B, the shell is
// scheduled every few ms in steady state — well above human
// typing speed.
//
// Two visible-polish items deferred from 3G-1:
//   * No fb-visible cursor at the insertion point. fb::print_str
//     advances its private cursor on every byte but does not
//     expose it; rendering an underscore (or any indicator) at
//     the insertion point requires either a new fb API or a
//     shadow cursor in the shell that re-derives fb's position.
//     Both are real surface decisions and deferred. Typed input
//     still appears on the framebuffer at the current print
//     position; users see what they typed in amber-on-navy, just
//     without a cursor glyph after.
//   * Destructive backspace is serial-only. fb::print_str passes
//     0x08 through to the glyph renderer (no special-case in the
//     match block) so the "\b \b" sequence draws three null
//     glyphs on fb. Polish lands alongside the cursor work in a
//     later 3G micro-commit or as M0 step 3 exit cleanup.

use core::fmt::Write;

use crate::apic;
use crate::frames;
use crate::kbd;
use crate::sched;
use crate::serial;

/// Maximum input line length, including the trailing newline. A
/// fixed-size stack buffer keeps the shell allocation-free and
/// bounds memory consumption against a runaway paste. 256 is
/// comfortable for M0's hand-typed commands; the limit grows in
/// post-M0 work if a real workload needs it.
const LINE_MAX: usize = 256;

/// Prompt string. Two characters — `>` and a space — matches the
/// ARSENAL.md M0 exit criterion ("boots to a `>` prompt"). The
/// IBM Plex Mono 8x16 in the framebuffer console renders this
/// at column 0 in amber on navy after every Enter.
const PROMPT: &str = "> ";

/// ASCII control bytes the line editor recognizes specially.
const BS: u8 = 0x08;
const LF: u8 = b'\n';

/// Shell task entry. `sched::spawn` requires `fn() -> !`; this
/// function never returns. Drops are unreachable.
pub fn run() -> ! {
    // Sentinel before the prompt so the smoke's grep observes the
    // marker on its own line; the prompt then prints on the next
    // line and is what humans see under `-display gtk`. Order is
    // deliberate — see commit body.
    serial::write_str("ARSENAL_PROMPT_OK\n");
    serial::write_str(PROMPT);

    let mut buf = [0u8; LINE_MAX];
    let mut len: usize = 0;

    loop {
        sched::yield_now();
        let Some(b) = kbd::poll() else { continue };
        match b {
            LF => {
                serial::write_str("\n");
                dispatch(&buf[..len]);
                len = 0;
                serial::write_str(PROMPT);
            }
            BS => {
                if len > 0 {
                    len -= 1;
                    // VT100 destructive-backspace sequence:
                    // move cursor left, overwrite with space, move
                    // left again. Real terminals interpret this as
                    // "delete the previous character." The fb-side
                    // gap is documented at the top of this file.
                    serial::write_str("\u{0008} \u{0008}");
                }
            }
            byte => {
                if len < LINE_MAX {
                    buf[len] = byte;
                    len += 1;
                    echo(byte);
                }
                // Buffer full: drop the byte silently. A real shell
                // would bell or visibly truncate; M0 silence is fine
                // because LINE_MAX is well above hand-typed commands
                // and overrun is not a path real input takes.
            }
        }
    }
}

/// Echo one byte to serial (which fans out to fb via 3E's mirror).
/// `kbd::poll` returns only ASCII bytes (table entries lie in
/// 0x00..0x7F), so the single-byte slice is always valid UTF-8.
fn echo(byte: u8) {
    let one = [byte];
    if let Ok(s) = core::str::from_utf8(&one) {
        serial::write_str(s);
    }
}

/// Command dispatcher. Parses the first whitespace-delimited token
/// from `buf` and routes to the matching `cmd_*` handler. Empty
/// buffer (bare Enter) is a no-op, matching most interactive shells.
fn dispatch(buf: &[u8]) {
    if buf.is_empty() {
        return;
    }
    let token_end = buf
        .iter()
        .position(|&b| b == b' ' || b == b'\t')
        .unwrap_or(buf.len());
    let token = &buf[..token_end];
    match token {
        b"help" => cmd_help(),
        b"hw" => cmd_hw(),
        b"panic" => cmd_panic(),
        _ => cmd_unknown(token),
    }
}

/// `help` — one-line description per known command. Add a line here
/// when a new command lands; the help text is the canonical
/// reference, not a separate doc.
fn cmd_help() {
    serial::write_str(
        "help  — list available commands\n\
         hw    — show hardware summary (CPU, memory, LAPIC, virtio)\n\
         panic — deliberately panic the kernel (interactive testing)\n",
    );
}

/// `hw` — the M0 step 3 usability gate's "shows hardware summary"
/// command. Each line is a single subsystem; new lines accrue as
/// the relevant subsystem lands. Output is plain ASCII and renders
/// cleanly in both serial and the framebuffer mirror.
fn cmd_hw() {
    serial::write_str("hw:\n");

    // CPUID brand string from extended leaves 0x80000002..0x80000004.
    // Each leaf returns 16 ASCII bytes across EAX/EBX/ECX/EDX.
    let mut brand = [0u8; 48];
    for (i, leaf) in [0x8000_0002u32, 0x8000_0003, 0x8000_0004]
        .iter()
        .enumerate()
    {
        // core::arch::x86_64::__cpuid is the safe wrapper; the
        // unsafe variant is __cpuid_count. Extended leaves
        // 0x80000002..0x80000004 return the processor brand string
        // per Intel SDM Vol. 2A §3.2 and AMD APM Vol. 3 §3.13.
        let r = core::arch::x86_64::__cpuid(*leaf);
        brand[i * 16..i * 16 + 4].copy_from_slice(&r.eax.to_le_bytes());
        brand[i * 16 + 4..i * 16 + 8].copy_from_slice(&r.ebx.to_le_bytes());
        brand[i * 16 + 8..i * 16 + 12].copy_from_slice(&r.ecx.to_le_bytes());
        brand[i * 16 + 12..i * 16 + 16].copy_from_slice(&r.edx.to_le_bytes());
    }
    // Brand string is NUL-terminated and may have leading spaces
    // (Intel pads right-aligned strings to 48 chars). Trim both.
    let nul = brand.iter().position(|&b| b == 0).unwrap_or(brand.len());
    let lead = brand[..nul]
        .iter()
        .position(|&b| b != b' ')
        .unwrap_or(nul);
    let cpu_brand = core::str::from_utf8(&brand[lead..nul]).unwrap_or("(non-ascii brand)");
    let _ = writeln!(serial::Writer, "  cpu: {cpu_brand}");
    let _ = writeln!(serial::Writer, "  cores: 1 (single-CPU stage)");

    // Memory from 3A's frame allocator. 4 KiB per frame.
    let free = frames::FRAMES.free_count();
    let total = frames::FRAMES.total_added();
    let _ = writeln!(
        serial::Writer,
        "  ram: {} KiB free / {} KiB total ({} / {} 4-KiB frames)",
        free * 4,
        total * 4,
        free,
        total,
    );

    // LAPIC summary from 3F. Version comes from the cached snapshot
    // apic::init stashed; vectors are compile-time constants.
    let _ = writeln!(
        serial::Writer,
        "  lapic: version={:#010x} timer-vector={:#x} spurious-vector={:#x}",
        apic::version(),
        apic::TIMER_VECTOR,
        apic::SPURIOUS_VECTOR,
    );

    // virtio device presence. 3C's smoke ran block + net successfully
    // (ARSENAL_BLK_OK / ARSENAL_NET_OK fired during boot) so by the
    // time `hw` runs they are known good. A richer summary with BDF
    // + device IDs waits for M1 where the PCI subsystem grows
    // beyond M0's scan + virtio-modern probe pair.
    serial::write_str("  virtio: blk=present net=present\n");
}

/// `panic` — exercise the panic handler from an interactive context.
/// Useful for verifying ARSENAL_PANIC routes through serial + fb
/// under `-display gtk`; never run from the smoke (the kernel halts
/// after the panic message lands).
fn cmd_panic() -> ! {
    panic!("user-initiated panic via shell `panic` command");
}

/// Unknown-command fallback. Echoes the unrecognized token back so
/// users can see exactly what got parsed (helpful when shifted
/// keys produce unexpected characters).
fn cmd_unknown(token: &[u8]) {
    let printable = core::str::from_utf8(token).unwrap_or("(non-utf8)");
    let _ = writeln!(
        serial::Writer,
        "unknown command: {printable}; try 'help'",
    );
}
