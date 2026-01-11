//! RelativeHumidityMeasurement cluster handler.
//!
//! The RelativeHumidityMeasurement cluster (0x0405) represents a humidity sensor.
//! Humidity is reported in centi-percent (value * 100).
//!
//! For example: 55.5% is reported as 5550.

use rs_matter::dm::{
    Access, Attribute, Cluster, Dataver, Handler, NonBlockingHandler, Quality, ReadContext,
    ReadReply, Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::TLVWrite;
use rs_matter::{attribute_enum, attributes, with};
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use strum::FromRepr;

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
    ),
    commands: &[],
    with_attrs: with!(all),
    with_cmds: with!(all),
};

/// Humidity sensor that can be updated from external sources.
pub struct HumiditySensor {
    /// Humidity in centi-percent (% * 100)
    value: AtomicU16,
    /// Version counter for change detection
    version: AtomicU32,
}

impl HumiditySensor {
    /// Create a new humidity sensor with initial value.
    ///
    /// # Arguments
    /// * `initial_percent` - Initial humidity in percent (0-100)
    pub fn new(initial_percent: f32) -> Self {
        Self {
            value: AtomicU16::new((initial_percent * 100.0) as u16),
            version: AtomicU32::new(0),
        }
    }

    /// Get the current humidity in percent.
    pub fn get_percent(&self) -> f32 {
        self.value.load(Ordering::SeqCst) as f32 / 100.0
    }

    /// Get the current humidity in centi-percent (raw Matter value).
    pub fn get_centipercent(&self) -> u16 {
        self.value.load(Ordering::SeqCst)
    }

    /// Set the humidity in percent.
    pub fn set_percent(&self, percent: f32) {
        let centipercent = (percent * 100.0) as u16;
        self.value.store(centipercent, Ordering::SeqCst);
        self.version.fetch_add(1, Ordering::SeqCst);
    }

    /// Get the current version (incremented on each change).
    pub fn version(&self) -> u32 {
        self.version.load(Ordering::SeqCst)
    }
}

/// Handler that serves a RelativeHumidityMeasurement cluster.
pub struct RelativeHumidityHandler {
    dataver: Dataver,
    sensor: Arc<HumiditySensor>,
    last_sensor_version: AtomicU32,
    /// Minimum humidity in centi-percent (0% = 0)
    min_value: u16,
    /// Maximum humidity in centi-percent (100% = 10000)
    max_value: u16,
}

impl RelativeHumidityHandler {
    /// Cluster definition for use in the data model
    pub const CLUSTER: Cluster<'static> = CLUSTER;

    /// Create a new handler with a sensor reference.
    ///
    /// Default range: 0% to 100%
    pub fn new(dataver: Dataver, sensor: Arc<HumiditySensor>) -> Self {
        Self {
            dataver,
            sensor,
            last_sensor_version: AtomicU32::new(0),
            min_value: 0,     // 0%
            max_value: 10000, // 100%
        }
    }

    /// Sync dataver with sensor version for subscription updates.
    fn sync_dataver(&self) {
        let sensor_version = self.sensor.version();
        let last = self.last_sensor_version.load(Ordering::SeqCst);
        if sensor_version != last {
            self.last_sensor_version
                .store(sensor_version, Ordering::SeqCst);
            self.dataver.changed();
        }
    }

    fn read_impl(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.sync_dataver();

        let attr = ctx.attr();

        let Some(mut writer) = reply.with_dataver(self.dataver.get())? else {
            return Ok(());
        };

        // Global attributes
        if attr.is_system() {
            return CLUSTER.read(attr, writer);
        }

        let tag = writer.tag();
        {
            let mut tw = writer.writer();

            match attr.attr_id.try_into()? {
                RelativeHumidityAttribute::MeasuredValue => {
                    tw.u16(tag, self.sensor.get_centipercent())?;
                }
                RelativeHumidityAttribute::MinMeasuredValue => {
                    tw.u16(tag, self.min_value)?;
                }
                RelativeHumidityAttribute::MaxMeasuredValue => {
                    tw.u16(tag, self.max_value)?;
                }
                RelativeHumidityAttribute::Tolerance => {
                    // Not implemented - return error
                    return Err(ErrorCode::AttributeNotFound.into());
                }
            }
        }

        writer.complete()
    }

    fn write_impl(&self, _ctx: impl WriteContext) -> Result<(), Error> {
        // Cluster is read-only
        Err(ErrorCode::UnsupportedAccess.into())
    }
}

impl Handler for RelativeHumidityHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.read_impl(ctx, reply)
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        self.write_impl(ctx)
    }
}

impl NonBlockingHandler for RelativeHumidityHandler {}
