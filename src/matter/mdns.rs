//! Direct mDNS responder using mdns-sd crate.
//!
//! This bypasses Avahi D-Bus which is broken on systems with complex networking
//! (Docker, multiple interfaces, etc.). Instead, we use the mdns-sd crate which
//! handles multicast directly.

use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, Ipv6Addr};

use mdns_sd::{IfKind, ServiceDaemon, ServiceInfo};
use nix::ifaddrs::getifaddrs;
use nix::sys::socket::{AddressFamily, SockaddrLike};

use rs_matter::error::Error;
use rs_matter::transport::network::mdns::Service;
use rs_matter::{Matter, MatterMdnsService};

/// A direct mDNS responder that uses the mdns-sd crate instead of Avahi D-Bus.
///
/// This solves the issue where Avahi's D-Bus interface reports success but
/// services never appear in mDNS queries due to broken multicast on systems
/// with complex networking (Docker bridges, veth interfaces, etc.).
pub struct DirectMdnsResponder<'a> {
    matter: &'a Matter<'a>,
    interface_name: &'static str,
    daemon: Option<ServiceDaemon>,
    /// Maps MatterMdnsService to the full service name registered with mdns-sd
    registered: HashMap<MatterMdnsService, String>,
}

impl<'a> DirectMdnsResponder<'a> {
    /// Create a new direct mDNS responder.
    ///
    /// # Arguments
    /// - `matter`: Reference to the Matter instance
    /// - `interface_name`: The network interface to bind to (e.g., "enp14s0")
    pub fn new(matter: &'a Matter<'a>, interface_name: &'static str) -> Self {
        Self {
            matter,
            interface_name,
            daemon: None,
            registered: HashMap::new(),
        }
    }

    /// Run the mDNS responder.
    ///
    /// This will register and deregister mDNS services as the Matter stack
    /// opens/closes commissioning windows.
    pub async fn run(&mut self) -> Result<(), Error> {
        // Create the mDNS daemon
        let daemon = ServiceDaemon::new().map_err(|e| {
            log::error!("Failed to create mDNS daemon: {:?}", e);
            rs_matter::error::ErrorCode::MdnsError
        })?;

        // Disable all interfaces first, then enable only our target interface
        daemon.disable_interface(IfKind::All).map_err(|e| {
            log::error!("Failed to disable all interfaces: {:?}", e);
            rs_matter::error::ErrorCode::MdnsError
        })?;

        daemon.enable_interface(self.interface_name).map_err(|e| {
            log::error!(
                "Failed to enable interface '{}': {:?}",
                self.interface_name,
                e
            );
            rs_matter::error::ErrorCode::MdnsError
        })?;

        log::info!(
            "Direct mDNS responder initialized on interface '{}'",
            self.interface_name
        );

        self.daemon = Some(daemon);

        loop {
            self.matter.wait_mdns().await;

            let mut services = HashSet::new();
            self.matter.mdns_services(|service| {
                services.insert(service);
                Ok(())
            })?;

            log::info!("mDNS services changed, updating...");

            self.update_services(&services).await?;

            log::info!("mDNS services updated");
        }
    }

    async fn update_services(
        &mut self,
        services: &HashSet<MatterMdnsService>,
    ) -> Result<(), Error> {
        let daemon = self
            .daemon
            .as_ref()
            .ok_or(rs_matter::error::ErrorCode::MdnsError)?;

        // Register new services
        for service in services {
            if !self.registered.contains_key(service) {
                log::info!("Registering mDNS service: {:?}", service);
                let full_name = self.register(daemon, service).await?;
                self.registered.insert(service.clone(), full_name);
            }
        }

        // Deregister removed services
        let to_remove: Vec<_> = self
            .registered
            .keys()
            .filter(|s| !services.contains(s))
            .cloned()
            .collect();

        for service in to_remove {
            if let Some(full_name) = self.registered.remove(&service) {
                log::info!("Deregistering mDNS service: {:?}", service);
                if let Err(e) = daemon.unregister(&full_name) {
                    log::warn!("Failed to unregister service '{}': {:?}", full_name, e);
                }
            }
        }

        Ok(())
    }

