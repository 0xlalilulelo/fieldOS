// SPDX-License-Identifier: BSD-2-Clause
//
// smoltcp Interface + DHCPv4 + TCP smoke + TLS 1.3 handshake smoke
// over our virtio-net Phy (M0 step 3D-2 / 3D-3 / 3D-4). 3D-1 built
// the phy::Device adapter; this module constructs the Interface on
// top, adds a DHCPv4 socket, and spawns a poll task that drives the
// stack cooperatively. Once DHCP hands us a lease:
//
//   - 3D-3 opens a plain TCP connection to slirp gateway 10.0.2.2:12345
//     and emits ARSENAL_TCP_OK on Established.
//   - 3D-4 opens a second TCP connection to 10.0.2.2:12346, wraps it
//     in rustls's UnbufferedClientConnection (the no_std API), drives
//     a TLS 1.3 handshake against a self-signed Python ssl listener
//     stood up by ci/qemu-smoke.sh, and emits ARSENAL_TLS_OK on
//     handshake completion (WriteTraffic state).
//
// Crypto provider: rustls-rustcrypto (pure Rust, no_std-native). The
// HANDOFF's rationale: ring's no_std story is incomplete; aws-lc-rs
// needs libcrypto-sys; rolling primitives by hand from RustCrypto's
// individual crates is more surface than this one provider crate.
// rustls-rustcrypto 0.0.2-alpha is pre-release; the smoke is what we
// pay for that.
//
// Cert verification: a custom ServerCertVerifier that accepts any
// certificate. The smoke target is a self-signed cert and the smoke's
// goal is "TLS 1.3 protocol completes," not "PKI validates." Real
// trust roots and OCSP land in M1+ when we have real network targets
// to validate against.
//
// Time: smoltcp wants a monotonic Instant. We read TSC and divide by
// a coarse constant to get microseconds. The rate is wildly approximate
// — QEMU TCG TSC ticks at host CPU speed, real CPUs vary — but slirp
// responds within a single poll cycle, so smoltcp's retransmit logic
// never fires at this timescale. 3F's APIC timer replaces this with a
// calibrated clock.
//
// MAC: locally-administered fixed value. virtio-net advertises
// VIRTIO_NET_F_MAC and we could negotiate the bit to read QEMU's
// assigned MAC, but slirp routes by whatever we put in the source MAC
// field — a fixed identity simplifies the bring-up. Negotiating F_MAC
// becomes interesting when we add other backends or run against real
// switches in M1.

use core::fmt::Write;

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::unbuffered::{
    ConnectionState, EncodeError, InsufficientSizeError, UnbufferedStatus,
};
use rustls::time_provider::TimeProvider;
use rustls::{
    ClientConfig, DigitallySignedStruct, SignatureScheme,
    client::UnbufferedClientConnection,
};
use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet};
use smoltcp::socket::{dhcpv4, tcp};
use smoltcp::time::Instant;
use smoltcp::wire::{
    EthernetAddress, HardwareAddress, IpAddress, IpCidr, IpEndpoint, Ipv4Address, Ipv4Cidr,
};

use spin::Mutex;

use crate::sched;
use crate::serial;
use crate::virtio_net::VirtioNet;

const MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x4a, 0x52, 0x53];

/// 3D-3 TCP smoke target. Slirp NATs guest → 10.0.2.2:N to the host's
/// 127.0.0.1:N, where ci/qemu-smoke.sh stands up a Python listener
/// before launching QEMU. The host listener accepts and holds the
/// connection open; the kernel observes Established and emits
/// ARSENAL_TCP_OK. Port and address are mirrored in ci/qemu-smoke.sh
/// (TCP_SMOKE_PORT).
const TCP_SMOKE_HOST: Ipv4Address = Ipv4Address::new(10, 0, 2, 2);
const TCP_SMOKE_PORT: u16 = 12345;
const TCP_LOCAL_PORT: u16 = 49152;
const TCP_BUF_BYTES: usize = 4096;

/// 3D-4 TLS smoke target. Same slirp NAT path; a second host listener
/// wraps incoming connections in a self-signed TLS 1.3 server. Port
/// 12346 keeps the TCP and TLS probes independent so their sentinels
/// fire from disjoint sockets and any regressions bisect cleanly.
const TLS_SMOKE_PORT: u16 = 12346;
const TLS_LOCAL_PORT: u16 = 49153;
const TCP_TLS_BUF_BYTES: usize = 16 * 1024;
const TLS_INCOMING_BUF_BYTES: usize = 16 * 1024;
const TLS_OUTGOING_INITIAL: usize = 4096;

