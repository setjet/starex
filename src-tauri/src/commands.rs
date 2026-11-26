//! The commands the UI can call. Each one is a thin wrapper over `starex-core`; slow work runs on
//! a blocking thread so the window never stalls.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use starex_core::antivirus::{self, ScanReport, ScanStatus, ScanSummary};
use starex_core::inspect::{self, FolderInspection};
use starex_core::manifest::{self, Manifest, Progress};
use starex_core::model::{Artwork, ArtworkKind, Game, GamePatch, NewGame};
use starex_core::runtimes::{self, Runtime};
use starex_core::saves::{self, BackupReason, SaveBackup, SaveLocation};
use starex_core::settings::Settings;
use starex_core::verify::{self, VerifyMode, VerifyReport};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::artwork;
use crate::details;
use crate::firewall;
use crate::releases;
use crate::savedb;
use crate::sessions::{self, GAME_EVENT};
use crate::shortcuts::{self, Place};
use crate::state::{self, AppState};
use crate::system;

type CmdResult<T> = Result<T, String>;

const TASK_EVENT: &str = "starex://task-progress";
/// Asks the interface to start a game (from a shortcut or a starex:// link).
pub const PLAY_EVENT: &str = "starex://play-request";
const REPOSITORY: &str = "https://github.com/setjet/starex";
/// The creator's X profile, linked from Settings > About.
const CREATOR_X: &str = "https://x.com/inthecayenne";

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningGame {
    game_id: String,
    started_at: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    games: Vec<Game>,
    settings: Settings,
    running: Vec<RunningGame>,
    data_dir: PathBuf,
    /// Started with the system: stay in the tray instead of opening the window.
    start_hidden: bool,
    /// Starex ships with its own Discord application ID.
    discord_builtin: bool,
    host_arch: &'static str,
    host_os: &'static str,
    repository: &'static str,
}

#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> Bootstrap {
    Bootstrap {
        games: state.library().games().to_vec(),
        settings: state.settings().clone(),
        running: state
            .sessions()
            .iter()
            .map(|(id, s)| RunningGame {
                game_id: id.clone(),
                started_at: s.started_at,
            })
            .collect(),
        data_dir: state.paths.root.clone(),
        start_hidden: std::env::args().any(|a| a == system::START_HIDDEN_ARG),
        discord_builtin: !crate::discord::DEFAULT_APP_ID.is_empty(),
        host_arch: std::env::consts::ARCH,
        host_os: std::env::consts::OS,
        repository: REPOSITORY,
    }
}

#[tauri::command]
pub async fn inspect_folder(path: PathBuf) -> CmdResult<FolderInspection> {
    tauri::async_runtime::spawn_blocking(move || inspect::inspect_folder(&path))
        .await
        .map_err(err)?
        .map_err(|e| format!("Could not read that folder: {e}"))
}

#[tauri::command]
pub async fn add_game(
    app: AppHandle,
    game: NewGame,
    detected: Option<Artwork>,
    scan: Option<ScanSummary>,
) -> CmdResult<Game> {
    let state = app.state::<AppState>();
    let added = state.library().add(game).map_err(err)?;

    let mut stored = Artwork::default();
    if let Some(detected) = detected {
        artwork::import_detected(&state.paths, &added.id, &mut stored, &detected);
    }
    // The folder was virus-scanned before it was added; keep that result with the game.
    let mut added = state
        .library()
        .update(&added.id, |g| {
            g.artwork = stored;
            g.last_scan = scan;
        })
        .map_err(err)?;

    let settings = state.settings().clone();
    // Find the save folder in the background from the save database, and the game's details.
    {
        let (app, id) = (app.clone(), added.id.clone());
        let wants_details =
            !settings.igdb_client_id.trim().is_empty() && !settings.igdb_client_secret.trim().is_empty();
        tauri::async_runtime::spawn(async move {
            detect_save_dir(&app, &id).await;
            if wants_details && let Err(e) = find_details(&app, &id).await {
                log::info!("automatic game details skipped: {e}");
            }
        });
    }
    system::refresh_tray(&app);
    if settings.auto_artwork && !settings.steamgriddb_key.trim().is_empty() {
        match fetch(&app, &added.id, false).await {
            Ok(updated) => added = updated,
            Err(e) => log::info!("automatic artwork skipped: {e}"),
        }
    }
    Ok(added)
}

