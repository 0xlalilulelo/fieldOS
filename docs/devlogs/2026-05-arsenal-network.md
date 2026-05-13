# M0 step 3D — smoltcp + rustls

*May 11, 2026. Three sessions. Five commits.*

3D is the fourth of seven sub-blocks in M0 step 3 (memory, scheduler,
virtio, network, framebuffer, SMP, `>` prompt). The exit criterion is
narrow: a TCP/IP stack on top of virtio-net that pulls a DHCPv4 lease
through QEMU's slirp gateway, opens a TCP connection to the host, and
completes a TLS 1.3 handshake against a self-signed listener. After
3D, the kernel speaks IP, TCP, and TLS — every wire-format layer
between the Ethernet frame 3C round-tripped and an HTTPS request is
present. The application layer above is M1+ territory.

## What landed

Five commits across three sessions:

- `622a436` *chore(deps): add smoltcp 0.12.* End-of-3C dep stub.
  smoltcp 0.12 in no_std + alloc mode (default features pulled `std`
  and `log`; both off). Features narrowed to `medium-ethernet`,
  `proto-ipv4`, `socket-tcp`, `socket-dhcpv4`, `alloc`. License:
  MIT OR Apache-2.0, clear under CLAUDE.md §3. Landed alone because
  the Phy adapter in 3D-1 wanted the dependency present but the
  shape of the wrapper hadn't been decided.
- `4499dc0` *feat(kernel): smoltcp phy::Device on virtio-net.* The
  adapter that mediates between smoltcp's `phy::Device` trait and
  virtio-net's RX / TX descriptor rings. RX completions surface as
  `RxToken::consume` borrows into the used-ring buffer; TX requests
  drive a `TxToken` that claims a free buffer, lets smoltcp fill
  it, and pushes the chain through `virtio_net::send_buffer`. The
  surface that wanted careful documentation was buffer ownership —
  smoltcp wants exclusive `&mut [u8]` access to RX bytes for the
  duration of `consume`, and our RX pool has to hold those buffers
  past consume's lifetime for the device to keep DMA'ing into them.
  Resolved by letting `consume` copy into an owned `Vec<u8>` —
  smoltcp's documentation says zero-copy is preferred but copy is
  allowed; one extra memcpy per RX is fine at M0 throughput.
- `f8bfa02` *feat(kernel): smoltcp interface + DHCPv4.* `net::init`
  builds an `Interface` over the Phy with a synthesized MAC (the
  one virtio-net reported via NET_F_MAC), installs a DHCPv4 socket,
  spawns `net::poll_loop` as a cooperative task. The poll loop
  yields between iterations; each iteration calls `iface.poll`,
  inspects the DHCP socket for state transitions, and on
  `Configured` snapshots the IP / gateway and stamps both onto the
  Interface. The slirp lease (10.0.2.15/24, gw 10.0.2.2) lands on
  the first poll after `Interface::up` and prints on serial. No
  new sentinel — the DHCP confirmation is informative, not
  load-bearing.
- `8c8a599` *feat(kernel): TCP smoke against slirp + ARSENAL_TCP_OK.*
  A second cooperative task opens a TCP socket to 10.0.2.2:12345
  (a Python listener stood up by the smoke script). The poll loop
  drives the socket through SYN-sent → established; on the first
  poll cycle where the socket reports `is_active() && may_send()`,
  prints the local + remote endpoint pair and the sentinel. The
  smoke script binds 12345 with a one-shot `socket.accept()` that
  closes the connection immediately — the goal is the three-way
  handshake, not data exchange. ARSENAL_TCP_OK lands seven kernel
  ticks after the DHCP lease.
