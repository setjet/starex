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
use starex_core::stores;
use starex_core::verify::{self, CheckStatus, VerifyMode};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

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
    let stored = state.library().get(id).map_err(|e| e.to_string())?.clone();
    // The launch profile chosen for Play (DX11, modded, …) decides the options and executable.
    let game = stored.with_profile(stored.active_profile.as_deref());
    let settings = state.settings().clone();
    // A game from another launcher is started by that launcher, with none of Starex's own steps.
    let from_store = game.source.is_some();
    if from_store && !game.install_dir.is_dir() {
        return Err(format!(
            "{} is not installed. Install it in its launcher, then play it here.",
            game.name
        ));
    }

    if settings.verify_before_launch && !from_store {
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
        && !from_store
        && let Some(dir) = &game.save_dir
    {
        let backups = state.paths.save_backups(id);
        if saves::list(&backups).is_empty() {
            match saves::backup(dir, &backups, BackupReason::BeforeLaunch) {
                Ok(_) => crate::savedb::mirror_to_cloud(app, &game),
                Err(e) => log::warn!("could not back up saves before launch: {e}"),
            }
        }
    }

    let child = if let Some((source, url)) = game.source.as_ref().and_then(|s| Some((s, stores::launch_url(s)?))) {
        let launcher = match source.store {
            starex_core::model::Store::Steam => "Steam",
            _ => "the Epic Games Launcher",
        };
        if !stores::launcher_can_start(source.store) {
            return Err(format!(
                "{} can't start because {launcher} isn't installed or can't be opened. Install or repair {launcher}, then try again.",
                game.name
            ));
        }
        app.opener().open_url(url, None::<&str>).map_err(|e| {
            log::warn!("could not open the launcher link: {e}");
            format!(
                "{} can't start because {launcher} did not open. Open {launcher}, then try again.",
                game.name
            )
        })?;
        Started::Launcher
    } else {
        start_program(&game, &settings, from_store)?
    };
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

/// Starts the game's own program: with Starex's own launch steps for games the user added, plain
/// for a launcher's game that has no launcher link (GOG, Xbox).
fn start_program(game: &Game, settings: &starex_core::settings::Settings, from_store: bool) -> Result<Started, String> {
    let wine = launch::find_wine(&settings.wine_path);
    let mut command = launch::command(game, wine.as_deref()).map_err(|e| e.to_string())?;
    if !from_store && let Some(mut pre) = launch::pre_launch_command(game) {
        log::info!("running the pre-launch command of {}", game.name);
        run_pre_launch(&mut pre).map_err(|reason| format!("{} did not start: {reason}", game.name))?;
    }
    // "Run as administrator" goes straight to Windows' prompt, as for a program that asks for it.
    let spawned = if cfg!(windows) && game.run_as_admin && !from_store {
        Err(std::io::Error::from_raw_os_error(740))
    } else {
        command.spawn()
    };
    match spawned {
        Ok(child) => Ok(Started::Child(child)),
        // The program asks to run as administrator. Ask Windows, which shows its UAC prompt.
        #[cfg(windows)]
        Err(e) if e.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED) => {
            log::info!("{} needs administrator rights, asking Windows", game.name);
            Ok(Started::Elevated(elevated::spawn(&command).map_err(|reason| {
                format!("{} could not start: {reason}", game.name)
            })?))
        }
        Err(e) => Err(spawn_error(game, &e)),
    }
}

pub fn stop(app: &AppHandle, id: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let sessions = state.sessions();
    let session = sessions.get(id).ok_or("This game is not running.")?;
    session.stop.store(true, Ordering::Relaxed);
    Ok(())
}

/// How long a pre-launch command may run before Starex gives up on it.
const PRE_LAUNCH_LIMIT: Duration = Duration::from_secs(120);

