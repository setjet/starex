//! Starts games and follows them until they close.
//!
//! Many games start through a small bootstrap executable that exits right away and leaves the
//! real game running. So a session lasts while the started process *or any process whose
//! executable lives in the game folder* is alive.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use starex_core::launch;
use starex_core::model::Game;
use starex_core::now_secs;
use starex_core::saves::{self, BackupReason};
use starex_core::settings::LaunchBehavior;
use starex_core::verify::{self, CheckStatus, VerifyMode};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::{AppState, Session};

pub const SESSION_EVENT: &str = "starex://session";
pub const GAME_EVENT: &str = "starex://game-updated";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "state")]
pub enum SessionEvent {
    Running {
        game_id: String,
        started_at: u64,
    },
    Exited {
        game_id: String,
        seconds: u64,
        exit_code: Option<i32>,
        /// A plain explanation when the exit code points at a known problem.
        exit_hint: Option<String>,
        game: Option<Box<Game>>,
    },
}

pub fn launch(app: &AppHandle, id: &str) -> Result<u64, String> {
    let state = app.state::<AppState>();
    if state.sessions().contains_key(id) {
        return Err("This game is already running.".into());
    }
    let game = state.library().get(id).map_err(|e| e.to_string())?.clone();
    let settings = state.settings().clone();

    if settings.verify_before_launch {
        let report = verify::verify(
            &game,
            &state.paths.baseline(id),
            VerifyMode::Quick,
            &AtomicBool::new(false),
            &|_| {},
        );
        if report.status == CheckStatus::Fail {
            let reason = report
                .checks
                .iter()
                .find(|c| c.status == CheckStatus::Fail)
                .map(|c| c.detail.clone())
                .unwrap_or_default();
            return Err(format!("Verification failed: {reason}"));
        }
    }

    // The first time a game runs with a save folder, keep a copy of the saves it starts from.
    if game.backup_saves
        && let Some(dir) = &game.save_dir
    {
        let backups = state.paths.save_backups(id);
        if saves::list(&backups).is_empty()
            && let Err(e) = saves::backup(dir, &backups, BackupReason::BeforeLaunch)
        {
            log::warn!("could not back up saves before launch: {e}");
        }
    }

    let wine = launch::find_wine(&settings.wine_path);
    let mut command = launch::command(&game, wine.as_deref()).map_err(|e| e.to_string())?;
    let child = command.spawn().map_err(|e| spawn_error(&game, &e))?;
    let started_at = now_secs();
    let stop = Arc::new(AtomicBool::new(false));
    state.sessions().insert(
        id.to_owned(),
        Session {
            started_at,
            stop: stop.clone(),
        },
    );
    log::info!("launched {} ({})", game.name, game.executable.display());
    let _ = app.emit(
        SESSION_EVENT,
        SessionEvent::Running {
            game_id: id.to_owned(),
            started_at,
        },
    );

    if let Some(window) = app.get_webview_window("main") {
        match settings.launch_behavior {
            LaunchBehavior::KeepOpen => {}
            LaunchBehavior::Minimize => {
                let _ = window.minimize();
            }
            LaunchBehavior::Hide => {
                let _ = window.hide();
            }
        }
    }

    let app = app.clone();
    thread::Builder::new()
        .name(format!("session-{id}"))
        .spawn(move || follow(app, game, child, started_at, stop))
        .map_err(|e| e.to_string())?;
    Ok(started_at)
}

pub fn stop(app: &AppHandle, id: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let sessions = state.sessions();
    let session = sessions.get(id).ok_or("This game is not running.")?;
    session.stop.store(true, Ordering::Relaxed);
    Ok(())
}

/// Turns a failed start into a sentence that says what to do.
fn spawn_error(game: &Game, err: &std::io::Error) -> String {
    let name = &game.name;
    let code = err.raw_os_error();
    let reason = match (cfg!(windows), code) {
        (true, Some(2 | 3)) => "its executable is missing. Verify the game files or choose the executable again in Properties.".to_owned(),
        (true, Some(5)) => "Windows denied access to its executable. Your antivirus may have blocked it, or the file is still being copied.".to_owned(),
        (true, Some(193 | 216)) => "its executable is damaged or built for a different kind of computer.".to_owned(),
        (true, Some(740)) => "it needs to run as administrator. Right-click its executable, open Properties > Compatibility, and tick \"Run this program as an administrator\".".to_owned(),
        (true, Some(1260)) => "a security policy on this computer blocks it.".to_owned(),
        (true, Some(14001)) => "a runtime it needs is missing. Install the Microsoft Visual C++ Redistributable (x64), often included in the game's _CommonRedist folder.".to_owned(),
        (false, Some(13)) => "its executable is not marked as runnable. Allow it with chmod +x, or check the folder's permissions.".to_owned(),
        (false, Some(8)) => "its executable is not a program this system can run.".to_owned(),
        _ if err.kind() == std::io::ErrorKind::NotFound => "its executable or the program needed to run it is missing.".to_owned(),
        _ => err.to_string(),
    };
    format!("{name} could not start: {reason}")
}

