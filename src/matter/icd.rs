//! ICD Check-In state, persistence, and runtime helpers.
//!
//! This module stores registered ICD clients (per fabric), the persistent
//! check-in counter, and exposes helpers used by the ICD Management cluster
//! handler plus the startup check-in engine.

use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, SystemTime};

use log::{error, info, warn};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::sync::Notify;

use rs_matter::crypto;
use rs_matter::error::{Error, ErrorCode};

/// Default maximum clients per fabric we allow to register.
pub const DEFAULT_CLIENTS_SUPPORTED_PER_FABRIC: u16 = 4;
/// Default maximum check-in backoff (seconds) used to rate-limit announcements.
pub const DEFAULT_MAX_CHECK_IN_BACKOFF_SECS: u32 = 900;
/// File name used for ICD persistence (stored in the same dir as the main PSM file).
pub const ICD_STATE_FILE: &str = "icd_state.json";

/// Client type as defined by the ICD Management spec.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum IcdClientType {
    Permanent = 0,
    Ephemeral = 1,
}

impl IcdClientType {
    pub fn from_u8(v: u8) -> Result<Self, Error> {
        match v {
            0 => Ok(IcdClientType::Permanent),
            1 => Ok(IcdClientType::Ephemeral),
            _ => Err(ErrorCode::ConstraintError.into()),
        }
    }
}

/// Registered ICD client entry (fabric scoped).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IcdRegisteredClient {
    pub fabric_index: u8,
    pub check_in_node_id: u64,
    pub monitored_subject: u64,
    pub shared_key: [u8; 16],
    pub verification_key: Option<[u8; 16]>,
    pub client_type: IcdClientType,
    /// Optional stay-active deadline as a UNIX timestamp (secs).
    pub stay_active_until: Option<u64>,
    /// Controller address for Check-In messages (captured from registration context)
    #[serde(default)]
    pub controller_addr: Option<String>,
}

/// ICD Check-In message constants (Matter spec Section 9.17.9)
pub mod check_in {
    /// Check-In message application payload size (counter + active_mode_threshold)
    pub const PAYLOAD_SIZE: usize = 6; // 4 bytes counter + 2 bytes threshold
    /// Check-In nonce size for AES-CCM
    pub const NONCE_SIZE: usize = 13;
    /// Check-In MIC (tag) size
    pub const MIC_SIZE: usize = 8; // 64-bit MIC for Check-In
    /// Total encrypted message size
    pub const ENCRYPTED_SIZE: usize = PAYLOAD_SIZE + MIC_SIZE;
    /// Active mode threshold we report (5000ms)
    pub const ACTIVE_MODE_THRESHOLD: u16 = 5000;
}

/// Persisted ICD state.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct IcdCheckInState {
    pub icd_counter: u32,
    pub clients: Vec<IcdRegisteredClient>,
}

/// Store for ICD state with persistence notification.
pub struct IcdStore {
    path: PathBuf,
    state: RwLock<IcdCheckInState>,
    notify: Notify,
    pending_persist: AtomicBool,
}

impl IcdStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            state: RwLock::new(IcdCheckInState::default()),
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
            Ok(bytes) => match serde_json::from_slice::<IcdCheckInState>(&bytes) {
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

    pub fn registered_clients_for_fabric(&self, fabric_index: u8) -> Vec<IcdRegisteredClient> {
        self.state
            .read()
            .clients
            .iter()
            .filter(|c| c.fabric_index == fabric_index)
            .cloned()
            .collect()
    }

    pub fn all_clients(&self) -> Vec<IcdRegisteredClient> {
        self.state.read().clients.clone()
    }

    pub fn register_client(
        &self,
        fabric_index: u8,
        check_in_node_id: u64,
        monitored_subject: u64,
        shared_key: [u8; 16],
        verification_key: Option<[u8; 16]>,
        client_type: IcdClientType,
    ) -> u32 {
        let mut state = self.state.write();
        state.clients.retain(|c| {
            !(c.fabric_index == fabric_index && c.check_in_node_id == check_in_node_id)
        });
        state.clients.push(IcdRegisteredClient {
            fabric_index,
            check_in_node_id,
            monitored_subject,
            shared_key,
            verification_key,
            client_type,
            stay_active_until: None,
            controller_addr: None, // Will be set separately if needed
        });
        let counter = state.icd_counter;
        drop(state);
        self.mark_dirty();
        counter
    }

    /// Update the controller address for a registered client
    pub fn set_controller_addr(&self, fabric_index: u8, check_in_node_id: u64, addr: String) {
        let mut state = self.state.write();
        for client in state.clients.iter_mut() {
            if client.fabric_index == fabric_index && client.check_in_node_id == check_in_node_id {
                client.controller_addr = Some(addr.clone());
                break;
            }
        }
        drop(state);
        self.mark_dirty();
    }

    pub fn unregister_client(
        &self,
        fabric_index: u8,
        check_in_node_id: u64,
        verification_key: Option<[u8; 16]>,
    ) -> bool {
        let mut state = self.state.write();
        let before = state.clients.len();
        state.clients.retain(|c| {
            if c.fabric_index != fabric_index || c.check_in_node_id != check_in_node_id {
                return true;
            }
            if let Some(ref vkey) = verification_key
                && let Some(existing) = &c.verification_key
            {
                return existing != vkey;
            }
            false
        });
        let removed = state.clients.len() < before;
        if removed {
            self.mark_dirty();
        }
        removed
    }

    pub fn stay_active_until(&self, fabric_index: u8, duration_ms: u32) -> u32 {
        let until = SystemTime::now()
            .checked_add(Duration::from_millis(duration_ms as u64))
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        let mut state = self.state.write();
        for client in state
            .clients
            .iter_mut()
            .filter(|c| c.fabric_index == fabric_index)
        {
            client.stay_active_until = until;
        }
        drop(state);
        self.mark_dirty();
        duration_ms
    }

    pub fn clients_supported_per_fabric(&self) -> u16 {
        DEFAULT_CLIENTS_SUPPORTED_PER_FABRIC
    }

    pub fn maximum_check_in_backoff(&self) -> u32 {
        DEFAULT_MAX_CHECK_IN_BACKOFF_SECS
    }
}