struct TlsState {
    conn: UnbufferedClientConnection,
    incoming: Vec<u8>,
    outgoing: Vec<u8>,
    handshake_done: bool,
}

struct NetStack {
    iface: Interface,
    device: VirtioNet,
    sockets: SocketSet<'static>,
    dhcp_handle: SocketHandle,
    configured: bool,
    tcp_handle: Option<SocketHandle>,
    tcp_ok: bool,
    tls_tcp_handle: Option<SocketHandle>,
    tls: Option<TlsState>,
    tls_ok: bool,
}

/// Permissive ServerCertVerifier: accept every certificate and every
/// signature. The smoke target is a self-signed cert generated at
/// smoke-run time; the smoke validates the TLS 1.3 protocol path, not
/// PKI. Real trust roots arrive when M1's userland HTTP client needs
/// them.
#[derive(Debug)]
struct NoopServerVerifier;

impl ServerCertVerifier for NoopServerVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        // Match what rustls-rustcrypto's provider lists. The provider's
        // signature_verification_algorithms determines which schemes
        // the handshake will actually negotiate; this method is
        // consulted by rustls before invoking the verifier so the
        // common set must be a superset.
        alloc::vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

/// TimeProvider that returns a static plausible "now." rustls's
/// default DefaultTimeProvider is gated on the `std` feature; in
/// no_std we provide our own. Our NoopServerVerifier ignores time
/// for cert validity; rustls may consult time elsewhere (session
/// ticket age, etc.), and any monotonic-ish value works. The TSC is
/// available — but converting it to UNIX time requires a real clock
/// origin, which we don't have until M1's RTC. A static lie suffices
/// for the smoke.
#[derive(Debug)]
struct StaticTimeProvider;

impl TimeProvider for StaticTimeProvider {
    fn current_time(&self) -> Option<UnixTime> {
        // 2026-05-11 ≈ this point in development calendar time.
        // Far enough from any cert's notBefore that handshake-time
        // checks (which we noop anyway) wouldn't reject.
        Some(UnixTime::since_unix_epoch(core::time::Duration::from_secs(
            1_778_976_000,
        )))
    }
}

fn build_tls_config() -> Arc<ClientConfig> {
    let provider = Arc::new(rustls_rustcrypto::provider());
    let time_provider: Arc<dyn TimeProvider> = Arc::new(StaticTimeProvider);
    let config = ClientConfig::builder_with_details(provider, time_provider)
        .with_safe_default_protocol_versions()
        .expect("tls: protocol-version selection failed (provider/version mismatch)")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoopServerVerifier))
        .with_no_client_auth();
    Arc::new(config)
}

static NET: Mutex<Option<NetStack>> = Mutex::new(None);

/// Monotonic timestamp for smoltcp. TSC-based; coarse but ordered.
/// 3F's APIC-timer clock replaces this with calibrated nanoseconds.
fn clock_now() -> Instant {
    // SAFETY: rdtsc is unconditionally available on x86_64 and has
    // no side effects beyond reading the TSC. CR4.TSD could trap it
    // for CPL > 0; we run in ring 0 where the bit doesn't apply.
    let tsc = unsafe { core::arch::x86_64::_rdtsc() };
    // Divide by ~3000 to land in microseconds for a typical 3 GHz
    // host. The constant doesn't need to be right — smoltcp only
    // requires monotonicity over the smoke window, and DHCP's
    // retransmit timeouts (seconds) are far above any plausible
    // misclibration here.
    Instant::from_micros((tsc / 3000) as i64)
}

/// Bring up the network stack on top of an initialized virtio-net.
/// Builds an Interface, installs a DHCPv4 socket, and stores the
/// stack into the NET singleton. Idempotent: a second call replaces
/// any prior stack.
pub fn init(mut device: VirtioNet) {
    let mac = EthernetAddress(MAC);
    let config = Config::new(HardwareAddress::Ethernet(mac));
    let iface = Interface::new(config, &mut device, clock_now());

    let mut sockets: SocketSet<'static> = SocketSet::new(Vec::new());
    let dhcp = dhcpv4::Socket::new();
    let dhcp_handle = sockets.add(dhcp);

    let _ = writeln!(
        serial::Writer,
        "net: smoltcp Interface up mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        MAC[0], MAC[1], MAC[2], MAC[3], MAC[4], MAC[5]
    );

    *NET.lock() = Some(NetStack {
        iface,
        device,
        sockets,
        dhcp_handle,
        configured: false,
        tcp_handle: None,
        tcp_ok: false,
        tls_tcp_handle: None,
        tls: None,
        tls_ok: false,
    });
}

