mod browse;
mod commands;
mod indexer;
mod state;
#[cfg(test)]
mod testutil;

use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            match state::AppState::open(stl_index::paths::app_dir()) {
                Ok(s) => {
                    app.manage(s);
                }
                Err(e) => {
                    app.dialog()
                        .message(format!("Cannot open the index database:\n{e}"))
                        .kind(MessageDialogKind::Error)
                        .title("STL Browser")
                        .blocking_show();
                    std::process::exit(1);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_folder,
            commands::status,
            commands::rescan,
            commands::add_folder,
            commands::select_thumb
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
