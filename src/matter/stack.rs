use std::fs;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::pin::pin;
use std::sync::{Arc, OnceLock};

use embassy_futures::select::{select, select4};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use log::{error, info};
use socket2::{Domain, Protocol, Socket, Type};
use static_cell::StaticCell;

use super::clusters::{CameraAvStreamMgmtHandler, ChimeHandler, WebRtcTransportProviderHandler};
use super::device_types::DEV_TYPE_VIDEO_DOORBELL;
use super::logging_udp::LoggingUdpSocket;
use super::mdns::DirectMdnsResponder;
use super::netif::{FilteredNetifs, get_interface_name};
use crate::clusters::camera_av_stream_mgmt::CameraAvStreamMgmtCluster;
use crate::clusters::chime::ChimeCluster;
use crate::clusters::webrtc_transport_provider::WebRtcTransportProviderCluster;
use parking_lot::RwLock as SyncRwLock;
use rs_matter::dm::IMBuffer;
use rs_matter::dm::clusters::desc::{self, ClusterHandler as _};
use rs_matter::dm::clusters::net_comm::NetworkType;
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
use rs_matter::respond::DefaultResponder;
use rs_matter::transport::MATTER_SOCKET_BIND_ADDR;
use rs_matter::utils::init::InitMaybeUninit;
use rs_matter::utils::select::Coalesce;
use rs_matter::utils::storage::pooled::PooledBuffers;
use rs_matter::{MATTER_PORT, Matter, clusters, devices};

use crate::config::MatterConfig;

/// Static cells for Matter resources (required for 'static lifetime)
static MATTER: StaticCell<Matter> = StaticCell::new();
static BUFFERS: StaticCell<PooledBuffers<10, NoopRawMutex, IMBuffer>> = StaticCell::new();
static SUBSCRIPTIONS: StaticCell<DefaultSubscriptions> = StaticCell::new();

/// Directory for fabric persistence data
const PERSIST_DIR: &str = ".config/virtual-matter-bridge";
const FABRICS_FILE: &str = "fabrics.bin";

/// Get the path to the fabrics persistence file
fn get_fabrics_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(PERSIST_DIR).join(FABRICS_FILE))
}

/// Load fabrics from persistent storage
fn load_fabrics(matter: &Matter) -> Result<bool, Error> {
    let Some(path) = get_fabrics_path() else {
        info!("Could not determine home directory for fabric persistence");
        return Ok(false);
    };

    if !path.exists() {
        info!("No persisted fabrics found at {:?}", path);
        return Ok(false);
    }

    match fs::read(&path) {
        Ok(data) => {
            if data.is_empty() {
                info!("Persisted fabrics file is empty");
                return Ok(false);
            }
            match matter.load_fabrics(&data) {
                Ok(()) => {
                    info!("Loaded fabrics from {:?}", path);
                    Ok(true)
                }
                Err(e) => {
                    error!("Failed to parse persisted fabrics: {:?}", e);
                    // Don't fail startup, just start fresh
                    Ok(false)
                }
            }
        }
        Err(e) => {
            error!("Failed to read fabrics file {:?}: {}", path, e);
            Ok(false)
        }
    }
}

/// Save fabrics to persistent storage
fn save_fabrics(matter: &Matter) -> Result<(), Error> {
    let Some(path) = get_fabrics_path() else {
        error!("Could not determine home directory for fabric persistence");
        return Ok(());
    };

    // Ensure directory exists
    if let Some(parent) = path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        error!("Failed to create persistence directory {:?}: {}", parent, e);
        return Ok(());
    }

    // Store fabrics - use a reasonably large buffer
    let mut buf = vec![0u8; 8192];
    match matter.store_fabrics(&mut buf) {
        Ok(len) => {
            buf.truncate(len);
            if let Err(e) = fs::write(&path, &buf) {
                error!("Failed to write fabrics to {:?}: {}", path, e);
            } else {
                info!("Saved fabrics to {:?} ({} bytes)", path, len);
            }
        }
        Err(e) => {
            error!("Failed to serialize fabrics: {:?}", e);
        }
    }

    Ok(())
}

