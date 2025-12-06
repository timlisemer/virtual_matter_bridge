//! Custom mDNS responder that registers services on a specific network interface.
//!
//! The default rs-matter AvahiMdnsResponder uses interface=-1 (all interfaces),
//! which causes Avahi to advertise addresses from all interfaces including
//! Thread mesh addresses visible via mDNS reflection.
//!
//! This implementation specifies a specific interface index when registering services,
//! which restricts Avahi to only advertise addresses from that interface.

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
use rs_matter::{Matter, MatterMdnsService};

/// An mDNS responder that registers services on a specific network interface.
///
/// Unlike the default `AvahiMdnsResponder` which uses interface=-1 (all interfaces),
/// this responder uses a specific interface index to ensure only addresses from
/// that interface are advertised via mDNS.
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

        // Log the addresses we found (for debugging)
        let _ = self.get_interface_addresses();

        log::info!(
            "Registering mDNS service on interface index {}",
            interface_index
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

                // Register the service with empty hostname - Avahi will use the system hostname
                // but restricted to our interface, so it will only advertise addresses from that interface
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

                // Log what we're registering
                log::info!(
                    "  Service name: '{}', type: '{}', port: {}",
                    service.name,
                    service.service_protocol,
                    service.port
                );
                log::info!("  TXT records: {:?}", service.txt_kvs);
                log::info!("  Subtypes: {:?}", service.service_subtypes);

                // Register the service.
                // We use -1 for interface (all interfaces) and rely on NixOS
                // services.avahi.allowInterfaces = ["enp14s0"] to restrict which
                // interfaces Avahi advertises on.
                // NOTE: Requires services.avahi.publish.userServices = true in NixOS config
                group
                    .add_service(
                        -1, // interface: -1 = all (filtered by allowInterfaces in avahi config)
                        -1, // protocol: -1 = both IPv4 and IPv6
                        0,
                        service.name,
                        service.service_protocol,
                        "", // domain (empty = default .local)
                        "", // hostname (empty = use system hostname)
                        service.port,
                        &txt,
                    )
                    .await?;

                log::info!("  add_service completed");

                for subtype in service.service_subtypes {
                    let avahi_subtype = format!("{}._sub.{}", subtype, service.service_protocol);
                    log::info!("  Adding subtype: {}", avahi_subtype);

                    group
                        .add_service_subtype(
                            -1, // interface: -1 = all (filtered by allowInterfaces)
                            -1, // protocol: -1 = both IPv4 and IPv6
                            0,
                            service.name,
                            service.service_protocol,
                            "",
                            &avahi_subtype,
                        )
                        .await?;
                }

                log::info!("  All subtypes added, committing...");

                group.commit().await?;

                // Check the entry group state after commit
                let state = group.get_state().await?;
                log::info!(
                    "mDNS entry group state immediately after commit: {} (0=uncommitted, 1=registering, 2=established, 3=collision, 4=failure)",
                    state
                );

                // Wait a bit for the state to settle and check again
                embassy_time::Timer::after_millis(100).await;
                let state = group.get_state().await?;
                log::info!("mDNS entry group state after 100ms: {}", state);

                if state == 3 {
                    log::error!("mDNS name collision detected!");
                } else if state == 4 {
                    log::error!("mDNS registration failed!");
                }

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
