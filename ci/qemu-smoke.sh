#!/usr/bin/env bash
# ci/qemu-smoke.sh
#
# Boot a Field OS ISO under headless QEMU and grep the serial log
# for the boot sentinel (FIELD_OS_BOOT_OK). Used by the CI smoke
# job; safe to run locally as a sanity check.
#
# Always uses TCG (no acceleration) so the same script works in
# GitHub Actions hosted runners (which lack nested virt) and on
# developer laptops regardless of host arch.
#
# Usage:
#   ci/qemu-smoke.sh                    # boots ./field-os-poc.iso
#   ci/qemu-smoke.sh path/to/foo.iso
#
# Tunables (env):
#   SMOKE_TIMEOUT  seconds to wait for sentinel (default: 30)
#
# Exit codes:
#   0  ok        - sentinel found and JIT witness intact within timeout
#   1  missing   - ISO or qemu-system-x86_64 not present
#   2  timeout   - QEMU did not print sentinel within $SMOKE_TIMEOUT
#   3  startup   - QEMU exited unexpectedly before printing anything
#   4  guest_err - QEMU reported guest CPU faults
#   5  jit_witness - JIT-emitted `X` missing or out of order
#                    relative to the entry/invoke log lines

set -euo pipefail

ISO="${1:-field-os-poc.iso}"
TIMEOUT="${SMOKE_TIMEOUT:-30}"
SERIAL_LOG=$(mktemp -t fieldos-smoke-serial.XXXXXX)
QEMU_LOG=$(mktemp -t fieldos-smoke-qemu.XXXXXX)
trap 'rm -f "$SERIAL_LOG" "$QEMU_LOG"' EXIT

[[ -f "$ISO" ]] || {
	echo "qemu-smoke.sh: ISO not found: $ISO" >&2
	echo "build it first with: make iso" >&2
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
	if grep -q "FIELD_OS_BOOT_OK" "$SERIAL_LOG" 2>/dev/null; then
		kill -TERM "$QPID" 2>/dev/null || true
		wait "$QPID" 2>/dev/null || true

		# 5-4d JIT witness bracket. ADR-0001 §3 step 5's exit gate
		# is that `holyc_eval("U0 F() { 'X\n'; } F();")` prints X
		# on serial. Without this bracket a silent regression — jump
		# to wrong offset, miss the printf rel32 patch, mis-aligned
		# string-pool label — would just look weird in the smoke log
		# rather than failing CI. Asserts:
		#   1. `Eval: entry main @ N` line appears
		#   2. before the next `Eval: invoke main` line, a line
		#      containing exactly `X` appears
		#   3. `Eval: invoke main` line appears
		# awk exit codes 1/2/3 distinguish which assertion failed
		# in stderr; the script exits 5 in any of those cases.
		if ! awk '
			/^Eval: entry main / { in_window = 1; saw_entry = 1; next }
			/^Eval: invoke main/ { in_window = 0; saw_invoke = 1 }
			in_window && $0 == "X" { found = 1 }
			END {
				if (!saw_entry)  exit 1
				if (!saw_invoke) exit 2
				if (!found)      exit 3
			}
		' "$SERIAL_LOG"; then
			echo "qemu-smoke.sh: JIT witness regression — X missing or out of order" >&2
			echo "--- serial output ---" >&2
			cat "$SERIAL_LOG" >&2
			exit 5
		fi

		echo "==> PASS (FIELD_OS_BOOT_OK in ${elapsed}s, JIT witness intact)"
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

echo "qemu-smoke.sh: sentinel not seen within ${TIMEOUT}s" >&2
echo "--- serial output ---" >&2
cat "$SERIAL_LOG" >&2
exit 2
