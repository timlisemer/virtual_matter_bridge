//! Handler bridges connecting EndpointHandler to Matter cluster handlers.
//!
//! These bridges wrap an `EndpointHandler` and provide the interface needed
//! by Matter cluster handlers (BooleanStateHandler, OccupancySensingHandler, OnOffHooks).

use super::endpoints::endpoints_helpers::{
    ClusterNotifier, EndpointChangeTracker, NotifiableSensor, Sensor,
};
use super::endpoints::handler::EndpointHandler;
use std::sync::Arc;

/// Bridge for sensor endpoints (ContactSensor, OccupancySensor).
///
/// Wraps an `EndpointHandler` and implements the `Sensor` trait needed by
/// BooleanStateHandler and OccupancySensingHandler.
///
/// State flow:
/// - `get()` calls handler.get_state()
/// - Version is tracked locally and incremented when pusher is called
/// - Notifier is wired up to push changes to Matter subscriptions
pub struct SensorBridge {
    handler: Arc<dyn EndpointHandler>,
    changes: EndpointChangeTracker,
}

impl SensorBridge {
    /// Create a new sensor bridge wrapping the given handler.
    pub fn new(handler: Arc<dyn EndpointHandler>) -> Arc<Self> {
        let bridge = Arc::new(Self {
            handler: handler.clone(),
            changes: EndpointChangeTracker::new(),
        });

        // Wire up the pusher so the handler can push state changes to Matter
        let bridge_weak = Arc::downgrade(&bridge);
        handler.set_state_pusher(Arc::new(move |_value| {
            if let Some(bridge) = bridge_weak.upgrade() {
                bridge.on_state_changed();
            }
        }));

        bridge
    }

    /// Get the current sensor state from the handler.
    pub fn get(&self) -> bool {
        self.handler.get_state()
    }

    /// Called when the handler pushes a state change.
    fn on_state_changed(&self) {
        self.changes.mark_changed();
    }
}

impl Sensor for SensorBridge {
    fn version(&self) -> u32 {
        self.changes.version()
    }
}

impl NotifiableSensor for SensorBridge {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.changes.set_notifier(notifier);
    }
}

/// Bridge for switch endpoints (Switch, LightSwitch).
///
/// Wraps an `EndpointHandler` and provides the interface needed by OnOff handlers.
///
/// State flow:
/// - `get()` calls handler.get_state()
/// - `set()` calls handler.on_command() and increments version
/// - Version tracking enables subscription updates
/// - Notifier pushes changes to Matter subscriptions
pub struct SwitchBridge {
    handler: Arc<dyn EndpointHandler>,
    changes: EndpointChangeTracker,
}

impl SwitchBridge {
    /// Create a new switch bridge wrapping the given handler.
    pub fn new(handler: Arc<dyn EndpointHandler>) -> Arc<Self> {
        let bridge = Arc::new(Self {
            handler: handler.clone(),
            changes: EndpointChangeTracker::new(),
        });

        // Wire up the pusher so the handler can push state changes to Matter
        let bridge_weak = Arc::downgrade(&bridge);
        handler.set_state_pusher(Arc::new(move |_value| {
            if let Some(bridge) = bridge_weak.upgrade() {
                bridge.on_state_changed();
            }
        }));

        bridge
    }

    /// Get the current switch state from the handler.
    pub fn get(&self) -> bool {
        self.handler.get_state()
    }

    /// Set the switch state (called by Matter when controller sends command).
    ///
    /// This forwards the command to the handler and updates version/notifier.
    pub fn set(&self, value: bool) {
        self.handler.on_command(value);
        self.changes.mark_changed();
    }

    /// Toggle the switch state and return the new value.
    pub fn toggle(&self) -> bool {
        let new_value = !self.handler.get_state();
        self.set(new_value);
        new_value
    }

    /// Called when the handler pushes a state change from external source.
    fn on_state_changed(&self) {
        self.changes.mark_changed();
    }
}

impl Sensor for SwitchBridge {
    fn version(&self) -> u32 {
        self.changes.version()
    }
}

impl NotifiableSensor for SwitchBridge {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.changes.set_notifier(notifier);
    }
}
