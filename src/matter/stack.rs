use super::clusters::camera_av_stream_mgmt::CameraAvStreamMgmtCluster;
use super::clusters::webrtc_transport_provider::WebRtcTransportProviderCluster;
use super::clusters::{
    BooleanStateHandler, BridgedHandler, CameraAvStreamMgmtHandler, OccupancySensingHandler,
    TimeSyncHandler, WebRtcTransportProviderHandler,
};
// DEV_INFO temporarily replaced with TEST_DEV_DET for label debugging (Iteration 2)
// use super::device_info::DEV_INFO;
use super::device_types::{
    DEV_TYPE_AGGREGATOR, DEV_TYPE_BRIDGED_NODE, DEV_TYPE_CONTACT_SENSOR, DEV_TYPE_OCCUPANCY_SENSOR,
    DEV_TYPE_ON_OFF_LIGHT, DEV_TYPE_ON_OFF_PLUG_IN_UNIT, DEV_TYPE_VIDEO_DOORBELL,
};
use super::endpoints::controls::{LightSwitch, Switch};
use super::logging_udp::LoggingUdpSocket;
use super::netif::{FilteredNetifs, get_interface_name};
use embassy_futures::select::{select, select4};
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embassy_sync::signal::Signal;
use log::{error, info};
use nix::ifaddrs::getifaddrs;
use nix::net::if_::if_nametoindex;
use nix::sys::socket::{AddressFamily, SockaddrLike};
use parking_lot::RwLock as SyncRwLock;
use rs_matter::dm::IMBuffer;
use rs_matter::dm::clusters::desc::{self, ClusterHandler as _, PartsMatcher};
use rs_matter::dm::clusters::on_off::{self, NoLevelControl, OnOffHooks};
use rs_matter::dm::devices;
use rs_matter::dm::devices::test::{TEST_DEV_ATT, TEST_DEV_COMM, TEST_DEV_DET};
use rs_matter::dm::endpoints;
use rs_matter::dm::subscriptions::DefaultSubscriptions;
use rs_matter::dm::{
    Async, AsyncHandler, AsyncMetadata, Cluster, DataModel, Dataver, EmptyHandler, Endpoint,
    EpClMatcher, Node,
};
use rs_matter::error::Error;
use rs_matter::pairing::DiscoveryCapabilities;
use rs_matter::pairing::qr::QrTextType;
use rs_matter::persist::{NO_NETWORKS, Psm};
use rs_matter::respond::DefaultResponder;
use rs_matter::transport::network::mdns::builtin::{BuiltinMdnsResponder, Host};
use rs_matter::transport::network::mdns::{
    MDNS_IPV4_BROADCAST_ADDR, MDNS_IPV6_BROADCAST_ADDR, MDNS_SOCKET_DEFAULT_BIND_ADDR,
};
use rs_matter::utils::init::InitMaybeUninit;
use rs_matter::utils::select::Coalesce;
use rs_matter::utils::storage::pooled::PooledBuffers;
use rs_matter::{MATTER_PORT, Matter, clusters, devices};
use socket2::{Domain, Protocol, Socket, Type};
use static_cell::StaticCell;
use std::ffi::CString;
use std::fs;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::pin::pin;
use std::sync::{Arc, OnceLock};

use super::clusters::{boolean_state, occupancy_sensing};
use super::endpoints::{ClusterNotifier, ContactSensor, NotifiableSensor, OccupancySensor};
use crate::config::MatterConfig;

/// Static cells for Matter resources (required for 'static lifetime)
static MATTER: StaticCell<Matter> = StaticCell::new();
static BUFFERS: StaticCell<PooledBuffers<10, NoopRawMutex, IMBuffer>> = StaticCell::new();
static SUBSCRIPTIONS: StaticCell<DefaultSubscriptions> = StaticCell::new();
static PSM: StaticCell<Psm<4096>> = StaticCell::new();
/// Signal for sensor change notifications (wakes subscription processor)
/// Uses CriticalSectionRawMutex because sensors are updated from different threads
static SENSOR_NOTIFY: StaticCell<Signal<CriticalSectionRawMutex, ()>> = StaticCell::new();

/// Static hostname storage for mDNS (needs 'static lifetime for Host struct)
static HOSTNAME: OnceLock<String> = OnceLock::new();

