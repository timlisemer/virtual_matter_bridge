//! WebRTC Transport Provider cluster handler for rs-matter integration.
//!
//! This module implements the Matter WebRTC Transport Provider cluster (0x0553)
//! by bridging the existing WebRtcTransportProviderCluster business logic to rs-matter's Handler trait.

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

use crate::clusters::webrtc_transport_provider::{
    IceServer, IceTransportPolicy, WebRtcSession, WebRtcTransportProviderCluster,
};

/// WebRTC Transport Provider Cluster ID (Matter spec)
pub const CLUSTER_ID: u32 = 0x0553;

/// Cluster revision
pub const CLUSTER_REVISION: u16 = 1;

/// Feature bits for this cluster
pub mod features {
    pub const METADATA: u32 = 0x0001;
}

/// Attribute IDs for the WebRTC Transport Provider cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum WebRtcAttribute {
    CurrentSessions = 0x0000,
}

attribute_enum!(WebRtcAttribute);

/// Command IDs for the WebRTC Transport Provider cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum WebRtcCommand {
    SolicitOffer = 0x01,
    ProvideOffer = 0x03,
    ProvideAnswer = 0x05,
    ProvideICECandidates = 0x06,
    EndSession = 0x07,
}

command_enum!(WebRtcCommand);

/// Response command IDs
pub mod response_commands {
    pub const SOLICIT_OFFER_RESPONSE: u32 = 0x02;
    pub const PROVIDE_OFFER_RESPONSE: u32 = 0x04;
}

/// Build cluster definition
pub const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    feature_map: 0,
    attributes: attributes!(
        // CurrentSessions - list of WebRTCSessionStruct, read-only, fabric-sensitive
        Attribute::new(
            WebRtcAttribute::CurrentSessions as _,
            Access::RV,
            Quality::A
        ),
    ),
    commands: commands!(
        Command::new(
            WebRtcCommand::SolicitOffer as _,
            Some(response_commands::SOLICIT_OFFER_RESPONSE),
            Access::WO
        ),
        Command::new(
            WebRtcCommand::ProvideOffer as _,
            Some(response_commands::PROVIDE_OFFER_RESPONSE),
            Access::WO
        ),
        Command::new(WebRtcCommand::ProvideAnswer as _, None, Access::WO),
        Command::new(WebRtcCommand::ProvideICECandidates as _, None, Access::WO),
        Command::new(WebRtcCommand::EndSession as _, None, Access::WO),
    ),
    with_attrs: with!(all),
    with_cmds: with!(all),
};

/// Handler that bridges the existing WebRtcTransportProviderCluster to rs-matter
pub struct WebRtcTransportProviderHandler {
    dataver: Dataver,
    cluster: Arc<RwLock<WebRtcTransportProviderCluster>>,
}

impl WebRtcTransportProviderHandler {
    /// The cluster definition for this handler
    pub const CLUSTER: Cluster<'static> = CLUSTER;

