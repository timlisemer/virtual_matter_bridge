//! Manufacturer-specific Shelly diagnostics cluster handler.
//!
//! This read-only cluster exposes MQTT diagnostics that do not have direct
//! standard Matter attributes in the bridge today.

use super::sync_dataver_with_sensor;
use crate::matter::endpoints::endpoints_helpers::{
    EndpointChangeTracker, Sensor, SourceReadiness, SourceSnapshot,
};
use crate::matter::endpoints::{ClusterNotifier, NotifiableSensor};
use parking_lot::RwLock;
use rs_matter::dm::{
    Access, Attribute, Cluster, Dataver, Handler, MatchContext, NonBlockingHandler, Quality,
    ReadContext, ReadReply, Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::TLVWrite;
use rs_matter::{attribute_enum, attributes, with};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
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
    values: RwLock<ShellyDiagnosticsValues>,
    changes: EndpointChangeTracker,
    readiness: Arc<SourceSnapshot<()>>,
}

impl ShellyDiagnosticsState {
    pub fn new() -> Self {
        Self {
            values: RwLock::new(ShellyDiagnosticsValues::default()),
            changes: EndpointChangeTracker::new(),
            readiness: Arc::new(SourceSnapshot::new()),
        }
    }

    pub fn set_values(&self, values: ShellyDiagnosticsValues) {
        let was_ready = self.readiness.is_ready();
        let mut guard = self.values.write();
        if *guard != values || !was_ready {
            *guard = values;
            self.changes.mark_changed();
        }
        drop(guard);
        self.readiness.mark_ready();
    }

    pub fn values(&self) -> ShellyDiagnosticsValues {
        self.values.read().clone()
    }

    pub fn readiness(&self) -> Arc<dyn SourceReadiness> {
        self.readiness.clone()
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
        self.changes.version()
    }
}

impl NotifiableSensor for ShellyDiagnosticsState {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.changes.set_notifier(notifier);
    }
}

pub struct ShellyDiagnosticsHandler {
    dataver: Dataver,
    state: Arc<ShellyDiagnosticsState>,
    last_state_version: AtomicU32,
}

impl ShellyDiagnosticsHandler {
    pub const CLUSTER: Cluster<'static> = CLUSTER;

    pub fn new(dataver: Dataver, state: Arc<ShellyDiagnosticsState>) -> Self {
        Self {
            dataver,
            state,
            last_state_version: AtomicU32::new(0),
        }
    }

    fn read_impl(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        sync_dataver_with_sensor(&*self.state, &self.last_state_version, &self.dataver);

        let attr = ctx.attr();
        let Some(mut writer) = reply.with_dataver(self.dataver.get())? else {
            return Ok(());
        };

        if attr.is_system() {
            return CLUSTER.read(attr, writer);
        }

        let tag = writer.tag();
        let values = self.state.values();
        {
            let mut tw = writer.writer();
            match attr.attr_id.try_into()? {
                ShellyDiagnosticsAttribute::DhcpEnabled => {
                    if let Some(value) = values.dhcp_enabled {
                        tw.bool(tag, value)?;
                    } else {
                        tw.null(tag)?;
                    }
                }
                ShellyDiagnosticsAttribute::IpAddress => {
                    if let Some(value) = values.ip_address.as_deref() {
                        tw.utf8(tag, value)?;
                    } else {
                        tw.null(tag)?;
                    }
                }
                ShellyDiagnosticsAttribute::Linkquality => {
                    if let Some(value) = values.linkquality {
                        tw.u16(tag, value)?;
                    } else {
                        tw.null(tag)?;
                    }
                }
                ShellyDiagnosticsAttribute::WifiConfigEnabled => {
                    if let Some(value) = values.wifi_config_enabled {
                        tw.bool(tag, value)?;
                    } else {
                        tw.null(tag)?;
                    }
                }
                ShellyDiagnosticsAttribute::WifiConfigSsid => {
                    if let Some(value) = values.wifi_config_ssid.as_deref() {
                        tw.utf8(tag, value)?;
                    } else {
                        tw.null(tag)?;
                    }
                }
                ShellyDiagnosticsAttribute::WifiStatus => {
                    if let Some(value) = values.wifi_status.as_deref() {
                        tw.utf8(tag, value)?;
                    } else {
                        tw.null(tag)?;
                    }
                }
            }
        }

        writer.complete()
    }

    fn write_impl(&self, _ctx: impl WriteContext) -> Result<(), Error> {
        Err(ErrorCode::UnsupportedAccess.into())
    }
}

impl Handler for ShellyDiagnosticsHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.read_impl(ctx, reply)
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        self.write_impl(ctx)
    }

    fn bump_dataver(&self, _ctx: impl MatchContext) {
        self.dataver.changed();
        self.last_state_version
            .store(self.state.version(), Ordering::SeqCst);
    }
}

impl NonBlockingHandler for ShellyDiagnosticsHandler {}

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
