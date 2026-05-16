#!/usr/bin/env bash
# SPDX-License-Identifier: BSD-2-Clause
#
# Recursive vendor-fetch for virtio_balloon.c + its transitive
# include closure from upstream Linux 6.12 LTS. Writes into
# vendor/linux-6.12/ verbatim, with SPDX headers preserved per
# ADR-0005 § 3. M1-2-5 Part B sub-task 1.
#
# Usage:  scripts/vendor-balloon.sh [PIN]
#   PIN defaults to v6.12 (the LTS release point). Override with
#   e.g. v6.12.5 once a security/correctness re-pin is needed
#   (the quarterly re-pin checklist in vendor/linux-6.12/README.md).
#
# The script is intentionally fail-loud: any 404, any missing
# SPDX header, any attempt to overwrite existing vendored files
# aborts with a non-zero exit. Re-vendor by `rm -rf` the relevant
# subtree first.
#
# Output policy: every fetch prints one line so the closure size
# is visible as it grows; a summary at the end breaks the total
# down by tree (linux/, asm-generic/, uapi/linux/, uapi/asm-generic/,
# arch/x86/include/asm/, arch/x86/include/uapi/asm/) so the
# scope question for Part B sub-task 2 (does the closure exceed
# the ADR-0005 "minimal subset" wording?) has concrete numbers.

set -euo pipefail

PIN="${1:-v6.12}"
REPO_RAW="https://raw.githubusercontent.com/torvalds/linux/${PIN}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${ROOT}/vendor/linux-6.12"
LOG_FETCHED="${DEST}/.fetched.tmp"

# Seed: balloon.c + its 12 documented top-level includes. Each
# entry is a (source-path-within-linux-tree) tuple; the script
# recurses by greppig #include <...> out of each fetched file.
SEEDS=(
    "drivers/virtio/virtio_balloon.c"
)

# Map an #include path to its source-path within the Linux tree.
# Returns the source-path on stdout, or empty string if the
# include should be skipped (e.g. host-system <stdint.h> — clang's
# resource-dir handles those per M1-2-4's -isystem fix).
map_include() {
    local inc="$1"
    case "$inc" in
        linux/*|asm-generic/*|uapi/linux/*|uapi/asm-generic/*)
            echo "include/${inc}"
            ;;
        asm/*)
            # arch/x86 path — Linux's build system selects this
            # for x86_64. The arsenal-kernel side substitutes for
            # most kernel internals these reference, but the
            # vendor must still ship the headers so the C compile
            # has something to parse. Surface them in the summary.
            echo "arch/x86/${inc}"
            ;;
        *)
            # System headers (stdint.h, stddef.h) — clang's
            # resource-dir handles these. Skip.
            echo ""
            ;;
    esac
}

# Fetch one source-path, write to DEST mirroring the tree.
# Aborts on 404 or missing SPDX header. Idempotent within a
# single run (skips if already fetched); refuses overwrite
# across runs.
fetch_one() {
    local src="$1"
    local dst="${DEST}/${src}"

    if grep -qFx "${src}" "${LOG_FETCHED}" 2>/dev/null; then
        return 0
    fi
    echo "${src}" >> "${LOG_FETCHED}"

    if [[ -f "${dst}" ]]; then
        echo "REFUSE: ${dst} already exists; rm -rf vendor/linux-6.12/{drivers,include,arch} to re-vendor" >&2
        exit 1
    fi

    mkdir -p "$(dirname "${dst}")"
    local url="${REPO_RAW}/${src}"
    local http_code
    http_code=$(curl -sS -o "${dst}" -w "%{http_code}" --max-time 30 "${url}") || {
        echo "FAIL: curl error fetching ${url}" >&2
        rm -f "${dst}"
        exit 1
    }
    if [[ "${http_code}" != "200" ]]; then
        echo "FAIL: ${url} returned HTTP ${http_code}" >&2
        rm -f "${dst}"
        exit 1
    fi

    # SPDX gate. Every upstream Linux file ships SPDX-License-
    # Identifier in the first ~5 lines; bail loud if absent.
    if ! head -5 "${dst}" | grep -q "SPDX-License-Identifier"; then
        echo "FAIL: ${dst} missing SPDX-License-Identifier in first 5 lines" >&2
        exit 1
    fi

    printf "  + %s\n" "${src}"
}

# Pop includes out of one source file and queue any new ones.
# Returns the queue on stdout (one per line), de-duplicated
# against LOG_FETCHED.
queue_includes() {
    local src="$1"
    local dst="${DEST}/${src}"
    local inc src_inc

    # Linux's #include syntax we care about: #include <path>.
    # Quoted #include "path" forms are rare in the headers we
    # vendor; ignore for now and surface in the summary if any
    # show up.
    grep -Eh '^[[:space:]]*#[[:space:]]*include[[:space:]]*<[^>]+>' "${dst}" 2>/dev/null \
        | sed -E 's/^[[:space:]]*#[[:space:]]*include[[:space:]]*<([^>]+)>.*/\1/' \
        | sort -u \
        | while read -r inc; do
            src_inc="$(map_include "${inc}")"
            if [[ -n "${src_inc}" ]] && ! grep -qFx "${src_inc}" "${LOG_FETCHED}" 2>/dev/null; then
                echo "${src_inc}"
            fi
        done
}

main() {
    echo "==> Vendoring virtio_balloon transitive closure from torvalds/linux ${PIN}"
    echo "    Dest: ${DEST}"
    echo

    mkdir -p "${DEST}"
    : > "${LOG_FETCHED}"

    # BFS over the include graph. Queue holds source-paths to
    # fetch; we drain it one at a time, fetching + scanning each
    # file for further includes.
    local queue=("${SEEDS[@]}")
    while [[ ${#queue[@]} -gt 0 ]]; do
        local src="${queue[0]}"
        queue=("${queue[@]:1}")

        fetch_one "${src}"

        # Scan + enqueue any new includes.
        local new_incs
        new_incs=$(queue_includes "${src}")
        while IFS= read -r inc; do
            [[ -z "${inc}" ]] && continue
            queue+=("${inc}")
        done <<< "${new_incs}"
    done

    echo
    echo "==> Closure summary"
    local total
    total=$(wc -l < "${LOG_FETCHED}" | tr -d '[:space:]')
    echo "    total files fetched: ${total}"
    for tree in "drivers/virtio" "include/linux" "include/asm-generic" \
                "include/uapi/linux" "include/uapi/asm-generic" \
                "arch/x86/include/asm" "arch/x86/include/uapi/asm"; do
        local n
        n=$(grep -c "^${tree}/" "${LOG_FETCHED}" || true)
        printf "      %-32s %4d\n" "${tree}" "${n}"
    done

    # Record the pin in the README so audits know what SHA the
    # tree corresponds to. Resolve PIN to its commit SHA via the
    # GitHub API so the record is unambiguous.
    local pin_sha
    pin_sha=$(curl -sS --max-time 20 "https://api.github.com/repos/torvalds/linux/commits/${PIN}" \
              | grep -E '^[[:space:]]*"sha":' | head -1 \
              | sed -E 's/.*"([0-9a-f]+)".*/\1/')
    echo
    echo "    pin: ${PIN}  sha: ${pin_sha:-<unresolved>}"

    rm -f "${LOG_FETCHED}"
    echo
    echo "==> Done. Audit the closure against ADR-0005 § 3 before committing."
}

main "$@"
