# Field OS top-level Makefile.
#
# The build chain is plain GNU Make for Phase 0. Per-component .mk
# files are include'd here as the project grows. Cross-compiler paths
# come from tools/toolchain.mk; refer to $(CROSS_CC) and friends,
# never bare `gcc`.

include tools/toolchain.mk
include kernel/kernel.mk
include kernel/holyc/holyc-kernel.mk
include holyc/holyc.mk

# REPL=1 (set by the `repl-iso` phony below via recursive make) selects
# field-os-poc-repl.iso + a separate build dir; default builds stay
# byte-identical to the smoke path. ADR-0001 §3 step 6.
ISO         := field-os-poc$(if $(REPL),-repl,).iso
ISO_ROOT    := build/iso-root$(if $(REPL),-repl,)
LIMINE_DIR  := vendor/limine
LIMINE_HOST := $(LIMINE_DIR)/limine

.DEFAULT_GOAL := help
.PHONY: help toolchain-check iso repl-iso limine-host clean distclean

# --- help ----------------------------------------------------------------
help:
	@echo "Field OS build targets"
	@echo ""
	@echo "  iso               Build $(ISO) (smoke-side; FIELDOS_REPL off)."
	@echo "  repl-iso          Build field-os-poc-repl.iso (interactive; FIELDOS_REPL=1)."
	@echo "  toolchain-check   Verify the x86_64-elf cross-compiler works."
	@echo "  holyc-host        Build the vendored holyc-lang compiler as a host tool."
	@echo "  holyc-host-smoke  Transpile holyc/bug-tests/Bug_171.HC to confirm the host build works."
	@echo "  holyc-kernel-subset       Compile the kernel-resident hcc subset (no link)."
	@echo "  holyc-kernel-subset-syms  Report undefined symbols per-object in the hcc subset."
	@echo "  holyc-kernel-subset-link  Partial-link the hcc subset + runtime; report residuals."
	@echo "  clean             Remove build artifacts."
	@echo "  distclean         Remove build artifacts and toolchain install."
	@echo "  help              This message."
	@echo ""
	@echo "Toolchain prefix: $(TOOLCHAIN_PREFIX)"
	@echo "Cross-CC:         $(CROSS_CC)"

# --- toolchain-check -----------------------------------------------------
toolchain-check:
	@[ -x "$(CROSS_CC)" ] || { echo "missing $(CROSS_CC) — run tools/build-toolchain.sh"; exit 1; }
	@[ -x "$(CROSS_LD)" ] || { echo "missing $(CROSS_LD) — run tools/build-toolchain.sh"; exit 1; }
	@echo "==> $$($(CROSS_CC) --version | head -1)"
	@echo "==> $$($(CROSS_LD) --version | head -1)"
	@tmp=$$(mktemp -d) && \
	  printf 'int field_os_toolchain_smoke = 42;\n' > $$tmp/t.c && \
	  $(CROSS_CC) -ffreestanding -nostdlib -c $$tmp/t.c -o $$tmp/t.o && \
	  echo "==> compile OK ($$(file $$tmp/t.o | sed 's/^[^:]*: //'))" && \
	  rm -rf $$tmp
	@echo "==> toolchain OK"

# --- limine-host ---------------------------------------------------------
# Build the host `limine` CLI used for `bios-install`. Idempotent —
# the upstream Makefile only rebuilds limine.c when changed.
$(LIMINE_HOST): $(LIMINE_DIR)/limine.c $(LIMINE_DIR)/Makefile
	$(MAKE) -C $(LIMINE_DIR)

limine-host: $(LIMINE_HOST)

# --- iso -----------------------------------------------------------------
$(ISO): $(KERNEL_ELF) $(LIMINE_HOST) boot/limine.conf
	@rm -rf $(ISO_ROOT)
	@mkdir -p $(ISO_ROOT)/boot $(ISO_ROOT)/EFI/BOOT
	cp $(KERNEL_ELF) $(ISO_ROOT)/boot/field-kernel
	cp boot/limine.conf $(ISO_ROOT)/boot/
	cp $(LIMINE_DIR)/limine-bios.sys \
	   $(LIMINE_DIR)/limine-bios-cd.bin \
	   $(LIMINE_DIR)/limine-uefi-cd.bin \
	   $(ISO_ROOT)/boot/
	cp $(LIMINE_DIR)/BOOTX64.EFI $(ISO_ROOT)/EFI/BOOT/
	xorriso -as mkisofs \
	    -b boot/limine-bios-cd.bin \
	    -no-emul-boot -boot-load-size 4 -boot-info-table \
	    --efi-boot boot/limine-uefi-cd.bin \
	    -efi-boot-part --efi-boot-image --protective-msdos-label \
	    $(ISO_ROOT) -o $@
	$(LIMINE_HOST) bios-install $@

iso: $(ISO)

# --- repl-iso ------------------------------------------------------------
# Recursive make with REPL=1 reparses the includes with a separate
# KERNEL_BUILD ($(KERNEL_BUILD)-repl), -DFIELDOS_REPL=1 in CFLAGS, and
# the -repl ISO suffix. ADR-0001 §3 step 6: the flag is off until M3
# exits so smoke stays green during step-6 development.
repl-iso:
	$(MAKE) iso REPL=1

# --- clean ---------------------------------------------------------------
clean: holyc-host-clean holyc-kernel-subset-clean
	rm -rf build kernel/build kernel/build-repl
	rm -f field-os-poc.iso field-os-poc-repl.iso
	-$(MAKE) -C $(LIMINE_DIR) clean 2>/dev/null

distclean: clean
	@echo "removing toolchain install at $(TOOLCHAIN_PREFIX)"
	rm -rf $(TOOLCHAIN_PREFIX)
