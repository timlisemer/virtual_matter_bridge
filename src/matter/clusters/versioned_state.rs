use crate::matter::endpoints::endpoints_helpers::Sensor;
use rs_matter::dm::Dataver;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Standard wrapper for sensor-backed cluster dataver synchronization.
pub(crate) struct VersionedClusterState<S> {
    dataver: Dataver,
    sensor: Arc<S>,
    last_sensor_version: AtomicU32,
}

impl<S: Sensor> VersionedClusterState<S> {
    pub(crate) fn new(dataver: Dataver, sensor: Arc<S>) -> Self {
        Self {
            dataver,
            sensor,
            last_sensor_version: AtomicU32::new(0),
        }
    }

    pub(crate) fn dataver(&self) -> &Dataver {
        &self.dataver
    }

    pub(crate) fn sensor(&self) -> &S {
        &self.sensor
    }

    pub(crate) fn sync_dataver(&self) {
        sync_dataver_with_sensor(&*self.sensor, &self.last_sensor_version, &self.dataver);
    }

    pub(crate) fn bump_dataver(&self) {
        self.dataver.changed();
        self.last_sensor_version
            .store(self.sensor.version(), Ordering::SeqCst);
    }
}

pub(crate) fn sync_dataver_with_sensor<S: Sensor + ?Sized>(
    sensor: &S,
    last_version: &AtomicU32,
    dataver: &Dataver,
) {
    let current = sensor.version();
    let last = last_version.swap(current, Ordering::SeqCst);
    if current != last {
        dataver.changed();
    }
}
