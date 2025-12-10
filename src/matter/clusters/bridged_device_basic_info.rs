//! BridgedDeviceBasicInformation Cluster (0x0039) handler.
//!
//! Provides endpoint names via the NodeLabel attribute for Matter bridges.
//! Controllers like Home Assistant read NodeLabel to display bridged device names.

use rs_matter::dm::{
    Access, Attribute, Cluster, Dataver, Handler, NonBlockingHandler, ReadContext, ReadReply,
    Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::TLVWrite;
use rs_matter::{attribute_enum, attributes, with};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use strum::FromRepr;

/// Matter Cluster ID for BridgedDeviceBasicInformation
pub const CLUSTER_ID: u32 = 0x0039;

/// Cluster revision
const CLUSTER_REVISION: u16 = 4;

/// Attribute IDs for the BridgedDeviceBasicInformation cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum BridgedDeviceBasicInfoAttribute {
    /// Node label - user-friendly name for the endpoint
    NodeLabel = 0x0005,
    /// Reachable - whether the bridged device is reachable
    Reachable = 0x0011,
}

attribute_enum!(BridgedDeviceBasicInfoAttribute);

/// Cluster metadata definition
const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    feature_map: 0,
    attributes: attributes!(
        // NodeLabel: optional, read-only string
        Attribute::new(
            BridgedDeviceBasicInfoAttribute::NodeLabel as _,
            Access::RV,
            rs_matter::dm::Quality::NONE
        ),
        // Reachable: mandatory, read-only bool
        Attribute::new(
            BridgedDeviceBasicInfoAttribute::Reachable as _,
            Access::RV,
            rs_matter::dm::Quality::NONE
        ),
    ),
    commands: &[],
    with_attrs: with!(all),
    with_cmds: with!(all),
};

/// Handler for BridgedDeviceBasicInformation cluster.
///
/// Provides endpoint names via the NodeLabel attribute. Controllers like Home Assistant
/// read this to name bridged device endpoints.
#[derive(Clone, Debug)]
pub struct BridgedHandler {
    dataver: Dataver,
    /// The display name for this endpoint
    name: &'static str,
    /// Dynamic reachable state (shared with parent DeviceSwitch)
    reachable: Arc<AtomicBool>,
}

impl BridgedHandler {
    /// Cluster definition for use in the data model
    pub const CLUSTER: Cluster<'static> = CLUSTER;

    /// Create a new handler with the given endpoint name and reachable state.
    pub fn new(dataver: Dataver, name: &'static str, reachable: Arc<AtomicBool>) -> Self {
        Self {
            dataver,
            name,
            reachable,
        }
    }

    /// Create a new handler that is always reachable (for parent endpoints).
    pub fn new_always_reachable(dataver: Dataver, name: &'static str) -> Self {
        Self {
            dataver,
            name,
            reachable: Arc::new(AtomicBool::new(true)),
        }
    }

    fn read_impl(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        let attr = ctx.attr();

        let Some(mut writer) = reply.with_dataver(self.dataver.get())? else {
            return Ok(()); // No update needed
        };

        // Global/system attributes (ClusterRevision, FeatureMap, AttributeList, etc.)
        if attr.is_system() {
            return CLUSTER.read(attr, writer);
        }

        let tag = writer.tag();
        {
            let mut tw = writer.writer();

            match attr.attr_id.try_into()? {
                BridgedDeviceBasicInfoAttribute::NodeLabel => {
                    tw.utf8(tag, self.name)?;
                }
                BridgedDeviceBasicInfoAttribute::Reachable => {
                    tw.bool(tag, self.reachable.load(Ordering::SeqCst))?;
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

impl Handler for BridgedHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.read_impl(ctx, reply)
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        self.write_impl(ctx)
    }
}

impl NonBlockingHandler for BridgedHandler {}
