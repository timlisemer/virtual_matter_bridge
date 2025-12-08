use super::rtsp_client::{AudioFrame, RtspClient, VideoFrame};
use crate::error::{BridgeError, Result};
use crate::matter::clusters::camera_av_stream_mgmt::{AudioCodec, VideoCodec, VideoResolution};
use crate::matter::clusters::webrtc_transport_provider::{IceServer, WebRtcSessionState};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;

/// Bridge session connecting an RTSP stream to a WebRTC peer
#[derive(Debug)]
pub struct BridgeSession {
    pub session_id: u16,
    pub webrtc_session_id: u16,
    pub state: BridgeSessionState,
    pub video_stream_id: Option<u16>,
    pub audio_stream_id: Option<u16>,
    pub stats: SessionStats,
}

/// Bridge session state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeSessionState {
    Initializing,
    Connecting,
    Active,
    Paused,
    Disconnected,
    Error,
}

/// Session statistics
#[derive(Debug, Default)]
pub struct SessionStats {
    pub video_frames_sent: AtomicU64,
    pub audio_frames_sent: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub start_time: Option<std::time::Instant>,
}

impl SessionStats {
    pub fn new() -> Self {
        Self {
            video_frames_sent: AtomicU64::new(0),
            audio_frames_sent: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            start_time: None,
        }
    }

    pub fn record_video_frame(&self, size: usize) {
        self.video_frames_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(size as u64, Ordering::Relaxed);
    }

    pub fn record_audio_frame(&self, size: usize) {
        self.audio_frames_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(size as u64, Ordering::Relaxed);
    }
}

/// Configuration for the WebRTC bridge
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub ice_servers: Vec<IceServer>,
    pub video_codec: VideoCodec,
    pub audio_codec: AudioCodec,
    pub max_bitrate: u32,
    pub target_resolution: Option<VideoResolution>,
    pub target_framerate: Option<u16>,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            ice_servers: vec![IceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_string()],
                username: None,
                credential: None,
            }],
            video_codec: VideoCodec::H264,
            audio_codec: AudioCodec::Opus,
            max_bitrate: 4_000_000,
            target_resolution: None,
            target_framerate: None,
        }
    }
}

/// RTSP to WebRTC bridge
/// Connects an RTSP camera stream to WebRTC peers
pub struct RtspWebRtcBridge {
    rtsp_client: Arc<RtspClient>,
    config: BridgeConfig,
    sessions: Arc<RwLock<HashMap<u16, BridgeSession>>>,
    next_session_id: Arc<std::sync::atomic::AtomicU16>,
}

impl RtspWebRtcBridge {
    pub fn new(rtsp_url: &str, config: BridgeConfig) -> Result<Self> {
        let rtsp_client = RtspClient::new(rtsp_url)?;

        Ok(Self {
            rtsp_client: Arc::new(rtsp_client),
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            next_session_id: Arc::new(std::sync::atomic::AtomicU16::new(1)),
        })
    }

    /// Get the RTSP client
    pub fn rtsp_client(&self) -> &RtspClient {
        &self.rtsp_client
    }

    /// Initialize the bridge by connecting to RTSP
    pub async fn initialize(&self) -> Result<()> {
        self.rtsp_client.connect().await?;
        Ok(())
    }

