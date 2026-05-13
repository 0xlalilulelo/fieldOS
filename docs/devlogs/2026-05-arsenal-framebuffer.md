# M0 step 3E — framebuffer console

*May 13, 2026. One session. Four commits.*

3E is the fifth of seven sub-blocks in M0 step 3 (memory, scheduler,
virtio, network, framebuffer, SMP, `>` prompt). The exit criterion
is narrow: probe Limine's framebuffer, write pixels through the
linear framebuffer that 3A's deep-clone left HHDM-mapped, render
8×16 text via a vendored glyph table, and fan every kernel serial
print out to the screen so the kernel's diagnostic stream appears
on both wires. After 3E, anything `serial::write_str` and
`writeln!(serial::Writer, ...)` print also lands on a display
attached through `-display gtk` / `-display sdl`. Headless CI is
unchanged — serial still drives the eight sentinels — but the
infrastructure for M0's `>` prompt to surface on a real screen
when 3G lands is in place.

## What landed

Four commits in one continuous session, plus a fifth catch-up
commit at the session opening:

- `4c5385b` *docs(devlogs): Arsenal smoltcp + rustls.* The 3D
  devlog written two days post-landing, before its calibration
  notes (the two-getrandom-version dance, the
  `UnbufferedClientConnection` discovery, the smoltcp
  `seq_to_transmit` post-close unwrap) faded. Not part of 3E
  proper; recording it here because the cadence question
  "do we keep per-sub-block devlogs?" was implicitly answered
  yes by writing this one before starting the next sub-block.
- `b604f87` *feat(kernel): probe Limine framebuffer.* Adds
  `FramebufferRequest` alongside the existing `MemoryMapRequest`
  and `HhdmRequest`, reads back the response after `frames::init`
  and `paging::init` are up, logs the first framebuffer's shape.
  Limine's response on QEMU q35 std-vga is 1280×800 at 32 bpp,
  pitch 5120 bytes, base at HHDM-mapped `0xffff8000fd000000`.
  No draw yet — informational log only.
- `6d9a2a3` *feat(kernel): framebuffer clear + put_pixel.* New
  `fb` module: `fb::init` stashes the framebuffer's shape behind
  a `Mutex<Option<FbInfo>>`, asserts the layout (32 bpp, RGB
  memory model, mask shifts 16/8/0, 4-byte-aligned pitch);
  `fb::clear(rgb)` paints every pixel; `fb::put_pixel(x, y, rgb)`
  writes one with bounds-checked no-op on out-of-bounds. Pixel
  format is packed `0x00RRGGBB`, which on little-endian + RGB
  byte order writes bytes `[BB, GG, RR, 00]` — the standard
  32-bpp LFB layout. Volatile writes throughout; the cost is
  negligible on x86_64 write-back caching and the discipline
  generalizes to Apple Silicon (M1+) where the LFB may be
  uncached. Color constants follow CLAUDE.md §4: `NAVY` 0x0A1A2A
  (chrome base), `AMBER` 0xFFB200 (primary signal). Smoke draw
  is a 16×16 amber square at (8, 8) over a navy clear.
- `fc5803f` *feat(kernel): 8x16 framebuffer console.* Vendored
  Spleen 8x16 v2.2.0 by Frederic Cambus (BSD-2-Clause) under
  `vendor/spleen/`; offline conversion of the BDF source to a
  256×16-byte `FONT` table in `arsenal-kernel/src/fb_font.rs`.
  `fb::render_string` takes the FB mutex once and walks
  `s.bytes()` left-to-right, calling a private
  `render_glyph_inner` per byte. Clips at the right margin.
  Smoke draw is "ARSENAL" at (8, 32) in amber on navy.
- `8aad04d` *feat(kernel): mirror serial to framebuffer console.*
  `serial::write_str` gains one line at the end — a call to
  `fb::print_str(s)`. Both bare `serial::write_str("...")` and
  `writeln!(serial::Writer, ...)` flow through it, so the
  mirror sees every kernel print without any call-site
  refactoring. `fb::print_str` is gated by a `FB_READY`
  `AtomicBool` (Release on init, Acquire on print) so calls
  before `fb::init` are silent no-ops, and uses `FB.try_lock()`
  so a panic during another fb operation still reaches serial.
  Cursor lives in `FbInfo`; `\n` advances to the next row, `\r`
  resets to column zero, glyph wrap on the right edge, and a
  scroll-by-blit (`core::ptr::copy` of `(height - GLYPH_H) *
  pitch_pixels` u32s, then a `NAVY` clear of the freed bottom
  band) when the cursor would land past the bottom row.

## How long it took

One session on 2026-05-13. Maybe 60 minutes of active time across
the four sub-commits (the timestamps span 08:31 to 09:28 plus the
STATUS flip at 09:30). The 3D devlog catch-up at 4c5385b ran before
3E-0 started and absorbed maybe 20 minutes more.

