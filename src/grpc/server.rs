use std::sync::{Arc, Mutex};
use tonic::{transport::Server, Request, Response, Status};

use crate::engine::PSKKEngine;
use crate::grpc::conversion::engine_output_to_proto;
use crate::grpc::proto::pskk_service_server::{PskkService, PskkServiceServer};
use crate::grpc::proto::{
    ConfigResponse, DictionarySizeResponse, Empty, EngineOutput, KeyEvent, ModeResponse, SetModeRequest,
};
use crate::henkan::HenkanProcessor;
use crate::kanchoku::KanchokuProcessor;
use crate::simultaneous_processor::SimultaneousInputProcessor;
use crate::util::{get_dictionary_files, load_and_merge_dictionary_files};

/// PSKK gRPC Service Implementation
pub struct PSKKServiceImpl {
    engine: Arc<Mutex<PSKKEngine>>,
}

impl PSKKServiceImpl {
    pub fn new() -> Result<Self, String> {
        // Layout will be loaded by the engine from config
        let simul = SimultaneousInputProcessor::new(None);
        let kanchoku = KanchokuProcessor::new(None);

        // Load dictionary from JSON files
        let dictionary_files = get_dictionary_files(None);
        let dictionary = load_and_merge_dictionary_files(&dictionary_files)
            .map_err(|e| format!("Failed to load dictionary: {}", e))?;

        let henkan = HenkanProcessor::new().with_dictionary(dictionary);
        let engine = PSKKEngine::new(simul, kanchoku, henkan)?;

        Ok(Self {
            engine: Arc::new(Mutex::new(engine)),
        })
    }
}

#[tonic::async_trait]
impl PskkService for PSKKServiceImpl {
    async fn process_key(
        &self,
        request: Request<KeyEvent>,
    ) -> Result<Response<EngineOutput>, Status> {
        let key_event = request.into_inner();

        let key_char = key_event.key_char.and_then(|s| s.chars().next());

        let mut engine = self
            .engine
            .lock()
            .map_err(|e| Status::internal(format!("Failed to lock engine: {}", e)))?;

        let output = engine.process_key_event(
            key_char,
            &key_event.key_name,
            key_event.is_pressed,
            key_event.modifiers,
        );

        Ok(Response::new(engine_output_to_proto(output)))
    }

    async fn set_mode(
        &self,
        request: Request<SetModeRequest>,
    ) -> Result<Response<EngineOutput>, Status> {
        let req = request.into_inner();
        let mode = crate::grpc::proto::InputMode::try_from(req.mode)
            .map_err(|_| Status::invalid_argument("Invalid input mode"))?;

        let mut engine = self
            .engine
            .lock()
            .map_err(|e| Status::internal(format!("Failed to lock engine: {}", e)))?;

        let output = engine.set_mode(mode);

        Ok(Response::new(engine_output_to_proto(output)))
    }

    async fn get_mode(&self, _request: Request<Empty>) -> Result<Response<ModeResponse>, Status> {
        let engine = self
            .engine
            .lock()
            .map_err(|e| Status::internal(format!("Failed to lock engine: {}", e)))?;

        let mode = engine.get_mode();

        Ok(Response::new(ModeResponse { mode: mode as i32 }))
    }

    async fn focus_out(&self, _request: Request<Empty>) -> Result<Response<EngineOutput>, Status> {
        let mut engine = self
            .engine
            .lock()
            .map_err(|e| Status::internal(format!("Failed to lock engine: {}", e)))?;

        let output = engine.focus_out();

        Ok(Response::new(engine_output_to_proto(output)))
    }

    async fn reset(&self, _request: Request<Empty>) -> Result<Response<Empty>, Status> {
        let mut engine = self
            .engine
            .lock()
            .map_err(|e| Status::internal(format!("Failed to lock engine: {}", e)))?;

        engine.reset_state();

        Ok(Response::new(Empty {}))
    }

    async fn get_config(&self, _request: Request<Empty>) -> Result<Response<ConfigResponse>, Status> {
        let engine = self
            .engine
            .lock()
            .map_err(|e| Status::internal(format!("Failed to lock engine: {}", e)))?;

        let config = engine.get_config();
        let config_json = serde_json::to_string_pretty(config)
            .map_err(|e| Status::internal(format!("Failed to serialize config: {}", e)))?;

        Ok(Response::new(ConfigResponse { config_json }))
    }

    async fn reload_config(&self, _request: Request<Empty>) -> Result<Response<Empty>, Status> {
        let mut engine = self
            .engine
            .lock()
            .map_err(|e| Status::internal(format!("Failed to lock engine: {}", e)))?;

        engine.reload_config()
            .map_err(|e| Status::internal(format!("Failed to reload config: {}", e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn get_dictionary_size(&self, _request: Request<Empty>) -> Result<Response<DictionarySizeResponse>, Status> {
        let engine = self
            .engine
            .lock()
            .map_err(|e| Status::internal(format!("Failed to lock engine: {}", e)))?;

        let size = engine.get_dictionary_size();

        Ok(Response::new(DictionarySizeResponse { size: size as u32 }))
    }
}

/// Run the gRPC server
pub async fn run_server(addr: std::net::SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let service = PSKKServiceImpl::new()
        .map_err(|e| format!("Failed to create service: {}", e))?;

    Server::builder()
        .add_service(PskkServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
