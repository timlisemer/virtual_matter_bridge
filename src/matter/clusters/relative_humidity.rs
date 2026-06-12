//! RelativeHumidityMeasurement cluster handler.
//!
//! The RelativeHumidityMeasurement cluster (0x0405) represents a humidity sensor.
//! Humidity is reported in centi-percent (value * 100).
//!
//! For example: 55.5% is reported as 5550.

use super::read_only_cluster::define_versioned_read_only_cluster_handler;
use super::scalar_measurement::define_scalar_measurement_handler;
use rs_matter::dm::{Access, Attribute, Cluster, Quality};
use rs_matter::tlv::TLVWrite;
use rs_matter::{attribute_enum, attributes, with};
use std::sync::Arc;
use strum::FromRepr;

use crate::matter::endpoints::endpoints_helpers::{
    ScalarMeasurementSensor, Sensor, SourceReadiness,
};
use crate::matter::endpoints::{ClusterNotifier, NotifiableSensor};

/// Matter Cluster ID for RelativeHumidityMeasurement
pub const CLUSTER_ID: u32 = 0x0405;

/// Cluster revision
pub const CLUSTER_REVISION: u16 = 3;

/// Attribute IDs for the RelativeHumidityMeasurement cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum RelativeHumidityAttribute {
    /// Measured humidity in centi-percent
    MeasuredValue = 0x0000,
    /// Minimum measurable humidity
    MinMeasuredValue = 0x0001,
    /// Maximum measurable humidity
    MaxMeasuredValue = 0x0002,
    /// Tolerance
    Tolerance = 0x0003,
}

attribute_enum!(RelativeHumidityAttribute);

/// Cluster metadata definition
pub const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    feature_map: 0,
    attributes: attributes!(
        Attribute::new(
            RelativeHumidityAttribute::MeasuredValue as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            RelativeHumidityAttribute::MinMeasuredValue as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            RelativeHumidityAttribute::MaxMeasuredValue as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            RelativeHumidityAttribute::Tolerance as _,
            Access::RV,
            Quality::NONE
        ),
    ),
    commands: &[],
    events: &[],
    with_attrs: with!(all),
    with_cmds: with!(all),
    with_events: with!(all),
};

/// Humidity sensor that can be updated from external sources.
pub struct HumiditySensor {
    state: ScalarMeasurementSensor<u16>,
}

impl HumiditySensor {
    /// Create a new humidity sensor with initial value.
    ///
    /// # Arguments
    /// * `initial_percent` - Initial humidity in percent (0-100)
    pub fn new(initial_percent: f32) -> Self {
        Self {
            state: ScalarMeasurementSensor::new((initial_percent * 100.0) as u16),
        }
    }

    /// Get the current humidity in percent.
    pub fn get_percent(&self) -> f32 {
        self.get_centipercent() as f32 / 100.0
    }

    /// Get the current humidity in centi-percent (raw Matter value).
    pub fn get_centipercent(&self) -> u16 {
        self.raw_value()
    }

    /// Set the humidity in percent.
    pub fn set_percent(&self, percent: f32) {
        let centipercent = (percent * 100.0) as u16;
        self.state.set_raw(centipercent);
    }

    pub fn readiness(&self) -> Arc<dyn SourceReadiness> {
        self.state.readiness()
    }

    pub fn raw_value(&self) -> u16 {
        self.state.get_raw()
    }
}

impl Sensor for HumiditySensor {
    fn version(&self) -> u32 {
        self.state.version()
    }
}

impl NotifiableSensor for HumiditySensor {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.state.set_notifier(notifier);
    }
}

define_scalar_measurement_handler!(
    RelativeHumidityHandler,
    HumiditySensor,
    RelativeHumidityAttribute,
    CLUSTER,
    0,
    10000,
    u16
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_percent_marks_first_update_even_when_raw_value_is_unchanged() {
        let sensor = HumiditySensor::new(45.5);

        sensor.set_percent(45.5);
        assert_eq!(sensor.version(), 1);

        sensor.set_percent(46.0);
        assert_eq!(sensor.version(), 2);

        sensor.set_percent(46.0);
        assert_eq!(sensor.version(), 2);
    }
}
