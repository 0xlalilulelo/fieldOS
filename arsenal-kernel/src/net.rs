// SPDX-License-Identifier: BSD-2-Clause
//
// smoltcp Interface + DHCPv4 + TCP smoke over our virtio-net Phy
// (M0 step 3D-2 / 3D-3). 3D-1 built the phy::Device adapter; this
// module constructs the Interface on top, adds a DHCPv4 socket, and
// spawns a poll task that drives the stack cooperatively. Once DHCP
// hands us a lease, 3D-3's TCP probe opens a connection to slirp's
// gateway and emits ARSENAL_TCP_OK on Established. 3D-4 wraps that
// connection in rustls.
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

use alloc::vec;
use alloc::vec::Vec;

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

struct NetStack {
    iface: Interface,
    device: VirtioNet,
    sockets: SocketSet<'static>,
    dhcp_handle: SocketHandle,
    configured: bool,
    tcp_handle: Option<SocketHandle>,
    tcp_ok: bool,
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
