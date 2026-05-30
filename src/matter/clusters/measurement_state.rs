use crate::matter::endpoints::endpoints_helpers::{
    EndpointChangeTracker, Sensor, SourceReadiness, SourceSnapshot,
};
use crate::matter::endpoints::{ClusterNotifier, NotifiableSensor};
use parking_lot::RwLock;
use std::sync::Arc;

pub struct MeasurementState<T> {
    value: RwLock<T>,
    changes: EndpointChangeTracker,
    readiness: Arc<SourceSnapshot<()>>,
}

impl<T> MeasurementState<T> {
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

impl<T: Copy> MeasurementState<T> {
    pub fn get(&self) -> T {
        *self.value.read()
    }
}

impl<T: Copy + PartialEq> MeasurementState<T> {
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

impl<T: Send + Sync + 'static> Sensor for MeasurementState<T> {
    fn version(&self) -> u32 {
        self.changes.version()
    }
}

impl<T: Send + Sync + 'static> NotifiableSensor for MeasurementState<T> {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.changes.set_notifier(notifier);
    }
}