    /// Create a new bridge session for a WebRTC peer
    pub async fn create_session(
        &self,
        webrtc_session_id: u16,
        video_stream_id: Option<u16>,
        audio_stream_id: Option<u16>,
    ) -> Result<u16> {
        let session_id = self
            .next_session_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let session = BridgeSession {
            session_id,
            webrtc_session_id,
            state: BridgeSessionState::Initializing,
            video_stream_id,
            audio_stream_id,
            stats: SessionStats::new(),
        };

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id, session);
        }

        log::info!(
            "Created bridge session {} for WebRTC session {}",
            session_id,
            webrtc_session_id
        );

        Ok(session_id)
    }

    /// Start streaming to a session
    pub async fn start_session(&self, session_id: u16) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(&session_id)
            .ok_or(BridgeError::SessionNotFound(session_id))?;

        session.state = BridgeSessionState::Connecting;

        // TODO: Set up WebRTC peer connection and start forwarding frames
        // This will involve:
        // 1. Creating a WebRTC peer connection with the session's SDP
        // 2. Setting up media tracks for video/audio
        // 3. Starting to forward RTSP frames to WebRTC

        session.state = BridgeSessionState::Active;
        session.stats.start_time = Some(std::time::Instant::now());

        log::info!("Started bridge session {}", session_id);

        Ok(())
    }

    /// Stop a streaming session
    pub async fn stop_session(&self, session_id: u16) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.state = BridgeSessionState::Disconnected;
            log::info!("Stopped bridge session {}", session_id);
        }
        Ok(())
    }

    /// Remove a session
    pub async fn remove_session(&self, session_id: u16) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(&session_id);
        log::info!("Removed bridge session {}", session_id);
        Ok(())
    }

    /// Get session state
    pub async fn get_session_state(&self, session_id: u16) -> Option<BridgeSessionState> {
        let sessions = self.sessions.read().await;
        sessions.get(&session_id).map(|s| s.state)
    }

    /// Get session statistics
    pub async fn get_session_stats(&self, session_id: u16) -> Option<(u64, u64, u64)> {
        let sessions = self.sessions.read().await;
        sessions.get(&session_id).map(|s| {
            (
                s.stats.video_frames_sent.load(Ordering::Relaxed),
                s.stats.audio_frames_sent.load(Ordering::Relaxed),
                s.stats.bytes_sent.load(Ordering::Relaxed),
            )
        })
    }

    /// Get all active session IDs
    pub async fn get_active_sessions(&self) -> Vec<u16> {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .filter(|s| s.state == BridgeSessionState::Active)
            .map(|s| s.session_id)
            .collect()
    }

    /// Forward a video frame to all active sessions
    pub async fn forward_video_frame(&self, frame: VideoFrame) {
        let sessions = self.sessions.read().await;
        for session in sessions.values() {
            if session.state == BridgeSessionState::Active && session.video_stream_id.is_some() {
                // TODO: Actually send frame via WebRTC
                session.stats.record_video_frame(frame.data.len());
            }
        }
    }

    /// Forward an audio frame to all active sessions
    pub async fn forward_audio_frame(&self, frame: AudioFrame) {
        let sessions = self.sessions.read().await;
        for session in sessions.values() {
            if session.state == BridgeSessionState::Active && session.audio_stream_id.is_some() {
                // TODO: Actually send frame via WebRTC
                session.stats.record_audio_frame(frame.data.len());
            }
        }
    }

    /// Shutdown the bridge
    pub async fn shutdown(&self) -> Result<()> {
        // Stop all sessions
        let session_ids: Vec<u16> = {
            let sessions = self.sessions.read().await;
            sessions.keys().copied().collect()
        };

        for session_id in session_ids {
            self.stop_session(session_id).await?;
        }

        // Disconnect RTSP
        self.rtsp_client.disconnect().await?;

        log::info!("Bridge shutdown complete");
        Ok(())
    }
}

/// Convert WebRTC session state to bridge session state
impl From<WebRtcSessionState> for BridgeSessionState {
    fn from(state: WebRtcSessionState) -> Self {
        match state {
            WebRtcSessionState::Connecting => BridgeSessionState::Connecting,
            WebRtcSessionState::Connected => BridgeSessionState::Active,
            WebRtcSessionState::Disconnected => BridgeSessionState::Disconnected,
            WebRtcSessionState::Failed => BridgeSessionState::Error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bridge_creation() {
        let bridge =
            RtspWebRtcBridge::new("rtsp://localhost:554/test", BridgeConfig::default()).unwrap();

        let result = bridge.initialize().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_session_creation() {
        let bridge =
            RtspWebRtcBridge::new("rtsp://localhost:554/test", BridgeConfig::default()).unwrap();

        bridge.initialize().await.unwrap();

        let session_id = bridge.create_session(1, Some(1), Some(1)).await.unwrap();
        assert_eq!(session_id, 1);

        let state = bridge.get_session_state(session_id).await;
        assert_eq!(state, Some(BridgeSessionState::Initializing));
    }
}
