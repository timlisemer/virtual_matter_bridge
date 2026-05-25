//! On/Off switch for Matter OnOff cluster.
//!
//! Implements the `OnOffHooks` trait from rs-matter using `BinarySwitchHelper`
//! for state management. Can be used for any on/off switch endpoint.

use super::device_switch::DeviceSwitch;
use super::helpers::BinarySwitchHelper;
use parking_lot::RwLock;
use rs_matter::dm::Cluster;
use rs_matter::dm::clusters::app::on_off as on_off_cluster;
use rs_matter::dm::clusters::app::on_off::{EffectVariantEnum, OnOffHooks, StartUpOnOffEnum};
use rs_matter::error::Error;
use rs_matter::tlv::Nullable;
use rs_matter::with;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// On/Off switch implementing Matter's OnOffHooks trait.
///
/// Uses `BinarySwitchHelper` for thread-safe state management with
/// support for live Matter subscription updates.
///
/// When used as a master switch, can cascade OFF commands to all
/// registered device switches (parent endpoints).
pub struct Switch {
    /// The underlying switch state
    helper: BinarySwitchHelper,
    /// Startup behavior configuration (encoded as Option discriminant + value)
    /// 0 = None, 1 = Off, 2 = On, 3 = Toggle
    start_up_on_off: AtomicU8,
    /// Device switches to cascade to when this switch turns OFF
    cascade_targets: RwLock<Vec<Arc<DeviceSwitch>>>,
}

impl Switch {
    /// Create a new switch with the given initial state.
    pub fn new(initial: bool) -> Self {
        Self {
            helper: BinarySwitchHelper::new(initial),
            start_up_on_off: AtomicU8::new(0), // None
            cascade_targets: RwLock::new(Vec::new()),
        }
    }

    /// Get the underlying helper for external state access.
    pub fn helper(&self) -> &BinarySwitchHelper {
        &self.helper
    }

    /// Get the current switch state.
    pub fn get(&self) -> bool {
        self.helper.get()
    }

    /// Set the switch state.
    pub fn set(&self, value: bool) {
        self.helper.set(value);
    }

    /// Toggle the switch state and return the new value.
    pub fn toggle(&self) -> bool {
        self.helper.toggle()
    }

    /// Add a device switch that should be cascaded when this switch turns OFF.
    /// Used to implement virtual_bridge_onoff → parent DeviceSwitch cascade.
    pub fn add_cascade_target(&self, target: Arc<DeviceSwitch>) {
        self.cascade_targets.write().push(target);
    }

    /// Cascade OFF command to all registered device switches.
    fn cascade_off(&self) {
        for target in self.cascade_targets.read().iter() {
            target.set_from_master(false);
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

impl Default for Switch {
    fn default() -> Self {
        Self::new(true) // On by default
    }
}

impl OnOffHooks for Switch {
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
            "[Matter] OnOff cluster: switch {}",
            if on { "on" } else { "off" }
        );
        self.helper.set(on);

        // Cascade OFF to all registered device switches (virtual_bridge_onoff behavior)
        if !on {
            self.cascade_off();
        }
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
