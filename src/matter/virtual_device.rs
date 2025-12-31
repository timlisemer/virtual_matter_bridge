//! Virtual Device configuration for dynamic Matter endpoint creation.
//!
//! A Virtual Device represents a parent endpoint with one or more child Endpoints.
//! This module provides the configuration types needed to define devices at startup.

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
    /// Handler for bidirectional communication with business logic
    pub handler: Arc<dyn EndpointHandler>,
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
        }
    }
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
