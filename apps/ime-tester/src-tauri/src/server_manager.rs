use std::process::{Child, Command};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Manages the PSKK server lifecycle
pub struct ServerManager {
    server_process: Arc<Mutex<Option<Child>>>,
}

impl ServerManager {
    pub fn new() -> Self {
        Self {
            server_process: Arc::new(Mutex::new(None)),
        }
    }

    /// Start the PSKK server if not already running
    pub async fn ensure_server_running(&self) -> Result<(), String> {
        let mut process_lock = self.server_process.lock().await;

        // Check if we already have a server process
        if let Some(child) = process_lock.as_mut() {
            // Check if it's still alive
            match child.try_wait() {
                Ok(None) => {
                    // Still running
                    return Ok(());
                }
                _ => {
                    // Process died, clean up
                    *process_lock = None;
                }
            }
        }

        // Try to find pskk-server in PATH or next to our binary
        let server_path = Self::find_server_binary()?;

        // Start the server
        let child = Command::new(&server_path)
            .spawn()
            .map_err(|e| format!("Failed to start server: {}", e))?;

        *process_lock = Some(child);

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        Ok(())
    }

    /// Find the server binary
    fn find_server_binary() -> Result<String, String> {
        // Try common locations
        let candidates = vec![
            // Development: in target directory
            "target/debug/pskk-server",
            "target/release/pskk-server",
            "../../../target/debug/pskk-server",
            "../../../target/release/pskk-server",
            // System installation
            "/opt/pskk/bin/pskk-server",
            "/usr/local/bin/pskk-server",
            // In PATH
            "pskk-server",
        ];

        for candidate in candidates {
            if let Ok(path) = which::which(candidate) {
                return Ok(path.to_string_lossy().to_string());
            }
        }

        Err("Could not find pskk-server binary. Please ensure it's installed or in PATH.".to_string())
    }

    /// Stop the server when IME tester closes
    pub async fn shutdown(&self) {
        let mut process_lock = self.server_process.lock().await;

        if let Some(mut child) = process_lock.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ServerManager {
    fn drop(&mut self) {
        // Best effort cleanup on drop
        if let Ok(mut process_lock) = self.server_process.try_lock() {
            if let Some(mut child) = process_lock.take() {
                let _ = child.kill();
            }
        }
    }
}
