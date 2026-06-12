//! ElectricalEnergyMeasurement cluster handler.
//!
//! Exposes Shelly per-channel cumulative energy telemetry using Matter cluster
//! 0x0091.

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

pub const CLUSTER_ID: u32 = 0x0091;
pub const CLUSTER_REVISION: u16 = 2;
const FEATURE_MAP: u32 = 0x07; // ImportedEnergy | ExportedEnergy | CumulativeEnergy

const MEASUREMENT_ELECTRICAL_ENERGY: u16 = 14;

#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum ElectricalEnergyMeasurementAttribute {
    Accuracy = 0x0000,
    CumulativeEnergyImported = 0x0001,
    CumulativeEnergyExported = 0x0002,
}

attribute_enum!(ElectricalEnergyMeasurementAttribute);

pub const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    feature_map: FEATURE_MAP,
    attributes: attributes!(
        Attribute::new(
            ElectricalEnergyMeasurementAttribute::Accuracy as _,
            Access::RV,
            Quality::NONE
        ),
        Attribute::new(
            ElectricalEnergyMeasurementAttribute::CumulativeEnergyImported as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            ElectricalEnergyMeasurementAttribute::CumulativeEnergyExported as _,
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
pub struct ElectricalEnergyValues {
    pub imported_energy_mwh: Option<i64>,
    pub exported_energy_mwh: Option<i64>,
}

pub struct ElectricalEnergyState {
    values: TrackedEndpointState<ElectricalEnergyValues>,
}

impl ElectricalEnergyState {
    pub fn new() -> Self {
        Self {
            values: TrackedEndpointState::new(ElectricalEnergyValues::default()),
        }
    }

    pub fn set_values(&self, values: ElectricalEnergyValues) {
        self.values.set(values);
    }

    pub fn values(&self) -> ElectricalEnergyValues {
        self.values.get()
    }

    pub fn readiness(&self) -> Arc<dyn SourceReadiness> {
        self.values.readiness()
    }

    pub fn imported_energy_mwh(&self) -> Option<i64> {
        self.values().imported_energy_mwh
    }
}

impl Default for ElectricalEnergyState {
    fn default() -> Self {
        Self::new()
    }
}

impl Sensor for ElectricalEnergyState {
    fn version(&self) -> u32 {
        self.values.version()
    }
}

impl NotifiableSensor for ElectricalEnergyState {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.values.set_notifier(notifier);
    }
}

define_versioned_read_only_cluster_handler!(
    ElectricalEnergyMeasurementHandler,
    ElectricalEnergyState,
    ElectricalEnergyMeasurementAttribute,
    CLUSTER,
    |sensor, tw, tag, attr| { write_energy_attr(&mut tw, tag, sensor.values(), attr) }
);

fn write_energy_attr(
    tw: &mut impl TLVWrite,
    tag: &TLVTag,
    values: ElectricalEnergyValues,
    attr: ElectricalEnergyMeasurementAttribute,
) -> Result<(), Error> {
    match attr {
        ElectricalEnergyMeasurementAttribute::Accuracy => {
            write_accuracy(tw, tag, values)?;
        }
        ElectricalEnergyMeasurementAttribute::CumulativeEnergyImported => {
            write_energy_measurement(tw, tag, values.imported_energy_mwh)?;
        }
        ElectricalEnergyMeasurementAttribute::CumulativeEnergyExported => {
            write_energy_measurement(tw, tag, values.exported_energy_mwh)?;
        }
    }
    Ok(())
}

fn write_accuracy(
    tw: &mut impl TLVWrite,
    tag: &TLVTag,
    values: ElectricalEnergyValues,
) -> Result<(), Error> {
    let value = values
        .imported_energy_mwh
        .or(values.exported_energy_mwh)
        .unwrap_or(0);
    tw.start_struct(tag)?;
    tw.u16(&TLVTag::Context(0), MEASUREMENT_ELECTRICAL_ENERGY)?;
    tw.bool(&TLVTag::Context(1), true)?;
    tw.i64(&TLVTag::Context(2), value)?;
    tw.i64(&TLVTag::Context(3), value)?;
    tw.start_array(&TLVTag::Context(4))?;
    tw.end_container()?;
    tw.end_container()?;
    Ok(())
}

fn write_energy_measurement(
    tw: &mut impl TLVWrite,
    tag: &TLVTag,
    value: Option<i64>,
) -> Result<(), Error> {
    write_nullable(tw, tag, value, |tw, tag, value| {
        tw.start_struct(tag)?;
        tw.i64(&TLVTag::Context(0), value)?;
        tw.end_container()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_values_marks_first_update_and_tracks_energy_values() {
        let state = ElectricalEnergyState::new();

        state.set_values(ElectricalEnergyValues {
            imported_energy_mwh: Some(120_000),
            exported_energy_mwh: Some(0),
        });

        assert_eq!(state.version(), 1);
        assert_eq!(state.imported_energy_mwh(), Some(120_000));
        assert_eq!(
            state.values(),
            ElectricalEnergyValues {
                imported_energy_mwh: Some(120_000),
                exported_energy_mwh: Some(0),
            }
        );
    }

    #[test]
    fn repeated_equivalent_values_do_not_bump_version_after_ready() {
        let state = ElectricalEnergyState::new();
        let values = ElectricalEnergyValues {
            imported_energy_mwh: Some(1),
            exported_energy_mwh: Some(2),
        };

        state.set_values(values);
        state.set_values(values);

        assert_eq!(state.version(), 1);
    }
}
