//! Lightweight JSON/TCP listener for the PSKK engine.
//!
//! The IBus client talks to `pskk-server` over gRPC (port 50051). The Fcitx 5
//! addon is written in C++ and intentionally avoids the heavy gRPC/protobuf
//! C++ toolchain, so `pskk-server` also exposes a minimal newline-delimited
//! JSON protocol on TCP (default 127.0.0.1:50052) for it.
//!
//! # Protocol
//!
//! One JSON object per line. Every request carries an `"op"` field naming the
//! operation; the remaining fields mirror the corresponding `pskk.proto`
//! message (enum fields use their numeric proto values). Responses are the
//! serialized response message of the operation, or `{"error": "..."}`.
//!
//! ```text
//! {"op":"process_key","key_char":"a","key_name":"a","is_pressed":true,
//!  "modifiers":{"shift":false,"ctrl":false,"alt":false,"super":false}}
//! {"op":"set_mode","mode":1}
//! {"op":"get_mode"}
//! {"op":"focus_out"}
//! {"op":"reset"}
//! {"op":"get_dictionary_size"}
//! ```
//!
//! Responses use exactly the proto message shapes (serialized via the serde
//! derives that `tonic-build` adds), e.g. for `process_key`:
//!
//! ```text
//! {"commit_string":"","preedit_segments":[],"preedit_cursor_pos":0,
//!  "candidates":[],"candidate_cursor_pos":0,"show_candidates":false,
//!  "consumed":false,"current_mode":1,"marker_state":0,"engine_state":0,
//!  "status":0}
//! ```
//!
//! `InputMode`: 0 = Alphanumeric, 1 = Hiragana.
//! `ResponseStatus`: 0 = OK, 1 = HENKAN_UNAVAILABLE.
//! `MarkerState` / `EngineState`: numeric values from `pskk.proto`.
//!
//! Errors are reported as `{"error":"..."}` (no newline inside the JSON).
//! A client may simply close the connection at any time.

use std::net::SocketAddr;

use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use crate::grpc::proto::{
    Empty, EngineOutput, KeyEvent as ProtoKeyEvent, KeyModifiers, ModeResponse, SetModeRequest,
};
use crate::grpc::server::PSKKServiceImpl;

const MAX_LINE_BYTES: usize = 256 * 1024;

/// Run the JSON/TCP server until the process exits.
pub async fn run_server(
    addr: SocketAddr,
    service: PSKKServiceImpl,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(addr).await?;
    eprintln!("PSKK JSON server listening on {}", addr);
    loop {
        let (stream, _peer) = listener.accept().await?;
        // Each connection gets a thin service handle around the shared engine.
        let service = PSKKServiceImpl::from_engine(service.engine());
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, service).await {
                eprintln!("JSON connection error: {}", e);
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Request DTOs (kept deliberately independent of the generated proto serde so
// the wire contract stays simple and stable for the C++ client).
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
#[serde(rename_all = "snake_case")]
struct ModifiersDto {
    #[serde(default)]
    shift: bool,
    #[serde(default)]
    ctrl: bool,
    #[serde(default)]
    alt: bool,
    #[serde(rename = "super", default)]
    super_key: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct KeyEventDto {
    #[serde(default)]
    key_char: String,
    #[serde(default)]
    key_name: String,
    #[serde(default)]
    is_pressed: bool,
    #[serde(default)]
    modifiers: ModifiersDto,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct SetModeDto {
    mode: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct RequestEnvelope {
    op: String,
}

async fn handle_connection(
    stream: TcpStream,
    service: PSKKServiceImpl,
) -> Result<(), Box<dyn std::error::Error>> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            // EOF: client closed the connection.
            break;
        }
        if line.len() > MAX_LINE_BYTES {
            let _ = write_half
                .write_all(b"{\"error\":\"request too large\"}\n")
                .await;
            let _ = write_half.flush().await;
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = dispatch(trimmed, &service);
        let payload = match response {
            Ok(Some(bytes)) => bytes,
            Ok(None) => continue, // nothing to answer (should not happen)
            Err(e) => serde_json::to_vec(&json!({"error": e}))?
                .into_iter()
                .chain(std::iter::once(b'\n'))
                .collect::<Vec<u8>>(),
        };

        write_half.write_all(&payload).await?;
        write_half.flush().await?;
    }

    Ok(())
}

/// Dispatch one request line. Returns the raw response bytes (including the
/// trailing newline), or `None`/`Err` as appropriate.
fn dispatch(
    line: &str,
    service: &PSKKServiceImpl,
) -> Result<Option<Vec<u8>>, String> {
    let value: Value =
        serde_json::from_str(line).map_err(|e| format!("invalid JSON: {}", e))?;

    let envelope: RequestEnvelope = serde_json::from_value(value.clone())
        .map_err(|_| "missing or invalid \"op\" field".to_string())?;

    let response: Value = match envelope.op.as_str() {
        "process_key" => {
            let dto: KeyEventDto = serde_json::from_value(value)
                .map_err(|e| format!("invalid process_key request: {}", e))?;
            let key_event = ProtoKeyEvent {
                key_char: dto.key_char,
                key_name: dto.key_name,
                is_pressed: dto.is_pressed,
                modifiers: Some(KeyModifiers {
                    shift: dto.modifiers.shift,
                    ctrl: dto.modifiers.ctrl,
                    alt: dto.modifiers.alt,
                    super_: dto.modifiers.super_key,
                }),
            };
            serde_json::to_value(service.handle_process_key(key_event)?)
                .map_err(|e| format!("failed to serialize response: {}", e))?
        }
        "set_mode" => {
            let dto: SetModeDto = serde_json::from_value(value)
                .map_err(|e| format!("invalid set_mode request: {}", e))?;
            let output: EngineOutput = service.handle_set_mode(SetModeRequest { mode: dto.mode })?;
            serde_json::to_value(output).map_err(|e| format!("failed to serialize response: {}", e))?
        }
        "get_mode" => {
            let resp: ModeResponse = service.handle_get_mode()?;
            serde_json::to_value(resp).map_err(|e| format!("failed to serialize response: {}", e))?
        }
        "focus_out" => {
            let output: EngineOutput = service.handle_focus_out()?;
            serde_json::to_value(output).map_err(|e| format!("failed to serialize response: {}", e))?
        }
        "reset" => {
            service.handle_reset()?;
            serde_json::to_value(Empty {}).map_err(|e| format!("failed to serialize response: {}", e))?
        }
        "get_dictionary_size" => {
            let resp = service.handle_get_dictionary_size()?;
            serde_json::to_value(resp).map_err(|e| format!("failed to serialize response: {}", e))?
        }
        other => return Err(format!("unknown op: {}", other)),
    };

    let mut bytes = serde_json::to_vec(&response)
        .map_err(|e| format!("failed to serialize response: {}", e))?;
    bytes.push(b'\n');
    Ok(Some(bytes))
}
