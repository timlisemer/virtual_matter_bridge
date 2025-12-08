//! WebRTC Transport Provider cluster (0x0553).
//!
//! This module implements the Matter WebRTC Transport Provider cluster,
//! including both the business logic/data structures and the rs-matter Handler.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use strum::FromRepr;

use rs_matter::dm::{
    Access, Attribute, Cluster, Command, Dataver, Handler, InvokeContext, InvokeReply,
    NonBlockingHandler, Quality, ReadContext, ReadReply, Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::{TLVTag, TLVWrite};
use rs_matter::{attribute_enum, attributes, command_enum, commands, with};

// ============================================================================
// Cluster Constants
// ============================================================================

/// Matter Cluster ID for WebRTC Transport Provider
pub const CLUSTER_ID: u32 = 0x0553;

/// Cluster revision
pub const CLUSTER_REVISION: u16 = 1;

// ============================================================================
// Feature Flags
// ============================================================================

/// Feature flags for the WebRTC Transport Provider cluster
#[derive(Debug, Clone, Copy, Default)]
pub struct Features {
    /// Metadata support
    pub metadata: bool,
}

/// Feature bits for this cluster (used by handler)
pub mod features {
    pub const METADATA: u32 = 0x0001;
}

// ============================================================================
// Data Enums
// ============================================================================

/// ICE transport policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum IceTransportPolicy {
    /// Use all available candidates
    All = 0x00,
    /// Only use relay candidates (TURN)
    Relay = 0x01,
}

/// WebRTC session state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum WebRtcSessionState {
    /// Session is being set up
    Connecting = 0x00,
    /// Session is active
    Connected = 0x01,
    /// Session is disconnected
    Disconnected = 0x02,
    /// Session failed
    Failed = 0x03,
}

// ============================================================================
// Data Structures
// ============================================================================

/// ICE server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

/// ICE candidate structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceCandidate {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_m_line_index: Option<u16>,
}

/// SFrame encryption structure (for secure media)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SFrame {
    pub cipher_suite: u16,
    pub base_key: Vec<u8>,
    pub kid: Vec<u8>,
}

/// WebRTC session structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRtcSession {
    pub session_id: u16,
    pub peer_node_id: u64,
    pub peer_fabric_index: u8,
    pub state: WebRtcSessionState,
    pub video_stream_id: Option<u16>,
    pub audio_stream_id: Option<u16>,
    pub ice_transport_policy: IceTransportPolicy,
    pub local_sdp: Option<String>,
    pub remote_sdp: Option<String>,
    pub ice_candidates: Vec<IceCandidate>,
}

// ============================================================================
// Cluster Business Logic
// ============================================================================

/// WebRTC Transport Provider cluster
pub struct WebRtcTransportProviderCluster {
    pub features: Features,
    sessions: HashMap<u16, WebRtcSession>,
    next_session_id: AtomicU16,
    default_ice_servers: Vec<IceServer>,
}

impl WebRtcTransportProviderCluster {
    pub fn new(features: Features, ice_servers: Vec<IceServer>) -> Self {
        Self {
            features,
            sessions: HashMap::new(),
            next_session_id: AtomicU16::new(1),
            default_ice_servers: ice_servers,
        }
    }

    /// Get current sessions (fabric-sensitive attribute)
    pub fn get_current_sessions(&self) -> Vec<&WebRtcSession> {
        self.sessions.values().collect()
    }

    /// Handle SolicitOffer command
    /// Initiates a new WebRTC session where the provider generates the offer
    pub fn solicit_offer(
        &mut self,
        peer_node_id: u64,
        peer_fabric_index: u8,
        video_stream_id: Option<u16>,
        audio_stream_id: Option<u16>,
        ice_servers: Option<Vec<IceServer>>,
        ice_transport_policy: Option<IceTransportPolicy>,
    ) -> Result<(u16, String, Vec<IceServer>), &'static str> {
        let session_id = self.next_session_id.fetch_add(1, Ordering::SeqCst);

        let session = WebRtcSession {
            session_id,
            peer_node_id,
            peer_fabric_index,
            state: WebRtcSessionState::Connecting,
            video_stream_id,
            audio_stream_id,
            ice_transport_policy: ice_transport_policy.unwrap_or(IceTransportPolicy::All),
            local_sdp: None,
            remote_sdp: None,
            ice_candidates: Vec::new(),
        };

        self.sessions.insert(session_id, session);

        let servers = ice_servers.unwrap_or_else(|| self.default_ice_servers.clone());
        let offer_sdp = self.generate_offer_sdp(session_id, video_stream_id, audio_stream_id)?;

        if let Some(session) = self.sessions.get_mut(&session_id) {
            session.local_sdp = Some(offer_sdp.clone());
        }

