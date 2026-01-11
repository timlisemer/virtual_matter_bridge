//! Device type definitions for Matter bridge.
//!
//! This module defines the device types used in this bridge,
//! following the Matter specification for various device types.

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

/// Matter Contact Sensor device type
///
/// Device Type ID: 0x0015 (21 decimal)
/// Device Type Revision: 1
///
/// Required clusters:
/// - BooleanState (0x0045)
/// - Descriptor (standard)
///
/// Used for binary sensors (open/closed, true/false states).
pub const DEV_TYPE_CONTACT_SENSOR: DeviceType = DeviceType {
    dtype: 0x0015,
    drev: 1,
};

/// Matter Occupancy Sensor device type
///
/// Device Type ID: 0x0107 (263 decimal)
/// Device Type Revision: 1
///
/// Required clusters:
/// - OccupancySensing (0x0406)
/// - Descriptor (standard)
///
/// Used for motion/presence sensors.
pub const DEV_TYPE_OCCUPANCY_SENSOR: DeviceType = DeviceType {
    dtype: 0x0107,
    drev: 1,
};

/// Matter On/Off Plug-in Unit device type
///
/// Device Type ID: 0x010A (266 decimal)
/// Device Type Revision: 2
///
/// Required clusters:
/// - OnOff (0x0006)
/// - Descriptor (standard)
///
/// Used for standalone on/off switches.
pub const DEV_TYPE_ON_OFF_PLUG_IN_UNIT: DeviceType = DeviceType {
    dtype: 0x010A,
    drev: 2,
};

/// Matter On/Off Light device type
///
/// Device Type ID: 0x0100 (256 decimal)
/// Device Type Revision: 2
///
/// Required clusters:
/// - OnOff (0x0006)
/// - Descriptor (standard)
///
/// Used for simple on/off lights.
pub const DEV_TYPE_ON_OFF_LIGHT: DeviceType = DeviceType {
    dtype: 0x0100,
    drev: 2,
};

/// Matter Aggregator device type (for bridge root)
///
/// Device Type ID: 0x000E (14 decimal)
/// Device Type Revision: 1
///
/// Required clusters:
/// - Descriptor (standard)
///
/// Used as the bridge aggregator endpoint that enumerates bridged devices.
pub const DEV_TYPE_AGGREGATOR: DeviceType = DeviceType {
    dtype: 0x000E,
    drev: 1,
};

/// Matter Bridged Node device type
///
/// Device Type ID: 0x0013 (19 decimal)
/// Device Type Revision: 1
///
/// Required clusters:
/// - BridgedDeviceBasicInformation (0x0039)
/// - Descriptor (standard)
///
/// Added to bridged device endpoints alongside their functional device type.
pub const DEV_TYPE_BRIDGED_NODE: DeviceType = DeviceType {
    dtype: 0x0013,
    drev: 1,
};

/// Matter Temperature Sensor device type
///
/// Device Type ID: 0x0302 (770 decimal)
/// Device Type Revision: 2
///
/// Required clusters:
/// - TemperatureMeasurement (0x0402)
/// - Descriptor (standard)
pub const DEV_TYPE_TEMPERATURE_SENSOR: DeviceType = DeviceType {
    dtype: 0x0302,
    drev: 2,
};

/// Matter Humidity Sensor device type
///
/// Device Type ID: 0x0307 (775 decimal)
/// Device Type Revision: 2
///
/// Required clusters:
/// - RelativeHumidityMeasurement (0x0405)
/// - Descriptor (standard)
pub const DEV_TYPE_HUMIDITY_SENSOR: DeviceType = DeviceType {
    dtype: 0x0307,
    drev: 2,
};

/// Virtual Device type enum for dynamic device creation.
///
/// This enum wraps the device type constants and provides a convenient way
/// to specify what type of Virtual Device (parent endpoint) to create.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VirtualDeviceType {
    /// Contact sensor (door/window open/close)
    ContactSensor,
    /// Occupancy/motion sensor
    OccupancySensor,
    /// On/Off plug-in unit (outlet/switch)
    OnOffPlugInUnit,
    /// On/Off light
    OnOffLight,
    /// Video doorbell with camera streaming
    VideoDoorbellDevice,
    /// Temperature sensor
    TemperatureSensor,
    /// Humidity sensor
    HumiditySensor,
}

impl VirtualDeviceType {
    /// Get the Matter DeviceType for this virtual device type.
    pub const fn device_type(&self) -> DeviceType {
        match self {
            Self::ContactSensor => DEV_TYPE_CONTACT_SENSOR,
            Self::OccupancySensor => DEV_TYPE_OCCUPANCY_SENSOR,
            Self::OnOffPlugInUnit => DEV_TYPE_ON_OFF_PLUG_IN_UNIT,
            Self::OnOffLight => DEV_TYPE_ON_OFF_LIGHT,
            Self::VideoDoorbellDevice => DEV_TYPE_VIDEO_DOORBELL,
            Self::TemperatureSensor => DEV_TYPE_TEMPERATURE_SENSOR,
            Self::HumiditySensor => DEV_TYPE_HUMIDITY_SENSOR,
        }
    }
}
