//! Chime cluster handler for rs-matter integration.
//!
//! This module implements the Matter Chime cluster (0x0556) by bridging
//! the existing ChimeCluster business logic to rs-matter's Handler trait.

use std::sync::Arc;

use parking_lot::RwLock;
use strum::FromRepr;

use rs_matter::dm::{
    Access, Attribute, Cluster, Command, Dataver, Handler, InvokeContext, InvokeReply,
    NonBlockingHandler, Quality, ReadContext, ReadReply, Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::{TLVTag, TLVWrite};
use rs_matter::{attribute_enum, attributes, command_enum, commands, with};

use crate::clusters::chime::ChimeCluster;

/// Chime Cluster ID (Matter spec)
pub const CLUSTER_ID: u32 = 0x0556;

/// Cluster revision
pub const CLUSTER_REVISION: u16 = 1;

/// Attribute IDs for the Chime cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum ChimeAttribute {
    InstalledChimeSounds = 0x0000,
    SelectedChime = 0x0001,
    Enabled = 0x0002,
}

attribute_enum!(ChimeAttribute);

/// Command IDs for the Chime cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum ChimeCommand {
    PlayChimeSound = 0x00,
}

command_enum!(ChimeCommand);

/// Full Chime cluster definition
pub const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    feature_map: 0, // No features
    attributes: attributes!(
        // InstalledChimeSounds - array of ChimeSoundStruct, read-only
        Attribute::new(
            ChimeAttribute::InstalledChimeSounds as _,
            Access::RV,
            Quality::A
        ),
        // SelectedChime - uint8, read-write with view/manage
        Attribute::new(
            ChimeAttribute::SelectedChime as _,
            Access::RWVM,
            Quality::NONE
        ),
        // Enabled - boolean, read-write with view/manage
        Attribute::new(ChimeAttribute::Enabled as _, Access::RWVM, Quality::NONE),
    ),
    commands: commands!(
        // PlayChimeSound command - no response (DefaultSuccess via Ok(()))
        Command::new(ChimeCommand::PlayChimeSound as _, None, Access::WO),
    ),
    with_attrs: with!(all),
    with_cmds: with!(all),
};

/// Handler that bridges the existing ChimeCluster to rs-matter
pub struct ChimeHandler {
    dataver: Dataver,
    cluster: Arc<RwLock<ChimeCluster>>,
}

impl ChimeHandler {
    /// The cluster definition for this handler
    pub const CLUSTER: Cluster<'static> = CLUSTER;

    /// Create a new ChimeHandler
    pub fn new(dataver: Dataver, cluster: Arc<RwLock<ChimeCluster>>) -> Self {
        Self { dataver, cluster }
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

        // Get cluster state
        let cluster = self.cluster.read();

        match attr.attr_id.try_into()? {
            ChimeAttribute::InstalledChimeSounds => {
                // Encode array of ChimeSoundStruct
                let sounds = cluster.get_installed_chime_sounds();

                // Get list index for array reads
                let list_index = attr.list_index.clone().map(|li| li.into_option());
                let tag = writer.tag();

                // Scope the TLV writer to ensure it's dropped before complete()
                {
                    let mut tw = writer.writer();

                    if list_index.is_none() {
                        // Read entire array
                        tw.start_array(tag)?;
                    }

                    if let Some(Some(index)) = list_index.as_ref() {
                        // Read single element
                        let sound = sounds
                            .get(*index as usize)
                            .ok_or(ErrorCode::ConstraintError)?;
                        tw.start_struct(&TLVTag::Anonymous)?;
                        tw.u8(&TLVTag::Context(0), sound.chime_id)?;
                        tw.utf8(&TLVTag::Context(1), &sound.name)?;
                        tw.end_container()?;
                    } else {
                        // Read all elements (or empty list_index)
                        for sound in sounds {
                            tw.start_struct(&TLVTag::Anonymous)?;
                            tw.u8(&TLVTag::Context(0), sound.chime_id)?;
                            tw.utf8(&TLVTag::Context(1), &sound.name)?;
                            tw.end_container()?;
                        }
                    }

                    if list_index.is_none() {
                        tw.end_container()?;
                    }
                }

                writer.complete()
            }
            ChimeAttribute::SelectedChime => writer.set(cluster.get_selected_chime()),
            ChimeAttribute::Enabled => writer.set(cluster.is_enabled()),
        }
    }

    fn write_impl(&self, ctx: impl WriteContext) -> Result<(), Error> {
        let attr = ctx.attr();
        let data = ctx.data();

        // Verify dataver
        attr.check_dataver(self.dataver.get())?;

        let mut cluster = self.cluster.write();

        match attr.attr_id.try_into()? {
            ChimeAttribute::InstalledChimeSounds => {
                // Read-only attribute - writes not supported
                Err(ErrorCode::UnsupportedAccess.into())
            }
            ChimeAttribute::SelectedChime => {
                let value: u8 = data.u8()?;
                cluster
                    .set_selected_chime(value)
                    .map_err(|_| Error::new(ErrorCode::ConstraintError))?;
                self.dataver.changed();
                Ok(())
            }
            ChimeAttribute::Enabled => {
                let value: bool = data.bool()?;
                cluster.set_enabled(value);
                self.dataver.changed();
                Ok(())
            }
        }
    }

    fn invoke_impl(&self, ctx: impl InvokeContext, _reply: impl InvokeReply) -> Result<(), Error> {
        let cmd = ctx.cmd();

        match cmd.cmd_id.try_into()? {
            ChimeCommand::PlayChimeSound => {
                let cluster = self.cluster.read();
                cluster.play_chime_sound().map_err(|e| {
                    log::warn!("PlayChimeSound failed: {}", e);
                    Error::new(ErrorCode::Failure)
                })?;

                // Return Ok(()) for DefaultSuccess (no response data)
                Ok(())
            }
        }
    }
}

impl Handler for ChimeHandler {
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

impl NonBlockingHandler for ChimeHandler {}
