//! BooleanState cluster handler for binary sensors.
//!
//! The BooleanState cluster (0x0045) represents a simple binary sensor.
//! Reads state from a shared BooleanSensor instance that can be updated
//! from external sources (HTTP, simulation, etc.).
//!
//! Uses version tracking to detect changes and notify subscribers automatically.

use super::super::endpoints::sensors::ContactSensor;
use super::read_only_cluster::define_versioned_read_only_cluster_handler;
use rs_matter::dm::{Access, Attribute, Cluster};
use rs_matter::tlv::TLVWrite;
use rs_matter::{attribute_enum, attributes, with};
use strum::FromRepr;

/// Matter Cluster ID for BooleanState
pub const CLUSTER_ID: u32 = 0x0045;

/// Cluster revision
pub const CLUSTER_REVISION: u16 = 1;

/// Attribute IDs for the BooleanState cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum BooleanStateAttribute {
    /// The current state value (true/false)
    StateValue = 0x00,
}

attribute_enum!(BooleanStateAttribute);

/// Cluster metadata definition
pub const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    feature_map: 0,
    attributes: attributes!(Attribute::new(
        BooleanStateAttribute::StateValue as _,
        Access::RV,
        rs_matter::dm::Quality::NONE
    ),),
    commands: &[],
    events: &[],
    with_attrs: with!(all),
    with_cmds: with!(all),
    with_events: with!(all),
};

define_versioned_read_only_cluster_handler!(
    BooleanStateHandler,
    ContactSensor,
    BooleanStateAttribute,
    CLUSTER,
    |sensor, tw, tag, attr| {
        match attr {
            BooleanStateAttribute::StateValue => {
                tw.bool(tag, sensor.get())?;
            }
        }
        Ok::<(), rs_matter::error::Error>(())
    }
);
