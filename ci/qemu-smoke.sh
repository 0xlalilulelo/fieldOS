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
#   SMOKE_TIMEOUT    seconds to wait for sentinels (default: 15)
#   TCP_SMOKE_PORT   host TCP port to listen on for 3D-3 (default: 12345)
#
# Exit codes:
#   0  ok         all required sentinels found within timeout
#   1  missing    ISO, qemu-system-x86_64, or python3 not present
#   2  timeout    one or more required sentinels missing within timeout
#   3  startup    QEMU exited unexpectedly before all sentinels
#   4  guest_err  QEMU reported guest CPU faults

set -euo pipefail

ISO="${1:-arsenal.iso}"
TIMEOUT="${SMOKE_TIMEOUT:-15}"
TCP_SMOKE_PORT="${TCP_SMOKE_PORT:-12345}"
# Required sentinels must all appear (in any order) within $TIMEOUT
# for the smoke to pass. Add a sentinel here when a milestone wants
# its "this subsystem survived" assertion in CI; remove one only when
# the underlying assertion is folded into a stronger downstream
# sentinel. Order does not matter — we wait for the full set.
REQUIRED_SENTINELS=("ARSENAL_BOOT_OK" "ARSENAL_HEAP_OK" "ARSENAL_FRAMES_OK" "ARSENAL_BLK_OK" "ARSENAL_NET_OK" "ARSENAL_SCHED_OK" "ARSENAL_TCP_OK")
SERIAL_LOG=$(mktemp -t arsenal-smoke-serial.XXXXXX)
QEMU_LOG=$(mktemp -t arsenal-smoke-qemu.XXXXXX)
LISTENER_PID=""
cleanup() {
	if [[ -n "$LISTENER_PID" ]]; then
		kill -KILL "$LISTENER_PID" 2>/dev/null || true
	fi
	rm -f "$SERIAL_LOG" "$QEMU_LOG"
}
trap cleanup EXIT

[[ -f "$ISO" ]] || {
	echo "qemu-smoke.sh: ISO not found: $ISO" >&2
	echo "build it first with: cargo xtask iso" >&2
	exit 1
}

command -v qemu-system-x86_64 >/dev/null 2>&1 || {
	echo "qemu-smoke.sh: qemu-system-x86_64 not in PATH" >&2
	exit 1
}

command -v python3 >/dev/null 2>&1 || {
	echo "qemu-smoke.sh: python3 not in PATH (needed for 3D-3 TCP listener)" >&2
	exit 1
}

# 3D-3: stand up a TCP listener on the host so the kernel's smoltcp
# probe completes a real three-way handshake. slirp NATs the guest's
# connect to 10.0.2.2:TCP_SMOKE_PORT to host 127.0.0.1:TCP_SMOKE_PORT.
# The host's kernel TCP stack handles SYN-ACK as soon as we listen(),
# regardless of whether userspace has called accept() — Python just
# needs to bind and keep the process alive so the socket stays in
# LISTEN. We accept-and-discard so the connection persists after the
# kernel observes Established.
python3 -c "
import socket, time
s = socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(('127.0.0.1', $TCP_SMOKE_PORT))
s.listen(1)
try:
    conn, _ = s.accept()
    time.sleep(3600)
except Exception:
    pass
" &
LISTENER_PID=$!
# Drop the listener from bash's job table so killing it during cleanup
# doesn't emit "Killed: 9" to stderr.
disown "$LISTENER_PID" 2>/dev/null || true
# Give the listener a moment to bind before QEMU's userspace nets up.
sleep 0.2

qemu-system-x86_64 \
	-cdrom "$ISO" \
	-m 256M -smp 1 \
	-machine q35 \
	-accel tcg -cpu max \
	-device virtio-rng-pci \
	-drive file="$ISO",if=none,id=blk0,format=raw,readonly=on \
	-device virtio-blk-pci,drive=blk0 \
	-netdev user,id=net0 \
	-device virtio-net-pci,netdev=net0 \
	-display none \
	-no-reboot -no-shutdown \
	-serial "file:$SERIAL_LOG" \
	-d guest_errors \
	-D "$QEMU_LOG" &
QPID=$!

elapsed=0
while (( elapsed < TIMEOUT )); do
	all_present=true
	for s in "${REQUIRED_SENTINELS[@]}"; do
		if ! grep -q "$s" "$SERIAL_LOG" 2>/dev/null; then
			all_present=false
			break
		fi
	done
	if $all_present; then
		kill -TERM "$QPID" 2>/dev/null || true
		wait "$QPID" 2>/dev/null || true
		echo "==> PASS (${#REQUIRED_SENTINELS[@]} sentinels in ${elapsed}s)"
		echo
		echo "--- serial output ---"
		cat "$SERIAL_LOG"
		exit 0
	fi
	if ! kill -0 "$QPID" 2>/dev/null; then
		echo "qemu-smoke.sh: QEMU exited before all sentinels" >&2
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

echo "qemu-smoke.sh: required sentinels missing after ${TIMEOUT}s" >&2
for s in "${REQUIRED_SENTINELS[@]}"; do
	if ! grep -q "$s" "$SERIAL_LOG" 2>/dev/null; then
		echo "  missing: $s" >&2
	fi
done
echo "--- serial output ---" >&2
cat "$SERIAL_LOG" >&2
exit 2
