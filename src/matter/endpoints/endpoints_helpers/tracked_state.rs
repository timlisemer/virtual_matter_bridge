use super::{ClusterNotifier, EndpointChangeTracker, NotifiableSensor, Sensor};
use super::{SourceReadiness, SourceSnapshot};
use parking_lot::RwLock;
use std::sync::Arc;

/// Owns endpoint value storage, readiness, and change notification.
pub struct TrackedEndpointState<T> {
    value: RwLock<T>,
    changes: EndpointChangeTracker,
    readiness: Arc<SourceSnapshot<()>>,
}

impl<T> TrackedEndpointState<T> {
    pub fn new(initial_value: T) -> Self {
        Self {
            value: RwLock::new(initial_value),
            changes: EndpointChangeTracker::new(),
            readiness: Arc::new(SourceSnapshot::new()),
        }
    }

    pub fn readiness(&self) -> Arc<dyn SourceReadiness> {
        self.readiness.clone()
    }
}

impl<T: Copy> TrackedEndpointState<T> {
    pub fn get(&self) -> T {
        *self.value.read()
    }
}

impl<T: Clone> TrackedEndpointState<T> {
    pub fn get_cloned(&self) -> T {
        self.value.read().clone()
    }
}

impl<T: PartialEq> TrackedEndpointState<T> {
    pub fn set(&self, value: T) {
        let was_ready = self.readiness.is_ready();
        let mut guard = self.value.write();
        if *guard != value || !was_ready {
            *guard = value;
            self.changes.mark_changed();
        }
        drop(guard);
        self.readiness.mark_ready();
    }
}

impl<T: Send + Sync + 'static> Sensor for TrackedEndpointState<T> {
    fn version(&self) -> u32 {
        self.changes.version()
    }
}

impl<T: Send + Sync + 'static> NotifiableSensor for TrackedEndpointState<T> {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.changes.set_notifier(notifier);
    }
}
