// SPDX-License-Identifier: BSD-2-Clause
//
// virtio-gpu driver (M1 step 4) — native Rust, the CI-substrate GPU
// inserted before amdgpu (QEMU does not emulate amdgpu, so without
// virtio-gpu the amdgpu step would have no per-commit smoke).
//
// 4-0 state: transport bring-up + the control queue + GET_DISPLAY_INFO.
// Find the device (modern PCI device id 0x1050), run the v1.2 § 3.1.1
// init dance via the shared virtio transport (the same path virtio-blk
// and virtio-net use), activate the control virtqueue (queue 0), bring
// the device DRIVER_OK, then issue VIRTIO_GPU_CMD_GET_DISPLAY_INFO and
// parse the enabled scanout's geometry into a `display::DisplayInfo`.
// The control queue is polled (yield-loop) like virtio_blk's smoke —
// the command set is low-frequency and bring-up runs on the boot stack
// before sched::init, so an interrupt path buys nothing yet.
//
// 4-1 will add RESOURCE_CREATE_2D + RESOURCE_ATTACH_BACKING; 4-2 adds
// SET_SCANOUT + TRANSFER_TO_HOST_2D + RESOURCE_FLUSH and lands the GPU
// sentinel. The whole sequence runs synchronously inside `init()` (the
// nvme/virtio-blk shape); nothing persists past bring-up because there
// is no compositor consumer until M2.
//
// References:
//   virtio v1.2 § 5.7 (GPU device)
//   virtio v1.2 § 3.1.1 (device init), § 4.1.4 (modern PCI transport)

use core::fmt::Write;
use core::ptr::read_volatile;

use alloc::boxed::Box;

use crate::display::{DisplayInfo, PixelFormat};
use crate::{paging, sched, serial, virtio};

// virtio-gpu is modern-only: PCI device id 0x1040 + virtio device type
// 16 = 0x1050. Unlike blk (0x1001) / net (0x1000) there is no
// transitional id.
const VIRTIO_GPU_DEVICE_ID: u16 = 0x1050;

// virtio_gpu_config field offsets (v1.2 § 5.7.4), read from device_cfg.
const GPU_CFG_NUM_SCANOUTS: usize = 8;

// Control queue is queue 0 (cursor queue is 1, unused at M1). A handful
// of commands ever issue, so a 16-descriptor ring is ample.
const CONTROL_QUEUE_IDX: u16 = 0;
const CONTROL_QUEUE_SIZE: u16 = 16;

// virtio_gpu_ctrl_type values (v1.2 § 5.7.6). 2D command subset only.
const VIRTIO_GPU_CMD_GET_DISPLAY_INFO: u32 = 0x0100;
const VIRTIO_GPU_CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
const VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;
const VIRTIO_GPU_RESP_OK_NODATA: u32 = 0x1100;
const VIRTIO_GPU_RESP_OK_DISPLAY_INFO: u32 = 0x1101;

// virtio_gpu_formats (v1.2 § 5.7.3): B8G8R8X8_UNORM is the 32-bpp
// little-endian XRGB layout — `[B, G, R, X]` in memory — matching
// display::PixelFormat::Xrgb8888.
const VIRTIO_GPU_FORMAT_B8G8R8X8_UNORM: u32 = 2;

// The single 2D resource we create for the scanout. Resource ids are
// driver-assigned, host-tracked handles; any non-zero value works.
const SCANOUT_RESOURCE_ID: u32 = 1;

// VIRTIO_GPU_MAX_SCANOUTS (v1.2 § 5.7.6.1) — the fixed pmodes array
// length in the display-info response.
const MAX_SCANOUTS: usize = 16;

/// virtio_gpu_ctrl_hdr (v1.2 § 5.7.6.7) — the 24-byte header every
/// command and response carries. We issue un-fenced commands (flags 0).
#[repr(C)]
struct CtrlHdr {
    type_: u32,
    flags: u32,
    fence_id: u64,
    ctx_id: u32,
    ring_idx: u8,
    padding: [u8; 3],
}