/// Node definition for our Matter Video Doorbell device
const NODE: Node<'static> = Node {
    id: 0,
    endpoints: &[
        // Endpoint 0: Root endpoint (required for all Matter devices)
        endpoints::root_endpoint(NetworkType::Ethernet),
        // Endpoint 1: Video Doorbell with camera, WebRTC, and chime clusters
        Endpoint {
            id: 1,
            device_types: devices!(DEV_TYPE_VIDEO_DOORBELL),
            clusters: clusters!(
                desc::DescHandler::CLUSTER,
                CameraAvStreamMgmtHandler::CLUSTER,
                WebRtcTransportProviderHandler::CLUSTER,
                ChimeHandler::CLUSTER
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

/// Build the data model handler with video doorbell clusters
fn dm_handler<'a>(
    matter: &'a Matter<'a>,
    camera_handler: &'a CameraAvStreamMgmtHandler,
    webrtc_handler: &'a WebRtcTransportProviderHandler,
    chime_handler: &'a ChimeHandler,
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
                // Chain handlers for endpoint 1 clusters
                EmptyHandler
                    .chain(
                        EpClMatcher::new(Some(1), Some(desc::DescHandler::CLUSTER.id)),
                        Async(desc::DescHandler::new(Dataver::new_rand(matter.rand())).adapt()),
                    )
                    .chain(
                        EpClMatcher::new(Some(1), Some(CameraAvStreamMgmtHandler::CLUSTER.id)),
                        Async(camera_handler),
                    )
                    .chain(
                        EpClMatcher::new(Some(1), Some(WebRtcTransportProviderHandler::CLUSTER.id)),
                        Async(webrtc_handler),
                    )
                    .chain(
                        EpClMatcher::new(Some(1), Some(ChimeHandler::CLUSTER.id)),
                        Async(chime_handler),
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
    chime_cluster: Arc<SyncRwLock<ChimeCluster>>,
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

    // Create UDP socket for Matter transport using socket2 for proper IPv6 dual-stack setup
    // This ensures IPV6_V6ONLY is set to false, allowing the socket to receive both IPv4 and IPv6
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
    raw_socket
        .bind(&MATTER_SOCKET_BIND_ADDR.into())
        .map_err(|e| {
            error!(
                "Failed to bind UDP socket on {:?}: {}",
                MATTER_SOCKET_BIND_ADDR, e
            );
            rs_matter::error::ErrorCode::StdIoError
        })?;
    let socket = async_io::Async::<UdpSocket>::new(raw_socket.into()).map_err(|e| {
        error!("Failed to create async socket: {}", e);
        rs_matter::error::ErrorCode::StdIoError
    })?;
    info!("Matter UDP socket bound to {:?}", MATTER_SOCKET_BIND_ADDR);

    // Try to load existing fabrics from persistent storage
    let was_commissioned = load_fabrics(matter)?;

    // Only open commissioning window if device is not already commissioned
    const COMM_WINDOW_TIMEOUT_SECS: u16 = 900; // 15 minutes
    if was_commissioned {
        info!("Device already commissioned, skipping commissioning window");
        info!("  (Delete {:?} to reset commissioning)", get_fabrics_path());
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
    let chime_handler = ChimeHandler::new(Dataver::new_rand(matter.rand()), chime_cluster);

    // Create the data model with our video doorbell handlers
    let handler = dm_handler(matter, &camera_handler, &webrtc_handler, &chime_handler);
    let dm = DataModel::new(matter, buffers, subscriptions, handler);

    // Create the responder that handles incoming Matter requests
    let responder = DefaultResponder::new(&dm);

    info!("Matter stack running. Waiting for controller connections...");

    // Run Matter transport with logging wrapper
    let logging_socket = LoggingUdpSocket::new(&socket);
    let mut transport = pin!(matter.run(&logging_socket, &logging_socket));

    // Create mDNS responder - runs in the same async executor as Matter stack
    // This avoids RefCell borrow conflicts that occur when mDNS runs in a separate thread
    // (mdns-sd's ServiceDaemon spawns its own internal thread for multicast I/O)
    let mut mdns_responder = DirectMdnsResponder::new(matter, get_interface_name());
    let mut mdns = pin!(mdns_responder.run());

    // Run the responder
    let mut respond = pin!(responder.run::<4, 4>());

    // Run data model background job (handles subscriptions)
    let mut dm_job = pin!(dm.run());

    // Persistence task - saves fabrics when they change
    let persist_task = async {
        loop {
            // Wait for notification that something changed
            matter.wait_persist().await;

            // Check if fabrics changed and need to be persisted
            if matter.fabrics_changed() {
                info!("Fabrics changed, persisting...");
                if let Err(e) = save_fabrics(matter) {
                    error!("Failed to persist fabrics: {:?}", e);
                }
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), Error>(())
    };
    let mut persist = pin!(persist_task);

    // Run all components concurrently in the same async executor
    // mDNS is included here to avoid RefCell borrow conflicts with Matter's internal state
    let result = select4(
        &mut transport,
        &mut mdns,
        select(&mut respond, &mut dm_job).coalesce(),
        &mut persist,
    )
    .coalesce()
    .await;

    if let Err(e) = result {
        error!("Matter stack error: {:?}", e);
        return Err(e);
    }

    Ok(())
}
