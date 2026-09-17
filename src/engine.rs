use crate::grpc::proto::{
    InputMode as ProtoInputMode,
    KeyModifiers as ProtoKeyModifiers,
    ResponseStatus as ProtoResponseStatus,
};
use crate::henkan::{Candidate, HenkanProcessor};
use crate::kanchoku::KanchokuProcessor;
use crate::keybinding::{KeyBinding, Modifiers};
use crate::simultaneous_processor::SimultaneousInputProcessor;
use crate::util::{get_config_data, get_layout_data, get_user_config_dir};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, trace};

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
    pub status: ProtoResponseStatus,
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
            status: ProtoResponseStatus::Ok,
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
    pressed_keys: std::collections::HashSet<String>,
    marker_had_input: bool,
    preedit_before_marker: String,
    
    pure_kanchoku_held: bool,
    pure_kanchoku_first_key: Option<char>,
    
    engine_state: EngineState,
    conversion_yomi: String,
    
    // Full configuration
    config: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Opt-in key-event trace
//
// Diagnostic aid for key-order/timing problems (e.g. "the bunsetsu marker only
// works if I hold space long enough"). When enabled, every key event logs an
// `in` line (as received by the engine) and an `out` line (the engine's
// decision). The lines carry a wall-clock timestamp `t` (epoch ms, directly
// comparable with the IBus client's trace) plus `dt_ms`, the gap since the
// previous traced event in this process, so the real order and spacing of the
// events can be read off the log.
//
// Enable with `"trace_keys": true` in ~/.config/pskk/config.json, or with the
// PSKK_TRACE_KEYS env var (which overrides the config).
// ---------------------------------------------------------------------------
static TRACE_ORIGIN: OnceLock<Instant> = OnceLock::new();
static TRACE_SEQ: AtomicU64 = AtomicU64::new(0);
static TRACE_LAST_MS: AtomicU64 = AtomicU64::new(0);
static TRACE_ENV: OnceLock<Option<bool>> = OnceLock::new();

fn trace_env_override() -> Option<bool> {
    *TRACE_ENV.get_or_init(|| {
        std::env::var("PSKK_TRACE_KEYS")
            .ok()
            .map(|v| !matches!(v.as_str(), "" | "0" | "false" | "False" | "off"))
    })
}

