//! Light switch for Matter OnOff Light device type.
//!
//! Implements the `OnOffHooks` trait from rs-matter using `BinarySwitchHelper`
//! for state management. Used for Matter On/Off Light endpoints.

use super::helpers::BinarySwitchHelper;
use rs_matter::dm::Cluster;
use rs_matter::dm::clusters::decl::on_off as on_off_cluster;
use rs_matter::dm::clusters::on_off::{EffectVariantEnum, OnOffHooks, StartUpOnOffEnum};
use rs_matter::error::Error;
use rs_matter::tlv::Nullable;
use rs_matter::with;
use std::sync::atomic::{AtomicU8, Ordering};

/// Light switch implementing Matter's OnOffHooks trait.
///
/// Uses `BinarySwitchHelper` for thread-safe state management with
/// support for live Matter subscription updates. Uses the On/Off Light
/// device type (0x0100) which appears as a light in controllers.
pub struct LightSwitch {
    /// The underlying switch state
    helper: BinarySwitchHelper,
    /// Startup behavior configuration (encoded as Option discriminant + value)
    /// 0 = None, 1 = Off, 2 = On, 3 = Toggle
    start_up_on_off: AtomicU8,
}

impl LightSwitch {
    /// Create a new light switch with the given initial state.
    pub fn new(initial: bool) -> Self {
        Self {
            helper: BinarySwitchHelper::new(initial),
            start_up_on_off: AtomicU8::new(0), // None
        }
    }

    /// Get the underlying helper for external state access.
    pub fn helper(&self) -> &BinarySwitchHelper {
        &self.helper
    }

    /// Get the current light state.
    pub fn get(&self) -> bool {
        self.helper.get()
    }

    /// Set the light state.
    pub fn set(&self, value: bool) {
        self.helper.set(value);
    }

    /// Toggle the light state and return the new value.
    pub fn toggle(&self) -> bool {
        self.helper.toggle()
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

impl Default for LightSwitch {
    fn default() -> Self {
        Self::new(false) // Off by default for lights
    }
}

impl OnOffHooks for LightSwitch {
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
            "[Matter] OnOff cluster: light {}",
            if on { "on" } else { "off" }
        );
        self.helper.set(on);
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
