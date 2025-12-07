//! Subscription persistence for session recovery after restart.
//!
//! This module persists subscription information so that after device restart,
//! we can signal to controllers that we need them to re-establish sessions.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use log::{error, info, warn};
use parking_lot::RwLock;
use rs_matter::error::Error;
use serde::{Deserialize, Serialize};

/// Persisted subscription info - enough to know who to reconnect to
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedSubscription {
    pub fabric_idx: u8,
    pub peer_node_id: u64,
    pub subscription_id: u32,
    pub min_int_secs: u16,
    pub max_int_secs: u16,
}

/// Persisted subscriptions state
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PersistedSubscriptions {
    pub subscriptions: Vec<PersistedSubscription>,
}

impl PersistedSubscriptions {
    /// Load from file
    pub fn load(path: &PathBuf) -> Self {
        match fs::read(path) {
            Ok(bytes) => match serde_json::from_slice::<PersistedSubscriptions>(&bytes) {
                Ok(state) => {
                    info!(
                        "Loaded {} persisted subscriptions from {:?}",
                        state.subscriptions.len(),
                        path
                    );
                    state
                }
                Err(e) => {
                    warn!("Failed to parse subscriptions file: {}", e);
                    Self::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!("No persisted subscriptions found (first run)");
                Self::default()
            }
            Err(e) => {
                error!("Failed to read subscriptions file: {}", e);
                Self::default()
            }
        }
    }

    /// Save to file
    pub fn save(&self, path: &PathBuf) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_vec_pretty(self)?;
        fs::write(path, data)?;
        info!(
            "Saved {} subscriptions to {:?}",
            self.subscriptions.len(),
            path
        );
        Ok(())
    }

    /// Add or update a subscription
    pub fn upsert(&mut self, sub: PersistedSubscription) {
        // Remove existing if present
        self.subscriptions
            .retain(|s| !(s.fabric_idx == sub.fabric_idx && s.peer_node_id == sub.peer_node_id));
        self.subscriptions.push(sub);
    }

    /// Remove a subscription
    pub fn remove(&mut self, fabric_idx: u8, peer_node_id: u64) {
        self.subscriptions
            .retain(|s| !(s.fabric_idx == fabric_idx && s.peer_node_id == peer_node_id));
    }

    /// Clear all subscriptions for a fabric
    pub fn remove_fabric(&mut self, fabric_idx: u8) {
        self.subscriptions.retain(|s| s.fabric_idx != fabric_idx);
    }
}

/// Store wrapper with auto-save
pub struct SubscriptionStore {
    path: PathBuf,
    state: RwLock<PersistedSubscriptions>,
}

impl SubscriptionStore {
    pub fn new(path: PathBuf) -> Self {
        let state = PersistedSubscriptions::load(&path);
        Self {
            path,
            state: RwLock::new(state),
        }
    }

    pub fn get_all(&self) -> Vec<PersistedSubscription> {
        self.state.read().subscriptions.clone()
    }

    pub fn add(&self, sub: PersistedSubscription) {
        let mut state = self.state.write();
        // Check if already present (same fabric_idx and peer_node_id)
        let already_present = state
            .subscriptions
            .iter()
            .any(|s| s.fabric_idx == sub.fabric_idx && s.peer_node_id == sub.peer_node_id);
        if already_present {
            return; // Already saved, no need to write again
        }
        state.upsert(sub);
        if let Err(e) = state.save(&self.path) {
            error!("Failed to save subscriptions: {}", e);
        }
    }

    pub fn remove(&self, fabric_idx: u8, peer_node_id: u64) {
        let mut state = self.state.write();
        state.remove(fabric_idx, peer_node_id);
        if let Err(e) = state.save(&self.path) {
            error!("Failed to save subscriptions: {}", e);
        }
    }

    pub fn has_subscriptions(&self) -> bool {
        !self.state.read().subscriptions.is_empty()
    }
}

/// Run subscription resumption - log persisted subscriptions on startup
/// The mDNS responder will handle announcements, controllers will reconnect
/// This function never returns - it pends forever after logging startup info
pub async fn run_subscription_resumption(store: Arc<SubscriptionStore>) -> Result<(), Error> {
    let subs = store.get_all();
    if subs.is_empty() {
        info!("No persisted subscriptions to resume");
    } else {
        info!(
            "Found {} persisted subscriptions - mDNS is broadcasting, waiting for controllers to reconnect",
            subs.len()
        );

        for sub in &subs {
            info!(
                "  Subscription: fabric={}, peer_node={:016x}, id={}",
                sub.fabric_idx, sub.peer_node_id, sub.subscription_id
            );
        }

        // mDNS responder already broadcasts on startup and every 30 seconds
        // Controllers should see our operational service (_matter._tcp) and initiate CASE
        info!("Subscription resumption ready - controllers will initiate CASE on mDNS discovery");
    }

    // Never return - pend forever so we don't terminate the select loop
    core::future::pending::<()>().await;
    Ok(())
}