3E was a return to the 3A / 3B / 3C pace, after 3D's three sessions
of crate-integration friction. The HANDOFF's prediction held: 3E
is template-shaped work where the bottleneck is keyboard speed,
not dep archaeology. The Limine request was a single static plus a
single response read; the pixel-write primitives were 30 lines;
the glyph renderer's bulk was the 4 KiB of vendored bitmap data,
not Rust code; the serial mirror needed one byte-level fan-out
plus a small cursor state machine.

The HANDOFF's calibration of "2–3 calendar weeks for the whole of
3E" against ~15 hours per week was wrong by an order of magnitude
in the same direction the 3A / 3B / 3C estimates were wrong —
spec-driven work benefits enormously from upfront HANDOFF
documentation, and what looks like several sessions of design
question reduces to mechanical implementation when the design
question has already been resolved in writing. The pattern is
holding across five sub-blocks now (3A, 3B, 3C, 3D's last commit,
3E). 3F is the next test — APIC vector collision with the existing
IDT entries is a real surface, not a HANDOFF-shaped one.

## Detours worth recording

**Font licensing was a real judgment call.** The 3E kickoff said
"public-domain VGA 8×16, embedded as a static `[u8; 4096]`." That
sentence elided the actual sourcing problem. The canonical "IBM
VGA 8×16" font's bytes circulate widely under various
public-domain claims, but the original is not actually public
domain — IBM never released it, and the abandonware framing
doesn't satisfy CLAUDE.md §3's explicit license list. The Linux
kernel's `font_8x16.c` is GPL'd and would forward GPL through
Arsenal's BSD-2-Clause base, which §3 also disallows outside the
LinuxKPI driver boundary. The v0.1 Field OS arc shipped an 8×8
font via TempleOS → ZealOS (Unlicense, clean per §3) but bringing
TempleOS provenance into post-pivot Arsenal undercuts ADR-0004 in
a way that the size mismatch makes worse (8×8 not 8×16). The
right move was to ask the user — `AskUserQuestion` with four
options (Spleen, Unifont, hand-roll, v0.1 TempleOS-lineage) — and
the answer was Spleen 8×16. Frederic Cambus's font, BSD-2-Clause,
exact license match against the Arsenal base. The upstream BDF
landed at `vendor/spleen/spleen-8x16.bdf` alongside its LICENSE
(same pattern as `vendor/limine/`); the derived 4096-byte bitmap
landed at `arsenal-kernel/src/fb_font.rs` with the lineage cite
in the header. A one-shot Python pass parsed the BDF, indexed
glyphs 0x00–0xFF, emitted one Rust line per glyph with an ASCII
label comment. Of Spleen's 995 ISO10646 glyphs, 192 fall in
0x00–0xFF; printable ASCII 0x20–0x7E is fully covered, and the
rest of the upper-half range stays zeroed (blank glyphs).

**QEMU q35 std-vga is 1280×800, not 1024×768.** The 3E kickoff
quoted 1024×768 as the smoke target's screen resolution. Reality
on `qemu-system-x86_64 -machine q35 -accel tcg -cpu max -display
none` (the exact line `ci/qemu-smoke.sh` uses) reports 1280×800.
The mismatch was visible immediately on the `fb:` probe line —
`addr=0xffff8000fd000000 1280x800 bpp=32 pitch=5120`. Pitch
exactly matches `width * 4` (5120 = 1280 × 4, no row padding),
which means the simple `pitch_pixels = pitch / 4` shape works
without a special case for padded rows. The lesson: probed
values are load-bearing, not the documentation-quoted defaults.
`fb::clear` and `fb::put_pixel` both go through `info.pitch_pixels`
and `info.width` from the probe, not constants. A future QEMU
default change won't break this code.

**Three speculative APIs caught by clippy.** Each sub-commit
introduced an API the kickoff plan named that turned out to have
no caller, and clippy's `-D warnings` (with `dead_code` implied)
caught all three. (a) 3E-1's `CYAN: u32 = 0x0000_C8E0` —
CLAUDE.md §4 names cyan as the secondary signal, but no rendering
code uses it yet. Dropped; reintroduces when the first caller
appears (likely 3F's preemption status hint or Stage chrome in
M2). (b) 3E-2's public `fb::render_glyph` — drafted alongside
`render_string`, but render_string is the only consumer of glyph
rendering and uses the private `render_glyph_inner` directly.
Dropped. (c) 3E-3's `FbWriter` implementing `core::fmt::Write` —
the kickoff explicitly named this type, and CLAUDE.md mentioned
mirroring the shape of `serial::Writer`. But the byte-level
fan-out from `serial::write_str` covers every kernel print
without anyone calling fb directly through `fmt::Write`. Dropped.
The pattern is worth recording: kickoff plans describe the "this
is the surface you'll need" shape ahead of time, and a fraction
of the surface turns out unused when the implementation reduces
to a smaller set of consumers. Clippy is the right enforcement
mechanism — flag and force the conversation — and CLAUDE.md's
"nothing speculative" wins out three times in a row.

