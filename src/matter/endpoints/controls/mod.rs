//! Control state management for Matter clusters.
//!
//! This module provides shared state for controls (like switches) that can be
//! updated from Matter commands and other sources.

pub mod helpers;
pub mod on_off_hooks;
pub mod switch;
pub mod switch_hooks;

pub use on_off_hooks::DevicePowerSwitch;
pub use switch::Switch;
pub use switch_hooks::SwitchHooks;
