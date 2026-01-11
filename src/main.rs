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

use crate::config::{Config, MqttConfig};
use crate::input::camera::CameraInput;
use crate::input::mqtt::{MqttClient, W100Device};
use crate::matter::clusters::{HumiditySensor, TemperatureSensor};
use crate::matter::endpoints::EndpointHandler;
use crate::matter::{EndpointConfig, VirtualDevice, VirtualDeviceType};
use log::{info, warn};
use parking_lot::RwLock as SyncRwLock;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::signal;
use tokio::sync::mpsc;

/// Type alias for the state pusher callback.
type StatePusher = Arc<dyn Fn(bool) + Send + Sync>;

fn init_logger() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
}

/// Example handler for simulated sensors/switches.
///
/// This is a simple implementation that can be used for testing.
/// Replace with your actual hardware or API integration.
pub struct SimulatedHandler {
    state: AtomicBool,
    pusher: SyncRwLock<Option<StatePusher>>,
}

impl SimulatedHandler {
    pub fn new(initial: bool) -> Self {
        Self {
            state: AtomicBool::new(initial),
            pusher: SyncRwLock::new(None),
        }
    }

    /// Update the state and push to Matter.
    /// Call this from your simulation or hardware integration.
    pub fn set_state(&self, value: bool) {
        let old = self.state.swap(value, Ordering::SeqCst);
        if old != value
            && let Some(pusher) = self.pusher.read().as_ref()
        {
            pusher(value);
        }
    }

    /// Toggle the state and push to Matter.
    pub fn toggle(&self) -> bool {
        let old = self.state.fetch_xor(true, Ordering::SeqCst);
        let new = !old;
        if let Some(pusher) = self.pusher.read().as_ref() {
            pusher(new);
        }
        new
    }
}

impl EndpointHandler for SimulatedHandler {
    fn on_command(&self, value: bool) {
        log::info!("[SimulatedHandler] Received command: {}", value);
        self.state.store(value, Ordering::SeqCst);
    }

    fn get_state(&self) -> bool {
        self.state.load(Ordering::SeqCst)
    }

    fn set_state_pusher(&self, pusher: Arc<dyn Fn(bool) + Send + Sync>) {
        *self.pusher.write() = Some(pusher);
    }
}

/// Run sensor simulation task (toggles sensors periodically for testing)
async fn run_sensor_simulation(
    door_handler: Arc<SimulatedHandler>,
    motion_handler: Arc<SimulatedHandler>,
) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        let new_state = door_handler.toggle();
        info!("[Simulation] Door sensor toggled to: {}", new_state);

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        let new_state = motion_handler.toggle();
        info!("[Simulation] Motion sensor toggled to: {}", new_state);
    }
}

