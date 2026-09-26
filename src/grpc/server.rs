use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{debug, info};

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

/// (path, last-modified time) pairs describing the dictionary files used by a
/// load. `None` mtime means the file did not exist.
type DictionaryFileSnapshot = Vec<(PathBuf, Option<SystemTime>)>;

fn dictionary_file_snapshot(files: &[PathBuf]) -> DictionaryFileSnapshot {
    files
        .iter()
        .map(|path| {
            let mtime = std::fs::metadata(path)
                .and_then(|meta| meta.modified())
                .ok();
            (path.clone(), mtime)
        })
        .collect()
}

/// Bookkeeping shared by the startup load and the background reloads that are
/// triggered when the user switches to direct input.
#[derive(Default)]
pub struct DictionaryReloadState {
    /// True while a load is in flight (prevents overlapping loads).
    loading: AtomicBool,
    /// Snapshot of the files used by the most recent successful load.
    loaded: Mutex<DictionaryFileSnapshot>,
}

impl DictionaryReloadState {
    /// Claim the loader. Returns false when a load is already running.
    fn begin(&self) -> bool {
        !self.loading.swap(true, Ordering::SeqCst)
    }

    fn finish(&self) {
        self.loading.store(false, Ordering::SeqCst);
    }

    fn loaded_snapshot(&self) -> DictionaryFileSnapshot {
        self.loaded.lock().map(|guard| guard.clone()).unwrap_or_default()
    }

    fn record(&self, snapshot: DictionaryFileSnapshot) {
        if let Ok(mut loaded) = self.loaded.lock() {
            *loaded = snapshot;
        }
    }

    /// Whether the given files differ from the last successfully loaded set.
    fn files_changed(&self, files: &[PathBuf]) -> bool {
        dictionary_file_snapshot(files) != self.loaded_snapshot()
    }
}

/// Releases the `loading` claim even if the load panics.
struct LoadingGuard(Arc<DictionaryReloadState>);

impl Drop for LoadingGuard {
    fn drop(&mut self) {
        self.0.finish();
    }
}

/// PSKK gRPC Service Implementation
#[derive(Clone)]
pub struct PSKKServiceImpl {
    engine: Arc<Mutex<PSKKEngine>>,
    dictionary_state: Arc<DictionaryReloadState>,
}

impl PSKKServiceImpl {
    pub fn new() -> Result<Self, String> {
        let engine = Arc::new(Mutex::new(Self::build_engine()?));
        Ok(Self::from_engine(engine))
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
        Self {
            engine,
            dictionary_state: Arc::new(DictionaryReloadState::default()),
        }
    }

    /// Shared dictionary-reload bookkeeping, so the startup load and any
    /// service clones agree on what is currently loaded.
    pub fn dictionary_state(&self) -> Arc<DictionaryReloadState> {
        self.dictionary_state.clone()
    }

    /// Shared access to the underlying engine (used by the JSON server too).
    pub(crate) fn engine(&self) -> Arc<Mutex<PSKKEngine>> {
        self.engine.clone()
    }

    /// Start loading the kana-to-kanji dictionary in the background so the
    /// listeners can bind immediately. Requests that need the dictionary
    /// return HENKAN_UNAVAILABLE until the load completes.
    pub fn spawn_dictionary_load(
        engine: Arc<Mutex<PSKKEngine>>,
        state: Arc<DictionaryReloadState>,
    ) {
        if !state.begin() {
            return;
        }
        tokio::task::spawn_blocking(move || {
            Self::load_dictionary(&engine, &state, false);
        });
    }

    /// Refresh the dictionary if its files changed since the last load. Called
    /// when the input mode switches to direct input, so the IO-heavy work runs
    /// while the user is not composing Japanese.
    fn spawn_dictionary_reload_if_changed(&self) {
        let state = self.dictionary_state.clone();
        if !state.begin() {
            debug!("Dictionary reload already in progress; skipping");
            return;
        }
        let engine = self.engine.clone();
        // Blocking file IO on a plain thread: works no matter which runtime the
        // caller runs on (gRPC and the JSON/TCP server both reach this code).
        std::thread::spawn(move || {
            Self::load_dictionary(&engine, &state, true);
        });
    }

