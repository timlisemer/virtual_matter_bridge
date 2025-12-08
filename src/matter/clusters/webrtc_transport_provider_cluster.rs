use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, Ordering};

/// Matter Cluster ID for WebRTC Transport Provider
pub const CLUSTER_ID: u32 = 0x0553;

/// Cluster revision
pub const CLUSTER_REVISION: u16 = 1;

/// Feature flags for the WebRTC Transport Provider cluster
#[derive(Debug, Clone, Copy, Default)]
pub struct Features {
    /// Metadata support
    pub metadata: bool,
}

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

/// WebRTC Transport Provider cluster handler
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
