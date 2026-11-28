// Release builds are GUI programs: no console window behind the launcher.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod artwork;
mod commands;
mod discord;
mod firewall;
mod releases;
mod savedb;
mod sessions;
mod shortcuts;
mod state;
mod steam;
mod system;

use tauri::{Emitter, Manager, WindowEvent};
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
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // A second launch from a game shortcut starts that game in the Starex already open;
            // any other second launch focuses its window.
            if let Some(id) = shortcuts::requested_game(args) {
                let _ = app.emit(commands::PLAY_EVENT, id);
            } else if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![system::START_HIDDEN_ARG]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
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
            // Opened from a game shortcut or a starex:// link: start that game once the window is ready.
            *state.launch_request() = shortcuts::requested_game(std::env::args());
            app.manage(state);
            // Logos stored before Starex trimmed their empty edges, fixed once in the background.
            artwork::trim_stored_logos_once(app.handle().clone());

            #[cfg(desktop)]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                // Make starex:// links open this copy of Starex (installed builds register it too).
                #[cfg(any(windows, target_os = "linux"))]
                if let Err(e) = app.deep_link().register_all() {
                    log::warn!("could not register starex:// links: {e}");
                }
                let handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        if let Some(id) = shortcuts::game_from_link(url.as_str()) {
                            let _ = handle.emit(commands::PLAY_EVENT, id);
                        }
                    }
                });
            }

            // Tray icon and the global shortcut from Settings.
            #[cfg(desktop)]
            {
                if let Err(e) = system::create_tray(app.handle()) {
                    log::warn!("could not create the tray icon: {e}");
                }
                let shortcut = app.state::<AppState>().settings().global_shortcut.clone();
                if let Err(e) = system::apply_shortcut(app.handle(), &shortcut) {
                    log::warn!("global shortcut not set: {e}");
                }
            }

            // Fetch or refresh the save database in the background.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = savedb::get(&handle).await;
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // With "Keep Starex in the tray" on, closing the window hides it instead.
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.app_handle().state::<AppState>().settings().close_to_tray
            {
                api.prevent_close();
                let _ = window.hide();
            }
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
            commands::check_runtimes,
            commands::install_runtime,
            commands::create_shortcut,
            commands::launch_link,
            commands::take_launch_request,
            commands::set_backdrop_image,
            commands::set_offline,
            commands::fetch_changelog,
            commands::scan_stores,
            commands::import_games,
            commands::store_status,
            commands::auto_import_stores,
            commands::steam_account,
            commands::sync_steam_playtime,
            commands::steam_news,
            commands::steam_achievements,
            commands::open_steam_news,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        log::error!("Starex stopped: {e}");
        eprintln!("Starex stopped: {e}");
        std::process::exit(1);
    }
}
