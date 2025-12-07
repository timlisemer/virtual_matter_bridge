//! Device type definitions for Matter video doorbell.
//!
//! This module defines the device types used in this bridge,
//! following the Matter 1.5 specification for camera devices.

use rs_matter::dm::DeviceType;

/// Matter Video Doorbell device type (Matter 1.5 spec)
///
/// Device Type ID: 0x0012 (18 decimal)
/// Device Type Revision: 1
///
/// Required clusters:
/// - CameraAvStreamManagement (0x0551)
/// - WebRTCTransportProvider (0x0553)
/// - Descriptor (standard)
pub const DEV_TYPE_VIDEO_DOORBELL: DeviceType = DeviceType {
    dtype: 0x0012,
    drev: 1,
};

/// Matter Basic Video Camera device type
///
/// Device Type ID: 0x0014 (20 decimal)
/// Device Type Revision: 1
///
/// Required clusters:
/// - CameraAvStreamManagement (0x0551)
/// - WebRTCTransportProvider (0x0553)
/// - Descriptor (standard)
///
/// This can be used as a fallback if controllers don't support
/// the Video Doorbell device type yet.
pub const DEV_TYPE_BASIC_VIDEO_CAMERA: DeviceType = DeviceType {
    dtype: 0x0014,
    drev: 1,
};
