//! Shelly 2PM Gen4 MQTT device abstraction.
//!
//! Parses zigbee2mqtt state for a two-channel Shelly relay and provides
//! Matter `EndpointHandler` implementations for each controllable channel.

use crate::matter::endpoints::EndpointHandler;
use crate::matter::endpoints::{SourceReadiness, SourceSnapshot};
use log::{info, warn};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;

type StatePusher = Arc<dyn Fn(bool) + Send + Sync>;

/// Zigbee2MQTT state payload for the Shelly 2PM.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Shelly2PmState {
    pub state_l1: Option<Shelly2PmSwitchState>,
    pub state_l2: Option<Shelly2PmSwitchState>,
    pub linkquality: Option<u16>,
    pub power_l1: Option<f64>,
    pub power_l2: Option<f64>,
    pub current_l1: Option<f64>,
    pub current_l2: Option<f64>,
    pub voltage_l1: Option<f64>,
    pub voltage_l2: Option<f64>,
    pub energy_l1: Option<f64>,
    pub energy_l2: Option<f64>,
    pub wifi_status: Option<String>,
}

/// Zigbee2MQTT ON/OFF value.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Shelly2PmSwitchState {
    On,
    Off,
}

impl Shelly2PmSwitchState {
    pub fn as_bool(self) -> bool {
        matches!(self, Self::On)
    }

    pub fn from_bool(value: bool) -> Self {
        if value { Self::On } else { Self::Off }
    }
}

/// Physical Shelly relay channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shelly2PmChannel {
    L1,
    L2,
}

impl Shelly2PmChannel {
    pub fn state_key(self) -> &'static str {
        match self {
            Self::L1 => "state_l1",
            Self::L2 => "state_l2",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::L1 => "L1",
            Self::L2 => "L2",
        }
    }
}

/// MQTT command emitted by a Matter endpoint command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shelly2PmCommand {
    pub friendly_name: String,
    pub channel: Shelly2PmChannel,
    pub state: Shelly2PmSwitchState,
}

impl Shelly2PmCommand {
    pub fn new(friendly_name: impl Into<String>, channel: Shelly2PmChannel, value: bool) -> Self {
        Self {
            friendly_name: friendly_name.into(),
            channel,
            state: Shelly2PmSwitchState::from_bool(value),
        }
    }

    pub fn set_topic(&self) -> String {
        format!("zigbee2mqtt/{}/set", self.friendly_name)
    }

    pub fn payload(&self) -> String {
        let state = match self.state {
            Shelly2PmSwitchState::On => "ON",
            Shelly2PmSwitchState::Off => "OFF",
        };
        serde_json::json!({ self.channel.state_key(): state }).to_string()
    }
}

/// Matter endpoint handler for one Shelly 2PM channel.
pub struct Shelly2PmChannelHandler {
    friendly_name: String,
    channel: Shelly2PmChannel,
    state: Arc<SourceSnapshot<bool>>,
    pusher: RwLock<Option<StatePusher>>,
    command_tx: mpsc::Sender<Shelly2PmCommand>,
}

impl Shelly2PmChannelHandler {
    pub fn new(
        friendly_name: impl Into<String>,
        channel: Shelly2PmChannel,
        _initial: bool,
        command_tx: mpsc::Sender<Shelly2PmCommand>,
    ) -> Self {
        Self {
            friendly_name: friendly_name.into(),
            channel,
            state: Arc::new(SourceSnapshot::new()),
            pusher: RwLock::new(None),
            command_tx,
        }
    }

    pub fn channel(&self) -> Shelly2PmChannel {
        self.channel
    }

    pub fn set_from_mqtt(&self, value: bool) {
        self.set_state(value, true);
    }

    fn set_state(&self, value: bool, push: bool) {
        let old = self.state.snapshot();
        self.state.update_source(value);
        if old != Some(value) {
            info!(
                "[Shelly 2PM] {} {} state updated: {}",
                self.friendly_name,
                self.channel.label(),
                if value { "ON" } else { "OFF" }
            );
        }
        if push
            && old != Some(value)
            && let Some(pusher) = self.pusher.read().as_ref()
        {
            pusher(value);
        }
    }

    fn queue_command(&self, value: bool) {
        let command = Shelly2PmCommand::new(&self.friendly_name, self.channel, value);
        if let Err(e) = self.command_tx.try_send(command) {
            warn!(
                "[Shelly 2PM] Failed to queue command for {} {}: {}",
                self.friendly_name,
                self.channel.label(),
                e
            );
        }
    }
}

impl EndpointHandler for Shelly2PmChannelHandler {
    fn on_command(&self, value: bool) {
        info!(
            "[Matter] Shelly 2PM {} {} command: {}",
            self.friendly_name,
            self.channel.label(),
            if value { "ON" } else { "OFF" }
        );
        if !self.state.is_ready() {
            warn!(
                "[Matter] Ignoring pre-ready Shelly 2PM {} {} command",
                self.friendly_name,
                self.channel.label()
            );
            return;
        }
        self.set_state(value, false);
        self.queue_command(value);
    }

    fn get_state(&self) -> Option<bool> {
        self.state.snapshot()
    }

    fn readiness(&self) -> Arc<dyn SourceReadiness> {
        self.state.clone()
    }

