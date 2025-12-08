//! Occupancy sensor for Matter OccupancySensing cluster.
//!
//! Exposes as Matter Occupancy Sensor device type (0x0107).

use super::binary_sensor::BinarySensor;

/// Occupancy sensor (motion/presence detection).
///
/// Type alias for [`BinarySensor`] - exposed as Matter Occupancy Sensor (0x0107)
/// using the OccupancySensing cluster (0x0406).
pub type OccupancySensor = BinarySensor;
