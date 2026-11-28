//! How Starex fits into the desktop: the tray icon, starting with the system, the global
//! shortcut that brings it up, and closing to the tray.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::commands::PLAY_EVENT;
use crate::state::AppState;

const TRAY_ID: &str = "starex";
/// Passed when Starex starts with the system: it then waits in the tray instead of opening.
pub const START_HIDDEN_ARG: &str = "--minimized";

/// Brings the main window to the front.
pub fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn tray_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "open", "Open Starex", true, None::<&str>)?)?;

    // The five games played most recently.
    let mut games: Vec<_> = app
        .state::<AppState>()
        .library()
        .games()
        .iter()
        .filter(|g| !g.hidden)
        .cloned()
        .collect();
    games.sort_by_key(|g| std::cmp::Reverse(g.last_played.unwrap_or(g.added_at)));
    if !games.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        for game in games.iter().take(5) {
            let label = if game.name.chars().count() > 40 {
                format!("Play {}…", game.name.chars().take(39).collect::<String>())
            } else {
                format!("Play {}", game.name)
            };
            menu.append(&MenuItem::with_id(
                app,
                format!("play:{}", game.id),
                label,
                true,
                None::<&str>,
            )?)?;
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit Starex", true, None::<&str>)?)?;
    Ok(menu)
}

/// Creates the tray icon.
pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Starex")
        .menu(&tray_menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_window(app),
            "quit" => app.exit(0),
            id => {
                if let Some(game) = id.strip_prefix("play:") {
                    // Show Starex so any question before Play (a missing runtime) is visible.
                    show_window(app);
                    let _ = app.emit(PLAY_EVENT, game.to_owned());
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Rebuilds the tray menu after the library changes.
pub fn refresh_tray(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), tray_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// Registers the global shortcut from Settings, replacing any earlier one.
pub fn apply_shortcut(app: &AppHandle, accelerator: &str) -> Result<(), String> {
    let manager = app.global_shortcut();
    let _ = manager.unregister_all();
    let accelerator = accelerator.trim();
    if accelerator.is_empty() {
        return Ok(());
    }
    let shortcut: Shortcut = accelerator
        .parse()
        .map_err(|_| format!("\"{accelerator}\" is not a key combination Starex understands."))?;
    manager
        .on_shortcut(shortcut, |app, _, event| {
            if event.state == ShortcutState::Pressed {
                show_window(app);
            }
        })
        .map_err(|_| format!("{accelerator} is already used by another program. Choose another combination."))
}

/// Turns starting with the system on or off.
pub fn apply_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let launcher = app.autolaunch();
    let current = launcher.is_enabled().unwrap_or(false);
    if current == enabled {
        return Ok(());
    }
    if enabled { launcher.enable() } else { launcher.disable() }
        .map_err(|e| format!("Could not change starting with the system: {e}"))
}

/// The signed-in user's name for greeting them: the display name Windows shows on the Start menu
/// (the Microsoft account's name when there is one), the full name on macOS and Linux, else the
/// account name. Read once and kept.
pub fn user_display_name() -> String {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        os_display_name()
            .or_else(|| std::env::var("USERNAME").or_else(|_| std::env::var("USER")).ok())
            .map(|n| n.trim().to_owned())
            .unwrap_or_default()
    })
    .clone()
}

#[cfg(windows)]
fn os_display_name() -> Option<String> {
    // The WinRT user API needs a COM apartment of its own, so ask from a short-lived thread.
    std::thread::spawn(|| {
        use windows::Foundation::IPropertyValue;
        use windows::System::{KnownUserProperties, User};
        use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
        use windows::core::Interface;

        // SAFETY: paired with CoUninitialize below, on this thread only.
        let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        let name = (|| {
            let users = User::FindAllAsync().ok()?.join().ok()?;
            let user = users.GetAt(0).ok()?;
            let value = user
                .GetPropertyAsync(&KnownUserProperties::DisplayName().ok()?)
                .ok()?
                .join()
                .ok()?;
            let name = value.cast::<IPropertyValue>().ok()?.GetString().ok()?.to_string();
            Some(name.trim().to_owned()).filter(|n| !n.is_empty())
        })();
        if com {
            // SAFETY: CoInitializeEx succeeded on this thread.
            unsafe { CoUninitialize() };
        }
        name
    })
    .join()
    .ok()
    .flatten()
}

#[cfg(target_os = "macos")]
fn os_display_name() -> Option<String> {
    let out = std::process::Command::new("id").arg("-F").output().ok()?;
    let name = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (out.status.success() && !name.is_empty()).then_some(name)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn os_display_name() -> Option<String> {
    // The first part of the account's comment field in /etc/passwd is its full name.
    let user = std::env::var("USER").ok()?;
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd
        .lines()
        .map(|line| line.split(':').collect::<Vec<_>>())
        .find(|fields| fields.first() == Some(&user.as_str()))
        .and_then(|fields| fields.get(4)?.split(',').next().map(str::trim).map(str::to_owned))
        .filter(|n| !n.is_empty())
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_the_users_name() {
        assert!(!super::user_display_name().is_empty());
    }
}
