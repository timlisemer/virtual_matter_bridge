//! ICD Management cluster handler for rs-matter integration.
//!
//! This module implements the Matter ICD Management cluster (0x0046)
//! to satisfy Home Assistant's queries. Since this bridge is always connected
//! (not battery-powered), we return values indicating an always-on device.

use rs_matter::dm::{
    Access, Attribute, Cluster, Dataver, Handler, InvokeContext, InvokeReply, NonBlockingHandler,
    Quality, ReadContext, ReadReply, Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::TLVWrite;
use rs_matter::{attribute_enum, attributes, with};
use strum::FromRepr;

/// ICD Management Cluster ID (Matter spec)
pub const CLUSTER_ID: u32 = 0x0046;

/// Cluster revision
pub const CLUSTER_REVISION: u16 = 3;

/// Attribute IDs for the ICD Management cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum IcdMgmtAttribute {
    /// Idle mode duration in seconds
    IdleModeDuration = 0x0000,
    /// Active mode duration in milliseconds
    ActiveModeDuration = 0x0001,
    /// Active mode threshold in milliseconds
    ActiveModeThreshold = 0x0002,
    /// Registered clients list (optional)
    RegisteredClients = 0x0003,
    /// ICD counter (optional)
    IcdCounter = 0x0004,
    /// Max clients per fabric (optional)
    ClientsSupportedPerFabric = 0x0005,
    /// User active mode trigger hint bitmap
    UserActiveModeTriggerHint = 0x0006,
    /// User active mode trigger instruction string
    UserActiveModeTriggerInstruction = 0x0007,
    /// Operating mode (optional)
    OperatingMode = 0x0008,
    /// Maximum check-in back-off (optional)
    MaximumCheckInBackOff = 0x0009,
}

attribute_enum!(IcdMgmtAttribute);

/// Build cluster definition
pub const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    feature_map: 0, // No optional features (we're not a real ICD)
    attributes: attributes!(
        // IdleModeDuration - int32u, read-only, required
        Attribute::new(
            IcdMgmtAttribute::IdleModeDuration as _,
            Access::RV,
            Quality::F // Fixed value
        ),
        // ActiveModeDuration - int32u, read-only, required
        Attribute::new(
            IcdMgmtAttribute::ActiveModeDuration as _,
            Access::RV,
            Quality::F
        ),
        // ActiveModeThreshold - int16u, read-only, required
        Attribute::new(
            IcdMgmtAttribute::ActiveModeThreshold as _,
            Access::RV,
            Quality::F
        ),
        // UserActiveModeTriggerHint - bitmap32, read-only, optional but HA queries it
        Attribute::new(
            IcdMgmtAttribute::UserActiveModeTriggerHint as _,
            Access::RV,
            Quality::F
        ),
        // UserActiveModeTriggerInstruction - string, read-only, optional but HA queries it
        Attribute::new(
            IcdMgmtAttribute::UserActiveModeTriggerInstruction as _,
            Access::RV,
            Quality::F
        ),
    ),
    commands: &[], // No commands - we're not a real ICD
    with_attrs: with!(all),
    with_cmds: with!(),
};

/// Handler for the ICD Management cluster
///
/// Returns values indicating an always-connected device:
/// - IdleModeDuration: 1 second (minimum, since we're always on)
/// - ActiveModeDuration: 10000ms (we're always active)
/// - ActiveModeThreshold: 5000ms
pub struct IcdMgmtHandler {
    dataver: Dataver,
}

impl IcdMgmtHandler {
    /// The cluster definition for this handler
    pub const CLUSTER: Cluster<'static> = CLUSTER;

    /// Create a new handler
    pub fn new(dataver: Dataver) -> Self {
        Self { dataver }
    }

    fn read_impl(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        let attr = ctx.attr();

        // Get the dataver-aware writer
        let Some(mut writer) = reply.with_dataver(self.dataver.get())? else {
            return Ok(()); // No update needed (dataver match)
        };

        // Handle global attributes via the cluster definition
        if attr.is_system() {
            return CLUSTER.read(attr, writer);
        }

        match attr.attr_id.try_into()? {
            IcdMgmtAttribute::IdleModeDuration => {
                // 1 second - we're always connected so this is minimal
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.u32(tag, 1)?;
                }
                writer.complete()
            }
            IcdMgmtAttribute::ActiveModeDuration => {
                // 10000ms (10 seconds) - we're always active
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.u32(tag, 10000)?;
                }
                writer.complete()
            }
            IcdMgmtAttribute::ActiveModeThreshold => {
                // 5000ms threshold
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.u16(tag, 5000)?;
                }
                writer.complete()
            }
            IcdMgmtAttribute::UserActiveModeTriggerHint => {
                // 0 = no user trigger hints (we're always on)
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.u32(tag, 0)?;
                }
                writer.complete()
            }
            IcdMgmtAttribute::UserActiveModeTriggerInstruction => {
                // Empty string - no instructions needed
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.utf8(tag, "")?;
                }
                writer.complete()
            }
            // These are optional and we don't advertise them, but handle gracefully
            IcdMgmtAttribute::RegisteredClients
            | IcdMgmtAttribute::IcdCounter
            | IcdMgmtAttribute::ClientsSupportedPerFabric
            | IcdMgmtAttribute::OperatingMode
            | IcdMgmtAttribute::MaximumCheckInBackOff => Err(ErrorCode::AttributeNotFound.into()),
        }
    }

    fn write_impl(&self, _ctx: impl WriteContext) -> Result<(), Error> {
        // All attributes are read-only
        Err(ErrorCode::UnsupportedAccess.into())
    }

    fn invoke_impl(&self, _ctx: impl InvokeContext, _reply: impl InvokeReply) -> Result<(), Error> {
        // No commands in this cluster for non-ICD devices
        Err(ErrorCode::InvalidCommand.into())
    }
}

impl Handler for IcdMgmtHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.read_impl(ctx, reply)
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        self.write_impl(ctx)
    }

    fn invoke(&self, ctx: impl InvokeContext, reply: impl InvokeReply) -> Result<(), Error> {
        self.invoke_impl(ctx, reply)
    }
}

impl NonBlockingHandler for IcdMgmtHandler {}
