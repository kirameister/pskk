use serde::{Deserialize, Serialize};
use crate::util::{
    get_datadir, get_dictionary_files, get_kanchoku_layout, get_user_config_dir,
    get_user_dictionaries_dir, save_config_data, write_json_value, UtilError,
};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredFile {
    pub id: String,
    pub display_label: String,
    pub user_path: Option<PathBuf>,
    pub system_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemDictionaryEntry {
    pub enabled: bool,
    pub relative_path: String,
    pub full_path: PathBuf,
    pub weight: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserDictionaryEntry {
    pub enabled: bool,
    pub filename: String,
    pub weight: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtDictionaryEntry {
    pub enabled: bool,
    pub display_name: String,
    pub full_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MurensoMapping {
    pub first_key: String,
    pub second_key: String,
    pub kanji: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveSettingsInput {
    pub layout: String,
    pub kanchoku_layout: String,
    pub candidate_window_size: i32,
    pub keybindings_by_action: HashMap<String, Vec<String>>,
    pub enabled_system_dicts: HashMap<String, i32>,
    pub enabled_user_dicts: HashMap<String, i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveSettingsOutput {
    pub config_path: PathBuf,
    pub keybinding_conflicts: HashMap<String, Vec<String>>,
}

pub fn list_available_layouts() -> Vec<DiscoveredFile> {
    let user_dir = get_user_config_dir().join("layouts");
    let system_dir = get_datadir().join("layouts");
    discover_json_files_with_precedence(
        &user_dir,
        &system_dir,
        |filename, in_user, in_system| {
            if in_user && in_system {
                format!("{filename} (User: $HOME/.config/, System: /opt/)")
            } else if in_user {
                format!("{filename} (User: $HOME/.config/)")
            } else {
                format!("{filename} (System: /opt/)")
            }
        },
    )
}

pub fn list_available_kanchoku_layouts() -> Vec<DiscoveredFile> {
    let user_dir = get_user_config_dir().join("kanchoku_layouts");
    let system_dir = get_datadir().join("kanchoku_layouts");
    discover_json_files_with_precedence(
        &user_dir,
        &system_dir,
        |filename, in_user, in_system| {
            if in_user && in_system {
                format!("{filename} (User, System)")
            } else if in_user {
                format!("{filename} (User)")
            } else {
                format!("{filename} (System)")
            }
        },
    )
}

pub fn list_system_dictionary_entries(config: &Value) -> Vec<SystemDictionaryEntry> {
    let dict_config = config
        .get("dictionaries")
        .and_then(Value::as_object)
        .and_then(|dicts| dicts.get("system"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let weights = normalize_weight_map(&dict_config);

    let root = get_datadir().join("data/skk_dict");
    let mut entries = Vec::new();
    for path in collect_files_recursive(&root) {
        // Skip non-dictionary files (like .gitignore)
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if filename.starts_with('.') || filename.is_empty() {
            continue;
        }
        
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .to_string();
        let full = path.to_string_lossy().to_string();
        entries.push(SystemDictionaryEntry {
            enabled: weights.contains_key(&full),
            relative_path: rel,
            full_path: path,
            weight: weights.get(&full).copied().unwrap_or(1),
        });
    }
    entries
}

pub fn list_user_dictionary_entries(config: &Value) -> Vec<UserDictionaryEntry> {
    let dict_config = config
        .get("dictionaries")
        .and_then(Value::as_object)
        .and_then(|dicts| dicts.get("user"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let weights = normalize_weight_map(&dict_config);

    let root = get_user_dictionaries_dir();
    let mut entries = Vec::new();
    if let Ok(read_dir) = fs::read_dir(&root) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            
            // Skip directories and hidden files
            if !path.is_file() {
                continue;
            }
            let filename = path.file_name().unwrap().to_string_lossy().to_string();
            if filename.starts_with('.') {
                continue;
            }
            
            entries.push(UserDictionaryEntry {
                enabled: weights.contains_key(&filename),
                filename: filename.clone(),
                weight: weights.get(&filename).copied().unwrap_or(1),
            });
        }
    }
    entries.sort_by(|a, b| a.filename.cmp(&b.filename));
    entries
}

pub fn list_ext_dictionary_entries() -> (Vec<ExtDictionaryEntry>, Vec<ExtDictionaryEntry>) {
    let sys_root = get_datadir().join("data/skk_dict");
    let mut sys_entries = Vec::new();
    for path in collect_files_recursive(&sys_root) {
        // Skip hidden files
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if filename.starts_with('.') || filename.is_empty() {
            continue;
        }
        
        let rel = path
            .strip_prefix(&sys_root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .to_string();
        sys_entries.push(ExtDictionaryEntry {
            enabled: false,
            display_name: rel,
            full_path: path,
        });
    }

    let user_root = get_user_dictionaries_dir();
    let mut user_entries = Vec::new();
    if let Ok(read_dir) = fs::read_dir(&user_root) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            
            // Skip directories and hidden files
            if !path.is_file() {
                continue;
            }
            let filename = path.file_name().unwrap().to_string_lossy().to_string();
            if filename.starts_with('.') {
                continue;
            }
            
            user_entries.push(ExtDictionaryEntry {
                enabled: false,
                display_name: filename,
                full_path: path,
            });
        }
    }
    user_entries.sort_by(|a, b| a.display_name.cmp(&b.display_name));

    (sys_entries, user_entries)
}

pub fn validate_ctrl_shift_key(binding: &str) -> bool {
    if binding.is_empty() {
        return false;
    }
    let parts: Vec<&str> = binding.split('+').collect();
    parts.contains(&"Control") && parts.contains(&"Shift") && parts.len() >= 3
}

pub fn normalize_hex_color(input: &str, default: &str) -> String {
    let text = input
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches('#')
        .to_lowercase();
    if text.len() == 6 && text.chars().all(|c| c.is_ascii_hexdigit()) {
        text
    } else {
        default.to_lowercase()
    }
}

pub fn validate_positive_weight(text: &str) -> Result<i32, UtilError> {
    let weight = text
        .trim()
        .parse::<i32>()
        .map_err(|_| UtilError::InvalidConfig("weight must be a positive integer"))?;
    if weight < 1 {
        return Err(UtilError::InvalidConfig("weight must be at least 1"));
    }
    Ok(weight)
}

pub fn validate_keybindings(
    action_key_pairs: &[(String, String)],
) -> (HashMap<String, Vec<String>>, HashMap<String, Vec<String>>) {
    let mut deduped: HashMap<String, HashSet<String>> = HashMap::new();
    for (action_id, key_value) in action_key_pairs {
        if action_id.is_empty() || key_value.is_empty() {
            continue;
        }
        deduped
            .entry(action_id.clone())
            .or_default()
            .insert(key_value.clone());
    }

    let mut normalized: HashMap<String, Vec<String>> = deduped
        .into_iter()
        .map(|(action, keys)| {
            let mut keys: Vec<String> = keys.into_iter().collect();
            keys.sort();
            (action, keys)
        })
        .collect();

    let mut key_to_actions: HashMap<String, Vec<String>> = HashMap::new();
    for (action, keys) in &normalized {
        for key in keys {
            key_to_actions.entry(key.clone()).or_default().push(action.clone());
        }
    }

    let conflicts = key_to_actions
        .into_iter()
        .filter_map(|(key, mut actions)| {
            if actions.len() > 1 {
                actions.sort();
                Some((key, actions))
            } else {
                None
            }
        })
        .collect();

    for keys in normalized.values_mut() {
        keys.sort();
    }

    (normalized, conflicts)
}

pub fn flatten_kanchoku_layout(layout: &Value) -> Vec<MurensoMapping> {
    let mut mappings = Vec::new();
    let Some(root) = layout.as_object() else {
        return mappings;
    };

    for (first_key, second_value) in root {
        let Some(second_map) = second_value.as_object() else {
            continue;
        };
        for (second_key, kanji) in second_map {
            mappings.push(MurensoMapping {
                first_key: first_key.clone(),
                second_key: second_key.clone(),
                kanji: kanji.as_str().unwrap_or_default().to_string(),
            });
        }
    }

    mappings.sort_by(|a, b| {
        a.first_key
            .cmp(&b.first_key)
            .then(a.second_key.cmp(&b.second_key))
            .then(a.kanji.cmp(&b.kanji))
    });
    mappings
}

pub fn build_kanchoku_layout(mappings: &[MurensoMapping]) -> Value {
    let mut root = serde_json::Map::new();
    for mapping in mappings {
        if mapping.first_key.is_empty() || mapping.second_key.is_empty() {
            continue;
        }
        let row = root
            .entry(mapping.first_key.clone())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        let row_obj = row.as_object_mut().unwrap();
        row_obj.insert(
            mapping.second_key.clone(),
            Value::String(mapping.kanji.clone()),
        );
    }
    Value::Object(root)
}

pub fn save_kanchoku_layout_mappings(
    config: &Value,
    mappings: &[MurensoMapping],
    path: Option<&Path>,
) -> Result<PathBuf, UtilError> {
    let resolved_path = path
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let filename = config
                .get("kanchoku_layout")
                .and_then(Value::as_str)
                .unwrap_or("aki_code.json");
            get_user_config_dir().join(filename)
        });
    let layout = build_kanchoku_layout(mappings);
    write_json_value(&resolved_path, &layout)?;
    Ok(resolved_path)
}

pub fn save_settings(
    current_config: &Value,
    input: SaveSettingsInput,
) -> Result<SaveSettingsOutput, UtilError> {
    let mut config = current_config.clone();
    let (keybindings_by_action, keybinding_conflicts) = validate_keybindings(
        &input
            .keybindings_by_action
            .iter()
            .flat_map(|(action, keys)| keys.iter().map(|key| (action.clone(), key.clone())))
            .collect::<Vec<_>>(),
    );

    if !keybinding_conflicts.is_empty() {
        return Ok(SaveSettingsOutput {
            config_path: get_user_config_dir().join("config.json"),
            keybinding_conflicts,
        });
    }

    config["layout"] = Value::String(input.layout);
    config["kanchoku_layout"] = Value::String(input.kanchoku_layout);
    config["ui"] = json!({
        "candidate_window_size": input.candidate_window_size,
    });

    for config_key in [
        "enable_hiragana_key",
        "disable_hiragana_key",
        "forced_preedit_trigger_key",
        "kanchoku_bunsetsu_marker",
        "kanchoku_pure_trigger_key",
        "bunsetsu_prediction_cycle_key",
        "user_dictionary_editor_trigger",
        "force_commit_key",
    ] {
        config[config_key] = Value::Array(
            keybindings_by_action
                .get(config_key)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(Value::String)
                .collect(),
        );
    }

    config["conversion_keys"] = json!({
        "to_katakana": keybindings_by_action.get("to_katakana").cloned().unwrap_or_default(),
        "to_hiragana": keybindings_by_action.get("to_hiragana").cloned().unwrap_or_default(),
        "to_ascii": keybindings_by_action.get("to_ascii").cloned().unwrap_or_default(),
        "to_zenkaku": keybindings_by_action.get("to_zenkaku").cloned().unwrap_or_default(),
    });

    config["dictionaries"] = json!({
        "system": input.enabled_system_dicts,
        "user": input.enabled_user_dicts,
    });

    save_config_data(&config)?;
    Ok(SaveSettingsOutput {
        config_path: get_user_config_dir().join("config.json"),
        keybinding_conflicts: HashMap::new(),
    })
}

pub fn load_current_kanchoku_mappings(config: &Value) -> Result<Vec<MurensoMapping>, UtilError> {
    let layout = get_kanchoku_layout(config)?;
    Ok(flatten_kanchoku_layout(&layout))
}

pub fn existing_dictionary_json_files() -> Vec<PathBuf> {
    get_dictionary_files(None)
}

fn discover_json_files_with_precedence(
    user_dir: &Path,
    system_dir: &Path,
    make_label: impl Fn(&str, bool, bool) -> String,
) -> Vec<DiscoveredFile> {
    let user_files = collect_json_filenames(user_dir);
    let system_files = collect_json_filenames(system_dir);
    let all_files: BTreeSet<String> = user_files.union(&system_files).cloned().collect();

    all_files
        .into_iter()
        .map(|filename| {
            let in_user = user_files.contains(&filename);
            let in_system = system_files.contains(&filename);
            DiscoveredFile {
                id: filename.clone(),
                display_label: make_label(&filename, in_user, in_system),
                user_path: in_user.then(|| user_dir.join(&filename)),
                system_path: in_system.then(|| system_dir.join(&filename)),
            }
        })
        .collect()
}

fn collect_json_filenames(dir: &Path) -> HashSet<String> {
    let mut files = HashSet::new();
    if let Ok(read_dir) = fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                    files.insert(name.to_string());
                }
            }
        }
    }
    files
}

fn collect_files_recursive(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive_inner(root, &mut files);
    files.sort();
    files
}

fn collect_files_recursive_inner(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(read_dir) = fs::read_dir(root) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive_inner(&path, files);
        } else if path.is_file() {
            files.push(path);
        }
    }
}

