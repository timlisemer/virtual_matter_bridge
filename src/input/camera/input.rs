//! Camera input source with RTSP and WebRTC support.
//!
//! This module provides a camera input that connects to an RTSP stream
//! and exposes it via Matter camera clusters (CameraAvStreamManagement
//! and WebRTCTransportProvider).

use super::webrtc_bridge::{BridgeConfig, RtspWebRtcBridge};
use crate::config::Config;
use crate::error::{BridgeError, Result};
use crate::matter::clusters::camera_av_stream_mgmt::{
    CameraAvStreamMgmtCluster, Features as CameraFeatures, StreamUsage, VideoCodec, VideoResolution,
};
use crate::matter::clusters::webrtc_transport_provider::{
    Features as WebRtcFeatures, IceServer, WebRtcTransportProviderCluster,
};
use crate::matter::controls::on_off_hooks::DevicePowerSwitch;
use parking_lot::RwLock as SyncRwLock;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock as AsyncRwLock;

/// Camera input source combining RTSP and WebRTC functionality.
///
/// Connects to an RTSP camera stream and exposes it through Matter
/// camera-related clusters for streaming to controllers.
pub struct CameraInput {
    config: Config,
    /// Cluster instances use sync RwLock for Matter handler compatibility
    camera_cluster: Arc<SyncRwLock<CameraAvStreamMgmtCluster>>,
    webrtc_cluster: Arc<SyncRwLock<WebRtcTransportProviderCluster>>,
    /// OnOff hooks for device power state (used by rs-matter's OnOffHandler)
    on_off_hooks: Arc<DevicePowerSwitch>,
    /// Bridge uses async RwLock for async I/O operations
    bridge: Arc<AsyncRwLock<Option<RtspWebRtcBridge>>>,
    running: Arc<AtomicBool>,
}

