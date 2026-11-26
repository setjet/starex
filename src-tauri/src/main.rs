// Release builds are GUI programs: no console window behind the launcher.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod artwork;
mod commands;
mod sessions;
mod state;

use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use crate::state::AppState;

fn main() {
    // A panic must never vanish silently: record it in the log before the process ends.
    std::panic::set_hook(Box::new(|info| {
        log::error!("Starex crashed: {info}");
        eprintln!("Starex crashed: {info}");
    }));

    let builder = tauri::Builder::default();

    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // A second launch focuses the window that is already open.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build());

    let result = builder
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(2_000_000)
                .build(),
        )
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let state = match AppState::load() {
                Ok(state) => state,
                Err(e) => {
                    // Without its data folder Starex cannot run; say why instead of vanishing.
                    log::error!("could not open the data folder: {e}");
                    app.dialog()
                        .message(format!(
                            "Starex could not open its data folder.\n\n{e}\n\nCheck that the drive is \
                             available and that you have permission to write to it."
                        ))
                        .title("Starex could not start")
                        .kind(MessageDialogKind::Error)
                        .blocking_show();
                    return Err(e);
                }
            };
            // Artwork lives in Starex's data folder; only that folder is served to the UI.
            app.asset_protocol_scope().allow_directory(&state.paths.artwork, true)?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::inspect_folder,
            commands::add_game,
            commands::update_game,
            commands::remove_game,
            commands::set_artwork,
            commands::fetch_artwork,
            commands::launch_game,
            commands::stop_game,
            commands::verify_game,
            commands::create_baseline,
            commands::delete_baseline,
            commands::baseline_info,
            commands::cancel_task,
            commands::save_settings,
            commands::reveal,
            commands::open_data_dir,
            commands::open_link,
            commands::scan_game,
            commands::scan_folder,
            commands::suggest_save_locations,
            commands::list_save_backups,
            commands::backup_saves,
            commands::restore_saves,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        log::error!("Starex stopped: {e}");
        eprintln!("Starex stopped: {e}");
        std::process::exit(1);
    }
}
