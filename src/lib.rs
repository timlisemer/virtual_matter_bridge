//! Virtual Matter Bridge library.
//!
//! This library provides the core functionality for creating virtual Matter
//! devices from various input sources.

#![allow(dead_code)]
#![allow(unexpected_cfgs)]
#![recursion_limit = "256"]

pub mod commissioning;
pub mod config;
pub mod error;
pub mod input;
pub mod matter;
