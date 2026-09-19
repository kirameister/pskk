// PSKK CRF Trainer — Tauri host process.
//
// PSKK CRFトレーナー — Tauriホストプロセス。
//
// This app is the GTK `conversion_model.py` panel reimagined as a Tauri app:
// the Test tab predicts bunsetsu splits, the Train tab runs the
// browse → extract → train pipeline, and the Corpus Stats tab inspects
// annotated data and the intermediate features TSV.
//
// このアプリはGTKの`conversion_model.py`パネルをTauriアプリとして再設計したもの。

// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod crf;
mod types;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_environment,
            commands::list_models,
            commands::check_model_target,
            commands::confirm_model_overwrite,
            commands::pick_corpus_file,
            commands::pick_model_file,
            commands::pick_save_file,
            commands::load_training_params,
            commands::save_training_params,
            commands::load_corpus_report,
            commands::inspect_feature_tsv,
            commands::extract_features,
            commands::train_model,
            commands::predict,
            commands::display_feature_keys,
        ])
        .run(tauri::generate_context!())
        .expect("error while running pskk-crf-trainer");
}
