//! Boolean sensor state for Matter BooleanState cluster.
//!
//! Provides thread-safe shared state for binary sensors that can be
//! read by Matter clusters and updated from external sources.

use super::Sensor;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Thread-safe boolean sensor state.
///
/// Used by the BooleanState Matter cluster to expose sensor values.
/// Can be updated from any thread (e.g., HTTP handlers, simulation tasks).
///
/// Implements the [`Sensor`] trait for change detection - the version
/// is incremented each time the value changes via `set()` or `toggle()`.
///
/// # Example
/// ```ignore
/// let sensor = Arc::new(BooleanSensor::new(false));
///
/// // Update from HTTP handler
/// sensor.set(true);
///
/// // Read from Matter cluster
/// let value = sensor.get();
/// ```
// TODO: Will be updated via HTTP POST /sensors/{name}
pub struct BooleanSensor {
    state: AtomicBool,
    version: AtomicU32,
}

impl BooleanSensor {
    /// Create a new boolean sensor with the given initial state.
    pub fn new(initial: bool) -> Self {
        Self {
            state: AtomicBool::new(initial),
            version: AtomicU32::new(0),
        }
    }

    /// Get the current sensor state.
    pub fn get(&self) -> bool {
        self.state.load(Ordering::SeqCst)
    }

    /// Set the sensor state. Increments version if value changed.
    pub fn set(&self, value: bool) {
        let old = self.state.swap(value, Ordering::SeqCst);
        if old != value {
            self.version.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Toggle the sensor state and return the new value. Always increments version.
    pub fn toggle(&self) -> bool {
        // fetch_xor with true flips the bit
        let old = self.state.fetch_xor(true, Ordering::SeqCst);
        self.version.fetch_add(1, Ordering::SeqCst);
        !old
    }
}

impl Sensor for BooleanSensor {
    fn version(&self) -> u32 {
        self.version.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let sensor = BooleanSensor::new(true);
        assert!(sensor.get());
        assert_eq!(sensor.version(), 0);

        let sensor = BooleanSensor::new(false);
        assert!(!sensor.get());
        assert_eq!(sensor.version(), 0);
    }

    #[test]
    fn test_set_increments_version() {
        let sensor = BooleanSensor::new(false);
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
        let sensor = BooleanSensor::new(false);
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
