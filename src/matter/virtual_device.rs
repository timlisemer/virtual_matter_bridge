//! Virtual Device configuration for dynamic Matter endpoint creation.
//!
//! A Virtual Device represents a parent endpoint with one or more child Endpoints.
//! This module provides the configuration types needed to define devices at startup.

use super::clusters::{
    BridgedDeviceInfo, ElectricalEnergyState, ElectricalPowerState, GenericSwitchState,
    HumiditySensor, ShellyDiagnosticsState, TemperatureSensor,
};
use super::endpoints::EndpointHandler;
use super::endpoints::endpoints_helpers::{ReadinessOnlyHandler, SourceReadiness};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

const MATTER_TOPOLOGY_MODEL_VERSION: u64 = 3;

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
    /// Generic switch using GenericSwitch cluster (0x003B) - for buttons
    GenericSwitch,
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
    /// Optional generic switch state (for GenericSwitch endpoints)
    pub generic_switch_state: Option<Arc<GenericSwitchState>>,
    /// Optional electrical power telemetry attached to switch/light endpoints
    pub electrical_power: Option<Arc<ElectricalPowerState>>,
    /// Optional electrical energy telemetry attached to switch/light endpoints
    pub electrical_energy: Option<Arc<ElectricalEnergyState>>,
    /// Optional Shelly diagnostics attached to switch/light endpoints
    pub shelly_diagnostics: Option<Arc<ShellyDiagnosticsState>>,
}

impl EndpointConfig {
    pub fn readiness(&self) -> Arc<dyn SourceReadiness> {
        match self.kind {
            EndpointKind::TemperatureSensor => self
                .temperature_sensor
                .as_ref()
                .map(|sensor| sensor.readiness())
                .unwrap_or_else(|| self.handler.readiness()),
            EndpointKind::HumiditySensor => self
                .humidity_sensor
                .as_ref()
                .map(|sensor| sensor.readiness())
                .unwrap_or_else(|| self.handler.readiness()),
            EndpointKind::GenericSwitch => self
                .generic_switch_state
                .as_ref()
                .map(|state| state.readiness())
                .unwrap_or_else(|| self.handler.readiness()),
            _ => self.handler.readiness(),
        }
    }

    fn without_optional_telemetry(
        label: &'static str,
        kind: EndpointKind,
        handler: Arc<dyn EndpointHandler>,
    ) -> Self {
        Self {
            label,
            kind,
            handler,
            temperature_sensor: None,
            humidity_sensor: None,
            generic_switch_state: None,
            electrical_power: None,
            electrical_energy: None,
            shelly_diagnostics: None,
        }
    }

    /// Create a contact sensor endpoint (BooleanState cluster).
    ///
    /// Used for door/window sensors that report open/closed state.
    pub fn contact_sensor(label: &'static str, handler: Arc<dyn EndpointHandler>) -> Self {
        Self::without_optional_telemetry(label, EndpointKind::ContactSensor, handler)
    }

    /// Create an occupancy sensor endpoint (OccupancySensing cluster).
    ///
    /// Used for motion/presence sensors.
    pub fn occupancy_sensor(label: &'static str, handler: Arc<dyn EndpointHandler>) -> Self {
        Self::without_optional_telemetry(label, EndpointKind::OccupancySensor, handler)
    }

    /// Create a switch endpoint (OnOff cluster, plug-in unit appearance).
    ///
    /// Used for power outlets, relays, or generic switches.
    pub fn switch(label: &'static str, handler: Arc<dyn EndpointHandler>) -> Self {
        Self::without_optional_telemetry(label, EndpointKind::Switch, handler)
    }

    /// Create a light switch endpoint (OnOff cluster, light appearance).
    ///
    /// Used for lights - appears as a light in controllers.
    pub fn light_switch(label: &'static str, handler: Arc<dyn EndpointHandler>) -> Self {
        Self::without_optional_telemetry(label, EndpointKind::LightSwitch, handler)
    }

    /// Create a video doorbell camera endpoint (CameraAvStreamMgmt + WebRtcTransportProvider clusters).
    ///
    /// Used for video doorbells and cameras with streaming capability.
    pub fn video_doorbell_camera(label: &'static str, handler: Arc<dyn EndpointHandler>) -> Self {
        Self::without_optional_telemetry(label, EndpointKind::VideoDoorbellCamera, handler)
    }

