use std::sync::{Arc, Mutex};
use tonic::{transport::Server, Request, Response, Status};

use crate::engine::PSKKEngine;
use crate::grpc::conversion::{
    engine_output_to_proto, input_mode_from_proto, key_modifiers_from_proto,
    mode_response_from_rust,
};
use crate::grpc::proto::pskk_service_server::{PskkService, PskkServiceServer};
use crate::grpc::proto::{ConfigResponse, Empty, EngineOutput, KeyEvent, ModeResponse, SetModeRequest};
use crate::henkan::HenkanProcessor;
use crate::kanchoku::KanchokuProcessor;
use crate::settings::load_current_kanchoku_mappings;
use crate::simultaneous_processor::SimultaneousInputProcessor;
use crate::util::get_config_data;

/// PSKK gRPC Service Implementation
pub struct PSKKServiceImpl {
    engine: Arc<Mutex<PSKKEngine>>,
}

impl PSKKServiceImpl {
    pub fn new() -> Self {
        // Load config and create engine
        let (config, layout) = match get_config_data() {
            Ok((config, _warnings)) => {
                let layout = match load_current_kanchoku_mappings(&config) {
                    Ok(mappings) => mappings
                        .into_iter()
                        .map(|m| {
                            let input = format!("{}{}", m.first_key, m.second_key);
                            (input, m.kanji, String::new(), None)
                        })
                        .collect(),
                    Err(e) => {
                        eprintln!("Failed to load kanchoku layout: {}, using default", e);
                        Self::default_layout()
                    }
                };

                (config, layout)
            }
            Err(e) => {
                eprintln!("Failed to load config: {}, using defaults", e);
                (serde_json::json!({}), Self::default_layout())
            }
        };

        let simul = SimultaneousInputProcessor::new(Some(layout));
        let kanchoku = KanchokuProcessor::new(None);
        let henkan = HenkanProcessor::new();
        let engine = PSKKEngine::new(simul, kanchoku, henkan, &config);

        Self {
            engine: Arc::new(Mutex::new(engine)),
        }
    }

    fn default_layout() -> Vec<(String, String, String, Option<u64>)> {
        vec![
            ("a".to_string(), "あ".to_string(), "".to_string(), None),
            ("i".to_string(), "い".to_string(), "".to_string(), None),
            ("u".to_string(), "う".to_string(), "".to_string(), None),
            ("e".to_string(), "え".to_string(), "".to_string(), None),
            ("o".to_string(), "お".to_string(), "".to_string(), None),
            ("ka".to_string(), "か".to_string(), "".to_string(), None),
            ("ki".to_string(), "き".to_string(), "".to_string(), None),
            ("ku".to_string(), "く".to_string(), "".to_string(), None),
            ("ke".to_string(), "け".to_string(), "".to_string(), None),
            ("ko".to_string(), "こ".to_string(), "".to_string(), None),
        ]
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
        let (shift, ctrl, alt) = key_modifiers_from_proto(key_event.modifiers);

        let mut engine = self
            .engine
            .lock()
            .map_err(|e| Status::internal(format!("Failed to lock engine: {}", e)))?;

        let output = engine.process_key_event(
            key_char,
            &key_event.key_name,
            key_event.is_pressed,
            shift,
            ctrl,
            alt,
        );

        Ok(Response::new(engine_output_to_proto(output)))
    }

    async fn set_mode(
        &self,
        request: Request<SetModeRequest>,
    ) -> Result<Response<EngineOutput>, Status> {
        let req = request.into_inner();
        let mode = input_mode_from_proto(
            crate::grpc::proto::InputMode::try_from(req.mode)
                .map_err(|_| Status::invalid_argument("Invalid input mode"))?,
        );

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

        Ok(Response::new(mode_response_from_rust(mode)))
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
}

/// Run the gRPC server
pub async fn run_server(addr: std::net::SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let service = PSKKServiceImpl::new();

    Server::builder()
        .add_service(PskkServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
