use super::clusters::{
    BooleanStateHandler, BridgedDeviceInfo, BridgedHandler, GenericSwitchHandler,
    GenericSwitchState, IcdManagementHandler, OccupancySensingHandler, RelativeHumidityHandler,
    TemperatureMeasurementHandler,
};
use super::device_info::DEV_INFO;
use super::device_types::{
    DEV_TYPE_AGGREGATOR, DEV_TYPE_BRIDGED_NODE, DEV_TYPE_CONTACT_SENSOR, DEV_TYPE_GENERIC_SWITCH,
    DEV_TYPE_HUMIDITY_SENSOR, DEV_TYPE_OCCUPANCY_SENSOR, DEV_TYPE_ON_OFF_LIGHT,
    DEV_TYPE_ON_OFF_PLUG_IN_UNIT, DEV_TYPE_TEMPERATURE_SENSOR, DEV_TYPE_VIDEO_DOORBELL,
};
use super::endpoints::controls::{DeviceSwitch, LightSwitch, Switch};
use super::handler_bridge::{SensorBridge, SwitchBridge};
use super::logging_udp::LoggingUdpSocket;
use super::netif::{FilteredNetifs, get_interface_name};
use super::virtual_device::{EndpointKind, VirtualDevice, compute_schema_hash};
use embassy_futures::select::{select, select4};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use log::{error, info};
use nix::ifaddrs::getifaddrs;
use nix::net::if_::if_nametoindex;
use nix::sys::socket::{AddressFamily, SockaddrLike};
use rs_matter::crypto::{Crypto, default_crypto};
use rs_matter::dm::IMBuffer;
use rs_matter::dm::clusters::app::on_off::{self, OnOffHooks};
use rs_matter::dm::clusters::desc::{self, ClusterHandler as _, PartsMatcher};
use rs_matter::dm::clusters::net_comm::SharedNetworks;
use rs_matter::dm::devices::test::{DAC_PRIVKEY, TEST_DEV_ATT, TEST_DEV_COMM};
use rs_matter::dm::endpoints;
use rs_matter::dm::events::Events;
use rs_matter::dm::networks::eth::EthNetwork;
use rs_matter::dm::subscriptions::Subscriptions;
use rs_matter::dm::{
    Async, AttrChangeNotifier, Cluster, DataModel, Dataver, DeviceType, Endpoint, EpClMatcher,
    EventEmitter, Handler, InvokeContext, InvokeReply, MatchContext, Matcher, Node,
    NonBlockingHandler, ReadContext, ReadReply, Reply, WriteContext,
};
use rs_matter::error::Error;
use rs_matter::im::EventPriority;
use rs_matter::pairing::DiscoveryCapabilities;
use rs_matter::pairing::qr::QrTextType;
use rs_matter::persist::{FileKvBlobStore, SharedKvBlobStore};
use rs_matter::respond::DefaultResponder;
use rs_matter::transport::network::mdns::builtin::{BuiltinMdnsResponder, Host};
use rs_matter::transport::network::mdns::{
    MDNS_IPV4_BROADCAST_ADDR, MDNS_IPV6_BROADCAST_ADDR, MDNS_SOCKET_DEFAULT_BIND_ADDR,
};
use rs_matter::utils::init::InitMaybeUninit;
use rs_matter::utils::select::Coalesce;
use rs_matter::utils::storage::pooled::PooledBuffers;
use rs_matter::utils::sync::DynBase;
use rs_matter::{MATTER_PORT, Matter, clusters, devices, root_endpoint};
use socket2::{Domain, Protocol, Socket, Type};
use static_cell::StaticCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::pin::pin;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};

use super::clusters::{
    boolean_state, generic_switch, occupancy_sensing, relative_humidity, temperature_measurement,
};
use super::endpoints::{ClusterChangeQueue, ClusterNotifier, NotifiableSensor};
use crate::config::MatterConfig;

/// Static cells for Matter resources (required for 'static lifetime)
static MATTER: StaticCell<Matter> = StaticCell::new();
static BUFFERS: StaticCell<PooledBuffers<10, IMBuffer>> = StaticCell::new();
static SUBSCRIPTIONS: StaticCell<Subscriptions> = StaticCell::new();
static EVENTS: StaticCell<Events<1024>> = StaticCell::new();
static KV_BUF: StaticCell<[u8; 4096]> = StaticCell::new();
/// Signal for sensor change notifications (wakes subscription processor)
/// Uses CriticalSectionRawMutex because sensors are updated from different threads
static SENSOR_NOTIFY: StaticCell<ClusterChangeQueue> = StaticCell::new();
static GENERIC_SWITCH_NOTIFY: StaticCell<Signal<CriticalSectionRawMutex, ()>> = StaticCell::new();

/// Static hostname storage for mDNS (needs 'static lifetime for Host struct)
static HOSTNAME: OnceLock<String> = OnceLock::new();

const ROOT_ENDPOINT_BASE: Endpoint<'static> = root_endpoint!(eth);
const ROOT_ENDPOINT: Endpoint<'static> = Endpoint {
    clusters: clusters!(eth; IcdManagementHandler::CLUSTER),
    ..ROOT_ENDPOINT_BASE
};

/// Dynamic PartsMatcher that handles all parent-child relationships.
/// Built from VirtualDevice configurations at runtime.
#[derive(Debug)]
pub struct DynamicPartsMatcher {
    /// parent_endpoint_id -> child_endpoint_ids
    parent_to_children: Vec<(u16, Vec<u16>)>,
    /// Aggregator (EP2) lists these parent IDs
    aggregator_parents: Vec<u16>,
}

impl DynamicPartsMatcher {
    pub fn new() -> Self {
        Self {
            parent_to_children: Vec::new(),
            aggregator_parents: Vec::new(),
        }
    }

    pub fn add_virtual_device(&mut self, parent_id: u16, child_ids: Vec<u16>) {
        self.parent_to_children.push((parent_id, child_ids.clone()));
        self.aggregator_parents.push(parent_id);
    }

    /// Check if a given endpoint is a parent endpoint for the aggregator.
    pub fn is_aggregator_parent(&self, endpoint: u16) -> bool {
        self.aggregator_parents.contains(&endpoint)
    }

    /// Get children for a given parent endpoint.
    pub fn get_children(&self, parent_id: u16) -> Option<&Vec<u16>> {
        self.parent_to_children
            .iter()
            .find(|(p, _)| *p == parent_id)
            .map(|(_, children)| children)
    }
}

impl Default for DynamicPartsMatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl PartsMatcher for DynamicPartsMatcher {
    fn matches(&self, our_endpoint: u16, endpoint: u16) -> bool {
        if our_endpoint == 2 {
            // Aggregator returns all parent endpoints
            self.aggregator_parents.contains(&endpoint)
        } else {
            // Parent returns its children
            self.parent_to_children
                .iter()
                .find(|(parent, _)| *parent == our_endpoint)
                .map(|(_, children)| children.contains(&endpoint))
                .unwrap_or(false)
        }
    }
}

