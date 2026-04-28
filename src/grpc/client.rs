use tonic::transport::Channel;

use crate::grpc::proto::pskk_service_client::PskkServiceClient;
use crate::grpc::proto::{Empty, KeyEvent, KeyModifiers, SetModeRequest};

/// PSKK gRPC Client
pub struct PSKKClient {
    client: PskkServiceClient<Channel>,
}

impl PSKKClient {
    /// Connect to PSKK server
    pub async fn connect(addr: String) -> Result<Self, String> {
        let client = PskkServiceClient::connect(addr)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Self { client })
    }

    /// Process a key event
    pub async fn process_key(
        &mut self,
        key_char: Option<char>,
        key_name: String,
        is_pressed: bool,
        shift: bool,
        ctrl: bool,
        alt: bool,
    ) -> Result<crate::grpc::proto::EngineOutput, String> {
        let request = tonic::Request::new(KeyEvent {
            key_char: key_char.map(|c| c.to_string()),
            key_name,
            is_pressed,
            modifiers: Some(KeyModifiers { shift, ctrl, alt }),
        });

        let response = self.client.process_key(request)
            .await
            .map_err(|e| e.to_string())?;
        Ok(response.into_inner())
    }

    /// Set input mode
    pub async fn set_mode(
        &mut self,
        mode: crate::grpc::proto::InputMode,
    ) -> Result<crate::grpc::proto::EngineOutput, String> {
        let request = tonic::Request::new(SetModeRequest { mode: mode as i32 });

        let response = self.client.set_mode(request)
            .await
            .map_err(|e| e.to_string())?;
        Ok(response.into_inner())
    }

    /// Get current input mode
    pub async fn get_mode(
        &mut self,
    ) -> Result<crate::grpc::proto::ModeResponse, String> {
        let request = tonic::Request::new(Empty {});

        let response = self.client.get_mode(request)
            .await
            .map_err(|e| e.to_string())?;
        Ok(response.into_inner())
    }

    /// Handle focus out
    pub async fn focus_out(
        &mut self,
    ) -> Result<crate::grpc::proto::EngineOutput, String> {
        let request = tonic::Request::new(Empty {});

        let response = self.client.focus_out(request)
            .await
            .map_err(|e| e.to_string())?;
        Ok(response.into_inner())
    }

    /// Reset engine state
    pub async fn reset(&mut self) -> Result<(), String> {
        let request = tonic::Request::new(Empty {});

        self.client.reset(request)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Get the current configuration
    pub async fn get_config(&mut self) -> Result<String, String> {
        let request = tonic::Request::new(Empty {});

        let response = self.client.get_config(request)
            .await
            .map_err(|e| e.to_string())?;
        Ok(response.into_inner().config_json)
    }
}
