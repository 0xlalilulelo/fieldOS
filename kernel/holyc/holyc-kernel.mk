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

# Witness sources for the in-kernel hcc subset. M3-B candidate B
# started at one file (aostr.c) at 95e8012 to exercise runtime.c's
# weak malloc/free shim; B-followup-2 (this set) expands to the four
# files that cctrl.c reaches for transitively — ast.c, arena.c, and
# containers.c — so the partial-link target below can produce a
# single relocatable .o whose residual undefined-symbol set is
# what candidate C must close before the kernel ELF can link the
# subset.
HOLYC_KERNEL_SRCS := \
    holyc/src/aostr.c \
    holyc/src/ast.c \
    holyc/src/arena.c \
    holyc/src/containers.c

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

# The kernel-side .o files the holyc subset will pair with once the
# kernel ELF link picks it up (candidate C). Listed explicitly rather
# than pulling in $(KERNEL_OBJS) so the partial-link target stays a
# closure of "what runtime.c reaches for transitively" rather than a
# whole-kernel rebuild gate.
HOLYC_KERNEL_LINK_DEPS := \
    $(KERNEL_BUILD)/kernel/holyc/runtime.o \
    $(KERNEL_BUILD)/kernel/mm/slab.o \
    $(KERNEL_BUILD)/kernel/mm/pmm.o \
    $(KERNEL_BUILD)/kernel/lib/format.o \
    $(KERNEL_BUILD)/kernel/arch/x86_64/serial.o

HOLYC_KERNEL_LINK_O := $(HOLYC_KERNEL_BUILD)/holyc-kernel-subset.o

.PHONY: holyc-kernel-subset holyc-kernel-subset-clean \
        holyc-kernel-subset-syms holyc-kernel-subset-link

holyc-kernel-subset: $(HOLYC_KERNEL_OBJS)
	@echo "==> $(words $(HOLYC_KERNEL_OBJS)) object(s) under $(HOLYC_KERNEL_BUILD)/"
	@for o in $(HOLYC_KERNEL_OBJS); do \
	  echo "  $$o ($$($(CROSS_OBJDUMP) -h $$o | awk '/\.text/{print $$3}' | head -1) text bytes)"; \
	done

# Per-object undefined-symbol report. Useful for tracing which file
# contributes which gap; noisy because cross-references between
# subset members appear in both directions. The partial-link target
# below is the cleaner post-resolution view.
holyc-kernel-subset-syms: $(HOLYC_KERNEL_OBJS)
	@echo "==> undefined symbols across the kernel-resident hcc subset"
	@for o in $(HOLYC_KERNEL_OBJS); do \
	  $(TOOLCHAIN_BIN)/$(TOOLCHAIN_TARGET)-nm -u $$o | awk '{print $$2}' | sort -u | while read s; do \
	    echo "  $$s   (in $$(basename $$o))"; \
	  done; \
	done | sort -u

# Discovery deliverable: ld -r the holyc subset together with the
# kernel-side .o files runtime.c reaches for, then nm -u the result.
# Internal cross-references resolve; the residuals are the real gaps
# that must close before candidate C can link the subset into the
# kernel ELF. The output .o is itself partial-linkable, but is not
# included in the kernel build — the boundary stays load-bearing.
$(HOLYC_KERNEL_LINK_O): $(HOLYC_KERNEL_OBJS) $(HOLYC_KERNEL_LINK_DEPS)
	@mkdir -p $(@D)
	$(CROSS_LD) -r -o $@ $(HOLYC_KERNEL_OBJS) $(HOLYC_KERNEL_LINK_DEPS)

holyc-kernel-subset-link: $(HOLYC_KERNEL_LINK_O)
	@echo "==> partial-linked $(HOLYC_KERNEL_LINK_O)"
	@echo "    inputs: $(words $(HOLYC_KERNEL_OBJS)) hcc + $(words $(HOLYC_KERNEL_LINK_DEPS)) kernel-side"
	@echo "    text: $$($(CROSS_OBJDUMP) -h $(HOLYC_KERNEL_LINK_O) | awk '/\.text/{print $$3}' | head -1) bytes"
	@echo "==> residual undefined symbols (filtered: limine_*_request_struct"
	@echo "    are partial-link artifacts, defined in kernel/main.c which"
	@echo "    is intentionally not in HOLYC_KERNEL_LINK_DEPS)"
	@$(TOOLCHAIN_BIN)/$(TOOLCHAIN_TARGET)-nm -u $(HOLYC_KERNEL_LINK_O) \
	  | awk '{print $$2}' | grep -vE '^limine_(hhdm|memmap)_request_struct$$' \
	  | sort -u | sed 's/^/  /'

holyc-kernel-subset-clean:
	rm -rf $(HOLYC_KERNEL_BUILD)
