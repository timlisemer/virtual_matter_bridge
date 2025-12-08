//! Camera input source with RTSP and WebRTC support.

mod input;
pub mod rtsp_client;
pub mod webrtc_bridge;

pub use input::CameraInput;
