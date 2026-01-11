//! Virtual Device configuration for dynamic Matter endpoint creation.
//!
//! A Virtual Device represents a parent endpoint with one or more child Endpoints.
//! This module provides the configuration types needed to define devices at startup.

use super::clusters::{HumiditySensor, TemperatureSensor};
use super::device_types::VirtualDeviceType;
use super::endpoints::EndpointHandler;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

/// Type of endpoint (determines which cluster handler to use).
///
/// This defines what kind of child endpoint to create within a Virtual Device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndpointKind {
    /// Contact sensor using BooleanState cluster (0x0045)
    ContactSensor,
    /// Occupancy sensor using OccupancySensing cluster (0x0406)
    OccupancySensor,
    /// Switch using OnOff cluster (0x0006) - appears as plug-in unit
    Switch,
    /// Light switch using OnOff cluster (0x0006) - appears as light
    LightSwitch,
    /// Video doorbell camera using CameraAvStreamMgmt (0x0551) and WebRtcTransportProvider (0x0553) clusters
    VideoDoorbellCamera,
    /// Temperature sensor using TemperatureMeasurement cluster (0x0402)
    TemperatureSensor,
    /// Humidity sensor using RelativeHumidityMeasurement cluster (0x0405)
    HumiditySensor,
}

/// Configuration for a child endpoint within a Virtual Device.
///
/// Each endpoint has a label (displayed in controllers), a kind (determines
/// the cluster), and a handler for bidirectional communication.
pub struct EndpointConfig {
    /// Label displayed in Matter controllers
    pub label: &'static str,
    /// Type of endpoint (determines cluster handler)
    pub kind: EndpointKind,
    /// Handler for bidirectional communication with business logic (boolean sensors/switches)
    pub handler: Arc<dyn EndpointHandler>,
    /// Optional temperature sensor (for TemperatureSensor endpoints)
    pub temperature_sensor: Option<Arc<TemperatureSensor>>,
    /// Optional humidity sensor (for HumiditySensor endpoints)
    pub humidity_sensor: Option<Arc<HumiditySensor>>,
}

impl EndpointConfig {
    /// Create a contact sensor endpoint (BooleanState cluster).
    ///
    /// Used for door/window sensors that report open/closed state.
    pub fn contact_sensor(label: &'static str, handler: Arc<dyn EndpointHandler>) -> Self {
        Self {
            label,
            kind: EndpointKind::ContactSensor,
            handler,
            temperature_sensor: None,
            humidity_sensor: None,
        }
    }

    /// Create an occupancy sensor endpoint (OccupancySensing cluster).
    ///
    /// Used for motion/presence sensors.
    pub fn occupancy_sensor(label: &'static str, handler: Arc<dyn EndpointHandler>) -> Self {
        Self {
            label,
            kind: EndpointKind::OccupancySensor,
            handler,
            temperature_sensor: None,
            humidity_sensor: None,
        }
    }

    /// Create a switch endpoint (OnOff cluster, plug-in unit appearance).
    ///
    /// Used for power outlets, relays, or generic switches.
    pub fn switch(label: &'static str, handler: Arc<dyn EndpointHandler>) -> Self {
        Self {
            label,
            kind: EndpointKind::Switch,
            handler,
            temperature_sensor: None,
            humidity_sensor: None,
        }
    }

    /// Create a light switch endpoint (OnOff cluster, light appearance).
    ///
    /// Used for lights - appears as a light in controllers.
    pub fn light_switch(label: &'static str, handler: Arc<dyn EndpointHandler>) -> Self {
        Self {
            label,
            kind: EndpointKind::LightSwitch,
            handler,
            temperature_sensor: None,
            humidity_sensor: None,
        }
    }

    /// Create a video doorbell camera endpoint (CameraAvStreamMgmt + WebRtcTransportProvider clusters).
    ///
    /// Used for video doorbells and cameras with streaming capability.
    pub fn video_doorbell_camera(label: &'static str, handler: Arc<dyn EndpointHandler>) -> Self {
        Self {
            label,
            kind: EndpointKind::VideoDoorbellCamera,
            handler,
            temperature_sensor: None,
            humidity_sensor: None,
        }
    }

