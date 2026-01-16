//! Single instance lock using Unix socket.
//!
//! Prevents multiple instances of the bridge from running simultaneously.
//! Uses a Unix socket which is automatically cleaned up by the OS when the
//! process dies, avoiding stale lock files.

use std::io;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use thiserror::Error;

/// Error types for instance lock operations.
#[derive(Debug, Error)]
pub enum InstanceLockError {
    /// Another instance is already running.
    #[error("another instance is already running")]
    AlreadyRunning,

    /// I/O error during lock acquisition.
    #[error("failed to acquire instance lock: {0}")]
    Io(#[from] io::Error),
}

/// Single instance lock using a Unix socket.
///
/// The lock is held as long as this struct exists. When dropped, the socket
/// file is removed. If the process crashes, the OS automatically removes
/// the socket, preventing stale locks.
pub struct InstanceLock {
    _listener: UnixListener,
    path: PathBuf,
}

impl InstanceLock {
    /// Attempt to acquire the instance lock.
    ///
    /// Returns `Ok(InstanceLock)` if this is the only instance running.
    /// Returns `Err(InstanceLockError::AlreadyRunning)` if another instance holds the lock.
    pub fn acquire() -> Result<Self, InstanceLockError> {
        let path = Self::socket_path();

        // Remove stale socket if it exists but no process holds it
        // This handles the case where the process was SIGKILL'd and
        // the Drop handler never ran, but the OS released the socket
        if path.exists() {
            // Try to connect - if it fails, the socket is stale
            match std::os::unix::net::UnixStream::connect(&path) {
                Ok(_) => {
                    // Connection succeeded - another instance is running
                    return Err(InstanceLockError::AlreadyRunning);
                }
                Err(_) => {
                    // Connection failed - socket is stale, remove it
                    let _ = std::fs::remove_file(&path);
                }
            }
        }

        // Try to bind the socket
        match UnixListener::bind(&path) {
            Ok(listener) => Ok(Self {
                _listener: listener,
                path,
            }),
            Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
                // Race condition: another instance bound between our check and bind
                Err(InstanceLockError::AlreadyRunning)
            }
            Err(e) => Err(InstanceLockError::Io(e)),
        }
    }

    /// Get the path to the socket file.
    pub fn socket_path() -> PathBuf {
        // Use XDG_RUNTIME_DIR if available (auto-cleaned on logout)
        // Fallback to /tmp
        std::env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
            .join("virtual-matter-bridge.sock")
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        // Clean up the socket file on normal exit
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_path_uses_xdg_runtime_dir() {
        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");
        let path = InstanceLock::socket_path();
        assert_eq!(
            path,
            PathBuf::from("/run/user/1000/virtual-matter-bridge.sock")
        );
        std::env::remove_var("XDG_RUNTIME_DIR");
    }

    #[test]
    fn test_socket_path_fallback_to_tmp() {
        std::env::remove_var("XDG_RUNTIME_DIR");
        let path = InstanceLock::socket_path();
        assert_eq!(path, PathBuf::from("/tmp/virtual-matter-bridge.sock"));
    }
}
