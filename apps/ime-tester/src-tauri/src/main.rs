#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod grpc_client_state;
mod server_manager;

use grpc_client_state::{GrpcClientState, KeyModifiers};
use pskk::grpc::proto::{EngineOutput, InputMode};
use tauri::{Manager, State};

#[tauri::command]
async fn process_key(
    state: State<'_, GrpcClientState>,
    key_char: Option<char>,
    key_name: String,
    is_pressed: bool,
    modifiers: KeyModifiers,
) -> Result<EngineOutput, String> {
    state.process_key(key_char, key_name, is_pressed, modifiers).await
}

#[tauri::command]
async fn set_mode(state: State<'_, GrpcClientState>, mode: i32) -> Result<EngineOutput, String> {
    let input_mode = InputMode::try_from(mode)
        .map_err(|_| "Invalid mode".to_string())?;
    state.set_mode(input_mode).await
}

#[tauri::command]
async fn get_mode(state: State<'_, GrpcClientState>) -> Result<i32, String> {
    let mode = state.get_mode().await?;
    Ok(mode as i32)
}

#[tauri::command]
async fn focus_out(state: State<'_, GrpcClientState>) -> Result<EngineOutput, String> {
    state.focus_out().await
}

#[tauri::command]
async fn connect_to_server(state: State<'_, GrpcClientState>) -> Result<String, String> {
    state.connect().await?;
    Ok("Connected to PSKK server at 127.0.0.1:50051".to_string())
}

#[tauri::command]
async fn is_connected(state: State<'_, GrpcClientState>) -> Result<bool, String> {
    Ok(state.is_connected().await)
}

#[tauri::command]
async fn load_sample_dictionary(_state: State<'_, GrpcClientState>) -> Result<String, String> {
    // Dictionary loading is now handled by the server
    // This command is kept for compatibility but does nothing
    Ok("Dictionary is managed by the server".to_string())
}

#[tauri::command]
async fn get_loaded_config(state: State<'_, GrpcClientState>) -> Result<String, String> {
    state.get_config().await
}

#[tauri::command]
async fn close_window(window: tauri::Window) {
    window.close().unwrap_or_else(|e| eprintln!("Failed to close window: {}", e));
}

fn main() {
    let client_state = GrpcClientState::new();
    
    tauri::Builder::default()
        .manage(client_state)
        .invoke_handler(tauri::generate_handler![
            connect_to_server,
            is_connected,
            process_key,
            set_mode,
            get_mode,
            focus_out,
            load_sample_dictionary,
            get_loaded_config,
            close_window,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                // Shutdown server when window closes
                let state = window.state::<GrpcClientState>();
                tauri::async_runtime::block_on(async {
                    state.shutdown().await;
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
