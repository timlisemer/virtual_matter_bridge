//! Software Diagnostics cluster handler for rs-matter integration.
//!
//! This module implements the Matter Software Diagnostics cluster (0x0046)
//! to provide software version and diagnostic information to controllers.
//!
//! All attributes return stub values since actual heap/thread monitoring
//! is not required for basic Matter functionality.

use rs_matter::dm::{
    Access, Attribute, Cluster, Dataver, Handler, InvokeContext, InvokeReply, NonBlockingHandler,
    Quality, ReadContext, ReadReply, Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::TLVWrite;
use rs_matter::{attribute_enum, attributes, with};
use strum::FromRepr;

/// Software Diagnostics Cluster ID (Matter spec)
pub const CLUSTER_ID: u32 = 0x0046;

/// Cluster revision
pub const CLUSTER_REVISION: u16 = 1;

/// Attribute IDs for the Software Diagnostics cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum SoftwareDiagAttribute {
    /// List of thread metrics (optional)
    ThreadMetrics = 0x0000,
    /// Current free heap memory in bytes (optional)
    CurrentHeapFree = 0x0001,
    /// Current used heap memory in bytes (optional)
    CurrentHeapUsed = 0x0002,
    /// High watermark of heap usage in bytes (optional)
    CurrentHeapHighWatermark = 0x0003,
}

attribute_enum!(SoftwareDiagAttribute);

/// Build cluster definition
pub const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    feature_map: 0, // No optional features enabled
    attributes: attributes!(
        // ThreadMetrics - list of ThreadMetricsStruct, read-only, optional
        Attribute::new(
            SoftwareDiagAttribute::ThreadMetrics as _,
            Access::RV,
            Quality::NONE
        ),
        // CurrentHeapFree - uint64, read-only, optional
        Attribute::new(
            SoftwareDiagAttribute::CurrentHeapFree as _,
            Access::RV,
            Quality::NONE
        ),
        // CurrentHeapUsed - uint64, read-only, optional
        Attribute::new(
            SoftwareDiagAttribute::CurrentHeapUsed as _,
            Access::RV,
            Quality::NONE
        ),
        // CurrentHeapHighWatermark - uint64, read-only, optional
        Attribute::new(
            SoftwareDiagAttribute::CurrentHeapHighWatermark as _,
            Access::RV,
            Quality::NONE
        ),
    ),
    commands: &[], // No commands in this cluster
    with_attrs: with!(all),
    with_cmds: with!(),
};

/// Handler for the Software Diagnostics cluster
pub struct SoftwareDiagHandler {
    dataver: Dataver,
}

impl SoftwareDiagHandler {
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
            SoftwareDiagAttribute::ThreadMetrics => {
                // Return empty list - thread metrics not implemented
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_array(tag)?;
                    tw.end_container()?;
                }
                writer.complete()
            }
            SoftwareDiagAttribute::CurrentHeapFree => {
                // Return stub value
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.u64(tag, 0)?;
                }
                writer.complete()
            }
            SoftwareDiagAttribute::CurrentHeapUsed => {
                // Return stub value
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.u64(tag, 0)?;
                }
                writer.complete()
            }
            SoftwareDiagAttribute::CurrentHeapHighWatermark => {
                // Return stub value
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.u64(tag, 0)?;
                }
                writer.complete()
            }
        }
    }

    fn write_impl(&self, _ctx: impl WriteContext) -> Result<(), Error> {
        // All attributes are read-only
        Err(ErrorCode::UnsupportedAccess.into())
    }

    fn invoke_impl(&self, _ctx: impl InvokeContext, _reply: impl InvokeReply) -> Result<(), Error> {
        // No commands in this cluster
        Err(ErrorCode::InvalidCommand.into())
    }
}

impl Handler for SoftwareDiagHandler {
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

impl NonBlockingHandler for SoftwareDiagHandler {}
