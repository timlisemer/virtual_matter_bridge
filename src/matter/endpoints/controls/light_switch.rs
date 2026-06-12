//! Light switch for Matter OnOff Light device type.
//!
//! Uses the shared on/off endpoint implementation with light-specific defaults.

use super::{OnOffEndpoint, OnOffEndpointConfig};

/// Matter On/Off Light endpoint configuration.
pub struct LightSwitchConfig;

impl OnOffEndpointConfig for LightSwitchConfig {
    const DEFAULT: bool = false;
    const LOG_NOUN: &'static str = "light";
}

/// Light switch implementing Matter's OnOffHooks trait.
pub type LightSwitch = OnOffEndpoint<LightSwitchConfig>;
