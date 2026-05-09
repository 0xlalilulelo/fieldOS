// SPDX-License-Identifier: BSD-2-Clause
//
// GDT + TSS bring-up. Replaces Limine's GDT with our own kernel CS/DS
// pair and a TSS whose IST table points at three reserved stacks for
// the faults that cannot share a stack with normal kernel code:
// #DF (double fault), #NMI, and #MC (machine check). The IDT (2-3)
// references these IST indices when wiring its handlers.

use spin::Lazy;
use x86_64::VirtAddr;
use x86_64::instructions::segmentation::{CS, DS, ES, FS, GS, SS, Segment};
use x86_64::instructions::tables::load_tss;
use x86_64::registers::segmentation::SegmentSelector;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable};
use x86_64::structures::tss::TaskStateSegment;

pub const DOUBLE_FAULT_IST: u16 = 0;
pub const NMI_IST: u16 = 1;
pub const MACHINE_CHECK_IST: u16 = 2;

const IST_STACK_SIZE: usize = 4096 * 5; // 20 KiB per IST stack

#[repr(C, align(16))]
struct IstStack([u8; IST_STACK_SIZE]);

static mut DOUBLE_FAULT_STACK: IstStack = IstStack([0; IST_STACK_SIZE]);
static mut NMI_STACK: IstStack = IstStack([0; IST_STACK_SIZE]);
static mut MACHINE_CHECK_STACK: IstStack = IstStack([0; IST_STACK_SIZE]);

fn stack_top(ptr: *mut IstStack) -> VirtAddr {
    let base = ptr as u64;
    VirtAddr::new(base + IST_STACK_SIZE as u64)
}

static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    // The Lazy initializer runs at most once. The CPU writes through
    // these addresses when invoking the IST during exception delivery;
    // we only ever yield raw pointers (never `&mut` references), so
    // there is no Rust-side aliasing concern when the hardware writes
    // simultaneously with kernel code that holds the original static.
    tss.interrupt_stack_table[DOUBLE_FAULT_IST as usize] =
        stack_top(&raw mut DOUBLE_FAULT_STACK);
    tss.interrupt_stack_table[NMI_IST as usize] = stack_top(&raw mut NMI_STACK);
    tss.interrupt_stack_table[MACHINE_CHECK_IST as usize] =
        stack_top(&raw mut MACHINE_CHECK_STACK);
    tss
});

pub struct Selectors {
    pub kernel_code: SegmentSelector,
    pub kernel_data: SegmentSelector,
    pub tss: SegmentSelector,
}

static GDT: Lazy<(GlobalDescriptorTable, Selectors)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    let kernel_code = gdt.append(Descriptor::kernel_code_segment());
    let kernel_data = gdt.append(Descriptor::kernel_data_segment());
    let tss = gdt.append(Descriptor::tss_segment(&TSS));
    (
        gdt,
        Selectors {
            kernel_code,
            kernel_data,
            tss,
        },
    )
});

pub fn init() {
    GDT.0.load();
    // SAFETY: the GDT just loaded contains valid kernel code/data
    // descriptors at GDT.1.kernel_code/kernel_data and a valid TSS
    // descriptor at GDT.1.tss. Reloading the segment registers
    // here is the canonical handoff from Limine's GDT to ours.
    unsafe {
        CS::set_reg(GDT.1.kernel_code);
        DS::set_reg(GDT.1.kernel_data);
        SS::set_reg(GDT.1.kernel_data);
        ES::set_reg(GDT.1.kernel_data);
        FS::set_reg(SegmentSelector::NULL);
        GS::set_reg(SegmentSelector::NULL);
        load_tss(GDT.1.tss);
    }
}
