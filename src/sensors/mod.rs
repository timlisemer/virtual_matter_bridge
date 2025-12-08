//! Sensor state management for Matter clusters.
//!
//! This module provides shared state for sensors that can be updated from
//! various input sources (HTTP, simulation, etc.) and read by Matter clusters.

pub mod boolean_sensor;

pub use boolean_sensor::BooleanSensor;
