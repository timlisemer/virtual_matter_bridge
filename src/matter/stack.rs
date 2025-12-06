use std::net::UdpSocket;
use std::pin::pin;

use embassy_futures::select::{select, select3};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use log::{error, info};
use socket2::{Domain, Protocol, Socket, Type};
use static_cell::StaticCell;

use super::logging_udp::LoggingUdpSocket;
use super::mdns::DirectMdnsResponder;
use super::netif::FilteredNetifs;
use rs_matter::dm::IMBuffer;
use rs_matter::dm::clusters::desc::{self, ClusterHandler as _};
use rs_matter::dm::clusters::net_comm::NetworkType;
use rs_matter::dm::devices::DEV_TYPE_ON_OFF_LIGHT;
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

/// Node definition for our Matter device
/// For now, we expose a simple on/off light endpoint as a minimal working example
/// TODO: Replace with video doorbell device type and clusters
const NODE: Node<'static> = Node {
    id: 0,
    endpoints: &[
        // Endpoint 0: Root endpoint (required for all Matter devices)
        endpoints::root_endpoint(NetworkType::Ethernet),
        // Endpoint 1: Simple on/off light (placeholder until doorbell clusters are implemented)
        Endpoint {
            id: 1,
            device_types: devices!(DEV_TYPE_ON_OFF_LIGHT),
            clusters: clusters!(desc::DescHandler::CLUSTER),
        },
    ],
};

/// The network interface to use for Matter communications.
/// Override with MATTER_INTERFACE env var (e.g., "eth0", "enp14s0").
/// This filters out Thread mesh addresses that may be visible via mDNS reflection.
static NETIFS: FilteredNetifs = FilteredNetifs::new("enp14s0");

/// Build the data model handler
fn dm_handler<'a>(matter: &'a Matter<'a>) -> impl AsyncMetadata + AsyncHandler + 'a {
    (
        NODE,
        endpoints::with_eth(
            &(),
            &NETIFS,
            matter.rand(),
            endpoints::with_sys(
                &false,
                matter.rand(),
                // Chain handlers for our endpoints
                EmptyHandler.chain(
                    EpClMatcher::new(Some(1), Some(desc::DescHandler::CLUSTER.id)),
                    Async(desc::DescHandler::new(Dataver::new_rand(matter.rand())).adapt()),
                ),
            ),
        ),
    )
}

/// Run the Matter stack
///
/// This function initializes and runs the Matter protocol stack, enabling:
/// - Device discovery via mDNS
/// - Commissioning (pairing) with controllers like Home Assistant
/// - Matter protocol communication
///
/// Note: Currently uses test device credentials for development.
/// TODO: Create proper static device info from MatterConfig
pub async fn run_matter_stack(_config: &MatterConfig) -> Result<(), Error> {
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

    // Open the commissioning window to allow pairing
    // This triggers mDNS advertisement
    const COMM_WINDOW_TIMEOUT_SECS: u16 = 900; // 15 minutes
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

    if let Err(e) = matter.print_standard_qr_code(QrTextType::Unicode, DiscoveryCapabilities::IP) {
        error!("Failed to print QR code: {:?}", e);
    }

    // Initialize pooled buffers in static memory
    let buffers = BUFFERS.uninit().init_with(PooledBuffers::init(0));

    // Initialize subscriptions manager in static memory
    let subscriptions = SUBSCRIPTIONS
        .uninit()
        .init_with(DefaultSubscriptions::init());

    // Create the data model with our handler
    let handler = dm_handler(matter);
    let dm = DataModel::new(matter, buffers, subscriptions, handler);

    // Create the responder that handles incoming Matter requests
    let responder = DefaultResponder::new(&dm);

    info!("Matter stack running. Waiting for controller connections...");

    // Run Matter transport with logging wrapper
    let logging_socket = LoggingUdpSocket::new(&socket);
    let mut transport = pin!(matter.run(&logging_socket, &logging_socket));

    // Run mDNS in a separate thread - mdns-sd's daemon blocks async_io's reactor
    // when run in the same async context
    //
    // SAFETY: matter lives for 'static (in StaticCell) and the mDNS thread
    // only reads from it. The thread will be terminated when the process exits.
    let matter_addr = matter as *const Matter<'_> as usize;
    std::thread::Builder::new()
        .name("mdns".into())
        .spawn(move || {
            let matter: &Matter<'_> = unsafe { &*(matter_addr as *const Matter<'_>) };
            if let Err(e) = futures_lite::future::block_on(run_mdns(matter)) {
                log::error!("mDNS error: {:?}", e);
            }
        })
        .expect("Failed to spawn mDNS thread");
    info!("mDNS responder started on separate thread");

    // Run the responder
    let mut respond = pin!(responder.run::<4, 4>());

    // Run data model background job (handles subscriptions)
    let mut dm_job = pin!(dm.run());

    // Run all components concurrently
    // Note: mDNS runs in its own thread to avoid blocking async_io reactor
    let result = select3(
        &mut transport,
        select(&mut respond, &mut dm_job).coalesce(),
        // Placeholder for future persistence
        core::future::pending::<Result<(), Error>>(),
    )
    .coalesce()
    .await;

    if let Err(e) = result {
        error!("Matter stack error: {:?}", e);
        return Err(e);
    }

    Ok(())
}

/// Run mDNS for device discovery using direct multicast (bypasses broken Avahi D-Bus)
async fn run_mdns(matter: &Matter<'_>) -> Result<(), Error> {
    // Use the same interface as FilteredNetifs
    const INTERFACE: &str = "enp14s0";

    DirectMdnsResponder::new(matter, INTERFACE).run().await
}
