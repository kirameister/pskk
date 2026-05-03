use crate::engine::{EngineOutput as RustEngineOutput, MarkerState};
use crate::grpc::proto::{
    Candidate, EngineOutput, PreeditSegment, MarkerState as ProtoMarkerState,
};

/// Convert Rust EngineOutput to protobuf EngineOutput
pub fn engine_output_to_proto(output: RustEngineOutput) -> EngineOutput {
    let marker_state = match output.marker_state {
        MarkerState::Idle => ProtoMarkerState::Idle,
        MarkerState::FirstPressed => ProtoMarkerState::FirstPressed,
        MarkerState::FirstReleased => ProtoMarkerState::FirstReleased,
        MarkerState::KanchokuSecondPressed => ProtoMarkerState::KanchokuSecondPressed,
    };

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
        current_mode: output.current_mode as i32,
        marker_state: marker_state as i32,
    }
}
