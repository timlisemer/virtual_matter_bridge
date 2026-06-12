//! Manufacturer-specific Shelly diagnostics cluster handler.
//!
//! This read-only cluster exposes MQTT diagnostics that do not have direct
//! standard Matter attributes in the bridge today.

use super::read_only_cluster::define_versioned_read_only_cluster_handler;
use crate::matter::clusters::tlv_helpers::write_nullable;
use crate::matter::endpoints::endpoints_helpers::{Sensor, SourceReadiness, TrackedEndpointState};
use crate::matter::endpoints::{ClusterNotifier, NotifiableSensor};
use rs_matter::dm::{Access, Attribute, Cluster, Quality};
use rs_matter::error::Error;
use rs_matter::tlv::{TLVTag, TLVWrite};
use rs_matter::{attribute_enum, attributes, with};
use std::sync::Arc;
use strum::FromRepr;

pub const CLUSTER_ID: u32 = 0xFC00;
pub const CLUSTER_REVISION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum ShellyDiagnosticsAttribute {
    DhcpEnabled = 0x0000,
    IpAddress = 0x0001,
    Linkquality = 0x0002,
    WifiConfigEnabled = 0x0003,
    WifiConfigSsid = 0x0004,
    WifiStatus = 0x0005,
}

attribute_enum!(ShellyDiagnosticsAttribute);

pub const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    feature_map: 0,
    attributes: attributes!(
        Attribute::new(
            ShellyDiagnosticsAttribute::DhcpEnabled as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            ShellyDiagnosticsAttribute::IpAddress as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            ShellyDiagnosticsAttribute::Linkquality as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            ShellyDiagnosticsAttribute::WifiConfigEnabled as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            ShellyDiagnosticsAttribute::WifiConfigSsid as _,
            Access::RV,
            Quality::NULLABLE
        ),
        Attribute::new(
            ShellyDiagnosticsAttribute::WifiStatus as _,
            Access::RV,
            Quality::NULLABLE
        ),
    ),
    commands: &[],
    events: &[],
    with_attrs: with!(all),
    with_cmds: with!(all),
    with_events: with!(all),
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellyDiagnosticsValues {
    pub dhcp_enabled: Option<bool>,
    pub ip_address: Option<String>,
    pub linkquality: Option<u16>,
    pub wifi_config_enabled: Option<bool>,
    pub wifi_config_ssid: Option<String>,
    pub wifi_status: Option<String>,
}

pub struct ShellyDiagnosticsState {
    values: TrackedEndpointState<ShellyDiagnosticsValues>,
}

impl ShellyDiagnosticsState {
    pub fn new() -> Self {
        Self {
            values: TrackedEndpointState::new(ShellyDiagnosticsValues::default()),
        }
    }

    pub fn set_values(&self, values: ShellyDiagnosticsValues) {
        self.values.set(values);
    }

    pub fn values(&self) -> ShellyDiagnosticsValues {
        self.values.get_cloned()
    }

    pub fn readiness(&self) -> Arc<dyn SourceReadiness> {
        self.values.readiness()
    }

    pub fn linkquality(&self) -> Option<u16> {
        self.values().linkquality
    }

    pub fn dhcp_enabled(&self) -> Option<bool> {
        self.values().dhcp_enabled
    }

    pub fn ip_address(&self) -> Option<String> {
        self.values().ip_address
    }
}

impl Default for ShellyDiagnosticsState {
    fn default() -> Self {
        Self::new()
    }
}

impl Sensor for ShellyDiagnosticsState {
    fn version(&self) -> u32 {
        self.values.version()
    }
}

impl NotifiableSensor for ShellyDiagnosticsState {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.values.set_notifier(notifier);
    }
}

define_versioned_read_only_cluster_handler!(
    ShellyDiagnosticsHandler,
    ShellyDiagnosticsState,
    ShellyDiagnosticsAttribute,
    CLUSTER,
    |sensor, tw, tag, attr| { write_diagnostics_attr(&mut tw, tag, sensor.values(), attr) }
);

fn write_diagnostics_attr(
    tw: &mut impl TLVWrite,
    tag: &TLVTag,
    values: ShellyDiagnosticsValues,
    attr: ShellyDiagnosticsAttribute,
) -> Result<(), Error> {
    match attr {
        ShellyDiagnosticsAttribute::DhcpEnabled => {
            write_nullable(tw, tag, values.dhcp_enabled, |tw, tag, value| {
                tw.bool(tag, value)
            })?;
        }
        ShellyDiagnosticsAttribute::IpAddress => {
            write_nullable(tw, tag, values.ip_address.as_deref(), |tw, tag, value| {
                tw.utf8(tag, value)
            })?;
        }
        ShellyDiagnosticsAttribute::Linkquality => {
            write_nullable(tw, tag, values.linkquality, |tw, tag, value| {
                tw.u16(tag, value)
            })?;
        }
        ShellyDiagnosticsAttribute::WifiConfigEnabled => {
            write_nullable(tw, tag, values.wifi_config_enabled, |tw, tag, value| {
                tw.bool(tag, value)
            })?;
        }
        ShellyDiagnosticsAttribute::WifiConfigSsid => {
            write_nullable(
                tw,
                tag,
                values.wifi_config_ssid.as_deref(),
                |tw, tag, value| tw.utf8(tag, value),
            )?;
        }
        ShellyDiagnosticsAttribute::WifiStatus => {
            write_nullable(tw, tag, values.wifi_status.as_deref(), |tw, tag, value| {
                tw.utf8(tag, value)
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_values_marks_first_update_and_tracks_diagnostics() {
        let state = ShellyDiagnosticsState::new();

        state.set_values(ShellyDiagnosticsValues {
            dhcp_enabled: Some(true),
            ip_address: Some("10.0.0.98".to_string()),
            linkquality: Some(148),
            wifi_config_enabled: Some(false),
            wifi_config_ssid: Some("TestWiFi".to_string()),
            wifi_status: Some("got ip".to_string()),
        });

        assert_eq!(state.version(), 1);
        assert_eq!(state.dhcp_enabled(), Some(true));
        assert_eq!(state.ip_address().as_deref(), Some("10.0.0.98"));
        assert_eq!(state.linkquality(), Some(148));
        assert_eq!(
            state.values(),
            ShellyDiagnosticsValues {
                dhcp_enabled: Some(true),
                ip_address: Some("10.0.0.98".to_string()),
                linkquality: Some(148),
                wifi_config_enabled: Some(false),
                wifi_config_ssid: Some("TestWiFi".to_string()),
                wifi_status: Some("got ip".to_string()),
            }
        );
    }

    #[test]
    fn repeated_equivalent_values_do_not_bump_version_after_ready() {
        let state = ShellyDiagnosticsState::new();
        let values = ShellyDiagnosticsValues {
            dhcp_enabled: Some(true),
            ip_address: Some("10.0.0.98".to_string()),
            linkquality: Some(148),
            wifi_config_enabled: Some(false),
            wifi_config_ssid: Some("TestWiFi".to_string()),
            wifi_status: Some("got ip".to_string()),
        };

        state.set_values(values.clone());
        state.set_values(values);

        assert_eq!(state.version(), 1);
    }
}
