//! Event data structures for Matter events.
//!
//! TLV encoding follows Matter Core Specification 1.4, Section 10.6.3.

use super::path::EventPath;

/// Event priority levels as defined in Matter spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EventPriority {
    /// Debug events - low priority, may be dropped
    Debug = 0,
    /// Info events - normal priority
    Info = 1,
    /// Critical events - high priority, should not be dropped
    Critical = 2,
}

impl EventPriority {
    /// Get the priority value as u8.
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Timestamp type for events.
/// Named to match Matter specification terminology.
#[derive(Debug, Clone, Copy)]
#[allow(clippy::enum_variant_names)]
pub enum EventTimestamp {
    /// System time in milliseconds since boot
    SystemTime(u64),
    /// Delta system time from previous event (milliseconds)
    DeltaSystemTime(u64),
    /// Delta epoch time from previous event (microseconds since Unix epoch)
    DeltaEpochTime(u64),
}

impl EventTimestamp {
    /// Get the timestamp type tag (for TLV encoding).
    pub fn tag(&self) -> u8 {
        match self {
            EventTimestamp::SystemTime(_) => tags::SYSTEM_TIMESTAMP,
            EventTimestamp::DeltaSystemTime(_) => tags::DELTA_SYSTEM_TIMESTAMP,
            EventTimestamp::DeltaEpochTime(_) => tags::DELTA_EPOCH_TIMESTAMP,
        }
    }

    /// Get the timestamp value.
    pub fn value(&self) -> u64 {
        match self {
            EventTimestamp::SystemTime(v)
            | EventTimestamp::DeltaSystemTime(v)
            | EventTimestamp::DeltaEpochTime(v) => *v,
        }
    }
}

/// EventDataIB represents a single event report.
///
/// ## TLV Structure
/// ```text
/// EventDataIB ::= STRUCTURE {
///     path [0]: EventPath,
///     event_number [1]: unsigned 64-bit,
///     priority [2]: unsigned 8-bit,
///     // One of the following timestamps:
///     system_timestamp [3, opt]: unsigned 64-bit,     // ms since boot
///     delta_system_timestamp [4, opt]: unsigned 64-bit,
///     delta_epoch_timestamp [5, opt]: unsigned 64-bit, // us since epoch
///     data [6, opt]: any,
/// }
/// ```
#[derive(Debug, Clone)]
pub struct EventDataIB {
    /// Event path (endpoint, cluster, event ID)
    pub path: EventPath,
    /// Sequential event number (never resets, unique per node)
    pub event_number: u64,
    /// Event priority
    pub priority: EventPriority,
    /// Timestamp
    pub timestamp: EventTimestamp,
    /// Event data payload (cluster-specific)
    pub data: EventData,
}

impl EventDataIB {
    /// Create a new EventDataIB with the given parameters.
    pub fn new(
        path: EventPath,
        event_number: u64,
        priority: EventPriority,
        timestamp: EventTimestamp,
        data: EventData,
    ) -> Self {
        Self {
            path,
            event_number,
            priority,
            timestamp,
            data,
        }
    }
}

/// Event data payload types.
///
/// Each cluster defines its own event data structures.
/// This enum provides common event types used by GenericSwitch.
#[derive(Debug, Clone)]
pub enum EventData {
    /// Empty event (no additional data)
    Empty,
    /// InitialPress event data (GenericSwitch)
    InitialPress {
        /// Current position (0 or 1 for momentary switch)
        new_position: u8,
    },
    /// ShortRelease event data (GenericSwitch)
    ShortRelease {
        /// Previous position before release
        previous_position: u8,
    },
    /// LongPress event data (GenericSwitch)
    LongPress {
        /// Current position during long press
        new_position: u8,
    },
    /// LongRelease event data (GenericSwitch)
    LongRelease {
        /// Previous position before release
        previous_position: u8,
    },
    /// MultiPressOngoing event data (GenericSwitch)
    MultiPressOngoing {
        /// Current position
        new_position: u8,
        /// Current press count
        current_number_of_presses_counted: u8,
    },
    /// MultiPressComplete event data (GenericSwitch)
    MultiPressComplete {
        /// Previous position
        previous_position: u8,
        /// Total number of presses detected
        total_number_of_presses_counted: u8,
    },
}

/// Context tags for EventDataIB TLV encoding
pub mod tags {
    /// Tag for path field
    pub const PATH: u8 = 0;
    /// Tag for event_number field
    pub const EVENT_NUMBER: u8 = 1;
    /// Tag for priority field
    pub const PRIORITY: u8 = 2;
    /// Tag for system_timestamp field
    pub const SYSTEM_TIMESTAMP: u8 = 3;
    /// Tag for delta_system_timestamp field
    pub const DELTA_SYSTEM_TIMESTAMP: u8 = 4;
    /// Tag for delta_epoch_timestamp field
    pub const DELTA_EPOCH_TIMESTAMP: u8 = 5;
    /// Tag for data field
    pub const DATA: u8 = 6;
}

/// GenericSwitch event IDs
pub mod generic_switch_events {
    /// Button was initially pressed down
    pub const INITIAL_PRESS: u32 = 0x01;
    /// Button was released after being held for a long time
    pub const LONG_PRESS: u32 = 0x02;
    /// Button was released after a short press
    pub const SHORT_RELEASE: u32 = 0x03;
    /// Button was released after a long press
    pub const LONG_RELEASE: u32 = 0x04;
    /// Multi-press sequence is ongoing
    pub const MULTI_PRESS_ONGOING: u32 = 0x05;
    /// Multi-press sequence completed
    pub const MULTI_PRESS_COMPLETE: u32 = 0x06;
}