impl DynBase for DynamicPartsMatcher {}

/// Handler entry for dynamic routing.
enum DynamicHandlerEntry {
    /// BooleanState cluster handler using SensorBridge
    BooleanState {
        dataver: Dataver,
        bridge: Arc<SensorBridge>,
    },
    /// OccupancySensing cluster handler using SensorBridge
    OccupancySensing {
        dataver: Dataver,
        bridge: Arc<SensorBridge>,
    },
    /// OnOff cluster handler using SwitchBridge (for child endpoints)
    OnOff {
        dataver: Dataver,
        bridge: Arc<SwitchBridge>,
    },
    /// OnOff cluster handler using DeviceSwitch (for parent endpoints)
    DeviceOnOff {
        dataver: Dataver,
        switch: Arc<DeviceSwitch>,
    },
    /// Descriptor handler for parent endpoints with PartsMatcher
    DescWithParts {
        dataver: Dataver,
        parts_matcher: &'static DynamicPartsMatcher,
    },
    /// Descriptor handler for child endpoints (no parts)
    Desc { dataver: Dataver },
    /// BridgedDeviceBasicInformation handler
    Bridged { handler: BridgedHandler },
    /// TemperatureMeasurement cluster handler
    Temperature {
        handler: TemperatureMeasurementHandler,
    },
    /// RelativeHumidityMeasurement cluster handler
    Humidity { handler: RelativeHumidityHandler },
    /// GenericSwitch cluster handler (for buttons)
    GenericSwitch { handler: GenericSwitchHandler },
}

/// Dynamic handler that routes requests based on (endpoint_id, cluster_id).
pub struct DynamicHandler {
    handlers: HashMap<(u16, u32), DynamicHandlerEntry>,
}

impl DynamicHandler {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn add_boolean_state(&mut self, ep: u16, dataver: Dataver, bridge: Arc<SensorBridge>) {
        self.handlers.insert(
            (ep, boolean_state::CLUSTER_ID),
            DynamicHandlerEntry::BooleanState { dataver, bridge },
        );
    }

    pub fn add_occupancy_sensing(&mut self, ep: u16, dataver: Dataver, bridge: Arc<SensorBridge>) {
        self.handlers.insert(
            (ep, occupancy_sensing::CLUSTER_ID),
            DynamicHandlerEntry::OccupancySensing { dataver, bridge },
        );
    }

    pub fn add_onoff(&mut self, ep: u16, dataver: Dataver, bridge: Arc<SwitchBridge>) {
        self.handlers.insert(
            (ep, Switch::CLUSTER.id),
            DynamicHandlerEntry::OnOff { dataver, bridge },
        );
    }

    pub fn add_device_onoff(&mut self, ep: u16, dataver: Dataver, switch: Arc<DeviceSwitch>) {
        self.handlers.insert(
            (ep, DeviceSwitch::CLUSTER.id),
            DynamicHandlerEntry::DeviceOnOff { dataver, switch },
        );
    }

    pub fn add_desc_with_parts(
        &mut self,
        ep: u16,
        dataver: Dataver,
        parts_matcher: &'static DynamicPartsMatcher,
    ) {
        self.handlers.insert(
            (ep, desc::DescHandler::CLUSTER.id),
            DynamicHandlerEntry::DescWithParts {
                dataver,
                parts_matcher,
            },
        );
    }

    pub fn add_desc(&mut self, ep: u16, dataver: Dataver) {
        self.handlers.insert(
            (ep, desc::DescHandler::CLUSTER.id),
            DynamicHandlerEntry::Desc { dataver },
        );
    }

    pub fn add_bridged(&mut self, ep: u16, handler: BridgedHandler) {
        self.handlers.insert(
            (ep, BridgedHandler::CLUSTER.id),
            DynamicHandlerEntry::Bridged { handler },
        );
    }

    pub fn add_temperature(&mut self, ep: u16, handler: TemperatureMeasurementHandler) {
        self.handlers.insert(
            (ep, temperature_measurement::CLUSTER_ID),
            DynamicHandlerEntry::Temperature { handler },
        );
    }

    pub fn add_humidity(&mut self, ep: u16, handler: RelativeHumidityHandler) {
        self.handlers.insert(
            (ep, relative_humidity::CLUSTER_ID),
            DynamicHandlerEntry::Humidity { handler },
        );
    }

    pub fn add_generic_switch(&mut self, ep: u16, handler: GenericSwitchHandler) {
        self.handlers.insert(
            (ep, generic_switch::CLUSTER_ID),
            DynamicHandlerEntry::GenericSwitch { handler },
        );
    }
}

