//! Cluster change notifier for live Matter subscription updates.
//!
//! When sensors change, they need to immediately notify the Matter subscription
//! system so updates are pushed to controllers (like Home Assistant) instantly.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

/// Notifies Matter subscriptions when sensor values change.
///
/// This is the bridge between sensors and the Matter stack.
/// When a sensor calls `notify()`, it wakes the subscription processor
/// to immediately push updates to controllers.
///
/// # Usage
/// ```ignore
/// // In sensor's set() method:
/// if let Some(notifier) = self.notifier.read().as_ref() {
///     notifier.notify();
/// }
/// ```
pub struct ClusterNotifier {
    signal: &'static Signal<CriticalSectionRawMutex, ()>,
    endpoint_id: u16,
    cluster_id: u32,
}

impl ClusterNotifier {
    /// Create a new notifier for a specific cluster.
    ///
    /// # Arguments
    /// * `signal` - Static signal that wakes the subscription processor
    /// * `endpoint_id` - Matter endpoint ID for this cluster
    /// * `cluster_id` - Matter cluster ID
    pub fn new(
        signal: &'static Signal<CriticalSectionRawMutex, ()>,
        endpoint_id: u16,
        cluster_id: u32,
    ) -> Self {
        Self {
            signal,
            endpoint_id,
            cluster_id,
        }
    }

    /// Get the endpoint ID this notifier is configured for.
    pub fn endpoint_id(&self) -> u16 {
        self.endpoint_id
    }

    /// Get the cluster ID this notifier is configured for.
    pub fn cluster_id(&self) -> u32 {
        self.cluster_id
    }

    /// Notify that this cluster's data changed.
    ///
    /// Wakes the Matter subscription processor to push updates immediately.
    /// This is a non-blocking operation.
    pub fn notify(&self) {
        self.signal.signal(());
    }
}
