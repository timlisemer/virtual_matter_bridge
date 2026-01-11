//! MQTT Integration orchestrator for clean device management.
//!
//! Provides a high-level API for integrating MQTT devices without exposing
//! MQTT internals to main.rs. Supports multiple W100 devices.

use super::client::{MqttClient, MqttMessage};
use crate::config::MqttConfig;
use crate::matter::clusters::{HumiditySensor, TemperatureSensor};
use log::{info, warn};
use rumqttc::QoS;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Configuration for a W100 climate sensor.
pub struct W100Config {
    /// Friendly name in zigbee2mqtt (e.g., "Tim-Thermometer")
    pub friendly_name: String,
    /// Shared temperature sensor (also used by Matter)
    pub temperature_sensor: Arc<TemperatureSensor>,
    /// Shared humidity sensor (also used by Matter)
    pub humidity_sensor: Arc<HumiditySensor>,
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
        }
    }
}

/// Internal W100 device state for the integration.
struct W100Device {
    friendly_name: String,
    temperature_sensor: Arc<TemperatureSensor>,
    humidity_sensor: Arc<HumiditySensor>,
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

        // Get subscribable client (AsyncClient is Send+Sync)
        let subscribe_client = mqtt_client.client();

        // Subscribe to all device topics
        for device in &self.w100_devices {
            for topic in device.subscribe_topics() {
                if let Err(e) = subscribe_client.subscribe(&topic, QoS::AtMostOnce).await {
                    warn!("[MQTT] Failed to subscribe to {}: {:?}", topic, e);
                }
            }
        }

        info!(
            "[MQTT] Integration started with {} W100 device(s)",
            self.w100_devices.len()
        );

        // Channel for MQTT messages
        let (msg_tx, mut msg_rx) = mpsc::channel::<MqttMessage>(64);

        // Spawn MQTT event loop
        let mqtt_loop = tokio::spawn(async move {
            mqtt_client.run(msg_tx).await;
        });

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
