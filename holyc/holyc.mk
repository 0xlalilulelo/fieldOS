# holyc/holyc.mk
#
# Host build rules for the vendored holyc-lang compiler. Supersedes the
# upstream CMake build (holyc/Makefile + holyc/src/CMakeLists.txt) for
# Field OS purposes. Those upstream files are left in place to keep the
# vendored tree diff-friendly when the pin is bumped (see holyc/VERSION);
# the Field OS top-level Makefile no longer invokes them.
#
# Why a host build, not the cross-GCC: the binary produced here runs on
# the developer machine and is used for `--transpile` (HolyC -> C source)
# and as a future AOT path that emits an x86_64 ELF object linked into
# the kernel image. Cross-GCC targets x86_64-elf and would emit a
# kernel-shaped binary the host cannot execute. The kernel-resident
# strip of holyc-lang (audit step 3, M3-B) is a separate compile against
# $(CROSS_CC) -ffreestanding; this rule set is strictly the host-side
# tool. CLAUDE.md forbids bare `cc` in the kernel build; this file is
# the documented exception, scoped to user-space tooling.

HOLYC_DIR     := holyc
HOLYC_SRC     := $(HOLYC_DIR)/src
HOLYC_BUILD   := build/holyc-host
HOLYC_HCC     := $(HOLYC_DIR)/hcc

# Staged install prefix. The hcc binary resolves its prelude at runtime
# as `$(INSTALL_PREFIX)/include/tos.HH` (compile.c:72, transpiler.c:1665);
# upstream relies on `make install` to populate /usr/local. Field OS
# avoids the system install — stage holyc/src/holyc-lib/ here instead
# and bake the absolute path into the binary via -DINSTALL_PREFIX.
HOLYC_PREFIX  := $(abspath $(HOLYC_BUILD)/prefix)
HOLYC_INCLUDE := $(HOLYC_PREFIX)/include
HOLYC_HOLYLIB := $(HOLYC_SRC)/holyc-lib
HOLYC_PRELUDE := $(HOLYC_INCLUDE)/tos.HH

HOLYC_CC      ?= cc

# Source list mirrors holyc/src/CMakeLists.txt SOURCES. Update both if
# the pin is bumped and the upstream file set shifts.
HOLYC_C_SRCS := \
    $(HOLYC_SRC)/aostr.c \
    $(HOLYC_SRC)/arena.c \
    $(HOLYC_SRC)/ast.c \
    $(HOLYC_SRC)/cctrl.c \
    $(HOLYC_SRC)/cfg-print.c \
    $(HOLYC_SRC)/cfg.c \
    $(HOLYC_SRC)/cli.c \
    $(HOLYC_SRC)/compile.c \
    $(HOLYC_SRC)/containers.c \
    $(HOLYC_SRC)/lexer.c \
    $(HOLYC_SRC)/list.c \
    $(HOLYC_SRC)/main.c \
    $(HOLYC_SRC)/memory.c \
    $(HOLYC_SRC)/mempool.c \
    $(HOLYC_SRC)/parser.c \
    $(HOLYC_SRC)/prsasm.c \
    $(HOLYC_SRC)/prslib.c \
    $(HOLYC_SRC)/prsutil.c \
    $(HOLYC_SRC)/transpiler.c \
    $(HOLYC_SRC)/x86.c

HOLYC_OBJS := $(patsubst $(HOLYC_SRC)/%.c,$(HOLYC_BUILD)/%.o,$(HOLYC_C_SRCS))

# Host AT&T-corpus capture tool. Links the host hcc object set MINUS
# main.o (single `main` symbol) against holyc/tools/dump-asm.c. The
# resulting binary takes (input.HC, output.s) and writes the AoStr
# returned by compileToAsm directly. Used by the `corpus` target
# below; lives behind ADR-0003 §2.
HOLYC_OBJS_NOMAIN := $(filter-out $(HOLYC_BUILD)/main.o,$(HOLYC_OBJS))
HOLYC_DUMP_ASM_SRC := $(HOLYC_DIR)/tools/dump-asm.c
HOLYC_DUMP_ASM     := $(HOLYC_BUILD)/dump-asm

