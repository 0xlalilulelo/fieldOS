// SPDX-License-Identifier: BSD-2-Clause
//
// Linear framebuffer access. Limine maps the LFB into HHDM for us;
// fb::init stashes the framebuffer's shape, fb::clear paints the
// whole frame, fb::put_pixel writes one pixel, fb::render_string
// draws a row of 8x16 glyphs, fb::print_str advances the cursor
// (with newline + scroll-by-blit) for the serial mirror.

use core::sync::atomic::{AtomicBool, Ordering};

use limine::framebuffer::{Framebuffer, MemoryModel};
use spin::Mutex;

// The 8x16 Spleen font lives next door as fb_font.rs; the #[path]
// keeps it a private submodule of fb without forcing fb itself
// into a directory layout.
#[path = "fb_font.rs"]
mod font;

// CLAUDE.md §4 — chrome base, primary signal, secondary signal.
// Encoded as 0x00RRGGBB; on little-endian + RGB byte order
// (red_mask_shift=16, green=8, blue=0) a u32 write lands as
// [BB, GG, RR, 00] in memory, which matches the standard 32-bpp
// LFB layout asserted in init().
pub const NAVY: u32 = 0x000A_1A2A;
pub const AMBER: u32 = 0x00FF_B200;

struct FbInfo {
    base: *mut u32,
    width: usize,
    height: usize,
    pitch_pixels: usize,
    // Mirror cursor — only print_str advances it. Independent of
    // render_string's caller-supplied (x, y), which never touches
    // the cursor.
    cursor_x: usize,
    cursor_y: usize,
}

// SAFETY: `base` is a kernel-owned HHDM pointer to the LFB region
// Limine reported. All access is mediated through FB's Mutex; no
// other code in the kernel touches the framebuffer.
unsafe impl Send for FbInfo {}

static FB: Mutex<Option<FbInfo>> = Mutex::new(None);

// Gate for print_str. Set Release after FB is populated; read
// Acquire before any FB.lock() attempt. Lets serial::write_str
// be the byte-level fan-out point regardless of whether fb::init
// has run yet — early prints (ARSENAL_BOOT_OK, the heap/frames
// banner) land on serial alone.
static FB_READY: AtomicBool = AtomicBool::new(false);

/// Stash a Limine framebuffer for use by clear / put_pixel and
/// (later) the glyph renderer + serial mirror. Asserts the layout
/// we depend on (32 bpp, RGB byte order, 4-byte-aligned pitch).
pub fn init(fb: &Framebuffer<'_>) {
    let bpp = fb.bpp();
    assert!(bpp == 32, "fb: bpp={bpp}, only 32-bit framebuffers supported in M0");
    assert!(
        fb.memory_model() == MemoryModel::RGB,
        "fb: non-RGB memory model"
    );
    let (rs, gs, bs) = (
        fb.red_mask_shift(),
        fb.green_mask_shift(),
        fb.blue_mask_shift(),
    );
    assert!(
        rs == 16 && gs == 8 && bs == 0,
        "fb: unexpected mask shifts r={rs} g={gs} b={bs}"
    );
    let pitch = fb.pitch() as usize;
    assert!(
        pitch.is_multiple_of(4),
        "fb: pitch={pitch} not 4-byte aligned"
    );

    *FB.lock() = Some(FbInfo {
        base: fb.addr() as *mut u32,
        width: fb.width() as usize,
        height: fb.height() as usize,
        pitch_pixels: pitch / 4,
        cursor_x: 0,
        cursor_y: 0,
    });
    // Release pairs with the Acquire load in print_str so any
    // reader that sees FB_READY=true also sees the populated
    // Mutex contents. Single-CPU pre-3F makes this theoretical,
    // but the discipline generalizes.
    FB_READY.store(true, Ordering::Release);
}

/// Paint every pixel with `rgb` (packed `0x00RRGGBB`).
pub fn clear(rgb: u32) {
    let guard = FB.lock();
    let info = guard.as_ref().expect("fb::clear before fb::init");
    for y in 0..info.height {
        // SAFETY: y < height and x < width keep the offset
        // (y * pitch_pixels + x) inside the LFB region Limine
        // reported. The base pointer is kernel-owned HHDM
        // mapped to the framebuffer; volatile writes match the
        // discipline we want for future architectures where the
        // mapping is uncached (Apple Silicon, M1+).
        unsafe {
            let row = info.base.add(y * info.pitch_pixels);
            for x in 0..info.width {
                row.add(x).write_volatile(rgb);
            }
        }
    }
}

/// Write one pixel; no-op on out-of-bounds. Bounds check is a
/// runtime branch but the only call site that knows dimensions at
/// compile time is clear().
pub fn put_pixel(x: usize, y: usize, rgb: u32) {
    let guard = FB.lock();
    let info = guard.as_ref().expect("fb::put_pixel before fb::init");
    if x >= info.width || y >= info.height {
        return;
    }
    // SAFETY: bounds-checked above; pointer arithmetic stays
    // inside the LFB region. Volatile write per the discipline
    // documented in clear().
    unsafe {
        info.base
            .add(y * info.pitch_pixels + x)
            .write_volatile(rgb);
    }
}