/// PartsMatcher for Power Strip composed device (endpoint 5).
/// Returns child endpoints [6, 7] as parts when queried from endpoint 5.
#[derive(Debug)]
struct PowerStripPartsMatcher;

impl PartsMatcher for PowerStripPartsMatcher {
    fn matches(&self, our_endpoint: u16, endpoint: u16) -> bool {
        // Only return parts when queried from endpoint 5 (Power Strip parent)
        // Child endpoints are 6 (Outlet 1) and 7 (Outlet 2)
        our_endpoint == 5 && (endpoint == 6 || endpoint == 7)
    }
}

/// Get IPv4 and IPv6 addresses for a specific network interface.
///
/// Filters out link-local IPv6 addresses (fe80::/10).
fn get_interface_addresses(interface_name: &str) -> Result<(Vec<Ipv4Addr>, Vec<Ipv6Addr>), Error> {
    let addrs = getifaddrs().map_err(|e| {
        error!("Failed to get interface addresses: {:?}", e);
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
                        // Filter out link-local addresses (fe80::/10)
                        let octets = ip.octets();
                        if !(octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80) {
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

/// Get the persistence file path
fn get_persist_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(PERSIST_DIR)
        .join(PERSIST_FILE)
}

/// Root endpoint cluster list with Time Synchronization added
const ROOT_CLUSTERS: &[Cluster<'static>] = clusters!(eth; TimeSyncHandler::CLUSTER);

/// Node definition for our Matter Bridge device
///
/// Architecture:
/// - Endpoint 0: Root node (standard)
/// - Endpoint 1: Virtual Matter Bridge (root device, NOT bridged)
/// - Endpoint 2: Aggregator (bridge root - enumerates bridged devices)
/// - Endpoints 3+: Bridged devices (each with BridgedDeviceBasicInformation)
const NODE: Node<'static> = Node {
    id: 0,
    endpoints: &[
        // Endpoint 0: Root node
        Endpoint {
            id: endpoints::ROOT_ENDPOINT_ID,
            device_types: devices!(devices::DEV_TYPE_ROOT_NODE),
            clusters: ROOT_CLUSTERS,
        },
        // Endpoint 1: Virtual Matter Bridge (root device, not bridged)
        Endpoint {
            id: 1,
            device_types: devices!(DEV_TYPE_VIDEO_DOORBELL),
            clusters: clusters!(
                desc::DescHandler::CLUSTER,
                Switch::CLUSTER,
                CameraAvStreamMgmtHandler::CLUSTER,
                WebRtcTransportProviderHandler::CLUSTER
            ),
        },
        // Endpoint 2: Aggregator (bridge root)
        Endpoint {
            id: 2,
            device_types: devices!(DEV_TYPE_AGGREGATOR),
            clusters: clusters!(desc::DescHandler::CLUSTER),
        },
        // Endpoint 3: Contact Sensor (bridged) - "Door"
        Endpoint {
            id: 3,
            device_types: devices!(DEV_TYPE_CONTACT_SENSOR, DEV_TYPE_BRIDGED_NODE),
            clusters: clusters!(
                desc::DescHandler::CLUSTER,
                BridgedHandler::CLUSTER,
                BooleanStateHandler::CLUSTER
            ),
        },
        // Endpoint 4: Occupancy Sensor (bridged) - "Motion"
        Endpoint {
            id: 4,
            device_types: devices!(DEV_TYPE_OCCUPANCY_SENSOR, DEV_TYPE_BRIDGED_NODE),
            clusters: clusters!(
                desc::DescHandler::CLUSTER,
                BridgedHandler::CLUSTER,
                OccupancySensingHandler::CLUSTER
            ),
        },
        // Endpoint 5: Power Strip parent (composed device, no OnOff - children have it)
        Endpoint {
            id: 5,
            device_types: devices!(DEV_TYPE_ON_OFF_PLUG_IN_UNIT, DEV_TYPE_BRIDGED_NODE),
            clusters: clusters!(desc::DescHandler::CLUSTER, BridgedHandler::CLUSTER),
        },
        // Endpoint 6: Outlet 1 (child of Power Strip)
        // Note: No DEV_TYPE_BRIDGED_NODE - children are discovered via parent's PartsList
        Endpoint {
            id: 6,
            device_types: devices!(DEV_TYPE_ON_OFF_PLUG_IN_UNIT),
            clusters: clusters!(
                desc::DescHandler::CLUSTER,
                BridgedHandler::CLUSTER,
                Switch::CLUSTER
            ),
        },
        // Endpoint 7: Outlet 2 (child of Power Strip)
        // Note: No DEV_TYPE_BRIDGED_NODE - children are discovered via parent's PartsList
        Endpoint {
            id: 7,
            device_types: devices!(DEV_TYPE_ON_OFF_PLUG_IN_UNIT),
            clusters: clusters!(
                desc::DescHandler::CLUSTER,
                BridgedHandler::CLUSTER,
                Switch::CLUSTER
            ),
        },
        // Endpoint 8: On/Off Light (bridged) - "Light"
        Endpoint {
            id: 8,
            device_types: devices!(DEV_TYPE_ON_OFF_LIGHT, DEV_TYPE_BRIDGED_NODE),
            clusters: clusters!(
                desc::DescHandler::CLUSTER,
                BridgedHandler::CLUSTER,
                LightSwitch::CLUSTER
            ),
        },
    ],
};

/// Cached network interface filter (lazily initialized)
static NETIFS: OnceLock<FilteredNetifs> = OnceLock::new();

/// Get the network interface filter, auto-detecting if necessary.
fn get_netifs() -> &'static FilteredNetifs {
    NETIFS.get_or_init(FilteredNetifs::auto_detect)
}

/// Build the data model handler for the Matter bridge
#[allow(clippy::too_many_arguments)]
fn dm_handler<'a>(
    matter: &'a Matter<'a>,
    camera_handler: &'a CameraAvStreamMgmtHandler,
    webrtc_handler: &'a WebRtcTransportProviderHandler,
    master_onoff_handler: &'a on_off::OnOffHandler<'a, &'a Switch, NoLevelControl>,
    time_sync_handler: &'a TimeSyncHandler,
    boolean_state_handler: &'a BooleanStateHandler,
    occupancy_sensing_handler: &'a OccupancySensingHandler,
    switch1_handler: &'a on_off::OnOffHandler<'a, &'a Switch, NoLevelControl>,
    switch2_handler: &'a on_off::OnOffHandler<'a, &'a Switch, NoLevelControl>,
    light_handler: &'a on_off::OnOffHandler<'a, &'a LightSwitch, NoLevelControl>,
    power_strip_matcher: &'a PowerStripPartsMatcher,
    label_door: &'a BridgedHandler,
    label_motion: &'a BridgedHandler,
    label_power_strip: &'a BridgedHandler,
    label_outlet_1: &'a BridgedHandler,
    label_outlet_2: &'a BridgedHandler,
    label_light: &'a BridgedHandler,
) -> impl AsyncMetadata + AsyncHandler + 'a {
    (
        NODE,
        endpoints::with_eth(
            &(),
            get_netifs(),
            matter.rand(),
            endpoints::with_sys(
                &false,
                matter.rand(),
                // Chain handlers for all endpoints
                EmptyHandler
                    // Endpoint 0: Time Synchronization (read-only stub)
                    .chain(
                        EpClMatcher::new(Some(0), Some(TimeSyncHandler::CLUSTER.id)),
                        Async(time_sync_handler),
                    )
                    // Endpoint 1: Descriptor (Virtual Matter Bridge - root device)
                    .chain(
                        EpClMatcher::new(Some(1), Some(desc::DescHandler::CLUSTER.id)),
                        Async(desc::DescHandler::new(Dataver::new_rand(matter.rand())).adapt()),
                    )
                    // Endpoint 1: OnOff (master on/off for all sub-devices)
                    .chain(
                        EpClMatcher::new(Some(1), Some(Switch::CLUSTER.id)),
                        on_off::HandlerAsyncAdaptor(master_onoff_handler),
                    )
                    // Endpoint 1: Camera AV Stream Management
                    .chain(
                        EpClMatcher::new(Some(1), Some(CameraAvStreamMgmtHandler::CLUSTER.id)),
                        Async(camera_handler),
                    )
                    // Endpoint 1: WebRTC Transport Provider
                    .chain(
                        EpClMatcher::new(Some(1), Some(WebRtcTransportProviderHandler::CLUSTER.id)),
                        Async(webrtc_handler),
                    )
                    // Endpoint 2: Aggregator Descriptor (uses new_aggregator for bridge)
                    .chain(
                        EpClMatcher::new(Some(2), Some(desc::DescHandler::CLUSTER.id)),
                        Async(
                            desc::DescHandler::new_aggregator(Dataver::new_rand(matter.rand()))
                                .adapt(),
                        ),
                    )
                    // Endpoint 3: Descriptor (Contact Sensor)
                    .chain(
                        EpClMatcher::new(Some(3), Some(desc::DescHandler::CLUSTER.id)),
                        Async(desc::DescHandler::new(Dataver::new_rand(matter.rand())).adapt()),
                    )
                    // Endpoint 3: BridgedDeviceBasicInformation
                    .chain(
                        EpClMatcher::new(Some(3), Some(BridgedHandler::CLUSTER.id)),
                        Async(label_door),
                    )
                    // Endpoint 3: BooleanState (contact sensor)
                    .chain(
                        EpClMatcher::new(Some(3), Some(BooleanStateHandler::CLUSTER.id)),
                        Async(boolean_state_handler),
                    )
                    // Endpoint 4: Descriptor (Occupancy Sensor)
                    .chain(
                        EpClMatcher::new(Some(4), Some(desc::DescHandler::CLUSTER.id)),
                        Async(desc::DescHandler::new(Dataver::new_rand(matter.rand())).adapt()),
                    )
                    // Endpoint 4: BridgedDeviceBasicInformation
                    .chain(
                        EpClMatcher::new(Some(4), Some(BridgedHandler::CLUSTER.id)),
                        Async(label_motion),
                    )
                    // Endpoint 4: OccupancySensing
                    .chain(
                        EpClMatcher::new(Some(4), Some(OccupancySensingHandler::CLUSTER.id)),
                        Async(occupancy_sensing_handler),
                    )
                    // Endpoint 5: Descriptor (Power Strip parent - composed device with PartsList=[6,7])
                    .chain(
                        EpClMatcher::new(Some(5), Some(desc::DescHandler::CLUSTER.id)),
                        Async(
                            desc::DescHandler::new_matching(
                                Dataver::new_rand(matter.rand()),
                                power_strip_matcher,
                            )
                            .adapt(),
                        ),
                    )
                    // Endpoint 5: BridgedDeviceBasicInformation
                    .chain(
                        EpClMatcher::new(Some(5), Some(BridgedHandler::CLUSTER.id)),
                        Async(label_power_strip),
                    )
                    // Endpoint 6: Descriptor (Outlet 1)
                    .chain(
                        EpClMatcher::new(Some(6), Some(desc::DescHandler::CLUSTER.id)),
                        Async(desc::DescHandler::new(Dataver::new_rand(matter.rand())).adapt()),
                    )
                    // Endpoint 6: BridgedDeviceBasicInformation
                    .chain(
                        EpClMatcher::new(Some(6), Some(BridgedHandler::CLUSTER.id)),
                        Async(label_outlet_1),
                    )
                    // Endpoint 6: OnOff (switch 1)
                    .chain(
                        EpClMatcher::new(Some(6), Some(Switch::CLUSTER.id)),
                        on_off::HandlerAsyncAdaptor(switch1_handler),
                    )
                    // Endpoint 7: Descriptor (Outlet 2)
                    .chain(
                        EpClMatcher::new(Some(7), Some(desc::DescHandler::CLUSTER.id)),
                        Async(desc::DescHandler::new(Dataver::new_rand(matter.rand())).adapt()),
                    )
                    // Endpoint 7: BridgedDeviceBasicInformation
                    .chain(
                        EpClMatcher::new(Some(7), Some(BridgedHandler::CLUSTER.id)),
                        Async(label_outlet_2),
                    )
                    // Endpoint 7: OnOff (switch 2)
                    .chain(
                        EpClMatcher::new(Some(7), Some(Switch::CLUSTER.id)),
                        on_off::HandlerAsyncAdaptor(switch2_handler),
                    )
                    // Endpoint 8: Descriptor (Light)
                    .chain(
                        EpClMatcher::new(Some(8), Some(desc::DescHandler::CLUSTER.id)),
                        Async(desc::DescHandler::new(Dataver::new_rand(matter.rand())).adapt()),
                    )
                    // Endpoint 8: BridgedDeviceBasicInformation
                    .chain(
                        EpClMatcher::new(Some(8), Some(BridgedHandler::CLUSTER.id)),
                        Async(label_light),
                    )
                    // Endpoint 8: OnOff (light)
                    .chain(
                        EpClMatcher::new(Some(8), Some(LightSwitch::CLUSTER.id)),
                        on_off::HandlerAsyncAdaptor(light_handler),
                    ),
            ),
        ),
    )
}