**`FB_READY` Release/Acquire and `try_lock` panic-safety.** Two
design moves in 3E-3 that matter under reentrancy, though both
are theoretical on the single-CPU cooperative kernel today. The
mirror has to be safe to call before `fb::init` runs, because
`ARSENAL_BOOT_OK` and ~ten more kernel prints fire before
`fb::init` lands. An `AtomicBool::store(true, Release)` in
`fb::init` and `AtomicBool::load(Acquire)` in `fb::print_str`
forms the smallest gate that generalizes cleanly to 3F's
preempted scheduler — when the timer IRQ arrives mid-`fb::init`,
the Release/Acquire pair ensures any IRQ context that sees
`FB_READY == true` also sees the populated Mutex contents. The
`try_lock()` instead of `lock()` is the panic-safety case: if a
panic fires inside `fb::clear` or `fb::render_string`, the panic
handler's `writeln!(serial::Writer, "ARSENAL_PANIC ...")` routes
through `serial::write_str` and into `fb::print_str`, which
would deadlock on a recursive `lock()`. `try_lock` drops the
mirror copy on contention; the panic message still hits serial.
Single-CPU pre-3F makes this defensive rather than load-bearing,
but the defensiveness costs nothing.

**The submodule pattern for vendored asset data.** `fb_font.rs`
lives next to `fb.rs` in `arsenal-kernel/src/` but is a private
submodule of `fb`, declared via `#[path = "fb_font.rs"] mod
font;` inside `fb.rs`. That's different from the existing
`virtio_*` modules, which are flat peers at the top level even
though `virtio_blk` depends on `virtio`. The reasoning: `fb_font`
is a vendored data asset, not a peer module. Keeping it
visible-only-to-`fb` matches the actual coupling, and the
`#[path]` attribute avoids forcing `fb` itself into a directory
layout. A future glyph submodule (an atlas, multiple fonts) would
formalize this into `fb/font/` properly.

**Scroll-by-blit is untested by the headless smoke.** Current
kernel boot is ~30 serial lines × 16 px per glyph ≈ 480 px
tall. The QEMU q35 framebuffer is 800 px tall. The cursor
never advances past the bottom row, so `maybe_scroll`'s body
never executes during the smoke. The code is code-review-correct
(`core::ptr::copy` is overlap-safe for the forward direction, the
bottom-band clear matches `fb::clear`'s loop shape), but it's
not test-correct in CI. 3F's preemption status hints
("vec 0xEF: tick 100", repeatedly) and 3G's interactive prompt
will exercise the scroll path naturally, and adding a 60-line
dummy print to force it now would be pollution per CLAUDE.md
"nothing speculative." Flagged for the M0 step 3 exit
retrospective.

## The numbers

- **4 commits in 3E** plus the 3D devlog catch-up (4c5385b) and
  the STATUS flip (e115095) — 6 commits if counting the paper
  trail of this session.
- **Net new Rust** in `arsenal-kernel/src/`: ~50 lines in
  `main.rs` (probe + init + clear + amber square + ARSENAL
  string + the `mod fb;` line), ~215 lines in `fb.rs` (init,
  clear, put_pixel, render_string, render_glyph_inner,
  print_str, print_str_inner, maybe_scroll), ~280 lines in
  `fb_font.rs` (a 4 KiB `static FONT` plus the header + GLYPH_W
  / GLYPH_H constants), ~9 lines in `serial.rs` (the fan-out
  call). Total ~554 lines added, ~3 removed.
- **Vendored assets**: `vendor/spleen/LICENSE` (1.3 KiB),
  `vendor/spleen/spleen-8x16.bdf` (153 KiB).
- **ELF**: 1,461,664 (end of 3D, post-3D-5 STATUS) → 1,474,744
  (end of 3E-3). Net +13,080 bytes across the four sub-steps:
  +80 (probe), -144 (clear / put_pixel inlined into the demo),
  +4240 (font table + glyph plumbing), +8904 (mirror state
  machine + scroll-by-blit). ISO unchanged at 19.3 MB.
- **Sentinels: still 8.** 3E intentionally rides on the existing
  set rather than adding `ARSENAL_FB_OK` — the smoke target is
  "the kernel continues past fb init / render / mirror without
  faulting and without deadlocking," which is implicit in the
  existing sentinel-completion check.
- **Smoke time: still ~1 s** locally. The mirror's per-glyph
  pixel writes happen on every kernel print, but they're write-
  back-cached u32 stores on x86_64 TCG and don't measurably
  affect the smoke.

## What the boot looks like

Serial trace is unchanged from 3D except for one new line
between `ARSENAL_FRAMES_OK` and the cpu banner:

```
ARSENAL_FRAMES_OK
cpu: id=0 (single-CPU stage)
fb: addr=0xffff8000fd000000 1280x800 bpp=32 pitch=5120
task: built (entry=0xffffffff80003ae0, saved_rsp=0xffff800000103fc0, state=Ready, stack=16 KiB)
...
```

The new visible signal is on the screen, under `-display gtk` or
`-display sdl`. After `fb::init` paints the 1280×800 frame navy
and fb-1's amber square + fb-2's "ARSENAL" banner land at the
top-left, the mirror starts capturing serial output from
ARSENAL_FRAMES_OK's line onward (the `fb:` probe and everything
after). The text appears in amber-on-navy at the cursor's
current position, advancing one glyph width per byte, wrapping
at the right margin, and (eventually, past ~50 rows) scrolling.
The first ~9 kernel prints — ARSENAL_BOOT_OK, the mm banner,
the int3 diagnostic, the paging line, ARSENAL_HEAP_OK, the
frames-reclaimed line, ARSENAL_FRAMES_OK, the cpu banner — fire
before `fb::init` runs and stay serial-only.