    /// Create a temperature sensor endpoint (TemperatureMeasurement cluster).
    ///
    /// Used for temperature sensors that report temperature values.
    /// The sensor Arc can be cloned and used to update the temperature from external sources.
    pub fn temperature_sensor(label: &'static str, sensor: Arc<TemperatureSensor>) -> Self {
        let handler = Arc::new(ReadinessOnlyHandler::always_ready());
        Self {
            label,
            kind: EndpointKind::TemperatureSensor,
            handler,
            temperature_sensor: Some(sensor),
            humidity_sensor: None,
            generic_switch_state: None,
            electrical_power: None,
            electrical_energy: None,
            shelly_diagnostics: None,
        }
    }

    /// Create a humidity sensor endpoint (RelativeHumidityMeasurement cluster).
    ///
    /// Used for humidity sensors that report relative humidity.
    /// The sensor Arc can be cloned and used to update the humidity from external sources.
    pub fn humidity_sensor(label: &'static str, sensor: Arc<HumiditySensor>) -> Self {
        let handler = Arc::new(ReadinessOnlyHandler::always_ready());
        Self {
            label,
            kind: EndpointKind::HumiditySensor,
            handler,
            temperature_sensor: None,
            humidity_sensor: Some(sensor),
            generic_switch_state: None,
            electrical_power: None,
            electrical_energy: None,
            shelly_diagnostics: None,
        }
    }

    /// Create a generic switch endpoint (GenericSwitch cluster).
    ///
    /// Used for physical buttons that emit press/release events.
    /// The state Arc can be cloned and used to trigger button events from external sources.
    pub fn generic_switch(label: &'static str, state: Arc<GenericSwitchState>) -> Self {
        let handler = Arc::new(ReadinessOnlyHandler::always_ready());
        Self {
            label,
            kind: EndpointKind::GenericSwitch,
            handler,
            temperature_sensor: None,
            humidity_sensor: None,
            generic_switch_state: Some(state),
            electrical_power: None,
            electrical_energy: None,
            shelly_diagnostics: None,
        }
    }

    pub fn with_electrical_power(mut self, state: Arc<ElectricalPowerState>) -> Self {
        self.electrical_power = Some(state);
        self
    }

    pub fn with_electrical_energy(mut self, state: Arc<ElectricalEnergyState>) -> Self {
        self.electrical_energy = Some(state);
        self
    }

    pub fn with_shelly_diagnostics(mut self, state: Arc<ShellyDiagnosticsState>) -> Self {
        self.shelly_diagnostics = Some(state);
        self
    }
}

/// A Virtual Device (parent endpoint) with one or more child Endpoints.
///
/// Virtual Devices are bridged devices that appear under the Aggregator endpoint.
/// Each Virtual Device has:
/// - A label (displayed in controllers)
/// - One or more child endpoints with functional clusters
///
/// # Example
/// ```ignore
/// let power_strip = VirtualDevice::new("Power Strip")
///     .with_endpoint(EndpointConfig::switch("Outlet 1", outlet1_handler))
///     .with_endpoint(EndpointConfig::switch("Outlet 2", outlet2_handler));
/// ```
pub struct VirtualDevice {
    /// Label displayed in Matter controllers
    pub label: &'static str,
    /// Child endpoints with functional clusters
    pub endpoints: Vec<EndpointConfig>,
    /// Optional device info (vendor, product, serial, etc.)
    pub device_info: Option<BridgedDeviceInfo>,
}

impl VirtualDevice {
    /// Create a new Virtual Device with the given label.
    ///
    /// Use `with_endpoint` to add child endpoints.
    pub fn new(label: &'static str) -> Self {
        Self {
            label,
            endpoints: Vec::new(),
            device_info: None,
        }
    }

    /// Add a child endpoint to this Virtual Device.
    ///
    /// Returns self for method chaining.
    pub fn with_endpoint(mut self, endpoint: EndpointConfig) -> Self {
        self.endpoints.push(endpoint);
        self
    }

