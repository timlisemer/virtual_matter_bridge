//! Shared OnOff control behavior for switch-like endpoints.

use super::helpers::BinarySwitchHelper;
use rs_matter::dm::Cluster;
use rs_matter::dm::clusters::app::on_off as on_off_cluster;
use rs_matter::dm::clusters::app::on_off::{EffectVariantEnum, OnOffHooks, StartUpOnOffEnum};
use rs_matter::error::Error;
use rs_matter::tlv::Nullable;
use rs_matter::with;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU8, Ordering};

pub struct OnOffControl {
    helper: BinarySwitchHelper,
    start_up_on_off: AtomicU8,
}

impl OnOffControl {
    pub fn new(initial: bool) -> Self {
        Self {
            helper: BinarySwitchHelper::new(initial),
            start_up_on_off: AtomicU8::new(0),
        }
    }

    pub fn helper(&self) -> &BinarySwitchHelper {
        &self.helper
    }

    pub fn get(&self) -> bool {
        self.helper.get()
    }

    pub fn set(&self, value: bool) {
        self.helper.set(value);
    }

    pub fn toggle(&self) -> bool {
        self.helper.toggle()
    }

    pub fn start_up_on_off(&self) -> Nullable<StartUpOnOffEnum> {
        match Self::decode_start_up(self.start_up_on_off.load(Ordering::SeqCst)) {
            Some(value) => Nullable::some(value),
            None => Nullable::none(),
        }
    }

    pub fn set_start_up_on_off(&self, value: Nullable<StartUpOnOffEnum>) -> Result<(), Error> {
        self.start_up_on_off
            .store(Self::encode_start_up(value.into_option()), Ordering::SeqCst);
        Ok(())
    }

    fn encode_start_up(value: Option<StartUpOnOffEnum>) -> u8 {
        match value {
            None => 0,
            Some(StartUpOnOffEnum::Off) => 1,
            Some(StartUpOnOffEnum::On) => 2,
            Some(StartUpOnOffEnum::Toggle) => 3,
        }
    }

    fn decode_start_up(value: u8) -> Option<StartUpOnOffEnum> {
        match value {
            0 => None,
            1 => Some(StartUpOnOffEnum::Off),
            2 => Some(StartUpOnOffEnum::On),
            3 => Some(StartUpOnOffEnum::Toggle),
            _ => None,
        }
    }
}

/// Configuration for a shared on/off endpoint role.
pub trait OnOffEndpointConfig {
    const DEFAULT: bool;
    const LOG_NOUN: &'static str;
}

/// Reusable on/off endpoint implementation.
pub struct OnOffEndpoint<C> {
    control: OnOffControl,
    config: PhantomData<C>,
}

impl<C> OnOffEndpoint<C> {
    /// Create a new on/off endpoint with the given initial state.
    pub fn new(initial: bool) -> Self {
        Self {
            control: OnOffControl::new(initial),
            config: PhantomData,
        }
    }

    /// Get the underlying helper for external state access.
    pub fn helper(&self) -> &BinarySwitchHelper {
        self.control.helper()
    }

    /// Get the current switch state.
    pub fn get(&self) -> bool {
        self.control.get()
    }

    /// Set the switch state.
    pub fn set(&self, value: bool) {
        self.control.set(value);
    }

    /// Toggle the switch state and return the new value.
    pub fn toggle(&self) -> bool {
        self.control.toggle()
    }

    /// Get the configured startup state.
    pub fn start_up_on_off(&self) -> Nullable<StartUpOnOffEnum> {
        self.control.start_up_on_off()
    }

    /// Set the configured startup state.
    pub fn set_start_up_on_off(&self, value: Nullable<StartUpOnOffEnum>) -> Result<(), Error> {
        self.control.set_start_up_on_off(value)
    }
}

impl<C: OnOffEndpointConfig> Default for OnOffEndpoint<C> {
    fn default() -> Self {
        Self::new(C::DEFAULT)
    }
}

impl<C: OnOffEndpointConfig> OnOffHooks for OnOffEndpoint<C> {
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
        self.control.get()
    }

    fn set_on_off(&self, on: bool) {
        log::info!(
            "[Matter] OnOff cluster: {} {}",
            C::LOG_NOUN,
            if on { "on" } else { "off" }
        );
        self.control.set(on);
    }

    fn start_up_on_off(&self) -> Nullable<StartUpOnOffEnum> {
        self.control.start_up_on_off()
    }

    fn set_start_up_on_off(&self, value: Nullable<StartUpOnOffEnum>) -> Result<(), Error> {
        self.control.set_start_up_on_off(value)
    }

    async fn handle_off_with_effect(&self, _effect: EffectVariantEnum) {
        // No special effect handling
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestConfig;

    impl OnOffEndpointConfig for TestConfig {
        const DEFAULT: bool = false;
        const LOG_NOUN: &'static str = "test";
    }

    #[test]
    fn endpoint_forwards_startup_state_accessors() {
        let endpoint = OnOffEndpoint::<TestConfig>::default();

        assert_eq!(endpoint.start_up_on_off(), Nullable::none());

        endpoint
            .set_start_up_on_off(Nullable::some(StartUpOnOffEnum::On))
            .unwrap();
        assert_eq!(
            endpoint.start_up_on_off(),
            Nullable::some(StartUpOnOffEnum::On)
        );

        endpoint.set_start_up_on_off(Nullable::none()).unwrap();
        assert_eq!(endpoint.start_up_on_off(), Nullable::none());
    }
}
