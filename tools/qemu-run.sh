#!/usr/bin/env bash
# tools/qemu-run.sh
#
# Boot a Field OS ISO under QEMU with the best available accelerator:
#   macOS  -> -accel hvf  -cpu max
#   Linux  -> -enable-kvm -cpu host  (if /dev/kvm is writable)
#   else   -> TCG fallback (-cpu max)
#
# Usage:
#   tools/qemu-run.sh                    # boots ./field-os-poc.iso
#   tools/qemu-run.sh path/to/foo.iso    # boots a specific ISO
#   tools/qemu-run.sh -h | --help        # this message
#
# Serial output is wired to stdio so the boot sentinel
# (FIELD_OS_BOOT_OK) is visible in the launching terminal. CI uses
# ci/qemu-smoke.sh, which is a headless variant of this script.

set -euo pipefail

case "${1:-}" in
  -h|--help)
    cat <<'EOF'
tools/qemu-run.sh — boot a Field OS ISO under QEMU.

Usage:
  tools/qemu-run.sh                    # boots ./field-os-poc.iso
  tools/qemu-run.sh path/to/foo.iso    # boots a specific ISO
  tools/qemu-run.sh -h | --help        # this message

Auto-selects the best available accelerator:
  macOS  -> -accel hvf  -cpu max
  Linux  -> -enable-kvm -cpu host  (if /dev/kvm is writable)
  else   -> TCG fallback (-cpu max)

Serial is wired to stdio so the boot sentinel (FIELD_OS_BOOT_OK)
is visible in the launching terminal. CI uses ci/qemu-smoke.sh,
which is a headless variant of this script.
EOF
    exit 0
    ;;
esac

ISO="${1:-field-os-poc.iso}"

[[ -f "$ISO" ]] || {
  echo "qemu-run.sh: ISO not found: $ISO" >&2
  echo "build it first with: make iso" >&2
  exit 1
}

command -v qemu-system-x86_64 >/dev/null 2>&1 || {
  echo "qemu-run.sh: qemu-system-x86_64 not in PATH" >&2
  echo "  macOS:  brew install qemu" >&2
  echo "  Linux:  apt install qemu-system-x86  # or: dnf install qemu-system-x86" >&2
  exit 1
}

ACCEL=()
case "$(uname -s)" in
  Darwin) ACCEL=(-accel hvf -cpu max) ;;
  Linux)
    if [[ -w /dev/kvm ]]; then
      ACCEL=(-enable-kvm -cpu host)
    else
      ACCEL=(-cpu max)
    fi
    ;;
  *) ACCEL=(-cpu max) ;;
esac

exec qemu-system-x86_64 \
  -cdrom "$ISO" \
  -m 1G -smp 2 \
  -machine q35 \
  "${ACCEL[@]}" \
  -serial stdio \
  -device VGA,vgamem_mb=32
