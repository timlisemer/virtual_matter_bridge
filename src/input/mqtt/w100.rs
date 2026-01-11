//! Aqara W100 Climate Sensor device abstraction.
//!
//! Parses zigbee2mqtt MQTT messages for the W100 and provides a clean interface
//! for reading sensor values and button events.

use log::{debug, info, warn};
use rumqttc::AsyncClient;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};

/// W100 button action types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum W100Action {
    SinglePlus,
    SingleMinus,
    SingleCenter,
    DoublePlus,
    DoubleMinus,
    DoubleCenter,
    HoldPlus,
    HoldMinus,
    HoldCenter,
    ReleasePlus,
    ReleaseMinus,
    ReleaseCenter,
    Unknown(String),
}

impl From<&str> for W100Action {
    fn from(s: &str) -> Self {
        match s {
            "single_plus" => W100Action::SinglePlus,
            "single_minus" => W100Action::SingleMinus,
            "single_center" => W100Action::SingleCenter,
            "double_plus" => W100Action::DoublePlus,
            "double_minus" => W100Action::DoubleMinus,
            "double_center" => W100Action::DoubleCenter,
            "hold_plus" => W100Action::HoldPlus,
            "hold_minus" => W100Action::HoldMinus,
            "hold_center" => W100Action::HoldCenter,
            "release_plus" => W100Action::ReleasePlus,
            "release_minus" => W100Action::ReleaseMinus,
            "release_center" => W100Action::ReleaseCenter,
            other => W100Action::Unknown(other.to_string()),
        }
    }
}

/// W100 state from zigbee2mqtt.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct W100State {
    /// Current temperature reading (°C)
    #[serde(default)]
    pub temperature: Option<f32>,

    /// Current humidity reading (%)
    #[serde(default)]
    pub humidity: Option<f32>,

    /// Display mode: "internal" or "external"
    #[serde(default)]
    pub sensor: Option<String>,

    /// External temperature value for display
    #[serde(default)]
    pub external_temperature: Option<f32>,

    /// External humidity value for display
    #[serde(default)]
    pub external_humidity: Option<f32>,

    /// Zigbee link quality (0-255)
    #[serde(default)]
    pub linkquality: Option<u8>,

    /// Button action (only present when a button is pressed)
    #[serde(default)]
    pub action: Option<String>,

    /// Display off setting
    #[serde(default)]
    pub display_off: Option<bool>,
}

/// W100 device handler.
///
/// Manages state and provides methods to interact with the W100 via MQTT.
pub struct W100Device {
    /// Device friendly name in zigbee2mqtt
    friendly_name: String,

    /// Current device state
    state: Arc<RwLock<W100State>>,

    /// MQTT client for publishing
    mqtt_client: Option<AsyncClient>,

    /// Channel for button action callbacks
    action_tx: Option<mpsc::Sender<W100Action>>,
}

impl W100Device {
    /// Create a new W100 device handler.
    ///
    /// # Arguments
    /// * `friendly_name` - The device's friendly name in zigbee2mqtt (e.g., "Tim-Thermometer")
    pub fn new(friendly_name: impl Into<String>) -> Self {
        Self {
            friendly_name: friendly_name.into(),
            state: Arc::new(RwLock::new(W100State::default())),
            mqtt_client: None,
            action_tx: None,
        }
    }

    /// Set the MQTT client for publishing commands.
    pub fn with_mqtt_client(mut self, client: AsyncClient) -> Self {
        self.mqtt_client = Some(client);
        self
    }

    /// Set a channel to receive button action events.
    pub fn with_action_channel(mut self, tx: mpsc::Sender<W100Action>) -> Self {
        self.action_tx = Some(tx);
        self
    }

    /// Get the MQTT topic for this device's state.
    pub fn state_topic(&self) -> String {
        format!("zigbee2mqtt/{}", self.friendly_name)
    }

    /// Get the MQTT topic for this device's actions.
    pub fn action_topic(&self) -> String {
        format!("zigbee2mqtt/{}/action", self.friendly_name)
    }

    /// Get the MQTT topic for sending commands to this device.
    pub fn set_topic(&self) -> String {
        format!("zigbee2mqtt/{}/set", self.friendly_name)
    }

    /// Get all topics to subscribe to for this device.
    pub fn subscribe_topics(&self) -> Vec<String> {
        vec![self.state_topic(), self.action_topic()]
    }

    /// Process an incoming MQTT message.
    ///
    /// Returns true if the message was for this device.
    pub async fn process_message(&self, topic: &str, payload: &str) -> bool {
        let state_topic = self.state_topic();
        let action_topic = self.action_topic();

        if topic == state_topic {
            self.process_state_message(payload).await;
            true
        } else if topic == action_topic {
            self.process_action_message(payload).await;
            true
        } else {
            false
        }
    }

