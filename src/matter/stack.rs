use super::clusters::{CameraAvStreamMgmtHandler, WebRtcTransportProviderHandler};
use super::device_types::DEV_TYPE_VIDEO_DOORBELL;
use super::logging_udp::LoggingUdpSocket;
use super::netif::{FilteredNetifs, get_interface_name};
use super::subscription_persistence::{SubscriptionStore, run_subscription_resumption};
use crate::clusters::camera_av_stream_mgmt::CameraAvStreamMgmtCluster;
use crate::clusters::webrtc_transport_provider::WebRtcTransportProviderCluster;
use crate::device::on_off_hooks::DoorbellOnOffHooks;
use embassy_futures::select::{select, select4};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use log::{error, info};
use nix::ifaddrs::getifaddrs;
use nix::net::if_::if_nametoindex;
use nix::sys::socket::{AddressFamily, SockaddrLike};
use parking_lot::RwLock as SyncRwLock;
use rs_matter::dm::IMBuffer;
use rs_matter::dm::clusters::desc::{self, ClusterHandler as _};
use rs_matter::dm::clusters::on_off::{self, NoLevelControl, OnOffHooks};
use rs_matter::dm::devices::test::{TEST_DEV_ATT, TEST_DEV_COMM, TEST_DEV_DET};
use rs_matter::dm::endpoints;
use rs_matter::dm::subscriptions::DefaultSubscriptions;
use rs_matter::dm::{
    Async, AsyncHandler, AsyncMetadata, DataModel, Dataver, EmptyHandler, Endpoint, EpClMatcher,
    Node,
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

use crate::config::MatterConfig;

/// Static cells for Matter resources (required for 'static lifetime)
static MATTER: StaticCell<Matter> = StaticCell::new();
static BUFFERS: StaticCell<PooledBuffers<10, NoopRawMutex, IMBuffer>> = StaticCell::new();
static SUBSCRIPTIONS: StaticCell<DefaultSubscriptions> = StaticCell::new();
static PSM: StaticCell<Psm<4096>> = StaticCell::new();

/// Static hostname storage for mDNS (needs 'static lifetime for Host struct)
static HOSTNAME: OnceLock<String> = OnceLock::new();

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
const SUBSCRIPTIONS_FILE: &str = "subscriptions.json";

/// Get the persistence file path
fn get_persist_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(PERSIST_DIR)
        .join(PERSIST_FILE)
}

/// Get the subscriptions persistence file path
fn get_subscriptions_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(PERSIST_DIR)
        .join(SUBSCRIPTIONS_FILE)
}

/// Node definition for our Matter Video Doorbell device
const NODE: Node<'static> = Node {
    id: 0,
    endpoints: &[
        // Endpoint 0: Root endpoint with standard Matter system clusters
        endpoints::root_endpoint(rs_matter::dm::clusters::net_comm::NetworkType::Ethernet),
        // Endpoint 1: Video Doorbell with camera and WebRTC clusters
        Endpoint {
            id: 1,
            device_types: devices!(DEV_TYPE_VIDEO_DOORBELL),
            clusters: clusters!(
                desc::DescHandler::CLUSTER,
                DoorbellOnOffHooks::CLUSTER,
                CameraAvStreamMgmtHandler::CLUSTER,
                WebRtcTransportProviderHandler::CLUSTER
            ),
        },
    ],
};

/// Cached network interface filter (lazily initialized)
static NETIFS: OnceLock<FilteredNetifs> = OnceLock::new();
/// Subscription store (persistent shared state)
static SUBSCRIPTION_STORE: OnceLock<Arc<SubscriptionStore>> = OnceLock::new();

/// Get the network interface filter, auto-detecting if necessary.
fn get_netifs() -> &'static FilteredNetifs {
    NETIFS.get_or_init(FilteredNetifs::auto_detect)
}

