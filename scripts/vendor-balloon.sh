#!/usr/bin/env bash
# SPDX-License-Identifier: BSD-2-Clause
#
# Vendor virtio_balloon.c + its three BSD-licensed UAPI carve-out
# headers from upstream Linux 6.12 LTS. Per ADR-0006: the Linux
# header surface is provided by linuxkpi/include/ (BSD-2 Arsenal-
# authored reimplementations); verbatim vendoring applies only to
# inherited .c source and a narrow UAPI carve-out for BSD-licensed
# device/protocol ABI headers.
#
# The recursive include-closure variant of this script (279 LOC)
# is preserved in git history at b2dd46f for any future "audit
# what the full closure would be" question.
#
# Usage:  scripts/vendor-balloon.sh [PIN]
#   PIN defaults to v6.12. Re-vendor a different LTS point by
#   rm -rf'ing the relevant files first; this script refuses to
#   overwrite.

set -euo pipefail

PIN="${1:-v6.12}"
REPO_RAW="https://raw.githubusercontent.com/torvalds/linux/${PIN}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${ROOT}/vendor/linux-6.12"

# Named files to vendor. Each entry is a (source-path-within-
# upstream-linux-tree). The four are: balloon's inherited C
# source + three BSD-licensed UAPI protocol headers (feature
# bits, device IDs, endian-tagged virtio types). The triple
# (license, content, transcription-risk) per ADR-0006 § 3 is
# recorded in the vendoring commit body for each carve-out.
FILES=(
    "drivers/virtio/virtio_balloon.c"
    "include/uapi/linux/virtio_balloon.h"
    "include/uapi/linux/virtio_ids.h"
    "include/uapi/linux/virtio_types.h"
)

fetch_one() {
    local src="$1"
    local dst="${DEST}/${src}"

    if [[ -f "${dst}" ]]; then
        echo "REFUSE: ${dst} already exists; rm it to re-vendor" >&2
        exit 1
    fi

    mkdir -p "$(dirname "${dst}")"
    local url="${REPO_RAW}/${src}"
    local http_code
    http_code=$(/usr/bin/curl -sS -o "${dst}" -w "%{http_code}" --max-time 30 "${url}") || {
        echo "FAIL: curl error fetching ${url}" >&2
        rm -f "${dst}"
        exit 1
    }
    if [[ "${http_code}" != "200" ]]; then
        rm -f "${dst}"
        echo "FAIL: HTTP ${http_code} for ${url}" >&2
        exit 1
    fi

    # Guard against raw.githubusercontent.com returning a 200
    # for an HTML error/redirect page.
    local first_byte
    first_byte=$(head -c 1 "${dst}")
    if [[ "${first_byte}" == "<" ]]; then
        echo "FAIL: ${dst} looks like HTML, not a Linux source file" >&2
        exit 1
    fi
    if [[ ! -s "${dst}" ]]; then
        echo "FAIL: ${dst} is empty" >&2
        exit 1
    fi

    printf "  + %s\n" "${src}"
}

main() {
    echo "==> Vendoring virtio_balloon from torvalds/linux ${PIN}"
    echo "    Dest: ${DEST}"
    echo

    mkdir -p "${DEST}"

    local f
    for f in "${FILES[@]}"; do
        fetch_one "${f}"
    done

    # Record the pin's commit SHA so audits know exactly what was
    # vendored.
    local pin_sha
    pin_sha=$(curl -sS --max-time 20 "https://api.github.com/repos/torvalds/linux/commits/${PIN}" \
              | grep -E '^[[:space:]]*"sha":' | head -1 \
              | sed -E 's/.*"([0-9a-f]+)".*/\1/')
    echo
    echo "    pin: ${PIN}  sha: ${pin_sha:-<unresolved>}"
    echo
    echo "==> Done. Verify SPDX/copyright headers in the four files"
    echo "    before committing; each UAPI header must be BSD/dual-"
    echo "    licensed (NOT GPL-2.0-only) per ADR-0006 § 3."
}

main "$@"
