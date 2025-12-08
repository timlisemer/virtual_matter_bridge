//! OnOff hooks implementation for the video doorbell.
//!
//! This module provides the `DoorbellOnOffHooks` struct that implements the `OnOffHooks` trait
//! from rs-matter. The OnOff state represents whether the doorbell is armed (will notify on press)
//! or disarmed.
//!
//! Uses atomics for thread-safe access since the hooks are shared between the main application
//! and the Matter stack thread.

use rs_matter::dm::Cluster;
use rs_matter::dm::clusters::decl::on_off as on_off_cluster;
use rs_matter::dm::clusters::on_off::{EffectVariantEnum, OnOffHooks, StartUpOnOffEnum};
use rs_matter::error::Error;
use rs_matter::tlv::Nullable;
use rs_matter::with;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// OnOff hooks for the video doorbell's armed/disarmed state.
///
/// When `on_off` is `true`, the doorbell is armed and will send notifications
/// when the doorbell button is pressed.
/// When `on_off` is `false`, the doorbell is disarmed and will not notify.
///
/// Uses atomic types for thread-safe access between the application and Matter stack.
pub struct DoorbellOnOffHooks {
    /// Current armed state (true = armed, false = disarmed)
    on_off: AtomicBool,
    /// Startup behavior configuration (encoded as Option discriminant + value)
    /// 0 = None, 1 = Off, 2 = On, 3 = Toggle
    start_up_on_off: AtomicU8,
}

// SAFETY: All fields use atomic types, making this safe to share across threads
unsafe impl Sync for DoorbellOnOffHooks {}

impl DoorbellOnOffHooks {
    /// Create a new DoorbellOnOffHooks with the doorbell armed by default.
    pub fn new() -> Self {
        Self {
            on_off: AtomicBool::new(true),     // Armed by default
            start_up_on_off: AtomicU8::new(0), // None
        }
    }

    /// Check if the doorbell is currently armed.
    pub fn is_armed(&self) -> bool {
        self.on_off.load(Ordering::SeqCst)
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

impl Default for DoorbellOnOffHooks {
    fn default() -> Self {
        Self::new()
    }
}

impl OnOffHooks for DoorbellOnOffHooks {
    /// Cluster definition with basic OnOff functionality.
    /// We don't need the LIGHTING feature since this is just an armed/disarmed toggle.
    const CLUSTER: Cluster<'static> = on_off_cluster::FULL_CLUSTER
        .with_revision(6)
        .with_attrs(with!(required; on_off_cluster::AttributeId::OnOff))
        .with_cmds(with!(
            on_off_cluster::CommandId::Off
                | on_off_cluster::CommandId::On
                | on_off_cluster::CommandId::Toggle
        ));

    fn on_off(&self) -> bool {
        self.on_off.load(Ordering::SeqCst)
    }

    fn set_on_off(&self, on: bool) {
        log::info!(
            "[Matter] OnOff cluster: doorbell {}",
            if on { "armed" } else { "disarmed" }
        );
        self.on_off.store(on, Ordering::SeqCst);
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
        // No special effect handling for doorbell armed state
    }
}
