//! Binary switch state alias for Matter OnOff controls.

use crate::matter::endpoints::endpoints_helpers::BinaryState;

/// Thread-safe binary switch state with version tracking and live notifications.
pub type BinarySwitchHelper = BinaryState;
