//! ElectricalPowerMeasurement cluster handler.
//!
//! Exposes Shelly per-channel electrical power telemetry using Matter cluster
//! 0x0090.

use super::read_only_cluster::define_versioned_read_only_cluster_handler;
use crate::matter::clusters::tlv_helpers::write_nullable;
use crate::matter::endpoints::endpoints_helpers::{Sensor, SourceReadiness, TrackedEndpointState};
use crate::matter::endpoints::{ClusterNotifier, NotifiableSensor};
use rs_matter::dm::{Access, Attribute, Cluster, Quality};
use rs_matter::error::Error;
use rs_matter::tlv::{TLVTag, TLVWrite};
use rs_matter::{attribute_enum, attributes, with};
use std::sync::Arc;
use strum::FromRepr;

pub const CLUSTER_ID: u32 = 0x0090;
pub const CLUSTER_REVISION: u16 = 3;
const FEATURE_MAP: u32 = 0x12; // AlternatingCurrent | PowerQuality

const POWER_MODE_AC: u8 = 2;
const MEASUREMENT_COUNT: u8 = 7;

const MEASUREMENT_ACTIVE_POWER: u16 = 5;
const MEASUREMENT_REACTIVE_POWER: u16 = 6;
const MEASUREMENT_APPARENT_POWER: u16 = 7;
const MEASUREMENT_RMS_VOLTAGE: u16 = 8;
const MEASUREMENT_RMS_CURRENT: u16 = 9;
const MEASUREMENT_FREQUENCY: u16 = 11;
const MEASUREMENT_POWER_FACTOR: u16 = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum ElectricalPowerMeasurementAttribute {
    PowerMode = 0x0000,
    NumberOfMeasurementTypes = 0x0001,
    Accuracy = 0x0002,
    ActivePower = 0x0008,
    ReactivePower = 0x0009,
    ApparentPower = 0x000A,
    RmsVoltage = 0x000B,
    RmsCurrent = 0x000C,
    Frequency = 0x000E,
    PowerFactor = 0x0011,
}

attribute_enum!(ElectricalPowerMeasurementAttribute);

pub const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    feature_map: FEATURE_MAP,
    attributes: attributes!(
        Attribute::new(
            ElectricalPowerMeasurementAttribute::PowerMode as _,
            Access::RV,
            Quality::NONE
        ),
        Attribute::new(
            ElectricalPowerMeasurementAttribute::NumberOfMeasurementTypes as _,
            Access::RV,
            Quality::NONE
        ),
        Attribute::new(
            ElectricalPowerMeasurementAttribute::Accuracy as _,
            Access::RV,
            Quality::NONE
        ),
        Attribute::new(
            ElectricalPowerMeasurementAttribute::ActivePower as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            ElectricalPowerMeasurementAttribute::ReactivePower as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            ElectricalPowerMeasurementAttribute::ApparentPower as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            ElectricalPowerMeasurementAttribute::RmsVoltage as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            ElectricalPowerMeasurementAttribute::RmsCurrent as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            ElectricalPowerMeasurementAttribute::Frequency as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            ElectricalPowerMeasurementAttribute::PowerFactor as _,
            Access::RV,
            Quality::NULLABLE
        ),
    ),
    commands: &[],
    events: &[],
    with_attrs: with!(all),
    with_cmds: with!(all),
    with_events: with!(all),
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ElectricalPowerValues {
    pub active_power_mw: Option<i64>,
    pub reactive_power_mvar: Option<i64>,
    pub apparent_power_mva: Option<i64>,
    pub rms_voltage_mv: Option<i64>,
    pub rms_current_ma: Option<i64>,
    pub frequency_mhz: Option<i64>,
    pub power_factor_centipercent: Option<i64>,
}

pub struct ElectricalPowerState {
    values: TrackedEndpointState<ElectricalPowerValues>,
}

impl ElectricalPowerState {
    pub fn new() -> Self {
        Self {
            values: TrackedEndpointState::new(ElectricalPowerValues::default()),
        }
    }

    pub fn set_values(&self, values: ElectricalPowerValues) {
        self.values.set(values);
    }

    pub fn values(&self) -> ElectricalPowerValues {
        self.values.get()
    }

    pub fn readiness(&self) -> Arc<dyn SourceReadiness> {
        self.values.readiness()
    }

    pub fn active_power_mw(&self) -> Option<i64> {
        self.values().active_power_mw
    }

    pub fn rms_voltage_mv(&self) -> Option<i64> {
        self.values().rms_voltage_mv
    }

    pub fn frequency_mhz(&self) -> Option<i64> {
        self.values().frequency_mhz
    }

