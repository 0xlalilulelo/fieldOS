#!/usr/bin/env bash
# tools/count-loc.sh
#
# Report Field OS base-system line count vs the 100,000-line
# budget (CLAUDE.md hard constraint #2 / phase-0.md §M0).
#
# Counts:
#   kernel/**/*.{c,h,HC,HH,S,ld}    kernel + runtime + arch
#   base/**/*.{HC,HH,S}             HolyC userspace base
# Excludes:
#   vendor/**          third-party vendored libs (Limine, etc.)
#   holyc/**           forked compiler (vendored once it lands)
#   kernel/drivers/**  driver shims (LinuxKPI port, not base)
#   **/build/          build artifacts
#   **/*_test.c        host-only test tooling (e.g. kernel/holyc/
#                      asm_test.c — built host-side via holyc/holyc.mk's
#                      `asm-test` target, never linked into the kernel)
#
# CI's loc-budget job invokes this. Exit codes:
#   0  under WARN_PCT (default 90%)
#   0  printed warning, between WARN_PCT and HARD_PCT
#   1  at or over HARD_PCT (default 95%) — CI fails the build
#
# Tunables (env):
#   LOC_BUDGET     total budget (default: 100000)
#   LOC_WARN_PCT   warning threshold percent (default: 90)
#   LOC_HARD_PCT   fail threshold percent (default: 95)

set -euo pipefail

BUDGET=${LOC_BUDGET:-100000}
WARN_PCT=${LOC_WARN_PCT:-90}
HARD_PCT=${LOC_HARD_PCT:-95}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# All files we count, with kernel/drivers and any build/ pruned.
collect() {
	local roots=("$@")
	[[ ${#roots[@]} -eq 0 ]] && return
	find "${roots[@]}" \
		\( -path 'kernel/drivers' -o -path '*/build' \) -prune \
		-o -type f \
		! -name '*_test.c' \
		\( -name '*.c' -o -name '*.h' \
		   -o -name '*.HC' -o -name '*.HH' \
		   -o -name '*.S' -o -name '*.ld' \) \
		-print 2>/dev/null
}

count_lines() {
	local files
	files="$(cat)"
	if [[ -z "$files" ]]; then
		echo 0
		return
	fi
	echo "$files" | tr '\n' '\0' | xargs -0 wc -l 2>/dev/null \
		| awk 'END { print $1+0 }'
}

# --- totals -----------------------------------------------------------------

[[ -d kernel ]] && roots+=(kernel)
[[ -d base   ]] && roots+=(base)
roots=("${roots[@]:-}")

all_files="$(collect "${roots[@]}")"
total=$(echo "$all_files" | count_lines)
file_count=$([[ -z "$all_files" ]] && echo 0 || echo "$all_files" | wc -l | tr -d ' ')

pct=$(( total * 100 / BUDGET ))

printf 'Field OS line-count budget\n'
printf '  used:   %7d / %d\n' "$total" "$BUDGET"
printf '  pct:    %d%%\n' "$pct"
printf '  files:  %d\n' "$file_count"
printf '\n'
printf 'breakdown:\n'
for area in kernel base; do
	[[ -d "$area" ]] || continue
	sub=$(collect "$area" | count_lines)
	printf '  %-10s %5d\n' "$area" "$sub"
done
printf '\n'

if (( pct >= HARD_PCT )); then
	printf 'FAIL: %d%% of budget (>= %d%% hard gate)\n' "$pct" "$HARD_PCT" >&2
	exit 1
fi
if (( pct >= WARN_PCT )); then
	printf 'WARN: %d%% of budget (>= %d%% warning)\n' "$pct" "$WARN_PCT"
fi

exit 0
