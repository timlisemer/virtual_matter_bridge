//! Generic binary sensor state for Matter clusters.
//!
//! Provides thread-safe shared state for binary sensors that can be
//! read by Matter clusters and updated from external sources.
//!
//! Supports live Matter subscription updates - when the value changes,
//! the notification is pushed instantly to Home Assistant.

use crate::matter::endpoints::endpoints_helpers::{
    ClusterNotifier, EndpointChangeTracker, NotifiableSensor, Sensor,
};
use std::sync::atomic::{AtomicBool, Ordering};

/// Thread-safe binary sensor state.
///
/// Used by Matter clusters to expose binary sensor values (contact, occupancy, etc.).
/// Can be updated from any thread (e.g., HTTP handlers, simulation tasks).
///
/// Implements the [`Sensor`] trait for change detection - the version
/// is incremented each time the value changes via `set()` or `toggle()`.
pub struct BinarySensorHelper {
    state: AtomicBool,
    changes: EndpointChangeTracker,
}

impl BinarySensorHelper {
    /// Create a new binary sensor with the given initial state.
    pub fn new(initial: bool) -> Self {
        Self {
            state: AtomicBool::new(initial),
            changes: EndpointChangeTracker::new(),
        }
    }

    /// Get the current sensor state.
    pub fn get(&self) -> bool {
        self.state.load(Ordering::SeqCst)
    }

    /// Set the sensor state. Increments version if value changed.
    ///
    /// If a notifier is configured, immediately pushes the update to
    /// Matter subscribers (e.g., Home Assistant).
    pub fn set(&self, value: bool) {
        let old = self.state.swap(value, Ordering::SeqCst);
        if old != value {
            self.changes.mark_changed();
        }
    }

    /// Toggle the sensor state and return the new value. Always increments version.
    ///
    /// If a notifier is configured, immediately pushes the update to
    /// Matter subscribers (e.g., Home Assistant).
    pub fn toggle(&self) -> bool {
        let old = self.state.fetch_xor(true, Ordering::SeqCst);
        self.changes.mark_changed();
        !old
    }
}

impl NotifiableSensor for BinarySensorHelper {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.changes.set_notifier(notifier);
    }
}

impl Sensor for BinarySensorHelper {
    fn version(&self) -> u32 {
        self.changes.version()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let sensor = BinarySensorHelper::new(true);
        assert!(sensor.get());
        assert_eq!(sensor.version(), 0);

        let sensor = BinarySensorHelper::new(false);
        assert!(!sensor.get());
        assert_eq!(sensor.version(), 0);
    }

    #[test]
    fn test_set_increments_version() {
        let sensor = BinarySensorHelper::new(false);
        assert_eq!(sensor.version(), 0);

        sensor.set(true);
        assert!(sensor.get());
        assert_eq!(sensor.version(), 1);

        // Setting same value doesn't increment
        sensor.set(true);
        assert_eq!(sensor.version(), 1);

        sensor.set(false);
        assert!(!sensor.get());
        assert_eq!(sensor.version(), 2);
    }

    #[test]
    fn test_toggle_increments_version() {
        let sensor = BinarySensorHelper::new(false);
        assert_eq!(sensor.version(), 0);

        let new_state = sensor.toggle();
        assert!(new_state);
        assert!(sensor.get());
        assert_eq!(sensor.version(), 1);

        let new_state = sensor.toggle();
        assert!(!new_state);
        assert!(!sensor.get());
        assert_eq!(sensor.version(), 2);
    }
}
