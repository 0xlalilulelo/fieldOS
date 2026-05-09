#!/usr/bin/env bash
# ci/qemu-smoke.sh
#
# Boot an Arsenal ISO under headless QEMU and grep the serial log
# for the boot sentinel (ARSENAL_BOOT_OK). Used by the CI smoke
# job; safe to run locally as a sanity check.
#
# Always uses TCG (no acceleration) so the same script works in
# GitHub Actions hosted runners (which lack nested virt) and on
# developer laptops regardless of host arch.
#
# Usage:
#   ci/qemu-smoke.sh                    # boots ./arsenal.iso
#   ci/qemu-smoke.sh path/to/foo.iso
#
# Tunables (env):
#   SMOKE_TIMEOUT  seconds to wait for sentinel (default: 15)
#
# Exit codes:
#   0  ok         sentinel found within timeout
#   1  missing    ISO or qemu-system-x86_64 not present
#   2  timeout    QEMU did not print sentinel within $SMOKE_TIMEOUT
#   3  startup    QEMU exited unexpectedly before printing the sentinel
#   4  guest_err  QEMU reported guest CPU faults

set -euo pipefail

ISO="${1:-arsenal.iso}"
TIMEOUT="${SMOKE_TIMEOUT:-15}"
# Final sentinel triggers the pass check; required sentinels must all
# appear (in any order) for the smoke to pass. Add a sentinel here
# when a milestone wants its "this subsystem survived" assertion in
# CI; remove one only when the underlying assertion is folded into a
# stronger downstream sentinel.
FINAL_SENTINEL="ARSENAL_HEAP_OK"
REQUIRED_SENTINELS=("ARSENAL_BOOT_OK" "ARSENAL_HEAP_OK")
SERIAL_LOG=$(mktemp -t arsenal-smoke-serial.XXXXXX)
QEMU_LOG=$(mktemp -t arsenal-smoke-qemu.XXXXXX)
trap 'rm -f "$SERIAL_LOG" "$QEMU_LOG"' EXIT

[[ -f "$ISO" ]] || {
	echo "qemu-smoke.sh: ISO not found: $ISO" >&2
	echo "build it first with: cargo xtask iso" >&2
	exit 1
}

command -v qemu-system-x86_64 >/dev/null 2>&1 || {
	echo "qemu-smoke.sh: qemu-system-x86_64 not in PATH" >&2
	exit 1
}

qemu-system-x86_64 \
	-cdrom "$ISO" \
	-m 256M -smp 1 \
	-machine q35 \
	-accel tcg -cpu max \
	-display none \
	-no-reboot -no-shutdown \
	-serial "file:$SERIAL_LOG" \
	-d guest_errors \
	-D "$QEMU_LOG" &
QPID=$!

elapsed=0
while (( elapsed < TIMEOUT )); do
	if grep -q "$FINAL_SENTINEL" "$SERIAL_LOG" 2>/dev/null; then
		kill -TERM "$QPID" 2>/dev/null || true
		wait "$QPID" 2>/dev/null || true
		for s in "${REQUIRED_SENTINELS[@]}"; do
			if ! grep -q "$s" "$SERIAL_LOG" 2>/dev/null; then
				echo "qemu-smoke.sh: required sentinel missing: $s" >&2
				echo "--- serial output ---" >&2
				cat "$SERIAL_LOG" >&2
				exit 5
			fi
		done
		echo "==> PASS (${#REQUIRED_SENTINELS[@]} sentinels in ${elapsed}s)"
		echo
		echo "--- serial output ---"
		cat "$SERIAL_LOG"
		exit 0
	fi
	if ! kill -0 "$QPID" 2>/dev/null; then
		echo "qemu-smoke.sh: QEMU exited before sentinel" >&2
		echo "--- serial output (partial) ---" >&2
		cat "$SERIAL_LOG" >&2 || true
		exit 3
	fi
	sleep 1
	elapsed=$((elapsed + 1))
done

kill -TERM "$QPID" 2>/dev/null || true
sleep 1
kill -KILL "$QPID" 2>/dev/null || true
wait "$QPID" 2>/dev/null || true

if grep -qE "guest CPU|cpu_reset|panic|triple fault" "$QEMU_LOG" 2>/dev/null; then
	echo "qemu-smoke.sh: guest CPU error within ${TIMEOUT}s" >&2
	echo "--- serial ---" >&2
	cat "$SERIAL_LOG" >&2
	echo "--- qemu log (last 40 lines) ---" >&2
	tail -40 "$QEMU_LOG" >&2
	exit 4
fi

echo "qemu-smoke.sh: $FINAL_SENTINEL not seen within ${TIMEOUT}s" >&2
echo "--- serial output ---" >&2
cat "$SERIAL_LOG" >&2
exit 2
