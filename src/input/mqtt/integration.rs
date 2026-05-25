//! MQTT Integration orchestrator for clean device management.
//!
//! Provides a high-level API for integrating MQTT devices without exposing
//! MQTT internals to main.rs. Supports multiple W100 devices.

use super::client::{MqttClient, MqttMessage};
use super::w100::{W100Action, W100State};
use crate::config::MqttConfig;
use crate::matter::clusters::{GenericSwitchState, HumiditySensor, TemperatureSensor};
use log::{info, warn};
use parking_lot::Mutex;
use rumqttc::QoS;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

const ACTION_DEDUP_WINDOW: Duration = Duration::from_millis(500);

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
struct W100IntegrationDevice {
    friendly_name: String,
    temperature_sensor: Arc<TemperatureSensor>,
    humidity_sensor: Arc<HumiditySensor>,
    button_plus: Option<Arc<GenericSwitchState>>,
    button_minus: Option<Arc<GenericSwitchState>>,
    button_center: Option<Arc<GenericSwitchState>>,
    last_action: Mutex<Option<RecentAction>>,
}

struct RecentAction {
    value: W100Action,
    received_at: Instant,
    source: ActionSource,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ActionSource {
    State,
    ActionTopic,
}

impl W100IntegrationDevice {
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
    fn process_message(&self, topic: &str, payload: &str, retain: bool) -> bool {
        let state_topic = self.state_topic();
        let action_topic = self.action_topic();

        if topic == state_topic {
            self.process_state_message(payload, retain);
            true
        } else if topic == action_topic {
            self.process_action_message(payload, retain);
            true
        } else {
            false
        }
    }

    fn process_state_message(&self, payload: &str, retain: bool) {
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
                if let Some(action) = state.action {
                    self.process_action_value(action.trim(), retain, ActionSource::State);
                }
            }
            Err(e) => {
                warn!("[MQTT] Failed to parse {} state: {}", self.friendly_name, e);
            }
        }
    }

    fn process_action_message(&self, payload: &str, retain: bool) {
        let action = payload.trim();
        self.process_action_value(action, retain, ActionSource::ActionTopic);
    }

    fn process_action_value(&self, action: &str, retain: bool, source: ActionSource) {
        let action_value = W100Action::from(action);

        if retain {
            info!(
                "[MQTT] Ignoring retained {} button action: {}",
                self.friendly_name, action
            );
            return;
        }
        if self.is_duplicate_action(&action_value, source) {
            info!(
                "[MQTT] Ignoring duplicate {} button action: {}",
                self.friendly_name, action
            );
            return;
        }

        info!("[MQTT] {} button action: {}", self.friendly_name, action);

        // Map W100 actions to GenericSwitch events
        match action_value {
            // Single press
            W100Action::SinglePlus => {
                if let Some(btn) = &self.button_plus {
                    btn.single_press();
                    info!("[Matter] Button Plus: single press event emitted");
                }
            }
            W100Action::SingleMinus => {
                if let Some(btn) = &self.button_minus {
                    btn.single_press();
                    info!("[Matter] Button Minus: single press event emitted");
                }
            }
            W100Action::SingleCenter => {
                if let Some(btn) = &self.button_center {
                    btn.single_press();
                    info!("[Matter] Button Center: single press event emitted");
                }
            }
            // Double press
            W100Action::DoublePlus => {
                if let Some(btn) = &self.button_plus {
                    btn.double_press();
                    info!("[Matter] Button Plus: double press event emitted");
                }
            }
            W100Action::DoubleMinus => {
                if let Some(btn) = &self.button_minus {
                    btn.double_press();
                    info!("[Matter] Button Minus: double press event emitted");
                }
            }
            W100Action::DoubleCenter => {
                if let Some(btn) = &self.button_center {
                    btn.double_press();
                    info!("[Matter] Button Center: double press event emitted");
                }
            }
            // Hold (long press)
            W100Action::HoldPlus => {
                if let Some(btn) = &self.button_plus {
                    btn.long_press();
                    info!("[Matter] Button Plus: long press event emitted");
                }
            }
            W100Action::HoldMinus => {
                if let Some(btn) = &self.button_minus {
                    btn.long_press();
                    info!("[Matter] Button Minus: long press event emitted");
                }
            }
            W100Action::HoldCenter => {
                if let Some(btn) = &self.button_center {
                    btn.long_press();
                    info!("[Matter] Button Center: long press event emitted");
                }
            }
            // Release (after hold)
            W100Action::ReleasePlus => {
                if let Some(btn) = &self.button_plus {
                    btn.hold_release();
                    info!("[Matter] Button Plus: release event emitted");
                }
            }
            W100Action::ReleaseMinus => {
                if let Some(btn) = &self.button_minus {
                    btn.hold_release();
                    info!("[Matter] Button Minus: release event emitted");
                }
            }
            W100Action::ReleaseCenter => {
                if let Some(btn) = &self.button_center {
                    btn.hold_release();
                    info!("[Matter] Button Center: release event emitted");
                }
            }
            W100Action::Unknown(action) => {
                warn!("[MQTT] Unknown W100 action: {}", action);
            }
        }
    }

    fn is_duplicate_action(&self, action: &W100Action, source: ActionSource) -> bool {
        let now = Instant::now();
        let mut last_action = self.last_action.lock();
        let is_duplicate = last_action.as_ref().is_some_and(|last| {
            &last.value == action
                && last.source != source
                && now.duration_since(last.received_at) <= ACTION_DEDUP_WINDOW
        });

        if !is_duplicate {
            *last_action = Some(RecentAction {
                value: action.clone(),
                received_at: now,
                source,
            });
        }

        is_duplicate
    }
}

