//! Shared change tracking for endpoints that notify Matter subscriptions.

use super::ClusterNotifier;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicU32, Ordering};

/// Shared version counter plus optional Matter subscription notifier.
///
/// Endpoint state objects use this to expose change detection through `Sensor::version()`
/// and to push live Matter subscription updates when their value changes.
pub struct EndpointChangeTracker {
    version: AtomicU32,
    notifiers: RwLock<Vec<ClusterNotifier>>,
}

impl EndpointChangeTracker {
    pub fn new() -> Self {
        Self {
            version: AtomicU32::new(0),
            notifiers: RwLock::new(Vec::new()),
        }
    }

    pub fn version(&self) -> u32 {
        self.version.load(Ordering::SeqCst)
    }

    pub fn mark_changed(&self) {
        self.version.fetch_add(1, Ordering::SeqCst);
        for notifier in self.notifiers.read().iter() {
            notifier.notify();
        }
    }

    pub fn set_notifier(&self, notifier: ClusterNotifier) {
        self.notifiers.write().push(notifier);
    }
}

impl Default for EndpointChangeTracker {
    fn default() -> Self {
        Self::new()
    }
}
