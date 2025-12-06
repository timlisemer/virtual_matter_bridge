use crate::clusters::camera_av_stream_mgmt::{
    CameraAvStreamMgmtCluster, Features as CameraFeatures, StreamUsage, VideoCodec, VideoResolution,
};
use crate::clusters::chime::{ChimeCluster, ChimeSound};
use crate::clusters::webrtc_transport_provider::{
    Features as WebRtcFeatures, IceServer, WebRtcTransportProviderCluster,
};
use crate::config::Config;
use crate::error::{BridgeError, Result};
use crate::rtsp::webrtc_bridge::{BridgeConfig, RtspWebRtcBridge};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;

/// Matter Device Type ID for Video Doorbell
/// Note: This is a placeholder - actual ID from Matter 1.5 spec
pub const DEVICE_TYPE_VIDEO_DOORBELL: u32 = 0x0012;

/// Video Doorbell device combining Camera and Doorbell functionality
pub struct VideoDoorbellDevice {
    config: Config,
    camera_cluster: Arc<RwLock<CameraAvStreamMgmtCluster>>,
    webrtc_cluster: Arc<RwLock<WebRtcTransportProviderCluster>>,
    chime_cluster: Arc<RwLock<ChimeCluster>>,
    bridge: Arc<RwLock<Option<RtspWebRtcBridge>>>,
    doorbell_pressed: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
}

impl VideoDoorbellDevice {
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

        // Initialize chime cluster
        let chimes: Vec<ChimeSound> = config
            .doorbell
            .installed_chimes
            .iter()
            .map(|c| ChimeSound {
                chime_id: c.id,
                name: c.name.clone(),
            })
            .collect();
        let mut chime_cluster = ChimeCluster::with_sounds(chimes);
        chime_cluster.set_enabled(config.doorbell.chime_enabled);
        let _ = chime_cluster.set_selected_chime(config.doorbell.selected_chime);

        Self {
            config,
            camera_cluster: Arc::new(RwLock::new(camera_cluster)),
            webrtc_cluster: Arc::new(RwLock::new(webrtc_cluster)),
            chime_cluster: Arc::new(RwLock::new(chime_cluster)),
            bridge: Arc::new(RwLock::new(None)),
            doorbell_pressed: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Initialize the device and connect to the RTSP camera
    pub async fn initialize(&self) -> Result<()> {
        log::info!("Initializing Video Doorbell device...");

        let bridge_config = BridgeConfig {
            ice_servers: self.get_ice_servers().await,
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

        log::info!("Video Doorbell device initialized successfully");
        Ok(())
    }

    /// Get configured ICE servers
    async fn get_ice_servers(&self) -> Vec<IceServer> {
        let webrtc = self.webrtc_cluster.read().await;
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

    /// Get the device name
    pub fn device_name(&self) -> &str {
        &self.config.matter.device_name
    }

    /// Get Matter vendor ID
    pub fn vendor_id(&self) -> u16 {
        self.config.matter.vendor_id
    }

    /// Get Matter product ID
    pub fn product_id(&self) -> u16 {
        self.config.matter.product_id
    }

    /// Get Matter discriminator
    pub fn discriminator(&self) -> u16 {
        self.config.matter.discriminator
    }

    /// Get Matter passcode
    pub fn passcode(&self) -> u32 {
        self.config.matter.passcode
    }

    /// Simulate a doorbell press
    pub async fn press_doorbell(&self) -> Result<()> {
        log::info!("Doorbell pressed!");

        self.doorbell_pressed.store(true, Ordering::SeqCst);

        // Play chime sound
        {
            let chime = self.chime_cluster.read().await;
            if let Err(e) = chime.play_chime_sound() {
                log::warn!("Failed to play chime: {}", e);
            }
        }

        // Reset doorbell state after a short delay
        let doorbell_pressed = self.doorbell_pressed.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            doorbell_pressed.store(false, Ordering::SeqCst);
        });

        Ok(())
    }

    /// Check if doorbell is currently pressed
    pub fn is_doorbell_pressed(&self) -> bool {
        self.doorbell_pressed.load(Ordering::SeqCst)
    }

    /// Request a video stream
    pub async fn request_video_stream(
        &self,
        peer_node_id: u64,
        peer_fabric_index: u8,
    ) -> Result<(u16, u16, String)> {
        // Allocate a video stream
        let video_stream_id = {
            let mut camera = self.camera_cluster.write().await;
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
            let mut webrtc = self.webrtc_cluster.write().await;
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

    /// End a video stream
    pub async fn end_video_stream(&self, session_id: u16, video_stream_id: u16) -> Result<()> {
        // End WebRTC session
        {
            let mut webrtc = self.webrtc_cluster.write().await;
            webrtc
                .end_session(session_id)
                .map_err(|e| BridgeError::WebRtcError(e.to_string()))?;
        }

        // Deallocate video stream
        {
            let mut camera = self.camera_cluster.write().await;
            camera
                .video_stream_deallocate(video_stream_id)
                .map_err(|e| BridgeError::StreamAllocationFailed(e.to_string()))?;
        }

        log::info!("Video stream {} ended", video_stream_id);
        Ok(())
    }

    /// Get camera cluster for external access
    pub fn camera_cluster(&self) -> Arc<RwLock<CameraAvStreamMgmtCluster>> {
        self.camera_cluster.clone()
    }

    /// Get WebRTC cluster for external access
    pub fn webrtc_cluster(&self) -> Arc<RwLock<WebRtcTransportProviderCluster>> {
        self.webrtc_cluster.clone()
    }

    /// Get chime cluster for external access
    pub fn chime_cluster(&self) -> Arc<RwLock<ChimeCluster>> {
        self.chime_cluster.clone()
    }

    /// Check if device is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Shutdown the device
    pub async fn shutdown(&self) -> Result<()> {
        log::info!("Shutting down Video Doorbell device...");

        self.running.store(false, Ordering::SeqCst);

        // Shutdown bridge
        {
            let bridge_lock = self.bridge.read().await;
            if let Some(bridge) = bridge_lock.as_ref() {
                bridge.shutdown().await?;
            }
        }

        log::info!("Video Doorbell device shutdown complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_device_creation() {
        let config = Config::default();
        let device = VideoDoorbellDevice::new(config);

        assert!(!device.is_running());
        assert!(!device.is_doorbell_pressed());
    }

    #[tokio::test]
    async fn test_doorbell_press() {
        let config = Config::default();
        let device = VideoDoorbellDevice::new(config);

        device.press_doorbell().await.unwrap();
        assert!(device.is_doorbell_pressed());

        // Wait for reset
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        assert!(!device.is_doorbell_pressed());
    }
}
