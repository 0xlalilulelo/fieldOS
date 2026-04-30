#!/usr/bin/env bash
# tools/build-toolchain.sh
#
# Build the x86_64-elf cross-compiler (binutils + GCC) for Field OS.
#
# Versions and SHA-256 hashes are pinned in tools/toolchain.mk. Update
# pins there, not here. The toolchain installs into $TOOLCHAIN_PREFIX
# (default $HOME/.local/x86_64-elf); the Field OS Makefile reads the
# same path via toolchain.mk, so no PATH change is required for the
# kernel build itself.
#
# This script is idempotent. A re-run after a partial failure picks
# up where the last run left off — completed binutils install, cached
# tarball with verified hash, etc. Force a from-scratch rebuild by
# `rm -rf $TOOLCHAIN_PREFIX`.
#
# Hosts: Linux (Debian/Ubuntu/Fedora) and macOS via Homebrew. Other
# hosts will fail loudly at the prereq probe.

set -euo pipefail

# --- Locate paths -----------------------------------------------------------

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
mk="$script_dir/toolchain.mk"
[[ -f "$mk" ]] || { echo "missing $mk" >&2; exit 1; }

# --- Read pins from toolchain.mk -------------------------------------------

read_var() {
  grep -E "^$1[[:space:]]*[?:]?=" "$mk" | head -1 \
    | sed -E 's/^[^=]+=[[:space:]]*//' | tr -d '"'
}

BINUTILS_VERSION=$(read_var BINUTILS_VERSION)
BINUTILS_SHA256=$(read_var BINUTILS_SHA256)
GCC_VERSION=$(read_var GCC_VERSION)
GCC_SHA256=$(read_var GCC_SHA256)
TARGET=$(read_var TOOLCHAIN_TARGET)
PREFIX="${TOOLCHAIN_PREFIX:-$HOME/.local/$TARGET}"
SRC="$PREFIX/src"
BUILD="$PREFIX/build"

[[ -n "$BINUTILS_VERSION" && -n "$BINUTILS_SHA256" ]] || { echo "binutils pin missing in $mk" >&2; exit 1; }
[[ -n "$GCC_VERSION"      && -n "$GCC_SHA256"      ]] || { echo "gcc pin missing in $mk"      >&2; exit 1; }

# --- Host detection ---------------------------------------------------------

case "$(uname -s)" in
  Linux)  jobs=$(getconf _NPROCESSORS_ONLN) ;;
  Darwin) jobs=$(sysctl -n hw.ncpu) ;;
  *) echo "unsupported host: $(uname -s)" >&2; exit 1 ;;
esac

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

say()  { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!!\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m**\033[0m %s\n' "$*" >&2; exit 1; }

say "Field OS x86_64-elf cross-compiler"
say "  binutils $BINUTILS_VERSION"
say "  gcc      $GCC_VERSION"
say "  prefix   $PREFIX"
say "  jobs     $jobs"

# --- Host prereq probe ------------------------------------------------------

need() { command -v "$1" >/dev/null 2>&1 || die "missing host tool: $1 ($2)"; }
need curl     "install curl"
need tar      "install tar with .xz support"
need make     "install make"
need bison    "install bison"
need flex     "install flex"
need m4       "install m4"
need texi2any "install texinfo"

GCC_HOST_FLAGS=()
case "$(uname -s)" in
  Darwin)
    command -v brew >/dev/null 2>&1 \
      || die "macOS host requires Homebrew (https://brew.sh)"
    # `brew --prefix <pkg>` returns the prospective install path for
    # any known formula whether or not it is installed; use
    # `brew ls --versions` to get a real installation signal.
    for pkg in gmp mpfr libmpc; do
      brew ls --versions "$pkg" >/dev/null 2>&1 \
        || die "missing Homebrew package: $pkg  (run: brew install $pkg)"
    done
    GCC_HOST_FLAGS=(
      "--with-gmp=$(brew --prefix gmp)"
      "--with-mpfr=$(brew --prefix mpfr)"
      "--with-mpc=$(brew --prefix libmpc)"
    )
    ;;
  Linux)
    # GCC's bundled `download_prerequisites` script handles gmp/mpfr/mpc on
    # Linux; if the host already has libgmp-dev/libmpfr-dev/libmpc-dev,
    # configure will use those instead. Either path works.
    :
    ;;
