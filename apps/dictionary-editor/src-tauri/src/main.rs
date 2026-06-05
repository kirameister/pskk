use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
struct DictionaryEntry {
    reading: String,
    kanji: String,
    count: i32,
}

#[tauri::command]
fn load_dictionary() -> Result<Vec<DictionaryEntry>, String> {
    let dict = pskk::util::load_user_dictionary()
        .map_err(|e| format!("Failed to load dictionary: {}", e))?;
    
    let mut entries = Vec::new();
    for (reading, candidates) in dict {
        for (kanji, count) in candidates {
            entries.push(DictionaryEntry {
                reading: reading.clone(),
                kanji,
                count,
            });
        }
    }
    
    Ok(entries)
}

#[tauri::command]
fn save_dictionary(entries: Vec<DictionaryEntry>) -> Result<(), String> {
    let mut dict: HashMap<String, HashMap<String, i32>> = HashMap::new();
    
    for entry in entries {
        dict.entry(entry.reading)
            .or_insert_with(HashMap::new)
            .insert(entry.kanji, entry.count);
    }
    
    pskk::util::save_user_dictionary(&dict)
        .map_err(|e| format!("Failed to save dictionary: {}", e))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![load_dictionary, save_dictionary])
        .run(tauri::generate_context!())
        .expect("error while running pskk-dictionary-editor");
}
