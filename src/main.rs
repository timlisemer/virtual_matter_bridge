// Allow dead code during development - these modules contain scaffolding
// that will be used when Matter stack integration is complete
#![allow(dead_code)]

mod clusters;
mod config;
mod device;
mod error;
mod rtsp;

use crate::config::Config;
use crate::device::video_doorbell::VideoDoorbellDevice;
use log::info;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::RwLock;

fn init_logger() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
}

#[tokio::main]
async fn main() {
    init_logger();
    info!("Starting Virtual Matter Bridge");

    // Load configuration
    let config = Config::from_env();
    info!("Configuration loaded:");
    info!("  Device Name: {}", config.matter.device_name);
    info!("  RTSP URL: {}", config.rtsp.url);
    info!("  Vendor ID: 0x{:04X}", config.matter.vendor_id);
    info!("  Product ID: 0x{:04X}", config.matter.product_id);
    info!("  Discriminator: {}", config.matter.discriminator);

    // Create the video doorbell device
    let device = Arc::new(RwLock::new(VideoDoorbellDevice::new(config)));

    // Initialize the device
    {
        let device_lock = device.read().await;
        if let Err(e) = device_lock.initialize().await {
            log::error!("Failed to initialize device: {}", e);
            std::process::exit(1);
        }
    }

    info!("Virtual Matter Bridge is running");
    info!("  - Video Doorbell device ready");
    info!("  - Press Ctrl+C to exit");

    // Spawn a task to simulate doorbell presses for testing
    let device_clone = device.clone();
    let doorbell_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let device_lock = device_clone.read().await;
            if device_lock.is_running() {
                info!("Simulating doorbell press...");
                if let Err(e) = device_lock.press_doorbell().await {
                    log::warn!("Failed to simulate doorbell press: {}", e);
                }
            } else {
                break;
            }
        }
    });

    // TODO: Start Matter stack and register device
    // This would involve:
    // 1. Initializing rs-matter with device attestation
    // 2. Registering our clusters (CameraAVStreamMgmt, WebRTCProvider, Chime)
    // 3. Starting the Matter responder
    // 4. Handling commissioning

    // Wait for shutdown signal
    match signal::ctrl_c().await {
        Ok(()) => {
            info!("Received shutdown signal");
        }
        Err(e) => {
            log::error!("Failed to listen for shutdown signal: {}", e);
        }
    }

    // Shutdown
    doorbell_task.abort();
    {
        let device_lock = device.read().await;
        if let Err(e) = device_lock.shutdown().await {
            log::error!("Error during shutdown: {}", e);
        }
    }

    info!("Virtual Matter Bridge stopped");
}
