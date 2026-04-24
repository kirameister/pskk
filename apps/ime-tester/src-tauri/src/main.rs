#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine_state;

use engine_state::{EngineState, KeyModifiers};
use pskk::engine::{EngineOutput, InputMode};
use pskk::util::Dictionary;
use std::collections::HashMap;
use tauri::State;

#[tauri::command]
fn process_key(
    state: State<EngineState>,
    key_char: Option<char>,
    key_name: String,
    is_pressed: bool,
    modifiers: KeyModifiers,
) -> EngineOutput {
    state.process_key(key_char, key_name, is_pressed, modifiers)
}

#[tauri::command]
fn set_mode(state: State<EngineState>, mode: InputMode) -> EngineOutput {
    state.set_mode(mode)
}

#[tauri::command]
fn get_mode(state: State<EngineState>) -> InputMode {
    state.get_mode()
}

#[tauri::command]
fn focus_out(state: State<EngineState>) -> EngineOutput {
    state.focus_out()
}

#[tauri::command]
fn load_sample_dictionary(_state: State<EngineState>) -> Result<String, String> {
    let mut dict: Dictionary = HashMap::new();
    
    let mut henkan_candidates = HashMap::new();
    henkan_candidates.insert("変換".to_string(), 100);
    henkan_candidates.insert("返還".to_string(), 50);
    henkan_candidates.insert("編纂".to_string(), 10);
    dict.insert("へんかん".to_string(), henkan_candidates);
    
    let mut kyou_candidates = HashMap::new();
    kyou_candidates.insert("今日".to_string(), 200);
    kyou_candidates.insert("京".to_string(), 80);
    kyou_candidates.insert("教".to_string(), 40);
    dict.insert("きょう".to_string(), kyou_candidates);
    
    let mut ha_candidates = HashMap::new();
    ha_candidates.insert("は".to_string(), 100);
    dict.insert("は".to_string(), ha_candidates);
    
    let mut tenki_candidates = HashMap::new();
    tenki_candidates.insert("天気".to_string(), 150);
    tenki_candidates.insert("転機".to_string(), 30);
    dict.insert("てんき".to_string(), tenki_candidates);
    
    let mut ga_candidates = HashMap::new();
    ga_candidates.insert("が".to_string(), 100);
    dict.insert("が".to_string(), ga_candidates);
    
    let mut yoi_candidates = HashMap::new();
    yoi_candidates.insert("良い".to_string(), 100);
    yoi_candidates.insert("酔い".to_string(), 20);
    dict.insert("よい".to_string(), yoi_candidates);
    
    let mut ai_candidates = HashMap::new();
    ai_candidates.insert("愛".to_string(), 100);
    ai_candidates.insert("相".to_string(), 50);
    ai_candidates.insert("藍".to_string(), 20);
    dict.insert("あい".to_string(), ai_candidates);
    
    let mut konnitiha_candidates = HashMap::new();
    konnitiha_candidates.insert("こんにちは".to_string(), 100);
    dict.insert("こんにちは".to_string(), konnitiha_candidates);
    
    let entry_count = dict.values().map(|v| v.len()).sum::<usize>();
    
    Ok(format!("Loaded {} entries from {} readings", entry_count, dict.len()))
}

fn main() {
    let engine_state = EngineState::new();
    
    tauri::Builder::default()
        .manage(engine_state)
        .invoke_handler(tauri::generate_handler![
            process_key,
            set_mode,
            get_mode,
            focus_out,
            load_sample_dictionary,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
