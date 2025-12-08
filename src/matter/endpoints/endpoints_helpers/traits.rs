//! Shared traits for sensors and controls.
//!
//! These traits provide change detection and live update support for Matter endpoints.

use super::notifier::ClusterNotifier;

/// Trait for endpoints with change detection.
///
/// Any endpoint implementing this trait can be used with Matter cluster handlers
/// that need to detect value changes for subscription notifications.
///
/// The version number should be incremented atomically each time the endpoint
/// value changes. Handlers compare versions to detect changes and update
/// their `Dataver` to notify subscribers.
pub trait Sensor: Send + Sync {
    /// Get the current version number.
    ///
    /// This should be incremented each time the value changes.
    fn version(&self) -> u32;
}

/// Trait for endpoints that support live Matter subscription updates.
///
/// Endpoints implementing this trait can push updates instantly to Home Assistant
/// when their values change, rather than waiting for polling.
pub trait NotifiableSensor: Sensor {
    /// Set the notifier for this endpoint.
    ///
    /// Called during Matter stack setup to wire the endpoint to the
    /// subscription notification system.
    fn set_notifier(&self, notifier: ClusterNotifier);
}
