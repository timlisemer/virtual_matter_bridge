//! GenericSwitch cluster handler (0x003B).
//!
//! The GenericSwitch cluster represents a physical switch/button that can emit events.
//! Uses rs-matter's native event support for Matter event reporting.
//!
//! ## Features Supported
//! - Momentary Switch (MS) - Button that returns to default position when released
//! - Momentary Switch Release (MSR) - Generates events on button release
//! - Momentary Switch Multi Press (MSM) - Multi-press detection
//!
//! ## Events
//! - InitialPress (0x01) - Button pressed down
//! - ShortRelease (0x03) - Button released after short press
//! - MultiPressComplete (0x06) - Multi-press sequence completed

use parking_lot::Mutex;
use rs_matter::dm::clusters::generic_switch::{
    encode_initial_press, encode_multi_press_complete, encode_short_release, events,
};
use rs_matter::dm::{
    Access, Attribute, Cluster, Dataver, Handler, NonBlockingHandler, Quality, ReadContext,
    ReadReply, Reply, WriteContext,
};
use rs_matter::dm::{EventNumberGenerator, EventSource, MAX_PENDING_EVENTS, PendingEvent};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::im::EventPriority;
use rs_matter::tlv::TLVWrite;
use rs_matter::{attribute_enum, attributes, with};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;
use strum::FromRepr;

/// Matter Cluster ID for GenericSwitch
pub const CLUSTER_ID: u32 = 0x003B;

/// Cluster revision
pub const CLUSTER_REVISION: u16 = 2;

/// Feature flags for GenericSwitch
pub mod features {
    /// Latching Switch feature (LS)
    pub const LATCHING_SWITCH: u32 = 0x01;
    /// Momentary Switch feature (MS)
    pub const MOMENTARY_SWITCH: u32 = 0x02;
    /// Momentary Switch Release feature (MSR)
    pub const MOMENTARY_SWITCH_RELEASE: u32 = 0x04;
    /// Momentary Switch Long Press feature (MSL)
    pub const MOMENTARY_SWITCH_LONG_PRESS: u32 = 0x08;
    /// Momentary Switch Multi Press feature (MSM)
    pub const MOMENTARY_SWITCH_MULTI_PRESS: u32 = 0x10;
}

/// Attribute IDs for the GenericSwitch cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum GenericSwitchAttribute {
    /// Number of switch positions (always 2 for momentary)
    NumberOfPositions = 0x0000,
    /// Current switch position (0 = released, 1 = pressed)
    CurrentPosition = 0x0001,
    /// Maximum number of presses for multi-press
    MultiPressMax = 0x0002,
}

attribute_enum!(GenericSwitchAttribute);

/// Cluster metadata definition for GenericSwitch with MS+MSR+MSM features
pub const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    // MS (0x02) + MSR (0x04) + MSM (0x10) = 0x16
    feature_map: features::MOMENTARY_SWITCH
        | features::MOMENTARY_SWITCH_RELEASE
        | features::MOMENTARY_SWITCH_MULTI_PRESS,
    attributes: attributes!(
        Attribute::new(
            GenericSwitchAttribute::NumberOfPositions as _,
            Access::RV,
            Quality::FIXED
        ),
        Attribute::new(
            GenericSwitchAttribute::CurrentPosition as _,
            Access::RV,
            Quality::NONE
        ),
        Attribute::new(
            GenericSwitchAttribute::MultiPressMax as _,
            Access::RV,
            Quality::FIXED
        ),
    ),
    commands: &[],
    with_attrs: with!(all),
    with_cmds: with!(all),
};

/// GenericSwitch state that can be shared and updated from external sources.
pub struct GenericSwitchState {
    /// Current position (0 = released, 1 = pressed)
    current_position: AtomicU8,
    /// Event number generator (sequential, never resets)
    event_number: EventNumberGenerator,
    /// Pending events queue (uses rs-matter's native type)
    pending_events: Mutex<heapless::Vec<PendingEvent, MAX_PENDING_EVENTS>>,
    /// Time when the switch was created (for elapsed timestamps)
    start_time: Instant,
    /// Endpoint ID (set when wired to Matter stack)
    endpoint_id: AtomicU8,
}

impl GenericSwitchState {
    /// Create a new GenericSwitch state.
    pub fn new() -> Self {
        Self {
            current_position: AtomicU8::new(0),
            event_number: EventNumberGenerator::new(),
            pending_events: Mutex::new(heapless::Vec::new()),
            start_time: Instant::now(),
            endpoint_id: AtomicU8::new(0),
        }
    }

    /// Set the endpoint ID (called when wiring to Matter stack).
    pub fn set_endpoint_id(&self, endpoint_id: u16) {
        self.endpoint_id.store(endpoint_id as u8, Ordering::SeqCst);
    }

    /// Get the current position.
    pub fn current_position(&self) -> u8 {
        self.current_position.load(Ordering::SeqCst)
    }

