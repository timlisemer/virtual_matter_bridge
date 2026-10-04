//! W100 battery readings in the Matter Power Source cluster.
use super::read_only_cluster::define_versioned_read_only_cluster_handler;
use crate::matter::endpoints::endpoints_helpers::{
    ScalarMeasurementSensor, Sensor, SourceReadiness,
};
use crate::matter::endpoints::{ClusterNotifier, NotifiableSensor};
use rs_matter::dm::{Access, Attribute, Cluster, Quality};
use rs_matter::error::Error;
use rs_matter::tlv::TLVWrite;
use rs_matter::{attribute_enum, attributes, with};
use std::sync::Arc;
use strum::FromRepr;

#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum BatteryAttribute {
    Status = 0,
    Order = 1,
    Description = 2,
    BatPercentRemaining = 12,
    BatChargeLevel = 14,
    BatReplacementNeeded = 15,
    BatReplaceability = 16,
    BatPresent = 17,
    BatReplacementDescription = 19,
    BatQuantity = 22,
    EndpointList = 31,
}
attribute_enum!(BatteryAttribute);
pub const CLUSTER: Cluster<'static> = Cluster {
    id: 0x002f,
    revision: 3,
    feature_map: 6, // Battery | Replaceable
    attributes: attributes!(
        Attribute::new(0, Access::RV, Quality::NONE),
        Attribute::new(1, Access::RV, Quality::NONE),
        Attribute::new(2, Access::RV, Quality::NONE),
        Attribute::new(12, Access::RV, Quality::NULLABLE),
        Attribute::new(14, Access::RV, Quality::NONE),
        Attribute::new(15, Access::RV, Quality::NONE),
        Attribute::new(16, Access::RV, Quality::NONE),
        Attribute::new(17, Access::RV, Quality::NONE),
        Attribute::new(19, Access::RV, Quality::NONE),
        Attribute::new(22, Access::RV, Quality::NONE),
        Attribute::new(31, Access::RV, Quality::NONE),
    ),
    commands: &[],
    events: &[],
    with_attrs: with!(all),
    with_cmds: with!(all),
    with_events: with!(all),
};

pub struct BatterySensor {
    state: ScalarMeasurementSensor<u8>,
}
impl Default for BatterySensor {
    fn default() -> Self {
        Self::new()
    }
}
impl BatterySensor {
    pub fn new() -> Self {
        Self {
            state: ScalarMeasurementSensor::new(0),
        }
    }
    pub fn set_percent(&self, percent: u8) {
        self.state.set_raw(percent.min(100) * 2);
    }
    pub fn half_percent(&self) -> Option<u8> {
        self.state
            .readiness()
            .is_ready()
            .then(|| self.state.get_raw())
    }
    pub fn readiness(&self) -> Arc<dyn SourceReadiness> {
        self.state.readiness()
    }
}
impl Sensor for BatterySensor {
    fn version(&self) -> u32 {
        self.state.version()
    }
}
impl NotifiableSensor for BatterySensor {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.state.set_notifier(notifier);
    }
}

define_versioned_read_only_cluster_handler!(
    BatteryHandler,
    BatterySensor,
    BatteryAttribute,
    CLUSTER,
    |sensor, tw, tag, attribute| {
        match attribute {
            BatteryAttribute::Status => {
                tw.u8(tag, if sensor.readiness().is_ready() { 1 } else { 3 })?
            }
            BatteryAttribute::Order => tw.u8(tag, 0)?,
            BatteryAttribute::Description => tw.utf8(tag, "W100 battery")?,
            BatteryAttribute::BatPercentRemaining => match sensor.half_percent() {
                Some(value) => tw.u8(tag, value)?,
                None => tw.null(tag)?,
            },
            BatteryAttribute::BatChargeLevel => tw.u8(
                tag,
                match sensor.half_percent() {
                    Some(0..=10) => 2,
                    Some(11..=40) => 1,
                    _ => 0,
                },
            )?,
            BatteryAttribute::BatReplacementNeeded => {
                tw.bool(tag, sensor.half_percent().is_some_and(|v| v <= 10))?
            }
            BatteryAttribute::BatReplaceability => tw.u8(tag, 1)?,
            BatteryAttribute::BatPresent => tw.bool(tag, true)?,
            BatteryAttribute::BatReplacementDescription => tw.utf8(tag, "2 CR2450")?,
            BatteryAttribute::BatQuantity => tw.u8(tag, 2)?,
            BatteryAttribute::EndpointList => {
                tw.start_array(tag)?;
                tw.end_container()?;
            }
        }
        Ok::<_, Error>(())
    }
);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn battery_has_no_reading_before_source_and_uses_half_percent_units() {
        let sensor = BatterySensor::new();
        assert_eq!(sensor.half_percent(), None);
        sensor.set_percent(43);
        assert_eq!(sensor.half_percent(), Some(86));
        sensor.set_percent(255);
        assert_eq!(sensor.half_percent(), Some(200));
    }
}
