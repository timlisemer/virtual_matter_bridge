use thiserror::Error as ThisError;

#[derive(ThisError, Debug)]
pub enum BridgeError {
    #[error("Failed to initialize Matter stack: {0}")]
    MatterInitFailed(String),

    #[error("WebRTC session error: {0}")]
    WebRtcError(String),

    #[error("RTSP connection failed: {0}")]
    RtspConnectionFailed(String),

    #[error("RTSP stream error: {0}")]
    RtspStreamError(String),

    #[error("Invalid RTSP URL: {0}")]
    InvalidRtspUrl(String),

    #[error("Video codec not supported: {0}")]
    UnsupportedVideoCodec(String),

    #[error("Audio codec not supported: {0}")]
    UnsupportedAudioCodec(String),

    #[error("Stream allocation failed: {0}")]
    StreamAllocationFailed(String),

    #[error("Session not found: {0}")]
    SessionNotFound(u16),

    #[error("Maximum concurrent streams reached")]
    MaxStreamsReached,

    #[error("ICE connection failed: {0}")]
    IceConnectionFailed(String),

    #[error("SDP negotiation failed: {0}")]
    SdpNegotiationFailed(String),

    #[error("Doorbell press simulation failed: {0}")]
    DoorbellSimulationFailed(String),

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, BridgeError>;