## What 3F looks like

Per ARSENAL.md M0, the next sub-block: LAPIC + preemption.

- **LAPIC bring-up.** xAPIC mode (MMIO at 0xfee00000, HHDM-
  mapped via `paging::map_mmio` from 3C's MMIO infrastructure).
  Read the LAPIC ID register, enable via spurious-vector
  register's bit 8, mask the legacy PIC (write 0xff to both
  0x21 and 0xa1 so it stops competing for vectors).
- **Periodic timer.** LAPIC timer's divide config, initial
  count, and LVT entry. Target tick rate ~100 Hz. The timer
  vector lives at an unused IDT slot above 32 (typical Linux
  convention is 0xEF or 0xF0). Worth verifying against
  intel-sdm Vol. 3A §10.5.4 since the LAPIC timer's "periodic"
  bit semantics changed once between xAPIC and x2APIC eras.
- **IDT wiring.** The IDT from 3A has int3 and a few CPU
  exceptions installed; the timer vector is a new entry. The
  handler ack's the interrupt (write 0 to LAPIC EOI register
  at 0xfee000b0) and calls `sched::yield_now`. The handler
  must save the full register set, not just callee-save —
  `extern "x86-interrupt"` from the existing `abi_x86_interrupt`
  feature.
- **Idle's hlt comes back.** The 3B-4 devlog noted idle had
  to drop `hlt` because cooperative-no-IRQ + hlt = stuck CPU.
  With a real timer IRQ, idle can `hlt` and the timer wakes
  it. Real power-save.
- **Smoke target.** The cooperative `yield_now`s in ping/pong
  become redundant; the timer schedules through them. The
  visible assertion would be an `ARSENAL_TIMER_OK` sentinel
  after N ticks (printable from the IRQ handler), but timer-
  in-IRQ context's print path through `serial::write_str` →
  `fb::print_str` → `FB.try_lock()` is exactly the reentrancy
  case 3E-3's `try_lock` was designed to handle. Worth verifying.

The bug-prone moment in 3F is vector collision and the
preemption-safety of the existing cooperative scheduler.
Specifically: `sched::yield_now` currently round-robins through
the runqueue with a `Mutex<VecDeque<Box<Task>>>` — a timer IRQ
that fires *while another CPU context holds that Mutex* would
deadlock. Single-CPU pre-SMP makes this theoretical for now,
but 3F's IRQ context might still need to lock the runqueue
from inside an interrupt handler, which on cooperative locks
is dangerous. The right shape may be a deferred-work flag set
in the IRQ handler and consumed at the next `yield_now`. To
be decided in the 3F HANDOFF.

## Cadence

This is the fifth sub-block devlog of M0 step 3 (3A, 3B, 3C, 3D,
3E). The font-licensing call, the QEMU-default-resolution
mismatch, and the three-dropped-speculative-APIs pattern all
feel genuinely useful records rather than rote bookkeeping —
specifically, the font call is the kind of judgment-under-
license-constraint decision the project will repeat at the
"first port of Firefox / mpv" point of M2 → v0.5, and recording
the resolution mechanism (`AskUserQuestion` with four enumerated
options) is more valuable than recording the answer alone. The
per-sub-block cadence continues. If 3F's devlog ends up reading
as "the APIC spec section we worked from," that's the signal to
consolidate.

The Asahi cadence stays the model — calibrated, honest, never
marketing.

—
