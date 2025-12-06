//! Custom mDNS responder that registers services on a specific network interface.
//!
//! This is a minimal wrapper around rs-matter's AvahiMdnsResponder to debug
//! why a custom implementation fails while the original works.

use std::collections::{HashMap, HashSet};
use std::io::Write as _;

use rs_matter::error::Error;
use rs_matter::transport::network::mdns::Service;
use rs_matter::utils::zbus::zvariant::{ObjectPath, OwnedObjectPath};
use rs_matter::utils::zbus::Connection;
use rs_matter::utils::zbus_proxies::avahi::entry_group::EntryGroupProxy;
use rs_matter::utils::zbus_proxies::avahi::server2::Server2Proxy;
use rs_matter::{Matter, MatterMdnsService};

/// An mDNS responder - minimal copy of rs-matter's AvahiMdnsResponder for debugging.
pub struct FilteredAvahiMdnsResponder<'a> {
    matter: &'a Matter<'a>,
    #[allow(dead_code)]
    interface_name: &'static str,
    services: HashMap<MatterMdnsService, OwnedObjectPath>,
}

impl<'a> FilteredAvahiMdnsResponder<'a> {
    /// Create a new filtered mDNS responder.
    pub fn new(matter: &'a Matter<'a>, interface_name: &'static str) -> Self {
        Self {
            matter,
            interface_name,
            services: HashMap::new(),
        }
    }

    /// Run the mDNS responder.
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
        Service::async_call_with(
            matter_service,
            self.matter.dev_det(),
            self.matter.port(),
            async |service| {
                let avahi = Server2Proxy::new(connection).await?;

                let path = avahi.entry_group_new().await?;
                log::info!("Entry group path: {:?}", path);

                let group = EntryGroupProxy::builder(connection)
                    .path(path.clone())?
                    .build()
                    .await?;

                let initial_state = group.get_state().await?;
                log::info!("Entry group initial state: {}", initial_state);

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

                group
                    .add_service(
                        -1,
                        -1,
                        0,
                        service.name,
                        service.service_protocol,
                        "",
                        "",
                        service.port,
                        &txt,
                    )
                    .await?;

                for subtype in service.service_subtypes {
                    let avahi_subtype = format!("{}._sub.{}", subtype, service.service_protocol);

                    group
                        .add_service_subtype(
                            -1,
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

                let final_state = group.get_state().await?;
                log::info!("Entry group state after commit: {} (0=uncommitted, 1=registering, 2=established, 3=collision, 4=failure)", final_state);

                log::info!("mDNS service registered: {}", service.name);

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