/// Build and encrypt a Check-In message for a client
///
/// Check-In message format (Matter spec 9.17.9):
/// - Nonce: 13 bytes derived from counter and node context
/// - Payload: counter (u32 LE) + active_mode_threshold (u16 LE)
/// - Encrypted with AES-CCM using the client's shared key
fn build_check_in_message(client: &IcdRegisteredClient, counter: u32) -> Result<Vec<u8>, Error> {
    // Build the plaintext payload: counter (4 bytes) + active_mode_threshold (2 bytes)
    let mut payload = [0u8; check_in::ENCRYPTED_SIZE];
    payload[0..4].copy_from_slice(&counter.to_le_bytes());
    payload[4..6].copy_from_slice(&check_in::ACTIVE_MODE_THRESHOLD.to_le_bytes());

    // Build the nonce: fabric_index (1) + monitored_subject (8) + counter (4) = 13 bytes
    let mut nonce = [0u8; check_in::NONCE_SIZE];
    nonce[0] = client.fabric_index;
    nonce[1..9].copy_from_slice(&client.monitored_subject.to_le_bytes());
    nonce[9..13].copy_from_slice(&counter.to_le_bytes());

    // Empty associated data for Check-In
    let ad: &[u8] = &[];

    // Encrypt in place (payload becomes ciphertext + MIC)
    let encrypted_len = crypto::encrypt_in_place(
        &client.shared_key,
        &nonce,
        ad,
        &mut payload,
        check_in::PAYLOAD_SIZE,
    )?;

    Ok(payload[..encrypted_len].to_vec())
}

/// Send Check-In message to a specific address
async fn send_check_in_to_addr(
    socket: &UdpSocket,
    addr: &SocketAddr,
    message: &[u8],
) -> Result<(), std::io::Error> {
    socket.send_to(message, addr).await?;
    Ok(())
}

/// Emit startup check-ins for all registered clients
///
/// For each client with a known address, we:
/// 1. Build an encrypted Check-In message
/// 2. Send it via UDP to the controller
/// 3. Increment the ICD counter
///
/// The controller should respond by initiating a new CASE session.
pub async fn run_startup_checkins(
    store: Arc<IcdStore>,
    local_addr: Option<SocketAddr>,
) -> Result<(), Error> {
    let clients = store.all_clients();

    if clients.is_empty() {
        info!("No registered ICD clients - skipping Check-In messages");
        return Ok(());
    }

    info!(
        "Sending ICD Check-In messages to {} registered client(s)",
        clients.len()
    );

    // Create a UDP socket for sending Check-In messages
    let socket = match local_addr {
        Some(addr) => {
            // Bind to a random port on the same interface
            let bind_addr: SocketAddr = match addr {
                SocketAddr::V4(_) => "0.0.0.0:0".parse().unwrap(),
                SocketAddr::V6(_) => "[::]:0".parse().unwrap(),
            };
            UdpSocket::bind(bind_addr).await.map_err(|e| {
                error!("Failed to bind Check-In socket: {}", e);
                ErrorCode::StdIoError
            })?
        }
        None => {
            // Default to IPv6 any
            UdpSocket::bind("[::]:0").await.map_err(|e| {
                error!("Failed to bind Check-In socket: {}", e);
                ErrorCode::StdIoError
            })?
        }
    };

    for client in &clients {
        let counter = store.next_counter();

        // Try to get controller address
        let addr_str = match &client.controller_addr {
            Some(addr) => addr.clone(),
            None => {
                // If no address stored, we can't send Check-In
                // The controller should re-establish via mDNS discovery
                warn!(
                    "ICD Check-In: No address for fabric {} node {:016x} - relying on mDNS",
                    client.fabric_index, client.check_in_node_id
                );
                continue;
            }
        };

        // Parse the address
        let target_addr: SocketAddr = match addr_str.parse() {
            Ok(addr) => addr,
            Err(e) => {
                warn!(
                    "ICD Check-In: Invalid address '{}' for fabric {}: {}",
                    addr_str, client.fabric_index, e
                );
                continue;
            }
        };

        // Build encrypted Check-In message
        match build_check_in_message(client, counter) {
            Ok(message) => {
                info!(
                    "ICD Check-In: Sending to {} (fabric {}, node {:016x}, counter {})",
                    target_addr, client.fabric_index, client.check_in_node_id, counter
                );

                if let Err(e) = send_check_in_to_addr(&socket, &target_addr, &message).await {
                    warn!("ICD Check-In: Failed to send to {}: {}", target_addr, e);
                }
            }
            Err(e) => {
                warn!(
                    "ICD Check-In: Failed to build message for fabric {}: {:?}",
                    client.fabric_index, e
                );
            }
        }
    }

    info!("ICD Check-In: All messages sent, waiting for controllers to establish CASE");
    Ok(())
}
