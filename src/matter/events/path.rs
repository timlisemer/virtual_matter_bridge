//! EventPath TLV structure.
//!
//! Identifies an event in the Matter data model.
//! TLV encoding follows Matter Core Specification 1.4, Section 10.6.3.

/// EventPath identifies an event in the Matter data model.
///
/// ## TLV Structure
/// ```text
/// EventPath ::= STRUCTURE {
///     endpoint [0, opt]: endpoint-id,
///     cluster [1, opt]: cluster-id,
///     event [2, opt]: event-id,
///     is_urgent [3, opt]: bool,
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventPath {
    /// Endpoint ID (optional for wildcard)
    pub endpoint_id: Option<u16>,
    /// Cluster ID (optional for wildcard)
    pub cluster_id: Option<u32>,
    /// Event ID (optional for wildcard)
    pub event_id: Option<u32>,
    /// Whether this event should be reported urgently
    pub is_urgent: bool,
}

impl EventPath {
    /// Create a new EventPath with specific endpoint, cluster, and event.
    pub fn new(endpoint_id: u16, cluster_id: u32, event_id: u32) -> Self {
        Self {
            endpoint_id: Some(endpoint_id),
            cluster_id: Some(cluster_id),
            event_id: Some(event_id),
            is_urgent: false,
        }
    }

    /// Create an urgent event path.
    pub fn urgent(mut self) -> Self {
        self.is_urgent = true;
        self
    }
}

/// Context tags for EventPath TLV encoding (Matter spec tags)
pub mod tags {
    /// Tag for endpoint_id field
    pub const ENDPOINT: u8 = 0;
    /// Tag for cluster_id field
    pub const CLUSTER: u8 = 1;
    /// Tag for event_id field
    pub const EVENT: u8 = 2;
    /// Tag for is_urgent field
    pub const IS_URGENT: u8 = 3;
}