    /// Create a new handler
    pub fn new(dataver: Dataver, cluster: Arc<RwLock<WebRtcTransportProviderCluster>>) -> Self {
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
            WebRtcAttribute::CurrentSessions => {
                let sessions = cluster.get_current_sessions();
                let list_index = attr.list_index.clone().map(|li| li.into_option());
                let tag = writer.tag();

                {
                    let mut tw = writer.writer();

                    if list_index.is_none() {
                        tw.start_array(tag)?;
                    }

                    if let Some(Some(index)) = list_index.as_ref() {
                        let session = sessions
                            .get(*index as usize)
                            .ok_or(ErrorCode::ConstraintError)?;
                        Self::write_session(&mut tw, session)?;
                    } else {
                        for session in sessions {
                            Self::write_session(&mut tw, session)?;
                        }
                    }

                    if list_index.is_none() {
                        tw.end_container()?;
                    }
                }
                writer.complete()
            }
        }
    }

    fn write_session(tw: &mut impl TLVWrite, session: &WebRtcSession) -> Result<(), Error> {
        tw.start_struct(&TLVTag::Anonymous)?;
        tw.u16(&TLVTag::Context(0), session.session_id)?;
        tw.u64(&TLVTag::Context(1), session.peer_node_id)?;
        tw.u8(&TLVTag::Context(2), session.peer_fabric_index)?;
        tw.u8(&TLVTag::Context(3), session.state as u8)?;
        if let Some(video_id) = session.video_stream_id {
            tw.u16(&TLVTag::Context(4), video_id)?;
        }
        if let Some(audio_id) = session.audio_stream_id {
            tw.u16(&TLVTag::Context(5), audio_id)?;
        }
        tw.end_container()?;
        Ok(())
    }

    fn write_ice_server(tw: &mut impl TLVWrite, server: &IceServer) -> Result<(), Error> {
        tw.start_struct(&TLVTag::Anonymous)?;
        // URLs array
        tw.start_array(&TLVTag::Context(0))?;
        for url in &server.urls {
            tw.utf8(&TLVTag::Anonymous, url)?;
        }
        tw.end_container()?;
        // Username (optional)
        if let Some(ref username) = server.username {
            tw.utf8(&TLVTag::Context(1), username)?;
        }
        // Credential (optional)
        if let Some(ref credential) = server.credential {
            tw.utf8(&TLVTag::Context(2), credential)?;
        }
        tw.end_container()?;
        Ok(())
    }

    fn write_impl(&self, _ctx: impl WriteContext) -> Result<(), Error> {
        // CurrentSessions is read-only
        Err(ErrorCode::UnsupportedAccess.into())
    }

    fn invoke_impl(&self, ctx: impl InvokeContext, reply: impl InvokeReply) -> Result<(), Error> {
        let cmd = ctx.cmd();
        let data = ctx.data();

        match cmd.cmd_id.try_into()? {
            WebRtcCommand::SolicitOffer => {
                let mut seq = data.structure()?;

                // Parse request fields
                // streamUsage (context 0) - optional
                let _stream_usage = seq.scan_ctx(0).ok().and_then(|e| e.u8().ok());
                // videoStreamID (context 1) - optional
                let video_stream_id = seq.scan_ctx(1).ok().and_then(|e| e.u16().ok());
                // audioStreamID (context 2) - optional
                let audio_stream_id = seq.scan_ctx(2).ok().and_then(|e| e.u16().ok());
                // ICE servers (context 3) - optional, skip for now
                // ICE transport policy (context 4) - optional
                let ice_policy_val = seq.scan_ctx(4).ok().and_then(|e| e.u8().ok());
                let ice_policy = ice_policy_val.and_then(|v| match v {
                    0 => Some(IceTransportPolicy::All),
                    1 => Some(IceTransportPolicy::Relay),
                    _ => None,
                });

                // Get peer info from context - using placeholder values for now
                let peer_node_id = 0u64; // Would come from session context
                let peer_fabric_index = 1u8;

                let mut cluster = self.cluster.write();
                let (session_id, sdp_offer, ice_servers) = cluster
                    .solicit_offer(
                        peer_node_id,
                        peer_fabric_index,
                        video_stream_id,
                        audio_stream_id,
                        None,
                        ice_policy,
                    )
                    .map_err(|e| {
                        log::warn!("SolicitOffer failed: {}", e);
                        Error::new(ErrorCode::Failure)
                    })?;

                self.dataver.changed();

                // Send response
                let mut writer = reply.with_command(response_commands::SOLICIT_OFFER_RESPONSE)?;
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_struct(tag)?;
                    tw.u16(&TLVTag::Context(0), session_id)?;
                    tw.utf8(&TLVTag::Context(1), &sdp_offer)?;
                    // ICE servers array
                    tw.start_array(&TLVTag::Context(2))?;
                    for server in &ice_servers {
                        Self::write_ice_server(&mut tw, server)?;
                    }
                    tw.end_container()?;
                    tw.end_container()?;
                }
                writer.complete()
            }
            WebRtcCommand::ProvideOffer => {
                let mut seq = data.structure()?;

                // Parse required SDP offer
                let sdp_offer = seq
                    .scan_ctx(0)?
                    .utf8()
                    .map_err(|_| Error::new(ErrorCode::InvalidCommand))?
                    .to_string();
                // videoStreamID (context 1) - optional
                let video_stream_id = seq.scan_ctx(1).ok().and_then(|e| e.u16().ok());
                // audioStreamID (context 2) - optional
                let audio_stream_id = seq.scan_ctx(2).ok().and_then(|e| e.u16().ok());
                // ICE transport policy (context 4) - optional
                let ice_policy_val = seq.scan_ctx(4).ok().and_then(|e| e.u8().ok());
                let ice_policy = ice_policy_val.and_then(|v| match v {
                    0 => Some(IceTransportPolicy::All),
                    1 => Some(IceTransportPolicy::Relay),
                    _ => None,
                });

                let peer_node_id = 0u64;
                let peer_fabric_index = 1u8;

                let mut cluster = self.cluster.write();
                let (session_id, sdp_answer, ice_servers) = cluster
                    .provide_offer(
                        peer_node_id,
                        peer_fabric_index,
                        sdp_offer,
                        video_stream_id,
                        audio_stream_id,
                        None,
                        ice_policy,
                    )
                    .map_err(|e| {
                        log::warn!("ProvideOffer failed: {}", e);
                        Error::new(ErrorCode::Failure)
                    })?;

                self.dataver.changed();

                // Send response
                let mut writer = reply.with_command(response_commands::PROVIDE_OFFER_RESPONSE)?;
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_struct(tag)?;
                    tw.u16(&TLVTag::Context(0), session_id)?;
                    tw.utf8(&TLVTag::Context(1), &sdp_answer)?;
                    tw.start_array(&TLVTag::Context(2))?;
                    for server in &ice_servers {
                        Self::write_ice_server(&mut tw, server)?;
                    }
                    tw.end_container()?;
                    tw.end_container()?;
                }
                writer.complete()
            }
            WebRtcCommand::ProvideAnswer => {
                let mut seq = data.structure()?;
                let session_id = seq.scan_ctx(0)?.u16()?;
                let sdp_answer = seq
                    .scan_ctx(1)?
                    .utf8()
                    .map_err(|_| Error::new(ErrorCode::InvalidCommand))?
                    .to_string();

                let mut cluster = self.cluster.write();
                cluster
                    .provide_answer(session_id, sdp_answer)
                    .map_err(|e| {
                        log::warn!("ProvideAnswer failed: {}", e);
                        Error::new(ErrorCode::NotFound)
                    })?;

                self.dataver.changed();
                Ok(())
            }
            WebRtcCommand::ProvideICECandidates => {
                let mut seq = data.structure()?;
                let session_id = seq.scan_ctx(0)?.u16()?;

                // Parse ICE candidates array - simplified for now
                // In a full implementation we'd parse the full ICECandidateStruct
                let cluster = self.cluster.read();

                // For now, just acknowledge receipt without parsing candidates
                if cluster.get_session(session_id).is_none() {
                    return Err(Error::new(ErrorCode::NotFound));
                }

                self.dataver.changed();
                Ok(())
            }
            WebRtcCommand::EndSession => {
                let mut seq = data.structure()?;
                let session_id = seq.scan_ctx(0)?.u16()?;

                let mut cluster = self.cluster.write();
                cluster.end_session(session_id).map_err(|e| {
                    log::warn!("EndSession failed: {}", e);
                    Error::new(ErrorCode::NotFound)
                })?;

                self.dataver.changed();
                Ok(())
            }
        }
    }
}

impl Handler for WebRtcTransportProviderHandler {
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

impl NonBlockingHandler for WebRtcTransportProviderHandler {}