fn normalize_weight_map(value: &Value) -> HashMap<String, i32> {
    match value {
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(|path| (path.to_string(), 1))
            .collect(),
        Value::Object(map) => map
            .iter()
            .filter_map(|(path, weight)| {
                weight
                    .as_i64()
                    .map(|weight| (path.clone(), weight.max(1) as i32))
            })
            .collect(),
        _ => HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_ctrl_shift_keybindings() {
        assert!(validate_ctrl_shift_key("Control+Shift+R"));
        assert!(!validate_ctrl_shift_key("Shift+R"));
        assert!(!validate_ctrl_shift_key("Control+Shift"));
    }

    #[test]
    fn normalizes_hex_colors() {
        assert_eq!(normalize_hex_color("0xAABBCC", "000000"), "aabbcc");
        assert_eq!(normalize_hex_color("#112233", "000000"), "112233");
        assert_eq!(normalize_hex_color("bad", "d1eaff"), "d1eaff");
    }

    #[test]
    fn detects_keybinding_conflicts() {
        let (bindings, conflicts) = validate_keybindings(&[
            ("to_hiragana".to_string(), "F6".to_string()),
            ("to_hiragana".to_string(), "F6".to_string()),
            ("to_ascii".to_string(), "F6".to_string()),
        ]);
        assert_eq!(bindings["to_hiragana"], vec!["F6".to_string()]);
        assert_eq!(conflicts["F6"].len(), 2);
    }

    #[test]
    fn flattens_and_rebuilds_kanchoku_layout() {
        let layout = json!({
            "a": { "b": "日" },
            "c": { "d": "月" }
        });
        let mappings = flatten_kanchoku_layout(&layout);
        assert_eq!(mappings.len(), 2);
        let rebuilt = build_kanchoku_layout(&mappings);
        assert_eq!(rebuilt, layout);
    }

    #[test]
    fn validates_positive_weight_values() {
        assert_eq!(validate_positive_weight("3").unwrap(), 3);
        assert!(validate_positive_weight("0").is_err());
        assert!(validate_positive_weight("abc").is_err());
    }
}
