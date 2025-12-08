//! Input sources for the Matter bridge.
//!
//! This module contains different input source types that can feed data
//! into the Matter device clusters. Each input source type handles a
//! specific protocol or data format.
//!
//! Current input sources:
//! - `camera`: RTSP camera with WebRTC streaming support

pub mod camera;

pub use camera::CameraInput;
