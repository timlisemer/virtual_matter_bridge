//! MQTT Integration orchestrator for clean device management.
//!
//! Provides a high-level API for integrating MQTT devices without exposing
//! MQTT internals to main.rs. Supports multiple W100 devices.

use super::client::{MqttClient, MqttMessage};
use crate::config::MqttConfig;
use crate::matter::clusters::{GenericSwitchState, HumiditySensor, TemperatureSensor};
use log::{info, warn};
use rumqttc::QoS;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

/// Configuration for a W100 climate sensor.
pub struct W100Config {
    /// Friendly name in zigbee2mqtt (e.g., "Tim-Thermometer")
    pub friendly_name: String,
    /// Shared temperature sensor (also used by Matter)
    pub temperature_sensor: Arc<TemperatureSensor>,
    /// Shared humidity sensor (also used by Matter)
    pub humidity_sensor: Arc<HumiditySensor>,
    /// Shared button states for Plus/Minus/Center buttons (also used by Matter)
    pub button_plus: Option<Arc<GenericSwitchState>>,
    pub button_minus: Option<Arc<GenericSwitchState>>,
    pub button_center: Option<Arc<GenericSwitchState>>,
}

impl W100Config {
    /// Create a new W100 configuration.
    pub fn new(
        friendly_name: impl Into<String>,
        temperature_sensor: Arc<TemperatureSensor>,
        humidity_sensor: Arc<HumiditySensor>,
    ) -> Self {
        Self {
            friendly_name: friendly_name.into(),
            temperature_sensor,
            humidity_sensor,
            button_plus: None,
            button_minus: None,
            button_center: None,
        }
    }

    /// Add button state handlers for Matter GenericSwitch integration.
    pub fn with_buttons(
        mut self,
        plus: Arc<GenericSwitchState>,
        minus: Arc<GenericSwitchState>,
        center: Arc<GenericSwitchState>,
    ) -> Self {
        self.button_plus = Some(plus);
        self.button_minus = Some(minus);
        self.button_center = Some(center);
        self
    }
}

/// Internal W100 device state for the integration.
struct W100Device {
    friendly_name: String,
    temperature_sensor: Arc<TemperatureSensor>,
    humidity_sensor: Arc<HumiditySensor>,
    button_plus: Option<Arc<GenericSwitchState>>,
    button_minus: Option<Arc<GenericSwitchState>>,
    button_center: Option<Arc<GenericSwitchState>>,
}

impl W100Device {
    fn state_topic(&self) -> String {
        format!("zigbee2mqtt/{}", self.friendly_name)
    }

    fn action_topic(&self) -> String {
        format!("zigbee2mqtt/{}/action", self.friendly_name)
    }

    fn subscribe_topics(&self) -> Vec<String> {
        vec![self.state_topic(), self.action_topic()]
    }

    /// Process a message and update sensors if applicable.
    /// Returns true if the message was for this device.
    fn process_message(&self, topic: &str, payload: &str) -> bool {
        let state_topic = self.state_topic();
        let action_topic = self.action_topic();

        if topic == state_topic {
            self.process_state_message(payload);
            true
        } else if topic == action_topic {
            self.process_action_message(payload);
            true
        } else {
            false
        }
    }

    fn process_state_message(&self, payload: &str) {
        #[derive(serde::Deserialize)]
        struct W100State {
            #[serde(default)]
            temperature: Option<f32>,
            #[serde(default)]
            humidity: Option<f32>,
            #[serde(default)]
            action: Option<String>,
        }

        match serde_json::from_str::<W100State>(payload) {
            Ok(state) => {
                if let Some(temp) = state.temperature {
                    let old_temp = self.temperature_sensor.get_celsius();
                    self.temperature_sensor.set_celsius(temp);
                    if (temp - old_temp).abs() > 0.1 {
                        info!(
                            "[MQTT] {} temperature updated: {:.1}°C",
                            self.friendly_name, temp
                        );
                    }
                }
                if let Some(humidity) = state.humidity {
                    let old_humidity = self.humidity_sensor.get_percent();
                    self.humidity_sensor.set_percent(humidity);
                    if (humidity - old_humidity).abs() > 0.5 {
                        info!(
                            "[MQTT] {} humidity updated: {:.1}%",
                            self.friendly_name, humidity
                        );
                    }
                }
                // Handle button actions (will be used for automations in Scope 5)
                if let Some(action) = state.action {
                    info!("[MQTT] {} button action: {}", self.friendly_name, action);
                }
            }
            Err(e) => {
                warn!("[MQTT] Failed to parse {} state: {}", self.friendly_name, e);
            }
        }
    }