/// virtio_gpu_rect — a scanout/resource rectangle.
#[repr(C)]
struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

/// virtio_gpu_display_one — one scanout's mode in the display-info
/// response: its rectangle, whether it is enabled, and flags.
#[repr(C)]
struct DisplayOne {
    r: Rect,
    enabled: u32,
    flags: u32,
}

/// virtio_gpu_resp_display_info — the GET_DISPLAY_INFO response: a
/// header plus a fixed array of per-scanout modes.
#[repr(C)]
struct RespDisplayInfo {
    hdr: CtrlHdr,
    pmodes: [DisplayOne; MAX_SCANOUTS],
}

/// virtio_gpu_resource_create_2d (v1.2 § 5.7.6.8) request body.
#[repr(C)]
struct ResourceCreate2d {
    hdr: CtrlHdr,
    resource_id: u32,
    format: u32,
    width: u32,
    height: u32,
}

/// virtio_gpu_mem_entry — one scatter-gather backing entry.
#[repr(C)]
struct MemEntry {
    addr: u64,
    length: u32,
    padding: u32,
}

/// virtio_gpu_resource_attach_backing (v1.2 § 5.7.6.10) with exactly
/// one backing entry. The framebuffer is a single physically-
/// contiguous heap allocation, so one entry suffices (no scatter-
/// gather list); a fragmented backing would carry `nr_entries` of
/// these.
#[repr(C)]
struct ResourceAttachBackingOne {
    hdr: CtrlHdr,
    resource_id: u32,
    nr_entries: u32,
    entry: MemEntry,
}

/// Request + response laid out contiguously so a single heap
/// allocation backs both descriptors of the control-queue chain (the
/// device reads `req`, writes `resp`). Mirrors virtio_blk's boxed
/// request. `R` is the command request body; the response is always a
/// bare header for the OK_NODATA commands, and `RespDisplayInfo` for
/// GET_DISPLAY_INFO.
#[repr(C)]
struct Xfer<R, P> {
    req: R,
    resp: P,
}

impl CtrlHdr {
    /// A zeroed header carrying `type_` — an un-fenced command request.
    fn cmd(type_: u32) -> Self {
        CtrlHdr {
            type_,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        }
    }
}

