//! OccupancySensing cluster handler for motion/presence sensors.
//!
//! The OccupancySensing cluster (0x0406) represents an occupancy sensor.
//! Reads state from a shared BooleanSensor instance that can be updated
//! from external sources (HTTP, simulation, etc.).
//!
//! Uses version tracking to detect changes and notify subscribers automatically.

use super::super::endpoints::sensors::OccupancySensor;
use super::read_only_cluster::define_versioned_read_only_cluster_handler;
use rs_matter::dm::{Access, Attribute, Cluster};
use rs_matter::tlv::TLVWrite;
use rs_matter::{attribute_enum, attributes, with};
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
    events: &[],
    with_attrs: with!(all),
    with_cmds: with!(all),
    with_events: with!(all),
};

define_versioned_read_only_cluster_handler!(
    OccupancySensingHandler,
    OccupancySensor,
    OccupancySensingAttribute,
    CLUSTER,
    |sensor, tw, tag, attr| {
        match attr {
            OccupancySensingAttribute::Occupancy => {
                let occupancy_bitmap: u8 = if sensor.get() { 0x01 } else { 0x00 };
                tw.u8(tag, occupancy_bitmap)?;
            }
            OccupancySensingAttribute::OccupancySensorType => {
                tw.u8(tag, OccupancySensorType::PhysicalContact as u8)?;
            }
            OccupancySensingAttribute::OccupancySensorTypeBitmap => {
                tw.u8(tag, 0x08)?;
            }
        }
        Ok::<(), rs_matter::error::Error>(())
    }
);
