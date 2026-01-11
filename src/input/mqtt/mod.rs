//! MQTT input source for zigbee2mqtt device integration.
//!
//! This module provides MQTT client functionality to communicate with zigbee2mqtt
//! and translate Zigbee device data into the Virtual Matter Bridge.

mod client;
mod integration;
mod w100;

// Main API - clean integration for use in main.rs
pub use integration::{MqttIntegration, W100Config};

// Legacy exports for test binary and reference
#[allow(unused_imports)]
pub use client::MqttClient;
#[allow(unused_imports)]
pub use w100::{W100Action, W100Device, W100State};