impl CameraInput {
    /// Create a new camera input with the given configuration.
    pub fn new(config: Config) -> Self {
        // Initialize camera cluster with video and audio features
        let camera_features = CameraFeatures {
            video: true,
            audio: true,
            privacy: false,
            snapshot: false,
            speaker: false,
            image_control: false,
            watermark: false,
            osd: false,
            local_storage: false,
            hdr: false,
            night_vision: false,
        };
        let camera_cluster = CameraAvStreamMgmtCluster::new(camera_features);

        // Initialize WebRTC cluster
        let webrtc_features = WebRtcFeatures { metadata: false };
        let ice_servers: Vec<IceServer> = config
            .webrtc
            .stun_servers
            .iter()
            .map(|url| IceServer {
                urls: vec![url.clone()],
                username: None,
                credential: None,
            })
            .chain(config.webrtc.turn_servers.iter().map(|turn| IceServer {
                urls: vec![turn.url.clone()],
                username: Some(turn.username.clone()),
                credential: Some(turn.credential.clone()),
            }))
            .collect();
        let webrtc_cluster = WebRtcTransportProviderCluster::new(webrtc_features, ice_servers);

        Self {
            config,
            camera_cluster: Arc::new(SyncRwLock::new(camera_cluster)),
            webrtc_cluster: Arc::new(SyncRwLock::new(webrtc_cluster)),
            on_off_hooks: Arc::new(DevicePowerSwitch::new()),
            bridge: Arc::new(AsyncRwLock::new(None)),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Initialize the camera input and connect to the RTSP stream.
    pub async fn initialize(&self) -> Result<()> {
        log::info!("Initializing camera input...");

        let bridge_config = BridgeConfig {
            ice_servers: self.get_ice_servers(),
            video_codec: VideoCodec::H264,
            ..Default::default()
        };

        let bridge = RtspWebRtcBridge::new(&self.config.rtsp.url, bridge_config)?;
        bridge.initialize().await?;

        {
            let mut bridge_lock = self.bridge.write().await;
            *bridge_lock = Some(bridge);
        }

        self.running.store(true, Ordering::SeqCst);

        log::info!("Camera input initialized successfully");
        Ok(())
    }

    /// Get configured ICE servers.
    fn get_ice_servers(&self) -> Vec<IceServer> {
        let webrtc = self.webrtc_cluster.read();
        webrtc
            .get_current_sessions()
            .first()
            .map(|_| vec![])
            .unwrap_or_else(|| {
                self.config
                    .webrtc
                    .stun_servers
                    .iter()
                    .map(|url| IceServer {
                        urls: vec![url.clone()],
                        username: None,
                        credential: None,
                    })
                    .collect()
            })
    }

    /// Get the device name from config.
    pub fn device_name(&self) -> &str {
        &self.config.matter.device_name
    }

    /// Get Matter vendor ID.
    pub fn vendor_id(&self) -> u16 {
        self.config.matter.vendor_id
    }

    /// Get Matter product ID.
    pub fn product_id(&self) -> u16 {
        self.config.matter.product_id
    }

    /// Get Matter discriminator.
    pub fn discriminator(&self) -> u16 {
        self.config.matter.discriminator
    }

    /// Get Matter passcode.
    pub fn passcode(&self) -> u32 {
        self.config.matter.passcode
    }

    /// Request a video stream.
    pub async fn request_video_stream(
        &self,
        peer_node_id: u64,
        peer_fabric_index: u8,
    ) -> Result<(u16, u16, String)> {
        // Allocate a video stream
        let video_stream_id = {
            let mut camera = self.camera_cluster.write();
            camera
                .video_stream_allocate(
                    StreamUsage::LiveView,
                    VideoCodec::H264,
                    15,
                    30,
                    VideoResolution::new(640, 480),
                    VideoResolution::new(1920, 1080),
                    500_000,
                    4_000_000,
                )
                .map_err(|e| BridgeError::StreamAllocationFailed(e.to_string()))?
        };

        // Start WebRTC session
        let (session_id, sdp, _servers) = {
            let mut webrtc = self.webrtc_cluster.write();
            webrtc
                .solicit_offer(
                    peer_node_id,
                    peer_fabric_index,
                    Some(video_stream_id),
                    None,
                    None,
                    None,
                )
                .map_err(|e| BridgeError::WebRtcError(e.to_string()))?
        };

        // Start bridge session
        {
            let bridge_lock = self.bridge.read().await;
            if let Some(bridge) = bridge_lock.as_ref() {
                let bridge_session_id = bridge
                    .create_session(session_id, Some(video_stream_id), None)
                    .await?;
                bridge.start_session(bridge_session_id).await?;
            }
        }

        log::info!(
            "Video stream {} started for WebRTC session {}",
            video_stream_id,
            session_id
        );

        Ok((session_id, video_stream_id, sdp))
    }

    /// End a video stream.
    pub async fn end_video_stream(&self, session_id: u16, video_stream_id: u16) -> Result<()> {
        // End WebRTC session
        {
            let mut webrtc = self.webrtc_cluster.write();
            webrtc
                .end_session(session_id)
                .map_err(|e| BridgeError::WebRtcError(e.to_string()))?;
        }

        // Deallocate video stream
        {
            let mut camera = self.camera_cluster.write();
            camera
                .video_stream_deallocate(video_stream_id)
                .map_err(|e| BridgeError::StreamAllocationFailed(e.to_string()))?;
        }

        log::info!("Video stream {} ended", video_stream_id);
        Ok(())
    }

    /// Get camera cluster for external access (Matter handler).
    pub fn camera_cluster(&self) -> Arc<SyncRwLock<CameraAvStreamMgmtCluster>> {
        self.camera_cluster.clone()
    }

    /// Get WebRTC cluster for external access (Matter handler).
    pub fn webrtc_cluster(&self) -> Arc<SyncRwLock<WebRtcTransportProviderCluster>> {
        self.webrtc_cluster.clone()
    }

    /// Get OnOff hooks for external access (Matter stack).
    pub fn on_off_hooks(&self) -> Arc<DevicePowerSwitch> {
        self.on_off_hooks.clone()
    }

    /// Check if the device power is on.
    pub fn is_powered_on(&self) -> bool {
        self.on_off_hooks.is_on()
    }

    /// Check if the camera input is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Shutdown the camera input.
    pub async fn shutdown(&self) -> Result<()> {
        log::info!("Shutting down camera input...");

        self.running.store(false, Ordering::SeqCst);

        // Shutdown bridge
        {
            let bridge_lock = self.bridge.read().await;
            if let Some(bridge) = bridge_lock.as_ref() {
                bridge.shutdown().await?;
            }
        }

        log::info!("Camera input shutdown complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_camera_creation() {
        let config = Config::default();
        let camera = CameraInput::new(config);

        assert!(!camera.is_running());
    }
}
