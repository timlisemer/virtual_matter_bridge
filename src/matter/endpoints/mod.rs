//! Matter endpoints - sensors and controls.
//!
//! This module organizes Matter endpoint components:
//! - `sensors`: Read-only state (contact, occupancy, etc.)
//! - `controls`: Read-write state (switches, lights, etc.)
//! - `endpoints_helpers`: Shared utilities (notifier, traits)

pub mod controls;
pub mod endpoints_helpers;
pub mod sensors;

// Re-export key types for convenience (allow unused as these are public API)
#[allow(unused_imports)]
pub use controls::{LightSwitch, Switch};
#[allow(unused_imports)]
pub use endpoints_helpers::{ClusterNotifier, NotifiableSensor, Sensor};
#[allow(unused_imports)]
pub use sensors::{ContactSensor, OccupancySensor};
