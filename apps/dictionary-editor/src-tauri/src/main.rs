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

/// Command line arguments the application was launched with.
#[derive(Debug, Default, Clone)]
struct LaunchArgs {
    /// Reading (yomi) to pre-fill, e.g. the current IME preedit.
    yomi: Option<String>,
}

impl LaunchArgs {
    /// Parse `--yomi <value>` and `--yomi=<value>` (unknown arguments are
    /// ignored so the app can be launched with extra framework flags).
    fn parse<I: Iterator<Item = String>>(mut iter: I) -> Self {
        let mut args = LaunchArgs::default();
        while let Some(arg) = iter.next() {
            if arg == "--yomi" {
                args.yomi = iter.next();
            } else if let Some(value) = arg.strip_prefix("--yomi=") {
                args.yomi = Some(value.to_string());
            }
        }
        args
    }

    fn from_env() -> Self {
        Self::parse(std::env::args().skip(1))
    }
}

#[tauri::command]
fn get_launch_yomi(args: tauri::State<'_, LaunchArgs>) -> Option<String> {
    args.yomi.clone()
}

fn main() {
    let launch_args = LaunchArgs::from_env();

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(launch_args)
        .invoke_handler(tauri::generate_handler![
            load_dictionary,
            save_dictionary,
            get_launch_yomi
        ])
        .run(tauri::generate_context!())
        .expect("error while running pskk-dictionary-editor");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> LaunchArgs {
        LaunchArgs::parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn parses_yomi_separate_argument() {
        assert_eq!(parse(&["--yomi", "こうえん"]).yomi.as_deref(), Some("こうえん"));
    }

    #[test]
    fn parses_yomi_equals_argument() {
        assert_eq!(parse(&["--yomi=みせ"]).yomi.as_deref(), Some("みせ"));
    }

    #[test]
    fn ignores_unknown_arguments() {
        assert_eq!(
            parse(&["--flag", "--yomi", "あい", "extra"]).yomi.as_deref(),
            Some("あい")
        );
    }

    #[test]
    fn no_arguments_yields_none() {
        assert_eq!(parse(&[]).yomi, None);
    }

    #[test]
    fn missing_value_after_yomi_is_none() {
        assert_eq!(parse(&["--yomi"]).yomi, None);
    }
}
