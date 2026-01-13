//! Matter Events Shim Module
//!
//! This module provides temporary Matter event support until rs-matter
//! implements events natively. Designed for easy upstream port.
//!
//! ## Architecture
//!
//! Events in Matter are different from attributes:
//! - Events have sequential event numbers that never reset
//! - Events have timestamps and priorities
//! - Events are reported in EventReportIB structures alongside AttributeReportIBs
//!
//! This shim provides:
//! - EventPath, EventDataIB, EventStatusIB TLV structures
//! - Event emission infrastructure
//! - Integration with existing subscription notifications

pub mod data;
mod path;

pub use data::{EventData, EventDataIB, EventPriority};
pub use path::EventPath;
