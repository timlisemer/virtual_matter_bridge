use super::{ClusterNotifier, EndpointChangeTracker, NotifiableSensor, Sensor};
use crate::matter::endpoints::EndpointHandler;
use parking_lot::RwLock;
use std::sync::Arc;

/// Readiness view shared with Matter endpoint metadata.
pub trait SourceReadiness: Sensor + NotifiableSensor + Send + Sync {
    fn is_ready(&self) -> bool;
    fn mark_unavailable(&self) -> bool;
}

#[derive(Debug)]
pub struct AlwaysReady;

impl Sensor for AlwaysReady {
    fn version(&self) -> u32 {
        1
    }
}

impl NotifiableSensor for AlwaysReady {
    fn set_notifier(&self, _notifier: ClusterNotifier) {}
}

impl SourceReadiness for AlwaysReady {
    fn is_ready(&self) -> bool {
        true
    }

    fn mark_unavailable(&self) -> bool {
        false
    }
}

pub struct AnyChildReady {
    children: Vec<Arc<dyn SourceReadiness>>,
}

impl AnyChildReady {
    pub fn new(children: Vec<Arc<dyn SourceReadiness>>) -> Self {
        Self { children }
    }
}

impl Sensor for AnyChildReady {
    fn version(&self) -> u32 {
        self.children
            .iter()
            .fold(0, |version, child| version.wrapping_add(child.version()))
    }
}

impl NotifiableSensor for AnyChildReady {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        for child in &self.children {
            child.set_notifier(notifier);
        }
    }
}

impl SourceReadiness for AnyChildReady {
    fn is_ready(&self) -> bool {
        self.children.iter().any(|child| child.is_ready())
    }

    fn mark_unavailable(&self) -> bool {
        let mut changed = false;
        for child in &self.children {
            changed = child.mark_unavailable() || changed;
        }
        changed
    }
}

pub struct ReadinessOnlyHandler {
    readiness: Arc<dyn SourceReadiness>,
}

impl ReadinessOnlyHandler {
    pub fn new(readiness: Arc<dyn SourceReadiness>) -> Self {
        Self { readiness }
    }

    pub fn always_ready() -> Self {
        Self {
            readiness: Arc::new(AlwaysReady),
        }
    }
}

impl EndpointHandler for ReadinessOnlyHandler {
    fn on_command(&self, _value: bool) {}

    fn get_state(&self) -> Option<bool> {
        None
    }

    fn readiness(&self) -> Arc<dyn SourceReadiness> {
        self.readiness.clone()
    }

    fn set_state_pusher(&self, _pusher: Arc<dyn Fn(bool) + Send + Sync>) {}
}

/// Source-owned optional snapshot for an endpoint.
///
/// `None` means the backing source has not delivered its first signal yet.
/// `Some(value)` means the endpoint is ready and the value came from the source.
pub struct SourceSnapshot<T> {
    value: RwLock<Option<T>>,
    changes: EndpointChangeTracker,
}

impl<T> SourceSnapshot<T> {
    pub fn new() -> Self {
        Self {
            value: RwLock::new(None),
            changes: EndpointChangeTracker::new(),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.value.read().is_some()
    }
}

impl<T: Clone> SourceSnapshot<T> {
    pub fn snapshot(&self) -> Option<T> {
        self.value.read().clone()
    }
}

impl<T: Clone + PartialEq> SourceSnapshot<T> {
    pub fn update_source(&self, value: T) -> bool {
        let mut guard = self.value.write();
        let changed = guard.as_ref() != Some(&value);
        *guard = Some(value);
        if changed {
            self.changes.mark_changed();
        }
        changed
    }
}

impl<T> SourceSnapshot<T> {
    pub fn clear_source(&self) -> bool {
        let mut guard = self.value.write();
        let changed = guard.is_some();
        *guard = None;
        if changed {
            self.changes.mark_changed();
        }
        changed
    }
}

impl SourceSnapshot<()> {
    pub fn mark_ready(&self) {
        self.update_source(());
    }
}

impl<T> Default for SourceSnapshot<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Sync + 'static> Sensor for SourceSnapshot<T> {
    fn version(&self) -> u32 {
        self.changes.version()
    }
}

impl<T: Send + Sync + 'static> NotifiableSensor for SourceSnapshot<T> {
    fn set_notifier(&self, notifier: ClusterNotifier) {
        self.changes.set_notifier(notifier);
    }
}

impl<T: Send + Sync + 'static> SourceReadiness for SourceSnapshot<T> {
    fn is_ready(&self) -> bool {
        SourceSnapshot::is_ready(self)
    }

    fn mark_unavailable(&self) -> bool {
        self.clear_source()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_update_marks_ready_even_when_value_is_default_equivalent() {
        let snapshot = SourceSnapshot::<bool>::new();

        snapshot.update_source(false);

        assert!(snapshot.is_ready());
        assert_eq!(snapshot.snapshot(), Some(false));
        assert_eq!(snapshot.version(), 1);
    }

    #[test]
    fn clear_source_returns_to_pre_ready_state() {
        let snapshot = SourceSnapshot::<bool>::new();

        assert!(!snapshot.clear_source());
        assert_eq!(snapshot.version(), 0);

        snapshot.update_source(false);
        assert!(snapshot.is_ready());
        assert_eq!(snapshot.version(), 1);

        assert!(snapshot.clear_source());
        assert!(!snapshot.is_ready());
        assert_eq!(snapshot.snapshot(), None);
        assert_eq!(snapshot.version(), 2);

        assert!(!snapshot.clear_source());
        assert_eq!(snapshot.version(), 2);
    }

    #[test]
    fn any_child_ready_tracks_whether_at_least_one_child_is_ready() {
        let first = Arc::new(SourceSnapshot::<bool>::new());
        let second = Arc::new(SourceSnapshot::<bool>::new());
        let readiness = AnyChildReady::new(vec![first.clone(), second.clone()]);

        assert!(!readiness.is_ready());
        assert_eq!(readiness.version(), 0);

        first.update_source(false);

        assert!(readiness.is_ready());
        assert_eq!(readiness.version(), 1);

        second.update_source(true);

        assert!(readiness.is_ready());
        assert_eq!(readiness.version(), 2);
    }
}
