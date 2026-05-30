// Allow dead code during development - these modules contain scaffolding
// that will be used when Matter stack integration is complete
#![allow(dead_code)]
// Allow unexpected_cfgs from rs_matter::import! macro (uses cfg(feature = "defmt"))
#![allow(unexpected_cfgs)]
// Increase recursion limit for deeply nested Matter handler chains
#![recursion_limit = "256"]

mod commissioning;
mod config;
mod error;
mod input;
mod matter;

use crate::config::Config;
use crate::input::camera::CameraInput;
use crate::input::mqtt::{
    MqttIntegration, Shelly2PmConfig, Shelly2PmParts, W100Config, shelly_2pm_parts,
};
use crate::matter::clusters::{
    BridgedDeviceInfo, GenericSwitchState, HumiditySensor, TemperatureSensor,
};
use crate::matter::endpoints::{
    EndpointHandler, ReadinessOnlyHandler, SourceReadiness, SourceSnapshot,
};
use crate::matter::{
    EndpointConfig, VirtualDevice, collect_endpoint_readiness, mark_endpoint_readiness_unavailable,
};
use log::{Level, debug, info};
use parking_lot::RwLock as SyncRwLock;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;

/// Type alias for the state pusher callback.
type StatePusher = Arc<dyn Fn(bool) + Send + Sync>;

const W100_MATTER_DEVICE_NAME: &str = "Büro Thermometer";
const W100_ZIGBEE2MQTT_FRIENDLY_NAME: &str = "Büro-Thermometer";
const SHELLY_2PM_SWITCH_1_MATTER_DEVICE_NAME: &str = "Shelly 2PM Gen4 - Switch 1";
const SHELLY_2PM_SWITCH_2_MATTER_DEVICE_NAME: &str = "Shelly 2PM Gen4 - Switch 2";
const SHELLY_2PM_SWITCH_1_ENDPOINT_NAME: &str = "Büro Licht";
const SHELLY_2PM_SWITCH_2_ENDPOINT_NAME: &str = "Tim-PC";
const SHELLY_2PM_ZIGBEE2MQTT_FRIENDLY_NAME: &str = "Büro Licht & PC Schalter";
const MATTER_UNAVAILABLE_PROPAGATION_GRACE: Duration = Duration::from_secs(2);

fn shelly_2pm_virtual_devices(parts: &Shelly2PmParts) -> [VirtualDevice; 2] {
    [
        VirtualDevice::new(SHELLY_2PM_SWITCH_1_MATTER_DEVICE_NAME)
            .with_device_info(
                BridgedDeviceInfo::new(SHELLY_2PM_SWITCH_1_MATTER_DEVICE_NAME)
                    .with_vendor("Shelly")
                    .with_product("Shelly 2PM Gen4"),
            )
            .with_endpoint(
                EndpointConfig::light_switch(
                    SHELLY_2PM_SWITCH_1_ENDPOINT_NAME,
                    parts.l2_handler.clone(),
                )
                .with_electrical_power(parts.l2_power.clone())
                .with_electrical_energy(parts.l2_energy.clone())
                .with_shelly_diagnostics(parts.diagnostics.clone()),
            ),
        VirtualDevice::new(SHELLY_2PM_SWITCH_2_MATTER_DEVICE_NAME)
            .with_device_info(
                BridgedDeviceInfo::new(SHELLY_2PM_SWITCH_2_MATTER_DEVICE_NAME)
                    .with_vendor("Shelly")
                    .with_product("Shelly 2PM Gen4"),
            )
            .with_endpoint(
                EndpointConfig::switch(SHELLY_2PM_SWITCH_2_ENDPOINT_NAME, parts.l1_handler.clone())
                    .with_electrical_power(parts.l1_power.clone())
                    .with_electrical_energy(parts.l1_energy.clone())
                    .with_shelly_diagnostics(parts.diagnostics.clone()),
            ),
    ]
}

fn init_logger() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format(|buf, record| {
            let message = record.args().to_string();

            if should_suppress_log_record(record.target(), record.level(), &message) {
                return Ok(());
            }

            writeln!(
                buf,
                "[{} {}{:<5}{} {}] {}",
                buf.timestamp_millis(),
                buf.default_level_style(record.level()),
                record.level(),
                buf.default_level_style(record.level()).render_reset(),
                record.target(),
                message
            )
        })
        .init();
}

