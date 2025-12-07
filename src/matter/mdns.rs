//! Direct mDNS responder using mdns-sd crate.
//!
//! This bypasses Avahi D-Bus which is broken on systems with complex networking
//! (Docker, multiple interfaces, etc.). Instead, we use the mdns-sd crate which
//! handles multicast directly.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

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
    /// Maps MatterMdnsService to the main service fullname for deregistration.
    /// Note: In mdns-sd, subtypes share the same fullname as the main service,
    /// so we only need to unregister once to clean up everything.
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
    ///
    /// IMPORTANT: This must run in the same async executor as the Matter stack
    /// to avoid RefCell borrow conflicts. rs-matter uses RefCell for internal state,
    /// which is not thread-safe.
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

        // Drop link-local addresses for this interface to avoid mdns-sd errors
        // when it attempts AAAA responses on fe80:: entries.
        for ll in get_link_local_ipv6_addresses(self.interface_name) {
            if let Err(e) = daemon.disable_interface(IfKind::Addr(IpAddr::V6(ll))) {
                log::warn!(
                    "Failed to disable link-local IPv6 {} on '{}': {:?}",
                    ll,
                    self.interface_name,
                    e
                );
            } else {
                log::info!(
                    "Disabled link-local IPv6 {} on interface '{}'",
                    ll,
                    self.interface_name
                );
            }
        }

        log::info!(
            "Direct mDNS responder initialized on interface '{}'",
            self.interface_name
        );

        self.daemon = Some(daemon);

        loop {
            self.matter.wait_mdns().await;

            // Direct call to mdns_services - safe because we're in the same
            // async executor as the Matter stack, so RefCell borrows are
            // properly sequenced
            let mut services = HashSet::new();
            self.matter.mdns_services(|service| {
                // Log discriminator for debugging multi-admin commissioning
                match &service {
                    MatterMdnsService::Commissionable { id, discriminator } => {
                        log::info!(
                            "mDNS service from rs-matter: Commissionable id={:016X} D={}",
                            id,
                            discriminator
                        );
                    }
                    MatterMdnsService::Commissioned {
                        compressed_fabric_id,
                        node_id,
                    } => {
                        log::info!(
                            "mDNS service from rs-matter: Commissioned fabric={:016X} node={:016X}",
                            compressed_fabric_id,
                            node_id
                        );
                    }
                }
                services.insert(service);
                Ok(())
            })?;

            log::info!(
                "mDNS services changed: {} current services, {} registered",
                services.len(),
                self.registered.len()
            );

            self.update_services(&services).await?;

            log::info!("mDNS services update complete");
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
                // Log discriminator for new registrations
                match service {
                    MatterMdnsService::Commissionable { discriminator, .. } => {
                        log::info!(
                            "Registering NEW mDNS service: Commissionable D={}",
                            discriminator
                        );
                    }
                    MatterMdnsService::Commissioned { .. } => {
                        log::info!("Registering NEW mDNS service: Commissioned (operational)");
                    }
                }
                let full_name = self.register(daemon, service).await?;
                log::info!("Registered service {:?} as '{}'", service, full_name);
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
                // Log discriminator for deregistrations
                match &service {
                    MatterMdnsService::Commissionable { discriminator, .. } => {
                        log::info!(
                            "Deregistering mDNS service: Commissionable D={} ('{}')",
                            discriminator,
                            full_name
                        );
                    }
                    MatterMdnsService::Commissioned { .. } => {
                        log::info!("Deregistering mDNS service: Commissioned ('{}')", full_name);
                    }
                }

                // Unregister the main service (subtypes are automatically cleaned up)
                match daemon.unregister(&full_name) {
                    Ok(receiver) => {
                        // Wait for unregister to complete to avoid "sending on closed channel" errors
                        match receiver.recv() {
                            Ok(status) => {
                                log::info!("Unregistered '{}': {:?}", full_name, status)
                            }
                            Err(e) => {
                                log::warn!(
                                    "Failed to receive unregister status for '{}': {:?}",
                                    full_name,
                                    e
                                )
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to unregister service '{}': {:?}", full_name, e)
                    }
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
                    log::debug!("  IPv4: {}", addr);
                }
                for addr in &ipv6_addrs {
                    log::debug!("  IPv6: {}", addr);
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

                // Register the main service
                daemon.register(service_info).map_err(|e| {
                    log::error!("Failed to register mDNS service: {:?}", e);
                    rs_matter::error::ErrorCode::MdnsError
                })?;

                log::info!(
                    "mDNS main service registered: {} (fullname: {})",
                    service.name,
                    full_name
                );

                // Save the main service fullname for deregistration
                let main_fullname = full_name;

                // Register subtypes (e.g., _L3840 for discriminator lookup)
                // Matter controllers query for discriminator subtypes like:
                // _L3840._sub._matterc._udp.local.
                log::info!(
                    "  Registering {} subtypes: {:?}",
                    service.service_subtypes.len(),
                    service.service_subtypes
                );

                for subtype in service.service_subtypes {
                    let subtype_service_type = format!(
                        "{}._sub.{}.local.",
                        subtype,                  // e.g., "_L3840"
                        service.service_protocol  // e.g., "_matterc._udp"
                    );

                    let subtype_info = ServiceInfo::new(
                        &subtype_service_type,
                        service.name,
                        &host_fqdn,
                        all_addrs.as_slice(),
                        service.port,
                        properties.as_slice(),
                    )
                    .map_err(|e| {
                        log::error!(
                            "Failed to create subtype ServiceInfo for {}: {:?}",
                            subtype,
                            e
                        );
                        rs_matter::error::ErrorCode::MdnsError
                    })?;

                    let subtype_full_name = subtype_info.get_fullname().to_string();

                    daemon.register(subtype_info).map_err(|e| {
                        log::error!("Failed to register mDNS subtype {}: {:?}", subtype, e);
                        rs_matter::error::ErrorCode::MdnsError
                    })?;

                    log::info!(
                        "  Subtype registered: {} (fullname: {})",
                        subtype,
                        subtype_full_name
                    );
                    // Note: We don't track subtype fullnames separately because
                    // mdns-sd returns the same fullname for subtypes as the main service.
                    // Unregistering the main service cleans up all subtypes.
                }

                // Return just the main service fullname for deregistration
                Ok(main_fullname)
            },
        )
        .await
    }
}

/// Get IPv4 and IPv6 addresses for a specific network interface.
///
/// Filters out:
/// - Link-local IPv6 addresses (fe80::/10)
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
                        if !is_link_local_ipv6(&ip) {
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

/// Collect link-local IPv6 addresses (fe80::/10) for an interface.
fn get_link_local_ipv6_addresses(interface_name: &str) -> Vec<Ipv6Addr> {
    let Ok(addrs) = getifaddrs() else {
        return Vec::new();
    };

    let mut ipv6 = Vec::new();
    for ifaddr in addrs {
        if ifaddr.interface_name != interface_name {
            continue;
        }

        if let Some(addr) = ifaddr.address
            && let Some(family) = addr.family()
            && family == AddressFamily::Inet6
            && let Some(sockaddr) = addr.as_sockaddr_in6()
        {
            let ip = sockaddr.ip();
            if is_link_local_ipv6(&ip) {
                ipv6.push(ip);
            }
        }
    }

    ipv6
}

/// Check if an IPv6 address is link-local (fe80::/10)
fn is_link_local_ipv6(addr: &Ipv6Addr) -> bool {
    let octets = addr.octets();
    octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80
}
