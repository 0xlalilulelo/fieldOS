# kernel/kernel.mk
#
# Kernel build rules. Included from the top-level Makefile after
# tools/toolchain.mk, so $(CROSS_CC) and $(CROSS_LD) are available.

KERNEL_BUILD := kernel/build
KERNEL_ELF   := $(KERNEL_BUILD)/field-kernel.elf

KERNEL_C_SRCS := \
    kernel/main.c \
    kernel/arch/x86_64/cpu.c \
    kernel/arch/x86_64/font_8x8.c \
    kernel/arch/x86_64/framebuffer.c \
    kernel/arch/x86_64/gdt.c \
    kernel/arch/x86_64/idt.c \
    kernel/arch/x86_64/serial.c \
    kernel/holyc/abi_table.c \
    kernel/holyc/asm.c \
    kernel/holyc/eval.c \
    kernel/holyc/jit.c \
    kernel/holyc/runtime.c \
    kernel/holyc/walker.c \
    kernel/lib/format.c \
    kernel/mm/pmm.c \
    kernel/mm/slab.c \
    kernel/mm/vmm.c

KERNEL_S_SRCS := \
    kernel/arch/x86_64/gdt_load.S \
    kernel/arch/x86_64/exceptions.S

KERNEL_OBJS := \
    $(patsubst %.c,$(KERNEL_BUILD)/%.o,$(KERNEL_C_SRCS)) \
    $(patsubst %.S,$(KERNEL_BUILD)/%.o,$(KERNEL_S_SRCS))

# Kernel-resident vendored holyc-lang subset. Variables live here
# (rather than in kernel/holyc/holyc-kernel.mk where the build rules
# live) because GNU Make expands the prerequisite list of the
# $(KERNEL_ELF) rule below at parse time, and that needs these to
# already be defined. The build rules and discovery targets that
# consume HOLYC_KERNEL_OBJS still live in holyc-kernel.mk; only the
# *list* of files moves here.
HOLYC_KERNEL_BUILD := build/holyc-kernel
HOLYC_KERNEL_SRCS := \
    holyc/src/aostr.c \
    holyc/src/ast.c \
    holyc/src/arena.c \
    holyc/src/cctrl.c \
    holyc/src/compile.c \
    holyc/src/containers.c \
    holyc/src/lexer.c \
    holyc/src/list.c \
    holyc/src/parser.c \
    holyc/src/prsasm.c \
    holyc/src/prslib.c \
    holyc/src/prsutil.c \
    holyc/src/x86.c
HOLYC_KERNEL_OBJS := \
    $(patsubst holyc/src/%.c,$(HOLYC_KERNEL_BUILD)/%.o,$(HOLYC_KERNEL_SRCS)) \
    $(HOLYC_KERNEL_BUILD)/math_shim.o

# -ffreestanding         no hosted libc; we provide our own runtime
# -nostdlib              don't link any libc
# -fno-stack-protector   no canary helpers; we have no libc to link
# -fno-pic / -fno-pie    fixed virtual address per linker script
# -mno-red-zone          no 128-byte stack red zone (kernel-correct)
# -mno-mmx / -mno-sse    no SIMD; would need explicit save/restore
# -mcmodel=kernel        large-model addressing for the higher-half VA
KERNEL_CFLAGS := \
    -ffreestanding -nostdlib \
    -fno-stack-protector -fno-stack-check \
    -fno-pic -fno-pie \
    -mno-red-zone -mno-mmx -mno-sse -mno-sse2 \
    -mcmodel=kernel \
    -O2 -g -Wall -Wextra \
    -std=gnu11 \
    -ffunction-sections -fdata-sections \
    -I kernel \
    -I vendor/limine

# --gc-sections strips unreached .text.* / .data.* / .rodata.*
# sections so the kernel-resident hcc subset (ast.o etc.) does not
# pull every vendored function into the kernel ELF; only what kmain
# transitively reaches survives. Limine request structs are KEEP()'d
# in the linker script and __attribute__((used)) on their globals,
# so they pass the GC unconditionally.
KERNEL_LDFLAGS := \
    -nostdlib -static \
    -m elf_x86_64 \
    -z max-page-size=0x1000 \
    --gc-sections \
    -T kernel/arch/x86_64/linker.ld

$(KERNEL_BUILD)/%.o: %.c
	@mkdir -p $(@D)
	$(CROSS_CC) $(KERNEL_CFLAGS) -c $< -o $@

$(KERNEL_BUILD)/%.o: %.S
	@mkdir -p $(@D)
	$(CROSS_CC) $(KERNEL_CFLAGS) -c $< -o $@

$(KERNEL_ELF): $(KERNEL_OBJS) $(HOLYC_KERNEL_OBJS) kernel/arch/x86_64/linker.ld
	@mkdir -p $(@D)
	$(CROSS_LD) $(KERNEL_LDFLAGS) -o $@ $(KERNEL_OBJS) $(HOLYC_KERNEL_OBJS)
