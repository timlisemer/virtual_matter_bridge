//! Shared helpers for sensors and controls.
//!
//! This module contains utilities used by both sensors and controls:
//! - `notifier`: Live subscription update notifications
//! - `traits`: Sensor and NotifiableSensor traits for change detection

pub mod notifier;
pub mod traits;

pub use notifier::ClusterNotifier;
pub use traits::{NotifiableSensor, Sensor};
