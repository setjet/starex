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
use starex_core::saves::{self, BackupReason, SaveBackup, SaveLocation};
use starex_core::settings::Settings;
use starex_core::verify::{self, VerifyMode, VerifyReport};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::artwork;
use crate::sessions::{self, GAME_EVENT};
use crate::state::{self, AppState};

type CmdResult<T> = Result<T, String>;

const TASK_EVENT: &str = "starex://task-progress";
const REPOSITORY: &str = "https://github.com/setjet/starex";

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
    if settings.auto_artwork && !settings.steamgriddb_key.trim().is_empty() {
        match fetch(&app, &added.id, false).await {
            Ok(updated) => added = updated,
            Err(e) => log::info!("automatic artwork skipped: {e}"),
        }
    }
    Ok(added)
}

#[tauri::command]
pub fn update_game(state: State<'_, AppState>, id: String, patch: GamePatch) -> CmdResult<Game> {
    state.library().patch(&id, patch).map_err(err)
}

#[tauri::command]
pub fn remove_game(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    if state.sessions().contains_key(&id) {
        return Err("Close the game before removing it from your library.".into());
    }
    state.library().remove(&id).map_err(err)?;
    artwork::remove_all(&state.paths, &id);
    manifest::delete(&state.paths.baseline(&id)).map_err(err)?;
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

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    let mut settings = settings;
    settings.steamgriddb_key = settings.steamgriddb_key.trim().to_owned();
    settings.save(&state.paths.settings).map_err(err)?;
    *state.settings() = settings.clone();
    Ok(settings)
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

#[tauri::command]
pub async fn suggest_save_locations(app: AppHandle, id: String) -> CmdResult<Vec<SaveLocation>> {
    let game = app.state::<AppState>().library().get(&id).map_err(err)?.clone();
    tauri::async_runtime::spawn_blocking(move || {
        saves::suggest_locations(&game.name, &game.install_dir, &state::save_roots())
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
    tauri::async_runtime::spawn_blocking(move || saves::backup(&source, &target, BackupReason::Manual))
        .await
        .map_err(err)?
        .map_err(err)
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
    let allowed = [REPOSITORY, "https://www.steamgriddb.com/"];
    if !allowed.iter().any(|prefix| url.starts_with(prefix)) {
        return Err("Starex only opens its own links.".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(err)
}
