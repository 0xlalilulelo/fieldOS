# Field OS top-level Makefile.
#
# The build chain is plain GNU Make for Phase 0. Per-component .mk
# files (e.g. kernel/kernel.mk in M0 step 3) are include'd from here
# as the project grows. Cross-compiler paths come from
# tools/toolchain.mk; refer to $(CROSS_CC) and friends, never bare
# `gcc`.

include tools/toolchain.mk

.DEFAULT_GOAL := help
.PHONY: help toolchain-check iso clean distclean

# --- help ----------------------------------------------------------------
help:
	@echo "Field OS build targets"
	@echo ""
	@echo "  toolchain-check   Verify the x86_64-elf cross-compiler works."
	@echo "  iso               Build field-os-poc.iso  (wired in M0 step 3)"
	@echo "  clean             Remove build artifacts."
	@echo "  distclean         Remove build artifacts and toolchain install."
	@echo "  help              This message."
	@echo ""
	@echo "Toolchain prefix: $(TOOLCHAIN_PREFIX)"
	@echo "Cross-CC:         $(CROSS_CC)"

# --- toolchain-check -----------------------------------------------------
# Confirm the cross-compiler is installed and can produce an
# x86_64-elf object file. Run this after tools/build-toolchain.sh and
# whenever the toolchain pin changes.
toolchain-check:
	@[ -x "$(CROSS_CC)" ] || { echo "missing $(CROSS_CC) — run tools/build-toolchain.sh"; exit 1; }
	@[ -x "$(CROSS_LD) " ] 2>/dev/null; [ -x "$(CROSS_LD)" ] || { echo "missing $(CROSS_LD) — run tools/build-toolchain.sh"; exit 1; }
	@echo "==> $$($(CROSS_CC) --version | head -1)"
	@echo "==> $$($(CROSS_LD) --version | head -1)"
	@tmp=$$(mktemp -d) && \
	  printf 'int field_os_toolchain_smoke = 42;\n' > $$tmp/t.c && \
	  $(CROSS_CC) -ffreestanding -nostdlib -c $$tmp/t.c -o $$tmp/t.o && \
	  echo "==> compile OK ($$(file $$tmp/t.o | sed 's/^[^:]*: //'))" && \
	  rm -rf $$tmp
	@echo "==> toolchain OK"

# --- iso (placeholder until M0 step 3) -----------------------------------
iso:
	@echo "iso: not yet wired — M0 step 3 brings up the kernel + Limine boot path."
	@exit 1

# --- clean ---------------------------------------------------------------
clean:
	rm -rf build kernel/build
	rm -f field-os-poc.iso

# --- distclean -----------------------------------------------------------
# Removes everything clean does, plus the cross-toolchain install.
# Use sparingly — rebuilding the toolchain takes ~25 min.
distclean: clean
	@echo "removing toolchain install at $(TOOLCHAIN_PREFIX)"
	rm -rf $(TOOLCHAIN_PREFIX)
