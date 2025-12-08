//! Matter cluster handlers for the video doorbell device.
//!
//! This module provides handlers that bridge the existing cluster business logic
//! in `src/clusters/` to rs-matter's data model traits.
//!
//! Note: We implement ClusterHandler traits manually rather than using the import! macro
//! because the provisional camera clusters have path resolution issues when used from
//! outside the rs-matter crate.

pub mod camera_av_stream_mgmt;
pub mod time_sync;
pub mod webrtc_transport_provider;

// Re-export for convenience
pub use camera_av_stream_mgmt::CameraAvStreamMgmtHandler;
pub use time_sync::TimeSyncHandler;
pub use webrtc_transport_provider::WebRtcTransportProviderHandler;
