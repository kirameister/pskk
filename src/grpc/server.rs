use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tonic::{transport::Server, Request, Response, Status};

use crate::engine::PSKKEngine;
use crate::grpc::conversion::engine_output_to_proto;
use crate::grpc::proto::pskk_service_server::{PskkService, PskkServiceServer};
use crate::grpc::proto::{
    ConfigResponse, DictionarySizeResponse, Empty, EngineOutput, InputMode, KeyEvent, ModeResponse,
    SetModeRequest,
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
        Ok(Self {
            engine: Arc::new(Mutex::new(Self::build_engine()?)),
        })
    }

    /// Build a shared engine instance. Layouts are loaded from config inside
    /// `PSKKEngine::new`; the kana-to-kanji dictionary is loaded in the
    /// background by [`PSKKServiceImpl::spawn_dictionary_load`] so listeners
    /// can bind their ports without waiting for startup.
    pub(crate) fn build_engine() -> Result<PSKKEngine, String> {
        let simul = SimultaneousInputProcessor::new(None);
        let kanchoku = KanchokuProcessor::new(None);
        let henkan = HenkanProcessor::new();
        PSKKEngine::new(simul, kanchoku, henkan)
    }

    /// Wrap an already-created shared engine (used when gRPC and the JSON
    /// server must serve the *same* engine instance).
    pub fn from_engine(engine: Arc<Mutex<PSKKEngine>>) -> Self {
        Self { engine }
    }

    /// Shared access to the underlying engine (used by the JSON server too).
    pub(crate) fn engine(&self) -> Arc<Mutex<PSKKEngine>> {
        self.engine.clone()
    }

    /// Start loading the kana-to-kanji dictionary in the background so the
    /// listeners can bind immediately. Requests that need the dictionary
    /// return HENKAN_UNAVAILABLE until the load completes.
    pub fn spawn_dictionary_load(engine: Arc<Mutex<PSKKEngine>>) {
        tokio::task::spawn_blocking(move || {
            let dictionary_files = get_dictionary_files(None);
            match load_and_merge_dictionary_files(&dictionary_files) {
                Ok(dictionary) => {
                    if let Ok(mut engine) = engine.lock() {
                        engine.load_henkan_dictionary(dictionary);
                        eprintln!("Dictionary loaded successfully");
                    } else {
                        eprintln!("Failed to lock engine to load dictionary");
                    }
                }
                Err(e) => {
                    eprintln!("Failed to load dictionary: {}", e);
                }
            }
        });
    }

    // ------------------------------------------------------------------
    // Synchronous core handlers. These are shared between the gRPC service
    // and the JSON/TCP listener so both frontends behave identically.
    // ------------------------------------------------------------------

    fn lock_engine(&self) -> Result<std::sync::MutexGuard<'_, PSKKEngine>, String> {
        self.engine
            .lock()
            .map_err(|e| format!("Failed to lock engine: {}", e))
    }

    pub(crate) fn handle_process_key(&self, key_event: KeyEvent) -> Result<EngineOutput, String> {
        let key_char = if key_event.key_char.is_empty() {
            None
        } else {
            key_event.key_char.chars().next()
        };

        let key_name = key_event.key_name.clone();
        let is_pressed = key_event.is_pressed;

        let output = {
            let mut engine = self.lock_engine()?;
            engine.process_key_event(
                key_char,
                &key_event.key_name,
                key_event.is_pressed,
                key_event.modifiers,
            )
        };

        // The engine mutex guard above is dropped before this write, so a blocked
        // stderr (e.g. an undrained pipe) can no longer wedge every RPC.
        eprintln!(
            "Key event: key_name='{}', is_pressed={}",
            key_name, is_pressed
        );
        Ok(engine_output_to_proto(output))
    }

    pub(crate) fn handle_set_mode(&self, req: SetModeRequest) -> Result<EngineOutput, String> {
        let mode = InputMode::try_from(req.mode).map_err(|_| "Invalid input mode".to_string())?;
        let mut engine = self.lock_engine()?;
        let output = engine.set_mode(mode);
        Ok(engine_output_to_proto(output))
    }

    pub(crate) fn handle_get_mode(&self) -> Result<ModeResponse, String> {
        let engine = self.lock_engine()?;
        let mode = engine.get_mode();
        Ok(ModeResponse { mode: mode as i32 })
    }

    pub(crate) fn handle_focus_out(&self) -> Result<EngineOutput, String> {
        let mut engine = self.lock_engine()?;
        let output = engine.focus_out();
        Ok(engine_output_to_proto(output))
    }

    pub(crate) fn handle_reset(&self) -> Result<(), String> {
        let mut engine = self.lock_engine()?;
        engine.reset_state();
        Ok(())
    }

    pub(crate) fn handle_get_config(&self) -> Result<ConfigResponse, String> {
        let engine = self.lock_engine()?;
        let config = engine.get_config();
        let config_json = serde_json::to_string_pretty(config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        Ok(ConfigResponse { config_json })
    }

    pub(crate) fn handle_reload_config(&self) -> Result<(), String> {
        let mut engine = self.lock_engine()?;
        engine.reload_config()
    }

    pub(crate) fn handle_get_dictionary_size(&self) -> Result<DictionarySizeResponse, String> {
        let engine = self.lock_engine()?;
        let size = engine.get_dictionary_size();
        Ok(DictionarySizeResponse { size: size as u32 })
    }
}