    fn set_state_pusher(&self, pusher: StatePusher) {
        *self.pusher.write() = Some(pusher);
    }
}

/// Create both channel handlers and the command receiver consumed by MQTT.
pub fn shelly_2pm_channel_pair(
    friendly_name: &str,
) -> (
    Arc<Shelly2PmChannelHandler>,
    Arc<Shelly2PmChannelHandler>,
    mpsc::Receiver<Shelly2PmCommand>,
) {
    let (command_tx, command_rx) = mpsc::channel(32);
    let l1 = Arc::new(Shelly2PmChannelHandler::new(
        friendly_name,
        Shelly2PmChannel::L1,
        false,
        command_tx.clone(),
    ));
    let l2 = Arc::new(Shelly2PmChannelHandler::new(
        friendly_name,
        Shelly2PmChannel::L2,
        false,
        command_tx,
    ));

    (l1, l2, command_rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const RETAINED_STATE: &str = r#"{
      "ac_frequency_l1":49.99,
      "ac_frequency_l2":49.99,
      "current_l1":1.23,
      "current_l2":0,
      "dhcp_enabled":true,
      "energy_l1":215.24,
      "energy_l2":0.12,
      "ip_address":"10.0.0.98",
      "linkquality":148,
      "power_l1":269,
      "power_l2":0,
      "state_l1":"ON",
      "state_l2":"OFF",
      "voltage_l1":231.67,
      "voltage_l2":229.76,
      "wifi_status":"got ip"
    }"#;

    #[test]
    fn parses_retained_shelly_state() {
        let state: Shelly2PmState = serde_json::from_str(RETAINED_STATE).unwrap();

        assert_eq!(state.state_l1, Some(Shelly2PmSwitchState::On));
        assert_eq!(state.state_l2, Some(Shelly2PmSwitchState::Off));
        assert_eq!(state.linkquality, Some(148));
        assert_eq!(state.power_l1, Some(269.0));
        assert_eq!(state.voltage_l2, Some(229.76));
        assert_eq!(state.wifi_status.as_deref(), Some("got ip"));
    }

    #[test]
    fn command_payload_targets_only_one_channel() {
        let l1 = Shelly2PmCommand::new("Büro Licht & PC Schalter", Shelly2PmChannel::L1, true);
        let l2 = Shelly2PmCommand::new("Büro Licht & PC Schalter", Shelly2PmChannel::L2, false);

        assert_eq!(l1.set_topic(), "zigbee2mqtt/Büro Licht & PC Schalter/set");
        assert_eq!(l1.payload(), r#"{"state_l1":"ON"}"#);
        assert_eq!(l2.payload(), r#"{"state_l2":"OFF"}"#);
    }

    #[test]
    fn mqtt_state_update_pushes_to_matter_on_change() {
        let (tx, _rx) = mpsc::channel(4);
        let handler = Shelly2PmChannelHandler::new(
            "Büro Licht & PC Schalter",
            Shelly2PmChannel::L1,
            false,
            tx,
        );
        let pushes = Arc::new(AtomicUsize::new(0));
        let pushes_for_pusher = pushes.clone();
        handler.set_state_pusher(Arc::new(move |_| {
            pushes_for_pusher.fetch_add(1, Ordering::SeqCst);
        }));

        handler.set_from_mqtt(true);
        handler.set_from_mqtt(true);
        handler.set_from_mqtt(false);

        assert_eq!(handler.get_state(), Some(false));
        assert_eq!(pushes.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn matter_command_updates_state_and_queues_mqtt_command_after_ready() {
        let (tx, mut rx) = mpsc::channel(4);
        let handler = Shelly2PmChannelHandler::new(
            "Büro Licht & PC Schalter",
            Shelly2PmChannel::L2,
            false,
            tx,
        );

        handler.set_from_mqtt(false);
        handler.on_command(true);

        assert_eq!(handler.get_state(), Some(true));
        assert_eq!(
            rx.recv().await,
            Some(Shelly2PmCommand::new(
                "Büro Licht & PC Schalter",
                Shelly2PmChannel::L2,
                true
            ))
        );
    }

    #[tokio::test]
    async fn matter_command_does_not_push_external_state_change() {
        let (tx, _rx) = mpsc::channel(4);
        let handler = Shelly2PmChannelHandler::new(
            "Büro Licht & PC Schalter",
            Shelly2PmChannel::L2,
            false,
            tx,
        );
        let pushes = Arc::new(AtomicUsize::new(0));
        let pushes_for_pusher = pushes.clone();
        handler.set_state_pusher(Arc::new(move |_| {
            pushes_for_pusher.fetch_add(1, Ordering::SeqCst);
        }));

        handler.set_from_mqtt(false);
        handler.on_command(true);

        assert_eq!(handler.get_state(), Some(true));
        assert_eq!(pushes.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn pre_ready_matter_command_does_not_queue_mqtt_command() {
        let (tx, mut rx) = mpsc::channel(4);
        let handler = Shelly2PmChannelHandler::new(
            "Büro Licht & PC Schalter",
            Shelly2PmChannel::L2,
            false,
            tx,
        );

        handler.on_command(true);

        assert_eq!(handler.get_state(), None);
        assert!(rx.try_recv().is_err());
    }
}