impl Default for DynamicHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl Handler for DynamicHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        let ep = ctx.attr().endpoint_id;
        let cl = ctx.attr().cluster_id;

        if let Some(entry) = self.handlers.get(&(ep, cl)) {
            match entry {
                DynamicHandlerEntry::BooleanState { dataver, bridge } => {
                    read_boolean_state(dataver, bridge, ctx, reply)
                }
                DynamicHandlerEntry::OccupancySensing { dataver, bridge } => {
                    read_occupancy_sensing(dataver, bridge, ctx, reply)
                }
                DynamicHandlerEntry::OnOff { dataver, bridge } => {
                    read_onoff(dataver, bridge, ctx, reply)
                }
                DynamicHandlerEntry::DeviceOnOff { dataver, switch } => {
                    read_device_onoff(dataver, switch, ctx, reply)
                }
                DynamicHandlerEntry::DescWithParts {
                    dataver,
                    parts_matcher,
                } => {
                    let handler = desc::DescHandler::new_matching(dataver.clone(), *parts_matcher);
                    Handler::read(&handler.adapt(), ctx, reply)
                }
                DynamicHandlerEntry::Desc { dataver } => {
                    let handler = desc::DescHandler::new(dataver.clone());
                    Handler::read(&handler.adapt(), ctx, reply)
                }
                DynamicHandlerEntry::Bridged { handler } => handler.read(ctx, reply),
                DynamicHandlerEntry::Temperature { handler } => handler.read(ctx, reply),
                DynamicHandlerEntry::Humidity { handler } => handler.read(ctx, reply),
                DynamicHandlerEntry::GenericSwitch { handler } => handler.read(ctx, reply),
            }
        } else {
            log::debug!(
                "DynamicHandler: no handler for endpoint {} cluster 0x{:04x}",
                ep,
                cl
            );
            Err(rs_matter::error::ErrorCode::ClusterNotFound.into())
        }
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        let ep = ctx.attr().endpoint_id;
        let cl = ctx.attr().cluster_id;

        if let Some(entry) = self.handlers.get(&(ep, cl)) {
            match entry {
                DynamicHandlerEntry::OnOff { dataver, bridge } => write_onoff(dataver, bridge, ctx),
                DynamicHandlerEntry::DeviceOnOff { dataver, switch } => {
                    write_device_onoff(dataver, switch, ctx)
                }
                _ => Err(rs_matter::error::ErrorCode::UnsupportedAccess.into()),
            }
        } else {
            Err(rs_matter::error::ErrorCode::ClusterNotFound.into())
        }
    }

    fn invoke(&self, ctx: impl InvokeContext, _reply: impl InvokeReply) -> Result<(), Error> {
        let cmd = ctx.cmd();
        let ep = cmd.endpoint_id;
        let cl = cmd.cluster_id;
        let cmd_id = cmd.cmd_id;

        if let Some(entry) = self.handlers.get(&(ep, cl)) {
            match entry {
                DynamicHandlerEntry::OnOff { bridge, .. } => {
                    // OnOff cluster commands: Off=0x00, On=0x01, Toggle=0x02
                    match cmd_id {
                        0x00 => {
                            bridge.set(false);
                            Ok(())
                        }
                        0x01 => {
                            bridge.set(true);
                            Ok(())
                        }
                        0x02 => {
                            bridge.toggle();
                            Ok(())
                        }
                        _ => Err(rs_matter::error::ErrorCode::CommandNotFound.into()),
                    }
                }
                DynamicHandlerEntry::DeviceOnOff { switch, .. } => {
                    // Parent device switch - uses OnOffHooks trait methods
                    match cmd_id {
                        0x00 => {
                            switch.set_on_off(false);
                            Ok(())
                        }
                        0x01 => {
                            switch.set_on_off(true);
                            Ok(())
                        }
                        0x02 => {
                            let new_val = !switch.on_off();
                            switch.set_on_off(new_val);
                            Ok(())
                        }
                        _ => Err(rs_matter::error::ErrorCode::CommandNotFound.into()),
                    }
                }
                _ => Err(rs_matter::error::ErrorCode::CommandNotFound.into()),
            }
        } else {
            Err(rs_matter::error::ErrorCode::ClusterNotFound.into())
        }
    }

    fn bump_dataver(&self, ctx: impl MatchContext) {
        let ep = ctx.endpt();
        let cl = ctx.cluster();

        for ((entry_ep, entry_cl), entry) in &self.handlers {
            let ep_matches = ep.is_none_or(|ep| ep == *entry_ep);
            let cl_matches = cl.is_none_or(|cl| cl == *entry_cl);

            if ep_matches && cl_matches {
                match entry {
                    DynamicHandlerEntry::BooleanState { dataver, .. }
                    | DynamicHandlerEntry::OccupancySensing { dataver, .. }
                    | DynamicHandlerEntry::OnOff { dataver, .. }
                    | DynamicHandlerEntry::DeviceOnOff { dataver, .. }
                    | DynamicHandlerEntry::DescWithParts { dataver, .. }
                    | DynamicHandlerEntry::Desc { dataver } => {
                        dataver.changed();
                    }
                    DynamicHandlerEntry::Bridged { handler } => handler.bump_dataver(&ctx),
                    DynamicHandlerEntry::Temperature { handler } => handler.bump_dataver(&ctx),
                    DynamicHandlerEntry::Humidity { handler } => handler.bump_dataver(&ctx),
                    DynamicHandlerEntry::GenericSwitch { handler } => handler.bump_dataver(&ctx),
                }
            }
        }
    }
}

impl NonBlockingHandler for DynamicHandler {}

impl Matcher for DynamicHandler {
    fn matches(&self, ctx: impl MatchContext) -> bool {
        self.handlers.iter().any(|((ep, cl), _)| {
            ctx.endpt().is_none_or(|ctx_ep| ctx_ep == *ep)
                && ctx.cluster().is_none_or(|ctx_cl| ctx_cl == *cl)
        })
    }
}

/// Read handler for BooleanState cluster.
fn read_boolean_state(
    dataver: &Dataver,
    bridge: &SensorBridge,
    ctx: impl ReadContext,
    reply: impl ReadReply,
) -> Result<(), Error> {
    use rs_matter::tlv::TLVWrite;

    let attr = ctx.attr();

    let Some(mut writer) = reply.with_dataver(dataver.get())? else {
        return Ok(());
    };

    if attr.is_system() {
        return boolean_state::CLUSTER.read(attr, writer);
    }

    let tag = writer.tag();
    {
        let mut tw = writer.writer();
        match attr.attr_id {
            0x00 => tw.bool(tag, bridge.get())?, // StateValue
            _ => return Err(rs_matter::error::ErrorCode::AttributeNotFound.into()),
        }
    }
    writer.complete()
}

/// Read handler for OccupancySensing cluster.
fn read_occupancy_sensing(
    dataver: &Dataver,
    bridge: &SensorBridge,
    ctx: impl ReadContext,
    reply: impl ReadReply,
) -> Result<(), Error> {
    use rs_matter::tlv::TLVWrite;

    let attr = ctx.attr();

    let Some(mut writer) = reply.with_dataver(dataver.get())? else {
        return Ok(());
    };

    if attr.is_system() {
        return occupancy_sensing::CLUSTER.read(attr, writer);
    }

    let tag = writer.tag();
    {
        let mut tw = writer.writer();
        match attr.attr_id {
            0x00 => tw.u8(tag, if bridge.get() { 1 } else { 0 })?, // Occupancy bitmap
            0x01 => tw.u8(tag, 0)?,                                // OccupancySensorType (PIR)
            0x02 => tw.u8(tag, 1)?,                                // OccupancySensorTypeBitmap
            _ => return Err(rs_matter::error::ErrorCode::AttributeNotFound.into()),
        }
    }
    writer.complete()
}

/// Read handler for OnOff cluster.
fn read_onoff(
    dataver: &Dataver,
    bridge: &SwitchBridge,
    ctx: impl ReadContext,
    reply: impl ReadReply,
) -> Result<(), Error> {
    use rs_matter::tlv::TLVWrite;

    let attr = ctx.attr();

    let Some(mut writer) = reply.with_dataver(dataver.get())? else {
        return Ok(());
    };

    if attr.is_system() {
        return Switch::CLUSTER.read(attr, writer);
    }

    let tag = writer.tag();
    {
        let mut tw = writer.writer();
        match attr.attr_id {
            0x00 => tw.bool(tag, bridge.get())?, // OnOff
            other => {
                log::debug!(
                    "OnOff cluster: unknown attr 0x{:04x} on endpoint {}",
                    other,
                    attr.endpoint_id
                );
                return Err(rs_matter::error::ErrorCode::AttributeNotFound.into());
            }
        }
    }
    writer.complete()
}

/// Write handler for OnOff cluster.
fn write_onoff(
    _dataver: &Dataver,
    bridge: &SwitchBridge,
    ctx: impl WriteContext,
) -> Result<(), Error> {
    let attr = ctx.attr();

    match attr.attr_id {
        0x00 => {
            // OnOff attribute - this is typically controlled via commands, not writes
            // But we support it for completeness
            let data = ctx.data();
            let value = data.bool()?;
            bridge.set(value);
            Ok(())
        }
        _ => Err(rs_matter::error::ErrorCode::UnsupportedAccess.into()),
    }
}