- `db4625e` *feat(kernel): rustls handshake + ARSENAL_TLS_OK.*
  rustls 0.23 with the no_std + alloc shape: `default-features =
  false`, `tls12` enabled. Crypto provider is rustls-rustcrypto
  0.0.2-alpha (pre-release; the alternative was rolling a
  RustCrypto-based provider from scratch, which is more surface
  than this one crate). Cert verification is `NoopServerVerifier`
  — the smoke listener uses a fresh self-signed P-256 cert each
  run, and verifying it would require dragging trust roots into
  the kernel. Documented as smoke-only; M2+ wires real verification.
  Third cooperative task opens a TCP socket to 10.0.2.2:12346,
  wraps it in `UnbufferedClientConnection` (rustls's no_std API),
  drives the state machine to `WriteTraffic`. ARSENAL_TLS_OK on
  arrival; the socket is then removed from the `SocketSet` to
  sidestep a smoltcp post-close panic (see below). ELF jumps from
  ~144 KB to ~1.46 MB, dominated by RustCrypto primitives — AES,
  ChaCha20-Poly1305, P-256 / P-384 / X25519 / Ed25519, RSA, SHA-2
  — plus rustls's protocol state machines.

## How long it took

Three sessions across two calendar days. The first commit
(`622a436` deps stub) closed the 3C evening on 2026-05-09;
`4499dc0` (Phy adapter) ran solo on the 2026-05-11 morning; the
three afternoon commits (`f8bfa02` / `8c8a599` / `db4625e`) landed
across one continuous block. Maybe six hours of active time across
the three.

This was the slowest M0 step 3 sub-block to date. 3A and 3B and 3C
each compressed into one calendar day; 3D wanted two. The asymmetry
between hardware-spec ground (PCI / virtio / page tables) and
multi-crate dep archaeology is real and the right thing to call out
ahead of future sessions. Spec-driven work runs at HANDOFF reading
speed plus typing speed. Crate-integration work runs at "the
docs say X but reality says Y" speed; the prior HANDOFF's flagging
of the rustls no_std API as bug-prone was directionally right and
under-counted by about a session.

That recalibration matters for what comes next. 3E (framebuffer) is
spec-driven again — Limine's framebuffer payload shape is in
`docs/skills/limine.md` (or will be when that file lands; the
pattern is documented in the limine crate's request types), and a
glyph bitmap is local data. The 3D pace was not the new baseline; it
was the cost of integrating two crates that hadn't seen no_std use
together before. 3F (SMP) returns to the 3D shape — getting Apple
Silicon's memory model right against x86_64's TSO assumptions is
"reality argues with the spec" territory.

## Detours worth recording

**The two-getrandom-version dance.** rustls-rustcrypto pulls
getrandom 0.4 directly and getrandom 0.2 transitively (via older
RustCrypto rc.N crates that haven't migrated yet). On
`x86_64-unknown-none` neither version has a system RNG to dispatch
to, and the two versions use entirely different custom-backend
mechanisms. getrandom 0.4 expects a build-time cfg
(`--cfg getrandom_backend="custom"` in `.cargo/config.toml`) plus an
`extern "Rust"` fn with a fixed name `__getrandom_v04_custom`.
getrandom 0.2 expects the `custom` cargo feature enabled (alongside
renaming the crate to `getrandom02` in Cargo.toml to disambiguate
from 0.4) plus a `register_custom_getrandom!` macro invocation
pointing at a regular function. Wiring one and not the other
manifests as link-time "undefined reference to `getrandom_inner`"
that takes a minute to map back to "which crate's getrandom is
this." Both backends now route through a single `fill_bytes` that
tries RDRAND first and falls back to a TSC xor-jumble — the fallback
exists for hypervisors that don't synthesize RDRAND on the boot CPU,
which is a hazard surface CLAUDE.md doesn't budget against but
TCG smoke does. Source: [`arsenal-kernel/src/rand.rs`](../../arsenal-kernel/src/rand.rs).

**rustls's no_std API is not `Connection`.** The std-mode rustls
`Connection` (`ClientConnection` / `ServerConnection`) takes a
`Read + Write` impl and runs as a blocking call: you give it a
TCP socket, it does the handshake. The HANDOFF for 3D-4 assumed
that shape and budgeted "wire smoltcp's TCP socket as Read + Write,
hand it to rustls, done." That API doesn't exist in no_std mode.
What exists is `UnbufferedClientConnection`, an explicit state
machine you pump via `process_tls_records(&mut incoming_buf)`,
which returns a `ConnectionState` discriminant tag —
`ReadTraffic` / `EncodeTlsData` / `TransmitTlsData` /
`BlockedHandshake` / `WriteTraffic` / `Closed` — that the caller
matches on, encodes outgoing bytes from, and shuttles to and from
the underlying transport. The handshake loop is ~50 LOC of state-
machine drive; the structure is in
[`net.rs:328-483`](../../arsenal-kernel/src/net.rs). Discovering
the API was one debug session; implementing against it once
discovered was straightforward. The cost was the discovery, and
the cost would have been zero if a single sentence in the rustls
no_std docs had said "the std `Connection` API is unavailable;
see `UnbufferedClientConnection`." That sentence is not present;
the no_std story is described as a feature flag rather than an
API change.

**smoltcp 0.12's `seq_to_transmit` unwrap.** After the TLS handshake
reaches `WriteTraffic`, the kernel prints the sentinel and the smoke
is satisfied. The Python listener's `wrap_socket` closes the
connection cleanly post-handshake — `close_notify` over TLS, then
TCP FIN. smoltcp processes the FIN, transitions the TCP socket to
`Closed`, and at that point `tuple` becomes `None`. The next
`iface.poll` iteration calls `seq_to_transmit` on that socket, which
in 0.12 unwraps `self.tuple` unconditionally and panics. The
mitigation that landed: immediately after firing `ARSENAL_TLS_OK`,
remove the TLS TCP socket from the `SocketSet` so smoltcp stops
polling it. That's correct for the smoke's purpose (the connection
is done) and side-steps a bug we'd otherwise have to file against
smoltcp upstream. It's a bug — `seq_to_transmit` should match on
`tuple` rather than unwrap — and the right longer-term fix is a
patch upstream. The mitigation is in
[`net.rs:471-481`](../../arsenal-kernel/src/net.rs) with a comment
explaining why removing the socket is correct rather than a
workaround.

**The smoke wait-loop's FINAL_SENTINEL → all-required-present
refactor.** The smoke script's pre-3D wait loop watched for a
single `FINAL_SENTINEL` (`ARSENAL_SCHED_OK` at end of 3B; would
have rotated to `ARSENAL_NET_OK` at 3C, then `ARSENAL_TCP_OK`,
then `ARSENAL_TLS_OK`). The 3D-3 rewrite changed the contract from
"final sentinel fires" to "all required sentinels present (in any
order, within the timeout)." Same observable behavior at 3D, but
3E–3G can now add a sentinel to `REQUIRED_SENTINELS` without
touching the wait loop. The bookkeeping cost (~20 LOC of bash) was
the smaller half of the diff; the larger half was the script's new
prereqs — `openssl req` for the self-signed cert and two Python
listeners (plain TCP + TLS 1.3) stood up before QEMU launches.
ubuntu-24.04 runners ship both; the prereq check at the top of the
script exits with a useful message if either is missing.

**The 144 KB → 1.46 MB ELF jump.** 3D-1 added smoltcp; the kernel
grew from ~81 KB (end of 3C) to ~144 KB. That's a 60 KB add for a
TCP/IP stack with DHCP, which is a small price. 3D-4 added rustls
plus RustCrypto's whole primitives suite, and the kernel grew from
~144 KB to ~1.46 MB. That 10× isn't an accident: rustls-rustcrypto
pulls AES (with separate AES-NI and software paths), ChaCha20-
Poly1305, P-256, P-384, X25519, Ed25519, RSA (for legacy server
trust), and SHA-2 — every primitive a 1.3 server could ever offer.
The smoke uses one cipher suite (TLS_AES_256_GCM_SHA384 negotiated
against the Python listener's defaults). Tree-shaking would help
substantially — the unused primitives still consume code size in
release builds because rustls's runtime negotiation forces them
referenceable. A custom `CryptoProvider` exposing only one cipher
suite would cut the binary back toward ~300 KB; the cost is hand-
maintaining the provider against rustls's API. Deferred to M2+
when binary-size budgets get real. M0's gate is "boots in under 2 s
under QEMU" and 1.46 MB at TCG speed still hits that.

## The numbers

- **5 commits.** Deps / Phy adapter / DHCPv4 / TCP / TLS. Plus this
  devlog and the (already-landed) STATUS update — 7 commits if you
  count the paper trail.
- **3665 lines of Rust kernel code** in `arsenal-kernel/src/`, up
  from 2734 at end of 3C. Net +931 LOC. New modules: `net.rs` (598),
  `rand.rs` (118). `virtio_net.rs` grew from 192 to 400 lines
  (+208), absorbing the descriptor-pool refactor that the Phy
  adapter needed. `main.rs` grew by ~15 lines; `Cargo.toml` grew by
  ~26 lines for the new dependencies plus the documentation
  comments explaining each one. `ci/qemu-smoke.sh` grew by ~131
  lines for the cert + dual-listener prep.
- **~1.46 MB ELF**, up from ~81 KB at end of 3C — 18× total, with
  ~10× of that arriving in `db4625e` alone (rustls + RustCrypto).
  ISO ~19.3 MB.
- **~1 second** local TCG smoke. Eight sentinels:
  `ARSENAL_BOOT_OK`, `ARSENAL_HEAP_OK`, `ARSENAL_FRAMES_OK`,
  `ARSENAL_BLK_OK`, `ARSENAL_NET_OK`, `ARSENAL_SCHED_OK`,
  `ARSENAL_TCP_OK`, `ARSENAL_TLS_OK`.
- **3 wire-format layers** brought up end-to-end:
  IPv4 (smoltcp's stack), TCP (smoltcp's socket), TLS 1.3 (rustls).
  Each gates a sentinel; each runs against a real listener stood up
  by the smoke script.
- **0 `unsafe` blocks added in 3D-3 / 3D-4.** The new code is
  smoltcp's Rust API, rustls's Rust API, and a small amount of
  socket plumbing. The `unsafe` continues to live where it always
  has — `serial`, `paging::map_mmio`, the asm in `sched`, the
  CPUID / RDTSC in `rand`, the virtio MMIO accessors.

## What the boot looks like

The serial trace is now twenty-one lines past the 3C baseline,
ending at `ARSENAL_TLS_OK`:

```
...
ARSENAL_NET_OK
net: smoltcp Interface up mac=52:54:00:4a:52:53
sched: init complete; switching to idle
sched: idle running
ping
pong
ping
pong
net: DHCPv4 lease ip=10.0.2.15/24 gw=Some(10.0.2.2)
net: TCP connect -> 10.0.2.2:12345
net: TLS connect -> 10.0.2.2:12346
ping
pong
ARSENAL_SCHED_OK
net: TCP established local=Some(Endpoint { addr: Ipv4(10.0.2.15), port: 49152 }) remote=Some(Endpoint { addr: Ipv4(10.0.2.2), port: 12345 })
ARSENAL_TCP_OK
net: TLS 1.3 handshake complete
ARSENAL_TLS_OK
```

The two relevant lines for 3D's exit criterion: the DHCP lease (a
slirp 10.0.2.15/24 with the canonical 10.0.2.2 gateway) and the TLS
1.3 handshake completion against the Python listener on 12346. The
TCP established line in between proves the plain-TCP path also
works, which is the load-bearing intermediate assertion — TLS
without a working TCP underneath would surface as an
EncodeTlsData stuck state, not the clean transit through the
state machine we see.

## What 3E looks like

Per ARSENAL.md M0, the next sub-block: framebuffer console.

- **Limine FramebufferRequest probe.** Add the request block,
  read back bpp / pitch / width / height / linear address. QEMU
  std-vga is 1024×768×32 by default; the address arrives
  HHDM-mapped. No new sentinel — informational log only.
- **Pixel write primitives.** `fb::clear(rgb)`, `fb::put_pixel(x,
  y, rgb)`. Smoke: clear-to-navy `#0A1A2A`, draw a 16×16 amber
  `#FFB200` square at (8, 8). Visible in `-display gtk`; smoke
  stays headless.
- **8×16 glyph bitmap.** Public-domain VGA 8×16 font, embedded as
  a 4 KiB static. `fb::render_glyph` + `fb::render_string`. Smoke
  draws "ARSENAL" at (8, 32) in amber on navy. Plex Mono belongs
  to M2 Stage's rasterize→atlas→wgpu pipeline; running that
  pipeline at M0 is premature.
- **fmt::Write + serial mirror.** A `FbWriter` implementing
  `core::fmt::Write` the same way `serial::Writer` does, with a
  cursor and scroll-by-blit. Then `serial::write_str` and
  `serial::Writer` both fan out to `fb::print` once the
  framebuffer is initialized. The screen mirror captures every
  sentinel from `ARSENAL_FRAMES_OK` onward; `ARSENAL_BOOT_OK`
  prints before fb-init and stays serial-only.

3E does not fight dependencies. The bottleneck is keyboard speed,
not crate archaeology. The bug-prone surface is the scroll-by-blit
math (off-by-one against `height % glyph_h` is the classic) and
the serial-mirror fan-out (sentinels are emitted by
`serial::write_str` rather than `Writer`, so the multiplex has to
land at the byte level, not the `fmt::Write` level).

## Cadence

This is the fourth sub-block devlog of M0 step 3 (3A, 3B, 3C, 3D).
3D had three substantive detours — the two-getrandom-version
dance, the rustls no_std API discovery, the smoltcp post-close
unwrap — plus the smoke-loop refactor and the binary-size jump.
Per-sub-block continues to feel useful rather than rote. 3E may
be the first one where consolidation is the right call; if it is,
the M0 step 3 wrap-up gets richer.

The Asahi cadence stays the model — calibrated, honest, never
marketing.

—
