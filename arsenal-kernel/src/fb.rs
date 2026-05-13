// SPDX-License-Identifier: BSD-2-Clause
//
// Linear framebuffer access. Limine maps the LFB into HHDM for us;
// fb::init stashes the framebuffer's shape, fb::clear paints the
// whole frame, fb::put_pixel writes one pixel. 3E-2 composes these
// into render_glyph / render_string; 3E-3 wires fmt::Write and the
// serial mirror.

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
}

// SAFETY: `base` is a kernel-owned HHDM pointer to the LFB region
// Limine reported. All access is mediated through FB's Mutex; no
// other code in the kernel touches the framebuffer.
unsafe impl Send for FbInfo {}

static FB: Mutex<Option<FbInfo>> = Mutex::new(None);

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
    });
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