/// Bring up the virtio-gpu device and read its display geometry (4-0).
/// No-ops (logs) when no virtio-gpu device is present, so the
/// production smoke without `-device virtio-gpu-pci` is unaffected —
/// the xhci.rs / nvme.rs precedent.
pub fn init() {
    let Some(dev) = virtio::find_device(VIRTIO_GPU_DEVICE_ID) else {
        let _ = writeln!(serial::Writer, "gpu: no virtio-gpu device found");
        return;
    };

    let _ = writeln!(
        serial::Writer,
        "gpu: device at {:02x}:{:02x}.{} common={:p} notify={:p}",
        dev.bus, dev.dev, dev.func, dev.common_cfg, dev.notify_base,
    );

    // Decline every optional feature (no VIRGL/3D, no EDID) — request
    // only VERSION_1, forcing the modern transport, exactly as
    // virtio-blk does.
    let driver_features = (virtio::VIRTIO_F_VERSION_1 as u64) << 32;
    let device_features = virtio::init_transport(&dev, driver_features);

    // num_scanouts from the device config region.
    // SAFETY: device_cfg is the mapped DEVICE_CFG MMIO region from
    // find_device; GPU_CFG_NUM_SCANOUTS (8) is within it.
    let num_scanouts =
        unsafe { read_volatile(dev.device_cfg.add(GPU_CFG_NUM_SCANOUTS) as *const u32) };
    let _ = writeln!(
        serial::Writer,
        "gpu: features dev={device_features:#018x} drv={driver_features:#018x} \
         num_scanouts={num_scanouts}",
    );

    // Control queue (queue 0), then DRIVER_OK so commands can flow.
    let mut ctrlq = virtio::Virtqueue::new(CONTROL_QUEUE_SIZE);
    let notify_ptr = virtio::activate_queue(&dev, CONTROL_QUEUE_IDX, &ctrlq);
    virtio::set_driver_ok(&dev);

    let info = get_display_info(&mut ctrlq, notify_ptr);
    let _ = writeln!(
        serial::Writer,
        "gpu: display info {}x{} format={:?} (scanout 0 enabled)",
        info.width, info.height, info.format,
    );

    // 4-1: create a 2D resource matching the scanout and attach a
    // physically-contiguous framebuffer as its backing. The heap is a
    // single contiguous physical region, so one heap allocation is one
    // contiguous backing (a single mem_entry, no scatter-gather).
    //
    // `fb` lives for the rest of init(): 4-2 writes a pattern into it
    // and issues TRANSFER_TO_HOST_2D + RESOURCE_FLUSH before init
    // returns. The device only DMAs the backing on TRANSFER, so the
    // backing is never read between attach (here) and that transfer;
    // when init returns and `fb` drops, no further DMA touches it
    // (there is no compositor consumer until M2).
    let fb_words = (info.width as usize) * (info.height as usize);
    let fb: Box<[u32]> = alloc::vec![0u32; fb_words].into_boxed_slice();
    let fb_phys = (fb.as_ptr() as u64) - paging::hhdm_offset();
    let fb_bytes = (fb_words * 4) as u32;

    cmd_nodata(
        &mut ctrlq,
        notify_ptr,
        ResourceCreate2d {
            hdr: CtrlHdr::cmd(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D),
            resource_id: SCANOUT_RESOURCE_ID,
            format: VIRTIO_GPU_FORMAT_B8G8R8X8_UNORM,
            width: info.width,
            height: info.height,
        },
        "RESOURCE_CREATE_2D",
    );

    cmd_nodata(
        &mut ctrlq,
        notify_ptr,
        ResourceAttachBackingOne {
            hdr: CtrlHdr::cmd(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING),
            resource_id: SCANOUT_RESOURCE_ID,
            nr_entries: 1,
            entry: MemEntry {
                addr: fb_phys,
                length: fb_bytes,
                padding: 0,
            },
        },
        "RESOURCE_ATTACH_BACKING",
    );

    let _ = writeln!(
        serial::Writer,
        "gpu: resource {SCANOUT_RESOURCE_ID} created ({}x{}) + backing attached \
         ({fb_bytes} bytes @ {fb_phys:#x})",
        info.width, info.height,
    );
}

/// Push a two-descriptor control-queue chain — request (device-read) →
/// response (device-write) — notify the device, and poll the used ring
/// until the command completes. Shared by every command. yield_now
/// early-returns pre-sched (empty runqueue); commands complete in
/// microseconds under TCG.
fn submit(
    ctrlq: &mut virtio::Virtqueue,
    notify_ptr: *mut u16,
    req_phys: u64,
    req_len: u32,
    resp_phys: u64,
    resp_len: u32,
) {
    ctrlq
        .push_chain(&[
            (req_phys, req_len, 0),
            (resp_phys, resp_len, virtio::VIRTQ_DESC_F_WRITE),
        ])
        .expect("gpu: control queue full");
    virtio::notify(notify_ptr, CONTROL_QUEUE_IDX);
    let mut spins = 0u64;
    while ctrlq.pop_used().is_none() {
        sched::yield_now();
        spins += 1;
        assert!(spins <= 1_000_000, "gpu: command never completed");
    }
}

