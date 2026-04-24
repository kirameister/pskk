use crate::engine::{EngineOutput as RustEngineOutput, InputMode as RustInputMode};
use crate::grpc::proto::{
    Candidate, EngineOutput, InputMode, KeyModifiers, ModeResponse, PreeditSegment,
};

/// Convert Rust InputMode to protobuf InputMode
pub fn input_mode_to_proto(mode: RustInputMode) -> InputMode {
    match mode {
        RustInputMode::Alphanumeric => InputMode::Alphanumeric,
        RustInputMode::Hiragana => InputMode::Hiragana,
    }
}

/// Convert protobuf InputMode to Rust InputMode
pub fn input_mode_from_proto(mode: InputMode) -> RustInputMode {
    match mode {
        InputMode::Alphanumeric => RustInputMode::Alphanumeric,
        InputMode::Hiragana => RustInputMode::Hiragana,
    }
}

/// Convert Rust EngineOutput to protobuf EngineOutput
pub fn engine_output_to_proto(output: RustEngineOutput) -> EngineOutput {
    EngineOutput {
        commit_string: output.commit_string,
        preedit_segments: output
            .preedit_segments
            .into_iter()
            .map(|seg| PreeditSegment {
                text: seg.text,
                is_selected: seg.is_selected,
            })
            .collect(),
        preedit_cursor_pos: output.preedit_cursor_pos as u32,
        candidates: output
            .candidates
            .into_iter()
            .map(|cand| Candidate {
                surface: cand.surface,
                reading: cand.reading,
            })
            .collect(),
        candidate_cursor_pos: output.candidate_cursor_pos as u32,
        show_candidates: output.show_candidates,
        consumed: output.consumed,
    }
}

/// Convert protobuf KeyModifiers to individual booleans
pub fn key_modifiers_from_proto(modifiers: Option<KeyModifiers>) -> (bool, bool, bool) {
    match modifiers {
        Some(m) => (m.shift, m.ctrl, m.alt),
        None => (false, false, false),
    }
}

/// Create a ModeResponse from Rust InputMode
pub fn mode_response_from_rust(mode: RustInputMode) -> ModeResponse {
    ModeResponse {
        mode: input_mode_to_proto(mode) as i32,
    }
}
