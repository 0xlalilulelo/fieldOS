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
#   TLS_SMOKE_PORT   host TLS port to listen on for 3D-4 (default: 12346)
#   BOOT_BUDGET_MS   M0 step 3 perf gate from ARSENAL.md: "boot to
#                    prompt in < 2 s under QEMU". Measured as the
#                    wall-clock delta between ARSENAL_BOOT_OK (kernel
#                    first serial line, after Limine hands off) and
#                    ARSENAL_PROMPT_OK (shell task online).
#                    Default 3000 ms gives hosted runner variance
#                    headroom over ARSENAL.md's 2000 ms target; set
#                    BOOT_BUDGET_MS=2000 for the local conformance
#                    check. No retry on overage — flake response is
#                    raise the budget or fix the variance source,
#                    not silently re-run.
#
# Exit codes:
#   0  ok         all required sentinels found within timeout
#   1  missing    ISO, qemu-system-x86_64, python3, or openssl not present
#   2  timeout    one or more required sentinels missing within timeout
#   3  startup    QEMU exited unexpectedly before all sentinels
#   4  guest_err  QEMU reported guest CPU faults
#   5  perf       boot-to-prompt exceeded BOOT_BUDGET_MS

set -euo pipefail

ISO="${1:-arsenal.iso}"
TIMEOUT="${SMOKE_TIMEOUT:-15}"
TCP_SMOKE_PORT="${TCP_SMOKE_PORT:-12345}"
TLS_SMOKE_PORT="${TLS_SMOKE_PORT:-12346}"
BOOT_BUDGET_MS="${BOOT_BUDGET_MS:-3000}"

# Millisecond-resolution wall clock. Python is already a smoke
# dependency (listener harness), so we use it rather than wrestle
# with `date`'s sub-second portability gap between Linux (%N) and
# macOS (no %N). Called only when a tracked sentinel is first
# observed — at most $#REQUIRED_SENTINELS times per run — so the
# python startup cost stays out of the polling hot path.
now_ms() {
	python3 -c 'import time; print(int(time.time()*1000))'
}
# Required sentinels must all appear (in any order) within $TIMEOUT
# for the smoke to pass. Add a sentinel here when a milestone wants
# its "this subsystem survived" assertion in CI; remove one only when
# the underlying assertion is folded into a stronger downstream
# sentinel. Order does not matter — we wait for the full set.
REQUIRED_SENTINELS=("ARSENAL_BOOT_OK" "ARSENAL_HEAP_OK" "ARSENAL_FRAMES_OK" "ARSENAL_BLK_OK" "ARSENAL_NET_OK" "ARSENAL_SCHED_OK" "ARSENAL_TCP_OK" "ARSENAL_TLS_OK" "ARSENAL_TIMER_OK" "ARSENAL_PROMPT_OK")
SERIAL_LOG=$(mktemp -t arsenal-smoke-serial.XXXXXX)
QEMU_LOG=$(mktemp -t arsenal-smoke-qemu.XXXXXX)
CERT_DIR=$(mktemp -d -t arsenal-smoke-cert.XXXXXX)
LISTENER_PID=""
TLS_LISTENER_PID=""
cleanup() {
	if [[ -n "$LISTENER_PID" ]]; then
		kill -KILL "$LISTENER_PID" 2>/dev/null || true
	fi
	if [[ -n "$TLS_LISTENER_PID" ]]; then
		kill -KILL "$TLS_LISTENER_PID" 2>/dev/null || true
	fi
	rm -f "$SERIAL_LOG" "$QEMU_LOG"
	rm -rf "$CERT_DIR"
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
	echo "qemu-smoke.sh: python3 not in PATH (needed for 3D-3 / 3D-4 listeners)" >&2
	exit 1
}

command -v openssl >/dev/null 2>&1 || {
	echo "qemu-smoke.sh: openssl not in PATH (needed for 3D-4 self-signed cert)" >&2
	exit 1
}

# 3D-4: generate a self-signed ECDSA P-256 cert for the TLS listener.
# Single SAN entry covering the guest's connect target. The kernel's
# NoopServerVerifier accepts anything; openssl just needs a cert to
# present in the handshake.
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 \
	-keyout "$CERT_DIR/key.pem" -out "$CERT_DIR/cert.pem" \
	-days 1 -nodes -subj "/CN=arsenal.smoke" \
	-addext "subjectAltName=DNS:arsenal.smoke,IP:10.0.2.2,IP:127.0.0.1" \
	>/dev/null 2>&1 || {
	echo "qemu-smoke.sh: openssl req failed (cert generation)" >&2
	exit 1
}

# 3D-3: stand up a plain TCP listener on the host so the kernel's
# smoltcp probe completes a real three-way handshake. slirp NATs the
# guest's connect to 10.0.2.2:TCP_SMOKE_PORT to host
# 127.0.0.1:TCP_SMOKE_PORT. The host's kernel TCP stack handles
# SYN-ACK as soon as we listen(), regardless of whether userspace has
# called accept() — Python just needs to bind and keep the process
# alive so the socket stays in LISTEN. We accept-and-sleep so the
# connection persists after the kernel observes Established.
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