/// Runs a game's pre-launch command and waits for it to finish.
fn run_pre_launch(command: &mut std::process::Command) -> Result<(), String> {
    let mut child = command
        .spawn()
        .map_err(|e| format!("its pre-launch command could not run ({e})."))?;
    let deadline = std::time::Instant::now() + PRE_LAUNCH_LIMIT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                let code = status.code().map_or_else(|| "none".to_owned(), |c| c.to_string());
                return Err(format!("its pre-launch command failed (exit code {code})."));
            }
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                return Err("its pre-launch command was still running after 2 minutes.".into());
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(format!("its pre-launch command could not be followed ({e}).")),
        }
    }
}

/// Turns a failed start into a sentence that says what to do.
fn spawn_error(game: &Game, err: &std::io::Error) -> String {
    let name = &game.name;
    let code = err.raw_os_error();
    let reason = match (cfg!(windows), code) {
        (true, Some(2 | 3)) => "its executable is missing. Verify the game files or choose the executable again in Properties.".to_owned(),
        (true, Some(5)) => "Windows denied access to its executable. Your antivirus may have blocked it, or the file is still being copied.".to_owned(),
        (true, Some(193 | 216)) => "its executable is damaged or built for a different kind of computer.".to_owned(),
        (true, Some(740)) => "it needs to run as administrator, and Windows did not allow it.".to_owned(),
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

/// Windows' answer when a program's manifest asks for administrator rights.
#[cfg(windows)]
const ERROR_ELEVATION_REQUIRED: i32 = 740;

/// How long a launcher may take to start its game (it may update it first) before Starex stops
/// waiting.
const LAUNCHER_WAIT: Duration = Duration::from_secs(180);

/// The process Starex started: an ordinary child, one Windows started as administrator, or none
/// when a launcher (Steam, Epic) starts the game and Starex follows it by its folder.
enum Started {
    Child(std::process::Child),
    #[cfg(windows)]
    Elevated(elevated::Process),
    Launcher,
}

impl Started {
    /// `Ok(Some(code))` once the process has ended.
    fn try_wait(&mut self) -> std::io::Result<Option<Option<i32>>> {
        match self {
            Started::Child(child) => child.try_wait().map(|s| s.map(|s| s.code())),
            #[cfg(windows)]
            Started::Elevated(process) => Ok(process.try_wait()),
            Started::Launcher => Ok(Some(None)),
        }
    }

    fn kill(&mut self) {
        match self {
            Started::Child(child) => {
                let _ = child.kill();
            }
            #[cfg(windows)]
            Started::Elevated(process) => process.kill(),
            Started::Launcher => {}
        }
    }
}

fn follow(app: AppHandle, game: Game, mut child: Started, started_at: u64, stop: Arc<AtomicBool>) {
    let root = path_key(&game.install_dir);
    let program_name = game
        .executable
        .is_file()
        .then(|| game.executable.file_name().map(|n| n.to_string_lossy().into_owned()))
        .flatten();
    let own_pid = Pid::from_u32(std::process::id());
    let mut system = System::new();
    let refresh = ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet);
    let mut exit_code = None;
    let mut child_alive = true;
    let mut exit_hint_override: Option<String> = None;
    // A launcher's game counts from when its program appears, and Starex waits for it to.
    let launcher_deadline = matches!(child, Started::Launcher).then(|| std::time::Instant::now() + LAUNCHER_WAIT);
    let mut first_seen: Option<u64> = None;

    // Show the game on Discord while it runs; dropping the presence clears it.
    let presence = {
        let settings = app.state::<AppState>().settings().clone();
        let app_id = if settings.discord_app_id.trim().is_empty() {
            crate::discord::DEFAULT_APP_ID.to_owned()
        } else {
            settings.discord_app_id.clone()
        };
        // A game Steam knows shows its own store art; Discord can only load images from the web.
        let image = stores::steam_app_id(&game)
            .map(|id| format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{id}/header.jpg"));
        (settings.discord_presence && game.show_on_discord)
            .then(|| crate::discord::Presence::show(&app_id, &game.name, started_at, image.as_deref()))
            .flatten()
    };

    loop {
        if child_alive {
            match child.try_wait() {
                Ok(Some(code)) => {
                    child_alive = false;
                    exit_code = code;
                }
                Ok(None) => {}
                Err(_) => child_alive = false,
            }
        }

        system.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh);
        let related: Vec<Pid> = system
            .processes()
            .iter()
            .filter(|(pid, p)| {
                **pid != own_pid
                    && match p.exe() {
                        Some(exe) => is_inside(&root, exe),
                        // Some games (often with anti-cheat) hide their path from other apps;
                        // then the program's name has to do.
                        None => program_name
                            .as_deref()
                            .is_some_and(|name| p.name().to_string_lossy().eq_ignore_ascii_case(name)),
                    }
            })
            .map(|(pid, _)| *pid)
            .collect();
        if first_seen.is_none() && !related.is_empty() {
            first_seen = Some(now_secs());
        }
        let waiting = related.is_empty()
            && first_seen.is_none()
            && launcher_deadline.is_some_and(|d| std::time::Instant::now() < d);

        if stop.load(Ordering::Relaxed) {
            if child_alive {
                child.kill();
            }
            for pid in &related {
                if let Some(process) = system.process(*pid) {
                    process.kill();
                }
            }
        }

        if !child_alive && related.is_empty() && (!waiting || stop.load(Ordering::Relaxed)) {
            break;
        }
        thread::sleep(Duration::from_millis(if child_alive { 1000 } else { 2000 }));
    }

    drop(presence);
    let ended_at = now_secs();
    let state = app.state::<AppState>();
    state.sessions().remove(&game.id);
    let from_launcher = launcher_deadline.is_some();
    let started_at = if from_launcher {
        first_seen.unwrap_or(ended_at)
    } else {
        started_at
    };
    // A launcher that never started the game leaves no session to record.
    let updated = if from_launcher && first_seen.is_none() {
        if !stop.load(Ordering::Relaxed) {
            exit_hint_override = Some(
                "Its launcher did not start it within 3 minutes. Open the launcher to see whether it is updating or needs attention."
                    .to_owned(),
            );
        }
        state.library().get(&game.id).ok().cloned()
    } else {
        state
            .library()
            .record_session(&game.id, started_at, ended_at)
            .map_err(|e| log::warn!("could not record play time: {e}"))
            .ok()
    };
    log::info!("{} closed after {}s", game.name, ended_at - started_at);

    // Keep a copy of the saves this session produced. The save folder may have been changed
    // in Properties while the game ran, so use the library's current entry.
    let mut latest = updated.clone().unwrap_or_else(|| game.clone());
    // A launcher's game keeps its saves to itself.
    let own = latest.source.is_none();
    // A game that saved for the first time may now have a save folder the database knows.
    if own
        && latest.save_dir.is_none()
        && let Some(db) = state.savedb().clone()
        && let Some(dir) = crate::savedb::save_dirs(&db, &latest).into_iter().next()
        && let Ok(found) = state.library().update(&game.id, |g| g.save_dir = Some(dir))
    {
        let _ = app.emit(GAME_EVENT, &found);
        latest = found;
    }
    if own
        && latest.backup_saves
        && let Some(dir) = &latest.save_dir
    {
        match saves::backup(dir, &state.paths.save_backups(&game.id), BackupReason::AfterSession) {
            Ok(Some(b)) => {
                log::info!("backed up {} save files", b.files);
                crate::savedb::mirror_to_cloud(&app, &latest);
            }
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
            exit_hint: exit_hint_override.or_else(|| exit_hint(exit_code)),
            game: updated.map(Box::new),
        },
    );

    crate::system::refresh_tray(&app);

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

/// Starting a program as administrator through Windows' UAC prompt.
#[cfg(windows)]
mod elevated {
    use std::ffi::OsStr;
    use std::process::Command;

    use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, HANDLE, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{GetExitCodeProcess, TerminateProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{HSTRING, PCWSTR, w};

    /// The started program. Without a handle (Windows does not always give one), Starex follows
    /// the game through the processes in its folder instead.
    pub struct Process(Option<HANDLE>);

    // SAFETY: a process handle can be waited on and closed from any thread.
    unsafe impl Send for Process {}

    impl Process {
        pub fn try_wait(&mut self) -> Option<Option<i32>> {
            let Some(handle) = self.0 else { return Some(None) };
            // SAFETY: the handle is open until `drop`.
            unsafe {
                if WaitForSingleObject(handle, 0) != WAIT_OBJECT_0 {
                    return None;
                }
                let mut code = 0u32;
                Some(GetExitCodeProcess(handle, &mut code).ok().map(|_| code as i32))
            }
        }

        /// Best effort: Windows may not let Starex end a program running as administrator.
        pub fn kill(&mut self) {
            if let Some(handle) = self.0 {
                // SAFETY: the handle is open until `drop`.
                unsafe {
                    let _ = TerminateProcess(handle, 1);
                }
            }
        }
    }

    impl Drop for Process {
        fn drop(&mut self) {
            if let Some(handle) = self.0.take() {
                // SAFETY: closed exactly once.
                unsafe {
                    let _ = CloseHandle(handle);
                }
            }
        }
    }

    /// Starts `command` as administrator. Windows shows its UAC prompt first.
    pub fn spawn(command: &Command) -> Result<Process, String> {
        if command.get_envs().next().is_some() {
            log::warn!("environment variables are not passed to a game started as administrator");
        }
        let file = HSTRING::from(command.get_program());
        let parameters = HSTRING::from(join_args(command.get_args()));
        let directory = command.get_current_dir().map(|d| HSTRING::from(d.as_os_str()));
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: w!("runas"),
            lpFile: PCWSTR(file.as_ptr()),
            lpParameters: PCWSTR(parameters.as_ptr()),
            lpDirectory: directory.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        // SAFETY: `info` and the strings it points to live until the call returns.
        if let Err(e) = unsafe { ShellExecuteExW(&mut info) } {
            return Err(if e.code() == ERROR_CANCELLED.to_hresult() {
                "it needs to run as administrator, and the Windows prompt was declined.".to_owned()
            } else {
                format!("Windows could not start it as administrator ({}).", e.message())
            });
        }
        Ok(Process((!info.hProcess.is_invalid()).then_some(info.hProcess)))
    }

    /// Joins arguments into one Windows command line, quoting the way programs parse it back.
    pub(super) fn join_args<'a>(args: impl Iterator<Item = &'a OsStr>) -> String {
        args.map(|a| quote(&a.to_string_lossy())).collect::<Vec<_>>().join(" ")
    }

    fn quote(arg: &str) -> String {
        if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
            return arg.to_owned();
        }
        let mut out = String::from('"');
        let mut backslashes = 0;
        for c in arg.chars() {
            if c == '\\' {
                backslashes += 1;
                continue;
            }
            let escape = if c == '"' { backslashes * 2 + 1 } else { backslashes };
            out.extend(std::iter::repeat_n('\\', escape));
            out.push(c);
            backslashes = 0;
        }
        out.extend(std::iter::repeat_n('\\', backslashes * 2));
        out.push('"');
        out
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

    #[cfg(windows)]
    #[test]
    fn quotes_arguments_for_an_elevated_start() {
        use std::ffi::OsStr;
        let args = ["-windowed", "-profile=My Save", "", r#"say "hi""#, r"C:\My Games\"];
        assert_eq!(
            elevated::join_args(args.iter().map(OsStr::new)),
            r#"-windowed "-profile=My Save" "" "say \"hi\"" "C:\My Games\\""#
        );
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
