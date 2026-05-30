//! Control state management for Matter clusters.
//!
//! This module provides shared state for controls (like switches) that can be
//! updated from Matter commands and other sources.

pub mod helpers;
pub mod light_switch;
pub mod switch;

pub use light_switch::LightSwitch;
pub use switch::Switch;
