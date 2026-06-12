//! TemperatureMeasurement cluster handler.
//!
//! The TemperatureMeasurement cluster (0x0402) represents a temperature sensor.
//! Temperature is reported in centidegrees Celsius (value * 100).
//!
//! For example: 21.5°C is reported as 2150.

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

/// Matter Cluster ID for TemperatureMeasurement
pub const CLUSTER_ID: u32 = 0x0402;

/// Cluster revision
pub const CLUSTER_REVISION: u16 = 4;

/// Attribute IDs for the TemperatureMeasurement cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum TemperatureMeasurementAttribute {
    /// Measured temperature in centidegrees Celsius
    MeasuredValue = 0x0000,
    /// Minimum measurable temperature
    MinMeasuredValue = 0x0001,
    /// Maximum measurable temperature
    MaxMeasuredValue = 0x0002,
    /// Tolerance
    Tolerance = 0x0003,
}

attribute_enum!(TemperatureMeasurementAttribute);

/// Cluster metadata definition
pub const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    feature_map: 0,
    attributes: attributes!(
        Attribute::new(
            TemperatureMeasurementAttribute::MeasuredValue as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            TemperatureMeasurementAttribute::MinMeasuredValue as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            TemperatureMeasurementAttribute::MaxMeasuredValue as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            TemperatureMeasurementAttribute::Tolerance as _,
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

/// Temperature sensor that can be updated from external sources.
pub struct TemperatureSensor {
    state: ScalarMeasurementSensor<i16>,
}

impl TemperatureSensor {
    /// Create a new temperature sensor with initial value.
    ///
    /// # Arguments
    /// * `initial_celsius` - Initial temperature in degrees Celsius
    pub fn new(initial_celsius: f32) -> Self {
        Self {
            state: ScalarMeasurementSensor::new((initial_celsius * 100.0) as i16),
        }
    }

    /// Get the current temperature in degrees Celsius.
    pub fn get_celsius(&self) -> f32 {
        self.get_centidegrees() as f32 / 100.0
    }

    /// Get the current temperature in centidegrees (raw Matter value).
    pub fn get_centidegrees(&self) -> i16 {
        self.raw_value()
    }

    /// Set the temperature in degrees Celsius.
    pub fn set_celsius(&self, celsius: f32) {
        let centidegrees = (celsius * 100.0) as i16;
        self.state.set_raw(centidegrees);
    }

    pub fn readiness(&self) -> Arc<dyn SourceReadiness> {
        self.state.readiness()
    }

    pub fn raw_value(&self) -> i16 {
        self.state.get_raw()
    }
}

impl Sensor for TemperatureSensor {
    fn version(&self) -> u32 {
        self.state.version()
    }
}

impl NotifiableSensor for TemperatureSensor {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.state.set_notifier(notifier);
    }
}

define_scalar_measurement_handler!(
    TemperatureMeasurementHandler,
    TemperatureSensor,
    TemperatureMeasurementAttribute,
    CLUSTER,
    -4000,
    12500,
    i16
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_celsius_marks_first_update_even_when_raw_value_is_unchanged() {
        let sensor = TemperatureSensor::new(21.5);

        sensor.set_celsius(21.5);
        assert_eq!(sensor.version(), 1);

        sensor.set_celsius(21.6);
        assert_eq!(sensor.version(), 2);

        sensor.set_celsius(21.6);
        assert_eq!(sensor.version(), 2);
    }
}