/// Allocate a TCP socket, add it to the SocketSet, and initiate a
/// connect to the smoke target. Called once, after DHCP has applied
/// an IP and default route. Stores the handle into stack.tcp_handle;
/// poll_loop watches state() and emits ARSENAL_TCP_OK on Established.
fn start_tcp_smoke(stack: &mut NetStack) {
    let rx_buf = tcp::SocketBuffer::new(vec![0u8; TCP_BUF_BYTES]);
    let tx_buf = tcp::SocketBuffer::new(vec![0u8; TCP_BUF_BYTES]);
    let mut socket = tcp::Socket::new(rx_buf, tx_buf);

    let remote = IpEndpoint::new(IpAddress::Ipv4(TCP_SMOKE_HOST), TCP_SMOKE_PORT);
    let cx = stack.iface.context();
    if let Err(e) = socket.connect(cx, remote, TCP_LOCAL_PORT) {
        let _ = writeln!(serial::Writer, "net: TCP connect refused: {e:?}");
        return;
    }

    let handle = stack.sockets.add(socket);
    stack.tcp_handle = Some(handle);
    let _ = writeln!(
        serial::Writer,
        "net: TCP connect -> {TCP_SMOKE_HOST}:{TCP_SMOKE_PORT}"
    );
}

/// Allocate a second TCP socket for the TLS target, initiate the
/// connect, and stage a rustls UnbufferedClientConnection. The actual
/// handshake is driven later in pump_tls once the TCP socket reaches
/// Established. ServerName uses the literal "arsenal.smoke" — the
/// NoopServerVerifier ignores it, but rustls still requires a value.
fn start_tls_smoke(stack: &mut NetStack) {
    let rx_buf = tcp::SocketBuffer::new(vec![0u8; TCP_TLS_BUF_BYTES]);
    let tx_buf = tcp::SocketBuffer::new(vec![0u8; TCP_TLS_BUF_BYTES]);
    let mut socket = tcp::Socket::new(rx_buf, tx_buf);

    let remote = IpEndpoint::new(IpAddress::Ipv4(TCP_SMOKE_HOST), TLS_SMOKE_PORT);
    let cx = stack.iface.context();
    if let Err(e) = socket.connect(cx, remote, TLS_LOCAL_PORT) {
        let _ = writeln!(serial::Writer, "net: TLS-TCP connect refused: {e:?}");
        return;
    }
    let tcp_handle = stack.sockets.add(socket);
    stack.tls_tcp_handle = Some(tcp_handle);

    let config = build_tls_config();
    let server_name = ServerName::try_from("arsenal.smoke")
        .expect("tls: ServerName::try_from on literal");
    let conn = match UnbufferedClientConnection::new(config, server_name) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(serial::Writer, "net: TLS client construct failed: {e:?}");
            return;
        }
    };

    stack.tls = Some(TlsState {
        conn,
        incoming: Vec::with_capacity(TLS_INCOMING_BUF_BYTES),
        outgoing: Vec::with_capacity(TLS_OUTGOING_INITIAL),
        handshake_done: false,
    });

    let _ = writeln!(
        serial::Writer,
        "net: TLS connect -> {TCP_SMOKE_HOST}:{TLS_SMOKE_PORT}"
    );
}

