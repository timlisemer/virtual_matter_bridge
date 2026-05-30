//! ElectricalEnergyMeasurement cluster handler.
//!
//! Exposes Shelly per-channel cumulative energy telemetry using Matter cluster
//! 0x0091.

use super::sync_dataver_with_sensor;
use crate::matter::endpoints::endpoints_helpers::{
    EndpointChangeTracker, Sensor, SourceReadiness, SourceSnapshot,
};
use crate::matter::endpoints::{ClusterNotifier, NotifiableSensor};
use parking_lot::RwLock;
use rs_matter::dm::{
    Access, Attribute, Cluster, Dataver, Handler, MatchContext, NonBlockingHandler, Quality,
    ReadContext, ReadReply, Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::{TLVTag, TLVWrite};
use rs_matter::{attribute_enum, attributes, with};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
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
    values: RwLock<ElectricalEnergyValues>,
    changes: EndpointChangeTracker,
    readiness: Arc<SourceSnapshot<()>>,
}

impl ElectricalEnergyState {
    pub fn new() -> Self {
        Self {
            values: RwLock::new(ElectricalEnergyValues::default()),
            changes: EndpointChangeTracker::new(),
            readiness: Arc::new(SourceSnapshot::new()),
        }
    }

    pub fn set_values(&self, values: ElectricalEnergyValues) {
        let was_ready = self.readiness.is_ready();
        let mut guard = self.values.write();
        if *guard != values || !was_ready {
            *guard = values;
            self.changes.mark_changed();
        }
        drop(guard);
        self.readiness.mark_ready();
    }

    pub fn values(&self) -> ElectricalEnergyValues {
        *self.values.read()
    }

    pub fn readiness(&self) -> Arc<dyn SourceReadiness> {
        self.readiness.clone()
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
        self.changes.version()
    }
}

impl NotifiableSensor for ElectricalEnergyState {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.changes.set_notifier(notifier);
    }
}

pub struct ElectricalEnergyMeasurementHandler {
    dataver: Dataver,
    state: Arc<ElectricalEnergyState>,
    last_state_version: AtomicU32,
}

impl ElectricalEnergyMeasurementHandler {
    pub const CLUSTER: Cluster<'static> = CLUSTER;

    pub fn new(dataver: Dataver, state: Arc<ElectricalEnergyState>) -> Self {
        Self {
            dataver,
            state,
            last_state_version: AtomicU32::new(0),
        }
    }

    fn read_impl(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        sync_dataver_with_sensor(&*self.state, &self.last_state_version, &self.dataver);

        let attr = ctx.attr();
        let Some(mut writer) = reply.with_dataver(self.dataver.get())? else {
            return Ok(());
        };

        if attr.is_system() {
            return CLUSTER.read(attr, writer);
        }

        let tag = writer.tag();
        let values = self.state.values();
        {
            let mut tw = writer.writer();
            match attr.attr_id.try_into()? {
                ElectricalEnergyMeasurementAttribute::Accuracy => {
                    Self::write_accuracy(&mut tw, tag, values)?;
                }
                ElectricalEnergyMeasurementAttribute::CumulativeEnergyImported => {
                    Self::write_energy_measurement(&mut tw, tag, values.imported_energy_mwh)?;
                }
                ElectricalEnergyMeasurementAttribute::CumulativeEnergyExported => {
                    Self::write_energy_measurement(&mut tw, tag, values.exported_energy_mwh)?;
                }
            }
        }

        writer.complete()
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
        if let Some(value) = value {
            tw.start_struct(tag)?;
            tw.i64(&TLVTag::Context(0), value)?;
            tw.end_container()?;
        } else {
            tw.null(tag)?;
        }
        Ok(())
    }

    fn write_impl(&self, _ctx: impl WriteContext) -> Result<(), Error> {
        Err(ErrorCode::UnsupportedAccess.into())
    }
}

impl Handler for ElectricalEnergyMeasurementHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.read_impl(ctx, reply)
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        self.write_impl(ctx)
    }

    fn bump_dataver(&self, _ctx: impl MatchContext) {
        self.dataver.changed();
        self.last_state_version
            .store(self.state.version(), Ordering::SeqCst);
    }
}

impl NonBlockingHandler for ElectricalEnergyMeasurementHandler {}

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
