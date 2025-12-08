//! Sensor state management for Matter clusters.
//!
//! This module provides shared state for sensors that can be updated from
//! various input sources (HTTP, simulation, etc.) and read by Matter clusters.
//!
//! All sensors implement the [`Sensor`] trait which provides version tracking
//! for change detection. This allows Matter cluster handlers to detect when
//! a sensor value has changed and notify subscribers accordingly.

pub mod boolean_sensor;

pub use boolean_sensor::BooleanSensor;

/// Trait for sensors with change detection.
///
/// Any sensor implementing this trait can be used with Matter cluster handlers
/// that need to detect value changes for subscription notifications.
///
/// The version number should be incremented atomically each time the sensor
/// value changes. Handlers compare versions to detect changes and update
/// their `Dataver` to notify subscribers.
///
/// # Example
/// ```ignore
/// impl Sensor for MySensor {
///     fn version(&self) -> u32 {
///         self.version.load(Ordering::SeqCst)
///     }
/// }
/// ```
pub trait Sensor: Send + Sync {
    /// Get the current version number.
    ///
    /// This should be incremented each time the sensor value changes.
    fn version(&self) -> u32;
}