/// Build the data model handler with video doorbell clusters
fn dm_handler<'a>(
    matter: &'a Matter<'a>,
    camera_handler: &'a CameraAvStreamMgmtHandler,
    webrtc_handler: &'a WebRtcTransportProviderHandler,
    on_off_handler: &'a on_off::OnOffHandler<'a, &'a DoorbellOnOffHooks, NoLevelControl>,
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
                // Chain handlers for endpoint 1 (video doorbell)
                EmptyHandler
                    // Endpoint 1: Descriptor
                    .chain(
                        EpClMatcher::new(Some(1), Some(desc::DescHandler::CLUSTER.id)),
                        Async(desc::DescHandler::new(Dataver::new_rand(matter.rand())).adapt()),
                    )
                    // Endpoint 1: OnOff (armed/disarmed)
                    .chain(
                        EpClMatcher::new(Some(1), Some(DoorbellOnOffHooks::CLUSTER.id)),
                        on_off::HandlerAsyncAdaptor(on_off_handler),
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
                    ),
            ),
        ),
    )
}

/// Run the Matter stack with video doorbell cluster handlers
///
/// This function initializes and runs the Matter protocol stack, enabling:
/// - Device discovery via mDNS
/// - Commissioning (pairing) with controllers like Home Assistant
/// - Matter protocol communication
///
/// The handlers bridge the existing cluster business logic to rs-matter's data model.
///
/// Note: Currently uses test device credentials for development.
/// TODO: Create proper static device info from MatterConfig
pub async fn run_matter_stack(
    _config: &MatterConfig,
    camera_cluster: Arc<SyncRwLock<CameraAvStreamMgmtCluster>>,
    webrtc_cluster: Arc<SyncRwLock<WebRtcTransportProviderCluster>>,
    on_off_hooks: Arc<DoorbellOnOffHooks>,
) -> Result<(), Error> {
    info!("Initializing Matter stack...");

    // Initialize the Matter instance in static memory
    // Using test device credentials for now (they have 'static lifetime)
    let matter = MATTER.uninit().init_with(Matter::init(
        &TEST_DEV_DET,
        TEST_DEV_COMM,
        &TEST_DEV_ATT,
        rs_matter::utils::epoch::sys_epoch,
        rs_matter::utils::rand::sys_rand,
        MATTER_PORT,
    ));

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

    // Initialize subscription store for persistence
    let subscription_store = SUBSCRIPTION_STORE
        .get_or_init(|| Arc::new(SubscriptionStore::new(get_subscriptions_path())))
        .clone();

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

    // Create handlers with properly randomized Dataver seeds
    // This is critical for subscription/attribute change tracking to work correctly
    let camera_handler =
        CameraAvStreamMgmtHandler::new(Dataver::new_rand(matter.rand()), camera_cluster);
    let webrtc_handler =
        WebRtcTransportProviderHandler::new(Dataver::new_rand(matter.rand()), webrtc_cluster);

    // Create OnOff handler for the doorbell's armed/disarmed state
    // new_standalone calls init internally
    let on_off_handler = on_off::OnOffHandler::new_standalone(
        Dataver::new_rand(matter.rand()),
        1, // endpoint ID
        on_off_hooks.as_ref(),
    );

    // Create the data model with our video doorbell handlers
    let handler = dm_handler(matter, &camera_handler, &webrtc_handler, &on_off_handler);
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

    // Persistence task - uses Psm to automatically save state when it changes
    let mut persist = pin!(psm.run(&persist_path, matter, NO_NETWORKS));

    // Subscription resumption task - monitors for controller reconnection after restart
    // Logs recovery status and detects when CASE sessions are re-established
    let mut sub_resume = pin!(run_subscription_resumption(subscription_store, matter));

    // Run all components concurrently in the same async executor
    // mDNS is included here to avoid RefCell borrow conflicts with Matter's internal state
    let result = select4(
        &mut transport,
        &mut mdns,
        select(&mut respond, &mut dm_job).coalesce(),
        select(&mut persist, &mut sub_resume).coalesce(),
    )
    .coalesce()
    .await;

    if let Err(e) = result {
        error!("Matter stack error: {:?}", e);
        return Err(e);
    }

    Ok(())
}
