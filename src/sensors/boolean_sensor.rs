//! Boolean sensor state for Matter BooleanState cluster.
//!
//! Provides thread-safe shared state for binary sensors that can be
//! read by Matter clusters and updated from external sources.

use std::sync::atomic::{AtomicBool, Ordering};

/// Thread-safe boolean sensor state.
///
/// Used by the BooleanState Matter cluster to expose sensor values.
/// Can be updated from any thread (e.g., HTTP handlers, simulation tasks).
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
}

impl BooleanSensor {
    /// Create a new boolean sensor with the given initial state.
    pub fn new(initial: bool) -> Self {
        Self {
            state: AtomicBool::new(initial),
        }
    }

    /// Get the current sensor state.
    pub fn get(&self) -> bool {
        self.state.load(Ordering::SeqCst)
    }

    /// Set the sensor state.
    pub fn set(&self, value: bool) {
        self.state.store(value, Ordering::SeqCst);
    }

    /// Toggle the sensor state and return the new value.
    pub fn toggle(&self) -> bool {
        // fetch_xor with true flips the bit
        let old = self.state.fetch_xor(true, Ordering::SeqCst);
        !old
    }
}

// SAFETY: AtomicBool is inherently thread-safe
unsafe impl Sync for BooleanSensor {}
unsafe impl Send for BooleanSensor {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let sensor = BooleanSensor::new(true);
        assert!(sensor.get());

        let sensor = BooleanSensor::new(false);
        assert!(!sensor.get());
    }

    #[test]
    fn test_set() {
        let sensor = BooleanSensor::new(false);
        sensor.set(true);
        assert!(sensor.get());
        sensor.set(false);
        assert!(!sensor.get());
    }

    #[test]
    fn test_toggle() {
        let sensor = BooleanSensor::new(false);

        let new_state = sensor.toggle();
        assert!(new_state);
        assert!(sensor.get());

        let new_state = sensor.toggle();
        assert!(!new_state);
        assert!(!sensor.get());
    }
}
