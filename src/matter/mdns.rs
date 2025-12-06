//! Custom mDNS responder that registers services on a specific network interface.
//!
//! The default rs-matter AvahiMdnsResponder uses interface=-1 (all interfaces) and
//! empty hostname, which causes Avahi to advertise addresses from all interfaces
//! including Thread mesh addresses visible via mDNS reflection.
//!
//! This implementation:
//! 1. Specifies a specific interface index when registering services
//! 2. Gets the actual IP addresses from that interface
//! 3. Explicitly registers A/AAAA records for the hostname with those addresses
//!
//! This ensures the mDNS advertisement contains only addresses from the intended interface.

use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::io::Write as _;
use std::net::{Ipv4Addr, Ipv6Addr};

use nix::ifaddrs::getifaddrs;
use nix::net::if_::if_nametoindex;
use nix::sys::socket::{AddressFamily, SockaddrLike};
use rs_matter::error::Error;
use rs_matter::transport::network::mdns::Service;
use rs_matter::utils::zbus::Connection;
use rs_matter::utils::zbus::zvariant::{ObjectPath, OwnedObjectPath};
use rs_matter::utils::zbus_proxies::avahi::entry_group::EntryGroupProxy;
use rs_matter::utils::zbus_proxies::avahi::server2::Server2Proxy;
use rs_matter::{MATTER_SERVICE_MAX_NAME_LEN, Matter, MatterMdnsService};

/// An mDNS responder that registers services on a specific network interface.
///
/// Unlike the default `AvahiMdnsResponder` which uses interface=-1 (all interfaces),
/// this responder:
/// - Uses a specific interface index
/// - Explicitly registers address records (A/AAAA) for the service hostname
/// - Ensures only addresses from the specified interface are advertised
pub struct FilteredAvahiMdnsResponder<'a> {
    matter: &'a Matter<'a>,
    interface_name: &'static str,
    services: HashMap<MatterMdnsService, OwnedObjectPath>,
}

impl<'a> FilteredAvahiMdnsResponder<'a> {
    /// Create a new filtered mDNS responder.
    ///
    /// # Arguments
    /// - `matter`: Reference to the Matter instance
    /// - `interface_name`: The network interface name to advertise on (e.g., "enp14s0", "eth0")
    pub fn new(matter: &'a Matter<'a>, interface_name: &'static str) -> Self {
        Self {
            matter,
            interface_name,
            services: HashMap::new(),
        }
    }

    /// Get the interface index for the configured interface name.
    fn get_interface_index(&self) -> i32 {
        let Ok(cname) = CString::new(self.interface_name) else {
            log::warn!(
                "FilteredAvahiMdnsResponder: invalid interface name '{}', using all interfaces",
                self.interface_name
            );
            return -1;
        };

        match if_nametoindex(cname.as_c_str()) {
            Ok(idx) => {
                log::info!(
                    "FilteredAvahiMdnsResponder: using interface '{}' (index {})",
                    self.interface_name,
                    idx
                );
                idx as i32
            }
            Err(e) => {
                log::warn!(
                    "FilteredAvahiMdnsResponder: failed to get index for '{}': {}, using all interfaces",
                    self.interface_name,
                    e
                );
                -1
            }
        }
    }

