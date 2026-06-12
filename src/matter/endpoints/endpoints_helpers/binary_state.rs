//! Shared thread-safe boolean state with Matter subscription change tracking.

use super::{ClusterNotifier, EndpointChangeTracker, NotifiableSensor, Sensor};
use std::sync::atomic::{AtomicBool, Ordering};

/// Thread-safe boolean state with change versioning and optional notifications.
pub struct BinaryState {
    state: AtomicBool,
    changes: EndpointChangeTracker,
}

impl BinaryState {
    /// Create a new binary state value.
    pub fn new(initial: bool) -> Self {
        Self {
            state: AtomicBool::new(initial),
            changes: EndpointChangeTracker::new(),
        }
    }

    /// Return the current boolean state.
    pub fn get(&self) -> bool {
        self.state.load(Ordering::SeqCst)
    }

    /// Set the current state, marking a change only when the value differs.
    pub fn set(&self, value: bool) {
        let old = self.state.swap(value, Ordering::SeqCst);
        if old != value {
            self.changes.mark_changed();
        }
    }

    /// Toggle the state, always marking a change, and return the new value.
    pub fn toggle(&self) -> bool {
        let old = self.state.fetch_xor(true, Ordering::SeqCst);
        self.changes.mark_changed();
        !old
    }
}

impl NotifiableSensor for BinaryState {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.changes.set_notifier(notifier);
    }
}

impl Sensor for BinaryState {
    fn version(&self) -> u32 {
        self.changes.version()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_initial_state() {
        let state = BinaryState::new(true);
        assert!(state.get());
        assert_eq!(state.version(), 0);

        let state = BinaryState::new(false);
        assert!(!state.get());
        assert_eq!(state.version(), 0);
    }

    #[test]
    fn set_increments_version_only_when_changed() {
        let state = BinaryState::new(false);
        assert_eq!(state.version(), 0);

        state.set(true);
        assert!(state.get());
        assert_eq!(state.version(), 1);

        state.set(true);
        assert_eq!(state.version(), 1);

        state.set(false);
        assert!(!state.get());
        assert_eq!(state.version(), 2);
    }

    #[test]
    fn toggle_increments_version() {
        let state = BinaryState::new(false);
        assert_eq!(state.version(), 0);

        let new_state = state.toggle();
        assert!(new_state);
        assert!(state.get());
        assert_eq!(state.version(), 1);

        let new_state = state.toggle();
        assert!(!new_state);
        assert!(!state.get());
        assert_eq!(state.version(), 2);
    }
}
