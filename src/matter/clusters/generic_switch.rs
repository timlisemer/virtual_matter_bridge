//! GenericSwitch cluster handler (0x003B).
//!
//! The GenericSwitch cluster represents a physical switch/button that can emit events.
//!
//! ## Features Supported
//! - Momentary Switch (MS) - Button that returns to default position when released
//! - Momentary Switch Release (MSR) - Generates events on button release
//!
//! ## Events
//! - InitialPress (0x01) - Button pressed down
//! - ShortRelease (0x03) - Button released after short press
//! - MultiPressComplete (0x06) - Multi-press sequence completed

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use parking_lot::{Mutex, RwLock};
use rs_matter::dm::events::EVENT_DATA_TAG;
use rs_matter::dm::{
    Access, Attribute, Cluster, Dataver, Event, Handler, MatchContext, NonBlockingHandler, Quality,
    ReadContext, ReadReply, Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::{TLVTag, TLVWrite};
use rs_matter::{attribute_enum, attributes, events, with};
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU16, Ordering};
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

/// GenericSwitch event IDs.
pub mod event_ids {
    pub const INITIAL_PRESS: u32 = 0x01;
    pub const LONG_PRESS: u32 = 0x02;
    pub const SHORT_RELEASE: u32 = 0x03;
    pub const LONG_RELEASE: u32 = 0x04;
    pub const MULTI_PRESS_ONGOING: u32 = 0x05;
    pub const MULTI_PRESS_COMPLETE: u32 = 0x06;
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

/// Cluster metadata definition for GenericSwitch with MS+MSR+MSL+MSM features
pub const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    // MS (0x02) + MSR (0x04) + MSL (0x08) + MSM (0x10) = 0x1e
    feature_map: features::MOMENTARY_SWITCH
        | features::MOMENTARY_SWITCH_RELEASE
        | features::MOMENTARY_SWITCH_LONG_PRESS
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
    events: events!(
        Event::new(0x01, Access::RV),
        Event::new(0x02, Access::RV),
        Event::new(0x03, Access::RV),
        Event::new(0x04, Access::RV),
        Event::new(0x05, Access::RV),
        Event::new(0x06, Access::RV),
    ),
    with_attrs: with!(all),
    with_cmds: with!(all),
    with_events: with!(all),
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenericSwitchPendingEvent {
    InitialPress {
        new_position: u8,
    },
    ShortRelease {
        previous_position: u8,
    },
    LongPress {
        new_position: u8,
    },
    LongRelease {
        previous_position: u8,
    },
    MultiPressOngoing {
        new_position: u8,
        current_number_of_presses_counted: u8,
    },
    MultiPressComplete {
        previous_position: u8,
        total_number_of_presses_counted: u8,
    },
}

impl GenericSwitchPendingEvent {
    pub fn event_id(&self) -> u32 {
        match self {
            Self::InitialPress { .. } => event_ids::INITIAL_PRESS,
            Self::LongPress { .. } => event_ids::LONG_PRESS,
            Self::ShortRelease { .. } => event_ids::SHORT_RELEASE,
            Self::LongRelease { .. } => event_ids::LONG_RELEASE,
            Self::MultiPressOngoing { .. } => event_ids::MULTI_PRESS_ONGOING,
            Self::MultiPressComplete { .. } => event_ids::MULTI_PRESS_COMPLETE,
        }
    }
}

pub(crate) fn write_event_data(
    tw: &mut impl TLVWrite,
    event: GenericSwitchPendingEvent,
) -> Result<(), Error> {
    tw.start_struct(&EVENT_DATA_TAG)?;
    match event {
        GenericSwitchPendingEvent::InitialPress { new_position }
        | GenericSwitchPendingEvent::LongPress { new_position } => {
            tw.u8(&TLVTag::Context(0), new_position)?;
        }
        GenericSwitchPendingEvent::ShortRelease { previous_position }
        | GenericSwitchPendingEvent::LongRelease { previous_position } => {
            tw.u8(&TLVTag::Context(0), previous_position)?;
        }
        GenericSwitchPendingEvent::MultiPressOngoing {
            new_position,
            current_number_of_presses_counted,
        } => {
            tw.u8(&TLVTag::Context(0), new_position)?;
            tw.u8(&TLVTag::Context(1), current_number_of_presses_counted)?;
        }
        GenericSwitchPendingEvent::MultiPressComplete {
            previous_position,
            total_number_of_presses_counted,
        } => {
            tw.u8(&TLVTag::Context(0), previous_position)?;
            tw.u8(&TLVTag::Context(1), total_number_of_presses_counted)?;
        }
    }
    tw.end_container()
}

/// GenericSwitch state that can be shared and updated from external sources.
pub struct GenericSwitchState {
    /// Current position (0 = released, 1 = pressed)
    current_position: AtomicU8,
    /// Pending events queue
    pending_events: Mutex<VecDeque<GenericSwitchPendingEvent>>,
    /// Endpoint ID (set when wired to Matter stack)
    endpoint_id: AtomicU16,
    /// Matter task notifier for pending events.
    notifier: RwLock<Option<&'static Signal<CriticalSectionRawMutex, ()>>>,
}

impl GenericSwitchState {
    /// Create a new GenericSwitch state.
    pub fn new() -> Self {
        Self {
            current_position: AtomicU8::new(0),
            pending_events: Mutex::new(VecDeque::new()),
            endpoint_id: AtomicU16::new(0),
            notifier: RwLock::new(None),
        }
    }

