// Allow dead code during development - these modules contain scaffolding
// that will be used when Matter stack integration is complete
#![allow(dead_code)]

mod config;
mod error;
mod input;
mod matter;

use crate::config::Config;
use crate::input::camera::CameraInput;
use crate::matter::sensors::{ContactSensor, OccupancySensor};
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

    // Clone config for Matter stack before moving to camera input
    let matter_config = config.matter.clone();

    // Create the camera input (handles RTSP/WebRTC)
    let camera = Arc::new(SyncRwLock::new(CameraInput::new(config)));

    // Create sensors for Matter endpoints
    let contact_sensor = Arc::new(ContactSensor::new(true)); // Contact Sensor (endpoint 2)
    let occupancy_sensor = Arc::new(OccupancySensor::new(false)); // Occupancy Sensor (endpoint 3)

    // Create Matter handlers from camera clusters
    let camera_cluster = camera.read().camera_cluster();
    let webrtc_cluster = camera.read().webrtc_cluster();
    let on_off_hooks = camera.read().on_off_hooks();

    // Initialize the camera input
    let camera_for_init = camera.clone();
    tokio::task::spawn_blocking(move || {
        let camera_lock = camera_for_init.read();
        futures_lite::future::block_on(async {
            if let Err(e) = camera_lock.initialize().await {
                log::error!("Failed to initialize camera: {}", e);
                std::process::exit(1);
            }
        });
    })
    .await
    .expect("Camera initialization task panicked");

    info!("Virtual Matter Bridge is running");
    info!("  - Camera input ready");
    info!("  - Press Ctrl+C to exit");

    // Spawn a task to simulate sensor state changes for testing
    // TODO: Replace this simulation with HTTP server endpoint
    // POST /sensors/{name} { "state": true/false }
    let contact_for_sim = contact_sensor.clone();
    let occupancy_for_sim = occupancy_sensor.clone();
    let sensor_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let new_contact = contact_for_sim.toggle();
            info!("[Sim] Contact sensor toggled to: {}", new_contact);
            let new_occupancy = occupancy_for_sim.toggle();
            info!("[Sim] Occupancy sensor toggled to: {}", new_occupancy);
        }
    });

    // Start Matter stack in a separate thread
    // Matter uses blocking I/O internally with embassy, so we run it on a dedicated thread
    let contact_sensor_for_matter = contact_sensor.clone();
    let occupancy_sensor_for_matter = occupancy_sensor.clone();
    let _matter_handle = std::thread::Builder::new()
        .name("matter-stack".into())
        .stack_size(550 * 1024) // 550KB stack for Matter operations (matches rs-matter examples)
        .spawn(move || {
            if let Err(e) = futures_lite::future::block_on(matter::run_matter_stack(
                &matter_config,
                camera_cluster,
                webrtc_cluster,
                on_off_hooks,
                contact_sensor_for_matter,
                occupancy_sensor_for_matter,
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
    sensor_task.abort();

    // Shutdown the camera input
    let camera_for_shutdown = camera.clone();
    tokio::task::spawn_blocking(move || {
        let camera_lock = camera_for_shutdown.read();
        futures_lite::future::block_on(async {
            if let Err(e) = camera_lock.shutdown().await {
                log::error!("Error during shutdown: {}", e);
            }
        });
    })
    .await
    .expect("Shutdown task panicked");

    info!("Virtual Matter Bridge stopped");
}
