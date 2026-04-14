use pskk::engine::{EngineOutput, InputMode, PSKKEngine, PreeditSegment};
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
        let layout = Self::default_layout();
        let simul = SimultaneousInputProcessor::new(Some(layout));
        let kanchoku = KanchokuProcessor::new(None);
        let henkan = HenkanProcessor::new();
        
        let engine = PSKKEngine::new(simul, kanchoku, henkan);
        
        Self {
            engine: Mutex::new(engine),
        }
    }

    pub fn with_dictionary(mut self, dictionary: Dictionary) -> Self {
        let mut engine = self.engine.lock().unwrap();
        let layout = Self::default_layout();
        let simul = SimultaneousInputProcessor::new(Some(layout));
        let kanchoku = KanchokuProcessor::new(None);
        let henkan = HenkanProcessor::new().with_dictionary(dictionary);
        
        *engine = PSKKEngine::new(simul, kanchoku, henkan);
        drop(engine);
        self
    }

    fn default_layout() -> Vec<(String, String, String, Option<u64>)> {
        vec![
            ("a".to_string(), "あ".to_string(), "".to_string(), None),
            ("i".to_string(), "い".to_string(), "".to_string(), None),
            ("u".to_string(), "う".to_string(), "".to_string(), None),
            ("e".to_string(), "え".to_string(), "".to_string(), None),
            ("o".to_string(), "お".to_string(), "".to_string(), None),
            ("ka".to_string(), "か".to_string(), "".to_string(), None),
            ("ki".to_string(), "き".to_string(), "".to_string(), None),
            ("ku".to_string(), "く".to_string(), "".to_string(), None),
            ("ke".to_string(), "け".to_string(), "".to_string(), None),
            ("ko".to_string(), "こ".to_string(), "".to_string(), None),
            ("sa".to_string(), "さ".to_string(), "".to_string(), None),
            ("si".to_string(), "し".to_string(), "".to_string(), None),
            ("su".to_string(), "す".to_string(), "".to_string(), None),
            ("se".to_string(), "せ".to_string(), "".to_string(), None),
            ("so".to_string(), "そ".to_string(), "".to_string(), None),
            ("ta".to_string(), "た".to_string(), "".to_string(), None),
            ("ti".to_string(), "ち".to_string(), "".to_string(), None),
            ("tu".to_string(), "つ".to_string(), "".to_string(), None),
            ("te".to_string(), "て".to_string(), "".to_string(), None),
            ("to".to_string(), "と".to_string(), "".to_string(), None),
            ("na".to_string(), "な".to_string(), "".to_string(), None),
            ("ni".to_string(), "に".to_string(), "".to_string(), None),
            ("nu".to_string(), "ぬ".to_string(), "".to_string(), None),
            ("ne".to_string(), "ね".to_string(), "".to_string(), None),
            ("no".to_string(), "の".to_string(), "".to_string(), None),
            ("ha".to_string(), "は".to_string(), "".to_string(), None),
            ("hi".to_string(), "ひ".to_string(), "".to_string(), None),
            ("hu".to_string(), "ふ".to_string(), "".to_string(), None),
            ("he".to_string(), "へ".to_string(), "".to_string(), None),
            ("ho".to_string(), "ほ".to_string(), "".to_string(), None),
            ("ma".to_string(), "ま".to_string(), "".to_string(), None),
            ("mi".to_string(), "み".to_string(), "".to_string(), None),
            ("mu".to_string(), "む".to_string(), "".to_string(), None),
            ("me".to_string(), "め".to_string(), "".to_string(), None),
            ("mo".to_string(), "も".to_string(), "".to_string(), None),
            ("ya".to_string(), "や".to_string(), "".to_string(), None),
            ("yu".to_string(), "ゆ".to_string(), "".to_string(), None),
            ("yo".to_string(), "よ".to_string(), "".to_string(), None),
            ("ra".to_string(), "ら".to_string(), "".to_string(), None),
            ("ri".to_string(), "り".to_string(), "".to_string(), None),
            ("ru".to_string(), "る".to_string(), "".to_string(), None),
            ("re".to_string(), "れ".to_string(), "".to_string(), None),
            ("ro".to_string(), "ろ".to_string(), "".to_string(), None),
            ("wa".to_string(), "わ".to_string(), "".to_string(), None),
            ("wo".to_string(), "を".to_string(), "".to_string(), None),
            ("nn".to_string(), "ん".to_string(), "".to_string(), None),
            ("ga".to_string(), "が".to_string(), "".to_string(), None),
            ("gi".to_string(), "ぎ".to_string(), "".to_string(), None),
            ("gu".to_string(), "ぐ".to_string(), "".to_string(), None),
            ("ge".to_string(), "げ".to_string(), "".to_string(), None),
            ("go".to_string(), "ご".to_string(), "".to_string(), None),
            ("za".to_string(), "ざ".to_string(), "".to_string(), None),
            ("zi".to_string(), "じ".to_string(), "".to_string(), None),
            ("zu".to_string(), "ず".to_string(), "".to_string(), None),
            ("ze".to_string(), "ぜ".to_string(), "".to_string(), None),
            ("zo".to_string(), "ぞ".to_string(), "".to_string(), None),
            ("da".to_string(), "だ".to_string(), "".to_string(), None),
            ("di".to_string(), "ぢ".to_string(), "".to_string(), None),
            ("du".to_string(), "づ".to_string(), "".to_string(), None),
            ("de".to_string(), "で".to_string(), "".to_string(), None),
            ("do".to_string(), "ど".to_string(), "".to_string(), None),
            ("ba".to_string(), "ば".to_string(), "".to_string(), None),
            ("bi".to_string(), "び".to_string(), "".to_string(), None),
            ("bu".to_string(), "ぶ".to_string(), "".to_string(), None),
            ("be".to_string(), "べ".to_string(), "".to_string(), None),
            ("bo".to_string(), "ぼ".to_string(), "".to_string(), None),
            ("pa".to_string(), "ぱ".to_string(), "".to_string(), None),
            ("pi".to_string(), "ぴ".to_string(), "".to_string(), None),
            ("pu".to_string(), "ぷ".to_string(), "".to_string(), None),
            ("pe".to_string(), "ぺ".to_string(), "".to_string(), None),
            ("po".to_string(), "ぽ".to_string(), "".to_string(), None),
        ]
    }

    pub fn process_key(
        &self,
        key_char: Option<char>,
        key_name: String,
        is_pressed: bool,
        modifiers: KeyModifiers,
    ) -> EngineOutput {
        let mut engine = self.engine.lock().unwrap();
        engine.process_key_event(
            key_char,
            &key_name,
            is_pressed,
            modifiers.shift,
            modifiers.ctrl,
            modifiers.alt,
        )
    }

    pub fn set_mode(&self, mode: InputMode) -> EngineOutput {
        let mut engine = self.engine.lock().unwrap();
        engine.set_mode(mode)
    }

    pub fn get_mode(&self) -> InputMode {
        let engine = self.engine.lock().unwrap();
        engine.get_mode()
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