    fn process_action_message(&self, payload: &str) {
        let action = payload.trim();
        info!("[MQTT] {} button action: {}", self.friendly_name, action);

        // Map W100 actions to GenericSwitch events
        match action {
            // Single press
            "single_plus" => {
                if let Some(btn) = &self.button_plus {
                    btn.single_press();
                    info!("[Matter] Button Plus: single press event emitted");
                }
            }
            "single_minus" => {
                if let Some(btn) = &self.button_minus {
                    btn.single_press();
                    info!("[Matter] Button Minus: single press event emitted");
                }
            }
            "single_center" | "single" => {
                if let Some(btn) = &self.button_center {
                    btn.single_press();
                    info!("[Matter] Button Center: single press event emitted");
                }
            }
            // Double press
            "double_plus" => {
                if let Some(btn) = &self.button_plus {
                    btn.double_press();
                    info!("[Matter] Button Plus: double press event emitted");
                }
            }
            "double_minus" => {
                if let Some(btn) = &self.button_minus {
                    btn.double_press();
                    info!("[Matter] Button Minus: double press event emitted");
                }
            }
            "double_center" | "double" => {
                if let Some(btn) = &self.button_center {
                    btn.double_press();
                    info!("[Matter] Button Center: double press event emitted");
                }
            }
            // Hold (long press)
            "hold_plus" => {
                if let Some(btn) = &self.button_plus {
                    btn.hold_start();
                    info!("[Matter] Button Plus: hold start event emitted");
                }
            }
            "hold_minus" => {
                if let Some(btn) = &self.button_minus {
                    btn.hold_start();
                    info!("[Matter] Button Minus: hold start event emitted");
                }
            }
            "hold_center" | "hold" => {
                if let Some(btn) = &self.button_center {
                    btn.hold_start();
                    info!("[Matter] Button Center: hold start event emitted");
                }
            }
            // Release (after hold)
            "release_plus" => {
                if let Some(btn) = &self.button_plus {
                    btn.hold_release();
                    info!("[Matter] Button Plus: release event emitted");
                }
            }
            "release_minus" => {
                if let Some(btn) = &self.button_minus {
                    btn.hold_release();
                    info!("[Matter] Button Minus: release event emitted");
                }
            }
            "release_center" | "release" => {
                if let Some(btn) = &self.button_center {
                    btn.hold_release();
                    info!("[Matter] Button Center: release event emitted");
                }
            }
            _ => {
                warn!("[MQTT] Unknown W100 action: {}", action);
            }
        }
    }
}

/// MQTT Integration orchestrator.
///
/// Manages MQTT client and device subscriptions, keeping MQTT internals
/// out of main.rs.
pub struct MqttIntegration {
    config: MqttConfig,
    w100_devices: Vec<W100Device>,
}

impl MqttIntegration {
    /// Create a new MQTT integration with the given broker config.
    pub fn new(config: MqttConfig) -> Self {
        Self {
            config,
            w100_devices: Vec::new(),
        }
    }

    /// Add a W100 climate sensor to the integration.
    pub fn with_w100(mut self, config: W100Config) -> Self {
        self.w100_devices.push(W100Device {
            friendly_name: config.friendly_name,
            temperature_sensor: config.temperature_sensor,
            humidity_sensor: config.humidity_sensor,
            button_plus: config.button_plus,
            button_minus: config.button_minus,
            button_center: config.button_center,
        });
        self
    }

    /// Start the MQTT integration.
    ///
    /// Spawns a background task that connects to the broker, subscribes to
    /// device topics, and routes messages to the appropriate handlers.
    /// Returns a JoinHandle that can be used to abort the task on shutdown.
    pub fn start(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            self.run().await;
        })
    }

    async fn run(self) {
        if self.w100_devices.is_empty() {
            info!("[MQTT] No devices configured, skipping MQTT integration");
            return;
        }

        info!(
            "[MQTT] Connecting to {}:{}",
            self.config.broker_host, self.config.broker_port
        );

        let mqtt_client = MqttClient::new(&self.config);

        // Get client for subscribing/publishing (AsyncClient is Send+Sync)
        let subscribe_client = mqtt_client.client();

        // Channel for MQTT messages
        let (msg_tx, mut msg_rx) = mpsc::channel::<MqttMessage>(64);

        // Channel to signal when connected
        let (connected_tx, connected_rx) = oneshot::channel();

        // Start MQTT event loop FIRST (so it can establish connection)
        let mqtt_loop = tokio::spawn(async move {
            mqtt_client.run(msg_tx, Some(connected_tx)).await;
        });

        // Wait for connection (with timeout)
        match tokio::time::timeout(Duration::from_secs(10), connected_rx).await {
            Ok(Ok(())) => {
                info!("[MQTT] Connection established, subscribing to topics");
            }
            Ok(Err(_)) => {
                warn!("[MQTT] Connection signal channel dropped");
                return;
            }
            Err(_) => {
                warn!("[MQTT] Connection timeout after 10 seconds");
                mqtt_loop.abort();
                return;
            }
        }

        // NOW subscribe to all device topics (after connection is established)
        for device in &self.w100_devices {
            for topic in device.subscribe_topics() {
                if let Err(e) = subscribe_client.subscribe(&topic, QoS::AtMostOnce).await {
                    warn!("[MQTT] Failed to subscribe to {}: {:?}", topic, e);
                }
            }
        }

        // Small delay to ensure subscriptions are processed before requesting state
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Request current state from all devices (W100 is battery-powered and sleeps)
        for device in &self.w100_devices {
            let get_topic = format!("zigbee2mqtt/{}/get", device.friendly_name);
            if let Err(e) = subscribe_client
                .publish(&get_topic, QoS::AtMostOnce, false, r#"{"state":""}"#)
                .await
            {
                warn!(
                    "[MQTT] Failed to request state for {}: {:?}",
                    device.friendly_name, e
                );
            } else {
                info!(
                    "[MQTT] Requested initial state for {}",
                    device.friendly_name
                );
            }
        }

        info!(
            "[MQTT] Integration started with {} W100 device(s)",
            self.w100_devices.len()
        );

        // Process incoming messages
        while let Some(msg) = msg_rx.recv().await {
            for device in &self.w100_devices {
                if device.process_message(&msg.topic, &msg.payload) {
                    break; // Message was handled by this device
                }
            }
        }

        mqtt_loop.abort();
    }
}
