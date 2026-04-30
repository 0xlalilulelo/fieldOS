# tools/toolchain.mk
#
# Pinned versions and SHA-256 hashes for the x86_64-elf cross-compiler
# toolchain. Consumed by tools/build-toolchain.sh, tools/fetch-toolchain.sh,
# and the top-level Makefile (which exposes $(CROSS_CC), $(CROSS_LD), etc).
#
# To bump a version:
#   1. Update the VERSION line below.
#   2. Download the upstream tarball and compute its SHA-256:
#        Linux:  sha256sum  binutils-X.Y.tar.xz
#        macOS:  shasum -a 256 binutils-X.Y.tar.xz
#   3. Verify against upstream's GPG-signed sha256sums file at
#      https://ftp.gnu.org/gnu/binutils/sha256.sum (or per-release
#      .sig) before trusting.
#   4. Update the SHA256 line below.
#   5. Re-run tools/build-toolchain.sh on a clean tree.
#   6. Once stable, publish a prebuilt artifact to GitHub Releases and
#      update the pin in tools/fetch-toolchain.sh.

# --- Pinned upstream sources -------------------------------------------------

BINUTILS_VERSION = 2.42
BINUTILS_SHA256  = f6e4d41fd5fc778b06b7891457b3620da5ecea1006c6a4a41ae998109f85a800

GCC_VERSION = 14.2.0
GCC_SHA256  = a7b39bc69cbf9e25826c5a60ab26477001f7c08d85cec04bc0e29cabed6f3cc9

# --- Install layout ----------------------------------------------------------

TOOLCHAIN_TARGET  = x86_64-elf
TOOLCHAIN_PREFIX ?= $(HOME)/.local/$(TOOLCHAIN_TARGET)
TOOLCHAIN_BIN     = $(TOOLCHAIN_PREFIX)/bin

# --- Tool aliases ------------------------------------------------------------
# The rest of the build refers to these, never to bare `gcc`.

CROSS_CC      = $(TOOLCHAIN_BIN)/$(TOOLCHAIN_TARGET)-gcc
CROSS_LD      = $(TOOLCHAIN_BIN)/$(TOOLCHAIN_TARGET)-ld
CROSS_AS      = $(TOOLCHAIN_BIN)/$(TOOLCHAIN_TARGET)-as
CROSS_AR      = $(TOOLCHAIN_BIN)/$(TOOLCHAIN_TARGET)-ar
CROSS_OBJCOPY = $(TOOLCHAIN_BIN)/$(TOOLCHAIN_TARGET)-objcopy
CROSS_OBJDUMP = $(TOOLCHAIN_BIN)/$(TOOLCHAIN_TARGET)-objdump
