//! Sensor state management for Matter clusters.
//!
//! This module provides shared state for sensors that can be updated from
//! various input sources (HTTP, simulation, etc.) and read by Matter clusters.

pub mod contact_sensor;
pub mod helpers;
pub mod occupancy_sensor;

pub use contact_sensor::ContactSensor;
pub use occupancy_sensor::OccupancySensor;