#[tauri::command]
pub fn update_game(app: AppHandle, id: String, patch: GamePatch) -> CmdResult<Game> {
    let game = app.state::<AppState>().library().patch(&id, patch).map_err(err)?;
    system::refresh_tray(&app);
    Ok(game)
}

#[tauri::command]
pub fn remove_game(app: AppHandle, id: String) -> CmdResult<()> {
    let state = app.state::<AppState>();
    if state.sessions().contains_key(&id) {
        return Err("Close the game before removing it from your library.".into());
    }
    state.library().remove(&id).map_err(err)?;
    artwork::remove_all(&state.paths, &id);
    manifest::delete(&state.paths.baseline(&id)).map_err(err)?;
    system::refresh_tray(&app);
    Ok(())
}

/// Sets one artwork slot from an image file, or clears it when `source` is `None`.
#[tauri::command]
pub fn set_artwork(
    state: State<'_, AppState>,
    id: String,
    kind: ArtworkKind,
    source: Option<PathBuf>,
) -> CmdResult<Game> {
    state.library().get(&id).map_err(err)?;
    let stored = match source {
        Some(source) => Some(artwork::import(&state.paths, &id, kind, &source)?),
        None => {
            artwork::remove(&state.paths, &id, kind);
            None
        }
    };
    state
        .library()
        .update(&id, |g| g.artwork.set(kind, stored))
        .map_err(err)
}

async fn fetch(app: &AppHandle, id: &str, overwrite: bool) -> CmdResult<Game> {
    let state = app.state::<AppState>();
    let game = state.library().get(id).map_err(err)?.clone();
    let key = state.settings().steamgriddb_key.clone();
    let artwork = artwork::fetch_steamgriddb(
        &state.http,
        &state.paths,
        &key,
        id,
        &game.name,
        &game.artwork,
        overwrite,
    )
    .await?;
    let updated = state.library().update(id, |g| g.artwork = artwork).map_err(err)?;
    let _ = app.emit(GAME_EVENT, &updated);
    Ok(updated)
}

#[tauri::command]
pub async fn fetch_artwork(app: AppHandle, id: String, overwrite: bool) -> CmdResult<Game> {
    fetch(&app, &id, overwrite).await
}

#[tauri::command]
pub fn launch_game(app: AppHandle, id: String) -> CmdResult<u64> {
    sessions::launch(&app, &id)
}

#[tauri::command]
pub fn stop_game(app: AppHandle, id: String) -> CmdResult<()> {
    sessions::stop(&app, &id)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskProgress<'a> {
    game_id: &'a str,
    task: &'static str,
    progress: Progress,
    /// The task has ended (finished, failed or cancelled); the UI clears its progress bar.
    done: bool,
}

/// Unregisters a task and tells the UI it ended, however it ended.
fn end_task(app: &AppHandle, id: &str, task: &'static str) {
    app.state::<AppState>().tasks().remove(id);
    let _ = app.emit(
        TASK_EVENT,
        TaskProgress {
            game_id: id,
            task,
            progress: Progress {
                done_files: 0,
                total_files: 0,
                done_bytes: 0,
                total_bytes: 0,
            },
            done: true,
        },
    );
}

/// Registers a cancellable task for a game, refusing a second one at the same time.
fn begin_task(state: &AppState, id: &str) -> CmdResult<Arc<AtomicBool>> {
    let mut tasks = state.tasks();
    if tasks.contains_key(id) {
        return Err("Starex is already checking this game's files.".into());
    }
    let flag = Arc::new(AtomicBool::new(false));
    tasks.insert(id.to_owned(), flag.clone());
    Ok(flag)
}