    /// Load, merge and install the dictionary. With `only_if_changed` the load
    /// is skipped unless the dictionary files changed since the last successful
    /// load, so a mode switch is usually just a few `stat` calls.
    fn load_dictionary(
        engine: &Arc<Mutex<PSKKEngine>>,
        state: &Arc<DictionaryReloadState>,
        only_if_changed: bool,
    ) {
        let _guard = LoadingGuard(state.clone());

        // Snapshot *before* reading: a file modified while we load then still
        // differs from the recorded snapshot, so the next check reloads again.
        let files = get_dictionary_files(None);
        let snapshot = dictionary_file_snapshot(&files);

        if only_if_changed && !state.files_changed(&files) {
            debug!("Dictionary files unchanged; skipping reload");
            return;
        }

        match load_and_merge_dictionary_files(&files) {
            Ok(dictionary) => match engine.lock() {
                Ok(mut engine) => {
                    engine.load_henkan_dictionary(dictionary);
                    state.record(snapshot);
                    info!("Dictionary loaded from {} file(s)", files.len());
                }
                Err(_) => eprintln!("Failed to lock engine to load dictionary"),
            },
            Err(e) => eprintln!("Failed to load dictionary: {}", e),
        }
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

        let (output, reload_requested) = {
            let mut engine = self.lock_engine()?;
            let output = engine.process_key_event(
                key_char,
                &key_event.key_name,
                key_event.is_pressed,
                key_event.modifiers,
            );
            let reload = engine.take_dictionary_reload_request();
            (output, reload)
        };

        if reload_requested {
            self.spawn_dictionary_reload_if_changed();
        }

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

        let (output, reload_requested) = {
            let mut engine = self.lock_engine()?;
            let output = engine.set_mode(mode);
            let reload = engine.take_dictionary_reload_request();
            (output, reload)
        };

        if reload_requested {
            self.spawn_dictionary_reload_if_changed();
        }

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
    let service = PSKKServiceImpl::from_engine(engine);
    PSKKServiceImpl::spawn_dictionary_load(service.engine(), service.dictionary_state());
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
    let service = PSKKServiceImpl::from_engine(engine);
    PSKKServiceImpl::spawn_dictionary_load(service.engine(), service.dictionary_state());

    let grpc_task = serve_grpc(grpc_addr, service.clone());
    let json_task = crate::json::server::run_server(json_addr, service);

    tokio::try_join!(grpc_task, json_task)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("pskk-dict-test-{}-{}", std::process::id(), name));
        path
    }

    #[test]
    fn snapshot_detects_created_and_modified_files() {
        let path = temp_path("snapshot.json");
        let _ = std::fs::remove_file(&path);
        let files = vec![path.clone()];

        // A missing file records no modification time
        let missing = dictionary_file_snapshot(&files);
        assert_eq!(missing.len(), 1);
        assert!(missing[0].1.is_none());

        // Creating the file is a change
        std::fs::write(&path, b"{}").unwrap();
        let created = dictionary_file_snapshot(&files);
        assert!(created[0].1.is_some());
        assert_ne!(missing, created);

        // Modifying the file is a change (sleep so the mtime differs)
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&path, b"{\"a\":1}").unwrap();
        let modified = dictionary_file_snapshot(&files);
        assert_ne!(created, modified);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reload_state_tracks_file_changes() {
        let state = DictionaryReloadState::default();
        let path = temp_path("changes.json");
        let _ = std::fs::remove_file(&path);
        let files = vec![path.clone()];

        // Nothing loaded yet, so a load is needed
        assert!(state.files_changed(&files));

        // Recording the current snapshot makes it "unchanged"
        state.record(dictionary_file_snapshot(&files));
        assert!(!state.files_changed(&files));

        // Creating the file is detected
        std::fs::write(&path, b"{}").unwrap();
        assert!(state.files_changed(&files));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reload_state_allows_only_one_load_at_a_time() {
        let state = DictionaryReloadState::default();

        assert!(state.begin());
        assert!(!state.begin(), "a second load must not start while one is running");
        state.finish();
        assert!(state.begin(), "the claim is released once the load finishes");
    }
}