    /// Process a state message from the device.
    async fn process_state_message(&self, payload: &str) {
        match serde_json::from_str::<W100State>(payload) {
            Ok(new_state) => {
                // Check for action in state message
                if let Some(action_str) = &new_state.action {
                    let action = W100Action::from(action_str.as_str());
                    self.handle_action(action).await;
                }

                // Update stored state
                let mut state = self.state.write().await;
                if let Some(t) = new_state.temperature {
                    state.temperature = Some(t);
                }
                if let Some(h) = new_state.humidity {
                    state.humidity = Some(h);
                }
                if let Some(s) = new_state.sensor {
                    state.sensor = Some(s);
                }
                if let Some(et) = new_state.external_temperature {
                    state.external_temperature = Some(et);
                }
                if let Some(eh) = new_state.external_humidity {
                    state.external_humidity = Some(eh);
                }
                if let Some(lq) = new_state.linkquality {
                    state.linkquality = Some(lq);
                }
                if let Some(d) = new_state.display_off {
                    state.display_off = Some(d);
                }

                debug!(
                    "W100 state updated: temp={:?}°C, humidity={:?}%, linkquality={:?}",
                    state.temperature, state.humidity, state.linkquality
                );
            }
            Err(e) => {
                warn!("Failed to parse W100 state: {}", e);
            }
        }
    }

    /// Process an action message (button press).
    async fn process_action_message(&self, payload: &str) {
        let action = W100Action::from(payload.trim());
        self.handle_action(action).await;
    }

    /// Handle a button action.
    async fn handle_action(&self, action: W100Action) {
        info!("W100 button action: {:?}", action);

        if let Some(tx) = &self.action_tx
            && tx.send(action).await.is_err()
        {
            warn!("Action channel closed");
        }
    }

    /// Get the current device state.
    pub async fn get_state(&self) -> W100State {
        self.state.read().await.clone()
    }

    /// Get the current temperature reading.
    pub async fn get_temperature(&self) -> Option<f32> {
        self.state.read().await.temperature
    }

    /// Get the current humidity reading.
    pub async fn get_humidity(&self) -> Option<f32> {
        self.state.read().await.humidity
    }

    /// Set the external temperature to display on the device.
    ///
    /// This also enables external sensor mode if not already enabled.
    pub async fn set_external_temperature(&self, temp: f32) -> Result<(), String> {
        let client = self
            .mqtt_client
            .as_ref()
            .ok_or("MQTT client not configured")?;

        let payload = serde_json::json!({
            "sensor": "external",
            "external_temperature": temp
        });

        client
            .publish(
                &self.set_topic(),
                rumqttc::QoS::AtMostOnce,
                false,
                payload.to_string().as_bytes(),
            )
            .await
            .map_err(|e| format!("Failed to publish: {}", e))?;

        info!("Set W100 external temperature to {}°C", temp);
        Ok(())
    }

    /// Set the external humidity to display on the device.
    pub async fn set_external_humidity(&self, humidity: f32) -> Result<(), String> {
        let client = self
            .mqtt_client
            .as_ref()
            .ok_or("MQTT client not configured")?;

        let payload = serde_json::json!({
            "sensor": "external",
            "external_humidity": humidity
        });

        client
            .publish(
                &self.set_topic(),
                rumqttc::QoS::AtMostOnce,
                false,
                payload.to_string().as_bytes(),
            )
            .await
            .map_err(|e| format!("Failed to publish: {}", e))?;

        info!("Set W100 external humidity to {}%", humidity);
        Ok(())
    }

    /// Set both external temperature and humidity.
    pub async fn set_external_values(&self, temp: f32, humidity: f32) -> Result<(), String> {
        let client = self
            .mqtt_client
            .as_ref()
            .ok_or("MQTT client not configured")?;

        let payload = serde_json::json!({
            "sensor": "external",
            "external_temperature": temp,
            "external_humidity": humidity
        });

        client
            .publish(
                &self.set_topic(),
                rumqttc::QoS::AtMostOnce,
                false,
                payload.to_string().as_bytes(),
            )
            .await
            .map_err(|e| format!("Failed to publish: {}", e))?;

        info!("Set W100 external values: {}°C, {}%", temp, humidity);
        Ok(())
    }

    /// Switch to internal sensor mode (hides external values).
    pub async fn set_internal_mode(&self) -> Result<(), String> {
        let client = self
            .mqtt_client
            .as_ref()
            .ok_or("MQTT client not configured")?;

        let payload = serde_json::json!({
            "sensor": "internal"
        });

        client
            .publish(
                &self.set_topic(),
                rumqttc::QoS::AtMostOnce,
                false,
                payload.to_string().as_bytes(),
            )
            .await
            .map_err(|e| format!("Failed to publish: {}", e))?;

        info!("Set W100 to internal sensor mode");
        Ok(())
    }
}
