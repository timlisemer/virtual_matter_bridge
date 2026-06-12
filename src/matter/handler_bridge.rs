//! Handler bridges connecting EndpointHandler to Matter cluster handlers.
//!
//! These bridges wrap an `EndpointHandler` and provide the interface needed
//! by Matter cluster handlers (BooleanStateHandler, OccupancySensingHandler, OnOffHooks).

use super::endpoints::endpoints_helpers::{
    ClusterNotifier, EndpointChangeTracker, NotifiableSensor, Sensor, SourceReadiness,
};
use super::endpoints::handler::EndpointHandler;
use rs_matter::error::{Error, ErrorCode};
use std::sync::Arc;

struct BridgeCore {
    handler: Arc<dyn EndpointHandler>,
    changes: EndpointChangeTracker,
}

impl BridgeCore {
    fn new(handler: Arc<dyn EndpointHandler>) -> Arc<Self> {
        let core = Arc::new(Self {
            handler: handler.clone(),
            changes: EndpointChangeTracker::new(),
        });

        let core_weak = Arc::downgrade(&core);
        handler.set_state_pusher(Arc::new(move |_value| {
            if let Some(core) = core_weak.upgrade() {
                core.on_state_changed();
            }
        }));

        core
    }

    fn get(&self) -> Option<bool> {
        self.handler.get_state()
    }

    fn readiness(&self) -> Arc<dyn SourceReadiness> {
        self.handler.readiness()
    }

    fn version(&self) -> u32 {
        self.changes.version()
    }

    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.changes.set_notifier(notifier);
    }

    fn on_state_changed(&self) {
        self.changes.mark_changed();
    }
}

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
    core: Arc<BridgeCore>,
}

impl SensorBridge {
    /// Create a new sensor bridge wrapping the given handler.
    pub fn new(handler: Arc<dyn EndpointHandler>) -> Arc<Self> {
        Arc::new(Self {
            core: BridgeCore::new(handler),
        })
    }

    /// Get the current sensor state from the handler.
    pub fn get(&self) -> Option<bool> {
        self.core.get()
    }

    pub fn readiness(&self) -> Arc<dyn SourceReadiness> {
        self.core.readiness()
    }
}

impl Sensor for SensorBridge {
    fn version(&self) -> u32 {
        self.core.version()
    }
}

impl NotifiableSensor for SensorBridge {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.core.set_notifier(notifier);
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
    core: Arc<BridgeCore>,
}

impl SwitchBridge {
    /// Create a new switch bridge wrapping the given handler.
    pub fn new(handler: Arc<dyn EndpointHandler>) -> Arc<Self> {
        Arc::new(Self {
            core: BridgeCore::new(handler),
        })
    }

    /// Get the current switch state from the handler.
    pub fn get(&self) -> Option<bool> {
        self.core.get()
    }

    /// Set the switch state (called by Matter when controller sends command).
    ///
    /// This forwards the command to the handler and updates version/notifier.
    pub fn set(&self, value: bool) -> Result<(), Error> {
        if !self.is_ready() {
            return Err(ErrorCode::Busy.into());
        }
        self.core.handler.on_command(value);
        self.core.on_state_changed();
        Ok(())
    }

    /// Toggle the switch state and return the new value.
    pub fn toggle(&self) -> Result<bool, Error> {
        let current = self.core.get().ok_or(ErrorCode::Busy)?;
        let new_value = !current;
        self.set(new_value)?;
        Ok(new_value)
    }

    pub fn is_ready(&self) -> bool {
        self.core.readiness().is_ready()
    }

    pub fn readiness(&self) -> Arc<dyn SourceReadiness> {
        self.core.readiness()
    }
}

impl Sensor for SwitchBridge {
    fn version(&self) -> u32 {
        self.core.version()
    }
}

impl NotifiableSensor for SwitchBridge {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.core.set_notifier(notifier);
    }
}