/// Drive the rustls UnbufferedClientConnection state machine one step
/// at a time. Returns true once the handshake reaches WriteTraffic
/// (handshake complete). Stops on BlockedHandshake (needs more bytes
/// from peer; caller drains TCP and retries on the next poll).
fn drive_rustls(tls: &mut TlsState) -> bool {
    loop {
        let UnbufferedStatus { discard, state } =
            tls.conn.process_tls_records(&mut tls.incoming);
        let state = match state {
            Ok(s) => s,
            Err(e) => {
                let _ = writeln!(serial::Writer, "tls: process_tls_records error: {e:?}");
                return false;
            }
        };

        let mut stop = false;
        match state {
            ConnectionState::EncodeTlsData(mut s) => {
                // Grow outgoing buffer as needed; rustls reports
                // InsufficientSize with the exact required_size.
                let start = tls.outgoing.len();
                let mut try_size = TLS_OUTGOING_INITIAL.max(2048);
                loop {
                    tls.outgoing.resize(start + try_size, 0);
                    match s.encode(&mut tls.outgoing[start..]) {
                        Ok(n) => {
                            tls.outgoing.truncate(start + n);
                            break;
                        }
                        Err(EncodeError::InsufficientSize(InsufficientSizeError {
                            required_size,
                        })) => {
                            try_size = required_size;
                            // Loop: resize larger and retry.
                        }
                        Err(e) => {
                            let _ = writeln!(serial::Writer, "tls: encode error: {e:?}");
                            tls.outgoing.truncate(start);
                            stop = true;
                            break;
                        }
                    }
                }
            }
            ConnectionState::TransmitTlsData(s) => {
                // Bytes are already encoded into tls.outgoing by a
                // prior EncodeTlsData step; the actual TX over the
                // wire happens in pump_tls. Mark this step done so
                // rustls advances its state machine.
                s.done();
            }
            ConnectionState::BlockedHandshake => {
                // Need more bytes from peer. Caller will drain TCP
                // and re-enter on the next poll.
                stop = true;
            }
            ConnectionState::WriteTraffic(_) => {
                // Handshake complete.
                tls.handshake_done = true;
                stop = true;
            }
            ConnectionState::Closed | ConnectionState::PeerClosed => {
                let _ = writeln!(serial::Writer, "tls: connection closed mid-handshake");
                stop = true;
            }
            ConnectionState::ReadTraffic(_) | ConnectionState::ReadEarlyData(_) => {
                // Server-data states; shouldn't appear pre-handshake
                // for a client. Advance and continue.
            }
            _ => {
                // Non-exhaustive variants exist for forwards-compat.
                stop = true;
            }
        }

        if discard > 0 {
            tls.incoming.drain(..discard);
        }

        if stop {
            return tls.handshake_done;
        }
    }
}

/// Pump bytes between the TLS-bearing TCP socket and the rustls
/// Connection. Drains TCP recv → tls.incoming, drives rustls one
/// step, flushes tls.outgoing → TCP send. Called from poll_loop on
/// every iteration once the TCP socket is Established.
fn pump_tls(stack: &mut NetStack) {
    let Some(tls_handle) = stack.tls_tcp_handle else { return };
    let Some(tls) = stack.tls.as_mut() else { return };
    if tls.handshake_done {
        return;
    }

    // Phase 1: drain TCP recv into tls.incoming, flush tls.outgoing
    // into TCP send. Both touch the same TCP socket and so are inside
    // one borrow scope.
    {
        let socket = stack.sockets.get_mut::<tcp::Socket>(tls_handle);
        if socket.state() != tcp::State::Established {
            return;
        }
        while socket.can_recv() {
            let chunk = socket.recv(|buf| {
                let len = buf.len();
                (len, buf.to_vec())
            });
            match chunk {
                Ok(c) if !c.is_empty() => tls.incoming.extend_from_slice(&c),
                _ => break,
            }
        }
        while !tls.outgoing.is_empty() && socket.can_send() {
            let sent = socket.send_slice(&tls.outgoing).unwrap_or(0);
            if sent == 0 {
                break;
            }
            tls.outgoing.drain(..sent);
        }
    }

    // Phase 2: drive rustls (no smoltcp borrows held).
    let just_done = drive_rustls(tls);

    // Phase 3: flush any newly-encoded outgoing bytes into TCP.
    {
        let socket = stack.sockets.get_mut::<tcp::Socket>(tls_handle);
        while !tls.outgoing.is_empty() && socket.can_send() {
            let sent = socket.send_slice(&tls.outgoing).unwrap_or(0);
            if sent == 0 {
                break;
            }
            tls.outgoing.drain(..sent);
        }
    }

    if just_done && !stack.tls_ok {
        let _ = writeln!(serial::Writer, "net: TLS 1.3 handshake complete");
        serial::write_str("ARSENAL_TLS_OK\n");
        stack.tls_ok = true;
        // Drop the TLS socket from the SocketSet. smoltcp 0.12's
        // seq_to_transmit unwraps self.tuple unconditionally, which
        // panics when the socket transitions to a Closed state with
        // tuple=None (which happens once the peer's close_notify +
        // TCP FIN arrive after the handshake settles). The smoke
        // goal is met at this point; removing the socket stops
        // smoltcp from re-polling it.
        if let Some(handle) = stack.tls_tcp_handle.take() {
            stack.sockets.remove(handle);
        }
        stack.tls = None;
    }
}


