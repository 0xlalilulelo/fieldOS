# kernel/holyc/holyc-kernel.mk
#
# Build rules for the kernel-resident subset of the vendored
# holyc-lang compiler. Compiled but NOT linked into the kernel ELF
# yet — that integration is M3-B candidate C (the holyc_eval entry
# point). This file's job is pure discovery per ADR-0001 §3 step 3:
# "drives the strip plan against measured ground."
#
# Vendored sources are compiled with the cross-GCC under
# -ffreestanding -nostdlib against the libc-shaped shim headers in
# kernel/holyc/include/ and the freestanding runtime in
# kernel/holyc/runtime.{c,h}. The .o files surface their gaps as
# undefined symbols; `make holyc-kernel-subset-syms` reports them
# so the next commit knows what to add to the runtime.
#
# Why a separate .mk: the kernel ELF and the holyc kernel-resident
# subset have different compile flags (-w on the vendored subset to
# silence upstream warnings we do not control) and different output
# directories (build/holyc-kernel/ vs kernel/build/). Mixing them in
# kernel.mk would obscure the boundary that's load-bearing for
# ADR-0001 §3 step 3's discovery framing.

HOLYC_KERNEL_BUILD := build/holyc-kernel

# The witness file. M3-B candidate B starts at one source — aostr.c —
# because it is the immediate consumer of runtime.c's weak malloc/free
# shim and therefore the smallest possible test of (A)'s integration.
# Extending this list happens in follow-up commits as each new file's
# undefined-symbol set is resolved.
HOLYC_KERNEL_SRCS := \
    holyc/src/aostr.c

HOLYC_KERNEL_OBJS := \
    $(patsubst holyc/src/%.c,$(HOLYC_KERNEL_BUILD)/%.o,$(HOLYC_KERNEL_SRCS))

# Build flags mirror KERNEL_CFLAGS for the parts that affect the ABI
# (-mcmodel, no-red-zone, freestanding) so the .o files can
# eventually link with the kernel ELF. -w silences upstream warnings;
# the vendored tree is owned by the audit, not by Field OS code style.
# Include order matters: kernel/holyc/include comes first so our shim
# wins over any cross-GCC freestanding header that happens to ship
# under the same name.
#
# SSE/MMX deliberately NOT disabled here, unlike KERNEL_CFLAGS in
# kernel/kernel.mk:41. Vendored aostr.c reads variadic doubles via
# va_arg(ap, double) for %f formatting; the SysV AMD64 ABI puts
# variadic floats in xmm registers, and -mno-sse refuses to compile
# the load. M3-B candidate B's first architectural discovery: linking
# this subset into the kernel requires either (i) extending the IDT
# exception entry path to save/restore xmm state (M4-aligned work,
# touches kernel/arch/x86_64/exceptions.S) or (ii) running hcc with
# interrupts disabled (poor M3 PoC compromise). Tracked for ADR-0002
# if it becomes the dominant constraint; not decided in this commit.
HOLYC_KERNEL_CFLAGS := \
    -ffreestanding -nostdlib \
    -fno-stack-protector -fno-stack-check \
    -fno-pic -fno-pie \
    -mno-red-zone \
    -mcmodel=kernel \
    -O2 -g \
    -std=gnu11 \
    -w \
    -I kernel/holyc/include \
    -I kernel \
    -I holyc/src

$(HOLYC_KERNEL_BUILD)/%.o: holyc/src/%.c
	@mkdir -p $(@D)
	$(CROSS_CC) $(HOLYC_KERNEL_CFLAGS) -c $< -o $@

.PHONY: holyc-kernel-subset holyc-kernel-subset-clean holyc-kernel-subset-syms

holyc-kernel-subset: $(HOLYC_KERNEL_OBJS)
	@echo "==> $(words $(HOLYC_KERNEL_OBJS)) object(s) under $(HOLYC_KERNEL_BUILD)/"
	@for o in $(HOLYC_KERNEL_OBJS); do \
	  echo "  $$o ($$($(CROSS_OBJDUMP) -h $$o | awk '/\.text/{print $$3}' | head -1) text bytes)"; \
	done

# Discovery deliverable: the undefined symbols that the runtime must
# eventually provide (or that vendored callers must be stripped from
# the kernel-resident subset). Run after `make holyc-kernel-subset`.
holyc-kernel-subset-syms: $(HOLYC_KERNEL_OBJS)
	@echo "==> undefined symbols across the kernel-resident hcc subset"
	@for o in $(HOLYC_KERNEL_OBJS); do \
	  $(TOOLCHAIN_BIN)/$(TOOLCHAIN_TARGET)-nm -u $$o | awk '{print $$2}' | sort -u | while read s; do \
	    echo "  $$s   (in $$(basename $$o))"; \
	  done; \
	done | sort -u

holyc-kernel-subset-clean:
	rm -rf $(HOLYC_KERNEL_BUILD)