    pub fn power_factor_centipercent(&self) -> Option<i64> {
        self.values().power_factor_centipercent
    }
}

impl Default for ElectricalPowerState {
    fn default() -> Self {
        Self::new()
    }
}

impl Sensor for ElectricalPowerState {
    fn version(&self) -> u32 {
        self.values.version()
    }
}

impl NotifiableSensor for ElectricalPowerState {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.values.set_notifier(notifier);
    }
}

define_versioned_read_only_cluster_handler!(
    ElectricalPowerMeasurementHandler,
    ElectricalPowerState,
    ElectricalPowerMeasurementAttribute,
    CLUSTER,
    |sensor, tw, tag, attr| { write_power_attr(&mut tw, tag, sensor.values(), attr) }
);

fn write_power_attr(
    tw: &mut impl TLVWrite,
    tag: &TLVTag,
    values: ElectricalPowerValues,
    attr: ElectricalPowerMeasurementAttribute,
) -> Result<(), Error> {
    match attr {
        ElectricalPowerMeasurementAttribute::PowerMode => tw.u8(tag, POWER_MODE_AC)?,
        ElectricalPowerMeasurementAttribute::NumberOfMeasurementTypes => {
            tw.u8(tag, MEASUREMENT_COUNT)?;
        }
        ElectricalPowerMeasurementAttribute::Accuracy => {
            write_accuracy_array(tw, tag, values)?;
        }
        ElectricalPowerMeasurementAttribute::ActivePower => {
            write_nullable_i64(tw, tag, values.active_power_mw)?;
        }
        ElectricalPowerMeasurementAttribute::ReactivePower => {
            write_nullable_i64(tw, tag, values.reactive_power_mvar)?;
        }
        ElectricalPowerMeasurementAttribute::ApparentPower => {
            write_nullable_i64(tw, tag, values.apparent_power_mva)?;
        }
        ElectricalPowerMeasurementAttribute::RmsVoltage => {
            write_nullable_i64(tw, tag, values.rms_voltage_mv)?;
        }
        ElectricalPowerMeasurementAttribute::RmsCurrent => {
            write_nullable_i64(tw, tag, values.rms_current_ma)?;
        }
        ElectricalPowerMeasurementAttribute::Frequency => {
            write_nullable_i64(tw, tag, values.frequency_mhz)?;
        }
        ElectricalPowerMeasurementAttribute::PowerFactor => {
            write_nullable_i64(tw, tag, values.power_factor_centipercent)?;
        }
    }
    Ok(())
}

fn write_nullable_i64(
    tw: &mut impl TLVWrite,
    tag: &TLVTag,
    value: Option<i64>,
) -> Result<(), Error> {
    write_nullable(tw, tag, value, |tw, tag, value| tw.i64(tag, value))
}

fn write_accuracy_array(
    tw: &mut impl TLVWrite,
    tag: &TLVTag,
    values: ElectricalPowerValues,
) -> Result<(), Error> {
    tw.start_array(tag)?;
    write_accuracy(tw, MEASUREMENT_ACTIVE_POWER, values.active_power_mw)?;
    write_accuracy(tw, MEASUREMENT_REACTIVE_POWER, values.reactive_power_mvar)?;
    write_accuracy(tw, MEASUREMENT_APPARENT_POWER, values.apparent_power_mva)?;
    write_accuracy(tw, MEASUREMENT_RMS_VOLTAGE, values.rms_voltage_mv)?;
    write_accuracy(tw, MEASUREMENT_RMS_CURRENT, values.rms_current_ma)?;
    write_accuracy(tw, MEASUREMENT_FREQUENCY, values.frequency_mhz)?;
    write_accuracy(
        tw,
        MEASUREMENT_POWER_FACTOR,
        values.power_factor_centipercent,
    )?;
    tw.end_container()?;
    Ok(())
}

fn write_accuracy(
    tw: &mut impl TLVWrite,
    measurement_type: u16,
    value: Option<i64>,
) -> Result<(), Error> {
    let value = value.unwrap_or(0);
    tw.start_struct(&TLVTag::Anonymous)?;
    tw.u16(&TLVTag::Context(0), measurement_type)?;
    tw.bool(&TLVTag::Context(1), true)?;
    tw.i64(&TLVTag::Context(2), value)?;
    tw.i64(&TLVTag::Context(3), value)?;
    tw.start_array(&TLVTag::Context(4))?;
    tw.end_container()?;
    tw.end_container()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_values_marks_first_update_even_when_default() {
        let state = ElectricalPowerState::new();
        state.set_values(ElectricalPowerValues::default());

        assert_eq!(state.version(), 1);
    }
}
