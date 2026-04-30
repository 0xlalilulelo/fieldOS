# kernel/kernel.mk
#
# Kernel build rules. Included from the top-level Makefile after
# tools/toolchain.mk, so $(CROSS_CC) and $(CROSS_LD) are available.

KERNEL_BUILD := kernel/build
KERNEL_ELF   := $(KERNEL_BUILD)/field-kernel.elf

KERNEL_C_SRCS := \
    kernel/main.c \
    kernel/arch/x86_64/gdt.c \
    kernel/arch/x86_64/serial.c

KERNEL_S_SRCS := \
    kernel/arch/x86_64/gdt_load.S

KERNEL_OBJS := \
    $(patsubst %.c,$(KERNEL_BUILD)/%.o,$(KERNEL_C_SRCS)) \
    $(patsubst %.S,$(KERNEL_BUILD)/%.o,$(KERNEL_S_SRCS))

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
    -I kernel \
    -I vendor/limine

KERNEL_LDFLAGS := \
    -nostdlib -static \
    -m elf_x86_64 \
    -z max-page-size=0x1000 \
    -T kernel/arch/x86_64/linker.ld

$(KERNEL_BUILD)/%.o: %.c
	@mkdir -p $(@D)
	$(CROSS_CC) $(KERNEL_CFLAGS) -c $< -o $@

$(KERNEL_BUILD)/%.o: %.S
	@mkdir -p $(@D)
	$(CROSS_CC) $(KERNEL_CFLAGS) -c $< -o $@

$(KERNEL_ELF): $(KERNEL_OBJS) kernel/arch/x86_64/linker.ld
	@mkdir -p $(@D)
	$(CROSS_LD) $(KERNEL_LDFLAGS) -o $@ $(KERNEL_OBJS)
