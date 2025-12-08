//! Switch control for Matter OnOff cluster.
//!
//! Exposes as a generic on/off switch that can be added to any Matter device.

use super::helpers::SwitchHelper;

/// On/Off switch control.
///
/// Type alias for [`SwitchHelper`] - exposed as Matter OnOff cluster.
/// Default state is `true` (on) - can be used for device power controls.
pub type Switch = SwitchHelper;
