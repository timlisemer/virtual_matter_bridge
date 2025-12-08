//! Matter cluster handlers for the Matter bridge.
//!
//! This module provides handlers that bridge the existing cluster business logic
//! to rs-matter's data model traits.
//!
//! Note: We implement ClusterHandler traits manually rather than using the import! macro
//! because the provisional camera clusters have path resolution issues when used from
//! outside the rs-matter crate.

use super::endpoints::endpoints_helpers::Sensor;
use rs_matter::dm::Dataver;
use std::sync::atomic::{AtomicU32, Ordering};

pub mod boolean_state;
pub mod bridged_device_basic_info;
pub mod camera_av_stream_mgmt;
pub mod occupancy_sensing;
pub mod time_sync;
pub mod webrtc_transport_provider;

// Re-export for convenience
pub use boolean_state::BooleanStateHandler;
pub use bridged_device_basic_info::{BridgedClusterHandler, BridgedHandler};
pub use camera_av_stream_mgmt::CameraAvStreamMgmtHandler;
pub use occupancy_sensing::OccupancySensingHandler;
pub use time_sync::TimeSyncHandler;
pub use webrtc_transport_provider::WebRtcTransportProviderHandler;

/// Sync dataver with sensor version changes.
///
/// Call this at the start of `read_impl()` for any cluster handler backed by a sensor.
/// When the sensor's version has changed since the last read, this bumps the dataver
/// to notify subscribers that the attribute value has changed.
///
/// # Arguments
/// * `sensor` - The sensor to check for changes
/// * `last_version` - Atomic storing the last seen sensor version
/// * `dataver` - The cluster's dataver to bump on changes
///
/// # Example
/// ```ignore
/// fn read_impl(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
///     sync_dataver_with_sensor(&*self.sensor, &self.last_sensor_version, &self.dataver);
///     // ... rest of read logic
/// }
/// ```
pub fn sync_dataver_with_sensor<S: Sensor>(
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
