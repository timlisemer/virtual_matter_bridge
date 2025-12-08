//! OccupancySensing cluster handler for motion/presence sensors.
//!
//! The OccupancySensing cluster (0x0406) represents an occupancy sensor.
//! Reads state from a shared BooleanSensor instance that can be updated
//! from external sources (HTTP, simulation, etc.).
//!
//! Uses version tracking to detect changes and notify subscribers automatically.

use super::super::endpoints::sensors::OccupancySensor;
use super::sync_dataver_with_sensor;
use rs_matter::dm::{
    Access, Attribute, Cluster, Dataver, Handler, NonBlockingHandler, ReadContext, ReadReply,
    Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::TLVWrite;
use rs_matter::{attribute_enum, attributes, with};
use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use strum::FromRepr;

/// Matter Cluster ID for OccupancySensing
pub const CLUSTER_ID: u32 = 0x0406;

/// Cluster revision
pub const CLUSTER_REVISION: u16 = 1;

/// Occupancy sensor type enum values
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum OccupancySensorType {
    Pir = 0x00,
    Ultrasonic = 0x01,
    PirAndUltrasonic = 0x02,
    PhysicalContact = 0x03,
}

/// Attribute IDs for the OccupancySensing cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum OccupancySensingAttribute {
    /// Bitmap8 where bit 0 indicates sensed occupancy
    Occupancy = 0x0000,
    /// The type of sensor (PIR, Ultrasonic, PhysicalContact, etc.)
    OccupancySensorType = 0x0001,
    /// Bitmap of supported sensor types
    OccupancySensorTypeBitmap = 0x0002,
}

attribute_enum!(OccupancySensingAttribute);

/// Cluster metadata definition
pub const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    feature_map: 0,
    attributes: attributes!(
        Attribute::new(
            OccupancySensingAttribute::Occupancy as _,
            Access::RV,
            rs_matter::dm::Quality::NONE
        ),
        Attribute::new(
            OccupancySensingAttribute::OccupancySensorType as _,
            Access::RV,
            rs_matter::dm::Quality::FIXED
        ),
        Attribute::new(
            OccupancySensingAttribute::OccupancySensorTypeBitmap as _,
            Access::RV,
            rs_matter::dm::Quality::FIXED
        ),
    ),
    commands: &[],
    with_attrs: with!(all),
    with_cmds: with!(all),
};

/// Handler that serves a read-only OccupancySensing cluster.
///
/// Reads state from a shared `BooleanSensor` that can be updated from
/// external sources (HTTP endpoints, simulation, etc.).
///
/// Automatically detects sensor value changes and notifies subscribers
/// by tracking the sensor's version number.
pub struct OccupancySensingHandler {
    dataver: Dataver,
    sensor: Arc<OccupancySensor>,
    last_sensor_version: AtomicU32,
}

impl OccupancySensingHandler {
    /// Cluster definition for use in the data model
    pub const CLUSTER: Cluster<'static> = CLUSTER;

    /// Create a new handler with a sensor reference.
    pub fn new(dataver: Dataver, sensor: Arc<OccupancySensor>) -> Self {
        Self {
            dataver,
            sensor,
            last_sensor_version: AtomicU32::new(0),
        }
    }

    fn read_impl(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        // Check if sensor changed and bump dataver to notify subscribers
        sync_dataver_with_sensor(&*self.sensor, &self.last_sensor_version, &self.dataver);

        let attr = ctx.attr();

        let Some(mut writer) = reply.with_dataver(self.dataver.get())? else {
            return Ok(()); // No update needed
        };

        // Global attributes
        if attr.is_system() {
            return CLUSTER.read(attr, writer);
        }

        let tag = writer.tag();
        {
            let mut tw = writer.writer();

            match attr.attr_id.try_into()? {
                OccupancySensingAttribute::Occupancy => {
                    // Bitmap8: bit 0 = sensed occupancy (1 = occupied, 0 = unoccupied)
                    let occupancy_bitmap: u8 = if self.sensor.get() { 0x01 } else { 0x00 };
                    tw.u8(tag, occupancy_bitmap)?;
                }
                OccupancySensingAttribute::OccupancySensorType => {
                    // PhysicalContact sensor type (virtual sensor)
                    tw.u8(tag, OccupancySensorType::PhysicalContact as u8)?;
                }
                OccupancySensingAttribute::OccupancySensorTypeBitmap => {
                    // Bitmap indicating PhysicalContact is supported (bit 3)
                    tw.u8(tag, 0x08)?; // bit 3 = PhysicalContact
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

impl Handler for OccupancySensingHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.read_impl(ctx, reply)
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        self.write_impl(ctx)
    }
}

impl NonBlockingHandler for OccupancySensingHandler {}
