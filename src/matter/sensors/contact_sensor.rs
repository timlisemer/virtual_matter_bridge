//! Contact sensor for Matter BooleanState cluster.
//!
//! Exposes as Matter Contact Sensor device type (0x0015).

use super::helpers::binary_sensor::BinarySensor;

/// Contact sensor (door/window open/close).
///
/// Type alias for [`BinarySensor`] - exposed as Matter Contact Sensor (0x0015)
/// using the BooleanState cluster (0x0045).
pub type ContactSensor = BinarySensor;