    /// Get elapsed time since start in milliseconds.
    fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// Get endpoint ID.
    fn get_endpoint_id(&self) -> u16 {
        self.endpoint_id.load(Ordering::SeqCst) as u16
    }

    /// Record an InitialPress event (button pressed down).
    pub fn press(&self) {
        self.current_position.store(1, Ordering::SeqCst);

        let payload: heapless::Vec<u8, 16> = encode_initial_press(1);
        let event = PendingEvent::with_payload(
            self.get_endpoint_id(),
            CLUSTER_ID,
            events::INITIAL_PRESS,
            self.event_number.next(),
            EventPriority::Info,
            self.elapsed_ms(),
            &payload,
        );

        let mut events = self.pending_events.lock();
        events.push(event).ok();
    }

    /// Record a ShortRelease event (button released after short press).
    pub fn release(&self) {
        let prev_position = self.current_position.swap(0, Ordering::SeqCst);

        let payload: heapless::Vec<u8, 16> = encode_short_release(prev_position);
        let event = PendingEvent::with_payload(
            self.get_endpoint_id(),
            CLUSTER_ID,
            events::SHORT_RELEASE,
            self.event_number.next(),
            EventPriority::Info,
            self.elapsed_ms(),
            &payload,
        );

        let mut events = self.pending_events.lock();
        events.push(event).ok();
    }

    /// Record a single press (InitialPress + ShortRelease).
    pub fn single_press(&self) {
        self.press();
        self.release();
    }

    /// Record a double press (MultiPressComplete with count=2).
    pub fn double_press(&self) {
        self.current_position.store(0, Ordering::SeqCst);

        let payload: heapless::Vec<u8, 16> = encode_multi_press_complete(1, 2);
        let event = PendingEvent::with_payload(
            self.get_endpoint_id(),
            CLUSTER_ID,
            events::MULTI_PRESS_COMPLETE,
            self.event_number.next(),
            EventPriority::Info,
            self.elapsed_ms(),
            &payload,
        );

        let mut events = self.pending_events.lock();
        events.push(event).ok();
    }

    /// Record a hold start (InitialPress, kept pressed).
    pub fn hold_start(&self) {
        self.press();
    }

    /// Record a hold release (ShortRelease after hold).
    pub fn hold_release(&self) {
        self.release();
    }
}

impl Default for GenericSwitchState {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSource for GenericSwitchState {
    fn take_pending_events(&self) -> heapless::Vec<PendingEvent, MAX_PENDING_EVENTS> {
        let mut events = self.pending_events.lock();
        core::mem::take(&mut *events)
    }

    fn has_pending_events(&self) -> bool {
        !self.pending_events.lock().is_empty()
    }
}

/// Handler for GenericSwitch cluster.
///
/// This handler serves the GenericSwitch cluster attributes and manages events.
/// Events are stored in the shared GenericSwitchState and reported via
/// the EventSource trait implementation.
pub struct GenericSwitchHandler {
    dataver: Dataver,
    state: Arc<GenericSwitchState>,
    /// Number of positions (always 2 for momentary switch)
    num_positions: u8,
    /// Maximum multi-press count
    multi_press_max: u8,
}

impl GenericSwitchHandler {
    /// Cluster definition for use in the data model
    pub const CLUSTER: Cluster<'static> = CLUSTER;

    /// Create a new handler with a shared state.
    pub fn new(dataver: Dataver, state: Arc<GenericSwitchState>) -> Self {
        Self {
            dataver,
            state,
            num_positions: 2,
            multi_press_max: 2,
        }
    }

    /// Get the shared state for external updates.
    pub fn state(&self) -> &Arc<GenericSwitchState> {
        &self.state
    }

    fn read_impl(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        let attr = ctx.attr();

        let Some(mut writer) = reply.with_dataver(self.dataver.get())? else {
            return Ok(());
        };

        // Global attributes
        if attr.is_system() {
            return CLUSTER.read(attr, writer);
        }

        let tag = writer.tag();
        {
            let mut tw = writer.writer();

            match attr.attr_id.try_into()? {
                GenericSwitchAttribute::NumberOfPositions => {
                    tw.u8(tag, self.num_positions)?;
                }
                GenericSwitchAttribute::CurrentPosition => {
                    tw.u8(tag, self.state.current_position())?;
                }
                GenericSwitchAttribute::MultiPressMax => {
                    tw.u8(tag, self.multi_press_max)?;
                }
            }
        }

        writer.complete()
    }

    fn write_impl(&self, _ctx: impl WriteContext) -> Result<(), Error> {
        // Cluster is read-only
        Err(ErrorCode::UnsupportedAccess.into())
    }
}

impl Handler for GenericSwitchHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.read_impl(ctx, reply)
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        self.write_impl(ctx)
    }
}

impl NonBlockingHandler for GenericSwitchHandler {}