# Corpus inputs and the .s files they produce. Inputs grow as later
# step-4 sub-rounds need new instruction-form coverage; per ADR-0003
# §2, each input is a checked-in .HC file the host hcc compiles
# successfully today.
CORPUS_DIR    := $(HOLYC_DIR)/tests/corpus
CORPUS_INPUTS := $(HOLYC_DIR)/bug-tests/Bug_171.HC
CORPUS_OUTS   := $(patsubst $(HOLYC_DIR)/bug-tests/%.HC,$(CORPUS_DIR)/%.s,$(CORPUS_INPUTS))

# HCC_GIT_HASH stamps the binary with a Field OS commit identifier so
# `hcc --version` reports which checkout produced it.
HOLYC_GIT_HASH := $(shell git rev-parse --short HEAD 2>/dev/null || echo unknown)

HOLYC_CFLAGS := -O2 -Wall -Wextra -Wno-implicit-fallthrough \
    -DHCC_GIT_HASH=\"field-os-$(HOLYC_GIT_HASH)\" \
    -DINSTALL_PREFIX=\"$(HOLYC_PREFIX)\"

$(HOLYC_BUILD)/%.o: $(HOLYC_SRC)/%.c
	@mkdir -p $(@D)
	$(HOLYC_CC) $(HOLYC_CFLAGS) -c $< -o $@

$(HOLYC_HCC): $(HOLYC_OBJS)
	$(HOLYC_CC) $(HOLYC_OBJS) -lm -o $@

# Stage the prelude tree into $(HOLYC_PREFIX)/include so the binary's
# runtime path lookup succeeds. tos.HH is the witness file; the rule
# copies the entire holyc-lib/ directory because the transpiler walks
# it for #include resolution against other .HC standard-library files.
$(HOLYC_PRELUDE): $(HOLYC_HOLYLIB)/tos.HH
	@mkdir -p $(HOLYC_INCLUDE)
	cp -R $(HOLYC_HOLYLIB)/. $(HOLYC_INCLUDE)/

.PHONY: holyc-host holyc-host-clean holyc-host-smoke

holyc-host: $(HOLYC_HCC) $(HOLYC_PRELUDE)
	@echo "==> $(HOLYC_HCC) built"
	@$(HOLYC_HCC) --version 2>&1 | head -1 || true

# Smallest end-to-end check: transpile the bundled HolyC reproducer to
# C source and confirm hcc exits 0 with non-empty output. The .HC file
# is checked in for exactly this purpose (audit "What M3 actually has
# to do" / step 2 warm-up).
holyc-host-smoke: $(HOLYC_HCC) $(HOLYC_PRELUDE)
	@out=$$(mktemp -t hcc-smoke.XXXXXX.c) && \
	  $(HOLYC_HCC) -transpile $(HOLYC_DIR)/bug-tests/Bug_171.HC > $$out && \
	  test -s $$out && \
	  echo "==> -transpile OK ($$out, $$(wc -l < $$out | tr -d ' ') lines)" && \
	  rm -f $$out

holyc-host-clean:
	rm -rf $(HOLYC_BUILD) $(HOLYC_HCC)

# --- Corpus capture (ADR-0003 §2) -----------------------------------------
#
# `make corpus` rebuilds the AT&T-text corpus under holyc/tests/corpus/
# from the inputs in CORPUS_INPUTS. The .s files are checked in; the
# rule's job is to keep them honest against the pinned holyc-lang and
# the in-tree x86.c. Diff before bumping holyc/VERSION.

$(HOLYC_DUMP_ASM): $(HOLYC_OBJS_NOMAIN) $(HOLYC_DUMP_ASM_SRC)
	@mkdir -p $(@D)
	$(HOLYC_CC) $(HOLYC_CFLAGS) -I$(HOLYC_SRC) \
	    $(HOLYC_DUMP_ASM_SRC) $(HOLYC_OBJS_NOMAIN) -lm -o $@

$(CORPUS_DIR)/%.s: $(HOLYC_DIR)/bug-tests/%.HC $(HOLYC_DUMP_ASM) $(HOLYC_PRELUDE)
	@mkdir -p $(@D)
	$(HOLYC_DUMP_ASM) $< $@

.PHONY: corpus corpus-clean

corpus: $(CORPUS_OUTS)
	@count=$$(ls $(CORPUS_DIR)/*.s 2>/dev/null | wc -l | tr -d ' ') && \
	  echo "==> $(CORPUS_DIR): $$count file(s)"

corpus-clean:
	rm -rf $(CORPUS_DIR) $(HOLYC_DUMP_ASM)
