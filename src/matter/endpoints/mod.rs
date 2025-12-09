//! Matter endpoints - sensors and controls.
//!
//! This module organizes Matter endpoint components:
//! - `sensors`: Read-only state (contact, occupancy, etc.)
//! - `controls`: Read-write state (switches, lights, etc.)
//! - `endpoints_helpers`: Shared utilities (notifier, traits)
//! - `handler`: EndpointHandler trait for bidirectional communication

pub mod controls;
pub mod endpoints_helpers;
pub mod handler;
pub mod sensors;

// Re-export key types for convenience
pub use endpoints_helpers::{ClusterNotifier, NotifiableSensor};
pub use handler::EndpointHandler;
