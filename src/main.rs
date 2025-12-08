// Allow dead code during development - these modules contain scaffolding
// that will be used when Matter stack integration is complete
#![allow(dead_code)]
// Allow unexpected_cfgs from rs_matter::import! macro (uses cfg(feature = "defmt"))
#![allow(unexpected_cfgs)]
// Increase recursion limit for deeply nested Matter handler chains
#![recursion_limit = "256"]

mod config;
mod error;
mod input;
mod matter;

use crate::config::Config;
use crate::input::camera::CameraInput;
use crate::input::simulation::run_sensor_simulation;
use crate::matter::controls::{LightSwitch, Switch};
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
    let contact_sensor = Arc::new(ContactSensor::new(true)); // Contact Sensor (endpoint 3)
    let occupancy_sensor = Arc::new(OccupancySensor::new(false)); // Occupancy Sensor (endpoint 4)

    // Create switches for Matter endpoints 5 and 6
    let switch1 = Arc::new(Switch::new(true)); // Switch 1 (endpoint 5)
    let switch2 = Arc::new(Switch::new(false)); // Switch 2 (endpoint 6)

    // Create light for Matter endpoint 7
    let light = Arc::new(LightSwitch::new(false)); // Light (endpoint 7)

    // Create Matter handlers from camera clusters
    let camera_cluster = camera.read().camera_cluster();
    let webrtc_cluster = camera.read().webrtc_cluster();
    let device_power = camera.read().device_power();

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
    let sensor_task = run_sensor_simulation(contact_sensor.clone(), occupancy_sensor.clone());

    // Start Matter stack in a separate thread
    // Matter uses blocking I/O internally with embassy, so we run it on a dedicated thread
    let contact_sensor_for_matter = contact_sensor.clone();
    let occupancy_sensor_for_matter = occupancy_sensor.clone();
    let switch1_for_matter = switch1.clone();
    let switch2_for_matter = switch2.clone();
    let light_for_matter = light.clone();
    let _matter_handle = std::thread::Builder::new()
        .name("matter-stack".into())
        .stack_size(550 * 1024) // 550KB stack for Matter operations (matches rs-matter examples)
        .spawn(move || {
            if let Err(e) = futures_lite::future::block_on(matter::run_matter_stack(
                &matter_config,
                camera_cluster,
                webrtc_cluster,
                device_power,
                contact_sensor_for_matter,
                occupancy_sensor_for_matter,
                switch1_for_matter,
                switch2_for_matter,
                light_for_matter,
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
