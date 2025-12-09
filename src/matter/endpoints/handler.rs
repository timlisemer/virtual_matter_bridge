//! EndpointHandler trait for bidirectional communication between Matter and business logic.
//!
//! Implement this trait to connect your sensors/switches to the Matter protocol.
//! - For sensors: push state changes via `set_state_pusher` callback
//! - For switches: receive commands via `on_command` and push state via callback

use std::sync::Arc;

/// Trait for bidirectional communication between Matter and your business logic.
///
/// Implement this trait to connect your device logic to the Matter stack.
///
/// # For Sensors (read-only devices like ContactSensor, OccupancySensor):
/// - `on_command` can be ignored (sensors don't receive commands)
/// - Call the pusher registered via `set_state_pusher` when state changes
///
/// # For Switches (read-write devices like Switch, LightSwitch):
/// - Implement `on_command` to handle Matter commands (ON/OFF)
/// - Call the pusher when state changes from external sources
///
/// # Example
/// ```ignore
/// struct MyDoorHandler {
///     state: AtomicBool,
///     pusher: RwLock<Option<Arc<dyn Fn(bool) + Send + Sync>>>,
/// }
///
/// impl EndpointHandler for MyDoorHandler {
///     fn on_command(&self, _value: bool) {
///         // Door sensor is read-only, ignore commands
///     }
///
///     fn get_state(&self) -> bool {
///         self.state.load(Ordering::SeqCst)
///     }
///
///     fn set_state_pusher(&self, pusher: Arc<dyn Fn(bool) + Send + Sync>) {
///         *self.pusher.write() = Some(pusher);
///     }
/// }
/// ```
pub trait EndpointHandler: Send + Sync + 'static {
    /// Called when Matter controller sends a command (e.g., switch ON/OFF).
    ///
    /// For read-only sensors, this should be a no-op.
    /// For switches, update your internal state and any connected hardware.
    fn on_command(&self, value: bool);

    /// Returns the current state value.
    ///
    /// Called when Matter controller reads the attribute.
    fn get_state(&self) -> bool;

    /// Register a callback to push state changes TO Matter.
    ///
    /// The stack calls this during initialization. Store the pusher and call it
    /// whenever your device state changes (e.g., from simulation, hardware, API).
    ///
    /// This enables live Matter subscription updates.
    fn set_state_pusher(&self, pusher: Arc<dyn Fn(bool) + Send + Sync>);
}