/// Read handler for DeviceSwitch OnOff cluster (parent endpoints).
fn read_device_onoff(
    dataver: &Dataver,
    switch: &DeviceSwitch,
    ctx: impl ReadContext,
    reply: impl ReadReply,
) -> Result<(), Error> {
    use rs_matter::tlv::TLVWrite;

    let attr = ctx.attr();

    let Some(mut writer) = reply.with_dataver(dataver.get())? else {
        return Ok(());
    };

    if attr.is_system() {
        return DeviceSwitch::CLUSTER.read(attr, writer);
    }

    let tag = writer.tag();
    {
        let mut tw = writer.writer();
        match attr.attr_id {
            0x00 => tw.bool(tag, switch.get())?, // OnOff
            other => {
                log::debug!(
                    "DeviceOnOff cluster: unknown attr 0x{:04x} on endpoint {}",
                    other,
                    attr.endpoint_id
                );
                return Err(rs_matter::error::ErrorCode::AttributeNotFound.into());
            }
        }
    }
    writer.complete()
}

/// Write handler for DeviceSwitch OnOff cluster (parent endpoints).
fn write_device_onoff(
    _dataver: &Dataver,
    switch: &DeviceSwitch,
    ctx: impl WriteContext,
) -> Result<(), Error> {
    let attr = ctx.attr();

    match attr.attr_id {
        0x00 => {
            // OnOff attribute - use OnOffHooks::set_on_off for cascade behavior
            let data = ctx.data();
            let value = data.bool()?;
            switch.set_on_off(value);
            Ok(())
        }
        _ => Err(rs_matter::error::ErrorCode::UnsupportedAccess.into()),
    }
}

/// Get IPv4 and IPv6 addresses for a specific network interface.
///
/// Prefers global IPv6 addresses, falls back to link-local (fe80::/10) if no global available.
fn get_interface_addresses(interface_name: &str) -> Result<(Vec<Ipv4Addr>, Vec<Ipv6Addr>), Error> {
    let addrs = getifaddrs().map_err(|e| {
        error!("Failed to get interface addresses: {:?}", e);
        rs_matter::error::ErrorCode::MdnsError
    })?;

    let mut ipv4 = Vec::new();
    let mut ipv6_global = Vec::new();
    let mut ipv6_link_local = Vec::new();

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
                        let octets = ip.octets();
                        // Separate link-local (fe80::/10) from global addresses
                        if octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80 {
                            ipv6_link_local.push(ip);
                        } else {
                            ipv6_global.push(ip);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Prefer global IPv6, fall back to link-local if no global available
    let ipv6 = if ipv6_global.is_empty() {
        ipv6_link_local
    } else {
        ipv6_global
    };

    Ok((ipv4, ipv6))
}

/// Get the interface index for a network interface name.
fn get_interface_index(interface_name: &str) -> Result<u32, Error> {
    let cname = CString::new(interface_name).map_err(|_| {
        error!("Invalid interface name: {}", interface_name);
        Error::from(rs_matter::error::ErrorCode::MdnsError)
    })?;
    if_nametoindex(cname.as_c_str()).map_err(|e| {
        error!(
            "Failed to get interface index for '{}': {:?}",
            interface_name, e
        );
        Error::from(rs_matter::error::ErrorCode::MdnsError)
    })
}

/// Directory for persistence data
const PERSIST_DIR: &str = ".config/virtual-matter-bridge";
const PERSIST_FILE: &str = "matter.bin";
const SCHEMA_FILE: &str = "schema.hash";

/// Get the persistence file path
fn get_persist_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(PERSIST_DIR)
        .join(PERSIST_FILE)
}

/// Get the schema hash file path
fn get_schema_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(PERSIST_DIR)
        .join(SCHEMA_FILE)
}

/// Check if schema has changed and reset persistence if needed.
///
/// Returns true if persistence was reset (commissioning window should open).
fn check_schema_and_maybe_reset(
    virtual_devices: &[VirtualDevice],
    persist_path: &PathBuf,
    schema_path: &PathBuf,
) -> bool {
    // Check for DEV_AUTO_RESET environment variable
    let force_reset = std::env::var("DEV_AUTO_RESET")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    if force_reset {
        info!("DEV_AUTO_RESET is set, forcing persistence reset");
        let _ = fs::remove_file(persist_path);
        return true;
    }

    // Compute current schema hash
    let current_hash = compute_schema_hash(virtual_devices);

    // Try to read stored schema hash
    let stored_hash = fs::read_to_string(schema_path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok());

    match stored_hash {
        Some(hash) if hash == current_hash => {
            info!(
                "Schema hash unchanged ({:#018x}), keeping persistence",
                current_hash
            );
            false
        }
        Some(old_hash) => {
            info!(
                "Schema hash changed ({:#018x} -> {:#018x}), resetting persistence",
                old_hash, current_hash
            );
            // Delete matter.bin to force re-commissioning
            if let Err(e) = fs::remove_file(persist_path)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                error!("Failed to delete persistence file: {}", e);
            }
            // Write new schema hash
            if let Err(e) = fs::write(schema_path, format!("{}\n", current_hash)) {
                error!("Failed to write schema hash: {}", e);
            }
            true
        }
        None => {
            // No stored hash, first run or corrupted - write current hash
            info!(
                "No schema hash found, storing current ({:#018x})",
                current_hash
            );
            if let Err(e) = fs::write(schema_path, format!("{}\n", current_hash)) {
                error!("Failed to write schema hash: {}", e);
            }
            false
        }
    }
}

fn register_cluster_notifier<T: NotifiableSensor + ?Sized>(
    target: &T,
    queue: &'static ClusterChangeQueue,
    endpoint_id: u16,
    cluster_id: u32,
) {
    target.set_notifier(ClusterNotifier::new(queue, endpoint_id, cluster_id));
}

/// Cached network interface filter (lazily initialized)
static NETIFS: OnceLock<FilteredNetifs> = OnceLock::new();

/// Get the network interface filter, auto-detecting if necessary.
fn get_netifs() -> &'static FilteredNetifs {
    NETIFS.get_or_init(FilteredNetifs::auto_detect)
}

/// Leak a slice to get 'static lifetime.
fn leak_slice<T: Clone>(items: &[T]) -> &'static [T] {
    Box::leak(items.to_vec().into_boxed_slice())
}

/// Leak a value to get 'static lifetime.
fn leak<T>(value: T) -> &'static T {
    Box::leak(Box::new(value))
}

/// Mapping of allocated endpoint IDs for a virtual device.
#[derive(Debug, Clone)]
pub struct EndpointMapping {
    /// Parent endpoint ID
    pub parent_id: u16,
    /// Child endpoint IDs (in order of EndpointConfig vec)
    pub child_ids: Vec<u16>,
}

