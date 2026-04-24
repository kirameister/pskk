use crate::henkan::{Candidate, HenkanProcessor};
use crate::kanchoku::KanchokuProcessor;
use crate::simultaneous_processor::SimultaneousInputProcessor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputMode {
    #[serde(rename = "A")]
    Alphanumeric,
    #[serde(rename = "あ")]
    Hiragana,
}

impl InputMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Alphanumeric => "A",
            Self::Hiragana => "あ",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerState {
    Idle,
    FirstPressed,
    FirstReleased,
    KanchokuSecondPressed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreeditSegment {
    pub text: String,
    pub is_selected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineOutput {
    pub commit_string: Option<String>,
    pub preedit_segments: Vec<PreeditSegment>,
    pub preedit_cursor_pos: usize,
    pub candidates: Vec<Candidate>,
    pub candidate_cursor_pos: usize,
    pub show_candidates: bool,
    pub consumed: bool,
}

impl EngineOutput {
    pub fn empty() -> Self {
        Self {
            commit_string: None,
            preedit_segments: Vec::new(),
            preedit_cursor_pos: 0,
            candidates: Vec::new(),
            candidate_cursor_pos: 0,
            show_candidates: false,
            consumed: false,
        }
    }

    pub fn consumed() -> Self {
        Self {
            consumed: true,
            ..Self::empty()
        }
    }

    pub fn passthrough() -> Self {
        Self {
            consumed: false,
            ..Self::empty()
        }
    }

    pub fn commit(text: String) -> Self {
        Self {
            commit_string: Some(text),
            consumed: true,
            ..Self::empty()
        }
    }
}

pub struct PSKKEngine {
    mode: InputMode,
    
    simul_processor: SimultaneousInputProcessor,
    kanchoku_processor: KanchokuProcessor,
    henkan_processor: HenkanProcessor,
    
    preedit_string: String,
    preedit_hiragana: String,
    preedit_ascii: String,
    preedit_pending: String,
    
    marker_state: MarkerState,
    marker_first_key: Option<char>,
    marker_keys_held: std::collections::HashSet<String>,
    marker_had_input: bool,
    preedit_before_marker: String,
    in_forced_preedit: bool,
    
    pure_kanchoku_held: bool,
    pure_kanchoku_first_key: Option<char>,
    
    bunsetsu_active: bool,
    in_conversion: bool,
    conversion_yomi: String,
    
    converted: bool,
    
    // Mode switching keys from config
    enable_hiragana_keys: Vec<String>,
    disable_hiragana_keys: Vec<String>,
}

impl PSKKEngine {
    pub fn new(
        simul_processor: SimultaneousInputProcessor,
        kanchoku_processor: KanchokuProcessor,
        henkan_processor: HenkanProcessor,
        config: &serde_json::Value,
    ) -> Self {
        // Load mode switching keys from config
        let enable_hiragana_keys = config
            .get("enable_hiragana_key")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_else(|| vec!["Convert".to_string()]);
            
        let disable_hiragana_keys = config
            .get("disable_hiragana_key")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_else(|| vec!["NonConvert".to_string()]);
        
        Self {
            mode: InputMode::Alphanumeric,
            simul_processor,
            kanchoku_processor,
            henkan_processor,
            enable_hiragana_keys,
            disable_hiragana_keys,
            preedit_string: String::new(),
            preedit_hiragana: String::new(),
            preedit_ascii: String::new(),
            preedit_pending: String::new(),
            marker_state: MarkerState::Idle,
            marker_first_key: None,
            marker_keys_held: std::collections::HashSet::new(),
            marker_had_input: false,
            preedit_before_marker: String::new(),
            in_forced_preedit: false,
            pure_kanchoku_held: false,
            pure_kanchoku_first_key: None,
            bunsetsu_active: false,
            in_conversion: false,
            conversion_yomi: String::new(),
            converted: false,
        }
    }

    pub fn get_mode(&self) -> InputMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: InputMode) -> EngineOutput {
        if self.mode == mode {
            return EngineOutput::empty();
        }

        let mut output = EngineOutput::empty();
        
        if !self.preedit_string.is_empty() {
            output.commit_string = Some(self.preedit_string.clone());
        }
        
        self.mode = mode;
        self.reset_state();
        
        output
    }

    pub fn process_key_event(
        &mut self,
        key_char: Option<char>,
        key_name: &str,
        is_pressed: bool,
        has_shift: bool,
        has_ctrl: bool,
        has_alt: bool,
    ) -> EngineOutput {
        // Check for mode switching keys first (before mode check)
        if is_pressed {
            eprintln!("DEBUG: Checking key '{}' against enable_keys: {:?}, disable_keys: {:?}", 
                key_name, self.enable_hiragana_keys, self.disable_hiragana_keys);
            
            // Check for enable hiragana keys (Convert, etc.)
            if self.enable_hiragana_keys.iter().any(|k| k == key_name) {
                eprintln!("DEBUG: Matched enable key! Switching to Hiragana");
                return self.set_mode(InputMode::Hiragana);
            }
            
            // Check for disable hiragana keys (NonConvert, etc.)
            if self.disable_hiragana_keys.iter().any(|k| k == key_name) {
                eprintln!("DEBUG: Matched disable key! Switching to Alphanumeric");
                return self.set_mode(InputMode::Alphanumeric);
            }
        }
        
        if self.mode == InputMode::Alphanumeric {
            return EngineOutput::passthrough();
        }

        self.process_hiragana_mode_key(key_char, key_name, is_pressed, has_shift, has_ctrl, has_alt)
    }

    fn process_hiragana_mode_key(
        &mut self,
        key_char: Option<char>,
        key_name: &str,
        is_pressed: bool,
        has_shift: bool,
        has_ctrl: bool,
        has_alt: bool,
    ) -> EngineOutput {
        if has_ctrl || has_alt {
            if !self.preedit_string.is_empty() {
                let commit = self.preedit_string.clone();
                self.reset_state();
                let mut output = EngineOutput::commit(commit);
                output.consumed = false;
                return output;
            }
            return EngineOutput::passthrough();
        }

        if is_pressed {
            match key_name {
                "Return" | "KP_Enter" => return self.handle_enter(),
                "Escape" => return self.handle_escape(),
                "BackSpace" => return self.handle_backspace(),
                "Down" if self.in_conversion => return self.handle_down_arrow(),
                "Up" if self.in_conversion => return self.handle_up_arrow(),
                "Right" if self.in_conversion => return self.handle_right_arrow(),
                "Left" if self.in_conversion => return self.handle_left_arrow(),
                "space" | "Space" => return self.handle_space_press(key_char),
                _ => {}
            }
        } else {
            if key_name == "space" || key_name == "Space" {
                return self.handle_space_release();
            }
        }

        if let Some(c) = key_char {
            if is_pressed {
                return self.handle_character_input(c, has_shift);
            }
        }

        EngineOutput::passthrough()
    }

    fn handle_enter(&mut self) -> EngineOutput {
        if self.in_conversion {
            return self.confirm_conversion();
        }
        
        if self.in_forced_preedit && !self.preedit_string.is_empty() {
            let commit = self.preedit_string.clone();
            self.in_forced_preedit = false;
            self.reset_preedit();
            return EngineOutput::commit(commit);
        }
        
        if self.bunsetsu_active && !self.preedit_string.is_empty() {
            let commit = self.preedit_string.clone();
            self.bunsetsu_active = false;
            self.reset_preedit();
            return EngineOutput::commit(commit);
        }
        
        if !self.preedit_string.is_empty() {
            let commit = self.preedit_string.clone();
            self.reset_preedit();
            
            let mut output = EngineOutput::commit(commit);
            output.consumed = false;
            return output;
        }
        
        EngineOutput::passthrough()
    }

    fn handle_escape(&mut self) -> EngineOutput {
        if self.in_conversion {
            return self.cancel_conversion();
        }
        
        if !self.preedit_string.is_empty() {
            self.reset_preedit();
            return self.build_preedit_output();
        }
        
        EngineOutput::passthrough()
    }

    fn handle_backspace(&mut self) -> EngineOutput {
        if self.in_conversion {
            return self.cancel_conversion();
        }
        
        if !self.preedit_string.is_empty() {
            if !self.preedit_hiragana.is_empty() {
                self.preedit_hiragana.pop();
            }
            if !self.preedit_ascii.is_empty() {
                self.preedit_ascii.pop();
            }
            self.preedit_string = self.preedit_hiragana.clone();
            return self.build_preedit_output();
        }
        
        EngineOutput::passthrough()
    }

    fn handle_space_press(&mut self, key_char: Option<char>) -> EngineOutput {
        match self.marker_state {
            MarkerState::Idle => {
                if !self.preedit_string.is_empty() && !self.in_conversion {
                    return self.trigger_conversion();
                }
                
                self.marker_state = MarkerState::FirstPressed;
                self.preedit_before_marker = self.preedit_string.clone();
                self.marker_had_input = false;
                self.marker_keys_held.clear();
                
                if let Some(c) = key_char {
                    self.marker_keys_held.insert(c.to_string());
                }
                
                EngineOutput::consumed()
            }
            MarkerState::FirstReleased => {
                self.marker_state = MarkerState::Idle;
                self.handle_marker_release_decision()
            }
            _ => EngineOutput::consumed(),
        }
    }

    fn handle_space_release(&mut self) -> EngineOutput {
        match self.marker_state {
            MarkerState::FirstPressed => {
                if !self.marker_had_input {
                    if !self.preedit_string.is_empty() && !self.in_conversion {
                        self.marker_state = MarkerState::Idle;
                        return self.trigger_conversion();
                    } else if self.preedit_string.is_empty() {
                        self.marker_state = MarkerState::Idle;
                        return EngineOutput::commit(" ".to_string());
                    }
                }
                
                self.marker_state = MarkerState::FirstReleased;
                EngineOutput::consumed()
            }
            MarkerState::KanchokuSecondPressed => {
                self.marker_state = MarkerState::Idle;
                self.handle_marker_release_decision()
            }
            _ => {
                self.marker_state = MarkerState::Idle;
                EngineOutput::consumed()
            }
        }
    }

    fn handle_marker_release_decision(&mut self) -> EngineOutput {
        if self.marker_first_key.is_some() && self.marker_keys_held.is_empty() {
            if let Some(first_char) = self.marker_first_key {
                if let Some(kanji) = self.try_kanchoku_lookup(first_char) {
                    self.preedit_string = self.preedit_before_marker.clone();
                    self.preedit_string.push_str(&kanji);
                    self.preedit_hiragana = self.preedit_before_marker.clone();
                    
                    self.marker_first_key = None;
                    self.marker_had_input = false;
                    
                    return self.build_preedit_output();
                }
                
                self.mark_bunsetsu_boundary(first_char);
                self.marker_first_key = None;
                return self.build_preedit_output();
            }
        }
        
        if self.marker_first_key == Some('f') {
            self.in_forced_preedit = true;
            self.marker_first_key = None;
            return self.build_preedit_output();
        }
        
        self.marker_first_key = None;
        self.marker_had_input = false;
        EngineOutput::consumed()
    }

    fn try_kanchoku_lookup(&mut self, first_char: char) -> Option<String> {
        if self.marker_keys_held.len() == 1 {
            if let Some(second_key_str) = self.marker_keys_held.iter().next() {
                if let Some(second_char) = second_key_str.chars().next() {
                    let kanji = self.kanchoku_processor.lookup_kanji(first_char, second_char);
                    if kanji != crate::kanchoku::MISSING_KANCHOKU_KANJI {
                        return Some(kanji);
                    }
                }
            }
        }
        None
    }

    fn mark_bunsetsu_boundary(&mut self, first_char: char) {
        self.bunsetsu_active = true;
        
        let (output, pending) = self.simul_processor.get_layout_output(
            &self.preedit_pending,
            &first_char.to_string(),
            true,
        );
        
        if let Some(out) = output {
            self.preedit_hiragana.push_str(&out);
            self.preedit_ascii.push(first_char);
        }
        
        self.preedit_pending = pending.unwrap_or_default();
        self.preedit_string = self.preedit_hiragana.clone();
    }

    fn handle_character_input(&mut self, c: char, _has_shift: bool) -> EngineOutput {
        if self.marker_state == MarkerState::FirstPressed {
            self.marker_first_key = Some(c);
            self.marker_had_input = true;
            self.marker_keys_held.insert(c.to_string());
            self.marker_state = MarkerState::KanchokuSecondPressed;
            return EngineOutput::consumed();
        }
        
        if self.marker_state == MarkerState::KanchokuSecondPressed {
            self.marker_keys_held.insert(c.to_string());
            return EngineOutput::consumed();
        }
        
        if self.converted {
            let commit = self.preedit_string.clone();
            self.reset_preedit();
            self.converted = false;
            
            let mut output = EngineOutput::commit(commit);
            output.consumed = false;
            return output;
        }
        
        let (output, pending) = self.simul_processor.get_layout_output(
            &self.preedit_pending,
            &c.to_string(),
            true,
        );
        
        if let Some(out) = output {
            self.preedit_hiragana.push_str(&out);
            self.preedit_ascii.push(c);
        }
        
        self.preedit_pending = pending.unwrap_or_default();
        self.preedit_string = self.preedit_hiragana.clone();
        
        self.build_preedit_output()
    }

    fn trigger_conversion(&mut self) -> EngineOutput {
        if self.preedit_string.is_empty() {
            return EngineOutput::passthrough();
        }
        
        self.conversion_yomi = self.preedit_hiragana.clone();
        self.in_conversion = true;
        
        let candidates = self.henkan_processor.convert(&self.conversion_yomi);
        
        let mut output = EngineOutput::empty();
        output.consumed = true;
        output.show_candidates = true;
        output.candidates = candidates.to_vec();
        output.candidate_cursor_pos = 0;
        
        if let Some(first) = candidates.first() {
            self.preedit_string = first.surface.clone();
        }
        
        output.preedit_segments = self.build_preedit_segments();
        
        output
    }

    fn confirm_conversion(&mut self) -> EngineOutput {
        if !self.in_conversion {
            return EngineOutput::passthrough();
        }
        
        let commit = self.preedit_string.clone();
        
        self.in_conversion = false;
        self.bunsetsu_active = false;
        self.conversion_yomi.clear();
        self.reset_preedit();
        
        EngineOutput::commit(commit)
    }

    fn cancel_conversion(&mut self) -> EngineOutput {
        if !self.in_conversion {
            return EngineOutput::passthrough();
        }
        
        self.preedit_string = self.conversion_yomi.clone();
        self.in_conversion = false;
        
        self.build_preedit_output()
    }

    fn handle_down_arrow(&mut self) -> EngineOutput {
        if !self.in_conversion {
            return EngineOutput::passthrough();
        }
        
        if self.henkan_processor.is_bunsetsu_mode() {
            self.henkan_processor.next_bunsetsu_candidate();
            let surface = self.henkan_processor.get_display_surface();
            self.preedit_string = surface;
        } else {
            if let Some(candidate) = self.henkan_processor.next_candidate() {
                self.preedit_string = candidate.surface.clone();
            }
        }
        
        self.build_conversion_output()
    }

    fn handle_up_arrow(&mut self) -> EngineOutput {
        if !self.in_conversion {
            return EngineOutput::passthrough();
        }
        
        if self.henkan_processor.is_bunsetsu_mode() {
            self.henkan_processor.previous_bunsetsu_candidate();
            let surface = self.henkan_processor.get_display_surface();
            self.preedit_string = surface;
        } else {
            if let Some(candidate) = self.henkan_processor.previous_candidate() {
                self.preedit_string = candidate.surface.clone();
            }
        }
        
        self.build_conversion_output()
    }

    fn handle_right_arrow(&mut self) -> EngineOutput {
        if !self.in_conversion || !self.henkan_processor.is_bunsetsu_mode() {
            return EngineOutput::passthrough();
        }
        
        self.henkan_processor.next_bunsetsu();
        self.build_conversion_output()
    }

    fn handle_left_arrow(&mut self) -> EngineOutput {
        if !self.in_conversion || !self.henkan_processor.is_bunsetsu_mode() {
            return EngineOutput::passthrough();
        }
        
        self.henkan_processor.previous_bunsetsu();
        self.build_conversion_output()
    }

    fn build_preedit_output(&self) -> EngineOutput {
        let mut output = EngineOutput::empty();
        output.consumed = true;
        output.preedit_segments = self.build_preedit_segments();
        output.preedit_cursor_pos = self.preedit_string.chars().count();
        output
    }

    fn build_conversion_output(&self) -> EngineOutput {
        let mut output = EngineOutput::empty();
        output.consumed = true;
        output.show_candidates = true;
        output.candidates = self.henkan_processor.get_candidates().to_vec();
        output.preedit_segments = self.build_preedit_segments();
        
        if let Some(selected) = self.henkan_processor.get_selected_candidate() {
            output.candidate_cursor_pos = self.henkan_processor.get_candidates()
                .iter()
                .position(|c| c.surface == selected.surface)
                .unwrap_or(0);
        }
        
        output
    }

    fn build_preedit_segments(&self) -> Vec<PreeditSegment> {
        if self.in_conversion && self.henkan_processor.is_bunsetsu_mode() {
            self.henkan_processor
                .get_display_surface_with_selection()
                .into_iter()
                .map(|(text, is_selected)| PreeditSegment { text, is_selected })
                .collect()
        } else {
            vec![PreeditSegment {
                text: self.preedit_string.clone(),
                is_selected: false,
            }]
        }
    }

    fn reset_preedit(&mut self) {
        self.preedit_string.clear();
        self.preedit_hiragana.clear();
        self.preedit_ascii.clear();
        self.preedit_pending.clear();
    }

    pub fn reset_state(&mut self) {
        self.reset_preedit();
        self.marker_state = MarkerState::Idle;
        self.marker_first_key = None;
        self.marker_keys_held.clear();
        self.marker_had_input = false;
        self.preedit_before_marker.clear();
        self.in_forced_preedit = false;
        self.pure_kanchoku_held = false;
        self.pure_kanchoku_first_key = None;
        self.bunsetsu_active = false;
        self.in_conversion = false;
        self.conversion_yomi.clear();
        self.converted = false;
        self.henkan_processor.reset();
        self.kanchoku_processor.reset();
        self.simul_processor.simultaneous_reset();
    }

    pub fn focus_out(&mut self) -> EngineOutput {
        let mut output = EngineOutput::empty();
        
        if !self.preedit_string.is_empty() {
            output.commit_string = Some(self.preedit_string.clone());
        }
        
        self.reset_state();
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_engine() -> PSKKEngine {
        let layout = vec![
            ("a".to_string(), "あ".to_string(), "".to_string(), None),
            ("i".to_string(), "い".to_string(), "".to_string(), None),
            ("ka".to_string(), "か".to_string(), "".to_string(), None),
        ];
        let simul = SimultaneousInputProcessor::new(Some(layout));
        
        let kanchoku = KanchokuProcessor::new(None);
        
        let mut dict = HashMap::new();
        let mut ai_candidates = HashMap::new();
        ai_candidates.insert("愛".to_string(), 100);
        dict.insert("あい".to_string(), ai_candidates);
        
        let henkan = HenkanProcessor::new().with_dictionary(dict);
        
        PSKKEngine::new(simul, kanchoku, henkan)
    }

    #[test]
    fn alphanumeric_mode_passes_through() {
        let mut engine = create_test_engine();
        assert_eq!(engine.get_mode(), InputMode::Alphanumeric);
        
        let output = engine.process_key_event(Some('a'), "a", true, false, false, false);
        assert!(!output.consumed);
    }

    #[test]
    fn mode_switching_commits_preedit() {
        let mut engine = create_test_engine();
        engine.set_mode(InputMode::Hiragana);
        
        engine.process_key_event(Some('a'), "a", true, false, false, false);
        assert!(!engine.preedit_string.is_empty());
        
        let output = engine.set_mode(InputMode::Alphanumeric);
        assert!(output.commit_string.is_some());
        assert!(engine.preedit_string.is_empty());
    }

    #[test]
    fn character_input_builds_preedit() {
        let mut engine = create_test_engine();
        engine.set_mode(InputMode::Hiragana);
        
        let output = engine.process_key_event(Some('a'), "a", true, false, false, false);
        assert!(output.consumed);
        assert_eq!(engine.preedit_string, "あ");
        
        let output = engine.process_key_event(Some('i'), "i", true, false, false, false);
        assert!(output.consumed);
        assert_eq!(engine.preedit_string, "あい");
    }

    #[test]
    fn backspace_removes_character() {
        let mut engine = create_test_engine();
        engine.set_mode(InputMode::Hiragana);
        
        engine.process_key_event(Some('a'), "a", true, false, false, false);
        engine.process_key_event(Some('i'), "i", true, false, false, false);
        assert_eq!(engine.preedit_string, "あい");
        
        let output = engine.process_key_event(None, "BackSpace", true, false, false, false);
        assert!(output.consumed);
        assert_eq!(engine.preedit_string, "あ");
    }

    #[test]
    fn space_triggers_conversion() {
        let mut engine = create_test_engine();
        engine.set_mode(InputMode::Hiragana);
        
        engine.process_key_event(Some('a'), "a", true, false, false, false);
        engine.process_key_event(Some('i'), "i", true, false, false, false);
        
        let output = engine.process_key_event(Some(' '), "space", true, false, false, false);
        assert!(output.consumed);
        assert!(output.show_candidates);
        assert!(!output.candidates.is_empty());
        assert_eq!(output.candidates[0].surface, "愛");
    }

    #[test]
    fn enter_confirms_conversion() {
        let mut engine = create_test_engine();
        engine.set_mode(InputMode::Hiragana);
        
        engine.process_key_event(Some('a'), "a", true, false, false, false);
        engine.process_key_event(Some('i'), "i", true, false, false, false);
        engine.process_key_event(Some(' '), "space", true, false, false, false);
        
        assert!(engine.in_conversion);
        
        let output = engine.process_key_event(None, "Return", true, false, false, false);
        assert!(output.consumed);
        assert_eq!(output.commit_string, Some("愛".to_string()));
        assert!(!engine.in_conversion);
        assert!(engine.preedit_string.is_empty());
    }

    #[test]
    fn escape_cancels_conversion() {
        let mut engine = create_test_engine();
        engine.set_mode(InputMode::Hiragana);
        
        engine.process_key_event(Some('a'), "a", true, false, false, false);
        engine.process_key_event(Some('i'), "i", true, false, false, false);
        engine.process_key_event(Some(' '), "space", true, false, false, false);
        
        assert!(engine.in_conversion);
        
        let output = engine.process_key_event(None, "Escape", true, false, false, false);
        assert!(output.consumed);
        assert!(!engine.in_conversion);
        assert_eq!(engine.preedit_string, "あい");
    }

    #[test]
    fn ctrl_key_commits_and_passes_through() {
        let mut engine = create_test_engine();
        engine.set_mode(InputMode::Hiragana);
        
        engine.process_key_event(Some('a'), "a", true, false, false, false);
        assert!(!engine.preedit_string.is_empty());
        
        let output = engine.process_key_event(Some('c'), "c", true, false, true, false);
        assert!(!output.consumed, "Ctrl+C should pass through to application");
        assert!(output.commit_string.is_some(), "Should commit preedit before passing through");
        assert!(engine.preedit_string.is_empty(), "Preedit should be cleared after commit");
    }
}
