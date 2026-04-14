mod commands;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::load_settings_state,
            commands::save_settings_state,
            commands::convert_system_dictionaries,
            commands::convert_user_dictionaries,
            commands::convert_extended_dictionary
        ])
        .run(tauri::generate_context!())
        .expect("error while running pskk-settings");
}