/// Issue a command whose request body is `req` and which the device
/// answers with a bare `OK_NODATA` header (RESOURCE_CREATE_2D,
/// ATTACH_BACKING, and the 4-2 scanout/transfer/flush commands all do).
/// Panics on any other response. `R` is the repr(C) request body; it is
/// boxed contiguously with the response header so one heap allocation
/// backs both descriptors (the virtio_blk pattern).
fn cmd_nodata<R>(ctrlq: &mut virtio::Virtqueue, notify_ptr: *mut u16, req: R, what: &str) {
    let xfer = Box::new(Xfer {
        req,
        resp: CtrlHdr::cmd(0),
    });
    let hhdm = paging::hhdm_offset();
    let req_phys = (&xfer.req as *const R as u64) - hhdm;
    let resp_phys = (&xfer.resp as *const CtrlHdr as u64) - hhdm;
    submit(
        ctrlq,
        notify_ptr,
        req_phys,
        core::mem::size_of::<R>() as u32,
        resp_phys,
        core::mem::size_of::<CtrlHdr>() as u32,
    );
    // SAFETY: xfer is a live Box; resp was filled by the device. Volatile
    // defeats constant-folding back to the cmd(0) initializer.
    let resp_type = unsafe { read_volatile(&xfer.resp.type_ as *const u32) };
    assert_eq!(
        resp_type, VIRTIO_GPU_RESP_OK_NODATA,
        "gpu: {what} response type {resp_type:#06x} (wanted {VIRTIO_GPU_RESP_OK_NODATA:#06x}=OK_NODATA)"
    );
}

/// Issue VIRTIO_GPU_CMD_GET_DISPLAY_INFO on the control queue and parse
/// the first enabled scanout's geometry. Panics if the device returns
/// anything but RESP_OK_DISPLAY_INFO or reports no enabled scanout — a
/// headless QEMU virtio-gpu always enables scanout 0.
fn get_display_info(ctrlq: &mut virtio::Virtqueue, notify_ptr: *mut u16) -> DisplayInfo {
    // Box the request+response so the device DMAs by physical address
    // derived from the heap virtual address via HHDM (the virtio_blk
    // pattern). resp is zeroed; the device overwrites it.
    let xfer = Box::new(Xfer {
        req: CtrlHdr::cmd(VIRTIO_GPU_CMD_GET_DISPLAY_INFO),
        // SAFETY: RespDisplayInfo is plain repr(C) POD; an all-zero bit
        // pattern is a valid value (the device overwrites it anyway).
        resp: unsafe { core::mem::zeroed::<RespDisplayInfo>() },
    });

    let hhdm = paging::hhdm_offset();
    let req_phys = (&xfer.req as *const CtrlHdr as u64) - hhdm;
    let resp_phys = (&xfer.resp as *const RespDisplayInfo as u64) - hhdm;
    submit(
        ctrlq,
        notify_ptr,
        req_phys,
        core::mem::size_of::<CtrlHdr>() as u32,
        resp_phys,
        core::mem::size_of::<RespDisplayInfo>() as u32,
    );

    // Read the response via volatile so the compiler can't constant-fold
    // it back to the zeroed initializer.
    // SAFETY: xfer is a live Box; resp was filled by the device.
    let resp_type = unsafe { read_volatile(&xfer.resp.hdr.type_ as *const u32) };
    assert_eq!(
        resp_type, VIRTIO_GPU_RESP_OK_DISPLAY_INFO,
        "gpu: GET_DISPLAY_INFO response type {resp_type:#06x} (wanted {VIRTIO_GPU_RESP_OK_DISPLAY_INFO:#06x})"
    );

    // First enabled scanout's rectangle gives the display geometry.
    for i in 0..MAX_SCANOUTS {
        // SAFETY: i < MAX_SCANOUTS; pmodes is a fixed array in the live
        // response; volatile reads defeat constant-folding.
        let (enabled, width, height) = unsafe {
            let m = &xfer.resp.pmodes[i];
            (
                read_volatile(&m.enabled as *const u32),
                read_volatile(&m.r.width as *const u32),
                read_volatile(&m.r.height as *const u32),
            )
        };
        if enabled != 0 {
            return DisplayInfo {
                width,
                height,
                format: PixelFormat::Xrgb8888,
            };
        }
    }
    panic!("gpu: GET_DISPLAY_INFO reported no enabled scanout");
}
