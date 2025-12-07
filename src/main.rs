// Allow dead code during development - these modules contain scaffolding
// that will be used when Matter stack integration is complete
#![allow(dead_code)]

mod clusters;
mod config;
mod device;
mod error;
mod matter;
mod rtsp;

use crate::config::Config;
use crate::device::video_doorbell::VideoDoorbellDevice;
use log::info;
use parking_lot::RwLock as SyncRwLock;
use std::sync::Arc;
use tokio::signal;

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

    // Clone config for Matter stack before moving to device
    let matter_config = config.matter.clone();

    // Create the video doorbell device
    let device = Arc::new(SyncRwLock::new(VideoDoorbellDevice::new(config)));

    // Create Matter handlers from device clusters
    // These need to be created before spawning the Matter thread
    let camera_cluster = device.read().camera_cluster();
    let webrtc_cluster = device.read().webrtc_cluster();

    // Initialize the device - use spawn_blocking since initialize() is async but uses sync locks internally
    let device_for_init = device.clone();
    tokio::task::spawn_blocking(move || {
        let device_lock = device_for_init.read();
        futures_lite::future::block_on(async {
            if let Err(e) = device_lock.initialize().await {
                log::error!("Failed to initialize device: {}", e);
                std::process::exit(1);
            }
        });
    })
    .await
    .expect("Device initialization task panicked");

    info!("Virtual Matter Bridge is running");
    info!("  - Video Doorbell device ready");
    info!("  - Press Ctrl+C to exit");

    // Spawn a task to simulate doorbell presses for testing
    let device_clone = device.clone();
    let doorbell_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let is_running = device_clone.read().is_running();

            if is_running {
                info!("Simulating doorbell press...");
                // TODO: Send Matter doorbell press event notification
            } else {
                break;
            }
        }
    });

    // Start Matter stack in a separate thread
    // Matter uses blocking I/O internally with embassy, so we run it on a dedicated thread
    let _matter_handle = std::thread::Builder::new()
        .name("matter-stack".into())
        .stack_size(550 * 1024) // 550KB stack for Matter operations (matches rs-matter examples)
        .spawn(move || {
            // Run the Matter stack (blocking) - futures_lite::block_on works with async_io
            // when async_io::Async is used for socket creation
            // Handlers are now created inside run_matter_stack with proper random Dataver seeds
            if let Err(e) = futures_lite::future::block_on(matter::run_matter_stack(
                &matter_config,
                camera_cluster,
                webrtc_cluster,
            )) {
                log::error!("Matter stack error: {:?}", e);
            }
        })
        .expect("Failed to spawn Matter thread");

    info!("Matter stack started on dedicated thread");

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

    // Shutdown the device - we need to do this carefully because the device
    // uses a sync RwLock for clusters but async RwLock for the bridge
    // The shutdown method is async, so we run it in a blocking task
    let device_for_shutdown = device.clone();
    tokio::task::spawn_blocking(move || {
        let device_lock = device_for_shutdown.read();
        futures_lite::future::block_on(async {
            if let Err(e) = device_lock.shutdown().await {
                log::error!("Error during shutdown: {}", e);
            }
        });
    })
    .await
    .expect("Shutdown task panicked");

    info!("Virtual Matter Bridge stopped");
}