    /// Set the endpoint ID (called when wiring to Matter stack).
    pub fn set_endpoint_id(&self, endpoint_id: u16) {
        self.endpoint_id.store(endpoint_id, Ordering::SeqCst);
    }

    /// Get the current position.
    pub fn current_position(&self) -> u8 {
        self.current_position.load(Ordering::SeqCst)
    }

    /// Get endpoint ID.
    pub fn endpoint_id(&self) -> u16 {
        self.endpoint_id.load(Ordering::SeqCst)
    }

    pub fn set_event_notifier(&self, notifier: &'static Signal<CriticalSectionRawMutex, ()>) {
        *self.notifier.write() = Some(notifier);
        notifier.signal(());
    }

    fn push_event(&self, event: GenericSwitchPendingEvent) {
        self.pending_events.lock().push_back(event);
        if let Some(notifier) = *self.notifier.read() {
            notifier.signal(());
        }
    }

    /// Record an InitialPress event (button pressed down).
    pub fn press(&self) {
        self.current_position.store(1, Ordering::SeqCst);
        self.push_event(GenericSwitchPendingEvent::InitialPress { new_position: 1 });
    }

    /// Record a ShortRelease event (button released after short press).
    pub fn release(&self) {
        let prev_position = self.current_position.swap(0, Ordering::SeqCst);
        self.push_event(GenericSwitchPendingEvent::ShortRelease {
            previous_position: prev_position,
        });
    }

    /// Record a single press sequence.
    pub fn single_press(&self) {
        self.press();
        self.release();
        self.push_event(GenericSwitchPendingEvent::MultiPressComplete {
            previous_position: 1,
            total_number_of_presses_counted: 1,
        });
    }

    /// Record a double press (MultiPressComplete with count=2).
    pub fn double_press(&self) {
        self.current_position.store(0, Ordering::SeqCst);
        self.push_event(GenericSwitchPendingEvent::MultiPressComplete {
            previous_position: 1,
            total_number_of_presses_counted: 2,
        });
    }

    /// Record a hold start (InitialPress, kept pressed).
    pub fn hold_start(&self) {
        self.current_position.store(1, Ordering::SeqCst);
        self.push_event(GenericSwitchPendingEvent::InitialPress { new_position: 1 });
    }

    /// Record a hold action after the long-press threshold has been reached.
    pub fn long_press(&self) {
        if self.current_position.swap(1, Ordering::SeqCst) == 0 {
            self.push_event(GenericSwitchPendingEvent::InitialPress { new_position: 1 });
        }
        self.push_event(GenericSwitchPendingEvent::LongPress { new_position: 1 });
    }

    /// Record a hold release (ShortRelease after hold).
    pub fn hold_release(&self) {
        let prev_position = self.current_position.swap(0, Ordering::SeqCst);
        self.push_event(GenericSwitchPendingEvent::LongRelease {
            previous_position: prev_position,
        });
    }