    /// Create a temperature sensor endpoint (TemperatureMeasurement cluster).
    ///
    /// Used for temperature sensors that report temperature values.
    /// The sensor Arc can be cloned and used to update the temperature from external sources.
    pub fn temperature_sensor(label: &'static str, sensor: Arc<TemperatureSensor>) -> Self {
        // Create a dummy handler - not used for temperature sensors
        let handler = Arc::new(DummyHandler);
        Self {
            label,
            kind: EndpointKind::TemperatureSensor,
            handler,
            temperature_sensor: Some(sensor),
            humidity_sensor: None,
        }
    }

    /// Create a humidity sensor endpoint (RelativeHumidityMeasurement cluster).
    ///
    /// Used for humidity sensors that report relative humidity.
    /// The sensor Arc can be cloned and used to update the humidity from external sources.
    pub fn humidity_sensor(label: &'static str, sensor: Arc<HumiditySensor>) -> Self {
        // Create a dummy handler - not used for humidity sensors
        let handler = Arc::new(DummyHandler);
        Self {
            label,
            kind: EndpointKind::HumiditySensor,
            handler,
            temperature_sensor: None,
            humidity_sensor: Some(sensor),
        }
    }
}

/// Dummy handler for endpoints that don't use the EndpointHandler interface.
struct DummyHandler;

impl EndpointHandler for DummyHandler {
    fn on_command(&self, _value: bool) {}
    fn get_state(&self) -> bool {
        false
    }
    fn set_state_pusher(&self, _pusher: Arc<dyn Fn(bool) + Send + Sync>) {}
}

/// A Virtual Device (parent endpoint) with one or more child Endpoints.
///
/// Virtual Devices are bridged devices that appear under the Aggregator endpoint.
/// Each Virtual Device has:
/// - A device type (ContactSensor, Light, etc.)
/// - A label (displayed in controllers)
/// - One or more child endpoints with functional clusters
///
/// # Example
/// ```ignore
/// let power_strip = VirtualDevice::new(VirtualDeviceType::OnOffPlugInUnit, "Power Strip")
///     .with_endpoint(EndpointConfig::switch("Outlet 1", outlet1_handler))
///     .with_endpoint(EndpointConfig::switch("Outlet 2", outlet2_handler));
/// ```
pub struct VirtualDevice {
    /// Device type for the parent endpoint
    pub device_type: VirtualDeviceType,
    /// Label displayed in Matter controllers
    pub label: &'static str,
    /// Child endpoints with functional clusters
    pub endpoints: Vec<EndpointConfig>,
}

impl VirtualDevice {
    /// Create a new Virtual Device with the given type and label.
    ///
    /// Use `with_endpoint` to add child endpoints.
    pub fn new(device_type: VirtualDeviceType, label: &'static str) -> Self {
        Self {
            device_type,
            label,
            endpoints: Vec::new(),
        }
    }

    /// Add a child endpoint to this Virtual Device.
    ///
    /// Returns self for method chaining.
    pub fn with_endpoint(mut self, endpoint: EndpointConfig) -> Self {
        self.endpoints.push(endpoint);
        self
    }

    /// Compute a hash of this device's structure for schema versioning.
    ///
    /// The hash includes device type, label, and all endpoint kinds/labels.
    /// This is used to detect when the device structure changes and
    /// persistence needs to be reset.
    pub fn schema_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.device_type.hash(&mut hasher);
        self.label.hash(&mut hasher);
        self.endpoints.len().hash(&mut hasher);
        for endpoint in &self.endpoints {
            endpoint.kind.hash(&mut hasher);
            endpoint.label.hash(&mut hasher);
        }
        hasher.finish()
    }
}

/// Compute a combined schema hash for all virtual devices.
///
/// This creates a deterministic hash of the entire device configuration,
/// used to detect when any device structure changes between runs.
pub fn compute_schema_hash(devices: &[VirtualDevice]) -> u64 {
    let mut hasher = DefaultHasher::new();
    devices.len().hash(&mut hasher);
    for device in devices {
        device.schema_hash().hash(&mut hasher);
    }
    hasher.finish()
}