        Ok((session_id, offer_sdp, servers))
    }

    /// Handle ProvideOffer command
    /// Client provides an SDP offer, provider generates answer
    #[allow(clippy::too_many_arguments)]
    pub fn provide_offer(
        &mut self,
        peer_node_id: u64,
        peer_fabric_index: u8,
        sdp_offer: String,
        video_stream_id: Option<u16>,
        audio_stream_id: Option<u16>,
        ice_servers: Option<Vec<IceServer>>,
        ice_transport_policy: Option<IceTransportPolicy>,
    ) -> Result<(u16, String, Vec<IceServer>), &'static str> {
        let session_id = self.next_session_id.fetch_add(1, Ordering::SeqCst);

        let session = WebRtcSession {
            session_id,
            peer_node_id,
            peer_fabric_index,
            state: WebRtcSessionState::Connecting,
            video_stream_id,
            audio_stream_id,
            ice_transport_policy: ice_transport_policy.unwrap_or(IceTransportPolicy::All),
            local_sdp: None,
            remote_sdp: Some(sdp_offer),
            ice_candidates: Vec::new(),
        };

        self.sessions.insert(session_id, session);

        let servers = ice_servers.unwrap_or_else(|| self.default_ice_servers.clone());
        let answer_sdp = self.generate_answer_sdp(session_id, video_stream_id, audio_stream_id)?;

        if let Some(session) = self.sessions.get_mut(&session_id) {
            session.local_sdp = Some(answer_sdp.clone());
        }

        Ok((session_id, answer_sdp, servers))
    }

    /// Handle ProvideAnswer command
    /// Client provides an SDP answer to our previously sent offer
    pub fn provide_answer(
        &mut self,
        session_id: u16,
        sdp_answer: String,
    ) -> Result<(), &'static str> {
        let session = self
            .sessions
            .get_mut(&session_id)
            .ok_or("Session not found")?;

        session.remote_sdp = Some(sdp_answer);
        session.state = WebRtcSessionState::Connected;

        Ok(())
    }

    /// Handle ProvideICECandidates command
    /// Add ICE candidates during the gathering phase
    pub fn provide_ice_candidates(
        &mut self,
        session_id: u16,
        candidates: Vec<IceCandidate>,
    ) -> Result<(), &'static str> {
        let session = self
            .sessions
            .get_mut(&session_id)
            .ok_or("Session not found")?;

        session.ice_candidates.extend(candidates);
        Ok(())
    }

    /// Handle EndSession command
    /// Terminate a WebRTC session
    pub fn end_session(&mut self, session_id: u16) -> Result<(), &'static str> {
        if self.sessions.remove(&session_id).is_some() {
            Ok(())
        } else {
            Err("Session not found")
        }
    }

    /// Get a session by ID
    pub fn get_session(&self, session_id: u16) -> Option<&WebRtcSession> {
        self.sessions.get(&session_id)
    }

    /// Update session state
    pub fn update_session_state(
        &mut self,
        session_id: u16,
        state: WebRtcSessionState,
    ) -> Result<(), &'static str> {
        let session = self
            .sessions
            .get_mut(&session_id)
            .ok_or("Session not found")?;
        session.state = state;
        Ok(())
    }

    /// Generate an SDP offer for a session
    fn generate_offer_sdp(
        &self,
        _session_id: u16,
        video_stream_id: Option<u16>,
        audio_stream_id: Option<u16>,
    ) -> Result<String, &'static str> {
        let mut sdp = String::from("v=0\r\n");
        sdp.push_str("o=- 0 0 IN IP4 0.0.0.0\r\n");
        sdp.push_str("s=Matter Camera\r\n");
        sdp.push_str("t=0 0\r\n");
        sdp.push_str("a=group:BUNDLE");

        if video_stream_id.is_some() {
            sdp.push_str(" video");
        }
        if audio_stream_id.is_some() {
            sdp.push_str(" audio");
        }
        sdp.push_str("\r\n");

        if video_stream_id.is_some() {
            sdp.push_str("m=video 9 UDP/TLS/RTP/SAVPF 96\r\n");
            sdp.push_str("c=IN IP4 0.0.0.0\r\n");
            sdp.push_str("a=rtcp:9 IN IP4 0.0.0.0\r\n");
            sdp.push_str("a=ice-ufrag:placeholder\r\n");
            sdp.push_str("a=ice-pwd:placeholder\r\n");
            sdp.push_str("a=fingerprint:sha-256 placeholder\r\n");
            sdp.push_str("a=setup:actpass\r\n");
            sdp.push_str("a=mid:video\r\n");
            sdp.push_str("a=sendonly\r\n");
            sdp.push_str("a=rtcp-mux\r\n");
            sdp.push_str("a=rtpmap:96 H264/90000\r\n");
            sdp.push_str("a=fmtp:96 profile-level-id=42e01f;packetization-mode=1\r\n");
        }

        if audio_stream_id.is_some() {
            sdp.push_str("m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n");
            sdp.push_str("c=IN IP4 0.0.0.0\r\n");
            sdp.push_str("a=rtcp:9 IN IP4 0.0.0.0\r\n");
            sdp.push_str("a=ice-ufrag:placeholder\r\n");
            sdp.push_str("a=ice-pwd:placeholder\r\n");
            sdp.push_str("a=fingerprint:sha-256 placeholder\r\n");
            sdp.push_str("a=setup:actpass\r\n");
            sdp.push_str("a=mid:audio\r\n");
            sdp.push_str("a=sendonly\r\n");
            sdp.push_str("a=rtcp-mux\r\n");
            sdp.push_str("a=rtpmap:111 opus/48000/2\r\n");
        }

        Ok(sdp)
    }

    /// Generate an SDP answer for a session
    fn generate_answer_sdp(
        &self,
        _session_id: u16,
        video_stream_id: Option<u16>,
        audio_stream_id: Option<u16>,
    ) -> Result<String, &'static str> {
        // Similar to offer but with setup:active instead of actpass
        let mut sdp = String::from("v=0\r\n");
        sdp.push_str("o=- 0 0 IN IP4 0.0.0.0\r\n");
        sdp.push_str("s=Matter Camera\r\n");
        sdp.push_str("t=0 0\r\n");
        sdp.push_str("a=group:BUNDLE");

        if video_stream_id.is_some() {
            sdp.push_str(" video");
        }
        if audio_stream_id.is_some() {
            sdp.push_str(" audio");
        }
        sdp.push_str("\r\n");

        if video_stream_id.is_some() {
            sdp.push_str("m=video 9 UDP/TLS/RTP/SAVPF 96\r\n");
            sdp.push_str("c=IN IP4 0.0.0.0\r\n");
            sdp.push_str("a=rtcp:9 IN IP4 0.0.0.0\r\n");
            sdp.push_str("a=ice-ufrag:placeholder\r\n");
            sdp.push_str("a=ice-pwd:placeholder\r\n");
            sdp.push_str("a=fingerprint:sha-256 placeholder\r\n");
            sdp.push_str("a=setup:active\r\n");
            sdp.push_str("a=mid:video\r\n");
            sdp.push_str("a=sendonly\r\n");
            sdp.push_str("a=rtcp-mux\r\n");
            sdp.push_str("a=rtpmap:96 H264/90000\r\n");
            sdp.push_str("a=fmtp:96 profile-level-id=42e01f;packetization-mode=1\r\n");
        }

        if audio_stream_id.is_some() {
            sdp.push_str("m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n");
            sdp.push_str("c=IN IP4 0.0.0.0\r\n");
            sdp.push_str("a=rtcp:9 IN IP4 0.0.0.0\r\n");
            sdp.push_str("a=ice-ufrag:placeholder\r\n");
            sdp.push_str("a=ice-pwd:placeholder\r\n");
            sdp.push_str("a=fingerprint:sha-256 placeholder\r\n");
            sdp.push_str("a=setup:active\r\n");
            sdp.push_str("a=mid:audio\r\n");
            sdp.push_str("a=sendonly\r\n");
            sdp.push_str("a=rtcp-mux\r\n");
            sdp.push_str("a=rtpmap:111 opus/48000/2\r\n");
        }

        Ok(sdp)
    }
}

// ============================================================================
// Handler Enums
// ============================================================================

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

// ============================================================================
// Handler Implementation
// ============================================================================

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

/// Handler that bridges the WebRtcTransportProviderCluster to rs-matter
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solicit_offer() {
        let ice_servers = vec![IceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            username: None,
            credential: None,
        }];

        let mut cluster = WebRtcTransportProviderCluster::new(Features::default(), ice_servers);

        let result = cluster.solicit_offer(
            12345,
            1,
            Some(1),
            Some(1),
            None,
            Some(IceTransportPolicy::All),
        );

        assert!(result.is_ok());
        let (session_id, sdp, servers) = result.unwrap();
        assert_eq!(session_id, 1);
        assert!(sdp.contains("m=video"));
        assert!(sdp.contains("m=audio"));
        assert!(!servers.is_empty());
    }

    #[test]
    fn test_end_session() {
        let mut cluster = WebRtcTransportProviderCluster::new(Features::default(), vec![]);

        let (session_id, _, _) = cluster
            .solicit_offer(12345, 1, Some(1), None, None, None)
            .unwrap();

        assert!(cluster.end_session(session_id).is_ok());
        assert!(cluster.get_session(session_id).is_none());
    }
}
