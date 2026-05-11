Kickoff for the next session — M0 step 3D, smoltcp + rustls.

M0 step 3C (virtio bring-up) closed cleanly across seven commits
ending at 3d7072d on 2026-05-09: PCI scan, virtio modern transport,
split-virtqueue infrastructure, virtio-blk + sector-0 read,
virtio-net + probe-TX, plus STATUS + devlog. Smoke now asserts
six sentinels (ARSENAL_BOOT_OK, ARSENAL_HEAP_OK, ARSENAL_FRAMES_OK,
ARSENAL_BLK_OK, ARSENAL_NET_OK, ARSENAL_SCHED_OK). The single
retry of the entire 3C arc was paging::map_mmio — Limine's HHDM
covers RAM only and device BAR ranges need explicit mapping. 3D
puts smoltcp on top of the virtio-net we built, brings TCP up to
the QEMU slirp gateway, and finishes with a rustls TLS handshake.
This is where the build-loop pace from 3A/3B/3C slows: smoltcp
buffer-lifetime invariants are the densest layer of M0 so far,
and rustls's no_std crypto-provider story has more configuration
surface than the protocol itself.

read CLAUDE.md (peer concerns, Rust-only, BSD-2-Clause base, build
loop sacred, no_std + nightly + abi_x86_interrupt) → STATUS.md
(3C complete, 3D is the active sub-block of step 3) →
docs/plan/ARSENAL.md § "Three Concrete Starting Milestones" →
M0 → docs/devlogs/2026-05-arsenal-virtio.md (what 3C landed, the
HHDM/MMIO detour, the calibration note that 3C-4 was easier than
3C-3) → arsenal-kernel/src/virtio_net.rs (RX pre-population, TX
chain shape, buffer ownership pattern smoltcp's Device trait
will reshape) → arsenal-kernel/src/virtio.rs (push_chain,
pop_used, init_transport — the primitives smoltcp's TxToken /
RxToken will sit on top of) → arsenal-kernel/src/sched.rs
(yield_now is what smoltcp's poll-loop will call between work)
→ Cargo.toml (3D adds smoltcp + rustls + webpki / rustls-pki-
types — see trade-off pairs for crate selection) →
git log --oneline -15 → run the sanity check below → propose
3D-N commit shape (or argue for a different decomposition) →
wait for me to pick → "go 3d-N" for code, "draft 3d-N" for
paper deliverables.

Where the project is

  - main is in sync with origin/main at HEAD 3d7072d
    (docs(devlogs): Arsenal virtio bring-up). Working tree is
    clean except this file. The CI workflow at
    .github/workflows/ci.yml is armed and expected green; smoke
    takes ~45 s end-to-end on ubuntu-24.04 runners.
  - LOC: 2734 lines of Rust kernel code in arsenal-kernel/src/
    across 14 files. ~81 KB release ELF. ISO ~17.9 MB.
  - Toolchain: nightly-2026-04-01 pinned in rust-toolchain.toml.
    smoltcp uses no nightly features; rustls's no_std story may
    require a flag bump.
  - Crates currently linked: limine 0.5, linked_list_allocator
    0.10, spin 0.10, x86_64 0.15. 3D adds at least smoltcp
    (likely 0.12+) and rustls (0.23+ for no_std). webpki /
    rustls-pki-types follow rustls. License check first per
    CLAUDE.md §3.
  - Sentinels: smoke requires ARSENAL_BOOT_OK, ARSENAL_HEAP_OK,
    ARSENAL_FRAMES_OK, ARSENAL_BLK_OK, ARSENAL_NET_OK,
    ARSENAL_SCHED_OK. 3D's exit assertions are ARSENAL_TCP_OK
    after a TCP exchange and ARSENAL_TLS_OK after a TLS 1.3
    handshake.
  - HANDOFF.md (this file) is untracked. Overwrite freely; do
    not commit. Refresh between session starts.

3D — smoltcp + rustls

The plan below is the kickoff proposal, not gospel. The user
picks the shape; deviations get justified before code lands.

Sub-candidate decomposition

  (3D-0) **Add smoltcp + dependency vetting.** Append `smoltcp`
         to Cargo.toml with `default-features = false` and the
         feature set we want (medium-ethernet, proto-ipv4, proto-
         dhcpv4, socket-tcp, alloc — no log, no std, no async).
         License check: smoltcp is dual MIT / Apache-2.0; clear.
         Verify it compiles in our no_std + alloc environment
         without pulling in std accidentally. ~10 LOC + Cargo.toml
         changes; one commit: `chore(deps): add smoltcp`. Use
         **go 3d-0**.

  (3D-1) **virtio-net `phy::Device` adapter.** Implement
         smoltcp's `phy::Device` trait against our virtio-net.
         The shape: a `VirtioNetPhy` struct holding the device
         + RX queue + TX queue + buffer pools, with `transmit()`
         and `receive()` returning `TxToken` / `RxToken`. Refactor
         virtio_net.rs's one-shot smoke into a reusable driver
         that the Phy can drive. Buffer pool grows to cover RX
         queue depth + a transmit working set. ~200 LOC, one
         commit: `feat(kernel): smoltcp phy::Device on virtio-net`.
         Use **go 3d-1**.

  (3D-2) **smoltcp interface + DHCPv4.** Construct an
         `Interface` over the Phy, set up a DHCPv4 socket, run
         a poll loop in a kernel task spawned by sched::spawn.
         The poll task yields between iterations. Smoke target:
         the interface gets an IP from QEMU's slirp DHCP server
         (10.0.2.15 is the standard slirp lease), printed to
         serial. ~120 LOC, one commit: `feat(kernel): smoltcp
         interface + DHCPv4`. Use **go 3d-2**.

  (3D-3) **TCP probe + ARSENAL_TCP_OK.** Open a TCP socket,
         connect to QEMU slirp's gateway (10.0.2.2) on a
         knowable port — port 80 to slirp's HTTP forwarder if
         configured, or any closed port for a clean RST. Smoke
         target is a successful three-way handshake (SYN →
         SYN-ACK → ACK) — observed by smoltcp transitioning
         the socket to Established state. ARSENAL_TCP_OK after.
         ~100 LOC, one commit: `feat(kernel): TCP smoke against
         slirp + ARSENAL_TCP_OK`. Use **go 3d-3**.

  (3D-4) **rustls TLS 1.3 handshake + ARSENAL_TLS_OK.** Add
         rustls (0.23+) with the no_std crypto provider — see
         trade-off pairs for ring vs aws-lc-rs vs RustCrypto.
         Wrap the TCP socket in rustls::ClientConnection,
         drive the handshake to completion against a localhost
         test server stood up by ci/qemu-smoke.sh (a tiny
         Python or openssl s_server on a slirp-forwarded port).
         Smoke target: handshake completes,
         ARSENAL_TLS_OK printed. The most slip-prone sub-block
         in 3D — rustls's crypto-provider configuration is
         where the M0 plan's "no surprises" pace ends. ~200
         LOC + smoke script changes, one commit: `feat(kernel):
         rustls handshake + ARSENAL_TLS_OK`. Use **go 3d-4**.

  (3D-5) **STATUS.md refresh + 3D devlog.** STATUS flips 3D
         from "next" to "complete," 3E (framebuffer console)
         becomes the next-session sub-block. Devlog at
         `docs/devlogs/2026-05-arsenal-network.md` records the
         buffer-lifetime adventures with smoltcp's Device
         trait, the crypto-provider selection rationale, and
         the cadence question revisited. Two commits:
         `docs(status): M0 step 3D complete` and
         `docs(devlogs): Arsenal smoltcp + rustls`. Use
         **go 3d-5** for STATUS, **draft 3d-5-devlog** for the
         devlog.

Realistic session-count estimate: 3D-0 + 3D-1 is one focused
session if smoltcp's no_std build is clean (~210 LOC). 3D-2 is
one session — DHCPv4 has interaction surface (offer / ack
timing) that QEMU slirp handles fine but the polling shape can
trip the first attempt. 3D-3 alone could be one session that
needs a retry. 3D-4 is two sessions baseline, three if the
crypto-provider selection bites. Per CLAUDE.md "~15 hours per
week, multiply by ~2.3," call it 4–8 calendar weeks for the
whole of 3D. The pace genuinely slows here vs 3A/3B/3C.

Trade-off pairs to surface explicitly

  **smoltcp version.**
  (i) smoltcp 0.12 (latest stable). Modern API, Interface
  takes a `&mut dyn AnyDevice`-shaped trait. Active maintenance.
  (ii) smoltcp 0.11 or earlier. Older API; some examples in
  the wild reference it. Recommend (i). The API churn between
  0.10 and 0.12 was substantive; staying current avoids a
  future migration we'd pay for anyway.

  **smoltcp feature flags.**
  (i) Minimum: `medium-ethernet`, `proto-ipv4`, `socket-tcp`,
  `proto-dhcpv4`, `alloc`. (ii) Add `proto-ipv6` for future-proof.
  (iii) Add `socket-udp` for DHCP — smoltcp's DHCPv4 socket is
  separate from raw UDP and doesn't require it. (iv) Add `log`
  for debugging, behind a feature gate. Recommend (i) + future
  add of (iv) when something is silently dropping. IPv6 lands
  when something needs it.

  **rustls version.**
  (i) rustls 0.23 (current stable). no_std works behind the
  `no_std` feature; alloc required. Active. (ii) rustls 0.22 —
  no_std support was added in 0.21+ but still maturing.
  Recommend (i). The no_std story is finally calibrated in
  0.23+.

  **rustls crypto provider.**
  (i) `rustls-rustcrypto` — pure Rust, BSD-style licensing,
  no_std-native. The honest fit. Slower than asm-accelerated
  alternatives; for handshake throughput this is fine. (ii)
  `ring` via `rustls/ring` feature — battle-tested, asm-backed,
  but ring's no_std support is incomplete (some primitives
  require std for thread-local). (iii) `aws-lc-rs` — current
  rustls default in std builds; no_std story exists but is
  newer. (iv) Roll our own hand-picked subset (RustCrypto's
  individual crates: aes, sha2, p256, hkdf, x25519). Most
  surface, most flexibility. Recommend (i) for 3D's smoke.
  Performance work revisits when v0.5 ports require throughput.

  **TLS handshake target.**
  (i) Localhost test server. ci/qemu-smoke.sh launches openssl
  s_server (or a Python `ssl` listener) on a slirp-forwarded
  port; kernel connects to that. Self-contained, deterministic,
  no host network dependency. (ii) Public endpoint via slirp
  passthrough — a real cloudflare.com handshake. Vivid demo
  but the smoke loses determinism (network failures, cert
  rotations, slow DNS). Recommend (i). The smoke target's
  point is "TLS works"; the handshake's interlocutor doesn't
  improve the assertion.

  **DHCP vs static IP.**
  (i) DHCPv4 from QEMU slirp. Slirp's built-in DHCP server
  hands out 10.0.2.15 + 10.0.2.2 gateway + 10.0.2.3 DNS. The
  realistic path; matches what real iron will do later. (ii)
  Static IP burned into kernel. Simpler smoke; lose the
  DHCP-validation dimension. Recommend (i). The DHCP machinery
  is small (smoltcp's Dhcpv4Socket is ~1 socket) and validates
  more.

  **Buffer pool sizing for the Phy.**
  (i) RX 16 buffers × 1528 bytes, TX 16 buffers × 1528 bytes.
  Same as virtio-net's current 8 × 1528 RX, scaled. ~50 KiB
  total for the Phy buffers. (ii) RX 64 × 1528, TX 32 × 1528.
  ~150 KiB. Generous; absorbs bursts from a TCP throughput
  test we don't have yet. (iii) Smaller, RX 8 × 1528, TX 8 ×
  1528. Tight; might drop frames under burst. Recommend (i).
  Right-sized for handshake-class workloads; revisit when
  smoltcp drops frames.

  **Polling loop scheduling.**
  (i) Spawn one task that runs `interface.poll(now); yield_now`
  in a tight loop. Cooperative single-CPU; the task gets a
  share each yield. (ii) Spawn three tasks: one for RX, one for
  TX, one for control / DHCP. Independent yield points; finer
  time granularity but more sync surface. (iii) Inline `poll()`
  in every yield_now call. Finest granularity; deepest
  cooperation. Recommend (i). The simplest shape that works.
  Multi-task reorganization revisits when 3F's preemption
  changes the equation.

  **TCP socket buffer sizes.**
  smoltcp lets you size RX / TX buffers per socket. (i) 4 KiB
  each — tiny but enough for handshakes and short exchanges.
  (ii) 64 KiB each — generous; half the heap. Recommend (i)
  for the smoke; a real interactive shell would want (ii).

  **Sub-candidate granularity.**
  (a) Five-commit shape above (deps / phy / iface / tcp / tls)
  plus 3D-5 STATUS+devlog. Bisect-rich. (b) Combine 3D-0+3D-1
  (deps land alongside the first user) for a four-commit shape.
  (c) Combine 3D-3+3D-4 (TCP and TLS together) — TLS strictly
  builds on TCP. Recommend (a). The crypto-provider work in
  3D-4 is genuinely separate from the TCP-shape work in 3D-3;
  bisecting "is the bug in our TCP plumbing or in rustls
  config?" is real on a first-pass integration.

Sanity check before kicking off

    git tag --list | grep field-os-v0.1   # field-os-v0.1
    git log --oneline -10                 # 3d7072d, 0796e7d,
                                          # 174127b, bc6ccfc,
                                          # 8764f62, 1d90405,
                                          # d4ea3d2, beb7340,
                                          # a294224, 1264c20
    git status --short                    # ?? HANDOFF.md (only)
    cargo build -p arsenal-kernel --target x86_64-unknown-none --release
                                          # clean, ~81 KB ELF
    cargo clippy -p arsenal-kernel --target x86_64-unknown-none --release -- -D warnings
                                          # clean
    cargo xtask iso                       # arsenal.iso ~17.9 MB
    ci/qemu-smoke.sh                      # ==> PASS (6 sentinels in 1s)
    cat target/x86_64-unknown-none/release/arsenal-kernel | wc -c
                                          # ~80500

Expected: HEAD as above; only HANDOFF.md untracked; smoke PASSes
in ~1 s with six sentinels including ARSENAL_BLK_OK and
ARSENAL_NET_OK. If smoke hangs after ARSENAL_FRAMES_OK or fails
on the BLK / NET sentinels, something has silently regressed in
the 3C drivers; bisect against the five 3C sub-commits before
proceeding.

Out of scope for 3D specifically

  - Userland network stack. smoltcp + rustls live in the kernel
    for M0; the userland refactor (Comm Tower in a Cardboard Box)
    is post-M0 when Wasmtime + WASI arrive.
  - IPv6 — `proto-ipv4` only. v0.5 or v1.0 territory.
  - DNS resolution — smoltcp doesn't include one; we'd need
    something like `hickory-resolver` or hand-rolled. The TLS
    smoke uses slirp's gateway IP (10.0.2.2) directly, no DNS.
  - HTTP, websockets, or any application-layer protocol beyond
    the TLS handshake.
  - WPA / 802.11 / iwlwifi — virtio-net is wired QEMU; real
    wireless lands in M1 via LinuxKPI.
  - IRQ-driven I/O. 3F brings the LAPIC timer; 3D continues the
    yield-poll pattern from 3C.
  - Anything in 3E–3G. Each gets its own kickoff.

Permanently out of scope (do not propose)

  - Any unsafe block without a // SAFETY: comment naming the
    invariant the caller must uphold. CLAUDE.md hard rule.
  - Reverting any 3A / 3B / 3C commit. All landed and cited by
    STATUS.md / 3A / 3B / 3C devlogs.
  - Force-pushing to origin. Branch is in sync; preserve history.
  - Dropping the BSD-2-Clause license header from any new file.
  - Pulling a GPL crate into the kernel base. M1's LinuxKPI shim
    will host GPLv2 inherited drivers under explicit license
    boundaries; nothing of that lands in M0. The Limine binaries
    under vendor/limine/ remain the boot-time-only exception.
  - Religious framing. CLAUDE.md hard rule.
  - Reintroducing HolyC. ADR-0004's discard is final.
  - Going back to stable Rust. The pin to nightly is permanent
    until / unless every nightly feature we use becomes stable.

Three notes worth flagging before you go

  1. **Buffer lifetime is the densest invariant in 3D.** smoltcp's
     `phy::Device` trait hands callers TxToken and RxToken whose
     lifetimes are bounded by the device borrow. Inside the
     token's `consume`, the caller writes / reads bytes; the
     token's drop submits to the device. Our virtio-net's RX
     pre-population pattern (Vec<Box<RxBuffer>>) has to migrate
     into a buffer pool that the Phy owns and recycles as RX
     completions land. Get this wrong and the device DMAs into
     freed memory; get it right and the pool is the cleanest
     part of the layer. Read smoltcp/examples/loopback.rs's Phy
     implementation before writing ours.

  2. **rustls crypto-provider selection is config, not code.**
     The handshake works either way (ring, rustcrypto,
     aws-lc-rs); the choice is licensing + maintenance + how
     much of rustls's example tree carries over. Picking
     rustls-rustcrypto means accepting slower handshakes
     (immaterial for one smoke handshake) and avoiding the
     "ring's no_std story is incomplete" trap. The rustls
     0.23 docs have the configuration recipe; deviating from it
     costs more than following it. If a future v0.5 perf gate
     wants ring or aws-lc-rs, swap then.

  3. **The 3A/3B/3C pace doesn't generalize.** Three sub-blocks
     in one calendar day is the asymmetry of well-trodden,
     spec-driven ground. 3D is still mostly spec-driven (RFC
     9293 for TCP, RFC 8446 for TLS) but the implementation
     layer (smoltcp + rustls) brings buffer-lifetime invariants
     that don't appear in PCI / virtio. Calibrate the HANDOFF
     for "this is where the integration testing time shows up,"
     not "this is where the spec is hard." If 3D-3 slips on
     ARP / DHCP timing or 3D-4 slips on crypto-provider
     mismatches, that's expected. Slow is fine.

Wait for the pick. Do not pick silently. The natural first split
is 3D-0 + 3D-1 in one focused session ("deps + Phy adapter"),
3D-2 alone in a session (DHCP machinery + interface bring-up),
3D-3 in a session that may need a retry, 3D-4 in two sessions if
the crypto-provider selection bites. Happy to do 3D-0 alone if
you want to confirm smoltcp's no_std build cleanliness before
committing to the rest. Your call.
