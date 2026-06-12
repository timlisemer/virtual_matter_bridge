//! Shared helpers for sensors and controls.
//!
//! This module contains utilities used by both sensors and controls:
//! - `change_tracker`: Version tracking and live update notification
//! - `notifier`: Live subscription update notifications
//! - `traits`: Sensor and NotifiableSensor traits for change detection

pub mod binary_state;
pub mod change_tracker;
pub mod notifier;
pub mod readiness;
pub mod scalar_measurement_sensor;
pub mod tracked_state;
pub mod traits;

pub use binary_state::BinaryState;
pub use change_tracker::EndpointChangeTracker;
pub use notifier::{ClusterChangeQueue, ClusterNotifier};
pub use readiness::{
    AlwaysReady, AnyChildReady, ReadinessOnlyHandler, SourceReadiness, SourceSnapshot,
};
pub use scalar_measurement_sensor::ScalarMeasurementSensor;
pub use tracked_state::TrackedEndpointState;
pub use traits::{NotifiableSensor, Sensor};