    /// Set device info (vendor, product, serial, etc.) for this Virtual Device.
    ///
    /// This information is exposed via the BridgedDeviceBasicInformation cluster
    /// and displayed in Matter controllers like Home Assistant.
    ///
    /// # Example
    /// ```ignore
    /// VirtualDevice::new("Büro Thermometer")
    ///     .with_device_info(
    ///         BridgedDeviceInfo::new("Büro Thermometer")
    ///             .with_vendor("Aqara")
    ///             .with_product("Climate Sensor W100")
    ///     )
    /// ```
    pub fn with_device_info(mut self, info: BridgedDeviceInfo) -> Self {
        self.device_info = Some(info);
        self
    }

    /// Compute a hash of this device's structure for schema versioning.
    ///
    /// The hash includes label and all endpoint kinds/labels.
    /// This is used to detect when the device structure changes and
    /// persistence needs to be reset.
    pub fn schema_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.label.hash(&mut hasher);
        self.endpoints.len().hash(&mut hasher);
        for endpoint in &self.endpoints {
            endpoint.kind.hash(&mut hasher);
            endpoint.label.hash(&mut hasher);
            endpoint.electrical_power.is_some().hash(&mut hasher);
            endpoint.electrical_energy.is_some().hash(&mut hasher);
            endpoint.shelly_diagnostics.is_some().hash(&mut hasher);
        }
        hasher.finish()
    }
}

pub fn collect_endpoint_readiness(
    virtual_devices: &[VirtualDevice],
) -> Vec<Arc<dyn SourceReadiness>> {
    virtual_devices
        .iter()
        .flat_map(|device| device.endpoints.iter().map(EndpointConfig::readiness))
        .collect()
}

pub fn mark_endpoint_readiness_unavailable(readiness: &[Arc<dyn SourceReadiness>]) -> usize {
    readiness
        .iter()
        .filter(|source| source.mark_unavailable())
        .count()
}

/// Compute a combined schema hash for all virtual devices.
///
/// This creates a deterministic hash of the entire device configuration,
/// used to detect when any device structure changes between runs.
pub fn compute_schema_hash(devices: &[VirtualDevice]) -> u64 {
    let mut hasher = DefaultHasher::new();
    MATTER_TOPOLOGY_MODEL_VERSION.hash(&mut hasher);
    devices.len().hash(&mut hasher);
    for device in devices {
        device.schema_hash().hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matter::endpoints::{EndpointHandler, SourceSnapshot};

    struct TestHandler {
        state: Arc<SourceSnapshot<bool>>,
    }

    impl TestHandler {
        fn new() -> Self {
            Self {
                state: Arc::new(SourceSnapshot::new()),
            }
        }

        fn set_state(&self, value: bool) {
            self.state.update_source(value);
        }
    }

    impl EndpointHandler for TestHandler {
        fn on_command(&self, value: bool) {
            self.set_state(value);
        }

        fn get_state(&self) -> Option<bool> {
            self.state.snapshot()
        }

        fn readiness(&self) -> Arc<dyn SourceReadiness> {
            self.state.clone()
        }

        fn set_state_pusher(&self, _pusher: Arc<dyn Fn(bool) + Send + Sync>) {}
    }

    #[test]
    fn endpoint_readiness_shutdown_marks_all_collected_endpoints_unavailable() {
        let switch = Arc::new(TestHandler::new());
        let temperature = Arc::new(TemperatureSensor::new(20.0));
        let button = Arc::new(GenericSwitchState::new());

        let devices = vec![
            VirtualDevice::new("Relay")
                .with_endpoint(EndpointConfig::switch("Switch", switch.clone())),
            VirtualDevice::new("Climate")
                .with_endpoint(EndpointConfig::temperature_sensor(
                    "Temperature",
                    temperature.clone(),
                ))
                .with_endpoint(EndpointConfig::generic_switch("Button", button.clone())),
        ];
        let readiness = collect_endpoint_readiness(&devices);

        switch.set_state(true);
        temperature.set_celsius(20.0);
        button.mark_ready();

        assert_eq!(readiness.len(), 3);
        assert!(readiness.iter().all(|source| source.is_ready()));
        assert_eq!(mark_endpoint_readiness_unavailable(&readiness), 3);
        assert!(readiness.iter().all(|source| !source.is_ready()));
        assert_eq!(mark_endpoint_readiness_unavailable(&readiness), 0);
    }
}
