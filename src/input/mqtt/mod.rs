//! MQTT input source for zigbee2mqtt device integration.
//!
//! This module provides MQTT client functionality to communicate with zigbee2mqtt
//! and translate Zigbee device data into the Virtual Matter Bridge.

mod client;
mod w100;

// TODO: Wire up MQTT bridge to main.rs - these exports will be used then
#[allow(unused_imports)]
pub use client::MqttClient;
#[allow(unused_imports)]
pub use w100::{W100Action, W100Device, W100State};
