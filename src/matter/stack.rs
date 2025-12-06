use std::net::UdpSocket;
use std::pin::pin;

use log::{error, info};

use rs_matter::dm::clusters::basic_info::BasicInfoConfig;
use rs_matter::error::Error;
use rs_matter::pairing::qr::QrTextType;
use rs_matter::pairing::DiscoveryCapabilities;
use rs_matter::respond::DefaultResponder;
use rs_matter::{BasicCommData, Matter, MATTER_PORT};

use crate::config::MatterConfig;

use super::dev_att::HardCodedDevAtt;

/// Device attestation data (static lifetime required by rs-matter)
static DEV_ATT: HardCodedDevAtt = HardCodedDevAtt;

/// Maximum time for commissioning window (seconds)
const MAX_COMM_WINDOW_SECS: u16 = 600;

/// Run the Matter stack
///
/// This function initializes and runs the Matter protocol stack, enabling:
/// - Device discovery via mDNS
/// - Commissioning (pairing) with controllers like Home Assistant
/// - Matter protocol communication
pub async fn run_matter_stack(config: &MatterConfig) -> Result<(), Error> {
    info!("Initializing Matter stack...");

    // Create device details with defaults and override from config
    let dev_det = BasicInfoConfig {
        vid: config.vendor_id,
        pid: config.product_id,
        device_name: "Virtual Doorbell",
        vendor_name: "Development",
        product_name: "Virtual Matter Bridge",
        hw_ver: 1,
        sw_ver: 1,
        serial_no: "VMBRIDGE001",
        ..Default::default()
    };

    // Create commissioning data from config
    let dev_comm = BasicCommData {
        password: config.passcode,
        discriminator: config.discriminator,
    };

    // Create the Matter instance
    let matter = Matter::new_default(&dev_det, dev_comm, &DEV_ATT, MATTER_PORT);

    // Initialize transport buffers
    matter.initialize_transport_buffers()?;

    // Create UDP socket for Matter transport
    let socket = UdpSocket::bind(format!("0.0.0.0:{}", MATTER_PORT))
        .map_err(|e| {
            error!("Failed to bind UDP socket on port {}: {}", MATTER_PORT, e);
            rs_matter::error::ErrorCode::Network
        })?;
    socket.set_nonblocking(true).map_err(|e| {
        error!("Failed to set socket non-blocking: {}", e);
        rs_matter::error::ErrorCode::Network
    })?;

    let socket = async_io::Async::new(socket).map_err(|e| {
        error!("Failed to create async socket: {}", e);
        rs_matter::error::ErrorCode::Network
    })?;

    // Print commissioning info
    info!("Matter device ready for commissioning");
    info!("  Discriminator: {}", config.discriminator);
    info!("  Passcode: {}", config.passcode);

    if let Err(e) = matter.print_standard_qr_text(DiscoveryCapabilities::IP) {
        error!("Failed to print QR text: {:?}", e);
    }

    if let Err(e) = matter.print_standard_qr_code(QrTextType::Unicode, DiscoveryCapabilities::IP) {
        error!("Failed to print QR code: {:?}", e);
    }

    // Create simple responder (handles incoming Matter requests)
    // Note: Full data model with clusters would be added here
    let responder = DefaultResponder::new(&matter, &matter.fabric_mgr, &DEV_ATT);

    info!("Matter stack running. Waiting for controller connections...");

    // Run Matter transport
    let mut transport = pin!(matter.run(&socket, &socket));

    // Run mDNS for discovery (using zbus/D-Bus)
    let mut mdns = pin!(rs_matter::mdns::zbus::run_zbus_mdns(&matter));

    // Run the responder
    let mut respond = pin!(responder.run::<4, 4>());

    // Run all components concurrently using select
    use embassy_futures::select::select3;
    let result = futures_lite::future::block_on(
        select3(&mut transport, &mut mdns, &mut respond).coalesce(),
    );

    if let Err(e) = result {
        error!("Matter stack error: {:?}", e);
        return Err(e);
    }

    Ok(())
}