/// Explains the exit codes that mean "the game could not even start properly".
pub fn exit_hint(code: Option<i32>) -> Option<String> {
    let code = code? as u32;
    let hint = match code {
        0xC000_0135 => {
            "A DLL the game needs is missing. Install the Visual C++ and DirectX runtimes, often in the game's _CommonRedist folder."
        }
        0xC000_007B => {
            "A DLL the game loaded is built for the wrong processor (32-bit and 64-bit mixed). Reinstall the game's runtimes."
        }
        0xC000_0142 => "The game failed to initialise. Restart your computer and try again.",
        0xC000_0005 => "The game crashed (access violation). Verify its files and update your graphics driver.",
        0xC000_0409 => "The game stopped itself after detecting a problem. Verify its files.",
        0xC000_001D => "The game uses processor instructions this computer does not support.",
        0xC000_0022 => "Windows denied the game access. Your antivirus may be blocking it.",
        _ => return None,
    };
    Some(hint.to_owned())
}

fn follow(app: AppHandle, game: Game, mut child: std::process::Child, started_at: u64, stop: Arc<AtomicBool>) {
    let root = path_key(&game.install_dir);
    let own_pid = Pid::from_u32(std::process::id());
    let mut system = System::new();
    let refresh = ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet);
    let mut exit_code = None;
    let mut child_alive = true;

    loop {
        if child_alive {
            match child.try_wait() {
                Ok(Some(status)) => {
                    child_alive = false;
                    exit_code = status.code();
                }
                Ok(None) => {}
                Err(_) => child_alive = false,
            }
        }

        system.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh);
        let related: Vec<Pid> = system
            .processes()
            .iter()
            .filter(|(pid, p)| **pid != own_pid && p.exe().is_some_and(|exe| is_inside(&root, exe)))
            .map(|(pid, _)| *pid)
            .collect();

        if stop.load(Ordering::Relaxed) {
            if child_alive {
                let _ = child.kill();
            }
            for pid in &related {
                if let Some(process) = system.process(*pid) {
                    process.kill();
                }
            }
        }

        if !child_alive && related.is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(if child_alive { 1000 } else { 2000 }));
    }

    let ended_at = now_secs();
    let state = app.state::<AppState>();
    state.sessions().remove(&game.id);
    let updated = state
        .library()
        .record_session(&game.id, started_at, ended_at)
        .map_err(|e| log::warn!("could not record play time: {e}"))
        .ok();
    log::info!("{} closed after {}s", game.name, ended_at - started_at);

    // Keep a copy of the saves this session produced. The save folder may have been changed
    // in Properties while the game ran, so use the library's current entry.
    let latest = updated.clone().unwrap_or_else(|| game.clone());
    if latest.backup_saves
        && let Some(dir) = &latest.save_dir
    {
        match saves::backup(dir, &state.paths.save_backups(&game.id), BackupReason::AfterSession) {
            Ok(Some(b)) => log::info!("backed up {} save files", b.files),
            Ok(None) => {}
            Err(e) => log::warn!("could not back up saves: {e}"),
        }
    }

    let _ = app.emit(
        SESSION_EVENT,
        SessionEvent::Exited {
            game_id: game.id.clone(),
            seconds: ended_at.saturating_sub(started_at),
            exit_code,
            exit_hint: exit_hint(exit_code),
            game: updated.map(Box::new),
        },
    );

    let settings = state.settings().clone();
    if settings.restore_on_exit
        && settings.launch_behavior != LaunchBehavior::KeepOpen
        && let Some(window) = app.get_webview_window("main")
    {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// A path in a form that compares the way Windows does: case-insensitive, `\` separated, no
/// trailing separator and no `\\?\` prefix.
fn path_key(path: &Path) -> String {
    let mut s = path.to_string_lossy().replace('/', "\\");
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        s = rest.to_owned();
    }
    let trimmed = s.trim_end_matches('\\');
    if cfg!(windows) {
        trimmed.to_lowercase()
    } else {
        trimmed.to_owned()
    }
}

fn is_inside(root: &str, path: &Path) -> bool {
    let key = path_key(path);
    key.len() > root.len() && key.starts_with(root) && key.as_bytes()[root.len()] == b'\\'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_events_use_camel_case_fields() {
        let json = serde_json::to_value(SessionEvent::Exited {
            game_id: "g".into(),
            seconds: 3,
            exit_code: Some(1),
            exit_hint: None,
            game: None,
        })
        .unwrap();
        assert_eq!(json["state"], "exited");
        assert_eq!(json["gameId"], "g");
        assert_eq!(json["exitCode"], 1);
    }

    #[test]
    fn explains_common_exit_codes() {
        assert!(exit_hint(Some(0xC000_0135_u32 as i32)).unwrap().contains("DLL"));
        assert!(exit_hint(Some(0)).is_none());
        assert!(exit_hint(None).is_none());
    }

    #[test]
    fn matches_processes_in_the_game_folder() {
        let root = path_key(Path::new(r"C:\Games\Star"));
        assert!(is_inside(&root, Path::new(r"C:\Games\Star\bin\star.exe")));
        assert!(is_inside(&root, Path::new(r"\\?\C:\Games\Star\star.exe")));
        assert!(!is_inside(&root, Path::new(r"C:\Games\Starfield\star.exe")));
        #[cfg(windows)]
        assert!(is_inside(&root, Path::new(r"c:\games\STAR\x.exe")));
    }
}