fn should_suppress_log_record(target: &str, level: Level, message: &str) -> bool {
    // Suppress high-volume subscription bookkeeping from rs-matter. These
    // messages are emitted for normal report delivery and do not indicate
    // subscription churn or controller errors.
    if target == "rs_matter::dm::subscriptions"
        && level == Level::Info
        && message.contains("kept after reporting")
    {
        return true;
    }

    // Suppress late standalone Matter MRP ACK noise for now. If future issues
    // appear around missed subscription reports, repeated retransmissions, or
    // controller ACK handling, remember that this warning is intentionally hidden.
    target == "rs_matter::transport"
        && level == Level::Warn
        && message.contains("MRPStandAloneAck")
        && message.contains("No valid exchange found")
}

/// Example handler for simulated sensors/switches.
///
/// This is a simple implementation that can be used for testing.
/// Replace with your actual hardware or API integration.
pub struct SimulatedHandler {
    state: Arc<SourceSnapshot<bool>>,
    pusher: SyncRwLock<Option<StatePusher>>,
}

impl SimulatedHandler {
    pub fn new(_initial: bool) -> Self {
        Self {
            state: Arc::new(SourceSnapshot::new()),
            pusher: SyncRwLock::new(None),
        }
    }

    /// Update the state and push to Matter.
    /// Call this from your simulation or hardware integration.
    pub fn set_state(&self, value: bool) {
        if self.state.update_source(value)
            && let Some(pusher) = self.pusher.read().as_ref()
        {
            pusher(value);
        }
    }

    /// Toggle the state and push to Matter.
    pub fn toggle(&self) -> bool {
        let new = !self.state.snapshot().unwrap_or(false);
        if self.state.update_source(new)
            && let Some(pusher) = self.pusher.read().as_ref()
        {
            pusher(new);
        }
        new
    }
}

impl EndpointHandler for SimulatedHandler {
    fn on_command(&self, value: bool) {
        log::info!("[SimulatedHandler] Received command: {}", value);
        self.state.update_source(value);
    }

    fn get_state(&self) -> Option<bool> {
        self.state.snapshot()
    }

    fn readiness(&self) -> Arc<dyn SourceReadiness> {
        self.state.clone()
    }

    fn set_state_pusher(&self, pusher: Arc<dyn Fn(bool) + Send + Sync>) {
        *self.pusher.write() = Some(pusher);
    }
}

/// Run sensor simulation task (toggles sensors periodically for testing)
async fn run_sensor_simulation(
    door_handler: Arc<SimulatedHandler>,
    motion_handler: Arc<SimulatedHandler>,
    outlet1_handler: Arc<SimulatedHandler>,
    outlet2_handler: Arc<SimulatedHandler>,
) {
    door_handler.set_state(true);
    motion_handler.set_state(false);
    outlet1_handler.set_state(true);
    outlet2_handler.set_state(false);

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        let new_state = door_handler.toggle();
        debug!("[Simulation] Door sensor toggled to: {}", new_state);

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        let new_state = motion_handler.toggle();
        debug!("[Simulation] Motion sensor toggled to: {}", new_state);
    }
}

