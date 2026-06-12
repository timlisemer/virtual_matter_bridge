//! Matter cluster handlers for the Matter bridge.
//!
//! This module provides handlers that bridge the existing cluster business logic
//! to rs-matter's data model traits.
//!
//! Note: We implement ClusterHandler traits manually rather than using the import! macro
//! because the provisional camera clusters have path resolution issues when used from
//! outside the rs-matter crate.

pub mod boolean_state;
pub mod bridged_device_basic_info;
pub mod camera_av_stream_mgmt;
pub mod electrical_energy_measurement;
pub mod electrical_power_measurement;
pub mod generic_switch;
pub mod icd_management;
pub mod occupancy_sensing;
pub mod read_only_cluster;
pub mod relative_humidity;
pub mod scalar_measurement;
pub mod shelly_diagnostics;
pub mod temperature_measurement;
mod tlv_helpers;
mod versioned_state;
pub mod webrtc_transport_provider;

// Re-export for convenience
pub use boolean_state::BooleanStateHandler;
pub use bridged_device_basic_info::{BridgedDeviceInfo, BridgedHandler};
pub use electrical_energy_measurement::{
    ElectricalEnergyMeasurementHandler, ElectricalEnergyState, ElectricalEnergyValues,
};
pub use electrical_power_measurement::{
    ElectricalPowerMeasurementHandler, ElectricalPowerState, ElectricalPowerValues,
};
pub use generic_switch::{GenericSwitchHandler, GenericSwitchState};
pub use icd_management::IcdManagementHandler;
pub use occupancy_sensing::OccupancySensingHandler;
pub use relative_humidity::{HumiditySensor, RelativeHumidityHandler};
pub use shelly_diagnostics::{
    ShellyDiagnosticsHandler, ShellyDiagnosticsState, ShellyDiagnosticsValues,
};
pub use temperature_measurement::{TemperatureMeasurementHandler, TemperatureSensor};
// TODO: Re-export when handlers are wired in stack.rs
// pub use camera_av_stream_mgmt::CameraAvStreamMgmtHandler;
// pub use webrtc_transport_provider::WebRtcTransportProviderHandler;