    async fn register(
        &self,
        daemon: &ServiceDaemon,
        matter_service: &MatterMdnsService,
    ) -> Result<String, Error> {
        Service::async_call_with(
            matter_service,
            self.matter.dev_det(),
            self.matter.port(),
            async |service| {
                // Get addresses for our interface
                let (ipv4_addrs, ipv6_addrs) = get_interface_addresses(self.interface_name)?;

                if ipv4_addrs.is_empty() && ipv6_addrs.is_empty() {
                    log::error!(
                        "No usable addresses found on interface '{}'",
                        self.interface_name
                    );
                    return Err(rs_matter::error::ErrorCode::MdnsError.into());
                }

                log::info!(
                    "Registering '{}' on {} with {} IPv4 and {} IPv6 addresses",
                    service.name,
                    service.service_protocol,
                    ipv4_addrs.len(),
                    ipv6_addrs.len()
                );
                for addr in &ipv4_addrs {
                    log::info!("  IPv4: {}", addr);
                }
                for addr in &ipv6_addrs {
                    log::info!("  IPv6: {}", addr);
                }

                // Build TXT properties
                let properties: Vec<(&str, &str)> =
                    service.txt_kvs.iter().map(|(k, v)| (*k, *v)).collect();

                log::info!("  TXT records:");
                for (k, v) in &properties {
                    log::info!("    {}={}", k, v);
                }

                // Get hostname
                let hostname = gethostname::gethostname();
                let hostname_str = hostname.to_string_lossy();
                let host_fqdn = format!("{}.local.", hostname_str);

                // Service type with domain
                let service_type = format!("{}.local.", service.service_protocol);

                // Create ServiceInfo
                // mdns-sd wants addresses as a slice - combine IPv4 and IPv6
                let all_addrs: Vec<std::net::IpAddr> = ipv4_addrs
                    .iter()
                    .map(|a| std::net::IpAddr::V4(*a))
                    .chain(ipv6_addrs.iter().map(|a| std::net::IpAddr::V6(*a)))
                    .collect();

                let service_info = ServiceInfo::new(
                    &service_type,
                    service.name,
                    &host_fqdn,
                    all_addrs.as_slice(),
                    service.port,
                    properties.as_slice(),
                )
                .map_err(|e| {
                    log::error!("Failed to create ServiceInfo: {:?}", e);
                    rs_matter::error::ErrorCode::MdnsError
                })?;

                // Get the full name for later unregistration
                let full_name = service_info.get_fullname().to_string();

                // Register the service
                daemon.register(service_info).map_err(|e| {
                    log::error!("Failed to register mDNS service: {:?}", e);
                    rs_matter::error::ErrorCode::MdnsError
                })?;

                log::info!(
                    "mDNS service registered: {} (fullname: {})",
                    service.name,
                    full_name
                );

                // Register subtypes
                for subtype in service.service_subtypes {
                    log::info!("  Subtype: {}", subtype);
                    // mdns-sd handles subtypes differently - they're part of the service type
                    // For now, log them. Full subtype support may need ServiceInfo::with_subtype()
                }

                Ok(full_name)
            },
        )
        .await
    }
}

/// Get IPv4 and IPv6 addresses for a specific network interface.
///
/// Filters out:
/// - Link-local IPv6 addresses (fe80::/10)
/// - Thread mesh addresses (fd00::/8 ULAs from Thread network)
fn get_interface_addresses(interface_name: &str) -> Result<(Vec<Ipv4Addr>, Vec<Ipv6Addr>), Error> {
    let addrs = getifaddrs().map_err(|e| {
        log::error!("Failed to get interface addresses: {:?}", e);
        rs_matter::error::ErrorCode::MdnsError
    })?;

    let mut ipv4 = Vec::new();
    let mut ipv6 = Vec::new();

    for ifaddr in addrs {
        if ifaddr.interface_name != interface_name {
            continue;
        }

        if let Some(addr) = ifaddr.address
            && let Some(family) = addr.family()
        {
            match family {
                AddressFamily::Inet => {
                    if let Some(sockaddr) = addr.as_sockaddr_in() {
                        ipv4.push(sockaddr.ip());
                    }
                }
                AddressFamily::Inet6 => {
                    if let Some(sockaddr) = addr.as_sockaddr_in6() {
                        let ip = sockaddr.ip();

                        // Filter out problematic addresses
                        if !is_link_local_ipv6(&ip) && !is_thread_mesh_address(&ip) {
                            ipv6.push(ip);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok((ipv4, ipv6))
}

/// Check if an IPv6 address is link-local (fe80::/10)
fn is_link_local_ipv6(addr: &Ipv6Addr) -> bool {
    let octets = addr.octets();
    octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80
}

/// Check if an IPv6 address is a Thread mesh address (fd00::/8 ULA)
///
/// Thread uses Unique Local Addresses in the fd00::/8 range.
/// These addresses are internal to the Thread mesh and shouldn't be advertised
/// for Matter over IP.
fn is_thread_mesh_address(addr: &Ipv6Addr) -> bool {
    let segments = addr.segments();
    // fd00::/8 - Unique Local Addresses used by Thread
    segments[0] & 0xff00 == 0xfd00
}
