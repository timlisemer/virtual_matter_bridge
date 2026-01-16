//! Minimal Time Synchronization cluster handler for rs-matter.
//!
//! Some controllers (e.g., Home Assistant) probe the Time Synchronization
//! cluster on the root endpoint. Previously we returned `UnsupportedCluster`
//! which surfaced as errors in the logs. This handler answers the basic reads
//! with sensible defaults so the controller can proceed without errors.

use std::time::{SystemTime, UNIX_EPOCH};

use rs_matter::dm::{
    Access, Attribute, Cluster, Dataver, Handler, NonBlockingHandler, ReadContext, ReadReply,
    Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::TLVWrite;
use rs_matter::{attribute_enum, attributes, with};
use strum::FromRepr;

/// Matter Cluster ID for Time Synchronization (see spec)
pub const CLUSTER_ID: u32 = 0x0046;

/// Cluster revision
pub const CLUSTER_REVISION: u16 = 1;

/// Attribute IDs for the Time Synchronization cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum TimeSyncAttribute {
    /// UTC time in microseconds since Unix epoch
    UtcTime = 0x00,
    /// Current time granularity (enum)
    Granularity = 0x01,
    /// Current time source (enum)
    TimeSource = 0x02,
    /// DST offset table (list)
    DstOffset = 0x06,
    /// Local time in microseconds since Unix epoch
    LocalTime = 0x07,
}

attribute_enum!(TimeSyncAttribute);

/// Cluster metadata definition
pub const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    feature_map: 0,
    attributes: attributes!(
        Attribute::new(
            TimeSyncAttribute::UtcTime as _,
            Access::RV,
            rs_matter::dm::Quality::NONE
        ),
        Attribute::new(
            TimeSyncAttribute::Granularity as _,
            Access::RV,
            rs_matter::dm::Quality::NONE
        ),
        Attribute::new(
            TimeSyncAttribute::TimeSource as _,
            Access::RV,
            rs_matter::dm::Quality::NONE
        ),
        Attribute::new(
            TimeSyncAttribute::DstOffset as _,
            Access::RV,
            rs_matter::dm::Quality::A // DstOffset is actually a list
        ),
        Attribute::new(
            TimeSyncAttribute::LocalTime as _,
            Access::RV,
            rs_matter::dm::Quality::NONE
        ),
    ),
    commands: &[],
    with_attrs: with!(all),
    with_cmds: with!(all),
};

/// Handler that serves a minimal, read-only Time Synchronization cluster.
pub struct TimeSyncHandler {
    dataver: Dataver,
}

impl TimeSyncHandler {
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
                TimeSyncAttribute::UtcTime => {
                    tw.i64(tag, Self::epoch_micros()?)?;
                }
                TimeSyncAttribute::Granularity => {
                    // 2 = Seconds granularity (conservative, but avoids overstating accuracy)
                    tw.u8(tag, 2)?;
                }
                TimeSyncAttribute::TimeSource => {
                    // 0 = Unknown/None (per spec enum), since we don't track a specific source yet
                    tw.u8(tag, 0)?;
                }
                TimeSyncAttribute::DstOffset => {
                    // No DST offsets configured - empty list
                    // For list reads with list_index, return ConstraintError (empty list)
                    if attr.list_index.as_ref().is_some_and(|li| li.is_some()) {
                        return Err(ErrorCode::ConstraintError.into());
                    }
                    // Otherwise write empty array
                    tw.start_array(tag)?;
                    tw.end_container()?;
                }
                TimeSyncAttribute::LocalTime => {
                    tw.i64(tag, Self::epoch_micros()?)?;
                }
            }
        }

        writer.complete()
    }

    fn write_impl(&self, _ctx: impl WriteContext) -> Result<(), Error> {
        // Cluster is read-only in this implementation
        Err(ErrorCode::UnsupportedAccess.into())
    }

    fn epoch_micros() -> Result<i64, Error> {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::new(ErrorCode::Failure))?;
        Ok(duration.as_micros().try_into().unwrap_or(i64::MAX))
    }
}

impl Handler for TimeSyncHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.read_impl(ctx, reply)
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        self.write_impl(ctx)
    }
}

impl NonBlockingHandler for TimeSyncHandler {}
