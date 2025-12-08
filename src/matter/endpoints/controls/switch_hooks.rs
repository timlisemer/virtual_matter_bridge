//! OnOff hooks implementation for addable switches.
//!
//! This module provides the `SwitchHooks` struct that implements the `OnOffHooks` trait
//! from rs-matter. Uses `SwitchHelper` for state management, allowing switches to be
//! added dynamically like sensors.
//!
//! Uses atomics for thread-safe access since the hooks are shared between the main application
//! and the Matter stack thread.

use super::helpers::SwitchHelper;
use rs_matter::dm::Cluster;
use rs_matter::dm::clusters::decl::on_off as on_off_cluster;
use rs_matter::dm::clusters::on_off::{EffectVariantEnum, OnOffHooks, StartUpOnOffEnum};
use rs_matter::error::Error;
use rs_matter::tlv::Nullable;
use rs_matter::with;
use std::sync::atomic::{AtomicU8, Ordering};

/// OnOff hooks for addable switches.
///
/// Wraps a `SwitchHelper` to implement the `OnOffHooks` trait from rs-matter.
/// Can be used to add multiple switches to a Matter device.
pub struct SwitchHooks {
    /// The underlying switch state
    switch: SwitchHelper,
    /// Startup behavior configuration (encoded as Option discriminant + value)
    /// 0 = None, 1 = Off, 2 = On, 3 = Toggle
    start_up_on_off: AtomicU8,
}

impl SwitchHooks {
    /// Create a new SwitchHooks with the given initial state.
    pub fn new(initial: bool) -> Self {
        Self {
            switch: SwitchHelper::new(initial),
            start_up_on_off: AtomicU8::new(0), // None
        }
    }

    /// Get the underlying switch helper for external state access.
    pub fn switch(&self) -> &SwitchHelper {
        &self.switch
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

impl Default for SwitchHooks {
    fn default() -> Self {
        Self::new(true) // On by default
    }
}

impl OnOffHooks for SwitchHooks {
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
        self.switch.get()
    }

    fn set_on_off(&self, on: bool) {
        log::info!(
            "[Matter] OnOff cluster: switch {}",
            if on { "on" } else { "off" }
        );
        self.switch.set(on);
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
