// SPDX-License-Identifier: BSD-2-Clause
//
// smoltcp Interface + DHCPv4 over our virtio-net Phy (M0 step 3D-2).
// 3D-1 built the phy::Device adapter; this module constructs the
// Interface on top, adds a DHCPv4 socket, and spawns a poll task that
// drives the stack cooperatively. 3D-3 adds TCP smoke; 3D-4 wraps
// rustls.
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

use alloc::vec::Vec;

use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet};
use smoltcp::socket::dhcpv4;
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, HardwareAddress, IpCidr, Ipv4Address, Ipv4Cidr};

use spin::Mutex;

use crate::sched;
use crate::serial;
use crate::virtio_net::VirtioNet;

const MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x4a, 0x52, 0x53];

struct NetStack {
    iface: Interface,
    device: VirtioNet,
    sockets: SocketSet<'static>,
    dhcp_handle: SocketHandle,
    configured: bool,
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
    });
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