/// Outcome of one DHCP socket poll, carried as owned data so the
/// match arm doesn't hold a borrow into stack.sockets while we mutate
/// stack.iface.
enum DhcpAction {
    Configure {
        ip: Ipv4Cidr,
        router: Option<Ipv4Address>,
    },
    Deconfigure,
}

/// Spawn entry: forever-poll the network stack, applying DHCP leases
/// to the interface as they arrive. Yields between iterations.
pub fn poll_loop() -> ! {
    loop {
        {
            let mut guard = NET.lock();
            let stack = guard.as_mut().expect("net: poll_loop before init");

            let now = clock_now();
            stack
                .iface
                .poll(now, &mut stack.device, &mut stack.sockets);

            // 3D-3: once a TCP probe is in flight, watch for the
            // socket reaching Established and emit ARSENAL_TCP_OK
            // exactly once. State transitions strictly follow the TCP
            // FSM, so even if the remote closes immediately (FIN
            // arriving in the same poll cycle as SYN-ACK), smoltcp
            // routes through Established before CloseWait — we'll
            // observe it on the poll that processed the SYN-ACK.
            if let Some(handle) = stack.tcp_handle
                && !stack.tcp_ok
            {
                let tcp_sock = stack.sockets.get::<tcp::Socket>(handle);
                if tcp_sock.state() == tcp::State::Established {
                    let _ = writeln!(
                        serial::Writer,
                        "net: TCP established local={:?} remote={:?}",
                        tcp_sock.local_endpoint(),
                        tcp_sock.remote_endpoint()
                    );
                    serial::write_str("ARSENAL_TCP_OK\n");
                    stack.tcp_ok = true;
                }
            }

            // 3D-4: pump TLS bytes between smoltcp's TCP socket and
            // rustls. pump_tls is a no-op until the TLS-TCP socket
            // reaches Established; once it does, it advances the
            // rustls state machine and emits ARSENAL_TLS_OK on
            // handshake completion.
            pump_tls(stack);

            let action = {
                let dhcp = stack
                    .sockets
                    .get_mut::<dhcpv4::Socket>(stack.dhcp_handle);
                match dhcp.poll() {
                    None => None,
                    Some(dhcpv4::Event::Configured(cfg)) => Some(DhcpAction::Configure {
                        ip: cfg.address,
                        router: cfg.router,
                    }),
                    Some(dhcpv4::Event::Deconfigured) => Some(DhcpAction::Deconfigure),
                }
            };

            match action {
                None => {}
                Some(DhcpAction::Configure { ip, router }) => {
                    stack.iface.update_ip_addrs(|addrs| {
                        addrs.clear();
                        let _ = addrs.push(IpCidr::Ipv4(ip));
                    });
                    if let Some(gw) = router {
                        let _ = stack.iface.routes_mut().add_default_ipv4_route(gw);
                    }
                    if !stack.configured {
                        let _ = writeln!(
                            serial::Writer,
                            "net: DHCPv4 lease ip={} gw={:?}",
                            ip, router
                        );
                        stack.configured = true;
                        // 3D-3: once we have a default route, fire
                        // the TCP probe. start_tcp_smoke is one-shot;
                        // a re-lease (e.g. after Deconfigure) won't
                        // re-probe.
                        if stack.tcp_handle.is_none() {
                            start_tcp_smoke(stack);
                        }
                        // 3D-4: same path, second socket on TLS port,
                        // wrapped in a rustls UnbufferedClientConnection.
                        // The handshake itself is driven from pump_tls.
                        if stack.tls_tcp_handle.is_none() {
                            start_tls_smoke(stack);
                        }
                    }
                }
                Some(DhcpAction::Deconfigure) => {
                    stack.iface.update_ip_addrs(|addrs| addrs.clear());
                    stack.iface.routes_mut().remove_default_ipv4_route();
                    if stack.configured {
                        let _ = writeln!(serial::Writer, "net: DHCPv4 lease lost");
                        stack.configured = false;
                    }
                }
            }
        }
        sched::yield_now();
    }
}
