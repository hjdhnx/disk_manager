mod cleaner;
mod commands;
mod hasher;
mod history;
mod models;
mod refresh;
mod scanner;

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

pub struct AppState {
    pub history: Mutex<history::History>,
    pub data_dir: PathBuf,
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&dir).ok();
            let history_path = dir.join("clean_history.json");
            let hist = history::History::load(history_path);
            app.manage(Mutex::new(hist));
            app.manage(dir.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_disks,
            commands::scan_space,
            commands::scan_big_files,
            commands::find_duplicates,
            commands::refresh_duplicates,
            commands::trash_files,
            commands::delete_files,
            commands::open_in_explorer,
            commands::save_export,
            commands::get_clean_history,
            commands::clear_clean_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}