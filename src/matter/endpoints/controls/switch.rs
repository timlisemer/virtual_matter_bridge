//! On/Off switch for Matter OnOff cluster.
//!
//! Provides the switch-specific configuration for shared OnOff endpoint behavior.

use super::on_off_control::{OnOffEndpoint, OnOffEndpointConfig};

/// Standard Matter on/off switch endpoint configuration.
pub struct SwitchConfig;

impl OnOffEndpointConfig for SwitchConfig {
    const DEFAULT: bool = true;
    const LOG_NOUN: &'static str = "switch";
}

/// On/Off switch implementing Matter's OnOffHooks trait.
pub type Switch = OnOffEndpoint<SwitchConfig>;