/// MQTT Integration orchestrator.
///
/// Manages MQTT client and device subscriptions, keeping MQTT internals
/// out of main.rs.
pub struct MqttIntegration {
    config: MqttConfig,
    w100_devices: Vec<W100IntegrationDevice>,
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
        self.w100_devices.push(W100IntegrationDevice {
            friendly_name: config.friendly_name,
            temperature_sensor: config.temperature_sensor,
            humidity_sensor: config.humidity_sensor,
            button_plus: config.button_plus,
            button_minus: config.button_minus,
            button_center: config.button_center,
            last_action: Mutex::new(None),
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
                if device.process_message(&msg.topic, &msg.payload, msg.retain) {
                    break; // Message was handled by this device
                }
            }
        }

        mqtt_loop.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matter::clusters::generic_switch::GenericSwitchPendingEvent;

    fn test_device() -> (
        W100IntegrationDevice,
        Arc<GenericSwitchState>,
        Arc<GenericSwitchState>,
        Arc<GenericSwitchState>,
    ) {
        let plus = Arc::new(GenericSwitchState::new());
        let minus = Arc::new(GenericSwitchState::new());
        let center = Arc::new(GenericSwitchState::new());

        (
            W100IntegrationDevice {
                friendly_name: "W100".to_string(),
                temperature_sensor: Arc::new(TemperatureSensor::new(20.0)),
                humidity_sensor: Arc::new(HumiditySensor::new(50.0)),
                button_plus: Some(plus.clone()),
                button_minus: Some(minus.clone()),
                button_center: Some(center.clone()),
                last_action: Mutex::new(None),
            },
            plus,
            minus,
            center,
        )
    }

    #[test]
    fn retained_state_payload_action_updates_sensors_without_emitting_button_events() {
        let (device, plus, _, _) = test_device();

        assert!(device.process_message(
            "zigbee2mqtt/W100",
            r#"{"temperature":21.5,"humidity":53.0,"action":"single_plus"}"#,
            true,
        ));

        assert_eq!(device.temperature_sensor.get_celsius(), 21.5);
        assert_eq!(device.humidity_sensor.get_percent(), 53.0);
        assert!(plus.take_pending_events().is_empty());
    }