esac

mkdir -p "$SRC" "$BUILD" "$PREFIX/bin"

# --- Fetch and verify -------------------------------------------------------

fetch_verify() {
  local url="$1" file="$2" want="$3"
  if [[ -f "$SRC/$file" ]]; then
    local have; have=$(sha256 "$SRC/$file")
    if [[ "$have" == "$want" ]]; then
      say "$file: cached, hash OK"
      return
    fi
    warn "$file: cached but hash mismatch; redownloading"
    rm -f "$SRC/$file"
  fi
  say "downloading $url"
  curl -fL --retry 3 -o "$SRC/$file" "$url"
  local have; have=$(sha256 "$SRC/$file")
  [[ "$have" == "$want" ]] || die "SHA-256 mismatch for $file
   want: $want
   have: $have
update tools/toolchain.mk if upstream is correct, or investigate tampering."
  say "$file: downloaded, hash OK"
}

binutils_tar="binutils-${BINUTILS_VERSION}.tar.xz"
gcc_tar="gcc-${GCC_VERSION}.tar.xz"

fetch_verify "https://ftp.gnu.org/gnu/binutils/$binutils_tar" \
             "$binutils_tar" "$BINUTILS_SHA256"
fetch_verify "https://ftp.gnu.org/gnu/gcc/gcc-${GCC_VERSION}/$gcc_tar" \
             "$gcc_tar" "$GCC_SHA256"

# --- Extract ----------------------------------------------------------------

[[ -d "$SRC/binutils-${BINUTILS_VERSION}" ]] \
  || tar -xf "$SRC/$binutils_tar" -C "$SRC"
[[ -d "$SRC/gcc-${GCC_VERSION}" ]] \
  || tar -xf "$SRC/$gcc_tar" -C "$SRC"

# --- Build binutils ---------------------------------------------------------

if [[ -x "$PREFIX/bin/${TARGET}-ld" ]]; then
  say "binutils: already installed"
else
  say "building binutils-${BINUTILS_VERSION}"
  rm -rf "$BUILD/binutils"
  mkdir -p "$BUILD/binutils"
  (
    cd "$BUILD/binutils"
    # --with-system-zlib avoids binutils's bundled zlib, whose K&R
    # function declarations fail to parse against the modern macOS
    # SDK <stdio.h>. Benign on Linux (system zlib is universal).
    "$SRC/binutils-${BINUTILS_VERSION}/configure" \
      --target="$TARGET" \
      --prefix="$PREFIX" \
      --with-sysroot \
      --with-system-zlib \
      --disable-nls \
      --disable-werror
    make -j"$jobs"
    make install
  )
  say "binutils: installed"
fi

# --- Build GCC --------------------------------------------------------------

if [[ -x "$PREFIX/bin/${TARGET}-gcc" ]]; then
  say "gcc: already installed"
else
  say "building gcc-${GCC_VERSION}  (~20 min on a modern laptop)"
  rm -rf "$BUILD/gcc"
  mkdir -p "$BUILD/gcc"
  export PATH="$PREFIX/bin:$PATH"
  (
    cd "$BUILD/gcc"
    "$SRC/gcc-${GCC_VERSION}/configure" \
      --target="$TARGET" \
      --prefix="$PREFIX" \
      --disable-nls \
      --enable-languages=c \
      --without-headers \
      --with-system-zlib \
      "${GCC_HOST_FLAGS[@]}"
    make -j"$jobs" all-gcc
    make -j"$jobs" all-target-libgcc
    make install-gcc
    make install-target-libgcc
  )
  say "gcc: installed"
fi

# --- Verify -----------------------------------------------------------------

say "verifying"
"$PREFIX/bin/${TARGET}-gcc" --version | head -1
"$PREFIX/bin/${TARGET}-ld"  --version | head -1

cat <<EOF

==> x86_64-elf cross-compiler ready at $PREFIX

The Field OS Makefile reads $TARGET-gcc through tools/toolchain.mk; no
PATH change is required for kernel builds. To invoke directly from a
shell:

  bash/zsh:  export PATH="$PREFIX/bin:\$PATH"
  fish:      set -gx PATH $PREFIX/bin \$PATH

EOF
