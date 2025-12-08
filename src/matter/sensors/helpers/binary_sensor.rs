//! Generic binary sensor state for Matter clusters.
//!
//! Provides thread-safe shared state for binary sensors that can be
//! read by Matter clusters and updated from external sources.
//!
//! Supports live Matter subscription updates - when the value changes,
//! the notification is pushed instantly to Home Assistant.

use super::super::{NotifiableSensor, Sensor};
use super::notifier::ClusterNotifier;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Thread-safe binary sensor state.
///
/// Used by Matter clusters to expose binary sensor values (contact, occupancy, etc.).
/// Can be updated from any thread (e.g., HTTP handlers, simulation tasks).
///
/// Implements the [`Sensor`] trait for change detection - the version
/// is incremented each time the value changes via `set()` or `toggle()`.
pub struct BinarySensor {
    state: AtomicBool,
    version: AtomicU32,
    notifier: RwLock<Option<ClusterNotifier>>,
}

impl BinarySensor {
    /// Create a new binary sensor with the given initial state.
    pub fn new(initial: bool) -> Self {
        Self {
            state: AtomicBool::new(initial),
            version: AtomicU32::new(0),
            notifier: RwLock::new(None),
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
            self.version.fetch_add(1, Ordering::SeqCst);
            if let Some(notifier) = self.notifier.read().as_ref() {
                notifier.notify();
            }
        }
    }

    /// Toggle the sensor state and return the new value. Always increments version.
    ///
    /// If a notifier is configured, immediately pushes the update to
    /// Matter subscribers (e.g., Home Assistant).
    pub fn toggle(&self) -> bool {
        let old = self.state.fetch_xor(true, Ordering::SeqCst);
        self.version.fetch_add(1, Ordering::SeqCst);
        if let Some(notifier) = self.notifier.read().as_ref() {
            notifier.notify();
        }
        !old
    }
}

impl NotifiableSensor for BinarySensor {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        *self.notifier.write() = Some(notifier);
    }
}

impl Sensor for BinarySensor {
    fn version(&self) -> u32 {
        self.version.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let sensor = BinarySensor::new(true);
        assert!(sensor.get());
        assert_eq!(sensor.version(), 0);

        let sensor = BinarySensor::new(false);
        assert!(!sensor.get());
        assert_eq!(sensor.version(), 0);
    }

    #[test]
    fn test_set_increments_version() {
        let sensor = BinarySensor::new(false);
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
        let sensor = BinarySensor::new(false);
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