/// Render a left-to-right string starting at (x, y), advancing
/// one glyph width per byte. Iterates over `s.bytes()` rather than
/// `s.chars()` because the font is byte-indexed; ASCII passes
/// through cleanly, and non-ASCII UTF-8 bytes index into the
/// upper-half slots (mostly blank under Spleen's 0xFF coverage).
/// Clips when the next glyph's left edge would land past the
/// right margin.
pub fn render_string(s: &str, x: usize, y: usize, fg: u32, bg: u32) {
    let guard = FB.lock();
    let info = guard.as_ref().expect("fb::render_string before fb::init");
    for (i, b) in s.bytes().enumerate() {
        let gx = x + i * font::GLYPH_W;
        if gx >= info.width {
            break;
        }
        render_glyph_inner(info, b, gx, y, fg, bg);
    }
}

/// Byte-stream entry point for the serial mirror. Returns
/// immediately if fb::init hasn't run yet (the FB_READY gate)
/// or if another fb operation already holds the lock (try_lock).
/// The try_lock path matters during a panic: the panic_handler's
/// writeln!(serial::Writer, ...) routes through serial::write_str
/// which calls us; if a panic fires mid-render we drop the
/// mirror copy so the serial line at least lands.
pub fn print_str(s: &str) {
    if !FB_READY.load(Ordering::Acquire) {
        return;
    }
    let Some(mut guard) = FB.try_lock() else {
        return;
    };
    let Some(info) = guard.as_mut() else {
        return;
    };
    print_str_inner(info, s);
}

fn print_str_inner(info: &mut FbInfo, s: &str) {
    for b in s.bytes() {
        match b {
            b'\n' => {
                info.cursor_x = 0;
                info.cursor_y += font::GLYPH_H;
                maybe_scroll(info);
            }
            b'\r' => {
                info.cursor_x = 0;
            }
            _ => {
                if info.cursor_x + font::GLYPH_W > info.width {
                    info.cursor_x = 0;
                    info.cursor_y += font::GLYPH_H;
                    maybe_scroll(info);
                }
                let cx = info.cursor_x;
                let cy = info.cursor_y;
                render_glyph_inner(info, b, cx, cy, AMBER, NAVY);
                info.cursor_x += font::GLYPH_W;
            }
        }
    }
}

/// If the cursor has advanced past the last full row, blit rows
/// up by one glyph height and clear the freed bottom band to NAVY.
/// At 1280x800 the blit is ~4 MiB per scroll; M0 doesn't budget
/// against that cost. 3G's perf gate may want a circular-row
/// alternative if scroll-heavy logs dominate the boot budget.
fn maybe_scroll(info: &mut FbInfo) {
    if info.cursor_y + font::GLYPH_H <= info.height {
        return;
    }
    let scroll = font::GLYPH_H;
    let keep_rows = info.height - scroll;
    let pitch = info.pitch_pixels;
    // SAFETY: src starts `scroll * pitch` pixels into the LFB and
    // dst starts at base; len = keep_rows * pitch pixels covers
    // the surviving region exactly. src > dst, so the ranges may
    // overlap forward — ptr::copy handles that; copy_nonoverlapping
    // would be UB. All addresses stay inside the LFB.
    unsafe {
        let src = info.base.add(scroll * pitch);
        let dst = info.base;
        core::ptr::copy(src, dst, keep_rows * pitch);
    }
    for y in keep_rows..info.height {
        // SAFETY: y < info.height; inner row pointer + x stays
        // inside the LFB. Volatile per the discipline in clear().
        unsafe {
            let row = info.base.add(y * pitch);
            for x in 0..info.width {
                row.add(x).write_volatile(NAVY);
            }
        }
    }
    info.cursor_y = keep_rows;
}

/// Lock-free inner: caller owns the FbInfo borrow. Renders one
/// glyph; clips per-pixel against the framebuffer extents.
fn render_glyph_inner(info: &FbInfo, c: u8, x: usize, y: usize, fg: u32, bg: u32) {
    let base_idx = (c as usize) * font::GLYPH_H;
    for row in 0..font::GLYPH_H {
        let bits = font::FONT[base_idx + row];
        let py = y + row;
        if py >= info.height {
            break;
        }
        // SAFETY: per-pixel bounds-checked below; the row pointer
        // stays inside the LFB. Volatile write per clear()'s
        // discipline.
        let row_ptr = unsafe { info.base.add(py * info.pitch_pixels) };
        for col in 0..font::GLYPH_W {
            let px = x + col;
            if px >= info.width {
                break;
            }
            let lit = (bits >> (7 - col)) & 1 == 1;
            let color = if lit { fg } else { bg };
            // SAFETY: px < info.width and py < info.height; the
            // pointer arithmetic stays within the LFB.
            unsafe { row_ptr.add(px).write_volatile(color) };
        }
    }
}
