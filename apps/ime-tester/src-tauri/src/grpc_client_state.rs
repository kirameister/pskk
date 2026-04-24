use pskk::grpc::client::PSKKClient;
use pskk::grpc::proto::{EngineOutput, InputMode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::server_manager::ServerManager;

const SERVER_ADDR: &str = "http://127.0.0.1:50051";
const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// Wrapper around gRPC client for Tauri state management
pub struct GrpcClientState {
    client: Arc<Mutex<Option<PSKKClient>>>,
    server_manager: Arc<ServerManager>,
}

impl GrpcClientState {
    pub fn new() -> Self {
        Self {
            client: Arc::new(Mutex::new(None)),
            server_manager: Arc::new(ServerManager::new()),
        }
    }

    /// Connect to the gRPC server (auto-starts server if needed)
    pub async fn connect(&self) -> Result<(), String> {
        // First, ensure server is running
        self.server_manager.ensure_server_running().await?;
        
        // Try to connect with retries
        let mut last_error = String::new();
        for attempt in 1..=MAX_RETRIES {
            match PSKKClient::connect(SERVER_ADDR.to_string()).await {
                Ok(client) => {
                    // Store client and return immediately
                    let mut client_lock = self.client.lock().await;
                    *client_lock = Some(client);
                    return Ok(());
                }
                Err(e) => {
                    // Convert error to String immediately
                    last_error = format!("{}", e);
                    // Drop e here by ending the match arm
                }
            }
            
            // Result is fully consumed, safe to await
            if attempt < MAX_RETRIES {
                tokio::time::sleep(tokio::time::Duration::from_millis(RETRY_DELAY_MS)).await;
            }
        }
        
        Err(format!("Failed to connect to server at {} after {} attempts: {}", 
            SERVER_ADDR, MAX_RETRIES, last_error))
    }
    
    /// Shutdown the server when closing
    pub async fn shutdown(&self) {
        self.server_manager.shutdown().await;
    }

    /// Check if connected to server
    pub async fn is_connected(&self) -> bool {
        self.client.lock().await.is_some()
    }

    /// Process a key event
    pub async fn process_key(
        &self,
        key_char: Option<char>,
        key_name: String,
        is_pressed: bool,
        modifiers: KeyModifiers,
    ) -> Result<EngineOutput, String> {
        let mut client_lock = self.client.lock().await;
        
        let client = client_lock
            .as_mut()
            .ok_or_else(|| "Not connected to server".to_string())?;

        client
            .process_key(
                key_char,
                key_name,
                is_pressed,
                modifiers.shift,
                modifiers.ctrl,
                modifiers.alt,
            )
            .await
            .map_err(|e| format!("gRPC error: {}", e))
    }

    /// Set input mode
    pub async fn set_mode(&self, mode: InputMode) -> Result<EngineOutput, String> {
        let mut client_lock = self.client.lock().await;
        
        let client = client_lock
            .as_mut()
            .ok_or_else(|| "Not connected to server".to_string())?;

        client
            .set_mode(mode)
            .await
            .map_err(|e| format!("gRPC error: {}", e))
    }

    /// Get current input mode
    pub async fn get_mode(&self) -> Result<InputMode, String> {
        let mut client_lock = self.client.lock().await;
        
        let client = client_lock
            .as_mut()
            .ok_or_else(|| "Not connected to server".to_string())?;

        let response = client
            .get_mode()
            .await
            .map_err(|e| format!("gRPC error: {}", e))?;

        InputMode::try_from(response.mode)
            .map_err(|_| "Invalid mode from server".to_string())
    }

    /// Handle focus out
    pub async fn focus_out(&self) -> Result<EngineOutput, String> {
        let mut client_lock = self.client.lock().await;
        
        let client = client_lock
            .as_mut()
            .ok_or_else(|| "Not connected to server".to_string())?;

        client
            .focus_out()
            .await
            .map_err(|e| format!("gRPC error: {}", e))
    }
}