    /// Get IP addresses from the configured interface.
    fn get_interface_addresses(&self) -> (Vec<Ipv4Addr>, Vec<Ipv6Addr>) {
        let mut ipv4_addrs = Vec::new();
        let mut ipv6_addrs = Vec::new();

        let Ok(addrs) = getifaddrs() else {
            log::warn!("FilteredAvahiMdnsResponder: failed to get interface addresses");
            return (ipv4_addrs, ipv6_addrs);
        };

        for ifaddr in addrs {
            if ifaddr.interface_name != self.interface_name {
                continue;
            }

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
                            // Skip link-local addresses (fe80::/10)
                            let octets = ip.octets();
                            if octets[0] != 0xfe || (octets[1] & 0xc0) != 0x80 {
                                ipv6_addrs.push(ip);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        log::info!(
            "FilteredAvahiMdnsResponder: found {} IPv4 and {} IPv6 addresses on '{}'",
            ipv4_addrs.len(),
            ipv6_addrs.len(),
            self.interface_name
        );
        for addr in &ipv4_addrs {
            log::info!("  IPv4: {}", addr);
        }
        for addr in &ipv6_addrs {
            log::info!("  IPv6: {}", addr);
        }

        (ipv4_addrs, ipv6_addrs)
    }

    /// Run the mDNS responder.
    ///
    /// # Arguments
    /// - `connection`: A reference to the D-Bus system connection.
    pub async fn run(&mut self, connection: &Connection) -> Result<(), Error> {
        {
            let avahi = Server2Proxy::new(connection).await?;
            log::info!("Avahi API version: {}", avahi.get_apiversion().await?);
        }

        loop {
            self.matter.wait_mdns().await;

            let mut services = HashSet::new();
            self.matter.mdns_services(|service| {
                services.insert(service);
                Ok(())
            })?;

            log::info!("mDNS services changed, updating...");

            self.update_services(connection, &services).await?;

            log::info!("mDNS services updated");
        }
    }

    async fn update_services(
        &mut self,
        connection: &Connection,
        services: &HashSet<MatterMdnsService>,
    ) -> Result<(), Error> {
        for service in services {
            if !self.services.contains_key(service) {
                log::info!("Registering mDNS service: {:?}", service);
                let path = self.register(connection, service).await?;
                self.services.insert(service.clone(), path);
            }
        }

        loop {
            let removed = self
                .services
                .iter()
                .find(|(service, _)| !services.contains(service));

            if let Some((service, path)) = removed {
                log::info!("Deregistering mDNS service: {:?}", service);
                Self::deregister(connection, path.as_ref()).await?;
                self.services.remove(&service.clone());
            } else {
                break;
            }
        }

        Ok(())
    }

    async fn register(
        &mut self,
        connection: &Connection,
        matter_service: &MatterMdnsService,
    ) -> Result<OwnedObjectPath, Error> {
        let interface_index = self.get_interface_index();
        let (ipv4_addrs, ipv6_addrs) = self.get_interface_addresses();

        // Get the service instance name (e.g., "E601FEB08A46D58F")
        let mut name_buf = [0u8; MATTER_SERVICE_MAX_NAME_LEN];
        let instance_name = matter_service.name(&mut name_buf).to_string();

        // Create a unique hostname for this Matter device
        // Use the instance name to avoid collisions
        let hostname = format!("{}.local", instance_name);

        log::info!(
            "Registering mDNS service with hostname '{}' on interface {} with {} IPv4 and {} IPv6 addresses",
            hostname,
            interface_index,
            ipv4_addrs.len(),
            ipv6_addrs.len()
        );

        Service::async_call_with(
            matter_service,
            self.matter.dev_det(),
            self.matter.port(),
            async |service| {
                let avahi = Server2Proxy::new(connection).await?;

                let path = avahi.entry_group_new().await?;

                let group = EntryGroupProxy::builder(connection)
                    .path(path.clone())?
                    .build()
                    .await?;

                // First, register explicit address records for our hostname
                // This ensures Avahi uses OUR addresses, not cached Thread addresses
                for ipv4 in &ipv4_addrs {
                    log::info!("  Adding A record: {} -> {}", hostname, ipv4);
                    group
                        .add_address(
                            interface_index,
                            0, // AVAHI_PROTO_INET (IPv4 only for A record)
                            0, // flags
                            &hostname,
                            &ipv4.to_string(),
                        )
                        .await?;
                }

                for ipv6 in &ipv6_addrs {
                    log::info!("  Adding AAAA record: {} -> {}", hostname, ipv6);
                    group
                        .add_address(
                            interface_index,
                            1, // AVAHI_PROTO_INET6 (IPv6 only for AAAA record)
                            0, // flags
                            &hostname,
                            &ipv6.to_string(),
                        )
                        .await?;
                }

                // Now register the service with our explicit hostname
                let mut txt_buf = Vec::new();

                let offsets = service
                    .txt_kvs
                    .iter()
                    .map(|(k, v)| {
                        let start = txt_buf.len();

                        if v.is_empty() {
                            txt_buf.extend_from_slice(k.as_bytes());
                        } else {
                            write!(&mut txt_buf, "{}={}", k, v).unwrap();
                        }

                        txt_buf.len() - start
                    })
                    .collect::<Vec<_>>();

                let mut txt_slice = txt_buf.as_slice();
                let mut txt = Vec::new();

                for offset in offsets {
                    let (entry, next_slice) = txt_slice.split_at(offset);
                    txt.push(entry);
                    txt_slice = next_slice;
                }

                // Use specific interface index and our explicit hostname
                group
                    .add_service(
                        interface_index,
                        -1, // protocol: -1 = both IPv4 and IPv6
                        0,
                        service.name,
                        service.service_protocol,
                        "",        // domain (empty = default .local)
                        &hostname, // Use our explicit hostname instead of empty
                        service.port,
                        &txt,
                    )
                    .await?;

                for subtype in service.service_subtypes {
                    let avahi_subtype = format!("{}._sub.{}", subtype, service.service_protocol);

                    group
                        .add_service_subtype(
                            interface_index,
                            -1,
                            0,
                            service.name,
                            service.service_protocol,
                            "",
                            &avahi_subtype,
                        )
                        .await?;
                }

                group.commit().await?;

                log::info!("mDNS service registered successfully");

                Ok(path)
            },
        )
        .await
    }

    async fn deregister(connection: &Connection, path: ObjectPath<'_>) -> Result<(), Error> {
        let group = EntryGroupProxy::builder(connection)
            .path(path)?
            .build()
            .await?;

        group.free().await?;

        Ok(())
    }
}