#[tonic::async_trait]
impl PskkService for PSKKServiceImpl {
    async fn process_key(
        &self,
        request: Request<KeyEvent>,
    ) -> Result<Response<EngineOutput>, Status> {
        let key_event = request.into_inner();
        self.handle_process_key(key_event)
            .map(Response::new)
            .map_err(|e| Status::internal(e))
    }

    async fn set_mode(
        &self,
        request: Request<SetModeRequest>,
    ) -> Result<Response<EngineOutput>, Status> {
        let req = request.into_inner();
        self.handle_set_mode(req)
            .map(Response::new)
            .map_err(|e| Status::invalid_argument(e))
    }

    async fn get_mode(&self, _request: Request<Empty>) -> Result<Response<ModeResponse>, Status> {
        self.handle_get_mode()
            .map(Response::new)
            .map_err(|e| Status::internal(e))
    }

    async fn focus_out(&self, _request: Request<Empty>) -> Result<Response<EngineOutput>, Status> {
        self.handle_focus_out()
            .map(Response::new)
            .map_err(|e| Status::internal(e))
    }

    async fn reset(&self, _request: Request<Empty>) -> Result<Response<Empty>, Status> {
        self.handle_reset()
            .map_err(|e| Status::internal(e))?;
        Ok(Response::new(Empty {}))
    }

    async fn get_config(&self, _request: Request<Empty>) -> Result<Response<ConfigResponse>, Status> {
        self.handle_get_config()
            .map(Response::new)
            .map_err(|e| Status::internal(e))
    }

    async fn reload_config(&self, _request: Request<Empty>) -> Result<Response<Empty>, Status> {
        self.handle_reload_config()
            .map_err(|e| Status::internal(e))?;
        Ok(Response::new(Empty {}))
    }

    async fn get_dictionary_size(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<DictionarySizeResponse>, Status> {
        self.handle_get_dictionary_size()
            .map(Response::new)
            .map_err(|e| Status::internal(e))
    }
}

/// Run only the gRPC server on `addr` (kept for API compatibility).
pub async fn run_server(addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let engine = Arc::new(Mutex::new(
        PSKKServiceImpl::build_engine().map_err(|e| format!("Failed to create engine: {}", e))?,
    ));
    PSKKServiceImpl::spawn_dictionary_load(engine.clone());
    let service = PSKKServiceImpl::from_engine(engine);
    serve_grpc(addr, service).await
}

/// Serve gRPC on `addr` with a pre-built service.
pub async fn serve_grpc(
    addr: SocketAddr,
    service: PSKKServiceImpl,
) -> Result<(), Box<dyn std::error::Error>> {
    Server::builder()
        .add_service(PskkServiceServer::new(service))
        .serve(addr)
        .await?;
    Ok(())
}

/// Run the gRPC server (50051) and the JSON/TCP server (50052) concurrently,
/// sharing one engine instance and one background dictionary load.
pub async fn run_all_servers(
    grpc_addr: SocketAddr,
    json_addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    let engine = Arc::new(Mutex::new(
        PSKKServiceImpl::build_engine().map_err(|e| format!("Failed to create engine: {}", e))?,
    ));
    PSKKServiceImpl::spawn_dictionary_load(engine.clone());

    let grpc_task = serve_grpc(grpc_addr, PSKKServiceImpl::from_engine(engine.clone()));
    let json_task = crate::json::server::run_server(json_addr, PSKKServiceImpl::from_engine(engine));

    tokio::try_join!(grpc_task, json_task)?;
    Ok(())
}
