use crate::grpc::proto::{InputMode as ProtoInputMode, KeyModifiers as ProtoKeyModifiers};
use crate::henkan::{Candidate, HenkanProcessor};
use crate::kanchoku::KanchokuProcessor;
use crate::simultaneous_processor::SimultaneousInputProcessor;
use crate::util::{get_config_data, get_layout_data};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkerState {
    Idle,
    MarkerHeld,
    FirstPressed,
    FirstReleased,
    KanchokuSecondPressed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineState {
    Normal,
    Bunsetsu,
    ForcedPreedit,
    Converting,
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
    pub current_mode: ProtoInputMode,
    pub marker_state: MarkerState,
    pub engine_state: EngineState,
}

impl EngineOutput {
    pub fn empty(mode: InputMode) -> Self {
        Self {
            commit_string: None,
            preedit_segments: Vec::new(),
            preedit_cursor_pos: 0,
            candidates: Vec::new(),
            candidate_cursor_pos: 0,
            show_candidates: false,
            consumed: false,
            current_mode: match mode {
                InputMode::Alphanumeric => ProtoInputMode::Alphanumeric,
                InputMode::Hiragana => ProtoInputMode::Hiragana,
            },
            marker_state: MarkerState::Idle,
            engine_state: EngineState::Normal,
        }
    }

    pub fn consumed(mode: InputMode) -> Self {
        Self {
            consumed: true,
            ..Self::empty(mode)
        }
    }

    pub fn passthrough(mode: InputMode) -> Self {
        Self {
            consumed: false,
            ..Self::empty(mode)
        }
    }

    pub fn commit(text: String, mode: InputMode) -> Self {
        Self {
            commit_string: Some(text),
            consumed: true,
            ..Self::empty(mode)
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
    marker_second_key: Option<char>,
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
    
    // Full configuration
    config: serde_json::Value,
}

impl PSKKEngine {
    fn mode_switch_key_matches(configured: &str, incoming: &str) -> bool {
        if configured == incoming {
            return true;
        }
        matches!(
            (configured, incoming),
            ("Convert", "Henkan")
                | ("Henkan", "Convert")
                | ("NonConvert", "Muhenkan")
                | ("Muhenkan", "NonConvert")
        )
    }

    pub fn new(
        simul_processor: SimultaneousInputProcessor,
        kanchoku_processor: KanchokuProcessor,
        henkan_processor: HenkanProcessor,
    ) -> Result<Self, String> {
        // Load config internally
        let (config, _warnings) = get_config_data()
            .map_err(|e| format!("Failed to load config: {}", e))?;

        // Load layout from config if not already provided
        let simul_processor = if simul_processor.layout_data.is_some() {
            eprintln!("Using provided layout data with {} entries", simul_processor.layout_data.as_ref().unwrap().len());
            simul_processor
        } else {
            // Load layout from config
            let layout_data = get_layout_data(&config)
                .map_err(|e| format!("Failed to load layout: {}", e))?;

            // Parse layout JSON into RawLayoutEntry format
            let layout_entries: Vec<(String, String, String, Option<u64>)> = layout_data
                .as_array()
                .ok_or_else(|| "Layout data is not an array".to_string())?
                .iter()
                .map(|entry| {
                    let arr = entry.as_array().ok_or_else(|| "Layout entry is not an array".to_string())?;
                    if arr.len() < 3 {
                        return Err("Layout entry must have at least 3 elements".to_string());
                    }
                    let input = arr[0].as_str().ok_or_else(|| "Input is not a string".to_string())?.to_string();
                    let output = arr[1].as_str().ok_or_else(|| "Output is not a string".to_string())?.to_string();
                    let pending = arr[2].as_str().ok_or_else(|| "Pending is not a string".to_string())?.to_string();
                    let simul_limit_ms = if arr.len() > 3 {
                        arr[3].as_u64()
                    } else {
                        None
                    };
                    Ok((input, output, pending, simul_limit_ms))
                })
                .collect::<Result<Vec<_>, String>>()?;

            eprintln!("Loaded {} layout entries from config", layout_entries.len());
            SimultaneousInputProcessor::new(Some(layout_entries))
        };

        // Load Kanchoku layout from config if not already provided
        let kanchoku_processor = if kanchoku_processor.has_layout() {
            eprintln!("Using provided Kanchoku layout");
            kanchoku_processor
        } else {
            use crate::util::get_kanchoku_layout;
            let kanchoku_layout_json = get_kanchoku_layout(&config)
                .map_err(|e| format!("Failed to load Kanchoku layout: {}", e))?;
            
            // Parse Kanchoku layout JSON into nested HashMap
            let kanchoku_layout = crate::kanchoku::parse_kanchoku_layout(&kanchoku_layout_json)
                .ok_or_else(|| "Failed to parse Kanchoku layout".to_string())?;
            
            eprintln!("Loaded Kanchoku layout with {} first-stroke keys", kanchoku_layout.len());
            KanchokuProcessor::new(Some(kanchoku_layout))
        };

        Ok(Self {
            mode: InputMode::Alphanumeric,
            simul_processor,
            kanchoku_processor,
            henkan_processor,
            config,
            preedit_string: String::new(),
            preedit_hiragana: String::new(),
            preedit_ascii: String::new(),
            preedit_pending: String::new(),
            marker_state: MarkerState::Idle,
            marker_first_key: None,
            marker_second_key: None,
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
        })
    }

    pub fn get_mode(&self) -> ProtoInputMode {
        match self.mode {
            InputMode::Alphanumeric => ProtoInputMode::Alphanumeric,
            InputMode::Hiragana => ProtoInputMode::Hiragana,
        }
    }

    pub fn get_dictionary_size(&self) -> usize {
        self.henkan_processor.get_dictionary_size()
    }

    pub fn get_config(&self) -> &serde_json::Value {
        &self.config
    }

    pub fn reload_config(&mut self) -> Result<(), String> {
        let (config, _warnings) = get_config_data()
            .map_err(|e| format!("Failed to reload config: {}", e))?;

        self.config = config;
        eprintln!("Config reloaded.");

        Ok(())
    }

    pub fn set_mode(&mut self, mode: ProtoInputMode) -> EngineOutput {
        let mode = match mode {
            ProtoInputMode::Alphanumeric => InputMode::Alphanumeric,
            ProtoInputMode::Hiragana => InputMode::Hiragana,
        };
        if self.mode == mode {
            return EngineOutput::empty(self.mode);
        }

        let mut output = EngineOutput::empty(self.mode);

        if !self.preedit_string.is_empty() {
            output.commit_string = Some(self.preedit_string.clone());
        }

        self.mode = mode;
        self.reset_state();

        // Update output to reflect the new mode
        output.current_mode = match mode {
            InputMode::Alphanumeric => ProtoInputMode::Alphanumeric,
            InputMode::Hiragana => ProtoInputMode::Hiragana,
        };

        output
    }

    pub fn process_key_event(
        &mut self,
        key_char: Option<char>,
        key_name: &str,
        is_pressed: bool,
        modifiers: Option<ProtoKeyModifiers>,
    ) -> EngineOutput {
        let (has_shift, has_ctrl, has_alt) = match modifiers {
            Some(m) => (m.shift, m.ctrl, m.alt),
            None => (false, false, false),
        };
        // Check for mode switching keys first (before mode check)
        if is_pressed {
            // eprintln!("KEY PRESSED: '{}' (char: {:?})", key_name, key_char);

            // Extract mode switching keys from config
            let enable_hiragana_keys = self
                .config
                .get("enable_hiragana_key")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                .unwrap_or_else(|| vec!["Henkan", "Convert"]);

            let disable_hiragana_keys = self
                .config
                .get("disable_hiragana_key")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                .unwrap_or_else(|| vec!["Muhenkan", "NonConvert"]);

            // eprintln!("  Checking against enable_keys: {:?}", enable_hiragana_keys);
            // eprintln!("  Checking against disable_keys: {:?}", disable_hiragana_keys);

            // Check for enable hiragana keys (Convert, etc.)
            if enable_hiragana_keys
                .iter()
                .any(|k| Self::mode_switch_key_matches(k, key_name))
            {
                // eprintln!("  ✓ MATCHED enable key! Switching to Hiragana");
                return self.set_mode(ProtoInputMode::Hiragana);
            }

            // Check for disable hiragana keys (NonConvert, etc.)
            if disable_hiragana_keys
                .iter()
                .any(|k| Self::mode_switch_key_matches(k, key_name))
            {
                // eprintln!("  ✓ MATCHED disable key! Switching to Alphanumeric");
                return self.set_mode(ProtoInputMode::Alphanumeric);
            }

            // eprintln!("  ✗ No match");
        }
        
        if self.mode == InputMode::Alphanumeric {
            return EngineOutput::passthrough(self.mode);
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
                let mut output = EngineOutput::commit(commit, self.mode);
                output.consumed = false;
                return output;
            }
            return EngineOutput::passthrough(self.mode);
        }

        if is_pressed {
            match key_name {
                "Return" | "KP_Enter" | "Enter" => return self.handle_enter(),
                "Escape" => return self.handle_escape(),
                "BackSpace" | "Backspace" => return self.handle_backspace(),
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
            
            // Handle character key release for marker state machine
            if let Some(c) = key_char {
                return self.handle_character_release(c);
            }
        }

        if let Some(c) = key_char {
            if is_pressed {
                return self.handle_character_input(c, has_shift);
            }
        }

        EngineOutput::passthrough(self.mode)
    }

    fn handle_enter(&mut self) -> EngineOutput {
        eprintln!("handle_enter: in_conversion={}, in_forced_preedit={}, bunsetsu_active={}, preedit_string='{}', preedit_hiragana='{}', preedit_pending='{}'",
                  self.in_conversion, self.in_forced_preedit, self.bunsetsu_active, self.preedit_string, self.preedit_hiragana, self.preedit_pending);
        
        if self.in_conversion {
            eprintln!("  -> Confirming conversion");
            return self.confirm_conversion();
        }
        
        if self.in_forced_preedit && !self.preedit_string.is_empty() {
            eprintln!("  -> Committing forced preedit: '{}' with consumed=false", self.preedit_string);
            let commit = self.preedit_string.clone();
            self.in_forced_preedit = false;
            self.reset_preedit();
            
            let mut output = EngineOutput::commit(commit, self.mode);
            output.consumed = false;
            eprintln!("  -> Returning commit with consumed=false, commit_string='{:?}'", output.commit_string);
            return output;
        }
        
        if self.bunsetsu_active && !self.preedit_string.is_empty() {
            eprintln!("  -> Committing bunsetsu preedit: '{}' with consumed=false", self.preedit_string);
            let commit = self.preedit_string.clone();
            self.bunsetsu_active = false;
            self.reset_preedit();
            
            let mut output = EngineOutput::commit(commit, self.mode);
            output.consumed = false;
            eprintln!("  -> Returning commit with consumed=false, commit_string='{:?}'", output.commit_string);
            return output;
        }
        
        if !self.preedit_string.is_empty() {
            eprintln!("  -> Committing normal preedit: '{}' with consumed=false", self.preedit_string);
            let commit = self.preedit_string.clone();
            self.reset_preedit();
            
            let mut output = EngineOutput::commit(commit, self.mode);
            output.consumed = false;
            eprintln!("  -> Returning commit with consumed=false, commit_string='{:?}'", output.commit_string);
            return output;
        }
        
        eprintln!("  -> Passthrough (no preedit)");
        EngineOutput::passthrough(self.mode)
    }

    fn handle_escape(&mut self) -> EngineOutput {
        if self.in_conversion {
            return self.cancel_conversion();
        }
        
        if !self.preedit_string.is_empty() {
            self.reset_preedit();
            return self.build_preedit_output();
        }
        
        EngineOutput::passthrough(self.mode)
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
        
        EngineOutput::passthrough(self.mode)
    }

    fn handle_space_press(&mut self, _key_char: Option<char>) -> EngineOutput {
        match self.marker_state {
            MarkerState::Idle => {
                // Always enter MarkerHeld state
                // Preedit will be committed on space release if it's a tap
                self.marker_state = MarkerState::MarkerHeld;
                self.preedit_before_marker = self.preedit_string.clone();
                self.marker_had_input = false;
                self.marker_keys_held.clear();
                self.marker_first_key = None;
                self.marker_second_key = None;

                let mut output = EngineOutput::consumed(self.mode);
                output.marker_state = self.marker_state;
                output
            }
            MarkerState::FirstReleased => {
                self.marker_state = MarkerState::Idle;
                self.handle_marker_release_decision()
            }
            _ => {
                let mut output = EngineOutput::consumed(self.mode);
                output.marker_state = self.marker_state;
                output
            }
        }
    }

    fn handle_space_release(&mut self) -> EngineOutput {
        eprintln!("handle_space_release: marker_state={:?}, in_conversion={}, bunsetsu_active={}",
                  self.marker_state, self.in_conversion, self.bunsetsu_active);
        
        match self.marker_state {
            MarkerState::MarkerHeld => {
                if self.marker_had_input {
                    // Keys were pressed during this space hold, not a tap
                    // Just release cleanly
                    self.marker_state = MarkerState::Idle;
                    self.marker_first_key = None;
                    self.marker_keys_held.clear();
                    let mut output = EngineOutput::consumed(self.mode);
                    output.marker_state = self.marker_state;
                    output
                } else if self.in_conversion {
                    // CONVERTING state: cycle to next candidate
                    self.marker_state = MarkerState::Idle;
                    self.marker_first_key = None;
                    self.marker_keys_held.clear();
                    self.handle_down_arrow()
                } else if self.bunsetsu_active || self.in_forced_preedit {
                    // BUNSETSU or FORCED_PREEDIT state: trigger conversion
                    self.marker_state = MarkerState::Idle;
                    self.marker_first_key = None;
                    self.marker_keys_held.clear();
                    self.trigger_conversion()
                } else {
                    // Normal mode: space tap commits preedit (no space character output)
                    let commit = self.preedit_string.clone();
                    self.reset_preedit();
                    self.marker_state = MarkerState::Idle;
                    self.marker_first_key = None;
                    self.marker_keys_held.clear();
                    
                    if !commit.is_empty() {
                        EngineOutput::commit(commit, self.mode)
                    } else {
                        // No preedit to commit - output space character
                        EngineOutput::commit(" ".to_string(), self.mode)
                    }
                }
            }
            MarkerState::FirstPressed | MarkerState::FirstReleased => {
                // Space released after key was pressed: activate bunsetsu mode
                self.handle_marker_release_decision()
            }
            MarkerState::KanchokuSecondPressed => {
                self.handle_marker_release_decision()
            }
            _ => {
                // Handle space release when in conversion mode (marker_state is Idle)
                if self.in_conversion {
                    eprintln!("Space released during conversion, returning conversion output");
                    return self.build_conversion_output();
                }
                
                self.marker_state = MarkerState::Idle;
                self.marker_first_key = None;
                self.marker_keys_held.clear();
                let mut output = EngineOutput::consumed(self.mode);
                output.marker_state = self.marker_state;
                output
            }
        }
    }

    fn handle_marker_release_decision(&mut self) -> EngineOutput {
        eprintln!("handle_marker_release_decision: marker_first_key={:?}, marker_second_key={:?}, marker_keys_held.is_empty()={}, preedit_string='{}'",
                  self.marker_first_key, self.marker_second_key, self.marker_keys_held.is_empty(), self.preedit_string);
        
        // Check for forced preedit trigger key first (before bunsetsu logic)
        if self.marker_first_key == Some('f') {
            eprintln!("Entering forced preedit mode");
            self.in_forced_preedit = true;
            self.marker_first_key = None;
            self.marker_state = MarkerState::Idle;
            return self.build_preedit_output();
        }
        
        if self.marker_first_key.is_some() && self.marker_keys_held.is_empty() {
            if let Some(first_char) = self.marker_first_key {
                // Check if Kanchoku was already processed (second key exists)
                if self.marker_second_key.is_some() {
                    eprintln!("Kanchoku already processed, returning to Idle");
                    self.marker_first_key = None;
                    self.marker_second_key = None;
                    self.marker_had_input = false;
                    self.marker_state = MarkerState::Idle;
                    
                    return self.build_preedit_output();
                }
                
                eprintln!("Checking kanchoku lookup for first_char='{}'", first_char);
                if let Some(kanji) = self.try_kanchoku_lookup(first_char) {
                    eprintln!("Kanchoku found: '{}'", kanji);
                    self.preedit_string = self.preedit_before_marker.clone();
                    self.preedit_string.push_str(&kanji);
                    self.preedit_hiragana = self.preedit_before_marker.clone();
                    
                    self.marker_first_key = None;
                    self.marker_second_key = None;
                    self.marker_had_input = false;
                    self.marker_state = MarkerState::Idle;
                    
                    return self.build_preedit_output();
                }
                
                eprintln!("No kanchoku, marking bunsetsu boundary with first_char='{}'", first_char);
                self.mark_bunsetsu_boundary(first_char);
                self.marker_first_key = None;
                self.marker_second_key = None;
                self.marker_state = MarkerState::Idle;
                return self.build_preedit_output();
            }
        }
        
        eprintln!("No action taken, returning consumed");
        self.marker_first_key = None;
        self.marker_had_input = false;
        self.marker_state = MarkerState::Idle;
        EngineOutput::consumed(self.mode)
    }

    fn try_kanchoku_lookup(&mut self, first_char: char) -> Option<String> {
        eprintln!("try_kanchoku_lookup: first_char='{}', marker_second_key={:?}", 
                  first_char, self.marker_second_key);
        
        // Check if we have both first and second keys for Kanchoku
        if let Some(second_char) = self.marker_second_key {
            eprintln!("Looking up Kanchoku: '{}' + '{}'", first_char, second_char);
            let kanji = self.kanchoku_processor.lookup_kanji(first_char, second_char);
            eprintln!("Kanchoku lookup result: '{}'", kanji);
            if kanji != crate::kanchoku::MISSING_KANCHOKU_KANJI {
                return Some(kanji);
            }
        }
        eprintln!("No valid Kanchoku pair found");
        None
    }

    fn mark_bunsetsu_boundary(&mut self, _first_char: char) {
        // Simply activate bunsetsu mode - the first_char was already processed
        // in handle_character_input, so we don't need to process it again
        self.bunsetsu_active = true;
        eprintln!("Bunsetsu mode activated. Current preedit: '{}', preedit_hiragana: '{}', preedit_pending: '{}'",
                  self.preedit_string, self.preedit_hiragana, self.preedit_pending);
    }

    fn handle_character_input(&mut self, c: char, _has_shift: bool) -> EngineOutput {
        // Track marker state but don't block character processing
        if self.marker_state == MarkerState::MarkerHeld {
            self.marker_first_key = Some(c);
            self.marker_keys_held.insert(c.to_string());
            self.marker_had_input = true;
            self.marker_state = MarkerState::FirstPressed;
            eprintln!("First key '{}' pressed, transitioning to FirstPressed", c);
            
            // If in CONVERTING state, commit the selected candidate before processing new character
            if self.in_conversion {
                eprintln!("Implicit conversion: committing '{}' before new character '{}'", self.preedit_string, c);
                let commit = self.preedit_string.clone();
                self.in_conversion = false;
                self.bunsetsu_active = false;
                self.conversion_yomi.clear();
                self.reset_preedit();
                self.henkan_processor.reset();
                
                // Now process the new character and return commit + new preedit
                let (output, pending) = self.simul_processor.get_layout_output("", &c.to_string(), true);
                if let Some(ref out) = output {
                    if !out.is_empty() {
                        self.preedit_hiragana.push_str(out);
                        self.preedit_ascii.push(c);
                    }
                }
                self.preedit_pending = pending.unwrap_or_default();
                self.preedit_string = format!("{}{}", self.preedit_hiragana, self.preedit_pending);
                
                let mut result = EngineOutput::commit(commit, self.mode);
                result.preedit_segments = self.build_preedit_segments();
                result.preedit_cursor_pos = self.preedit_string.chars().count();
                result.marker_state = self.marker_state;
                result.engine_state = self.get_engine_state();
                return result;
            }
            
            // If in BUNSETSU state, perform implicit conversion and commit before processing new character
            if self.bunsetsu_active {
                let yomi = self.preedit_string.clone();
                let commit = if !yomi.is_empty() {
                    let candidates = self.henkan_processor.convert(&yomi).to_vec();
                    if let Some(first) = candidates.first() {
                        eprintln!("Immediate implicit conversion: '{}' → '{}'", yomi, first.surface);
                        first.surface.clone()
                    } else {
                        eprintln!("No candidates, committing yomi: '{}'", yomi);
                        yomi
                    }
                } else {
                    String::new()
                };
                
                self.bunsetsu_active = false;
                self.reset_preedit();
                self.henkan_processor.reset();
                
                // Now process the new character and return commit + new preedit
                let (output, pending) = self.simul_processor.get_layout_output("", &c.to_string(), true);
                if let Some(ref out) = output {
                    if !out.is_empty() {
                        self.preedit_hiragana.push_str(out);
                        self.preedit_ascii.push(c);
                    }
                }
                self.preedit_pending = pending.unwrap_or_default();
                self.preedit_string = format!("{}{}", self.preedit_hiragana, self.preedit_pending);
                
                let mut result = if !commit.is_empty() {
                    EngineOutput::commit(commit, self.mode)
                } else {
                    EngineOutput::empty(self.mode)
                };
                result.consumed = true;
                result.preedit_segments = self.build_preedit_segments();
                result.preedit_cursor_pos = self.preedit_string.chars().count();
                result.marker_state = self.marker_state;
                result.engine_state = self.get_engine_state();
                return result;
            }
            
            // Process character input normally and return
            // (Don't fall through to FirstPressed check below!)
            let (output, pending) = self.simul_processor.get_layout_output(
                &self.preedit_pending,
                &c.to_string(),
                true,
            );

            eprintln!("Simul processor output: output={:?}, pending={:?}, preedit_pending='{}', char='{}'",
                  output, pending, self.preedit_pending, c);

            if let Some(ref out) = output {
                if !out.is_empty() {
                    self.preedit_hiragana.push_str(out);
                    self.preedit_ascii.push(c);
                    eprintln!("Updated preedit_hiragana: '{}'", self.preedit_hiragana);
                }
            }

            self.preedit_pending = pending.unwrap_or_default();
            self.preedit_string = format!("{}{}", self.preedit_hiragana, self.preedit_pending);
            eprintln!("Final preedit_string: '{}' (hiragana='{}' + pending='{}')",
                      self.preedit_string, self.preedit_hiragana, self.preedit_pending);

            return self.build_preedit_output();
        }

        if self.marker_state == MarkerState::FirstPressed || self.marker_state == MarkerState::FirstReleased {
            // Second key pressed - try Kanchoku lookup immediately
            self.marker_second_key = Some(c);
            self.marker_keys_held.insert(c.to_string());
            eprintln!("Second key '{}' pressed (state={:?}), attempting Kanchoku lookup", c, self.marker_state);
            
            // Try Kanchoku lookup with first and second keys
            if let Some(first_char) = self.marker_first_key {
                if let Some(kanji) = self.try_kanchoku_lookup(first_char) {
                    eprintln!("Kanchoku found: '{}', committing immediately", kanji);
                    
                    // In normal mode: commit the kanji directly
                    if !self.in_forced_preedit {
                        // Restore preedit to state before marker and clear pending
                        self.preedit_string = self.preedit_before_marker.clone();
                        self.preedit_hiragana = self.preedit_before_marker.clone();
                        self.preedit_pending.clear();
                        self.preedit_ascii.clear();
                        
                        // Transition to KanchokuSecondPressed to wait for releases
                        self.marker_state = MarkerState::KanchokuSecondPressed;
                        
                        // Commit the kanji
                        let mut output = EngineOutput::commit(kanji, self.mode);
                        output.preedit_segments = self.build_preedit_segments();
                        output.preedit_cursor_pos = self.preedit_string.chars().count();
                        output.marker_state = self.marker_state;
                        output.engine_state = self.get_engine_state();
                        return output;
                    } else {
                        // In forced preedit mode: add to preedit
                        self.preedit_string = self.preedit_before_marker.clone();
                        self.preedit_string.push_str(&kanji);
                        self.preedit_hiragana = self.preedit_before_marker.clone();
                        
                        self.marker_state = MarkerState::KanchokuSecondPressed;
                        
                        let mut output = self.build_preedit_output();
                        output.marker_state = self.marker_state;
                        return output;
                    }
                }
            }
            
            // Not a valid Kanchoku pair - treat as bunsetsu marker
            eprintln!("Not a valid Kanchoku pair, will activate bunsetsu on space release");
            self.marker_state = MarkerState::KanchokuSecondPressed;
            
            let mut output = EngineOutput::consumed(self.mode);
            output.marker_state = self.marker_state;
            output.engine_state = self.get_engine_state();
            return output;
        }

        if self.marker_state == MarkerState::KanchokuSecondPressed {
            // Additional key after Kanchoku - start a new Kanchoku sequence
            eprintln!("Additional key '{}' pressed in KanchokuSecondPressed, starting new Kanchoku sequence", c);
            
            // Reset marker state for new sequence
            self.marker_first_key = Some(c);
            self.marker_second_key = None;
            self.marker_keys_held.clear();
            self.marker_keys_held.insert(c.to_string());
            self.marker_state = MarkerState::FirstPressed;
            
            // Process the character normally (add to preedit)
            let (output, pending) = self.simul_processor.get_layout_output(
                &self.preedit_pending,
                &c.to_string(),
                true,
            );

            eprintln!("Simul processor output: output={:?}, pending={:?}, preedit_pending='{}', char='{}'",
                  output, pending, self.preedit_pending, c);

            if let Some(ref out) = output {
                if !out.is_empty() {
                    self.preedit_hiragana.push_str(out);
                    self.preedit_ascii.push(c);
                    eprintln!("Updated preedit_hiragana: '{}'", self.preedit_hiragana);
                }
            }

            self.preedit_pending = pending.unwrap_or_default();
            self.preedit_string = format!("{}{}", self.preedit_hiragana, self.preedit_pending);
            eprintln!("Final preedit_string: '{}' (hiragana='{}' + pending='{}')",
                      self.preedit_string, self.preedit_hiragana, self.preedit_pending);

            return self.build_preedit_output();
        }

        // If in CONVERTING state and typing a new character (without holding space),
        // commit the selected candidate and continue with the new character
        if self.in_conversion {
            eprintln!("Char input in CONVERTING: confirming '{}' and adding '{}'", self.preedit_string, c);
            let commit = self.preedit_string.clone();
            self.in_conversion = false;
            self.in_forced_preedit = false;
            self.conversion_yomi.clear();
            self.reset_preedit();
            self.henkan_processor.reset();
            
            // Process the new character
            let (output, pending) = self.simul_processor.get_layout_output("", &c.to_string(), true);
            if let Some(ref out) = output {
                if !out.is_empty() {
                    self.preedit_hiragana.push_str(out);
                    self.preedit_ascii.push(c);
                }
            }
            self.preedit_pending = pending.unwrap_or_default();
            self.preedit_string = format!("{}{}", self.preedit_hiragana, self.preedit_pending);
            
            let mut result = EngineOutput::commit(commit, self.mode);
            result.preedit_segments = self.build_preedit_segments();
            result.preedit_cursor_pos = self.preedit_string.chars().count();
            result.marker_state = self.marker_state;
            result.engine_state = self.get_engine_state();
            return result;
        }

        if self.converted {
            let commit = self.preedit_string.clone();
            self.reset_preedit();
            self.converted = false;

            let mut output = EngineOutput::commit(commit, self.mode);
            output.consumed = false;
            return output;
        }

        let (output, pending) = self.simul_processor.get_layout_output(
            &self.preedit_pending,
            &c.to_string(),
            true,
        );

        eprintln!("Simul processor output: output={:?}, pending={:?}, preedit_pending='{}', char='{}'",
              output, pending, self.preedit_pending, c);

        // For simultaneous input layouts, if output is empty but pending is not,
        // show the pending string in the preedit
        if let Some(ref out) = output {
            if !out.is_empty() {
                self.preedit_hiragana.push_str(out);
                self.preedit_ascii.push(c);
                eprintln!("Updated preedit_hiragana: '{}'", self.preedit_hiragana);
            }
        }

        self.preedit_pending = pending.unwrap_or_default();

        // For simultaneous input layouts, show both preedit_hiragana and preedit_pending
        // in the preedit display
        self.preedit_string = format!("{}{}", self.preedit_hiragana, self.preedit_pending);
        eprintln!("Final preedit_string: '{}' (hiragana='{}' + pending='{}')",
                  self.preedit_string, self.preedit_hiragana, self.preedit_pending);

        self.build_preedit_output()
    }

    fn handle_character_release(&mut self, c: char) -> EngineOutput {
        eprintln!("handle_character_release: c='{}', marker_state={:?}, marker_keys_held={:?}",
                  c, self.marker_state, self.marker_keys_held);
        
        // Track marker state transitions on key release
        if self.marker_state == MarkerState::FirstPressed {
            let key_str = c.to_string();
            eprintln!("Removing '{}' from marker_keys_held", key_str);
            self.marker_keys_held.remove(&key_str);
            
            // If all keys are released, transition to FirstReleased
            if self.marker_keys_held.is_empty() {
                eprintln!("All keys released, transitioning to FirstReleased");
                self.marker_state = MarkerState::FirstReleased;
            }
        }
        
        if self.marker_state == MarkerState::KanchokuSecondPressed {
            let key_str = c.to_string();
            eprintln!("Kanchoku: Removing '{}' from marker_keys_held", key_str);
            self.marker_keys_held.remove(&key_str);
            
            // Don't transition state yet - wait for space release to process Kanchoku
            eprintln!("Kanchoku: Key released, marker_keys_held={:?}", self.marker_keys_held);
        }
        
        eprintln!("After release: marker_state={:?}, marker_keys_held={:?}",
                  self.marker_state, self.marker_keys_held);
        
        // Return current preedit state instead of passthrough to preserve preedit display
        self.build_preedit_output()
    }

    fn trigger_conversion(&mut self) -> EngineOutput {
        if self.preedit_string.is_empty() {
            return EngineOutput::passthrough(self.mode);
        }
        
        // Use preedit_string which includes both hiragana and pending
        self.conversion_yomi = self.preedit_string.clone();
        self.in_conversion = true;
        
        eprintln!("Triggering conversion for yomi: '{}'", self.conversion_yomi);
        let candidates = self.henkan_processor.convert(&self.conversion_yomi).to_vec();
        let is_bunsetsu = self.henkan_processor.is_bunsetsu_mode();
        eprintln!("Got {} candidates, is_bunsetsu_mode={}", candidates.len(), is_bunsetsu);
        
        let mut output = EngineOutput::empty(self.mode);
        output.consumed = true;
        output.show_candidates = true;
        output.candidates = candidates.clone();
        output.candidate_cursor_pos = 0;
        
        if let Some(first) = candidates.first() {
            self.preedit_string = first.surface.clone();
            eprintln!("Set preedit_string to first candidate: '{}'", self.preedit_string);
        }
        
        output.preedit_segments = self.build_preedit_segments();
        eprintln!("Built {} preedit segments", output.preedit_segments.len());
        
        output
    }

    fn confirm_conversion(&mut self) -> EngineOutput {
        if !self.in_conversion {
            return EngineOutput::passthrough(self.mode);
        }
        
        let commit = self.preedit_string.clone();
        
        self.in_conversion = false;
        self.bunsetsu_active = false;
        self.conversion_yomi.clear();
        self.reset_preedit();
        
        EngineOutput::commit(commit, self.mode)
    }

    fn cancel_conversion(&mut self) -> EngineOutput {
        if !self.in_conversion {
            return EngineOutput::passthrough(self.mode);
        }
        
        self.preedit_string = self.conversion_yomi.clone();
        self.in_conversion = false;
        
        self.build_preedit_output()
    }

    fn handle_down_arrow(&mut self) -> EngineOutput {
        if !self.in_conversion {
            return EngineOutput::passthrough(self.mode);
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
            return EngineOutput::passthrough(self.mode);
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
            return EngineOutput::passthrough(self.mode);
        }
        
        self.henkan_processor.next_bunsetsu();
        self.build_conversion_output()
    }

    fn handle_left_arrow(&mut self) -> EngineOutput {
        if !self.in_conversion || !self.henkan_processor.is_bunsetsu_mode() {
            return EngineOutput::passthrough(self.mode);
        }
        
        self.henkan_processor.previous_bunsetsu();
        self.build_conversion_output()
    }

    fn get_engine_state(&self) -> EngineState {
        if self.in_conversion {
            EngineState::Converting
        } else if self.in_forced_preedit {
            EngineState::ForcedPreedit
        } else if self.bunsetsu_active {
            EngineState::Bunsetsu
        } else {
            EngineState::Normal
        }
    }

    fn build_preedit_output(&self) -> EngineOutput {
        let mut output = EngineOutput::empty(self.mode);
        output.consumed = true;
        output.preedit_segments = self.build_preedit_segments();
        output.preedit_cursor_pos = self.preedit_string.chars().count();
        output.marker_state = self.marker_state;
        output.engine_state = self.get_engine_state();
        eprintln!("Built preedit output: segments.len()={}, preedit_string='{}', cursor_pos={}",
                  output.preedit_segments.len(), self.preedit_string, output.preedit_cursor_pos);
        output
    }

    fn build_conversion_output(&self) -> EngineOutput {
        let mut output = EngineOutput::empty(self.mode);
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

        output.marker_state = self.marker_state;
        output.engine_state = self.get_engine_state();
        output
    }

    fn build_preedit_segments(&self) -> Vec<PreeditSegment> {
        eprintln!("build_preedit_segments: in_conversion={}, is_bunsetsu_mode={}, preedit_string='{}'",
                  self.in_conversion, self.henkan_processor.is_bunsetsu_mode(), self.preedit_string);
        
        if self.in_conversion && self.henkan_processor.is_bunsetsu_mode() {
            let segments = self.henkan_processor
                .get_display_surface_with_selection()
                .into_iter()
                .map(|(text, is_selected)| PreeditSegment { text, is_selected })
                .collect::<Vec<_>>();
            eprintln!("Returning {} bunsetsu segments", segments.len());
            segments
        } else {
            let segment = vec![PreeditSegment {
                text: self.preedit_string.clone(),
                is_selected: false,
            }];
            eprintln!("Returning single segment with text: '{}'", self.preedit_string);
            segment
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
        let mut output = EngineOutput::empty(self.mode);
        
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

        // For tests, create engine with default config directly
        let config = serde_json::json!({
            "enable_hiragana_key": ["Henkan", "Convert"],
            "disable_hiragana_key": ["Muhenkan", "NonConvert"],
        });

        PSKKEngine {
            mode: InputMode::Alphanumeric,
            simul_processor: simul,
            kanchoku_processor: kanchoku,
            henkan_processor: henkan,
            config,
            preedit_string: String::new(),
            preedit_hiragana: String::new(),
            preedit_ascii: String::new(),
            preedit_pending: String::new(),
            marker_state: MarkerState::Idle,
            marker_first_key: None,
            marker_second_key: None,
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

    #[test]
    fn alphanumeric_mode_passes_through() {
        let mut engine = create_test_engine();
        assert_eq!(engine.get_mode(), ProtoInputMode::Alphanumeric);

        let output = engine.process_key_event(Some('a'), "a", true, None);
        assert!(!output.consumed);
    }

    #[test]
    fn mode_switching_commits_preedit() {
        let mut engine = create_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        engine.process_key_event(Some('a'), "a", true, None);
        assert!(!engine.preedit_string.is_empty());

        let output = engine.set_mode(ProtoInputMode::Alphanumeric);
        assert!(output.commit_string.is_some());
        assert!(engine.preedit_string.is_empty());
    }

    #[test]
    fn character_input_builds_preedit() {
        let mut engine = create_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        let output = engine.process_key_event(Some('a'), "a", true, None);
        assert!(output.consumed);
        assert_eq!(engine.preedit_string, "あ");

        let output = engine.process_key_event(Some('i'), "i", true, None);
        assert!(output.consumed);
        assert_eq!(engine.preedit_string, "あい");
    }

    #[test]
    fn backspace_removes_character() {
        let mut engine = create_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        engine.process_key_event(Some('a'), "a", true, None);
        engine.process_key_event(Some('i'), "i", true, None);
        assert_eq!(engine.preedit_string, "あい");

        let output = engine.process_key_event(None, "BackSpace", true, None);
        assert!(output.consumed);
        assert_eq!(engine.preedit_string, "あ");
    }

    #[test]
    fn space_triggers_conversion() {
        let mut engine = create_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        engine.process_key_event(Some('a'), "a", true, None);
        engine.process_key_event(Some('i'), "i", true, None);

        let output = engine.process_key_event(Some(' '), "space", true, None);
        assert!(output.consumed);
        assert!(output.show_candidates);
        assert!(!output.candidates.is_empty());
        assert_eq!(output.candidates[0].surface, "愛");
    }

    #[test]
    fn enter_confirms_conversion() {
        let mut engine = create_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        engine.process_key_event(Some('a'), "a", true, None);
        engine.process_key_event(Some('i'), "i", true, None);
        engine.process_key_event(Some(' '), "space", true, None);

        assert!(engine.in_conversion);

        let output = engine.process_key_event(None, "Return", true, None);
        assert!(output.consumed);
        assert_eq!(output.commit_string, Some("愛".to_string()));
        assert!(!engine.in_conversion);
        assert!(engine.preedit_string.is_empty());
    }

    #[test]
    fn escape_cancels_conversion() {
        let mut engine = create_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        engine.process_key_event(Some('a'), "a", true, None);
        engine.process_key_event(Some('i'), "i", true, None);
        engine.process_key_event(Some(' '), "space", true, None);

        assert!(engine.in_conversion);

        let output = engine.process_key_event(None, "Escape", true, None);
        assert!(output.consumed);
        assert!(!engine.in_conversion);
        assert_eq!(engine.preedit_string, "あい");
    }

    #[test]
    fn ctrl_key_commits_and_passes_through() {
        let mut engine = create_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        engine.process_key_event(Some('a'), "a", true, None);
        assert!(!engine.preedit_string.is_empty());

        let modifiers = ProtoKeyModifiers { shift: false, ctrl: true, alt: false };
        let output = engine.process_key_event(Some('c'), "c", true, Some(modifiers));
        assert!(!output.consumed, "Ctrl+C should pass through to application");
        assert!(output.commit_string.is_some(), "Should commit preedit before passing through");
        assert!(engine.preedit_string.is_empty(), "Preedit should be cleared after commit");
    }

    #[test]
    fn mode_switch_accepts_henkan_for_convert_alias() {
        let mut engine = create_test_engine();
        assert_eq!(engine.get_mode(), ProtoInputMode::Alphanumeric);

        let _output = engine.process_key_event(None, "Henkan", true, None);
        assert_eq!(engine.get_mode(), ProtoInputMode::Hiragana);
    }

    #[test]
    fn mode_switch_accepts_muhenkan_for_nonconvert_alias() {
        let mut engine = create_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        let _output = engine.process_key_event(None, "Muhenkan", true, None);
        assert_eq!(engine.get_mode(), ProtoInputMode::Alphanumeric);
    }
}
