//! ICD Management cluster state and persistence.
//!
//! This module provides the state storage for the ICD Management cluster (0x0046).
//! For always-on devices like this bridge, we implement the basic cluster without
//! Check-In Protocol support (feature_map: 0).
//!
//! The ICD Management cluster is required by some controllers (like Home Assistant)
//! even for always-connected devices. We return values indicating an always-on device:
//! - IdleModeDuration: 1 second (minimum)
//! - ActiveModeDuration: 10000ms (always active)
//! - ActiveModeThreshold: 5000ms

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use log::error;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

use rs_matter::error::{Error, ErrorCode};

/// File name used for ICD persistence (stored in the same dir as the main PSM file).
pub const ICD_STATE_FILE: &str = "icd_state.json";

/// Persisted ICD state (counter only for always-on devices).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct IcdState {
    pub icd_counter: u32,
}

/// Store for ICD state with persistence notification.
pub struct IcdStore {
    path: PathBuf,
    state: RwLock<IcdState>,
    notify: Notify,
    pending_persist: AtomicBool,
}

impl IcdStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            state: RwLock::new(IcdState::default()),
            notify: Notify::new(),
            pending_persist: AtomicBool::new(false),
        }
    }

    /// Load persisted ICD state if present.
    pub fn load(&self) -> Result<(), Error> {
        if let Some(parent) = self.path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            error!("Failed to create ICD persist dir {:?}: {}", parent, e);
        }

        match fs::read(&self.path) {
            Ok(bytes) => match serde_json::from_slice::<IcdState>(&bytes) {
                Ok(state) => {
                    *self.state.write() = state;
                    Ok(())
                }
                Err(e) => {
                    error!("Failed to parse ICD state: {}", e);
                    Err(ErrorCode::StdIoError.into())
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => {
                error!("Failed to read ICD state: {}", e);
                Err(ErrorCode::StdIoError.into())
            }
        }
    }

    /// Persist ICD state to disk.
    fn persist(&self) -> Result<(), Error> {
        let state = self.state.read().clone();
        let data = serde_json::to_vec_pretty(&state).map_err(|_| ErrorCode::StdIoError)?;
        if let Some(parent) = self.path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            error!("Failed to create ICD persist dir {:?}: {}", parent, e);
        }
        fs::write(&self.path, data).map_err(|_| ErrorCode::StdIoError.into())
    }

    /// Run background persistence when state changes.
    pub async fn run(&self) -> Result<(), Error> {
        loop {
            self.notify.notified().await;
            if self.pending_persist.swap(false, Ordering::SeqCst)
                && let Err(e) = self.persist()
            {
                error!("Failed to persist ICD state: {:?}", e);
            }
        }
    }

    fn mark_dirty(&self) {
        self.pending_persist.store(true, Ordering::SeqCst);
        self.notify.notify_one();
    }

    pub fn icd_counter(&self) -> u32 {
        self.state.read().icd_counter
    }

    /// Increment the counter and schedule persistence.
    pub fn next_counter(&self) -> u32 {
        let mut state = self.state.write();
        state.icd_counter = state.icd_counter.wrapping_add(1);
        let counter = state.icd_counter;
        drop(state);
        self.mark_dirty();
        counter
    }
}