/// Run the Matter stack with bridge cluster handlers
///
/// This function initializes and runs the Matter protocol stack, enabling:
/// - Device discovery via mDNS
/// - Commissioning (pairing) with controllers like Home Assistant
/// - Matter protocol communication
///
/// The handlers bridge the existing cluster business logic to rs-matter's data model.
///
/// Note: Currently uses test device credentials for development.
#[allow(clippy::too_many_arguments)]
pub async fn run_matter_stack(
    _config: &MatterConfig,
    camera_cluster: Arc<SyncRwLock<CameraAvStreamMgmtCluster>>,
    webrtc_cluster: Arc<SyncRwLock<WebRtcTransportProviderCluster>>,
    master_onoff: Arc<Switch>,
    contact_sensor: Arc<ContactSensor>,
    occupancy_sensor: Arc<OccupancySensor>,
    switch1: Arc<Switch>,
    switch2: Arc<Switch>,
    light: Arc<LightSwitch>,
) -> Result<(), Error> {
    info!("Initializing Matter stack...");

    // Initialize the Matter instance in static memory
    // Using TEST_DEV_DET temporarily for label debugging (Iteration 2)
    let matter = MATTER.uninit().init_with(Matter::init(
        &TEST_DEV_DET,
        TEST_DEV_COMM,
        &TEST_DEV_ATT,
        rs_matter::utils::epoch::sys_epoch,
        rs_matter::utils::rand::sys_rand,
        MATTER_PORT,
    ));
    // Use shared reference going forward (avoid moving the &mut)
    let matter: &'static Matter = &*matter;

    // Initialize transport buffers
    matter.initialize_transport_buffers()?;

    // Detect network interface and get addresses BEFORE socket creation
    // This is critical because we need to bind to the specific IPv6 address we advertise in mDNS
    // to ensure the kernel uses the same source address when sending responses
    let interface_name = get_interface_name();
    let interface_index = get_interface_index(interface_name)?;
    let (ipv4_addrs, ipv6_addrs) = get_interface_addresses(interface_name)?;

    if ipv4_addrs.is_empty() {
        error!("No IPv4 address found on interface '{}'", interface_name);
        return Err(rs_matter::error::ErrorCode::MdnsError.into());
    }

    // Need at least one IPv6 address for Matter
    let ipv6_addr = if ipv6_addrs.is_empty() {
        info!(
            "No global IPv6 address on '{}', using unspecified",
            interface_name
        );
        Ipv6Addr::UNSPECIFIED
    } else {
        ipv6_addrs[0]
    };

    info!(
        "Using interface '{}' (index {}) with {} and {}",
        interface_name, interface_index, ipv4_addrs[0], ipv6_addr
    );

    // Create UDP socket for Matter transport using socket2 for proper IPv6 dual-stack setup
    // CRITICAL: Bind to the specific IPv6 address we advertise in mDNS, NOT to [::]
    // This ensures responses come from the same source IP that HA sent packets to,
    // which is required for multi-admin commissioning to work
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

    // Bind to specific IPv6 address (with IPV6_V6ONLY=false, IPv4 will use IPv6-mapped addresses)
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

    // Initialize Psm (Persistent State Manager) and load existing state
    let persist_path = get_persist_path();

    // Ensure the persistence directory exists
    if let Some(parent) = persist_path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        error!("Failed to create persistence directory {:?}: {}", parent, e);
    }

    let psm = PSM.uninit().init_with(Psm::init());
    if let Err(e) = psm.load(&persist_path, matter, NO_NETWORKS) {
        error!(
            "Failed to load persisted state from {:?}: {:?}",
            persist_path, e
        );
        // Continue anyway - will start fresh
    }

    // Only open commissioning window if device is not already commissioned
    const COMM_WINDOW_TIMEOUT_SECS: u16 = 900; // 15 minutes
    if matter.is_commissioned() {
        info!("Device already commissioned, skipping commissioning window");
        info!("  (Delete {:?} to reset commissioning)", persist_path);
    } else {
        info!(
            "Opening commissioning window for {} seconds...",
            COMM_WINDOW_TIMEOUT_SECS
        );
        matter.open_basic_comm_window(COMM_WINDOW_TIMEOUT_SECS)?;

        // Print commissioning info
        info!("Matter device ready for commissioning");
        info!("  Discriminator: {}", TEST_DEV_COMM.discriminator);
        info!("  Passcode: {}", TEST_DEV_COMM.password);

        if let Err(e) = matter.print_standard_qr_text(DiscoveryCapabilities::IP) {
            error!("Failed to print QR text: {:?}", e);
        }

        if let Err(e) =
            matter.print_standard_qr_code(QrTextType::Unicode, DiscoveryCapabilities::IP)
        {
            error!("Failed to print QR code: {:?}", e);
        }
    }

    // Initialize pooled buffers in static memory
    let buffers = BUFFERS.uninit().init_with(PooledBuffers::init(0));

    // Initialize subscriptions manager in static memory
    let subscriptions = SUBSCRIPTIONS
        .uninit()
        .init_with(DefaultSubscriptions::init());

    // Initialize sensor notification signal (thread-safe for cross-thread updates)
    let sensor_notify = SENSOR_NOTIFY.uninit().init_with(Signal::new());
    let sensor_notify_ref: &'static Signal<CriticalSectionRawMutex, ()> = sensor_notify;

    // Wire up the contact sensor to push live updates to Matter subscriptions
    // When sensor.set() or sensor.toggle() is called, it signals sensor_notify,
    // which wakes the forwarding task to call subscriptions.notify_cluster_changed()
    contact_sensor.set_notifier(ClusterNotifier::new(
        sensor_notify_ref,
        3, // endpoint_id for Contact Sensor (was 2, now 3 due to Aggregator at 1)
        boolean_state::CLUSTER_ID,
    ));

    // Wire up the occupancy sensor for live updates
    occupancy_sensor.set_notifier(ClusterNotifier::new(
        sensor_notify_ref,
        4, // endpoint_id for Occupancy Sensor (was 3, now 4)
        occupancy_sensing::CLUSTER_ID,
    ));

    // Create handlers with properly randomized Dataver seeds
    // This is critical for subscription/attribute change tracking to work correctly
    let camera_handler =
        CameraAvStreamMgmtHandler::new(Dataver::new_rand(matter.rand()), camera_cluster);
    let webrtc_handler =
        WebRtcTransportProviderHandler::new(Dataver::new_rand(matter.rand()), webrtc_cluster);
    let time_sync_handler = TimeSyncHandler::new(Dataver::new_rand(matter.rand()));
    let boolean_state_handler =
        BooleanStateHandler::new(Dataver::new_rand(matter.rand()), contact_sensor);
    let occupancy_sensing_handler =
        OccupancySensingHandler::new(Dataver::new_rand(matter.rand()), occupancy_sensor);

    // Create OnOff handler for master on/off (endpoint 1 - controls all sub-devices)
    let master_onoff_handler = on_off::OnOffHandler::new_standalone(
        Dataver::new_rand(matter.rand()),
        1, // endpoint ID
        master_onoff.as_ref(),
    );

    // Create OnOff handler for switch 1 (endpoint 6 - Outlet 1)
    let switch1_handler = on_off::OnOffHandler::new_standalone(
        Dataver::new_rand(matter.rand()),
        6, // endpoint ID
        switch1.as_ref(),
    );

    // Create OnOff handler for switch 2 (endpoint 7 - Outlet 2)
    let switch2_handler = on_off::OnOffHandler::new_standalone(
        Dataver::new_rand(matter.rand()),
        7, // endpoint ID
        switch2.as_ref(),
    );

    // Create OnOff handler for light (endpoint 8)
    let light_handler = on_off::OnOffHandler::new_standalone(
        Dataver::new_rand(matter.rand()),
        8, // endpoint ID
        light.as_ref(),
    );

    // Create PartsMatcher for Power Strip composed device
    let power_strip_matcher = PowerStripPartsMatcher;

    // Create BridgedHandler for endpoint names (via BridgedDeviceBasicInformation.NodeLabel)
    // Note: EP1 (Virtual Matter Bridge) and EP2 (Aggregator) are not bridged
    let label_door = BridgedHandler::new(Dataver::new_rand(matter.rand()), "Door");
    let label_motion = BridgedHandler::new(Dataver::new_rand(matter.rand()), "Motion");
    let label_power_strip = BridgedHandler::new(Dataver::new_rand(matter.rand()), "Power Strip");
    let label_outlet_1 = BridgedHandler::new(Dataver::new_rand(matter.rand()), "Outlet 1");
    let label_outlet_2 = BridgedHandler::new(Dataver::new_rand(matter.rand()), "Outlet 2");
    let label_light = BridgedHandler::new(Dataver::new_rand(matter.rand()), "Light");

    // Create the data model with our bridge handlers
    let handler = dm_handler(
        matter,
        &camera_handler,
        &webrtc_handler,
        &master_onoff_handler,
        &time_sync_handler,
        &boolean_state_handler,
        &occupancy_sensing_handler,
        &switch1_handler,
        &switch2_handler,
        &light_handler,
        &power_strip_matcher,
        &label_door,
        &label_motion,
        &label_power_strip,
        &label_outlet_1,
        &label_outlet_2,
        &label_light,
    );
    let dm = DataModel::new(matter, buffers, subscriptions, handler);

    // Create the responder that handles incoming Matter requests
    let responder = DefaultResponder::new(&dm);

    info!("Matter stack running. Waiting for controller connections...");

    // Run Matter transport with logging wrapper
    let logging_socket = LoggingUdpSocket::new(&socket);
    let mut transport = pin!(matter.run(&logging_socket, &logging_socket));

    // Create mDNS responder using rs-matter's built-in implementation
    // This correctly handles subtype PTR queries (e.g., _S1._sub._matterc._udp.local.)
    // which is required for multi-admin commissioning via phone apps
    // Note: interface_name, interface_index, ipv4_addrs, ipv6_addr are already set above

    // Create mDNS socket (separate from Matter transport, binds to port 5353)
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

    // Store hostname statically (Host struct requires 'static lifetime)
    let hostname =
        HOSTNAME.get_or_init(|| gethostname::gethostname().to_string_lossy().into_owned());

    // Create host info for mDNS responder
    let host = Host {
        id: 0,
        hostname,
        ip: ipv4_addrs[0].octets().into(),
        ipv6: ipv6_addr.octets().into(),
    };

    // Use built-in mDNS responder - correctly handles subtype queries
    let mdns_responder = BuiltinMdnsResponder::new(matter);
    let mut mdns = pin!(mdns_responder.run(
        &mdns_socket,
        &mdns_socket,
        &host,
        Some(ipv4_addrs[0].octets().into()),
        Some(interface_index),
    ));

    // Run the responder
    let mut respond = pin!(responder.run::<4, 4>());

    // Run data model background job (handles subscriptions)
    let mut dm_job = pin!(dm.run());

    // Persistence task - saves Matter state to disk when signaled
    let persist_path_clone = persist_path.clone();
    let matter_ref = matter;
    let mut persist = pin!(async move {
        loop {
            matter_ref.wait_persist().await;
            if let Err(e) = psm.store(&persist_path_clone, matter_ref, NO_NETWORKS) {
                error!("Failed to store persisted state: {:?}", e);
            }
        }
    });

    // Sensor notification forwarding task - bridges sensor signals to Matter subscriptions
    // When a sensor calls notify(), this task wakes up and triggers subscription reports
    // Note: Both sensors share the same signal, so we notify both clusters each time.
    // The dataver check in each handler ensures only actually-changed values trigger reports.
    let mut sensor_forward = pin!(async {
        loop {
            sensor_notify_ref.wait().await;
            // Notify subscriptions for Contact Sensor (endpoint 3 - was 2)
            subscriptions.notify_cluster_changed(3, boolean_state::CLUSTER_ID);
            // Notify subscriptions for Occupancy Sensor (endpoint 4 - was 3)
            subscriptions.notify_cluster_changed(4, occupancy_sensing::CLUSTER_ID);
        }
    });

    // Run all components concurrently in the same async executor
    // mDNS is included here to avoid RefCell borrow conflicts with Matter's internal state
    let result = select4(
        &mut transport,
        &mut mdns,
        select(&mut respond, &mut dm_job).coalesce(),
        select(&mut persist, &mut sensor_forward).coalesce(),
    )
    .coalesce()
    .await;

    if let Err(e) = result {
        error!("Matter stack error: {:?}", e);
        return Err(e);
    }

    Ok(())
}
