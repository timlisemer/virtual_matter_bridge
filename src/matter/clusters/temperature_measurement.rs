//! TemperatureMeasurement cluster handler.
//!
//! The TemperatureMeasurement cluster (0x0402) represents a temperature sensor.
//! Temperature is reported in centidegrees Celsius (value * 100).
//!
//! For example: 21.5°C is reported as 2150.

use rs_matter::dm::{
    Access, Attribute, Cluster, Dataver, Handler, NonBlockingHandler, Quality, ReadContext,
    ReadReply, Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::TLVWrite;
use rs_matter::{attribute_enum, attributes, with};
use std::sync::Arc;
use std::sync::atomic::{AtomicI16, AtomicU32, Ordering};
use strum::FromRepr;

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
    with_attrs: with!(all),
    with_cmds: with!(all),
};

/// Temperature sensor that can be updated from external sources.
pub struct TemperatureSensor {
    /// Temperature in centidegrees Celsius (°C * 100)
    value: AtomicI16,
    /// Version counter for change detection
    version: AtomicU32,
}

impl TemperatureSensor {
    /// Create a new temperature sensor with initial value.
    ///
    /// # Arguments
    /// * `initial_celsius` - Initial temperature in degrees Celsius
    pub fn new(initial_celsius: f32) -> Self {
        Self {
            value: AtomicI16::new((initial_celsius * 100.0) as i16),
            version: AtomicU32::new(0),
        }
    }

    /// Get the current temperature in degrees Celsius.
    pub fn get_celsius(&self) -> f32 {
        self.value.load(Ordering::SeqCst) as f32 / 100.0
    }

    /// Get the current temperature in centidegrees (raw Matter value).
    pub fn get_centidegrees(&self) -> i16 {
        self.value.load(Ordering::SeqCst)
    }

    /// Set the temperature in degrees Celsius.
    pub fn set_celsius(&self, celsius: f32) {
        let centidegrees = (celsius * 100.0) as i16;
        self.value.store(centidegrees, Ordering::SeqCst);
        self.version.fetch_add(1, Ordering::SeqCst);
    }

    /// Get the current version (incremented on each change).
    pub fn version(&self) -> u32 {
        self.version.load(Ordering::SeqCst)
    }
}

/// Handler that serves a TemperatureMeasurement cluster.
pub struct TemperatureMeasurementHandler {
    dataver: Dataver,
    sensor: Arc<TemperatureSensor>,
    last_sensor_version: AtomicU32,
    /// Minimum temperature in centidegrees (-40°C = -4000)
    min_value: i16,
    /// Maximum temperature in centidegrees (125°C = 12500)
    max_value: i16,
}

impl TemperatureMeasurementHandler {
    /// Cluster definition for use in the data model
    pub const CLUSTER: Cluster<'static> = CLUSTER;

    /// Create a new handler with a sensor reference.
    ///
    /// Default range: -40°C to 125°C (typical sensor range)
    pub fn new(dataver: Dataver, sensor: Arc<TemperatureSensor>) -> Self {
        Self {
            dataver,
            sensor,
            last_sensor_version: AtomicU32::new(0),
            min_value: -4000, // -40°C
            max_value: 12500, // 125°C
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
                TemperatureMeasurementAttribute::MeasuredValue => {
                    tw.i16(tag, self.sensor.get_centidegrees())?;
                }
                TemperatureMeasurementAttribute::MinMeasuredValue => {
                    tw.i16(tag, self.min_value)?;
                }
                TemperatureMeasurementAttribute::MaxMeasuredValue => {
                    tw.i16(tag, self.max_value)?;
                }
                TemperatureMeasurementAttribute::Tolerance => {
                    // Tolerance in 0.01°C units (0 = not specified)
                    tw.u16(tag, 0)?;
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

impl Handler for TemperatureMeasurementHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.read_impl(ctx, reply)
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        self.write_impl(ctx)
    }
}

impl NonBlockingHandler for TemperatureMeasurementHandler {}