fn epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
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

    /// Whether the opt-in key-event trace is enabled (env var beats config).
    fn trace_enabled(&self) -> bool {
        trace_env_override().unwrap_or_else(|| {
            self.config
                .get("trace_keys")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
    }

    /// Emit one line of the key-event trace; see the statics above.
    fn key_trace(&self, phase: &str, key_name: &str, is_pressed: bool, detail: &str) {
        let origin = TRACE_ORIGIN.get_or_init(Instant::now);
        let rel_ms = origin.elapsed().as_secs_f64() * 1000.0;
        let rel_u = rel_ms as u64;
        let prev = TRACE_LAST_MS.swap(rel_u, Ordering::Relaxed);
        let seq = TRACE_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
        info!(
            target: "pskk::keytrace",
            "KEYTRACE src=server seq={} phase={} t={} dt_ms={} rel_ms={:.1} key='{}' pressed={} mode={:?} marker={:?} engine={:?} preedit='{}' pending='{}' {}",
            seq,
            phase,
            epoch_ms(),
            rel_u.saturating_sub(prev),
            rel_ms,
            key_name,
            is_pressed,
            self.mode,
            self.marker_state,
            self.engine_state,
            self.preedit_string,
            self.preedit_pending,
            detail
        );
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
            info!("Using provided layout data with {} entries", simul_processor.layout_data.as_ref().unwrap().len());
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

            info!("Loaded {} layout entries from config", layout_entries.len());
            SimultaneousInputProcessor::new(Some(layout_entries))
        };

        // Load Kanchoku layout from config if not already provided
        let kanchoku_processor = if kanchoku_processor.has_layout() {
            info!("Using provided Kanchoku layout");
            kanchoku_processor
        } else {
            use crate::util::get_kanchoku_layout;
            let kanchoku_layout_json = get_kanchoku_layout(&config)
                .map_err(|e| format!("Failed to load Kanchoku layout: {}", e))?;
            
            // Parse Kanchoku layout JSON into nested HashMap
            let kanchoku_layout = crate::kanchoku::parse_kanchoku_layout(&kanchoku_layout_json)
                .ok_or_else(|| "Failed to parse Kanchoku layout".to_string())?;
            
            info!("Loaded Kanchoku layout with {} first-stroke keys", kanchoku_layout.len());
            KanchokuProcessor::new(Some(kanchoku_layout))
        };

        // Load the pass-through (prefix/suffix) dictionary and its discount
        // weight from config. A missing dictionary file yields an empty
        // pass-through dictionary (no composed candidates).
        let mut henkan_processor = henkan_processor;
        let passthrough_discount = config
            .get("passthrough_discount")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.09);
        let passthrough_dict = crate::passthrough::PassThroughDictionary::load(
            &get_user_config_dir().join("pass_through_dictionary.json"),
        );
        henkan_processor.load_passthrough_dictionary(passthrough_dict, passthrough_discount);

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
            pressed_keys: std::collections::HashSet::new(),
            marker_had_input: false,
            preedit_before_marker: String::new(),
            pure_kanchoku_held: false,
            pure_kanchoku_first_key: None,
            engine_state: EngineState::Normal,
            conversion_yomi: String::new(),
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

        // Reload the pass-through dictionary and discount along with the config
        let passthrough_discount = self
            .config
            .get("passthrough_discount")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.09);
        let passthrough_dict = crate::passthrough::PassThroughDictionary::load(
            &get_user_config_dir().join("pass_through_dictionary.json"),
        );
        self.henkan_processor
            .load_passthrough_dictionary(passthrough_dict, passthrough_discount);

        info!("Config reloaded.");

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

    /// Load the kana-to-kanji dictionary into the engine after startup.
    pub fn load_henkan_dictionary(&mut self, dictionary: crate::util::Dictionary) {
        self.henkan_processor.load_dictionary(dictionary);
    }

    /// Build an EngineOutput that tells the client the henkan dictionary is still loading.
    fn henkan_unavailable_output(&self) -> EngineOutput {
        let mut output = self.build_preedit_output();
        output.consumed = true;
        output.status = ProtoResponseStatus::HenkanUnavailable;
        output
    }

    /// Entry point for key events. Emits the opt-in key-event trace around the
    /// real handler so that every early return is captured.
    pub fn process_key_event(
        &mut self,
        key_char: Option<char>,
        key_name: &str,
        is_pressed: bool,
        modifiers: Option<ProtoKeyModifiers>,
    ) -> EngineOutput {
        let trace = self.trace_enabled();
        if trace {
            self.key_trace("in", key_name, is_pressed, "");
        }

        let output = self.process_key_event_inner(key_char, key_name, is_pressed, modifiers);

        if trace {
            let commit = output.commit_string.as_deref().unwrap_or("");
            let detail = format!(
                "consumed={} status={:?} commit='{}'",
                output.consumed, output.status, commit
            );
            self.key_trace("out", key_name, is_pressed, &detail);
        }

        output
    }

    fn process_key_event_inner(
        &mut self,
        key_char: Option<char>,
        key_name: &str,
        is_pressed: bool,
        modifiers: Option<ProtoKeyModifiers>,
    ) -> EngineOutput {
        let (has_shift, has_ctrl, has_alt, has_super) = match modifiers {
            Some(m) => (m.shift, m.ctrl, m.alt, m.super_),
            None => (false, false, false, false),
        };
        
        // Pass through Super key combos (system shortcuts like Super+Space for IME switching)
        if has_super {
            eprintln!("Super key detected, passing through");
            return EngineOutput::passthrough(self.mode);
        }

        // Track key releases in every mode, so a key that was pressed in Hiragana
        // mode but released after switching to Alphanumeric (where presses are not
        // tracked) does not stay stuck as "held".
        if !is_pressed {
            self.pressed_keys.remove(key_name);
        }
        // Check for mode switching keys first (before mode check)
        if is_pressed {
            // debug!("KEY PRESSED: '{}' (char: {:?})", key_name, key_char);

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

            // debug!("  Checking against enable_keys: {:?}", enable_hiragana_keys);
            // debug!("  Checking against disable_keys: {:?}", disable_hiragana_keys);

            // Check for enable hiragana keys (Convert, etc.)
            if enable_hiragana_keys
                .iter()
                .any(|k| Self::mode_switch_key_matches(k, key_name))
            {
                // debug!("  ✓ MATCHED enable key! Switching to Hiragana");
                return self.set_mode(ProtoInputMode::Hiragana);
            }

            // Check for disable hiragana keys (NonConvert, etc.)
            if disable_hiragana_keys
                .iter()
                .any(|k| Self::mode_switch_key_matches(k, key_name))
            {
                // debug!("  ✓ MATCHED disable key! Switching to Alphanumeric");
                return self.set_mode(ProtoInputMode::Alphanumeric);
            }

            // debug!("  ✗ No match");
        }
        
        eprintln!("Current mode: {:?}, is_pressed: {}", self.mode, is_pressed);
        
        if self.mode == InputMode::Alphanumeric {
            eprintln!("  -> In Alphanumeric mode, passing through");
            return EngineOutput::passthrough(self.mode);
        }

        // Hiragana mode: suppress OS key-repeat events for character-input keys
        // (a key pressed again without an intervening release). Editing keys such
        // as BackSpace/Delete/arrows are excluded so holding them still repeats.
        if is_pressed && Self::is_repeat_suppressed_key(key_name) {
            if self.pressed_keys.contains(key_name) {
                debug!("Key repeat detected for '{}', ignoring", key_name);
                return self.build_repeat_output();
            }
            self.pressed_keys.insert(key_name.to_string());
        }

        eprintln!("  -> Processing in Hiragana mode");
        self.process_hiragana_mode_key(key_char, key_name, is_pressed, has_shift, has_ctrl, has_alt)
    }

    /// Check if a key is relevant to IME processing (whitelist)
    /// Keys not in this list will trigger commit + passthrough behavior
    fn is_ime_relevant_key(key_name: &str) -> bool {
        match key_name {
            // Alphanumeric characters
            "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" |
            "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" |
            "u" | "v" | "w" | "x" | "y" | "z" |
            "A" | "B" | "C" | "D" | "E" | "F" | "G" | "H" | "I" | "J" |
            "K" | "L" | "M" | "N" | "O" | "P" | "Q" | "R" | "S" | "T" |
            "U" | "V" | "W" | "X" | "Y" | "Z" |
            "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => true,
            
            // Conversion and editing keys
            "space" | "Space" | "Return" | "KP_Enter" | "Enter" | 
            "Escape" | "BackSpace" | "Backspace" | "Delete" => true,
            
            // Arrow keys (context-dependent behavior, but in whitelist)
            "Up" | "Down" | "Left" | "Right" |
            "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" => true,
            
            // Tab (behavior to be configured later)
            "Tab" | "ISO_Left_Tab" => true,
            
            // Common punctuation (used in layouts and kanchoku)
            "comma" | "period" | "slash" | "backslash" | "minus" | "equal" |
            "bracketleft" | "bracketright" | "semicolon" | "apostrophe" |
            "grave" | "asciitilde" | "exclam" | "at" | "numbersign" |
            "dollar" | "percent" | "asciicircum" | "ampersand" | "asterisk" |
            "parenleft" | "parenright" | "underscore" | "plus" |
            "braceleft" | "braceright" | "bar" | "colon" | "quotedbl" |
            "less" | "greater" | "question" => true,
            
            // Modifier keys (don't trigger commit when pressed alone)
            "Control_L" | "Control_R" | "Alt_L" | "Alt_R" |
            "Shift_L" | "Shift_R" | "Super_L" | "Super_R" |
            "Meta_L" | "Meta_R" | "Hyper_L" | "Hyper_R" => true,
            
            // Japanese-specific keys
            "Henkan" | "Muhenkan" | "Convert" | "NonConvert" |
            "Hiragana_Katakana" | "Zenkaku_Hankaku" | "Eisu_toggle" => true,
            
            // Everything else is not IME-relevant
            _ => false,
        }
    }

    /// Keys whose OS key-repeat should be suppressed in Hiragana mode.
    /// Editing/navigation keys (BackSpace, Delete, arrows, ...) are excluded so
    /// holding them keeps the native repeat behavior (e.g. holding BackSpace
    /// deletes multiple characters).
    fn is_repeat_suppressed_key(key_name: &str) -> bool {
        !matches!(
            key_name,
            "space" | "Space"
                | "Return" | "KP_Enter" | "Enter"
                | "Escape"
                | "BackSpace" | "Backspace" | "Delete"
                | "Up" | "Down" | "Left" | "Right"
                | "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight"
                | "Tab" | "ISO_Left_Tab"
                | "Control_L" | "Control_R" | "Alt_L" | "Alt_R"
                | "Shift_L" | "Shift_R" | "Super_L" | "Super_R"
                | "Meta_L" | "Meta_R"
        )
    }
    
    /// Check if we should commit current state and passthrough the key
    /// This happens when a non-whitelisted key is pressed with active preedit/conversion
    fn should_commit_and_passthrough(&self, key_name: &str) -> bool {
        // If no active state, nothing to commit
        if self.preedit_string.is_empty() && self.engine_state == EngineState::Normal {
            return false;
        }
        
        // If it's an IME-relevant key, don't passthrough
        if Self::is_ime_relevant_key(key_name) {
            return false;
        }
        
        // Non-IME key with active preedit/conversion -> commit and passthrough
        true
    }
    
    /// Get the text to commit based on current engine state
    fn get_commit_text(&self) -> String {
        match self.engine_state {
            EngineState::Converting => {
                // In conversion mode, commit the current selection (preedit_string already contains it)
                self.preedit_string.clone()
            }
            EngineState::Bunsetsu | EngineState::ForcedPreedit | EngineState::Normal => {
                // In other modes, commit the preedit as-is
                self.preedit_string.clone()
            }
        }
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
        eprintln!("process_hiragana_mode_key: key_name='{}', is_pressed={}, has_ctrl={}, has_alt={}, preedit='{}'",
                  key_name, is_pressed, has_ctrl, has_alt, self.preedit_string);
        trace!("process_hiragana_mode_key: key_name='{}', is_pressed={}, engine_state={:?}",
                  key_name, is_pressed, self.engine_state);
        
        // Handle Ctrl/Alt combos - check for PSKK commands first
        // But ignore if the key itself is a modifier key (Ctrl, Alt, Shift, etc.)
        let is_modifier_key = matches!(key_name,
            "Control_L" | "Control_R" | "Alt_L" | "Alt_R" |
            "Shift_L" | "Shift_R" | "Super_L" | "Super_R" |
            "Meta_L" | "Meta_R" | "Hyper_L" | "Hyper_R"
        );
        
        // If this is a modifier key press/release, return current state without consuming
        if is_modifier_key {
            eprintln!("Modifier key detected: {}, preedit_string='{}', engine_state={:?}",
                     key_name, self.preedit_string, self.engine_state);
            return self.build_current_output_passthrough();
        }
        
        // Check for non-whitelisted keys that should trigger commit + passthrough
        // This must be checked BEFORE handling specific keys, but AFTER modifier key check
        if is_pressed && self.should_commit_and_passthrough(key_name) {
            eprintln!("Non-whitelisted key '{}' with active state, committing and passing through", key_name);
            let commit = self.get_commit_text();
            self.reset_state();
            let mut output = EngineOutput::commit(commit, self.mode);
            output.consumed = false;  // Passthrough the key to application
            return output;
        }
        
        if is_pressed && (has_ctrl || has_alt) {
            // `super_` is always false here: Super combos are passed through to
            // the system above, before command handling. Bindings that mention
            // Super are still parsed and normalised, they are simply not
            // dispatched yet.
            let modifiers = Modifiers {
                ctrl: has_ctrl,
                alt: has_alt,
                shift: has_shift,
                super_: false,
            };
            // Try to handle as PSKK command
            if let Some(output) = self.handle_modifier_combo(key_char, key_name, modifiers) {
                return output;
            }
            
            // Not a PSKK command - commit and passthrough
            // This handles Ctrl/Alt combos with whitelisted keys that aren't PSKK commands
            if !self.preedit_string.is_empty() {
                let commit = self.get_commit_text();
                self.reset_state();
                let mut output = EngineOutput::commit(commit, self.mode);
                output.consumed = false;
                return output;
            }
            return EngineOutput::passthrough(self.mode);
        }

        if is_pressed {
            // Handle arrow keys: in Converting mode they navigate, otherwise commit + passthrough
            match key_name {
                "Down" | "ArrowDown" => {
                    if self.engine_state == EngineState::Converting {
                        return self.handle_down_arrow();
                    } else if !self.preedit_string.is_empty() {
                        // Commit preedit and passthrough arrow key
                        let commit = self.preedit_string.clone();
                        self.reset_state();
                        let mut output = EngineOutput::commit(commit, self.mode);
                        output.consumed = false;
                        return output;
                    }
                }
                "Up" | "ArrowUp" => {
                    if self.engine_state == EngineState::Converting {
                        return self.handle_up_arrow();
                    } else if !self.preedit_string.is_empty() {
                        let commit = self.preedit_string.clone();
                        self.reset_state();
                        let mut output = EngineOutput::commit(commit, self.mode);
                        output.consumed = false;
                        return output;
                    }
                }
                "Right" | "ArrowRight" => {
                    if self.engine_state == EngineState::Converting {
                        return self.handle_right_arrow();
                    } else if !self.preedit_string.is_empty() {
                        let commit = self.preedit_string.clone();
                        self.reset_state();
                        let mut output = EngineOutput::commit(commit, self.mode);
                        output.consumed = false;
                        return output;
                    }
                }
                "Left" | "ArrowLeft" => {
                    if self.engine_state == EngineState::Converting {
                        return self.handle_left_arrow();
                    } else if !self.preedit_string.is_empty() {
                        let commit = self.preedit_string.clone();
                        self.reset_state();
                        let mut output = EngineOutput::commit(commit, self.mode);
                        output.consumed = false;
                        return output;
                    }
                }
                "Return" | "KP_Enter" | "Enter" => return self.handle_enter(),
                "Escape" => return self.handle_escape(),
                "BackSpace" | "Backspace" => return self.handle_backspace(),
                // Space is always the bunsetsu marker. This is core to PSKK's
                // input model and is deliberately NOT configurable: there is no
                // config key for it (the former `kanchoku_bunsetsu_marker` was
                // removed for that reason).
                "space" | "Space" => return self.handle_space_press(key_char),
                _ => {}
            }
        } else {
            // Release of the fixed bunsetsu marker; see handle_space_press.
            if key_name == "space" || key_name == "Space" {
                return self.handle_space_release();
            }
            
            // For arrow keys in conversion mode, return conversion output on release
            if self.engine_state == EngineState::Converting {
                match key_name {
                    "Down" | "ArrowDown" | "Up" | "ArrowUp" | "Right" | "ArrowRight" | "Left" | "ArrowLeft" => {
                        debug!("Arrow key '{}' released in conversion mode, returning conversion output", key_name);
                        return self.build_conversion_output();
                    }
                    _ => {}
                }
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

        // For unhandled key releases (e.g. BackSpace release), preserve the
        // current preedit/conversion state so the client does not hide it.
        self.build_current_output_passthrough()
    }

    fn handle_enter(&mut self) -> EngineOutput {
        debug!("handle_enter: engine_state={:?}, preedit_string='{}', preedit_hiragana='{}', preedit_pending='{}'",
                  self.engine_state, self.preedit_string, self.preedit_hiragana, self.preedit_pending);
        
        if self.engine_state == EngineState::Converting {
            debug!("  -> Confirming conversion");
            return self.confirm_conversion();
        }
        
        if self.engine_state == EngineState::ForcedPreedit && !self.preedit_string.is_empty() {
            debug!("  -> Committing forced preedit: '{}' with consumed=false", self.preedit_string);
            let commit = self.preedit_string.clone();
            self.engine_state = EngineState::Normal;
            self.reset_preedit();
            
            let mut output = EngineOutput::commit(commit, self.mode);
            output.consumed = false;
            debug!("  -> Returning commit with consumed=false, commit_string='{:?}'", output.commit_string);
            return output;
        }
        
        if self.engine_state == EngineState::Bunsetsu && !self.preedit_string.is_empty() {
            debug!("  -> Committing bunsetsu preedit: '{}' with consumed=false", self.preedit_string);
            let commit = self.preedit_string.clone();
            self.engine_state = EngineState::Normal;
            self.reset_preedit();
            
            let mut output = EngineOutput::commit(commit, self.mode);
            output.consumed = false;
            debug!("  -> Returning commit with consumed=false, commit_string='{:?}'", output.commit_string);
            return output;
        }
        
        if !self.preedit_string.is_empty() {
            debug!("  -> Committing normal preedit: '{}' with consumed=false", self.preedit_string);
            let commit = self.preedit_string.clone();
            self.reset_preedit();
            
            let mut output = EngineOutput::commit(commit, self.mode);
            output.consumed = false;
            debug!("  -> Returning commit with consumed=false, commit_string='{:?}'", output.commit_string);
            return output;
        }
        
        debug!("  -> Passthrough (no preedit)");
        EngineOutput::passthrough(self.mode)
    }

    fn handle_escape(&mut self) -> EngineOutput {
        if self.engine_state == EngineState::Converting {
            return self.cancel_conversion();
        }
        
        if !self.preedit_string.is_empty() {
            self.reset_preedit();
            return self.build_preedit_output();
        }
        
        EngineOutput::passthrough(self.mode)
    }

    fn handle_backspace(&mut self) -> EngineOutput {
        if self.engine_state == EngineState::Converting {
            return self.cancel_conversion();
        }

        if !self.preedit_string.is_empty() {
            // If there is pending (raw) input, remove one pending char first.
            // Otherwise, remove the last confirmed kana and its associated ASCII.
            if !self.preedit_pending.is_empty() {
                self.preedit_pending.pop();
                // The pending kana's key stroke was recorded in `preedit_ascii`
                // when it was typed, so drop it together with the kana.
                self.preedit_ascii.pop();
            } else {
                if !self.preedit_hiragana.is_empty() {
                    self.preedit_hiragana.pop();
                }
                if !self.preedit_ascii.is_empty() {
                    self.preedit_ascii.pop();
                }
            }
            self.preedit_string = format!("{}{}", self.preedit_hiragana, self.preedit_pending);
            return self.build_preedit_output();
        }

        EngineOutput::passthrough(self.mode)
    }

    fn handle_modifier_combo(
        &mut self,
        key_char: Option<char>,
        key_name: &str,
        modifiers: Modifiers,
    ) -> Option<EngineOutput> {
        // Bindings are compared in PSKK's IMF-agnostic canonical form (see
        // `crate::keybinding`): the key is identified by the character it
        // produces wherever one exists, and modifiers are compared as a set,
        // so `Control+l` (settings UI / KeyboardEvent), `Ctrl+l` (canonical)
        // and any framework spelling of the same key all refer to each other.
        let event = KeyBinding::from_event(key_char, key_name, modifiers)?;
        debug!("Checking keybinding: {}", event.canonical());
        
        // Check conversion_keys config
        if let Some(conversion_keys) = self.config.get("conversion_keys").and_then(|v| v.as_object()) {
            // to_katakana (default: Ctrl+K)
            if let Some(keys) = conversion_keys.get("to_katakana").and_then(|v| v.as_array()) {
                if self.matches_key_binding(&event, keys) {
                    debug!("Matched to_katakana");
                    return Some(self.convert_to_katakana());
                }
            }
            
            // to_hiragana (default: Ctrl+J)
            if let Some(keys) = conversion_keys.get("to_hiragana").and_then(|v| v.as_array()) {
                if self.matches_key_binding(&event, keys) {
                    debug!("Matched to_hiragana");
                    return Some(self.convert_to_hiragana());
                }
            }
            
            // to_ascii (default: Ctrl+L)
            if let Some(keys) = conversion_keys.get("to_ascii").and_then(|v| v.as_array()) {
                if self.matches_key_binding(&event, keys) {
                    debug!("Matched to_ascii");
                    return Some(self.convert_to_ascii());
                }
            }
            
            // to_zenkaku (default: Ctrl+Shift+L)
            if let Some(keys) = conversion_keys.get("to_zenkaku").and_then(|v| v.as_array()) {
                if self.matches_key_binding(&event, keys) {
                    debug!("Matched to_zenkaku");
                    return Some(self.convert_to_zenkaku());
                }
            }
        }
        
        // Check force_commit_key (default: Ctrl+O)
        if let Some(keys) = self.config.get("force_commit_key").and_then(|v| v.as_array()) {
            if self.matches_key_binding(&event, keys) {
                debug!("Matched force_commit_key");
                if !self.preedit_string.is_empty() {
                    let commit = self.preedit_string.clone();
                    self.reset_state();
                    return Some(EngineOutput::commit(commit, self.mode));
                }
                // Empty preedit, passthrough to app
                return Some(EngineOutput::passthrough(self.mode));
            }
        }
        
        // Check user_dictionary_editor_trigger (default: Ctrl+Shift+R)
        if let Some(keys) = self.config.get("user_dictionary_editor_trigger").and_then(|v| v.as_array()) {
            if self.matches_key_binding(&event, keys) {
                debug!("Matched user_dictionary_editor_trigger");
                // TODO: Implement dictionary editor trigger
                // For now, just passthrough
                return Some(EngineOutput::passthrough(self.mode));
            }
        }
        
        // Not a PSKK command
        None
    }
    
    /// Does the incoming key event match any of the configured bindings?
    /// Legacy spellings (`Control+semicolon`, `BackSpace`) are handled by the
    /// parser, so configs written before the canonical format still work.
    fn matches_key_binding(&self, event: &KeyBinding, config_keys: &[serde_json::Value]) -> bool {
        config_keys.iter().any(|v| {
            v.as_str()
                .and_then(KeyBinding::parse)
                .map_or(false, |binding| binding.matches(event))
        })
    }

    /// Key(s) that start forced-preedit mode when tapped as the *single* first
    /// key of a marker sequence. Read from `forced_preedit_trigger_key`
    /// (default `["f"]`; an empty list disables the trigger).
    ///
    /// Entries are the same key strings the settings UI records from
    /// `event.key`, so a plain character such as "f" is the normal case. Only
    /// single-character entries can be matched, because the marker records its
    /// first key as a char: modifier combos and named keys ("Shift+F",
    /// "Space") are ignored with a debug message.
    fn forced_preedit_trigger_chars(&self) -> Vec<char> {
        let default_entries = vec![serde_json::Value::String("f".to_string())];
        let entries = self
            .config
            .get("forced_preedit_trigger_key")
            .and_then(|v| v.as_array())
            .unwrap_or(&default_entries);

        entries
            .iter()
            .filter_map(|v| v.as_str())
            .filter_map(|spec| {
                let mut chars = spec.trim().chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => Some(c),
                    _ => {
                        debug!(
                            "Ignoring forced_preedit_trigger_key entry '{}': only single-character keys are supported",
                            spec
                        );
                        None
                    }
                }
            })
            .collect()
    }

    /// Space is the bunsetsu marker: while held, following keys are kanchoku
    /// strokes; pressing it with an existing preedit commits that preedit and
    /// arms the marker, and releasing it marks the start of the bunsetsu. This
    /// is fixed behaviour, not a user setting (see the dispatch in
    /// `process_hiragana_mode_key`).
    fn handle_space_press(&mut self, _key_char: Option<char>) -> EngineOutput {
        match self.marker_state {
            MarkerState::Idle => {
                // If already in conversion mode, just enter MarkerHeld to prepare for cycling
                if self.engine_state == EngineState::Converting {
                    debug!("Space pressed in conversion mode, entering MarkerHeld for cycling");
                    self.marker_state = MarkerState::MarkerHeld;
                    self.marker_had_input = false;
                    self.marker_keys_held.clear();
                    self.marker_first_key = None;
                    self.marker_second_key = None;
                    
                    // Return current conversion output to keep preedit visible
                    return self.build_conversion_output();
                }
                
                // If in bunsetsu mode with preedit, trigger conversion and enter MarkerHeld
                if self.engine_state == EngineState::Bunsetsu && !self.preedit_string.is_empty() {
                    if !self.henkan_processor.is_ready() {
                        return self.henkan_unavailable_output();
                    }
                    debug!("Space pressed in bunsetsu mode, triggering conversion and entering MarkerHeld");
                    self.marker_state = MarkerState::MarkerHeld;
                    self.preedit_before_marker = self.preedit_string.clone();
                    self.marker_had_input = true; // Treat as "had input" to prevent cycling on release
                    self.marker_keys_held.clear();
                    self.marker_first_key = None;
                    self.marker_second_key = None;
                    
                    let mut output = self.trigger_conversion();
                    output.marker_state = self.marker_state;
                    return output;
                }
                
                // If in forced preedit mode, save preedit and enter MarkerHeld for Kanchoku input
                if self.engine_state == EngineState::ForcedPreedit {
                    debug!("Space pressed in forced preedit mode, entering MarkerHeld for Kanchoku");
                    self.marker_state = MarkerState::MarkerHeld;
                    self.preedit_before_marker = self.preedit_string.clone();
                    self.marker_had_input = false;
                    self.marker_keys_held.clear();
                    self.marker_first_key = None;
                    self.marker_second_key = None;
                    
                    let mut output = self.build_preedit_output();
                    output.marker_state = self.marker_state;
                    return output;
                }
                
                // If there's existing preedit in normal mode, commit it first
                if !self.preedit_string.is_empty() {
                    debug!("Space pressed with existing preedit '{}', committing it", self.preedit_string);
                    let commit = self.preedit_string.clone();
                    self.reset_preedit();
                    
                    // Enter MarkerHeld state for potential marker input
                    self.marker_state = MarkerState::MarkerHeld;
                    self.preedit_before_marker.clear();
                    self.marker_had_input = false;
                    self.marker_keys_held.clear();
                    self.marker_first_key = None;
                    self.marker_second_key = None;
                    
                    let mut output = EngineOutput::commit(commit, self.mode);
                    output.marker_state = self.marker_state;
                    return output;
                }
                
                // No existing preedit - just enter MarkerHeld state
                self.marker_state = MarkerState::MarkerHeld;
                self.preedit_before_marker.clear();
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
        debug!("handle_space_release: marker_state={:?}, engine_state={:?}",
                  self.marker_state, self.engine_state);
        
        match self.marker_state {
            MarkerState::MarkerHeld => {
                if self.marker_had_input {
                    // Keys were pressed during this space hold, not a tap
                    // If in conversion mode, just return conversion output (don't cycle)
                    if self.engine_state == EngineState::Converting {
                        debug!("Space released after triggering conversion, staying in conversion");
                        self.marker_state = MarkerState::Idle;
                        self.marker_first_key = None;
                        self.marker_keys_held.clear();
                        return self.build_conversion_output();
                    }
                    // Otherwise just release cleanly
                    self.marker_state = MarkerState::Idle;
                    self.marker_first_key = None;
                    self.marker_keys_held.clear();
                    let mut output = EngineOutput::consumed(self.mode);
                    output.marker_state = self.marker_state;
                    output
                } else if self.engine_state == EngineState::Converting {
                    // CONVERTING state: cycle to next candidate
                    self.marker_state = MarkerState::Idle;
                    self.marker_first_key = None;
                    self.marker_keys_held.clear();
                    self.handle_down_arrow()
                } else if self.engine_state == EngineState::Bunsetsu || self.engine_state == EngineState::ForcedPreedit {
                    // BUNSETSU or FORCED_PREEDIT state: trigger conversion
                    if !self.henkan_processor.is_ready() {
                        return self.henkan_unavailable_output();
                    }
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
                if self.engine_state == EngineState::Converting {
                    debug!("Space released during conversion, returning conversion output");
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
        debug!("handle_marker_release_decision: marker_first_key={:?}, marker_second_key={:?}, marker_keys_held.is_empty()={}, preedit_string='{}', engine_state={:?}",
                  self.marker_first_key, self.marker_second_key, self.marker_keys_held.is_empty(), self.preedit_string, self.engine_state);
        
        // If in conversion mode, commit the conversion and activate bunsetsu mode
        if self.engine_state == EngineState::Converting {
            debug!("Committing conversion '{}' and activating bunsetsu mode", self.preedit_string);
            let commit = self.preedit_string.clone();
            self.engine_state = EngineState::Bunsetsu;
            self.conversion_yomi.clear();
            self.henkan_processor.reset();
            
            // Reset preedit but keep the first key's hiragana for bunsetsu mode
            self.reset_preedit();
            if let Some(first_char) = self.marker_first_key {
                let (output, pending) = self.simul_processor.get_layout_output("", &first_char.to_string(), true);
                if let Some(ref out) = output {
                    if !out.is_empty() {
                        self.preedit_hiragana.push_str(out);
                        self.preedit_ascii.push(first_char);
                    }
                }
                self.preedit_pending = pending.unwrap_or_default();
                self.preedit_string = format!("{}{}", self.preedit_hiragana, self.preedit_pending);
            }
            self.marker_first_key = None;
            self.marker_second_key = None;
            self.marker_keys_held.clear();
            self.marker_state = MarkerState::Idle;
            
            let mut output = EngineOutput::commit(commit, self.mode);
            output.preedit_segments = self.build_preedit_segments();
            output.preedit_cursor_pos = self.preedit_string.chars().count();
            output.engine_state = self.get_engine_state();
            return output;
        }
        
        // Check for forced preedit trigger key first (before bunsetsu logic).
        // Only a *single* tap of a trigger key (no second stroke) enters
        // forced preedit. When the trigger key is the first of a two-stroke
        // kanchoku pair (e.g. f + s -> 二), the pair must be handled as
        // kanchoku instead of re-triggering the mode.
        // The trigger key(s) come from `forced_preedit_trigger_key` in config.
        if let Some(trigger_char) = self.marker_first_key {
            let is_trigger = self.marker_second_key.is_none()
                && self.forced_preedit_trigger_chars().contains(&trigger_char);
            if is_trigger {
                debug!(
                    "Entering forced preedit mode, clearing '{}' trigger from preedit",
                    trigger_char
                );
                self.engine_state = EngineState::ForcedPreedit;
                self.marker_first_key = None;
                self.marker_keys_held.clear();
                self.marker_state = MarkerState::Idle;

                // Restore preedit to state before marker (remove the trigger character)
                self.preedit_string = self.preedit_before_marker.clone();
                self.preedit_hiragana = self.preedit_before_marker.clone();
                self.preedit_pending.clear();
                self.preedit_ascii.clear();

                return self.build_preedit_output();
            }
        }
        
        // Decide as soon as the marker is released; do not additionally wait for
        // the character keys to be released. Key events can arrive in any order,
        // so fast typing often releases the marker *before* the character keys
        // (observed in the field: space up ~30 ms before the last key up).
        // Requiring `marker_keys_held` to be empty silently dropped bunsetsu
        // mode in exactly that case. A resolved pair (`marker_second_key`) has
        // already produced its preedit by now, and for a single key there is
        // nothing left to wait for once the marker itself is up.
        if self.marker_first_key.is_some() {
            if let Some(first_char) = self.marker_first_key {
                // Check if Kanchoku/simultaneous was already processed (second key exists)
                if self.marker_second_key.is_some() {
                    // After Kanchoku/simultaneous input, determine next state
                    if self.engine_state == EngineState::ForcedPreedit {
                        // In forced preedit mode, stay in forced preedit after Kanchoku
                        debug!("Kanchoku in forced preedit, staying in ForcedPreedit mode");
                    } else if !self.preedit_string.is_empty() {
                        // If preedit has content, activate bunsetsu mode (simultaneous input)
                        debug!("Simultaneous input processed, activating bunsetsu mode");
                        self.engine_state = EngineState::Bunsetsu;
                    } else {
                        // If preedit is empty, return to Idle (Kanchoku was committed)
                        debug!("Kanchoku already committed, returning to Idle");
                        self.engine_state = EngineState::Normal;
                    }
                    
                    self.marker_first_key = None;
                    self.marker_second_key = None;
                    self.marker_had_input = false;
                    self.marker_keys_held.clear();
                    self.marker_state = MarkerState::Idle;
                    
                    return self.build_preedit_output();
                }
                
                debug!("Checking kanchoku lookup for first_char='{}'", first_char);
                if let Some(kanji) = self.try_kanchoku_lookup(first_char) {
                    debug!("Kanchoku found: '{}'", kanji);
                    self.preedit_string = self.preedit_before_marker.clone();
                    self.preedit_string.push_str(&kanji);
                    self.preedit_hiragana = self.preedit_before_marker.clone();
                    
                    self.marker_first_key = None;
                    self.marker_second_key = None;
                    self.marker_had_input = false;
                    self.marker_keys_held.clear();
                    self.marker_state = MarkerState::Idle;
                    
                    return self.build_preedit_output();
                }
                
                debug!("No kanchoku, marking bunsetsu boundary with first_char='{}'", first_char);
                self.mark_bunsetsu_boundary(first_char);
                self.marker_first_key = None;
                self.marker_second_key = None;
                self.marker_keys_held.clear();
                self.marker_state = MarkerState::Idle;
                return self.build_preedit_output();
            }
        }
        
        debug!("No action taken, returning consumed");
        self.marker_first_key = None;
        self.marker_had_input = false;
        self.marker_keys_held.clear();
        self.marker_state = MarkerState::Idle;
        EngineOutput::consumed(self.mode)
    }

    fn try_kanchoku_lookup(&mut self, first_char: char) -> Option<String> {
        debug!("try_kanchoku_lookup: first_char='{}', marker_second_key={:?}", 
                  first_char, self.marker_second_key);
        
        // Check if we have both first and second keys for Kanchoku
        if let Some(second_char) = self.marker_second_key {
            debug!("Looking up Kanchoku: '{}' + '{}'", first_char, second_char);
            let kanji = self.kanchoku_processor.lookup_kanji(first_char, second_char);
            debug!("Kanchoku lookup result: '{}'", kanji);
            if kanji != crate::kanchoku::MISSING_KANCHOKU_KANJI {
                return Some(kanji);
            }
        }
        debug!("No valid Kanchoku pair found");
        None
    }

    fn mark_bunsetsu_boundary(&mut self, _first_char: char) {
        // Simply activate bunsetsu mode - the first_char was already processed
        // in handle_character_input, so we don't need to process it again
        self.engine_state = EngineState::Bunsetsu;
        debug!("Bunsetsu mode activated. Current preedit: '{}', preedit_hiragana: '{}', preedit_pending: '{}'",
                  self.preedit_string, self.preedit_hiragana, self.preedit_pending);
    }

    fn handle_character_input(&mut self, c: char, _has_shift: bool) -> EngineOutput {
        // `preedit_ascii` records the raw key characters that make up the
        // preedit, which is what `to_ascii`/`to_zenkaku` render. It has to be
        // appended here, once per key press: a key press only flushes the
        // *previous* kana (in kana-direct layouts) or resolves the previous
        // prefix (in romaji layouts), so recording it at flush time used to
        // skip the first key of every preedit and shift the rest by one
        // ("のとかん" typed as a s d f converted to "sdf" instead of "asdf").
        self.preedit_ascii.push(c);

        // Track marker state but don't block character processing
        if self.marker_state == MarkerState::MarkerHeld {
            self.marker_first_key = Some(c);
            self.marker_keys_held.insert(c.to_string());
            self.marker_had_input = true;
            self.marker_state = MarkerState::FirstPressed;
            debug!("First key '{}' pressed, transitioning to FirstPressed", c);
            
            // If in CONVERTING state, commit the conversion immediately and start new preedit
            if self.engine_state == EngineState::Converting {
                debug!("First key '{}' pressed during conversion, committing conversion and starting new preedit", c);
                let commit = self.preedit_string.clone();
                self.engine_state = EngineState::Normal;
                self.conversion_yomi.clear();
                self.reset_preedit();
                self.henkan_processor.reset();
                
                // Clear preedit_before_marker to prevent old preedit from being restored
                self.preedit_before_marker.clear();
                
                // Process the new character
                let (output, pending) = self.simul_processor.get_layout_output("", &c.to_string(), true);
                if let Some(ref out) = output {
                    if !out.is_empty() {
                        self.preedit_hiragana.push_str(out);
                    }
                }
                self.preedit_pending = pending.unwrap_or_default();
                self.preedit_string = format!("{}{}", self.preedit_hiragana, self.preedit_pending);
                
                // Return commit with new preedit
                let mut result = EngineOutput::commit(commit, self.mode);
                result.preedit_segments = self.build_preedit_segments();
                result.preedit_cursor_pos = self.preedit_string.chars().count();
                result.marker_state = self.marker_state;
                result.engine_state = self.get_engine_state();
                return result;
            }
            
            // If in BUNSETSU state, perform implicit conversion and commit before processing new character
            // Note: ForcedPreedit is NOT included here because we want to allow Kanchoku input
            if self.engine_state == EngineState::Bunsetsu {
                let yomi = self.preedit_string.clone();
                let commit = if !yomi.is_empty() {
                    if !self.henkan_processor.is_ready() {
                        return self.henkan_unavailable_output();
                    }
                    let candidates = self.henkan_processor.convert(&yomi).to_vec();
                    if let Some(first) = candidates.first() {
                        debug!("Immediate implicit conversion: '{}' → '{}'", yomi, first.surface);
                        first.surface.clone()
                    } else {
                        debug!("No candidates, committing yomi: '{}'", yomi);
                        yomi
                    }
                } else {
                    String::new()
                };
                
                self.engine_state = EngineState::Bunsetsu;
                self.reset_preedit();
                self.henkan_processor.reset();
                
                // Now process the new character and return commit + new preedit
                let (output, pending) = self.simul_processor.get_layout_output("", &c.to_string(), true);
                if let Some(ref out) = output {
                    if !out.is_empty() {
                        self.preedit_hiragana.push_str(out);
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

            debug!("Simul processor output: output={:?}, pending={:?}, preedit_pending='{}', char='{}'",
                  output, pending, self.preedit_pending, c);

            if let Some(ref out) = output {
                if !out.is_empty() {
                    self.preedit_hiragana.push_str(out);
                    debug!("Updated preedit_hiragana: '{}'", self.preedit_hiragana);
                }
            }

            self.preedit_pending = pending.unwrap_or_default();
            self.preedit_string = format!("{}{}", self.preedit_hiragana, self.preedit_pending);
            debug!("Final preedit_string: '{}' (hiragana='{}' + pending='{}')",
                      self.preedit_string, self.preedit_hiragana, self.preedit_pending);

            return self.build_preedit_output();
        }

        if self.marker_state == MarkerState::FirstPressed || self.marker_state == MarkerState::FirstReleased {
            // Second key pressed - check if it's simultaneous input first, then try Kanchoku
            self.marker_second_key = Some(c);
            self.marker_keys_held.insert(c.to_string());
            debug!("Second key '{}' pressed (state={:?}), checking simultaneous input first", c, self.marker_state);
            
            // First check if the two keys form a valid simultaneous input
            if let Some(first_char) = self.marker_first_key {
                // Check if adding the second key to the pending state produces simultaneous output
                let (simul_output, simul_pending) = self.simul_processor.get_layout_output(
                    &self.preedit_pending,
                    &c.to_string(),
                    true,
                );
                
                debug!("Checking simultaneous: pending='{}' + key='{}' → output={:?}, pending={:?}", 
                          self.preedit_pending, c, simul_output, simul_pending);
                
                // Check if we got a pending result (simultaneous input detected)
                // Simultaneous input returns empty output and the result in pending
                if let Some(ref pending_result) = simul_pending {
                    if !pending_result.is_empty() && simul_output.as_ref().map_or(true, |o| o.is_empty()) {
                        debug!("Simultaneous input found: '{}' + '{}' → '{}' (in pending)", first_char, c, pending_result);
                        self.preedit_hiragana.push_str(pending_result);
                        self.preedit_pending.clear();
                        self.preedit_string = self.preedit_hiragana.clone();
                        
                        self.marker_state = MarkerState::KanchokuSecondPressed;
                        return self.build_preedit_output();
                    }
                }

                // Some chords deliver their result in the *output* with an empty
                // pending (e.g. か + o -> み in the default layout), which the
                // pending-based check above misses. Detect those by re-checking
                // the layout for the full pending+key combo and verifying the
                // processor actually returned the chord's output. This also
                // respects the simultaneous-input time window: a timed-out chord
                // falls back to a different output and is correctly treated as
                // non-simultaneous (so kanchoku can still fire).
                if !self.preedit_pending.is_empty() {
                    let chord_key = format!("{}{}", self.preedit_pending, c);
                    let key_idx = chord_key.chars().count().saturating_sub(1);
                    let chord_output = self
                        .simul_processor
                        .simultaneous_map
                        .get(key_idx)
                        .and_then(|bucket| bucket.get(&chord_key))
                        .map(|entry| entry.output.clone());
                    if let Some(chord_output) = chord_output {
                        if !chord_output.is_empty()
                            && simul_output.as_deref() == Some(chord_output.as_str())
                            && simul_pending.as_deref() == Some("")
                        {
                            debug!("Simultaneous chord found: '{}' + '{}' → '{}' (in output)", first_char, c, chord_output);
                            self.preedit_hiragana.push_str(&chord_output);
                            self.preedit_pending.clear();
                            self.preedit_string = self.preedit_hiragana.clone();

                            self.marker_state = MarkerState::KanchokuSecondPressed;
                            return self.build_preedit_output();
                        }
                    }
                }
                
                debug!("No simultaneous input, attempting Kanchoku lookup");
                // No simultaneous input - try Kanchoku lookup
                if let Some(kanji) = self.try_kanchoku_lookup(first_char) {
                    debug!("Kanchoku found: '{}', committing immediately", kanji);
                    
                    // In normal mode: commit the kanji directly
                    if self.engine_state != EngineState::ForcedPreedit {
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
                        self.preedit_hiragana.push_str(&kanji);
                        self.preedit_pending.clear();
                        
                        self.marker_state = MarkerState::KanchokuSecondPressed;
                        
                        let mut output = self.build_preedit_output();
                        output.marker_state = self.marker_state;
                        return output;
                    }
                }
            }
            
            // Not a valid Kanchoku pair - treat as bunsetsu marker
            debug!("Not a valid Kanchoku pair, will activate bunsetsu on space release");
            self.marker_state = MarkerState::KanchokuSecondPressed;
            
            let mut output = EngineOutput::consumed(self.mode);
            output.marker_state = self.marker_state;
            output.engine_state = self.get_engine_state();
            return output;
        }

        if self.marker_state == MarkerState::KanchokuSecondPressed {
            // Additional key after Kanchoku - start a new Kanchoku sequence
            debug!("Additional key '{}' pressed in KanchokuSecondPressed, starting new Kanchoku sequence", c);
            
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

            debug!("Simul processor output: output={:?}, pending={:?}, preedit_pending='{}', char='{}'",
                  output, pending, self.preedit_pending, c);

            if let Some(ref out) = output {
                if !out.is_empty() {
                    self.preedit_hiragana.push_str(out);
                    debug!("Updated preedit_hiragana: '{}'", self.preedit_hiragana);
                }
            }

            self.preedit_pending = pending.unwrap_or_default();
            self.preedit_string = format!("{}{}", self.preedit_hiragana, self.preedit_pending);
            debug!("Final preedit_string: '{}' (hiragana='{}' + pending='{}')",
                      self.preedit_string, self.preedit_hiragana, self.preedit_pending);

            return self.build_preedit_output();
        }

        // If in CONVERTING state and typing a new character (without holding space),
        // commit the selected candidate and continue with the new character
        if self.engine_state == EngineState::Converting {
            debug!("Char input in CONVERTING: confirming '{}' and adding '{}'", self.preedit_string, c);
            let commit = self.preedit_string.clone();
            self.engine_state = EngineState::Normal;
            self.conversion_yomi.clear();
            self.reset_preedit();
            self.henkan_processor.reset();
            
            // Process the new character
            let (output, pending) = self.simul_processor.get_layout_output("", &c.to_string(), true);
            if let Some(ref out) = output {
                if !out.is_empty() {
                    self.preedit_hiragana.push_str(out);
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

        let (output, pending) = self.simul_processor.get_layout_output(
            &self.preedit_pending,
            &c.to_string(),
            true,
        );

        debug!("Simul processor output: output={:?}, pending={:?}, preedit_pending='{}', char='{}'",
              output, pending, self.preedit_pending, c);

        // For simultaneous input layouts, if output is empty but pending is not,
        // show the pending string in the preedit
        if let Some(ref out) = output {
            if !out.is_empty() {
                self.preedit_hiragana.push_str(out);
                debug!("Updated preedit_hiragana: '{}'", self.preedit_hiragana);
            }
        }

        self.preedit_pending = pending.unwrap_or_default();

        // For simultaneous input layouts, show both preedit_hiragana and preedit_pending
        // in the preedit display
        self.preedit_string = format!("{}{}", self.preedit_hiragana, self.preedit_pending);
        debug!("Final preedit_string: '{}' (hiragana='{}' + pending='{}')",
                  self.preedit_string, self.preedit_hiragana, self.preedit_pending);

        self.build_preedit_output()
    }

    fn handle_character_release(&mut self, c: char) -> EngineOutput {
        debug!("handle_character_release: c='{}', marker_state={:?}, marker_keys_held={:?}",
                  c, self.marker_state, self.marker_keys_held);
        
        // Always remove the released key from the held set, regardless of marker
        // state. A release can arrive after the marker already reset to Idle
        // (e.g. space released before the stroke key); without this the key
        // stays stuck in marker_keys_held and corrupts later marker flows.
        self.marker_keys_held.remove(&c.to_string());
        
        // Track marker state transitions on key release
        if self.marker_state == MarkerState::FirstPressed {
            let key_str = c.to_string();
            debug!("Removing '{}' from marker_keys_held", key_str);
            self.marker_keys_held.remove(&key_str);
            
            // If all keys are released, transition to FirstReleased
            if self.marker_keys_held.is_empty() {
                debug!("All keys released, transitioning to FirstReleased");
                self.marker_state = MarkerState::FirstReleased;
            }
        }
        
        if self.marker_state == MarkerState::KanchokuSecondPressed {
            let key_str = c.to_string();
            debug!("Kanchoku: Removing '{}' from marker_keys_held", key_str);
            self.marker_keys_held.remove(&key_str);
            
            // Don't transition state yet - wait for space release to process Kanchoku
            debug!("Kanchoku: Key released, marker_keys_held={:?}", self.marker_keys_held);
        }
        
        debug!("After release: marker_state={:?}, marker_keys_held={:?}",
                  self.marker_state, self.marker_keys_held);
        
        // Return appropriate output based on engine state
        if self.engine_state == EngineState::Converting {
            self.build_conversion_output()
        } else {
            self.build_preedit_output()
        }
    }

    fn trigger_conversion(&mut self) -> EngineOutput {
        if self.preedit_string.is_empty() {
            return EngineOutput::passthrough(self.mode);
        }

        if !self.henkan_processor.is_ready() {
            return self.henkan_unavailable_output();
        }

        // Use preedit_string which includes both hiragana and pending
        self.conversion_yomi = self.preedit_string.clone();
        self.engine_state = EngineState::Converting;
        
        debug!("Triggering conversion for yomi: '{}'", self.conversion_yomi);
        let candidates = self.henkan_processor.convert(&self.conversion_yomi).to_vec();
        let is_bunsetsu = self.henkan_processor.is_bunsetsu_mode();
        debug!("Got {} candidates, is_bunsetsu_mode={}", candidates.len(), is_bunsetsu);
        
        let mut output = EngineOutput::empty(self.mode);
        output.consumed = true;
        output.show_candidates = true;
        output.candidates = candidates.clone();
        output.candidate_cursor_pos = 0;
        
        if let Some(first) = candidates.first() {
            self.preedit_string = first.surface.clone();
            debug!("Set preedit_string to first candidate: '{}'", self.preedit_string);
        }
        
        output.preedit_segments = self.build_preedit_segments();
        debug!("Built {} preedit segments", output.preedit_segments.len());
        
        output
    }

    fn confirm_conversion(&mut self) -> EngineOutput {
        if self.engine_state != EngineState::Converting {
            return EngineOutput::passthrough(self.mode);
        }
        
        let commit = self.preedit_string.clone();
        
        self.engine_state = EngineState::Normal;
        self.conversion_yomi.clear();
        self.reset_preedit();
        
        EngineOutput::commit(commit, self.mode)
    }

    fn cancel_conversion(&mut self) -> EngineOutput {
        if self.engine_state != EngineState::Converting {
            return EngineOutput::passthrough(self.mode);
        }
        
        self.preedit_string = self.conversion_yomi.clone();
        self.engine_state = EngineState::Bunsetsu;
        
        self.build_preedit_output()
    }

    fn handle_down_arrow(&mut self) -> EngineOutput {
        if self.engine_state != EngineState::Converting {
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
        if self.engine_state != EngineState::Converting {
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
        if self.engine_state != EngineState::Converting || !self.henkan_processor.is_bunsetsu_mode() {
            return EngineOutput::passthrough(self.mode);
        }
        
        self.henkan_processor.next_bunsetsu();
        self.build_conversion_output()
    }

    fn handle_left_arrow(&mut self) -> EngineOutput {
        if self.engine_state != EngineState::Converting || !self.henkan_processor.is_bunsetsu_mode() {
            return EngineOutput::passthrough(self.mode);
        }
        
        self.henkan_processor.previous_bunsetsu();
        self.build_conversion_output()
    }

    fn get_engine_state(&self) -> EngineState {
        self.engine_state
    }

    fn build_preedit_output(&self) -> EngineOutput {
        let mut output = EngineOutput::empty(self.mode);
        output.consumed = true;
        output.preedit_segments = self.build_preedit_segments();
        output.preedit_cursor_pos = self.preedit_string.chars().count();
        output.marker_state = self.marker_state;
        output.engine_state = self.get_engine_state();
        debug!("Built preedit output: segments.len()={}, preedit_string='{}', cursor_pos={}",
                  output.preedit_segments.len(), self.preedit_string, output.preedit_cursor_pos);
        output
    }

    fn build_current_output_passthrough(&self) -> EngineOutput {
        let mut output = EngineOutput::empty(self.mode);
        output.consumed = false;  // Don't consume modifier keys
        
        // Preserve current preedit state if any
        if !self.preedit_string.is_empty() {
            output.preedit_segments = self.build_preedit_segments();
            output.preedit_cursor_pos = self.preedit_string.chars().count();
        }
        
        // Preserve conversion state if active
        if self.engine_state == EngineState::Converting {
            output.show_candidates = true;
            output.candidates = self.henkan_processor.get_candidates().to_vec();
            
            if let Some(selected) = self.henkan_processor.get_selected_candidate() {
                output.candidate_cursor_pos = self.henkan_processor.get_candidates()
                    .iter()
                    .position(|c| c.surface == selected.surface)
                    .unwrap_or(0);
            }
        }
        
        output.marker_state = self.marker_state;
        output.engine_state = self.get_engine_state();
        output
    }

    /// Output for a suppressed key-repeat event: consume it (so the app never
    /// receives the repeated character) but keep the current preedit/conversion
    /// UI visible.
    fn build_repeat_output(&self) -> EngineOutput {
        let mut output = self.build_current_output_passthrough();
        output.consumed = true;
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
        debug!("build_preedit_segments: engine_state={:?}, is_bunsetsu_mode={}, preedit_string='{}'",
                  self.engine_state, self.henkan_processor.is_bunsetsu_mode(), self.preedit_string);
        
        if self.engine_state == EngineState::Converting && self.henkan_processor.is_bunsetsu_mode() {
            let segments = self.henkan_processor
                .get_display_surface_with_selection()
                .into_iter()
                .map(|(text, is_selected)| PreeditSegment { text, is_selected })
                .collect::<Vec<_>>();
            debug!("Returning {} bunsetsu segments", segments.len());
            segments
        } else {
            if self.preedit_string.is_empty() {
                debug!("Preedit string is empty, returning empty segments");
                vec![]
            } else {
                let segment = vec![PreeditSegment {
                    text: self.preedit_string.clone(),
                    is_selected: false,
                }];
                debug!("Returning single segment with text: '{}'", self.preedit_string);
                segment
            }
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
        self.pressed_keys.clear();
        self.marker_had_input = false;
        self.preedit_before_marker.clear();
        self.pure_kanchoku_held = false;
        self.pure_kanchoku_first_key = None;
        self.engine_state = EngineState::Normal;
        self.conversion_yomi.clear();
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
    
    // Conversion methods for Ctrl+K/J/L commands
    /// Replace the visible preedit with `text`, staying in the normal (Idle)
    /// editing state, so the converted text stays editable in the preedit
    /// buffer instead of being committed to the application.
    /// Fold the pending (unconfirmed) input into the confirmed preedit.
    ///
    /// Pending input only exists while the user could still complete a
    /// simultaneous-input chord: the newest key is not part of
    /// `preedit_hiragana` until the next key arrives. Once a command acts on
    /// the preedit it has to be part of it, so the conversion keys call this
    /// before converting.
    fn flush_pending(&mut self) {
        if self.preedit_pending.is_empty() {
            return;
        }
        self.preedit_hiragana.push_str(&self.preedit_pending);
        self.preedit_pending.clear();
        self.preedit_string = self.preedit_hiragana.clone();
    }

    fn replace_preedit(&mut self, text: String) -> EngineOutput {
        if text.is_empty() {
            // Nothing to put in the preedit - keep it as it is rather than
            // clearing the buffer.
            return self.build_preedit_output();
        }

        if self.engine_state == EngineState::Converting {
            // A conversion key supersedes the candidate conversion.
            self.engine_state = EngineState::Normal;
            self.conversion_yomi.clear();
            self.henkan_processor.reset();
        }

        self.preedit_string = text.clone();
        self.preedit_hiragana = text;
        self.preedit_pending.clear();
        self.build_preedit_output()
    }

    fn convert_to_katakana(&mut self) -> EngineOutput {
        if self.preedit_string.is_empty() {
            return EngineOutput::passthrough(self.mode);
        }
        self.flush_pending();

        // Convert hiragana to katakana.
        let katakana = self
            .preedit_hiragana
            .chars()
            .map(|c| {
                if c >= 'ぁ' && c <= 'ん' {
                    // Hiragana to Katakana conversion (add 0x60)
                    char::from_u32(c as u32 + 0x60).unwrap_or(c)
                } else {
                    c
                }
            })
            .collect::<String>();

        self.replace_preedit(katakana)
    }

    fn convert_to_hiragana(&mut self) -> EngineOutput {
        if self.preedit_string.is_empty() {
            return EngineOutput::passthrough(self.mode);
        }
        self.flush_pending();

        // Katakana back to hiragana, so the conversion keys can be used as a
        // toggle over the same preedit.
        let hiragana = self
            .preedit_hiragana
            .chars()
            .map(|c| {
                if c >= 'ァ' && c <= 'ン' {
                    // Katakana to Hiragana conversion (subtract 0x60)
                    char::from_u32(c as u32 - 0x60).unwrap_or(c)
                } else {
                    c
                }
            })
            .collect::<String>();

        self.replace_preedit(hiragana)
    }

    fn convert_to_ascii(&mut self) -> EngineOutput {
        if self.preedit_string.is_empty() {
            return EngineOutput::passthrough(self.mode);
        }
        self.flush_pending();

        // Show the keystrokes behind the current preedit, in the preedit.
        let ascii = self.preedit_ascii.clone();
        self.replace_preedit(ascii)
    }

    fn convert_to_zenkaku(&mut self) -> EngineOutput {
        if self.preedit_string.is_empty() {
            return EngineOutput::passthrough(self.mode);
        }
        self.flush_pending();

        // Convert ASCII to full-width (zenkaku)
        let zenkaku = self.preedit_ascii.chars().map(|c| {
            if c >= '!' && c <= '~' {
                // ASCII to full-width conversion
                char::from_u32(c as u32 - 0x21 + 0xFF01).unwrap_or(c)
            } else if c == ' ' {
                '　' // Full-width space
            } else {
                c
            }
        }).collect::<String>();

        self.replace_preedit(zenkaku)
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
            pressed_keys: std::collections::HashSet::new(),
            marker_had_input: false,
            preedit_before_marker: String::new(),
            pure_kanchoku_held: false,
            pure_kanchoku_first_key: None,
            engine_state: EngineState::Normal,
            conversion_yomi: String::new(),
        }
    }

    /// Engine with a layout where d+o / o+d form the chord み, and a kanchoku
    /// layout where the same pairs map to 九 / 三 — used to verify that the
    /// simultaneous chord wins over kanchoku under the marker.
    fn create_chord_test_engine() -> PSKKEngine {
        let layout = vec![
            ("d".to_string(), "".to_string(), "か".to_string(), None),
            ("o".to_string(), "".to_string(), "が".to_string(), None),
            ("かo".to_string(), "み".to_string(), "".to_string(), None),
            ("がd".to_string(), "み".to_string(), "".to_string(), None),
        ];
        let simul = SimultaneousInputProcessor::new(Some(layout));

        let mut kanchoku_map: crate::kanchoku::KanchokuLayout = HashMap::new();
        let mut d_second = HashMap::new();
        d_second.insert('o', "九".to_string());
        kanchoku_map.insert('d', d_second);
        let mut o_second = HashMap::new();
        o_second.insert('d', "三".to_string());
        kanchoku_map.insert('o', o_second);
        let kanchoku = KanchokuProcessor::new(Some(kanchoku_map));

        let henkan = HenkanProcessor::new();

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
            pressed_keys: std::collections::HashSet::new(),
            marker_had_input: false,
            preedit_before_marker: String::new(),
            pure_kanchoku_held: false,
            pure_kanchoku_first_key: None,
            engine_state: EngineState::Normal,
            conversion_yomi: String::new(),
        }
    }

    #[test]
    fn chord_under_marker_beats_kanchoku_and_enters_bunsetsu() {
        let mut engine = create_chord_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        // space + d + o: chord かo -> み must win over kanchoku d:o -> 九
        engine.process_key_event(None, "space", true, None);
        engine.process_key_event(Some('d'), "d", true, None);
        let o = engine.process_key_event(Some('o'), "o", true, None);
        assert!(o.commit_string.is_none(), "kanchoku must not fire for a chord");
        let preedit = o.preedit_segments.iter().map(|s| s.text.clone()).collect::<String>();
        assert_eq!(preedit, "み");

        engine.process_key_event(Some('o'), "o", false, None);
        engine.process_key_event(Some('d'), "d", false, None);
        let o = engine.process_key_event(None, "space", false, None);
        assert_eq!(o.engine_state, EngineState::Bunsetsu, "space release must enter bunsetsu mode");
        let preedit = o.preedit_segments.iter().map(|s| s.text.clone()).collect::<String>();
        assert_eq!(preedit, "み");
    }

    #[test]
    fn reverse_order_chord_also_wins() {
        let mut engine = create_chord_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        // space + o + d: chord がd -> み must win over kanchoku o:d -> 三
        engine.process_key_event(None, "space", true, None);
        engine.process_key_event(Some('o'), "o", true, None);
        let o = engine.process_key_event(Some('d'), "d", true, None);
        assert!(o.commit_string.is_none());
        let preedit = o.preedit_segments.iter().map(|s| s.text.clone()).collect::<String>();
        assert_eq!(preedit, "み");

        engine.process_key_event(Some('d'), "d", false, None);
        engine.process_key_event(Some('o'), "o", false, None);
        let o = engine.process_key_event(None, "space", false, None);
        assert_eq!(o.engine_state, EngineState::Bunsetsu);
    }

    /// Regression: releasing the marker (space) *before* the character keys of a
    /// resolved chord have been released is ordinary fast typing - the events
    /// can arrive in any order - but it used to drop bunsetsu mode because
    /// `handle_marker_release_decision` insists on `marker_keys_held` being
    /// empty even when the pair has already been resolved.
    #[test]
    fn space_release_before_chord_keys_enters_bunsetsu() {
        let mut engine = create_chord_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        engine.process_key_event(None, "space", true, None);
        engine.process_key_event(Some('d'), "d", true, None);
        let o = engine.process_key_event(Some('o'), "o", true, None);
        assert!(o.commit_string.is_none(), "kanchoku must not fire for a chord");
        let preedit = o.preedit_segments.iter().map(|s| s.text.clone()).collect::<String>();
        assert_eq!(preedit, "み");

        // Field order: the marker is released between the two key releases
        // (t up, space up, o up in the original report).
        engine.process_key_event(Some('d'), "d", false, None);
        let o = engine.process_key_event(None, "space", false, None);
        assert_eq!(
            o.engine_state,
            EngineState::Bunsetsu,
            "space release must still enter bunsetsu mode when chord keys are held"
        );
        let preedit = o.preedit_segments.iter().map(|s| s.text.clone()).collect::<String>();
        assert_eq!(preedit, "み");

        // The late key release must not corrupt the state.
        let o = engine.process_key_event(Some('o'), "o", false, None);
        assert_eq!(o.engine_state, EngineState::Bunsetsu);
    }

    /// Same race as above but for a single (non-chord) key: the marker can be
    /// released before the character key, and the bunsetsu boundary must still
    /// be marked.
    #[test]
    fn space_release_before_single_key_enters_bunsetsu() {
        let mut engine = create_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        engine.process_key_event(None, "space", true, None);
        let o = engine.process_key_event(Some('a'), "a", true, None);
        let preedit = o.preedit_segments.iter().map(|s| s.text.clone()).collect::<String>();
        assert_eq!(preedit, "あ");

        // Space released while 'a' is still held.
        let o = engine.process_key_event(None, "space", false, None);
        assert_eq!(
            o.engine_state,
            EngineState::Bunsetsu,
            "space release must still enter bunsetsu mode when the key is held"
        );

        let o = engine.process_key_event(Some('a'), "a", false, None);
        assert_eq!(o.engine_state, EngineState::Bunsetsu);
        let preedit = o.preedit_segments.iter().map(|s| s.text.clone()).collect::<String>();
        assert_eq!(preedit, "あ");
    }

    /// Engine with f -> ん and s -> と in the layout, and a kanchoku layout
    /// where both f+s and s+s map to 二. This engine's config omits
    /// `forced_preedit_trigger_key`, so the default trigger ('f') applies:
    /// that exercises the conflict between the trigger and two-stroke kanchoku
    /// pairs that start with 'f'.
    fn create_trigger_kanchoku_test_engine() -> PSKKEngine {
        let layout = vec![
            ("f".to_string(), "".to_string(), "ん".to_string(), None),
            ("s".to_string(), "".to_string(), "と".to_string(), None),
        ];
        let simul = SimultaneousInputProcessor::new(Some(layout));

        let mut kanchoku_map: crate::kanchoku::KanchokuLayout = HashMap::new();
        let mut f_second = HashMap::new();
        f_second.insert('s', "二".to_string());
        kanchoku_map.insert('f', f_second);
        let mut s_second = HashMap::new();
        s_second.insert('s', "二".to_string());
        kanchoku_map.insert('s', s_second);
        let kanchoku = KanchokuProcessor::new(Some(kanchoku_map));

        let henkan = HenkanProcessor::new();

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
            pressed_keys: std::collections::HashSet::new(),
            marker_had_input: false,
            preedit_before_marker: String::new(),
            pure_kanchoku_held: false,
            pure_kanchoku_first_key: None,
            engine_state: EngineState::Normal,
            conversion_yomi: String::new(),
        }
    }

    #[test]
    fn kanchoku_pair_starting_with_trigger_key_keeps_preedit() {
        let mut engine = create_trigger_kanchoku_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        // Enter forced preedit via a single f tap
        engine.process_key_event(None, "space", true, None);
        engine.process_key_event(Some('f'), "f", true, None);
        engine.process_key_event(Some('f'), "f", false, None);
        let o = engine.process_key_event(None, "space", false, None);
        assert_eq!(o.engine_state, EngineState::ForcedPreedit);

        // Two-stroke kanchoku pair f + s -> 二 must stay in the preedit.
        // Regression: the 'f' trigger check used to fire on space release and
        // wipe the preedit.
        engine.process_key_event(None, "space", true, None);
        engine.process_key_event(Some('f'), "f", true, None);
        engine.process_key_event(Some('f'), "f", false, None);
        engine.process_key_event(Some('s'), "s", true, None);
        engine.process_key_event(Some('s'), "s", false, None);
        let o = engine.process_key_event(None, "space", false, None);
        assert_eq!(o.engine_state, EngineState::ForcedPreedit);
        let preedit = o.preedit_segments.iter().map(|s| s.text.clone()).collect::<String>();
        assert_eq!(preedit, "二");
    }

    #[test]
    fn trigger_key_pair_works_in_normal_mode_too() {
        let mut engine = create_trigger_kanchoku_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        // In normal mode (no forced preedit), space + f + s commits 二 at the
        // second stroke and must not spuriously enter forced preedit on space
        // release.
        engine.process_key_event(None, "space", true, None);
        engine.process_key_event(Some('f'), "f", true, None);
        engine.process_key_event(Some('f'), "f", false, None);
        let o = engine.process_key_event(Some('s'), "s", true, None);
        assert_eq!(o.commit_string, Some("二".to_string()));
        engine.process_key_event(Some('s'), "s", false, None);
        let o = engine.process_key_event(None, "space", false, None);
        assert_eq!(o.engine_state, EngineState::Normal);
    }

    #[test]
    fn single_f_tap_still_enters_forced_preedit() {
        let mut engine = create_trigger_kanchoku_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        // A lone f tap (no second stroke) must still enter forced preedit.
        engine.process_key_event(None, "space", true, None);
        engine.process_key_event(Some('f'), "f", true, None);
        engine.process_key_event(Some('f'), "f", false, None);
        let o = engine.process_key_event(None, "space", false, None);
        assert_eq!(o.engine_state, EngineState::ForcedPreedit);
    }

    /// Test engine in Hiragana mode with an explicit
    /// `forced_preedit_trigger_key` config value.
    fn create_forced_preedit_engine(trigger_keys: serde_json::Value) -> PSKKEngine {
        let mut engine = create_test_engine();
        engine.config["forced_preedit_trigger_key"] = trigger_keys;
        engine.set_mode(ProtoInputMode::Hiragana);
        engine
    }

    /// Press and release `c` as its own key event pair.
    fn tap_char(engine: &mut PSKKEngine, c: char) {
        let name = c.to_string();
        engine.process_key_event(Some(c), &name, true, None);
        engine.process_key_event(Some(c), &name, false, None);
    }

    /// Tap `trigger` as the marker's single first key and report the state the
    /// space release leaves the engine in.
    fn state_after_trigger_gap(trigger_keys: serde_json::Value, key: char) -> EngineState {
        let mut engine = create_forced_preedit_engine(trigger_keys);
        engine.process_key_event(None, "space", true, None);
        tap_char(&mut engine, key);
        engine.process_key_event(None, "space", false, None).engine_state
    }

    #[test]
    fn forced_preedit_trigger_key_is_read_from_config() {
        assert_eq!(
            state_after_trigger_gap(serde_json::json!(["q"]), 'q'),
            EngineState::ForcedPreedit
        );
    }

    #[test]
    fn configured_trigger_key_replaces_the_default() {
        // 'f' is only the default; with a configured trigger it is an ordinary
        // key and simply marks a bunsetsu boundary.
        assert_eq!(
            state_after_trigger_gap(serde_json::json!(["q"]), 'f'),
            EngineState::Bunsetsu
        );
    }

    #[test]
    fn multiple_forced_preedit_trigger_keys_are_supported() {
        let keys = serde_json::json!(["f", "q"]);
        assert_eq!(
            state_after_trigger_gap(keys.clone(), 'f'),
            EngineState::ForcedPreedit
        );
        assert_eq!(
            state_after_trigger_gap(keys, 'q'),
            EngineState::ForcedPreedit
        );
    }

    #[test]
    fn empty_forced_preedit_trigger_key_disables_the_trigger() {
        assert_eq!(
            state_after_trigger_gap(serde_json::json!([]), 'f'),
            EngineState::Bunsetsu
        );
    }

    #[test]
    fn non_single_character_trigger_entries_are_ignored() {
        // Modifier combos are not supported for this trigger yet; such an entry
        // must not silently match a plain 'f'.
        assert_eq!(
            state_after_trigger_gap(serde_json::json!(["Shift+F"]), 'f'),
            EngineState::Bunsetsu
        );
    }

    /// Regression: the settings UI writes keybindings from `KeyboardEvent`
    /// (`Control+l`, `Control+;`, `Control+Shift+L`) while the engine used to
    /// build `Ctrl+<IBus keyval name>`. The two never compared equal, so
    /// UI-configured conversion keys silently did nothing - Ctrl+L committed
    /// the hiragana unchanged instead of converting it to katakana.
    #[test]
    fn ui_style_conversion_bindings_are_matched() {
        let ctrl = ProtoKeyModifiers {
            shift: false,
            ctrl: true,
            alt: false,
            super_: false,
        };
        let ctrl_shift = ProtoKeyModifiers {
            shift: true,
            ctrl: true,
            alt: false,
            super_: false,
        };

        let mut engine = create_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);
        engine.config["conversion_keys"] = serde_json::json!({
            "to_katakana": ["Control+l"],
            "to_hiragana": [],
            "to_ascii": [],
            "to_zenkaku": [],
        });
        engine.process_key_event(Some('a'), "a", true, None);
        let o = engine.process_key_event(Some('l'), "l", true, Some(ctrl.clone()));
        assert_eq!(o.commit_string, None, "Control+l must not commit");
        assert_eq!(
            preedit_text(&o),
            "ア",
            "Control+l must convert the preedit to katakana"
        );

        // IBus reports the ';' key as the "semicolon" keyval; the binding is
        // written with the character.
        let mut engine = create_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);
        engine.config["conversion_keys"] = serde_json::json!({
            "to_katakana": [],
            "to_hiragana": [],
            "to_ascii": ["Control+;"],
            "to_zenkaku": [],
        });
        engine.process_key_event(Some('a'), "a", true, None);
        let o = engine.process_key_event(Some(';'), "semicolon", true, Some(ctrl.clone()));
        assert_eq!(o.commit_string, None, "Control+; must not commit");
        assert_eq!(
            preedit_text(&o),
            "a",
            "Control+; must convert the preedit to ascii"
        );

        let mut engine = create_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);
        engine.config["conversion_keys"] = serde_json::json!({
            "to_katakana": [],
            "to_hiragana": ["Control+Shift+L"],
            "to_ascii": [],
            "to_zenkaku": [],
        });
        engine.process_key_event(Some('a'), "a", true, None);
        let o = engine.process_key_event(Some('L'), "L", true, Some(ctrl_shift));
        // An unmatched Ctrl/Alt combo would commit the preedit and pass the key
        // through, so "no commit and consumed" proves the binding matched.
        assert_eq!(o.commit_string, None, "Control+Shift+L must not commit");
        assert!(o.consumed);
        assert_eq!(preedit_text(&o), "あ");

        // Canonical spellings (what the packaged defaults use) keep working.
        let mut engine = create_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);
        engine.config["conversion_keys"] = serde_json::json!({
            "to_katakana": ["Ctrl+K"],
            "to_hiragana": [],
            "to_ascii": [],
            "to_zenkaku": [],
        });
        engine.process_key_event(Some('a'), "a", true, None);
        let o = engine.process_key_event(Some('k'), "k", true, Some(ctrl));
        assert_eq!(o.commit_string, None);
        assert_eq!(preedit_text(&o), "ア");
    }

    /// Engine whose layout keeps each kana in `preedit_pending` until the next
    /// key arrives, like the shipped shingeta layout.
    fn create_kana_pending_engine() -> PSKKEngine {
        let mut engine = create_test_engine();
        engine.simul_processor = SimultaneousInputProcessor::new(Some(vec![
            ("a".to_string(), "".to_string(), "の".to_string(), None),
            ("s".to_string(), "".to_string(), "と".to_string(), None),
            ("d".to_string(), "".to_string(), "か".to_string(), None),
            ("f".to_string(), "".to_string(), "ん".to_string(), None),
        ]));
        engine.config["conversion_keys"] = serde_json::json!({
            "to_katakana": ["Ctrl+l"],
            "to_hiragana": ["Ctrl+j"],
            "to_ascii": ["Ctrl+;"],
            "to_zenkaku": ["Ctrl+Shift+L"],
        });
        engine.set_mode(ProtoInputMode::Hiragana);
        engine
    }

    fn preedit_text(output: &EngineOutput) -> String {
        output
            .preedit_segments
            .iter()
            .map(|s| s.text.clone())
            .collect()
    }

    /// Regression: `preedit_ascii` used to be appended only when a key press
    /// *flushed* a kana, so the key that produced the flushed kana was never
    /// recorded and every remaining entry was shifted by one. Converting
    /// "のとかん" (typed a s d f) therefore produced "sdf" instead of "asdf".
    #[test]
    fn ascii_conversion_keeps_every_keystroke() {
        let ctrl = ProtoKeyModifiers {
            shift: false,
            ctrl: true,
            alt: false,
            super_: false,
        };
        let mut engine = create_kana_pending_engine();
        for (c, name) in [('a', "a"), ('s', "s"), ('d', "d"), ('f', "f")] {
            engine.process_key_event(Some(c), name, true, None);
            engine.process_key_event(Some(c), name, false, None);
        }
        assert_eq!(engine.preedit_ascii, "asdf");

        let o = engine.process_key_event(Some(';'), "semicolon", true, Some(ctrl));
        assert_eq!(o.commit_string, None);
        assert_eq!(preedit_text(&o), "asdf");
    }

    /// Backspace has to drop the pending kana's key stroke as well, otherwise
    /// the ascii buffer drifts ahead of the kana it describes.
    #[test]
    fn backspace_drops_the_pending_keystroke_too() {
        let mut engine = create_kana_pending_engine();
        engine.process_key_event(Some('a'), "a", true, None);
        engine.process_key_event(Some('s'), "s", true, None);
        assert_eq!(engine.preedit_hiragana, "の");
        assert_eq!(engine.preedit_pending, "と");
        assert_eq!(engine.preedit_ascii, "as");

        // Removes the pending kana (と) and the key stroke that produced it.
        engine.process_key_event(None, "BackSpace", true, None);
        assert_eq!(engine.preedit_pending, "");
        assert_eq!(engine.preedit_ascii, "a");

        // Removes the confirmed kana (の) and its key stroke.
        engine.process_key_event(None, "BackSpace", true, None);
        assert_eq!(engine.preedit_hiragana, "");
        assert_eq!(engine.preedit_ascii, "");
    }

    /// Regression: the conversion keys read `preedit_hiragana`, which excludes
    /// the newest kana while it still sits in `preedit_pending`, and then
    /// committed the result. Ctrl+L on "のとかん" therefore produced a committed
    /// "ノトカ": the last letter was dropped from the preedit *and* the whole
    /// text left the buffer.
    #[test]
    fn conversion_keys_convert_the_whole_preedit_in_place() {
        let ctrl = ProtoKeyModifiers {
            shift: false,
            ctrl: true,
            alt: false,
            super_: false,
        };
        let mut engine = create_kana_pending_engine();

        for (c, name) in [('a', "a"), ('s', "s"), ('d', "d"), ('f', "f")] {
            engine.process_key_event(Some(c), name, true, None);
            engine.process_key_event(Some(c), name, false, None);
        }
        assert_eq!(engine.preedit_string, "のとかん", "pending kana must show");

        // Ctrl+L (to_katakana): everything is converted, nothing is committed.
        let o = engine.process_key_event(Some('l'), "l", true, Some(ctrl.clone()));
        assert_eq!(o.commit_string, None, "conversion must not commit the text");
        assert_eq!(
            o.engine_state,
            EngineState::Normal,
            "must stay in Idle mode"
        );
        assert_eq!(preedit_text(&o), "ノトカン");

        // The pending kana was folded into the confirmed preedit first: it is
        // no longer "waiting to be completed" once a conversion was asked for.
        assert!(engine.preedit_pending.is_empty(), "pending must be resolved");
        assert_eq!(engine.preedit_hiragana, "ノトカン");

        // Ctrl+J (to_hiragana) toggles back over the same preedit.
        let o = engine.process_key_event(Some('j'), "j", true, Some(ctrl));
        assert_eq!(o.commit_string, None);
        assert_eq!(o.engine_state, EngineState::Normal);
        assert_eq!(preedit_text(&o), "のとかん");
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

        assert_eq!(engine.engine_state, EngineState::Converting);

        let output = engine.process_key_event(None, "Return", true, None);
        assert!(output.consumed);
        assert_eq!(output.commit_string, Some("愛".to_string()));
        assert_ne!(engine.engine_state, EngineState::Converting);
        assert!(engine.preedit_string.is_empty());
    }

    #[test]
    fn escape_cancels_conversion() {
        let mut engine = create_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        engine.process_key_event(Some('a'), "a", true, None);
        engine.process_key_event(Some('i'), "i", true, None);
        engine.process_key_event(Some(' '), "space", true, None);

        assert_eq!(engine.engine_state, EngineState::Converting);

        let output = engine.process_key_event(None, "Escape", true, None);
        assert!(output.consumed);
        assert_ne!(engine.engine_state, EngineState::Converting);
        assert_eq!(engine.preedit_string, "あい");
    }

    #[test]
    fn ctrl_key_commits_and_passes_through() {
        let mut engine = create_test_engine();
        engine.set_mode(ProtoInputMode::Hiragana);

        engine.process_key_event(Some('a'), "a", true, None);
        assert!(!engine.preedit_string.is_empty());

        let modifiers = ProtoKeyModifiers { shift: false, ctrl: true, alt: false, super_: false };
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
