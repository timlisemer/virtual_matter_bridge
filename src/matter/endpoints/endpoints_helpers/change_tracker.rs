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
    notifier: RwLock<Option<ClusterNotifier>>,
}

impl EndpointChangeTracker {
    pub fn new() -> Self {
        Self {
            version: AtomicU32::new(0),
            notifier: RwLock::new(None),
        }
    }

    pub fn version(&self) -> u32 {
        self.version.load(Ordering::SeqCst)
    }

    pub fn mark_changed(&self) {
        self.version.fetch_add(1, Ordering::SeqCst);
        if let Some(notifier) = self.notifier.read().as_ref() {
            notifier.notify();
        }
    }

    pub fn set_notifier(&self, notifier: ClusterNotifier) {
        *self.notifier.write() = Some(notifier);
    }
}

impl Default for EndpointChangeTracker {
    fn default() -> Self {
        Self::new()
    }
}
