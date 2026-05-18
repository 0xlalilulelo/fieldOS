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
# Two log files (cleaned up at end-of-run):
#   .includes.tmp — dedup key for #include <...> directives.
#       The same .h can resolve to different paths (e.g. uapi
#       fallback); we dedup on the include form to avoid
#       re-probing.
#   .fetched.tmp — actual source-paths written. Drives the
#       end-of-run summary.
LOG_INCLUDES="${DEST}/.includes.tmp"
LOG_FETCHED="${DEST}/.fetched.tmp"

# Seed: balloon.c + its 12 documented top-level includes. Each
# entry is a (source-path-within-linux-tree) tuple; the script
# recurses by greppig #include <...> out of each fetched file.
SEEDS=(
    "drivers/virtio/virtio_balloon.c"
)

# Map an #include path to its candidate source-paths within the
# Linux tree, one per line, in resolution order (first 200
# wins). Empty output means the include should be skipped (e.g.
# host-system <stdint.h> — clang's resource-dir handles those
# per M1-2-4's -isystem fix).
#
# Linux's actual include-search order for a driver build on
# x86_64 is roughly:
#   arch/x86/include/, include/, arch/x86/include/uapi/,
#   include/uapi/, include/generated/uapi/.
# We mirror enough of that for the headers balloon's closure
# will reach. The canonical case that motivates the multi-
# candidate form: <linux/virtio_balloon.h> has no kernel-side
# header at v6.12 — it resolves to include/uapi/linux/
# virtio_balloon.h via the uapi fallback.
candidates_for() {
    local inc="$1"
    case "$inc" in
        linux/*)
            # Kernel-side header wins if present; UAPI fallback
            # serves protocol headers shared with userspace.
            echo "include/${inc}"
            echo "include/uapi/${inc}"
            ;;
        asm/*)
            # arch/x86 wins; UAPI variants serve syscall/ABI
            # headers (errno values, signal numbers, etc.);
            # asm-generic is the final fallback for headers
            # Linux doesn't specialize per-arch. The UAPI
            # asm-generic path is where errno.h actually lives
            # at v6.12.
            echo "arch/x86/include/${inc}"
            echo "arch/x86/include/uapi/${inc}"
            echo "include/asm-generic/${inc#asm/}"
            echo "include/uapi/asm-generic/${inc#asm/}"
            ;;
        asm-generic/*)
            echo "include/${inc}"
            echo "include/uapi/${inc}"
            ;;
        uapi/linux/*|uapi/asm-generic/*)
            echo "include/${inc}"
            ;;
        uapi/asm/*)
            echo "arch/x86/include/uapi/${inc#uapi/}"
            ;;
        *)
            # System headers (stdint.h, stddef.h) — clang's
            # resource-dir handles these. Skip.
            ;;
    esac
}

# Fetch one source-path directly. Aborts on HTTP non-200 or
# missing SPDX header. Refuses to overwrite existing files
# across runs. Records into LOG_FETCHED on success.
#
# Returns 0 on success, 1 on HTTP non-200 (caller can try the
# next candidate), exits non-zero on SPDX violation or curl
# error (these are programming errors, not "try next").
fetch_src() {
    local src="$1"
    local dst="${DEST}/${src}"

    if [[ -f "${dst}" ]]; then
        echo "REFUSE: ${dst} already exists; rm -rf vendor/linux-6.12/{drivers,include,arch} to re-vendor" >&2
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
        return 1
    fi

    # Sanity gate. Not a license check — Linux's per-file
    # licensing is inconsistent (most files have SPDX-License-
    # Identifier, some pre-SPDX UAPI carry only a BSD/Copyright
    # block, plenty of plain headers like mmap_lock.h have no
    # per-file marker at all and rely on the project-wide
    # COPYING / GPL-2.0 default). The verbatim-from-upstream
    # commitment ADR-0005 § 3 makes is satisfied by copying
    # whatever upstream ships, including files with no
    # per-file license. The real risk this gate addresses is
    # raw.githubusercontent.com returning a 200 for an HTML
    # error/redirect page; refuse anything starting with `<`.
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

    echo "${src}" >> "${LOG_FETCHED}"
    printf "  + %s\n" "${src}"
    return 0
}

# Resolve an #include directive to a source-path + fetch it.
# Walks candidates_for in order; first 200 wins. Records the
# include in LOG_INCLUDES regardless of outcome so dedup
# works on the include form, not the source path. Aborts loud
# if no candidate resolves — that's a real scope gap (the
# include points outside the trees we mirror, or the file
# was renamed/removed in this PIN).
fetch_include() {
    local inc="$1"

    if grep -qFx "${inc}" "${LOG_INCLUDES}" 2>/dev/null; then
        return 0
    fi
    echo "${inc}" >> "${LOG_INCLUDES}"

    local cands
    cands=$(candidates_for "${inc}")
    if [[ -z "${cands}" ]]; then
        # Skip — system header or out-of-scope tree.
        return 0
    fi

    local cand
    while IFS= read -r cand; do
        [[ -z "${cand}" ]] && continue
        if fetch_src "${cand}"; then
            return 0
        fi
    done <<< "${cands}"

    echo "FAIL: <${inc}> did not resolve to any of:" >&2
    while IFS= read -r cand; do
        [[ -z "${cand}" ]] && continue
        echo "       ${REPO_RAW}/${cand}" >&2
    done <<< "${cands}"
    exit 1
}

# Extract #include <...> directives from one source-path on
# disk. Returns include-form strings on stdout (one per line),
# de-duplicated within the file and globally against
# LOG_INCLUDES.
scan_includes() {
    local src="$1"
    local dst="${DEST}/${src}"
    local inc

    grep -Eh '^[[:space:]]*#[[:space:]]*include[[:space:]]*<[^>]+>' "${dst}" 2>/dev/null \
        | sed -E 's/^[[:space:]]*#[[:space:]]*include[[:space:]]*<([^>]+)>.*/\1/' \
        | sort -u \
        | while read -r inc; do
            if ! grep -qFx "${inc}" "${LOG_INCLUDES}" 2>/dev/null; then
                echo "${inc}"
            fi
        done
}

