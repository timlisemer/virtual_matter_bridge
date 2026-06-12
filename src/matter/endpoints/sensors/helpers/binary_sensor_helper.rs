//! Binary sensor state alias for Matter sensor clusters.

use crate::matter::endpoints::endpoints_helpers::BinaryState;

/// Thread-safe binary sensor state with version tracking and live notifications.
pub type BinarySensorHelper = BinaryState;
