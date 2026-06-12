use super::{Sensor, SourceReadiness, TrackedEndpointState};
use crate::matter::endpoints::{ClusterNotifier, NotifiableSensor};
use std::sync::Arc;

pub struct ScalarMeasurementSensor<T> {
    state: TrackedEndpointState<T>,
}

impl<T> ScalarMeasurementSensor<T> {
    pub fn new(raw_value: T) -> Self {
        Self {
            state: TrackedEndpointState::new(raw_value),
        }
    }

    pub fn readiness(&self) -> Arc<dyn SourceReadiness> {
        self.state.readiness()
    }
}

impl<T: Copy> ScalarMeasurementSensor<T> {
    pub fn get_raw(&self) -> T {
        self.state.get()
    }
}

impl<T: PartialEq> ScalarMeasurementSensor<T> {
    pub fn set_raw(&self, raw_value: T) {
        self.state.set(raw_value);
    }
}

impl<T: Send + Sync + 'static> Sensor for ScalarMeasurementSensor<T> {
    fn version(&self) -> u32 {
        self.state.version()
    }
}

impl<T: Send + Sync + 'static> NotifiableSensor for ScalarMeasurementSensor<T> {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.state.set_notifier(notifier);
    }
}