/// Run MQTT task for W100 climate sensor (Aqara TH-S04D)
///
/// Subscribes to zigbee2mqtt topic and updates Matter sensors with live values.
async fn run_w100_mqtt(
    mqtt_config: config::MqttConfig,
    temperature_sensor: Arc<TemperatureSensor>,
    humidity_sensor: Arc<HumiditySensor>,
) {
    info!("[MQTT] Connecting to {}:{}", mqtt_config.broker_host, mqtt_config.broker_port);

    // Create MQTT client
    let mut mqtt_client = match MqttClient::new(&mqtt_config) {
        Ok(client) => client,
        Err(e) => {
            warn!("[MQTT] Failed to create MQTT client: {:?}", e);
            return;
        }
    };

    // Create W100 device handler
    let (action_tx, mut action_rx) = mpsc::channel(32);
    let w100 = Arc::new(W100Device::new("Tim-Thermometer"));
    w100.with_mqtt_client(mqtt_client.client());
    w100.with_action_channel(action_tx);

    // Subscribe to W100 topics
    let state_topic = "zigbee2mqtt/Tim-Thermometer";
    let action_topic = "zigbee2mqtt/Tim-Thermometer/action";
    if let Err(e) = mqtt_client.subscribe(state_topic).await {
        warn!("[MQTT] Failed to subscribe to {}: {:?}", state_topic, e);
    }
    if let Err(e) = mqtt_client.subscribe(action_topic).await {
        warn!("[MQTT] Failed to subscribe to {}: {:?}", action_topic, e);
    }

    // Channel for MQTT messages
    let (msg_tx, mut msg_rx) = mpsc::channel(64);

    // Spawn MQTT event loop
    let mqtt_loop = tokio::spawn(async move {
        mqtt_client.run(msg_tx).await;
    });

    info!("[MQTT] W100 integration started - subscribed to {}", state_topic);

    // Process incoming messages
    loop {
        tokio::select! {
            Some((topic, payload)) = msg_rx.recv() => {
                // Process message through W100 device
                w100.process_message(&topic, &payload).await;

                // Update Matter sensors with current values
                if let Some(temp) = w100.get_temperature().await {
                    let old_temp = temperature_sensor.get_celsius();
                    temperature_sensor.set_celsius(temp);
                    if (temp - old_temp).abs() > 0.1 {
                        info!("[MQTT] Temperature updated: {:.1}°C", temp);
                    }
                }
                if let Some(humidity) = w100.get_humidity().await {
                    let old_humidity = humidity_sensor.get_percent();
                    humidity_sensor.set_percent(humidity);
                    if (humidity - old_humidity).abs() > 0.5 {
                        info!("[MQTT] Humidity updated: {:.1}%", humidity);
                    }
                }
            }
            Some(action) = action_rx.recv() => {
                // Log button actions (will be used for automations in Scope 5)
                info!("[MQTT] W100 button action: {:?}", action);
            }
            else => break,
        }
    }

    mqtt_loop.abort();
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

    // Create handlers for our virtual devices
    let door_handler = Arc::new(SimulatedHandler::new(true));
    let motion_handler = Arc::new(SimulatedHandler::new(false));
    let outlet1_handler = Arc::new(SimulatedHandler::new(true));
    let outlet2_handler = Arc::new(SimulatedHandler::new(false));
    let light_handler = Arc::new(SimulatedHandler::new(false));
    let doorbell_handler = Arc::new(SimulatedHandler::new(false));

    // Create W100 climate sensors (will be updated by MQTT)
    let w100_temperature = Arc::new(TemperatureSensor::new(20.0)); // Default 20°C
    let w100_humidity = Arc::new(HumiditySensor::new(50.0)); // Default 50%

    // Define our virtual devices using the new API
    let virtual_devices = vec![
        // Door sensor (parent) with contact sensor endpoint (child)
        VirtualDevice::new(VirtualDeviceType::ContactSensor, "Door").with_endpoint(
            EndpointConfig::contact_sensor("Door Sensor", door_handler.clone()),
        ),
        // Motion sensor (parent) with occupancy sensor endpoint (child)
        VirtualDevice::new(VirtualDeviceType::OccupancySensor, "Motion").with_endpoint(
            EndpointConfig::occupancy_sensor("Occupancy", motion_handler.clone()),
        ),
        // Power strip (parent) with two switch endpoints (children)
        VirtualDevice::new(VirtualDeviceType::OnOffPlugInUnit, "Power Strip")
            .with_endpoint(EndpointConfig::switch("Outlet 1", outlet1_handler.clone()))
            .with_endpoint(EndpointConfig::switch("Outlet 2", outlet2_handler.clone())),
        // Light (parent) with light switch endpoint (child)
        VirtualDevice::new(VirtualDeviceType::OnOffLight, "Light")
            .with_endpoint(EndpointConfig::light_switch("Light", light_handler.clone())),
        // Video Doorbell (parent) with camera endpoint (child)
        // Note: Camera handlers are stub - actual streaming awaits Matter 1.5 controller support
        VirtualDevice::new(VirtualDeviceType::VideoDoorbellDevice, "Video Doorbell").with_endpoint(
            EndpointConfig::video_doorbell_camera("Camera", doorbell_handler.clone()),
        ),
        // W100 Climate Sensor (Aqara TH-S04D) via MQTT/zigbee2mqtt
        VirtualDevice::new(VirtualDeviceType::TemperatureSensor, "Tim Thermometer")
            .with_endpoint(EndpointConfig::temperature_sensor(
                "Temperature",
                w100_temperature.clone(),
            ))
            .with_endpoint(EndpointConfig::humidity_sensor(
                "Humidity",
                w100_humidity.clone(),
            )),
    ];

    // Get the bridge master on/off switch from camera input
    let virtual_bridge_onoff = camera.read().device_power();

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
    info!("  - {} virtual devices configured", virtual_devices.len());
    info!("  - Press Ctrl+C to exit");

    // Spawn a task to simulate sensor state changes for testing
    let door_for_sim = door_handler.clone();
    let motion_for_sim = motion_handler.clone();
    let sensor_task = tokio::spawn(async move {
        run_sensor_simulation(door_for_sim, motion_for_sim).await;
    });

    // Spawn MQTT task for W100 climate sensor
    let temp_sensor = w100_temperature.clone();
    let humidity_sensor = w100_humidity.clone();
    let mqtt_config = config::MqttConfig::from_env();
    let mqtt_task = tokio::spawn(async move {
        run_w100_mqtt(mqtt_config, temp_sensor, humidity_sensor).await;
    });

    // Start Matter stack in a separate thread
    // Matter uses blocking I/O internally with embassy, so we run it on a dedicated thread
    let _matter_handle = std::thread::Builder::new()
        .name("matter-stack".into())
        .stack_size(550 * 1024) // 550KB stack for Matter operations (matches rs-matter examples)
        .spawn(move || {
            if let Err(e) = futures_lite::future::block_on(matter::run_matter_stack(
                &matter_config,
                virtual_bridge_onoff,
                virtual_devices,
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
    mqtt_task.abort();

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
