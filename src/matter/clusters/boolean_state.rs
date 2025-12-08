//! BooleanState cluster handler for binary sensors.
//!
//! The BooleanState cluster (0x0110) represents a simple binary sensor.
//! This implementation provides a read-only "test sensor" that is always true.

use rs_matter::dm::{
    Access, Attribute, Cluster, Dataver, Handler, NonBlockingHandler, ReadContext, ReadReply,
    Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
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
    with_attrs: with!(all),
    with_cmds: with!(all),
};

/// Handler that serves a read-only BooleanState cluster (test sensor always true).
pub struct BooleanStateHandler {
    dataver: Dataver,
}

impl BooleanStateHandler {
    /// Cluster definition for use in the data model
    pub const CLUSTER: Cluster<'static> = CLUSTER;

    /// Create a new handler
    pub const fn new(dataver: Dataver) -> Self {
        Self { dataver }
    }

    fn read_impl(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
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
                BooleanStateAttribute::StateValue => {
                    tw.bool(tag, true)?; // Test sensor: always true
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

impl Handler for BooleanStateHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.read_impl(ctx, reply)
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        self.write_impl(ctx)
    }
}

impl NonBlockingHandler for BooleanStateHandler {}