# 3D-4: stand up a TLS 1.3 listener with the self-signed cert. Python's
# ssl module wraps the accepted socket; the handshake runs against
# arsenal-kernel's UnbufferedClientConnection. Accept loop is single-
# shot for the smoke; on handshake completion we sleep so the
# connection persists past the kernel's WriteTraffic observation.
python3 -c "
import socket, ssl, time
ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
ctx.minimum_version = ssl.TLSVersion.TLSv1_3
ctx.load_cert_chain(certfile='$CERT_DIR/cert.pem', keyfile='$CERT_DIR/key.pem')
s = socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(('127.0.0.1', $TLS_SMOKE_PORT))
s.listen(1)
try:
    raw, _ = s.accept()
    tls = ctx.wrap_socket(raw, server_side=True)
    time.sleep(3600)
except Exception as e:
    import sys; print('tls listener:', e, file=sys.stderr)
" &
TLS_LISTENER_PID=$!
disown "$TLS_LISTENER_PID" 2>/dev/null || true

# Give both listeners a moment to bind before QEMU's userspace nets up.
sleep 0.3

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

# Polling loop for the perf gate.
#
# Resolution caveat: we measure "time between observing BOOT_OK in
# the log and observing PROMPT_OK in the log" at ~50 ms granularity
# (the sleep interval). When kernel boot is fast enough that both
# sentinels land in the same poll, the recorded delta is 0 ms — a
# truthful observation that boot-to-prompt fits within one polling
# interval, not a measurement error. The gate trips when boot grows
# to multiple polling intervals (50 ms × N), so it catches
# regressions where boot-to-prompt exceeds ~one polling cycle but
# not microsecond drift. Finer resolution would require streaming
# QEMU's serial through a timestamper (mkfifo + python tee); a real
# but post-M0 surface, flagged for the M0 step 3 retrospective.
#
# Each sentinel's first-seen wall clock is captured exactly once via
# a parallel "found" flag array (bash 3.2-compatible — macOS's
# system bash does not have associative arrays). Sentinels that
# participate in the perf gate (ARSENAL_BOOT_OK and
# ARSENAL_PROMPT_OK) get their own dedicated capture variables.
SENTINEL_FOUND=()
for _ in "${REQUIRED_SENTINELS[@]}"; do
	SENTINEL_FOUND+=("0")
done
FOUND_COUNT=0
TOTAL_SENTINELS=${#REQUIRED_SENTINELS[@]}
BOOT_OK_MS=""
PROMPT_OK_MS=""
START_MS=$(now_ms)
TIMEOUT_MS=$((TIMEOUT * 1000))

while true; do
	# Snapshot the wall clock once per iteration. Sentinels that
	# first appear in the same poll all share this stamp — the
	# measurement resolution is the poll interval (~50 ms), not the
	# accumulating python-startup cost of calling `now_ms` per
	# sentinel inside the inner loop.
	iter_ms=$(now_ms)
	for i in "${!REQUIRED_SENTINELS[@]}"; do
		if [[ "${SENTINEL_FOUND[$i]}" == "0" ]]; then
			s="${REQUIRED_SENTINELS[$i]}"
			if grep -q "$s" "$SERIAL_LOG" 2>/dev/null; then
				SENTINEL_FOUND[$i]="1"
				FOUND_COUNT=$((FOUND_COUNT + 1))
				case "$s" in
					ARSENAL_BOOT_OK) BOOT_OK_MS=$iter_ms ;;
					ARSENAL_PROMPT_OK) PROMPT_OK_MS=$iter_ms ;;
				esac
			fi
		fi
	done
	if (( FOUND_COUNT == TOTAL_SENTINELS )); then
		kill -TERM "$QPID" 2>/dev/null || true
		wait "$QPID" 2>/dev/null || true

		end_ms=$(now_ms)
		total_ms=$((end_ms - START_MS))
		boot_to_prompt_ms=$((PROMPT_OK_MS - BOOT_OK_MS))

		echo "==> PASS (${#REQUIRED_SENTINELS[@]} sentinels in ${total_ms} ms)"
		echo "    boot→prompt: ${boot_to_prompt_ms} ms (budget ${BOOT_BUDGET_MS} ms)"
		echo
		echo "--- serial output ---"
		cat "$SERIAL_LOG"

		if (( boot_to_prompt_ms > BOOT_BUDGET_MS )); then
			echo >&2
			echo "qemu-smoke.sh: boot-to-prompt ${boot_to_prompt_ms} ms exceeds BOOT_BUDGET_MS=${BOOT_BUDGET_MS}" >&2
			echo "  ARSENAL.md M0 step 3 perf gate target is 2000 ms; default budget is 3000 ms for hosted-runner headroom." >&2
			echo "  Set BOOT_BUDGET_MS=<n> to override, or investigate the variance source." >&2
			exit 5
		fi
		exit 0
	fi
	if ! kill -0 "$QPID" 2>/dev/null; then
		echo "qemu-smoke.sh: QEMU exited before all sentinels" >&2
		echo "--- serial output (partial) ---" >&2
		cat "$SERIAL_LOG" >&2 || true
		exit 3
	fi
	if (( (iter_ms - START_MS) >= TIMEOUT_MS )); then
		break
	fi
	sleep 0.05
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
