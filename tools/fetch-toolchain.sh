#!/usr/bin/env bash
# tools/fetch-toolchain.sh
#
# CI-side: download a prebuilt x86_64-elf cross-compiler tarball
# published as a GitHub Release artifact. Faster than rebuilding from
# source on every CI run (which takes ~20 min and burns billable
# minutes for nothing — the toolchain bumps roughly once a year).
#
# Local development should use tools/build-toolchain.sh instead. This
# script is a thin downloader; it does not build anything.
#
# Publishing a new toolchain artifact (manual, after each toolchain
# version bump):
#
#   tools/build-toolchain.sh
#   cd "$HOME/.local"
#   tar -cJf "x86_64-elf-toolchain-$(uname -s)-$(uname -m)-$(git -C ~/path/to/fieldOS rev-parse --short HEAD).tar.xz" x86_64-elf
#   shasum -a 256 x86_64-elf-toolchain-*.tar.xz
#   gh release create toolchain-vYYYY.MM ./x86_64-elf-toolchain-*.tar.xz \
#       --title "Toolchain vYYYY.MM" \
#       --notes "binutils X.Y, gcc A.B.C"
#
# Then update RELEASE_TAG, TARBALL_NAME, and TARBALL_SHA256 below.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
mk="$script_dir/toolchain.mk"

read_var() {
  grep -E "^$1[[:space:]]*[?:]?=" "$mk" | head -1 \
    | sed -E 's/^[^=]+=[[:space:]]*//' | tr -d '"'
}
TARGET=$(read_var TOOLCHAIN_TARGET)
PREFIX="${TOOLCHAIN_PREFIX:-$HOME/.local/$TARGET}"

# --- Pin: bump together when republishing the toolchain artifact ----------

RELEASE_TAG=""           # e.g. toolchain-v2026.05
TARBALL_NAME=""          # e.g. x86_64-elf-toolchain-Linux-x86_64-abc1234.tar.xz
TARBALL_SHA256=""        # 64 hex chars

# --- No release published yet: surface the chicken-and-egg state ----------

if [[ -z "$RELEASE_TAG" ]]; then
  cat >&2 <<'EOF'
fetch-toolchain.sh: no prebuilt toolchain release pinned yet.

Local dev:
  tools/build-toolchain.sh

CI bootstrap path:
  Run tools/build-toolchain.sh manually once on a clean Ubuntu 24.04
  host, package $HOME/.local/x86_64-elf, publish it as a GitHub
  Release, then pin RELEASE_TAG / TARBALL_NAME / TARBALL_SHA256 in
  this script. Until then, CI's build-iso job should call
  tools/build-toolchain.sh directly (with `actions/cache` keyed on
  toolchain.mk's hash to avoid rebuilds).
EOF
  exit 1
fi

# --- Skip if already installed --------------------------------------------

if [[ -x "$PREFIX/bin/$TARGET-gcc" ]]; then
  echo "fetch-toolchain.sh: $PREFIX already populated; skipping download."
  exit 0
fi

repo="${GITHUB_REPOSITORY:-0xlalilulelo/fieldOS}"
url="https://github.com/$repo/releases/download/$RELEASE_TAG/$TARBALL_NAME"

mkdir -p "$(dirname "$PREFIX")"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "==> fetching $url"
curl -fL --retry 3 -o "$tmp/$TARBALL_NAME" "$url"

if command -v sha256sum >/dev/null 2>&1; then
  have=$(sha256sum "$tmp/$TARBALL_NAME" | awk '{print $1}')
else
  have=$(shasum -a 256 "$tmp/$TARBALL_NAME" | awk '{print $1}')
fi

if [[ "$have" != "$TARBALL_SHA256" ]]; then
  echo "fetch-toolchain.sh: SHA-256 mismatch" >&2
  echo "  want: $TARBALL_SHA256" >&2
  echo "  have: $have" >&2
  exit 1
fi

echo "==> extracting to $(dirname "$PREFIX")"
tar -xJf "$tmp/$TARBALL_NAME" -C "$(dirname "$PREFIX")"

"$PREFIX/bin/$TARGET-gcc" --version | head -1
echo "==> toolchain ready at $PREFIX"
