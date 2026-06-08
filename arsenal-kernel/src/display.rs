// SPDX-License-Identifier: BSD-2-Clause
//
// Shared display vocabulary (M1 step 4). Plain data describing a
// presentable surface — resolution + pixel format — that any display
// backend populates: the Limine linear framebuffer (fb.rs) today, the
// virtio-gpu scanout (virtio_gpu.rs) at M1, amdgpu KMS at step 5.
//
// No trait yet. Per the 4-0 design decision, the cross-driver Display
// trait is deferred to step 5, when amdgpu becomes the second GPU
// backend and the unification is informed by two real GPU drivers
// rather than one. At M1 the Limine LFB (write-through, no flush) and
// a GPU (explicit transfer + flush) have genuinely different present
// models; a trait papering over them at n=1-GPU would be designed
// against too little. The vocabulary is the durable part and lands
// now; the trait is cheap to add once it is no longer speculative.

/// Pixel layout of a presentable surface. M1 ships one format: a
/// 32-bpp little-endian XRGB word (`0x00RRGGBB`), which lands in
/// memory as `[B, G, R, X]` — matching both the Limine LFB layout
/// fb.rs asserts (red shift 16 / green 8 / blue 0) and the virtio-gpu
/// `B8G8R8X8_UNORM` 2D resource format requested in virtio_gpu.rs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PixelFormat {
    /// 32-bpp `0x00RRGGBB`; in memory `[B, G, R, X]`.
    Xrgb8888,
}

/// Geometry + format of a presentable surface. Populated by a display
/// backend at bring-up: virtio-gpu reads it from `GET_DISPLAY_INFO`
/// (4-0); the Limine LFB has it from the bootloader.
#[derive(Clone, Copy, Debug)]
pub struct DisplayInfo {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
}
