use pskk::engine::{EngineOutput, InputMode, PSKKEngine, PreeditSegment};
use pskk::grpc::proto::{InputMode as ProtoInputMode, KeyModifiers as ProtoKeyModifiers};
use pskk::henkan::{Candidate, HenkanProcessor};
use pskk::kanchoku::KanchokuProcessor;
use pskk::simultaneous_processor::SimultaneousInputProcessor;
use pskk::util::Dictionary;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStateSnapshot {
    pub mode: String,
    pub in_conversion: bool,
    pub bunsetsu_mode: bool,
    pub preedit_string: String,
    pub preedit_hiragana: String,
    pub conversion_yomi: String,
}

pub struct EngineState {
    engine: Mutex<PSKKEngine>,
}

impl EngineState {
    pub fn new() -> Self {
        // Initialize logging first
        // Load config to get logging level
        let config = pskk::util::get_config_data()
            .ok()
            .map(|(config, _)| config);
        
        // Initialize logging (will log to both terminal and file)
        let _ = pskk::util::init_logging(config.as_ref());
        
        // Layout will be loaded by the engine from config
        let simul = SimultaneousInputProcessor::new(None);
        let kanchoku = KanchokuProcessor::new(None);
        let henkan = HenkanProcessor::new();

        let engine = PSKKEngine::new(simul, kanchoku, henkan)
            .expect("Failed to create engine");

        Self {
            engine: Mutex::new(engine),
        }
    }

    pub fn with_dictionary(self, dictionary: Dictionary) -> Self {
        let mut engine = self.engine.lock().unwrap();
        // Layout will be loaded by the engine from config
        let simul = SimultaneousInputProcessor::new(None);
        let kanchoku = KanchokuProcessor::new(None);
        let henkan = HenkanProcessor::new().with_dictionary(dictionary);

        *engine = PSKKEngine::new(simul, kanchoku, henkan)
            .expect("Failed to create engine with dictionary");
        drop(engine);
        self
    }

    pub fn process_key(
        &self,
        key_char: Option<char>,
        key_name: String,
        is_pressed: bool,
        modifiers: KeyModifiers,
    ) -> EngineOutput {
        let mut engine = self.engine.lock().unwrap();
        let proto_modifiers = ProtoKeyModifiers {
            shift: modifiers.shift,
            ctrl: modifiers.ctrl,
            alt: modifiers.alt,
        };
        engine.process_key_event(
            key_char,
            &key_name,
            is_pressed,
            Some(proto_modifiers),
        )
    }

    pub fn set_mode(&self, mode: InputMode) -> EngineOutput {
        let mut engine = self.engine.lock().unwrap();
        let proto_mode = match mode {
            InputMode::Alphanumeric => ProtoInputMode::Alphanumeric,
            InputMode::Hiragana => ProtoInputMode::Hiragana,
        };
        engine.set_mode(proto_mode)
    }

    pub fn get_mode(&self) -> InputMode {
        let engine = self.engine.lock().unwrap();
        let proto_mode = engine.get_mode();
        match proto_mode {
            ProtoInputMode::Alphanumeric => InputMode::Alphanumeric,
            ProtoInputMode::Hiragana => InputMode::Hiragana,
        }
    }

    pub fn focus_out(&self) -> EngineOutput {
        let mut engine = self.engine.lock().unwrap();
        engine.focus_out()
    }
}

impl Default for EngineState {
    fn default() -> Self {
        Self::new()
    }
}
