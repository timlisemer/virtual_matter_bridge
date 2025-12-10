//! Device-level OnOff switch for parent Matter endpoints.
//!
//! Implements the `OnOffHooks` trait from rs-matter using `BinarySwitchHelper`
//! for state management. When turned OFF, cascades to all child endpoints
//! (turns off their switches and marks them unreachable).

use super::helpers::BinarySwitchHelper;
use crate::matter::handler_bridge::SwitchBridge;
use parking_lot::RwLock;
use rs_matter::dm::Cluster;
use rs_matter::dm::clusters::decl::on_off as on_off_cluster;
use rs_matter::dm::clusters::on_off::{EffectVariantEnum, OnOffHooks, StartUpOnOffEnum};
use rs_matter::error::Error;
use rs_matter::tlv::Nullable;
use rs_matter::with;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Device-level OnOff switch for parent endpoints.
///
/// When this switch is turned OFF:
/// 1. All child OnOff switches are turned OFF
/// 2. All children are marked as `Reachable=false` in BridgedDeviceBasicInfo
///
/// When turned ON:
/// 1. All children are marked as `Reachable=true`
/// 2. Children can now be individually controlled
pub struct DeviceSwitch {
    /// The underlying switch state
    helper: BinarySwitchHelper,
    /// Startup behavior configuration (encoded as Option discriminant + value)
    /// 0 = None, 1 = Off, 2 = On, 3 = Toggle
    start_up_on_off: AtomicU8,
    /// Child switch bridges to cascade ON/OFF commands to
    child_switches: RwLock<Vec<Arc<SwitchBridge>>>,
    /// Child reachable flags to update when this device is turned on/off
    child_reachable: RwLock<Vec<Arc<AtomicBool>>>,
}

impl DeviceSwitch {
    /// Create a new device switch with the given initial state.
    pub fn new(initial: bool) -> Self {
        Self {
            helper: BinarySwitchHelper::new(initial),
            start_up_on_off: AtomicU8::new(0), // None
            child_switches: RwLock::new(Vec::new()),
            child_reachable: RwLock::new(Vec::new()),
        }
    }

    /// Get the underlying helper for external state access.
    pub fn helper(&self) -> &BinarySwitchHelper {
        &self.helper
    }

    /// Get the current device state.
    pub fn get(&self) -> bool {
        self.helper.get()
    }

    /// Add a child switch that will be controlled when this device turns on/off.
    pub fn add_child_switch(&self, switch: Arc<SwitchBridge>) {
        self.child_switches.write().push(switch);
    }

    /// Add a child reachable flag that will be updated when this device turns on/off.
    pub fn add_child_reachable(&self, reachable: Arc<AtomicBool>) {
        self.child_reachable.write().push(reachable);
    }

    /// Set the device state with cascade to children.
    fn set_with_cascade(&self, on: bool) {
        let old = self.helper.get();
        self.helper.set(on);

        if on != old {
            if !on {
                // Turning OFF: cascade OFF to all children and mark unreachable
                for switch in self.child_switches.read().iter() {
                    switch.set(false);
                }
                for reachable in self.child_reachable.read().iter() {
                    reachable.store(false, Ordering::SeqCst);
                }
            } else {
                // Turning ON: mark all children reachable (but don't change their state)
                for reachable in self.child_reachable.read().iter() {
                    reachable.store(true, Ordering::SeqCst);
                }
            }
        }
    }

    /// Called by virtual_bridge_onoff when it turns OFF - forces this device OFF.
    pub fn set_from_master(&self, on: bool) {
        if !on {
            self.set_with_cascade(false);
        }
    }

    /// Encode StartUpOnOffEnum to u8
    fn encode_start_up(value: Option<StartUpOnOffEnum>) -> u8 {
        match value {
            None => 0,
            Some(StartUpOnOffEnum::Off) => 1,
            Some(StartUpOnOffEnum::On) => 2,
            Some(StartUpOnOffEnum::Toggle) => 3,
        }
    }

    /// Decode u8 to Option<StartUpOnOffEnum>
    fn decode_start_up(value: u8) -> Option<StartUpOnOffEnum> {
        match value {
            0 => None,
            1 => Some(StartUpOnOffEnum::Off),
            2 => Some(StartUpOnOffEnum::On),
            3 => Some(StartUpOnOffEnum::Toggle),
            _ => None, // Invalid value, treat as None
        }
    }
}

impl Default for DeviceSwitch {
    fn default() -> Self {
        Self::new(true) // On by default
    }
}

impl OnOffHooks for DeviceSwitch {
    /// Cluster definition with basic OnOff functionality.
    const CLUSTER: Cluster<'static> = on_off_cluster::FULL_CLUSTER
        .with_revision(6)
        .with_attrs(with!(required; on_off_cluster::AttributeId::OnOff))
        .with_cmds(with!(
            on_off_cluster::CommandId::Off
                | on_off_cluster::CommandId::On
                | on_off_cluster::CommandId::Toggle
        ));

    fn on_off(&self) -> bool {
        self.helper.get()
    }

    fn set_on_off(&self, on: bool) {
        log::info!(
            "[Matter] DeviceSwitch: device {}",
            if on { "on" } else { "off" }
        );
        self.set_with_cascade(on);
    }

    fn start_up_on_off(&self) -> Nullable<StartUpOnOffEnum> {
        match Self::decode_start_up(self.start_up_on_off.load(Ordering::SeqCst)) {
            Some(value) => Nullable::some(value),
            None => Nullable::none(),
        }
    }

    fn set_start_up_on_off(&self, value: Nullable<StartUpOnOffEnum>) -> Result<(), Error> {
        self.start_up_on_off
            .store(Self::encode_start_up(value.into_option()), Ordering::SeqCst);
        Ok(())
    }

    async fn handle_off_with_effect(&self, _effect: EffectVariantEnum) {
        // No special effect handling
    }
}