#[tokio::main]
async fn main() {
    // Load .env file before anything else
    config::load_dotenv();

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

    // Clone config parts before moving to camera input
    let matter_config = config.matter.clone();
    let mqtt_config = config.mqtt.clone();

    // Create the camera input (handles RTSP/WebRTC)
    let camera = Arc::new(SyncRwLock::new(CameraInput::new(config)));

    // Create handlers for our virtual devices
    let door_handler = Arc::new(SimulatedHandler::new(true));
    let motion_handler = Arc::new(SimulatedHandler::new(false));
    let outlet1_handler = Arc::new(SimulatedHandler::new(true));
    let outlet2_handler = Arc::new(SimulatedHandler::new(false));

    // Shelly 2PM Gen4 two-channel relay via MQTT/zigbee2mqtt.
    // L1/state_l1 is Switch 2: Tim-PC, L2/state_l2 is Switch 1: Büro Licht.
    let shelly_parts = shelly_2pm_parts(SHELLY_2PM_ZIGBEE2MQTT_FRIENDLY_NAME);

    // Create W100 climate sensors (will be updated by MQTT)
    let w100_temperature = Arc::new(TemperatureSensor::new(20.0)); // Default 20°C
    let w100_humidity = Arc::new(HumiditySensor::new(50.0)); // Default 50%
    // W100 button states (Plus, Minus, Center buttons)
    let w100_button_plus = Arc::new(GenericSwitchState::new());
    let w100_button_minus = Arc::new(GenericSwitchState::new());
    let w100_button_center = Arc::new(GenericSwitchState::new());

    let doorbell_handler = Arc::new(ReadinessOnlyHandler::new(camera.read().readiness()));

    // Define our virtual devices using the new API
    let [shelly_switch_1_device, shelly_switch_2_device] =
        shelly_2pm_virtual_devices(&shelly_parts);
    let virtual_devices = vec![
        // Door sensor (parent) with contact sensor endpoint (child)
        VirtualDevice::new("Door").with_endpoint(EndpointConfig::contact_sensor(
            "Door Sensor",
            door_handler.clone(),
        )),
        // Motion sensor (parent) with occupancy sensor endpoint (child)
        VirtualDevice::new("Motion").with_endpoint(EndpointConfig::occupancy_sensor(
            "Occupancy",
            motion_handler.clone(),
        )),
        // Power strip (parent) with two switch endpoints (children)
        VirtualDevice::new("Power Strip")
            .with_endpoint(EndpointConfig::switch("Outlet 1", outlet1_handler.clone()))
            .with_endpoint(EndpointConfig::switch("Outlet 2", outlet2_handler.clone())),
        // Shelly 2PM Gen4 split into one Matter bridged device per channel.
        shelly_switch_1_device,
        shelly_switch_2_device,
        // Video Doorbell (parent) with camera endpoint (child)
        // Note: Camera handlers are stub - actual streaming awaits Matter 1.5 controller support
        VirtualDevice::new("Video Doorbell").with_endpoint(EndpointConfig::video_doorbell_camera(
            "Camera",
            doorbell_handler.clone(),
        )),
        // W100 Climate Sensor (Aqara TH-S04D) via MQTT/zigbee2mqtt
        VirtualDevice::new(W100_MATTER_DEVICE_NAME)
            .with_device_info(
                BridgedDeviceInfo::new(W100_MATTER_DEVICE_NAME)
                    .with_vendor("Aqara")
                    .with_product("Climate Sensor W100"),
            )
            .with_endpoint(EndpointConfig::temperature_sensor(
                "Temperature",
                w100_temperature.clone(),
            ))
            .with_endpoint(EndpointConfig::humidity_sensor(
                "Humidity",
                w100_humidity.clone(),
            ))
            .with_endpoint(EndpointConfig::generic_switch(
                "Button Plus",
                w100_button_plus.clone(),
            ))
            .with_endpoint(EndpointConfig::generic_switch(
                "Button Minus",
                w100_button_minus.clone(),
            ))
            .with_endpoint(EndpointConfig::generic_switch(
                "Button Center",
                w100_button_center.clone(),
            )),
    ];
    let endpoint_readiness = collect_endpoint_readiness(&virtual_devices);

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
    let outlet1_for_sim = outlet1_handler.clone();
    let outlet2_for_sim = outlet2_handler.clone();
    let sensor_task = tokio::spawn(async move {
        run_sensor_simulation(
            door_for_sim,
            motion_for_sim,
            outlet1_for_sim,
            outlet2_for_sim,
        )
        .await;
    });

    // Start MQTT integration for W100 climate sensor (self-contained!)
    let mqtt_task = MqttIntegration::new(mqtt_config)
        .with_w100(
            W100Config::new(
                W100_ZIGBEE2MQTT_FRIENDLY_NAME,
                w100_temperature.clone(),
                w100_humidity.clone(),
            )
            .with_buttons(
                w100_button_plus.clone(),
                w100_button_minus.clone(),
                w100_button_center.clone(),
            ),
        )
        .with_shelly_2pm(Shelly2PmConfig::new(
            SHELLY_2PM_ZIGBEE2MQTT_FRIENDLY_NAME,
            shelly_parts,
        ))
        .start();

    // Start Matter stack in a separate thread
    // Matter uses blocking I/O internally with embassy, so we run it on a dedicated thread
    let _matter_handle = std::thread::Builder::new()
        .name("matter-stack".into())
        .stack_size(550 * 1024) // 550KB stack for Matter operations (matches rs-matter examples)
        .spawn(move || {
            if let Err(e) = futures_lite::future::block_on(matter::run_matter_stack(
                &matter_config,
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

    // Stop inputs that could mark endpoints ready again while we publish shutdown readiness.
    sensor_task.abort();
    mqtt_task.shutdown().await;

    info!("Marking Matter endpoints unavailable");
    let unavailable_count = mark_endpoint_readiness_unavailable(&endpoint_readiness);
    info!(
        "Marked {} Matter endpoint(s) unavailable",
        unavailable_count
    );

    info!(
        "Waiting {}s for Matter availability updates to propagate",
        MATTER_UNAVAILABLE_PROPAGATION_GRACE.as_secs()
    );
    tokio::time::sleep(MATTER_UNAVAILABLE_PROPAGATION_GRACE).await;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulated_handler_is_not_ready_until_first_source_update() {
        let handler = SimulatedHandler::new(false);

        assert!(!handler.readiness().is_ready());
        assert_eq!(handler.get_state(), None);

        handler.set_state(false);

        assert!(handler.readiness().is_ready());
        assert_eq!(handler.get_state(), Some(false));
    }

    #[test]
    fn suppresses_late_standalone_ack_without_hiding_other_transport_warnings() {
        let late_ack = "\
>>RCV UDP [::1]:5540 [SID:1,CTR:1][I|A,EID:6413,PROTO:0,OP:10,ACTR:2]
      SC::MRPStandAloneAck
      => No valid exchange found, dropping";

        assert!(should_suppress_log_record(
            "rs_matter::transport",
            Level::Warn,
            late_ack
        ));

        assert!(!should_suppress_log_record(
            "rs_matter::transport",
            Level::Warn,
            "=> No valid session found, replying with SessionNotFound"
        ));
        assert!(!should_suppress_log_record(
            "rs_matter::transport",
            Level::Debug,
            late_ack
        ));
    }

    #[test]
    fn suppresses_subscription_bookkeeping_without_hiding_other_subscription_logs() {
        assert!(should_suppress_log_record(
            "rs_matter::dm::subscriptions",
            Level::Info,
            "Subscription SubscriptionIds { id: 1, fab_idx: 1, peer_node_id: 112233 } kept after reporting; max-attr-change-id: 32, max-seen-event-number: 1"
        ));

        assert!(!should_suppress_log_record(
            "rs_matter::dm::subscriptions",
            Level::Info,
            "Added subscription SubscriptionIds { id: 1, fab_idx: 1, peer_node_id: 112233 }"
        ));
        assert!(!should_suppress_log_record(
            "rs_matter::dm::subscriptions",
            Level::Warn,
            "Subscription SubscriptionIds { id: 1, fab_idx: 1, peer_node_id: 112233 } kept after reporting; max-attr-change-id: 32, max-seen-event-number: 1"
        ));
    }

    #[test]
    fn shelly_virtual_devices_are_split_and_named_by_channel() {
        let parts = shelly_2pm_parts(SHELLY_2PM_ZIGBEE2MQTT_FRIENDLY_NAME);
        let devices = shelly_2pm_virtual_devices(&parts);

        assert_eq!(devices[0].label, SHELLY_2PM_SWITCH_1_MATTER_DEVICE_NAME);
        assert_eq!(devices[1].label, SHELLY_2PM_SWITCH_2_MATTER_DEVICE_NAME);
        assert_ne!(devices[0].label, SHELLY_2PM_ZIGBEE2MQTT_FRIENDLY_NAME);
        assert_ne!(devices[1].label, SHELLY_2PM_ZIGBEE2MQTT_FRIENDLY_NAME);

        assert_eq!(devices[0].endpoints.len(), 1);
        assert_eq!(
            devices[0].endpoints[0].label,
            SHELLY_2PM_SWITCH_1_ENDPOINT_NAME
        );
        assert_eq!(
            devices[0].endpoints[0].kind,
            crate::matter::virtual_device::EndpointKind::LightSwitch
        );
        assert!(devices[0].endpoints[0].electrical_power.is_some());
        assert!(devices[0].endpoints[0].electrical_energy.is_some());
        assert!(devices[0].endpoints[0].shelly_diagnostics.is_some());

        assert_eq!(devices[1].endpoints.len(), 1);
        assert_eq!(
            devices[1].endpoints[0].label,
            SHELLY_2PM_SWITCH_2_ENDPOINT_NAME
        );
        assert_eq!(
            devices[1].endpoints[0].kind,
            crate::matter::virtual_device::EndpointKind::Switch
        );
        assert!(devices[1].endpoints[0].electrical_power.is_some());
        assert!(devices[1].endpoints[0].electrical_energy.is_some());
        assert!(devices[1].endpoints[0].shelly_diagnostics.is_some());
    }
}
