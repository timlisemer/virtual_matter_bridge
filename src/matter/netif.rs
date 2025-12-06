//! Custom network interface implementation that filters to a specific interface.
//!
//! This is needed because the default UnixNetifs implementation returns all interfaces,
//! including Thread mesh addresses that may be visible via mDNS reflection but don't
//! belong to this host.

use std::ffi::CString;
use std::net::{Ipv4Addr, Ipv6Addr};

use nix::ifaddrs::getifaddrs;
use nix::net::if_::{InterfaceFlags, if_nametoindex};
use nix::sys::socket::{AddressFamily, SockaddrLike};

use rs_matter::dm::clusters::gen_diag::{InterfaceTypeEnum, NetifDiag, NetifInfo};
use rs_matter::dm::networks::NetChangeNotif;
use rs_matter::error::Error;

/// A network interface implementation that only returns addresses from a specific interface.
///
/// This solves the problem where UnixNetifs returns Thread mesh addresses that are
/// visible via mDNS reflection but don't belong to this host.
#[derive(Clone, Copy)]
pub struct FilteredNetifs {
    /// The interface name to filter to (e.g., "enp14s0", "eth0")
    interface_name: &'static str,
}

impl FilteredNetifs {
    /// Create a new FilteredNetifs that only reports addresses from the given interface.
    pub const fn new(interface_name: &'static str) -> Self {
        Self { interface_name }
    }
}

impl NetifDiag for FilteredNetifs {
    fn netifs(&self, f: &mut dyn FnMut(&NetifInfo) -> Result<(), Error>) -> Result<(), Error> {
        // Get all interfaces from the system
        let Ok(addrs) = getifaddrs() else {
            return Ok(());
        };

        let mut ipv4_addrs: Vec<Ipv4Addr> = Vec::new();
        let mut ipv6_addrs: Vec<Ipv6Addr> = Vec::new();
        let mut hw_addr = [0u8; 8];
        let mut operational = false;
        let mut found = false;
        let mut netif_index = 0u32;

        for ifaddr in addrs {
            let name = &ifaddr.interface_name;
            if name != self.interface_name {
                continue;
            }

            found = true;

            // Get interface index
            if netif_index == 0
                && let Ok(cname) = CString::new(name.as_str())
                && let Ok(idx) = if_nametoindex(cname.as_c_str())
            {
                netif_index = idx;
            }

            // Check operational status
            if ifaddr.flags.contains(InterfaceFlags::IFF_RUNNING) {
                operational = true;
            }

            // Extract addresses
            if let Some(addr) = ifaddr.address
                && let Some(family) = addr.family()
            {
                match family {
                    AddressFamily::Inet => {
                        if let Some(sockaddr) = addr.as_sockaddr_in() {
                            ipv4_addrs.push(sockaddr.ip());
                        }
                    }
                    AddressFamily::Inet6 => {
                        if let Some(sockaddr) = addr.as_sockaddr_in6() {
                            let ip = sockaddr.ip();
                            // Skip link-local addresses (fe80::/10) as they're not useful for Matter
                            let octets = ip.octets();
                            if octets[0] != 0xfe || (octets[1] & 0xc0) != 0x80 {
                                ipv6_addrs.push(ip);
                            }
                        }
                    }
                    AddressFamily::Packet => {
                        if let Some(link_addr) = addr.as_link_addr()
                            && let Some(mac) = link_addr.addr()
                        {
                            let len = mac.len().min(8);
                            hw_addr[..len].copy_from_slice(&mac[..len]);
                        }
                    }
                    _ => {}
                }
            }
        }

        if !found {
            log::warn!(
                "FilteredNetifs: interface '{}' not found",
                self.interface_name
            );
            return Ok(());
        }

        log::info!(
            "FilteredNetifs: using interface '{}' with {} IPv4 and {} IPv6 addresses",
            self.interface_name,
            ipv4_addrs.len(),
            ipv6_addrs.len()
        );
        for addr in &ipv4_addrs {
            log::info!("  IPv4: {}", addr);
        }
        for addr in &ipv6_addrs {
            log::info!("  IPv6: {}", addr);
        }

        let info = NetifInfo {
            name: self.interface_name,
            operational,
            offprem_svc_reachable_ipv4: None,
            offprem_svc_reachable_ipv6: None,
            hw_addr: &hw_addr,
            ipv4_addrs: &ipv4_addrs,
            ipv6_addrs: &ipv6_addrs,
            netif_type: InterfaceTypeEnum::Ethernet,
            netif_index,
        };

        f(&info)
    }
}

impl NetChangeNotif for FilteredNetifs {
    async fn wait_changed(&self) {
        // Not implemented - just wait forever
        core::future::pending().await
    }
}
