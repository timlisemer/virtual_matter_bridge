//! Generic binary switch state for Matter OnOff cluster.
//!
//! Provides thread-safe shared state for on/off controls that can be
//! read and written by Matter clusters and updated from external sources.
//!
//! Supports live Matter subscription updates - when the value changes,
//! the notification is pushed instantly to Home Assistant.

use crate::matter::endpoints::endpoints_helpers::{ClusterNotifier, NotifiableSensor, Sensor};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Thread-safe binary switch state.
///
/// Used by Matter clusters to expose on/off control values.
/// Can be updated from any thread (e.g., Matter commands, HTTP handlers).
///
/// Implements the [`Sensor`] trait for change detection - the version
/// is incremented each time the value changes via `set()` or `toggle()`.
pub struct BinarySwitchHelper {
    state: AtomicBool,
    version: AtomicU32,
    notifier: RwLock<Option<ClusterNotifier>>,
}

impl BinarySwitchHelper {
    /// Create a new binary switch with the given initial state.
    pub fn new(initial: bool) -> Self {
        Self {
            state: AtomicBool::new(initial),
            version: AtomicU32::new(0),
            notifier: RwLock::new(None),
        }
    }

    /// Get the current switch state.
    pub fn get(&self) -> bool {
        self.state.load(Ordering::SeqCst)
    }

    /// Set the switch state. Increments version if value changed.
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

    /// Toggle the switch state and return the new value. Always increments version.
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

impl NotifiableSensor for BinarySwitchHelper {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        *self.notifier.write() = Some(notifier);
    }
}

impl Sensor for BinarySwitchHelper {
    fn version(&self) -> u32 {
        self.version.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let switch = BinarySwitchHelper::new(true);
        assert!(switch.get());
        assert_eq!(switch.version(), 0);

        let switch = BinarySwitchHelper::new(false);
        assert!(!switch.get());
        assert_eq!(switch.version(), 0);
    }

    #[test]
    fn test_set_increments_version() {
        let switch = BinarySwitchHelper::new(true);
        assert_eq!(switch.version(), 0);

        switch.set(false);
        assert!(!switch.get());
        assert_eq!(switch.version(), 1);

        // Setting same value doesn't increment
        switch.set(false);
        assert_eq!(switch.version(), 1);

        switch.set(true);
        assert!(switch.get());
        assert_eq!(switch.version(), 2);
    }

    #[test]
    fn test_toggle_increments_version() {
        let switch = BinarySwitchHelper::new(true);
        assert_eq!(switch.version(), 0);

        let new_state = switch.toggle();
        assert!(!new_state);
        assert!(!switch.get());
        assert_eq!(switch.version(), 1);

        let new_state = switch.toggle();
        assert!(new_state);
        assert!(switch.get());
        assert_eq!(switch.version(), 2);
    }
}
