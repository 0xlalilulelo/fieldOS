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

/// Command dispatcher. 3G-2 fills in the `help` / `hw` / `panic`
/// table; for 3G-1 the structural call site exists so the
/// shell-task loop is the same shape across both sub-blocks.
/// The empty-buffer case (user hit Enter on a bare prompt) falls
/// through here too and produces no output — matching most
/// interactive shells.
fn dispatch(_buf: &[u8]) {
    // intentionally empty at 3G-1
}
