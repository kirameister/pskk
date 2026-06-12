use crate::engine::{EngineOutput as RustEngineOutput, MarkerState, EngineState};
use crate::grpc::proto::{
    Candidate, EngineOutput, PreeditSegment, MarkerState as ProtoMarkerState, EngineState as ProtoEngineState,
};

/// Convert Rust EngineOutput to protobuf EngineOutput
pub fn engine_output_to_proto(output: RustEngineOutput) -> EngineOutput {
    eprintln!("=== Engine Output ===");
    eprintln!("  consumed: {}", output.consumed);
    eprintln!("  commit_string: {:?}", output.commit_string);
    eprintln!("  preedit_segments: {} segments", output.preedit_segments.len());
    for (i, seg) in output.preedit_segments.iter().enumerate() {
        eprintln!("    [{}] '{}' (selected: {})", i, seg.text, seg.is_selected);
    }
    eprintln!("  candidates: {}", output.candidates.len());
    eprintln!("  show_candidates: {}", output.show_candidates);

    let marker_state = match output.marker_state {
        MarkerState::Idle => ProtoMarkerState::Idle,
        MarkerState::MarkerHeld => ProtoMarkerState::MarkerHeld,
        MarkerState::FirstPressed => ProtoMarkerState::FirstPressed,
        MarkerState::FirstReleased => ProtoMarkerState::FirstReleased,
        MarkerState::KanchokuSecondPressed => ProtoMarkerState::KanchokuSecondPressed,
    };

    let engine_state = match output.engine_state {
        EngineState::Normal => ProtoEngineState::Normal,
        EngineState::Bunsetsu => ProtoEngineState::Bunsetsu,
        EngineState::ForcedPreedit => ProtoEngineState::ForcedPreedit,
        EngineState::Converting => ProtoEngineState::Converting,
    };

    EngineOutput {
        commit_string: output.commit_string.unwrap_or_default(),
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
        current_mode: output.current_mode as i32,
        marker_state: marker_state as i32,
        engine_state: engine_state as i32,
    }
}
