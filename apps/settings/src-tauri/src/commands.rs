use pskk::settings::{
    list_available_kanchoku_layouts, list_available_layouts, list_ext_dictionary_entries,
    list_system_dictionary_entries, list_user_dictionary_entries, load_current_kanchoku_mappings,
    save_kanchoku_layout_mappings, save_settings, ExtDictionaryEntry, MurensoMapping,
    SaveSettingsInput,
};
use pskk::util::{
    generate_crf_feature_materials, generate_system_dictionary_from_sources,
    generate_user_dictionary_from_sources, get_config_data,
};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct SettingsAppState {
    pub config: serde_json::Value,
    pub warnings: Vec<String>,
    pub layouts: Vec<pskk::settings::DiscoveredFile>,
    pub kanchoku_layouts: Vec<pskk::settings::DiscoveredFile>,
    pub system_dictionaries: Vec<pskk::settings::SystemDictionaryEntry>,
    pub user_dictionaries: Vec<pskk::settings::UserDictionaryEntry>,
    pub ext_system_dictionaries: Vec<ExtDictionaryEntry>,
    pub ext_user_dictionaries: Vec<ExtDictionaryEntry>,
    pub murenso_mappings: Vec<MurensoMapping>,
}

#[derive(Debug, Serialize)]
pub struct SaveSettingsCommandResult {
    pub saved: bool,
    pub config_path: Option<String>,
    pub keybinding_conflicts: HashMap<String, Vec<String>>,
    pub state: SettingsAppState,
}

#[derive(Debug, Serialize)]
pub struct DictionaryGenerationResult {
    pub success: bool,
    pub output_path: Option<String>,
    pub files_processed: usize,
    pub total_readings: usize,
    pub total_candidates: usize,
    pub okurigana_entries_expanded: usize,
    pub state: SettingsAppState,
}

#[tauri::command]
pub fn load_settings_state() -> Result<SettingsAppState, String> {
    build_state().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn save_settings_state(
    input: SaveSettingsInput,
    murenso_mappings: Vec<MurensoMapping>,
) -> Result<SaveSettingsCommandResult, String> {
    let (config, _warnings) = get_config_data().map_err(|err| err.to_string())?;
    let save_result = save_settings(&config, input).map_err(|err| err.to_string())?;
    if save_result.keybinding_conflicts.is_empty() {
        let (saved_config, _) = get_config_data().map_err(|err| err.to_string())?;
        save_kanchoku_layout_mappings(&saved_config, &murenso_mappings, None)
            .map_err(|err| err.to_string())?;
    }

    Ok(SaveSettingsCommandResult {
        saved: save_result.keybinding_conflicts.is_empty(),
        config_path: save_result
            .config_path
            .to_str()
            .map(|value| value.to_string()),
        keybinding_conflicts: save_result.keybinding_conflicts,
        state: build_state().map_err(|err| err.to_string())?,
    })
}

#[tauri::command]
pub fn convert_system_dictionaries(
    source_weights: HashMap<String, i32>,
) -> Result<DictionaryGenerationResult, String> {
    let (output_path, stats) =
        generate_system_dictionary_from_sources(&source_weights).map_err(|err| err.to_string())?;
    let _ = generate_crf_feature_materials(None);
    Ok(DictionaryGenerationResult {
        success: true,
        output_path: output_path.to_str().map(|value| value.to_string()),
        files_processed: stats.files_processed,
        total_readings: stats.total_readings,
        total_candidates: stats.total_candidates,
        okurigana_entries_expanded: stats.okurigana_entries_expanded,
        state: build_state().map_err(|err| err.to_string())?,
    })
}

#[tauri::command]
pub fn convert_user_dictionaries(
    source_weights: HashMap<String, i32>,
) -> Result<DictionaryGenerationResult, String> {
    let (output_path, stats) =
        generate_user_dictionary_from_sources(&source_weights).map_err(|err| err.to_string())?;
    let _ = generate_crf_feature_materials(None);
    Ok(DictionaryGenerationResult {
        success: true,
        output_path: output_path.and_then(|path| path.to_str().map(|value| value.to_string())),
        files_processed: stats.files_processed,
        total_readings: stats.total_readings,
        total_candidates: stats.total_candidates,
        okurigana_entries_expanded: stats.okurigana_entries_expanded,
        state: build_state().map_err(|err| err.to_string())?,
    })
}

fn build_state() -> Result<SettingsAppState, pskk::util::UtilError> {
    let (config, warnings) = get_config_data()?;
    let layouts = list_available_layouts();
    let kanchoku_layouts = list_available_kanchoku_layouts();
    let system_dictionaries = list_system_dictionary_entries(&config);
    let user_dictionaries = list_user_dictionary_entries(&config);
    let (ext_system_dictionaries, ext_user_dictionaries) = list_ext_dictionary_entries();
    let murenso_mappings = load_current_kanchoku_mappings(&config)?;

    Ok(SettingsAppState {
        config,
        warnings,
        layouts,
        kanchoku_layouts,
        system_dictionaries,
        user_dictionaries,
        ext_system_dictionaries,
        ext_user_dictionaries,
        murenso_mappings,
    })
}
