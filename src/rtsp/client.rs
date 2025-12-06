use crate::error::{BridgeError, Result};
use std::sync::Arc;
use tokio::sync::RwLock;

/// RTSP stream information
#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub video_codec: String,
    pub video_width: u32,
    pub video_height: u32,
    pub video_fps: u32,
    pub audio_codec: Option<String>,
    pub audio_sample_rate: Option<u32>,
    pub audio_channels: Option<u8>,
}

/// RTSP client state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientState {
    Disconnected,
    Connecting,
    Connected,
    Streaming,
    Error,
}

/// Video frame from RTSP stream
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub data: Vec<u8>,
    pub timestamp: u64,
    pub is_keyframe: bool,
}

/// Audio frame from RTSP stream
#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub data: Vec<u8>,
    pub timestamp: u64,
}

/// Frame receiver callback types
pub type VideoFrameCallback = Box<dyn Fn(VideoFrame) + Send + Sync>;
pub type AudioFrameCallback = Box<dyn Fn(AudioFrame) + Send + Sync>;

/// RTSP client for connecting to camera streams
pub struct RtspClient {
    url: String,
    state: Arc<RwLock<ClientState>>,
    stream_info: Arc<RwLock<Option<StreamInfo>>>,
}

impl RtspClient {
    pub fn new(url: &str) -> Result<Self> {
        if !url.starts_with("rtsp://") && !url.starts_with("rtsps://") {
            return Err(BridgeError::InvalidRtspUrl(format!(
                "URL must start with rtsp:// or rtsps://, got: {}",
                url
            )));
        }

        Ok(Self {
            url: url.to_string(),
            state: Arc::new(RwLock::new(ClientState::Disconnected)),
            stream_info: Arc::new(RwLock::new(None)),
        })
    }

    /// Get the RTSP URL
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Get current client state
    pub async fn state(&self) -> ClientState {
        *self.state.read().await
    }

    /// Get stream information (available after connection)
    pub async fn stream_info(&self) -> Option<StreamInfo> {
        self.stream_info.read().await.clone()
    }

    /// Connect to the RTSP server and get stream information
    pub async fn connect(&self) -> Result<StreamInfo> {
        {
            let mut state = self.state.write().await;
            *state = ClientState::Connecting;
        }

        log::info!("Connecting to RTSP server: {}", self.url);

        // TODO: Use retina crate to actually connect
        // For now, simulate a successful connection with H.264 stream
        let info = StreamInfo {
            video_codec: "H264".to_string(),
            video_width: 1920,
            video_height: 1080,
            video_fps: 30,
            audio_codec: Some("AAC".to_string()),
            audio_sample_rate: Some(44100),
            audio_channels: Some(2),
        };

        {
            let mut stream_info = self.stream_info.write().await;
            *stream_info = Some(info.clone());
        }

        {
            let mut state = self.state.write().await;
            *state = ClientState::Connected;
        }

        log::info!(
            "Connected to RTSP server. Stream: {}x{} @ {}fps, codec: {}",
            info.video_width,
            info.video_height,
            info.video_fps,
            info.video_codec
        );

        Ok(info)
    }

    /// Start receiving frames from the RTSP stream
    pub async fn start_streaming(
        &self,
        video_callback: Option<VideoFrameCallback>,
        audio_callback: Option<AudioFrameCallback>,
    ) -> Result<()> {
        let current_state = self.state().await;
        if current_state != ClientState::Connected {
            return Err(BridgeError::RtspStreamError(format!(
                "Cannot start streaming in state {:?}",
                current_state
            )));
        }

        {
            let mut state = self.state.write().await;
            *state = ClientState::Streaming;
        }

        log::info!("Starting RTSP stream...");

        // TODO: Implement actual streaming with retina crate
        // This will involve:
        // 1. Setting up RTP/RTCP receivers
        // 2. Depacketizing H.264/AAC frames
        // 3. Calling the callbacks with frame data

        // Simulate streaming for now
        let state_clone = self.state.clone();
        tokio::spawn(async move {
            let mut frame_count = 0u64;
            loop {
                let current_state = *state_clone.read().await;
                if current_state != ClientState::Streaming {
                    break;
                }

                // Simulate receiving a video frame every ~33ms (30fps)
                tokio::time::sleep(tokio::time::Duration::from_millis(33)).await;

                if let Some(ref callback) = video_callback {
                    let frame = VideoFrame {
                        data: vec![0u8; 1024], // Placeholder data
                        timestamp: frame_count * 33,
                        is_keyframe: frame_count.is_multiple_of(30), // Keyframe every 30 frames
                    };
                    callback(frame);
                }

                if frame_count.is_multiple_of(2)
                    && let Some(ref callback) = audio_callback
                {
                    let frame = AudioFrame {
                        data: vec![0u8; 256], // Placeholder data
                        timestamp: frame_count * 33,
                    };
                    callback(frame);
                }

                frame_count += 1;
            }
        });

        Ok(())
    }

    /// Stop streaming
    pub async fn stop_streaming(&self) -> Result<()> {
        let mut state = self.state.write().await;
        if *state == ClientState::Streaming {
            *state = ClientState::Connected;
            log::info!("Stopped RTSP stream");
        }
        Ok(())
    }

    /// Disconnect from the RTSP server
    pub async fn disconnect(&self) -> Result<()> {
        let mut state = self.state.write().await;
        *state = ClientState::Disconnected;

        let mut stream_info = self.stream_info.write().await;
        *stream_info = None;

        log::info!("Disconnected from RTSP server");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_url() {
        let result = RtspClient::new("http://example.com/stream");
        assert!(result.is_err());
    }

    #[test]
    fn test_valid_url() {
        let result = RtspClient::new("rtsp://user:pass@10.0.0.1:554/stream");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_connect() {
        let client = RtspClient::new("rtsp://localhost:554/test").unwrap();
        let result = client.connect().await;
        assert!(result.is_ok());
        assert_eq!(client.state().await, ClientState::Connected);
    }
}