#[tauri::command]
pub async fn verify_game(app: AppHandle, id: String, mode: VerifyMode) -> CmdResult<VerifyReport> {
    let state = app.state::<AppState>();
    let game = state.library().get(&id).map_err(err)?.clone();
    let baseline = state.paths.baseline(&id);
    let cancel = begin_task(&state, &id)?;

    let emitter = app.clone();
    let report = tauri::async_runtime::spawn_blocking(move || {
        let sink = |progress: Progress| {
            let _ = emitter.emit(
                TASK_EVENT,
                TaskProgress {
                    game_id: &game.id,
                    task: "verify",
                    progress,
                    done: false,
                },
            );
        };
        verify::verify(&game, &baseline, mode, &cancel, &sink)
    })
    .await;
    end_task(&app, &id, "verify");
    let report = report.map_err(err)?;

    if !report.cancelled {
        let summary = report.summary();
        if let Ok(updated) = state.library().update(&id, |g| g.last_verification = Some(summary)) {
            let _ = app.emit(GAME_EVENT, &updated);
        }
    }
    Ok(report)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselineInfo {
    files: usize,
    total_bytes: u64,
    generated_at: Option<u64>,
    /// `true` when the game folder ships its own `starex.manifest.json`, which takes precedence.
    shipped: bool,
}

fn baseline_summary(manifest: &Manifest, shipped: bool) -> BaselineInfo {
    BaselineInfo {
        files: manifest.files.len(),
        total_bytes: manifest.total_bytes(),
        generated_at: manifest.generated_at,
        shipped,
    }
}

#[tauri::command]
pub async fn create_baseline(app: AppHandle, id: String) -> CmdResult<Option<BaselineInfo>> {
    let state = app.state::<AppState>();
    let game = state.library().get(&id).map_err(err)?.clone();
    let target = state.paths.baseline(&id);
    let cancel = begin_task(&state, &id)?;

    let emitter = app.clone();
    let built = tauri::async_runtime::spawn_blocking(move || {
        let sink = |progress: Progress| {
            let _ = emitter.emit(
                TASK_EVENT,
                TaskProgress {
                    game_id: &game.id,
                    task: "baseline",
                    progress,
                    done: false,
                },
            );
        };
        let manifest = manifest::build(&game.install_dir, &game.name, Some(&game.executable), &cancel, &sink)?;
        if let Some(manifest) = &manifest {
            manifest.save(&target)?;
        }
        Ok::<_, starex_core::Error>(manifest)
    })
    .await;
    end_task(&app, &id, "baseline");
    let built = built.map_err(err)?.map_err(err)?;
    Ok(built.map(|m| baseline_summary(&m, false)))
}

#[tauri::command]
pub fn delete_baseline(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    manifest::delete(&state.paths.baseline(&id)).map_err(err)
}

#[tauri::command]
pub async fn baseline_info(app: AppHandle, id: String) -> CmdResult<Option<BaselineInfo>> {
    let state = app.state::<AppState>();
    let game = state.library().get(&id).map_err(err)?.clone();
    let baseline = state.paths.baseline(&id);
    tauri::async_runtime::spawn_blocking(move || {
        let shipped = game.install_dir.join(manifest::MANIFEST_FILE);
        if let Ok(Some(m)) = Manifest::load(&shipped) {
            return Ok(Some(baseline_summary(&m, true)));
        }
        Manifest::load(&baseline)
            .map(|m| m.map(|m| baseline_summary(&m, false)))
            .map_err(err)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub fn cancel_task(state: State<'_, AppState>, id: String) {
    if let Some(flag) = state.tasks().get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Saves Settings and applies the ones that change the system (the global shortcut, starting
/// with the system). A change the system refuses is undone and reported.
#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> CmdResult<Settings> {
    let state = app.state::<AppState>();
    let previous = state.settings().clone();
    let mut settings = settings;
    settings.steamgriddb_key = settings.steamgriddb_key.trim().to_owned();
    settings.igdb_client_id = settings.igdb_client_id.trim().to_owned();
    settings.igdb_client_secret = settings.igdb_client_secret.trim().to_owned();
    settings.discord_app_id = settings.discord_app_id.trim().to_owned();
    settings.global_shortcut = settings.global_shortcut.trim().to_owned();

    let mut problem = None;
    if settings.global_shortcut != previous.global_shortcut
        && let Err(e) = system::apply_shortcut(&app, &settings.global_shortcut)
    {
        settings.global_shortcut = previous.global_shortcut.clone();
        let _ = system::apply_shortcut(&app, &previous.global_shortcut);
        problem = Some(e);
    }
    if settings.start_with_system != previous.start_with_system
        && let Err(e) = system::apply_autostart(&app, settings.start_with_system)
    {
        settings.start_with_system = previous.start_with_system;
        problem = Some(e);
    }
    settings.save(&state.paths.settings).map_err(err)?;
    *state.settings() = settings.clone();
    match problem {
        Some(e) => Err(e),
        None => Ok(settings),
    }
}

/// Uses an image as the window's backdrop and switches the backdrop to it.
#[tauri::command]
pub fn set_backdrop_image(state: State<'_, AppState>, source: PathBuf) -> CmdResult<Settings> {
    let stored = artwork::import_backdrop(&state.paths, &source)?;
    let mut settings = state.settings().clone();
    settings.backdrop_image = stored.display().to_string();
    settings.backdrop = starex_core::settings::Backdrop::Custom;
    settings.save(&state.paths.settings).map_err(err)?;
    *state.settings() = settings.clone();
    Ok(settings)
}

async fn find_details(app: &AppHandle, id: &str) -> CmdResult<Game> {
    let state = app.state::<AppState>();
    let game = state.library().get(id).map_err(err)?.clone();
    let (client_id, secret) = {
        let s = state.settings();
        (s.igdb_client_id.clone(), s.igdb_client_secret.clone())
    };
    let found = details::fetch(&state.http, &client_id, &secret, &game.name).await?;
    let updated = state.library().update(id, |g| g.details = Some(found)).map_err(err)?;
    let _ = app.emit(GAME_EVENT, &updated);
    Ok(updated)
}

/// Looks the game up on IGDB: description, genres, release date, developer, time to beat.
#[tauri::command]
pub async fn fetch_details(app: AppHandle, id: String) -> CmdResult<Game> {
    find_details(&app, &id).await
}

/// Starex's release notes from GitHub, newest first, for the changelog.
#[tauri::command]
pub async fn fetch_changelog(state: State<'_, AppState>) -> CmdResult<Vec<releases::Release>> {
    releases::fetch(&state.http).await
}

/// Blocks or allows the game's internet access with Windows Firewall.
#[tauri::command]
pub async fn set_offline(app: AppHandle, id: String, offline: bool) -> CmdResult<Game> {
    let state = app.state::<AppState>();
    let game = state.library().get(&id).map_err(err)?.clone();
    tauri::async_runtime::spawn_blocking(move || firewall::set(&game, offline))
        .await
        .map_err(err)??;
    let updated = state.library().update(&id, |g| g.offline = offline).map_err(err)?;
    let _ = app.emit(GAME_EVENT, &updated);
    Ok(updated)
}

pub const SCAN_EVENT: &str = "starex://scan-finished";

/// Scans a game's code files and records the result. Every finished scan is also announced as
/// an event, so automatic scans after adding a game reach the UI too.
async fn run_scan(app: &AppHandle, id: &str) -> CmdResult<ScanReport> {
    let state = app.state::<AppState>();
    let game = state.library().get(id).map_err(err)?.clone();
    let cancel = begin_task(&state, id)?;
    let emitter = app.clone();
    let report = tauri::async_runtime::spawn_blocking(move || {
        let sink = |progress: Progress| {
            let _ = emitter.emit(
                TASK_EVENT,
                TaskProgress {
                    game_id: &game.id,
                    task: "scan",
                    progress,
                    done: false,
                },
            );
        };
        antivirus::scan(&game.id, &game.install_dir, &cancel, &sink)
    })
    .await;
    end_task(app, id, "scan");
    let report = report.map_err(err)?;
    if report.status != ScanStatus::Cancelled
        && let Ok(updated) = state.library().update(id, |g| g.last_scan = Some(report.summary()))
    {
        let _ = app.emit(GAME_EVENT, &updated);
    }
    log::info!(
        "virus scan of {id}: {:?}, {} files",
        report.status,
        report.files_scanned
    );
    let _ = app.emit(SCAN_EVENT, &report);
    Ok(report)
}

/// Scans a folder before it becomes a game. `task_id` names the task for progress and
/// cancellation.
#[tauri::command]
pub async fn scan_folder(app: AppHandle, path: PathBuf, task_id: String) -> CmdResult<ScanReport> {
    let state = app.state::<AppState>();
    if !path.is_dir() {
        return Err("That folder no longer exists.".into());
    }
    let cancel = begin_task(&state, &task_id)?;
    let (emitter, id) = (app.clone(), task_id.clone());
    let report = tauri::async_runtime::spawn_blocking(move || {
        let sink = |progress: Progress| {
            let _ = emitter.emit(
                TASK_EVENT,
                TaskProgress {
                    game_id: &id,
                    task: "scan",
                    progress,
                    done: false,
                },
            );
        };
        antivirus::scan(&id, &path, &cancel, &sink)
    })
    .await;
    end_task(&app, &task_id, "scan");
    let report = report.map_err(err)?;
    log::info!(
        "virus scan before adding: {:?}, {} files",
        report.status,
        report.files_scanned
    );
    Ok(report)
}

#[tauri::command]
pub async fn scan_game(app: AppHandle, id: String) -> CmdResult<ScanReport> {
    run_scan(&app, &id).await
}

/// Sets a game's save folder from the save database, when it has none and the database knows one
/// that exists.
async fn detect_save_dir(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    let Ok(game) = state.library().get(id).cloned() else {
        return;
    };
    if game.save_dir.is_some() {
        return;
    }
    let Some(db) = savedb::get(app).await else { return };
    if let Some(dir) = savedb::save_dirs(&db, &game).into_iter().next() {
        log::info!("save folder for {} found in the save database", game.name);
        if let Ok(updated) = state.library().update(id, |g| {
            if g.save_dir.is_none() {
                g.save_dir = Some(dir);
            }
        }) {
            let _ = app.emit(GAME_EVENT, &updated);
        }
    }
}

#[tauri::command]
pub async fn suggest_save_locations(app: AppHandle, id: String) -> CmdResult<Vec<SaveLocation>> {
    let game = app.state::<AppState>().library().get(&id).map_err(err)?.clone();
    let db = savedb::get(&app).await;
    tauri::async_runtime::spawn_blocking(move || {
        let mut found: Vec<SaveLocation> = db
            .map(|db| savedb::save_dirs(&db, &game))
            .unwrap_or_default()
            .into_iter()
            .map(|dir| saves::location(dir, true))
            .collect();
        for guess in saves::suggest_locations(&game.name, &game.install_dir, &state::save_roots()) {
            if !found.iter().any(|f| f.path == guess.path) {
                found.push(guess);
            }
        }
        found
    })
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn list_save_backups(app: AppHandle, id: String) -> CmdResult<Vec<SaveBackup>> {
    let dir = app.state::<AppState>().paths.save_backups(&id);
    tauri::async_runtime::spawn_blocking(move || saves::list(&dir))
        .await
        .map_err(err)
}

fn save_folder(game: &Game) -> CmdResult<PathBuf> {
    game.save_dir
        .clone()
        .ok_or_else(|| "Choose the game's save folder first, in Properties > Saves.".to_owned())
}

#[tauri::command]
pub async fn backup_saves(app: AppHandle, id: String) -> CmdResult<Option<SaveBackup>> {
    let state = app.state::<AppState>();
    let game = state.library().get(&id).map_err(err)?.clone();
    let source = save_folder(&game)?;
    let target = state.paths.save_backups(&id);
    let backup = tauri::async_runtime::spawn_blocking(move || saves::backup(&source, &target, BackupReason::Manual))
        .await
        .map_err(err)?
        .map_err(err)?;
    savedb::mirror_to_cloud(&app, &game);
    Ok(backup)
}

/// The system runtimes a game needs and whether each is installed.
#[tauri::command]
pub async fn check_runtimes(app: AppHandle, id: String) -> CmdResult<Vec<Runtime>> {
    let game = app.state::<AppState>().library().get(&id).map_err(err)?.clone();
    tauri::async_runtime::spawn_blocking(move || runtimes::check(&game.executable, &game.install_dir))
        .await
        .map_err(err)
}

/// Starts a runtime's installer from the game folder, or opens Microsoft's download.
#[tauri::command]
pub async fn install_runtime(app: AppHandle, id: String, runtime_id: String) -> CmdResult<()> {
    let game = app.state::<AppState>().library().get(&id).map_err(err)?.clone();
    let list = tauri::async_runtime::spawn_blocking(move || runtimes::check(&game.executable, &game.install_dir))
        .await
        .map_err(err)?;
    let runtime = list
        .into_iter()
        .find(|r| r.id == runtime_id)
        .ok_or("This runtime is no longer needed.")?;
    let opener = app.opener();
    if let Some(installer) = runtime.installer {
        // Opened through the shell, so Windows asks for administrator rights when it needs them.
        opener.open_path(installer.to_string_lossy(), None::<&str>).map_err(err)
    } else if let Some(url) = runtime.download {
        opener.open_url(url, None::<&str>).map_err(err)
    } else {
        Err("No installer is available for this runtime.".into())
    }
}

/// Creates a desktop or Start menu shortcut that starts the game through Starex.
#[tauri::command]
pub fn create_shortcut(state: State<'_, AppState>, id: String, place: Place) -> CmdResult<String> {
    let game = state.library().get(&id).map_err(err)?.clone();
    shortcuts::create(&game, place).map(|p| p.display().to_string())
}

#[tauri::command]
pub fn launch_link(id: String) -> String {
    shortcuts::link(&id)
}

/// A game Starex was opened to start (from a shortcut or link), handed over once.
#[tauri::command]
pub fn take_launch_request(state: State<'_, AppState>) -> Option<String> {
    state.launch_request().take()
}

#[tauri::command]
pub async fn restore_saves(app: AppHandle, id: String, backup_id: String) -> CmdResult<()> {
    let state = app.state::<AppState>();
    if state.sessions().contains_key(&id) {
        return Err("Close the game before restoring its saves.".into());
    }
    let game = state.library().get(&id).map_err(err)?.clone();
    let target = save_folder(&game)?;
    let backups = state.paths.save_backups(&id);
    tauri::async_runtime::spawn_blocking(move || saves::restore(&backups, &backup_id, &target))
        .await
        .map_err(err)?
        .map(|_| ())
        .map_err(err)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RevealTarget {
    Folder,
    Executable,
    Saves,
}

#[tauri::command]
pub fn reveal(app: AppHandle, id: String, target: RevealTarget) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let game = state.library().get(&id).map_err(err)?.clone();
    let opener = app.opener();
    match target {
        RevealTarget::Folder => opener
            .open_path(game.install_dir.to_string_lossy(), None::<&str>)
            .map_err(err),
        RevealTarget::Executable => opener.reveal_item_in_dir(&game.executable).map_err(err),
        RevealTarget::Saves => opener
            .open_path(save_folder(&game)?.to_string_lossy(), None::<&str>)
            .map_err(err),
    }
}

#[tauri::command]
pub fn open_data_dir(app: AppHandle) -> CmdResult<()> {
    let root = app.state::<AppState>().paths.root.clone();
    app.opener()
        .open_path(root.to_string_lossy(), None::<&str>)
        .map_err(err)
}

/// Opens one of the few web pages the UI links to.
#[tauri::command]
pub fn open_link(app: AppHandle, url: String) -> CmdResult<()> {
    let allowed = [
        REPOSITORY,
        "https://www.steamgriddb.com/",
        "https://dev.twitch.tv/",
        "https://discord.com/developers/",
        CREATOR_X,
    ];
    if !allowed.iter().any(|prefix| url.starts_with(prefix)) {
        return Err("Starex only opens its own links.".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(err)
}