/// Result of building the dynamic node.
pub struct BuiltNode {
    /// The static Node reference
    pub node: &'static Node<'static>,
    /// The dynamic parts matcher
    pub parts_matcher: &'static DynamicPartsMatcher,
    /// Endpoint mappings for each virtual device (in order)
    pub mappings: Vec<EndpointMapping>,
}

/// Build the Matter Node dynamically from virtual device configurations.
///
/// Returns the node with 'static lifetime (via Box::leak) and the parts matcher.
pub fn build_node(virtual_devices: &[VirtualDevice]) -> BuiltNode {
    let mut endpoints_vec = Vec::new();
    let mut parts_matcher = DynamicPartsMatcher::new();
    let mut mappings = Vec::new();
    let mut next_id: u16 = 3; // Start after Root(0), Doorbell(1), Aggregator(2)

    // Endpoint 0: Root node
    endpoints_vec.push(ROOT_ENDPOINT);

    // Endpoint 1: Bridge master on/off control
    endpoints_vec.push(Endpoint::new(
        1,
        devices!(DEV_TYPE_ON_OFF_PLUG_IN_UNIT),
        clusters!(desc::DescHandler::CLUSTER, Switch::CLUSTER),
    ));

    // Endpoint 2: Aggregator (bridge root - will use DynamicPartsMatcher)
    endpoints_vec.push(Endpoint::new(
        2,
        devices!(DEV_TYPE_AGGREGATOR),
        clusters!(desc::DescHandler::CLUSTER),
    ));

    // Add virtual devices dynamically
    for device in virtual_devices {
        let parent_id = next_id;
        next_id += 1;

        // Get device types for parent (always OnOffPlugInUnit - parent is generic on/off switch)
        let parent_device_types: &'static [DeviceType] =
            leak_slice(&[DEV_TYPE_ON_OFF_PLUG_IN_UNIT, DEV_TYPE_BRIDGED_NODE]);

        // Parent endpoint (bridged node with OnOff cluster for device-level control)
        endpoints_vec.push(Endpoint::new(
            parent_id,
            parent_device_types,
            clusters!(
                desc::DescHandler::CLUSTER,
                BridgedHandler::CLUSTER,
                DeviceSwitch::CLUSTER
            ),
        ));

        // Add child endpoints
        let mut child_ids = Vec::new();
        for ep_config in &device.endpoints {
            let child_id = next_id;
            next_id += 1;
            child_ids.push(child_id);

            let (device_types, clusters): (&'static [DeviceType], &'static [Cluster<'static>]) =
                match ep_config.kind {
                    EndpointKind::ContactSensor => (
                        leak_slice(&[DEV_TYPE_CONTACT_SENSOR]),
                        clusters!(
                            desc::DescHandler::CLUSTER,
                            BridgedHandler::CLUSTER,
                            BooleanStateHandler::CLUSTER
                        ),
                    ),
                    EndpointKind::OccupancySensor => (
                        leak_slice(&[DEV_TYPE_OCCUPANCY_SENSOR]),
                        clusters!(
                            desc::DescHandler::CLUSTER,
                            BridgedHandler::CLUSTER,
                            OccupancySensingHandler::CLUSTER
                        ),
                    ),
                    EndpointKind::Switch => (
                        leak_slice(&[DEV_TYPE_ON_OFF_PLUG_IN_UNIT]),
                        clusters!(
                            desc::DescHandler::CLUSTER,
                            BridgedHandler::CLUSTER,
                            Switch::CLUSTER
                        ),
                    ),
                    EndpointKind::LightSwitch => (
                        leak_slice(&[DEV_TYPE_ON_OFF_LIGHT]),
                        clusters!(
                            desc::DescHandler::CLUSTER,
                            BridgedHandler::CLUSTER,
                            LightSwitch::CLUSTER
                        ),
                    ),
                    EndpointKind::VideoDoorbellCamera => (
                        leak_slice(&[DEV_TYPE_VIDEO_DOORBELL]),
                        clusters!(
                            desc::DescHandler::CLUSTER,
                            BridgedHandler::CLUSTER // TODO: Add CameraAvStreamMgmtHandler::CLUSTER and WebRtcTransportProviderHandler::CLUSTER
                                                    // when handlers are wired (see TODO at ~line 1145)
                        ),
                    ),
                    EndpointKind::TemperatureSensor => (
                        leak_slice(&[DEV_TYPE_TEMPERATURE_SENSOR]),
                        clusters!(
                            desc::DescHandler::CLUSTER,
                            BridgedHandler::CLUSTER,
                            TemperatureMeasurementHandler::CLUSTER
                        ),
                    ),
                    EndpointKind::HumiditySensor => (
                        leak_slice(&[DEV_TYPE_HUMIDITY_SENSOR]),
                        clusters!(
                            desc::DescHandler::CLUSTER,
                            BridgedHandler::CLUSTER,
                            RelativeHumidityHandler::CLUSTER
                        ),
                    ),
                    EndpointKind::GenericSwitch => (
                        leak_slice(&[DEV_TYPE_GENERIC_SWITCH]),
                        clusters!(
                            desc::DescHandler::CLUSTER,
                            BridgedHandler::CLUSTER,
                            GenericSwitchHandler::CLUSTER
                        ),
                    ),
                };

            endpoints_vec.push(Endpoint::new(child_id, device_types, clusters));
        }

        parts_matcher.add_virtual_device(parent_id, child_ids.clone());
        mappings.push(EndpointMapping {
            parent_id,
            child_ids,
        });
    }

    // Leak to get 'static lifetime
    let node = leak(Node {
        endpoints: leak_slice(&endpoints_vec),
    });

    let parts_matcher = leak(parts_matcher);

    BuiltNode {
        node,
        parts_matcher,
        mappings,
    }
}

/// Run the Matter stack with dynamic virtual devices.
///
/// This function initializes and runs the Matter protocol stack, enabling:
/// - Device discovery via mDNS
/// - Commissioning (pairing) with controllers like Home Assistant
/// - Matter protocol communication
///
/// The virtual devices define what bridged devices to expose.
///
/// Note: Currently uses test device credentials for development.
pub async fn run_matter_stack(
    config: &MatterConfig,
    // Bridge master on/off switch (controls EP1, cascades to all parent DeviceSwitches)
    virtual_bridge_onoff: Arc<Switch>,
    // Dynamic virtual devices (bridged under EP2 Aggregator)
    virtual_devices: Vec<VirtualDevice>,
) -> Result<(), Error> {
    info!("Initializing Matter stack...");

    // Build the dynamic node structure
    let built_node = build_node(&virtual_devices);
    info!(
        "Built Matter node with {} endpoints ({} virtual devices)",
        built_node.node.endpoints.len(),
        virtual_devices.len()
    );

    // Initialize the Matter instance in static memory
    let matter = MATTER.uninit().init_with(Matter::init(
        &DEV_INFO,
        TEST_DEV_COMM,
        &TEST_DEV_ATT,
        MATTER_PORT,
    ));

    let crypto = default_crypto(rand::thread_rng(), DAC_PRIVKEY);
    let mut rand = crypto.rand()?;

    // Detect network interface and get addresses BEFORE socket creation
    let interface_name = get_interface_name();
    let interface_index = get_interface_index(interface_name)?;
    let (ipv4_addrs, ipv6_addrs) = get_interface_addresses(interface_name)?;

    if ipv4_addrs.is_empty() {
        error!("No IPv4 address found on interface '{}'", interface_name);
        return Err(rs_matter::error::ErrorCode::MdnsError.into());
    }

    let ipv6_addr = if ipv6_addrs.is_empty() {
        info!("No IPv6 address on '{}', using unspecified", interface_name);
        Ipv6Addr::UNSPECIFIED
    } else {
        // Log whether we're using global or link-local
        let addr = ipv6_addrs[0];
        let octets = addr.octets();
        if octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80 {
            info!(
                "Using link-local IPv6 address {} (no global available)",
                addr
            );
        }
        addr
    };

    info!(
        "Using interface '{}' (index {}) with {} and {}",
        interface_name, interface_index, ipv4_addrs[0], ipv6_addr
    );

    // Create UDP socket for Matter transport
    let raw_socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP)).map_err(|e| {
        error!("Failed to create UDP socket: {}", e);
        rs_matter::error::ErrorCode::StdIoError
    })?;
    raw_socket.set_reuse_address(true).map_err(|e| {
        error!("Failed to set SO_REUSEADDR: {}", e);
        rs_matter::error::ErrorCode::StdIoError
    })?;
    raw_socket.set_only_v6(false).map_err(|e| {
        error!("Failed to set IPV6_V6ONLY=false: {}", e);
        rs_matter::error::ErrorCode::StdIoError
    })?;
    raw_socket.set_nonblocking(true).map_err(|e| {
        error!("Failed to set non-blocking: {}", e);
        rs_matter::error::ErrorCode::StdIoError
    })?;

    let bind_addr = SocketAddr::new(IpAddr::V6(ipv6_addr), MATTER_PORT);
    raw_socket.bind(&bind_addr.into()).map_err(|e| {
        error!("Failed to bind UDP socket to {:?}: {}", bind_addr, e);
        rs_matter::error::ErrorCode::StdIoError
    })?;
    let socket = async_io::Async::<UdpSocket>::new(raw_socket.into()).map_err(|e| {
        error!("Failed to create async socket: {}", e);
        rs_matter::error::ErrorCode::StdIoError
    })?;
    info!("Matter UDP socket bound to {:?}", bind_addr);

    // Initialize key-value persistence and load existing state
    let persist_path = get_persist_path();
    let schema_path = get_schema_path();

    if let Some(parent) = persist_path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        error!("Failed to create persistence directory {:?}: {}", parent, e);
    }

    // Check if schema changed and reset persistence if needed
    let schema_reset = check_schema_and_maybe_reset(&virtual_devices, &persist_path, &schema_path);

    let kv_buf = KV_BUF.uninit().init_with([0; 4096]);
    let events = EVENTS.uninit().init_with(Events::init());
    let mut kv = FileKvBlobStore::new(persist_path.clone());

    if persist_path.exists() {
        let load_result = async {
            matter.load_persist(&mut kv, &mut kv_buf[..]).await?;
            events.load_persist(&mut kv, &mut kv_buf[..]).await
        }
        .await;

        if let Err(e) = load_result {
            error!(
                "Failed to load persisted state from {:?}: {:?}",
                persist_path, e
            );
            if let Err(reset_err) = matter.reset_persist(&mut kv, &mut kv_buf[..]).await {
                error!("Failed to reset Matter persisted state: {:?}", reset_err);
            }
            events.reset();
            if let Err(del_err) = fs::remove_file(&persist_path) {
                error!("Failed to delete corrupt persistence file: {}", del_err);
            } else {
                info!("Deleted corrupt persistence file, device will need re-commissioning");
            }
            kv = FileKvBlobStore::new(persist_path.clone());
        }
    } else if schema_reset {
        info!("Persistence file deleted due to schema change, starting fresh");
    }

    // Use shared reference going forward (avoid moving the &mut)
    let matter: &'static Matter = &*matter;

    // Initialize pooled buffers in static memory
    let buffers = BUFFERS.uninit().init_with(PooledBuffers::init(0));

    // Initialize subscriptions manager in static memory
    let subscriptions = SUBSCRIPTIONS.uninit().init_with(Subscriptions::init());

    // Initialize sensor notification signal
    let sensor_notify_ref = SENSOR_NOTIFY.uninit().init_with(ClusterChangeQueue::new());
    let generic_switch_notify = GENERIC_SWITCH_NOTIFY.uninit().init_with(Signal::new());
    let generic_switch_notify_ref: &'static Signal<CriticalSectionRawMutex, ()> =
        generic_switch_notify;

    // Create DynamicHandler for virtual device endpoints (EP3+)
    let mut dynamic_handler = DynamicHandler::new();

    // Collect parent DeviceSwitches for virtual_bridge_onoff cascade
    let mut parent_device_switches: Vec<Arc<DeviceSwitch>> = Vec::new();
    let mut generic_switch_states: Vec<Arc<GenericSwitchState>> = Vec::new();

    for (device_idx, device) in virtual_devices.iter().enumerate() {
        let mapping = &built_node.mappings[device_idx];
        let parent_id = mapping.parent_id;

        // Create the parent DeviceSwitch for this virtual device
        let device_switch = Arc::new(DeviceSwitch::new(true));
        parent_device_switches.push(device_switch.clone());

        // Add descriptor handler for parent (with parts matcher for children)
        dynamic_handler.add_desc_with_parts(
            parent_id,
            Dataver::new_rand(&mut rand),
            built_node.parts_matcher,
        );

        // Add bridged device info handler for parent (always reachable)
        // Use device_info if provided, otherwise create from label
        let parent_device_info = device
            .device_info
            .clone()
            .unwrap_or_else(|| BridgedDeviceInfo::new(device.label));
        dynamic_handler.add_bridged(
            parent_id,
            BridgedHandler::new_always_reachable(Dataver::new_rand(&mut rand), parent_device_info),
        );

        // Add OnOff handler for parent (device-level switch)
        dynamic_handler.add_device_onoff(
            parent_id,
            Dataver::new_rand(&mut rand),
            device_switch.clone(),
        );

        // Create handlers for each child endpoint
        for (child_idx, ep_config) in device.endpoints.iter().enumerate() {
            let child_id = mapping.child_ids[child_idx];

            // Create reachable flag for this child (controlled by parent DeviceSwitch)
            let child_reachable = Arc::new(AtomicBool::new(true));
            device_switch.add_child_reachable(child_reachable.clone());

            // Add descriptor handler for child (no parts)
            dynamic_handler.add_desc(child_id, Dataver::new_rand(&mut rand));

            // Add bridged device info handler for child (with dynamic reachable)
            // Child endpoints use label only (device info is on parent)
            dynamic_handler.add_bridged(
                child_id,
                BridgedHandler::new_with_name(
                    Dataver::new_rand(&mut rand),
                    ep_config.label,
                    child_reachable,
                ),
            );

            match ep_config.kind {
                EndpointKind::ContactSensor => {
                    let bridge = SensorBridge::new(ep_config.handler.clone());
                    register_cluster_notifier(
                        bridge.as_ref(),
                        sensor_notify_ref,
                        child_id,
                        boolean_state::CLUSTER_ID,
                    );
                    dynamic_handler.add_boolean_state(
                        child_id,
                        Dataver::new_rand(&mut rand),
                        bridge,
                    );
                }
                EndpointKind::OccupancySensor => {
                    let bridge = SensorBridge::new(ep_config.handler.clone());
                    register_cluster_notifier(
                        bridge.as_ref(),
                        sensor_notify_ref,
                        child_id,
                        occupancy_sensing::CLUSTER_ID,
                    );
                    dynamic_handler.add_occupancy_sensing(
                        child_id,
                        Dataver::new_rand(&mut rand),
                        bridge,
                    );
                }
                EndpointKind::Switch | EndpointKind::LightSwitch => {
                    let bridge = SwitchBridge::new(ep_config.handler.clone());
                    register_cluster_notifier(
                        bridge.as_ref(),
                        sensor_notify_ref,
                        child_id,
                        Switch::CLUSTER.id,
                    );
                    // Add child switch to parent's cascade list
                    device_switch.add_child_switch(bridge.clone());
                    dynamic_handler.add_onoff(child_id, Dataver::new_rand(&mut rand), bridge);
                }
                EndpointKind::VideoDoorbellCamera => {
                    // TODO: Camera handlers are async and need different handling than DynamicHandler.
                    // For now, VideoDoorbellCamera endpoints are registered but handlers are not wired.
                    // This requires extending DynamicHandler to support async handlers or
                    // building a separate async handler chain for camera endpoints.
                    log::warn!(
                        "VideoDoorbellCamera endpoint {} registered but camera handlers not yet wired",
                        child_id
                    );
                }
                EndpointKind::TemperatureSensor => {
                    // Use sensor from EndpointConfig (created by caller)
                    if let Some(sensor) = &ep_config.temperature_sensor {
                        register_cluster_notifier(
                            sensor.as_ref(),
                            sensor_notify_ref,
                            child_id,
                            temperature_measurement::CLUSTER_ID,
                        );
                        let handler = TemperatureMeasurementHandler::new(
                            Dataver::new_rand(&mut rand),
                            sensor.clone(),
                        );
                        dynamic_handler.add_temperature(child_id, handler);
                    } else {
                        log::warn!(
                            "TemperatureSensor endpoint {} missing sensor in config",
                            child_id
                        );
                    }
                }
                EndpointKind::HumiditySensor => {
                    // Use sensor from EndpointConfig (created by caller)
                    if let Some(sensor) = &ep_config.humidity_sensor {
                        register_cluster_notifier(
                            sensor.as_ref(),
                            sensor_notify_ref,
                            child_id,
                            relative_humidity::CLUSTER_ID,
                        );
                        let handler = RelativeHumidityHandler::new(
                            Dataver::new_rand(&mut rand),
                            sensor.clone(),
                        );
                        dynamic_handler.add_humidity(child_id, handler);
                    } else {
                        log::warn!(
                            "HumiditySensor endpoint {} missing sensor in config",
                            child_id
                        );
                    }
                }
                EndpointKind::GenericSwitch => {
                    // Use state from EndpointConfig (created by caller)
                    if let Some(state) = &ep_config.generic_switch_state {
                        // Set endpoint ID so events know where they came from
                        state.set_endpoint_id(child_id);
                        state.set_event_notifier(generic_switch_notify_ref);
                        generic_switch_states.push(state.clone());
                        let handler =
                            GenericSwitchHandler::new(Dataver::new_rand(&mut rand), state.clone());
                        dynamic_handler.add_generic_switch(child_id, handler);
                        info!(
                            "[Matter] GenericSwitch endpoint {} registered for '{}'",
                            child_id, ep_config.label
                        );
                    } else {
                        log::warn!(
                            "GenericSwitch endpoint {} missing state in config",
                            child_id
                        );
                    }
                }
            }
        }
    }

    // Wire up virtual_bridge_onoff to cascade to all parent DeviceSwitches
    for device_switch in &parent_device_switches {
        virtual_bridge_onoff.add_cascade_target(device_switch.clone());
    }

    // Create OnOff handler for bridge master on/off (endpoint 1)
    let virtual_bridge_onoff_handler = on_off::OnOffHandler::new_standalone(
        Dataver::new_rand(&mut rand),
        1,
        virtual_bridge_onoff.as_ref(),
    );

    let eth_sys_handler = endpoints::EthSysHandlerBuilder::new()
        .netif_diag(get_netifs())
        .build(&mut rand);
    let icd_management_handler = IcdManagementHandler::new(Dataver::new_rand(&mut rand));

    // Build the handler chain with dynamic handler for virtual devices
    let handler = (
        built_node.node,
        eth_sys_handler
            .chain(
                EpClMatcher::new(Some(0), Some(IcdManagementHandler::CLUSTER.id)),
                Async(
                    rs_matter::dm::clusters::decl::icd_management::HandlerAdaptor(
                        icd_management_handler,
                    ),
                ),
            )
            // === Endpoint 1: Bridge master on/off control ===
            .chain(
                EpClMatcher::new(Some(1), Some(desc::DescHandler::CLUSTER.id)),
                Async(desc::DescHandler::new(Dataver::new_rand(&mut rand)).adapt()),
            )
            .chain(
                EpClMatcher::new(Some(1), Some(Switch::CLUSTER.id)),
                virtual_bridge_onoff_handler.adapt(),
            )
            // === Endpoint 2: Aggregator ===
            .chain(
                EpClMatcher::new(Some(2), Some(desc::DescHandler::CLUSTER.id)),
                Async(
                    desc::DescHandler::new_matching(
                        Dataver::new_rand(&mut rand),
                        built_node.parts_matcher,
                    )
                    .adapt(),
                ),
            )
            // === Endpoint 3+: Virtual Devices (dynamic) ===
            .chain(
                &dynamic_handler, // Only matches (ep, cl) pairs we have handlers for
                Async(&dynamic_handler),
            ),
    );

    let dm = DataModel::new(
        matter,
        &crypto,
        buffers,
        subscriptions,
        events,
        handler,
        SharedKvBlobStore::new(kv, &mut kv_buf[..]),
        SharedNetworks::new(EthNetwork::new_default()),
    );

    // Only open commissioning window if device is not already commissioned
    const COMM_WINDOW_TIMEOUT_SECS: u16 = 900;
    if matter.is_commissioned() {
        info!("Device already commissioned, skipping commissioning window");
        info!("  (Delete {:?} to reset commissioning)", persist_path);
    } else {
        info!(
            "Opening commissioning window for {} seconds...",
            COMM_WINDOW_TIMEOUT_SECS
        );
        dm.open_basic_comm_window(COMM_WINDOW_TIMEOUT_SECS)?;

        info!("Matter device ready for commissioning");
        info!("  Discriminator: {}", TEST_DEV_COMM.discriminator);
        info!(
            "  Passcode: {}",
            u32::from_le_bytes(*TEST_DEV_COMM.password.access())
        );

        if let Some(ref server_url) = config.server_url {
            info!("Auto-commission enabled: {}", server_url);
            let url = server_url.clone();
            let discriminator = config.discriminator;
            let passcode = config.passcode;
            let vendor_id = config.vendor_id;
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    if schema_reset {
                        info!("Schema changed, cleaning up old nodes from controller");
                        if let Err(e) =
                            crate::commissioning::remove_bridge_nodes(&url, vendor_id).await
                        {
                            log::warn!("Failed to cleanup old nodes: {}", e);
                        }
                    }

                    match crate::commissioning::auto_commission(&url, discriminator, passcode).await
                    {
                        Ok(()) => {
                            info!("Auto-commission completed successfully");
                        }
                        Err(e) => {
                            log::warn!("Auto-commission failed: {}", e);
                            let pairing_code = crate::commissioning::generate_pairing_code(
                                discriminator,
                                passcode,
                            );
                            info!("Commission manually with pairing code: {}", pairing_code);
                        }
                    }
                })
            });
        } else {
            if let Err(e) = matter.print_standard_qr_text(DiscoveryCapabilities::IP) {
                error!("Failed to print QR text: {:?}", e);
            }

            if let Err(e) =
                matter.print_standard_qr_code(QrTextType::Unicode, DiscoveryCapabilities::IP)
            {
                error!("Failed to print QR code: {:?}", e);
            }
        }
    }

    // Create the responder
    let responder = DefaultResponder::new(&dm);

    info!("Matter stack running. Waiting for controller connections...");

    // Run Matter transport with logging wrapper
    let logging_socket = LoggingUdpSocket::new(&socket);
    let mut transport =
        pin!(matter.run(&crypto, &logging_socket, &logging_socket, &logging_socket));

    // Create mDNS socket
    let mdns_socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP)).map_err(|e| {
        error!("Failed to create mDNS socket: {}", e);
        rs_matter::error::ErrorCode::MdnsError
    })?;
    mdns_socket.set_reuse_address(true).map_err(|e| {
        error!("Failed to set SO_REUSEADDR on mDNS socket: {}", e);
        rs_matter::error::ErrorCode::MdnsError
    })?;
    mdns_socket.set_only_v6(false).map_err(|e| {
        error!("Failed to set IPV6_V6ONLY=false on mDNS socket: {}", e);
        rs_matter::error::ErrorCode::MdnsError
    })?;
    mdns_socket.set_nonblocking(true).map_err(|e| {
        error!("Failed to set non-blocking on mDNS socket: {}", e);
        rs_matter::error::ErrorCode::MdnsError
    })?;
    mdns_socket
        .bind(&MDNS_SOCKET_DEFAULT_BIND_ADDR.into())
        .map_err(|e| {
            error!(
                "Failed to bind mDNS socket to {:?}: {}",
                MDNS_SOCKET_DEFAULT_BIND_ADDR, e
            );
            rs_matter::error::ErrorCode::MdnsError
        })?;

    let mdns_socket =
        async_io::Async::<UdpSocket>::new_nonblocking(mdns_socket.into()).map_err(|e| {
            error!("Failed to create async mDNS socket: {}", e);
            rs_matter::error::ErrorCode::MdnsError
        })?;

    // Join multicast groups for mDNS
    mdns_socket
        .get_ref()
        .join_multicast_v6(&MDNS_IPV6_BROADCAST_ADDR, interface_index)
        .map_err(|e| {
            error!("Failed to join IPv6 multicast group: {}", e);
            rs_matter::error::ErrorCode::MdnsError
        })?;
    mdns_socket
        .get_ref()
        .join_multicast_v4(&MDNS_IPV4_BROADCAST_ADDR, &ipv4_addrs[0])
        .map_err(|e| {
            error!("Failed to join IPv4 multicast group: {}", e);
            rs_matter::error::ErrorCode::MdnsError
        })?;

    info!("mDNS socket bound to {:?}", MDNS_SOCKET_DEFAULT_BIND_ADDR);

    let hostname =
        HOSTNAME.get_or_init(|| gethostname::gethostname().to_string_lossy().into_owned());

    let host = Host {
        hostname,
        ip: ipv4_addrs[0],
        ipv6: ipv6_addr,
    };

    let mut mdns_responder = BuiltinMdnsResponder::new();
    let mut mdns = pin!(mdns_responder.run(
        &mdns_socket,
        &mdns_socket,
        &host,
        Some(ipv4_addrs[0].octets().into()),
        Some(interface_index),
        matter,
        &crypto,
    ));

    let mut respond = pin!(responder.run::<4, 4>());
    let mut dm_job = pin!(dm.run());

    // Sensor notification forwarding task
    let mut sensor_forward = pin!(async {
        loop {
            let (endpoint_id, cluster_id) = sensor_notify_ref.receive().await;
            dm.notify_cluster_changed(endpoint_id, cluster_id);
        }
    });

    let mut generic_switch_forward = pin!(async {
        loop {
            generic_switch_notify_ref.wait().await;
            for state in &generic_switch_states {
                let endpoint_id = state.endpoint_id();
                if endpoint_id == 0 {
                    continue;
                }

                for event in state.take_pending_events() {
                    let event_id = event.event_id();
                    if let Err(e) = dm.emit_event(
                        endpoint_id,
                        generic_switch::CLUSTER_ID,
                        event_id,
                        EventPriority::Info,
                        |mut tw| generic_switch::write_event_data(&mut tw, event),
                    ) {
                        error!(
                            "Failed to emit GenericSwitch event 0x{:02x} on endpoint {}: {:?}",
                            event_id, endpoint_id, e
                        );
                    }
                }
            }
        }
    });

    let result = select4(
        &mut transport,
        &mut mdns,
        select(&mut respond, &mut dm_job).coalesce(),
        select(&mut sensor_forward, &mut generic_switch_forward).coalesce(),
    )
    .coalesce()
    .await;

    if let Err(e) = result {
        error!("Matter stack error: {:?}", e);
        return Err(e);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_endpoint_keeps_standard_clusters_when_adding_icd_management() {
        let cluster_ids: Vec<u32> = ROOT_ENDPOINT
            .clusters
            .iter()
            .map(|cluster| cluster.id)
            .collect();

        assert!(cluster_ids.contains(&desc::DescHandler::CLUSTER.id));
        assert!(cluster_ids.contains(&IcdManagementHandler::CLUSTER.id));
    }
}