    #[test]
    fn action_topic_emits_button_events() {
        let (device, plus, _, center) = test_device();

        assert!(device.process_message("zigbee2mqtt/W100/action", "double_plus", false));
        assert_eq!(
            plus.take_pending_events(),
            vec![GenericSwitchPendingEvent::MultiPressComplete {
                previous_position: 1,
                total_number_of_presses_counted: 2,
            }]
        );

        assert!(device.process_message("zigbee2mqtt/W100/action", "double", false));
        assert_eq!(
            center.take_pending_events(),
            vec![GenericSwitchPendingEvent::MultiPressComplete {
                previous_position: 1,
                total_number_of_presses_counted: 2,
            }]
        );
    }

    #[test]
    fn hold_action_emits_long_press_after_press_transition() {
        let (device, _, minus, _) = test_device();

        assert!(device.process_message("zigbee2mqtt/W100/action", "hold_minus", false));
        assert_eq!(
            minus.take_pending_events(),
            vec![
                GenericSwitchPendingEvent::InitialPress { new_position: 1 },
                GenericSwitchPendingEvent::LongPress { new_position: 1 },
            ]
        );
        assert_eq!(minus.current_position(), 1);

        assert!(device.process_message("zigbee2mqtt/W100/action", "release_minus", false));
        assert_eq!(
            minus.take_pending_events(),
            vec![GenericSwitchPendingEvent::LongRelease {
                previous_position: 1,
            }]
        );
        assert_eq!(minus.current_position(), 0);
    }

    #[test]
    fn live_state_payload_action_emits_button_event() {
        let (device, plus, _, _) = test_device();

        assert!(device.process_message("zigbee2mqtt/W100", r#"{"action":"single_plus"}"#, false,));

        assert_eq!(
            plus.take_pending_events(),
            vec![
                GenericSwitchPendingEvent::InitialPress { new_position: 1 },
                GenericSwitchPendingEvent::ShortRelease {
                    previous_position: 1,
                },
            ]
        );
    }

    #[test]
    fn duplicate_action_on_state_and_action_topics_is_ignored() {
        let (device, plus, _, _) = test_device();

        assert!(device.process_message("zigbee2mqtt/W100/action", "single_plus", false));
        assert!(device.process_message("zigbee2mqtt/W100", r#"{"action":"single_plus"}"#, false));

        assert_eq!(
            plus.take_pending_events(),
            vec![
                GenericSwitchPendingEvent::InitialPress { new_position: 1 },
                GenericSwitchPendingEvent::ShortRelease {
                    previous_position: 1,
                },
            ]
        );
    }

    #[test]
    fn alias_equivalent_duplicate_actions_are_ignored() {
        let (device, _, _, center) = test_device();

        assert!(device.process_message("zigbee2mqtt/W100/action", "single_center", false));
        assert!(device.process_message("zigbee2mqtt/W100", r#"{"action":"single"}"#, false));

        assert_eq!(
            center.take_pending_events(),
            vec![
                GenericSwitchPendingEvent::InitialPress { new_position: 1 },
                GenericSwitchPendingEvent::ShortRelease {
                    previous_position: 1,
                },
            ]
        );
    }

    #[test]
    fn repeated_action_topic_events_are_not_deduplicated() {
        let (device, plus, _, _) = test_device();

        assert!(device.process_message("zigbee2mqtt/W100/action", "single_plus", false));
        assert!(device.process_message("zigbee2mqtt/W100/action", "single_plus", false));

        assert_eq!(
            plus.take_pending_events(),
            vec![
                GenericSwitchPendingEvent::InitialPress { new_position: 1 },
                GenericSwitchPendingEvent::ShortRelease {
                    previous_position: 1,
                },
                GenericSwitchPendingEvent::InitialPress { new_position: 1 },
                GenericSwitchPendingEvent::ShortRelease {
                    previous_position: 1,
                },
            ]
        );
    }
}