    /// Get and clear pending events.
    pub fn take_pending_events(&self) -> Vec<GenericSwitchPendingEvent> {
        let mut events = self.pending_events.lock();
        events.drain(..).collect()
    }

    /// Check if there are pending events.
    pub fn has_pending_events(&self) -> bool {
        !self.pending_events.lock().is_empty()
    }
}

impl Default for GenericSwitchState {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for GenericSwitch cluster.
///
/// This handler serves the GenericSwitch cluster attributes and manages events.
/// Events are stored in the shared GenericSwitchState and should be retrieved
/// and reported via the event notification system.
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

    fn bump_dataver(&self, _ctx: impl MatchContext) {
        self.dataver.changed();
    }
}

impl NonBlockingHandler for GenericSwitchHandler {}

#[cfg(test)]
mod tests {
    use super::*;
    use rs_matter::tlv::{TLV, TLVElement, TLVTag, TLVValue};
    use rs_matter::utils::storage::WriteBuf;

    #[test]
    fn hold_start_only_records_press_transition() {
        let state = GenericSwitchState::new();

        state.hold_start();

        assert_eq!(state.current_position(), 1);
        assert_eq!(
            state.take_pending_events(),
            vec![GenericSwitchPendingEvent::InitialPress { new_position: 1 }]
        );
    }

    #[test]
    fn hold_release_records_long_release() {
        let state = GenericSwitchState::new();
        state.hold_start();
        state.take_pending_events();

        state.hold_release();

        assert_eq!(state.current_position(), 0);
        assert_eq!(
            state.take_pending_events(),
            vec![GenericSwitchPendingEvent::LongRelease {
                previous_position: 1
            }]
        );
    }

    #[test]
    fn single_press_completes_multi_press_sequence_with_count_one() {
        let state = GenericSwitchState::new();

        state.single_press();

        assert_eq!(state.current_position(), 0);
        assert_eq!(
            state.take_pending_events(),
            vec![
                GenericSwitchPendingEvent::InitialPress { new_position: 1 },
                GenericSwitchPendingEvent::ShortRelease {
                    previous_position: 1
                },
                GenericSwitchPendingEvent::MultiPressComplete {
                    previous_position: 1,
                    total_number_of_presses_counted: 1,
                },
            ]
        );
    }

    #[test]
    fn long_press_records_press_and_long_press() {
        let state = GenericSwitchState::new();

        state.long_press();

        assert_eq!(state.current_position(), 1);
        assert_eq!(
            state.take_pending_events(),
            vec![
                GenericSwitchPendingEvent::InitialPress { new_position: 1 },
                GenericSwitchPendingEvent::LongPress { new_position: 1 },
            ]
        );
    }

    #[test]
    fn writes_single_field_event_payload() {
        let mut buf = [0; 32];
        let mut wb = WriteBuf::new(&mut buf);

        write_event_data(
            &mut wb,
            GenericSwitchPendingEvent::InitialPress { new_position: 1 },
        )
        .unwrap();

        let root = TLVElement::new(wb.as_slice()).structure().unwrap();
        let mut fields = root.iter();
        assert_eq!(
            fields.next().unwrap().unwrap().tlv().unwrap(),
            TLV {
                tag: TLVTag::Context(0),
                value: TLVValue::U8(1),
            }
        );
        assert!(fields.next().is_none());
    }

    #[test]
    fn writes_multi_field_event_payload() {
        let mut buf = [0; 32];
        let mut wb = WriteBuf::new(&mut buf);

        write_event_data(
            &mut wb,
            GenericSwitchPendingEvent::MultiPressComplete {
                previous_position: 1,
                total_number_of_presses_counted: 2,
            },
        )
        .unwrap();

        let root = TLVElement::new(wb.as_slice()).structure().unwrap();
        let mut fields = root.iter();
        assert_eq!(
            fields.next().unwrap().unwrap().tlv().unwrap(),
            TLV {
                tag: TLVTag::Context(0),
                value: TLVValue::U8(1),
            }
        );
        assert_eq!(
            fields.next().unwrap().unwrap().tlv().unwrap(),
            TLV {
                tag: TLVTag::Context(1),
                value: TLVValue::U8(2),
            }
        );
        assert!(fields.next().is_none());
    }
}
