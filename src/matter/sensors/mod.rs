//! Sensor state management for Matter clusters.
//!
//! This module provides shared state for sensors that can be updated from
//! various input sources (HTTP, simulation, etc.) and read by Matter clusters.
//!
//! All sensors implement the [`Sensor`] trait which provides version tracking
//! for change detection. Sensors that support live updates also implement
//! [`NotifiableSensor`] to push changes instantly to Matter subscribers.

pub mod contact_sensor;
pub mod helpers;
pub mod occupancy_sensor;

pub use contact_sensor::ContactSensor;
pub use helpers::notifier::ClusterNotifier;
pub use occupancy_sensor::OccupancySensor;

/// Trait for sensors with change detection.
///
/// Any sensor implementing this trait can be used with Matter cluster handlers
/// that need to detect value changes for subscription notifications.
///
/// The version number should be incremented atomically each time the sensor
/// value changes. Handlers compare versions to detect changes and update
/// their `Dataver` to notify subscribers.
pub trait Sensor: Send + Sync {
    /// Get the current version number.
    ///
    /// This should be incremented each time the sensor value changes.
    fn version(&self) -> u32;
}

/// Trait for sensors that support live Matter subscription updates.
///
/// Sensors implementing this trait can push updates instantly to Home Assistant
/// when their values change, rather than waiting for polling.
pub trait NotifiableSensor: Sensor {
    /// Set the notifier for this sensor.
    ///
    /// Called during Matter stack setup to wire the sensor to the
    /// subscription notification system.
    fn set_notifier(&self, notifier: ClusterNotifier);
}