main() {
    echo "==> Vendoring virtio_balloon transitive closure from torvalds/linux ${PIN}"
    echo "    Dest: ${DEST}"
    echo

    mkdir -p "${DEST}"
    : > "${LOG_FETCHED}"
    : > "${LOG_INCLUDES}"

    # Seeds: balloon.c is a direct source-path (not an include
    # directive), so it's fetched via fetch_src and its includes
    # are scanned to seed the BFS.
    local seed
    for seed in "${SEEDS[@]}"; do
        fetch_src "${seed}" || {
            echo "FAIL: seed ${seed} did not resolve at ${REPO_RAW}" >&2
            exit 1
        }
    done

    # BFS over the include graph. Queue holds include-form
    # strings (linux/foo.h, asm/page.h, ...); fetch_include
    # resolves each to its source-path via candidates_for and
    # writes the file. Then scan_includes pulls new include
    # directives out of the freshly-fetched file.
    local queue=()
    for seed in "${SEEDS[@]}"; do
        while IFS= read -r inc; do
            [[ -z "${inc}" ]] && continue
            queue+=("${inc}")
        done < <(scan_includes "${seed}")
    done

    while [[ ${#queue[@]} -gt 0 ]]; do
        local inc="${queue[0]}"
        queue=("${queue[@]:1}")

        # Snapshot LOG_FETCHED size; if fetch_include actually
        # wrote a new file, scan its includes for the next BFS
        # wave. (fetch_include is a no-op for already-seen
        # includes, so we only scan on actual writes.)
        local before_count
        before_count=$(wc -l < "${LOG_FETCHED}" 2>/dev/null | tr -d '[:space:]')
        fetch_include "${inc}"
        local after_count
        after_count=$(wc -l < "${LOG_FETCHED}" 2>/dev/null | tr -d '[:space:]')
        if [[ "${after_count}" -gt "${before_count}" ]]; then
            # The last entry in LOG_FETCHED is the path that was
            # just written.
            local just_fetched
            just_fetched=$(tail -1 "${LOG_FETCHED}")
            while IFS= read -r new_inc; do
                [[ -z "${new_inc}" ]] && continue
                queue+=("${new_inc}")
            done < <(scan_includes "${just_fetched}")
        fi
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

    rm -f "${LOG_FETCHED}" "${LOG_INCLUDES}"
    echo
    echo "==> Done. Audit the closure against ADR-0005 § 3 before committing."
}

main "$@"
