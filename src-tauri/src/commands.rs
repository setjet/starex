//! The commands the UI can call. Each one is a thin wrapper over `starex-core`; slow work runs on
//! a blocking thread so the window never stalls.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use starex_core::antivirus::{self, ScanReport, ScanStatus, ScanSummary};
use starex_core::inspect::{self, FolderInspection};
use starex_core::manifest::{self, Manifest, Progress};
use starex_core::model::{Artwork, ArtworkKind, Game, GamePatch, NewGame, Store, StoreRef};
use starex_core::runtimes::{self, Runtime};
use starex_core::saves::{self, BackupReason, SaveBackup, SaveLocation};
use starex_core::settings::Settings;
use starex_core::stores;
use starex_core::verify::{self, VerifyMode, VerifyReport};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::artwork;
use crate::firewall;
use crate::releases;
use crate::savedb;
use crate::sessions::{self, GAME_EVENT};
use crate::shortcuts::{self, Place};
use crate::state::{self, AppState};
use crate::steam;
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
    /// The signed-in user's name, for the launch animation's welcome.
    user_name: String,
    /// The number of a private test build; `None` for every normal build.
    test_build: Option<&'static str>,
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
        user_name: system::user_display_name(),
        test_build: starex_core::TEST_BUILD,
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
    let added = app.state::<AppState>().library().add(game).map_err(err)?;
    finish_adding(&app, added, detected, scan).await
}

/// Stores a new game's detected artwork and virus scan, then finds its saves and artwork in the
/// background.
async fn finish_adding(
    app: &AppHandle,
    added: Game,
    detected: Option<Artwork>,
    scan: Option<ScanSummary>,
) -> CmdResult<Game> {
    let state = app.state::<AppState>();
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
    // Find the save folder in the background from the save database. A launcher's game keeps its
    // saves to itself.
    if added.source.is_none() {
        let (app, id) = (app.clone(), added.id.clone());
        tauri::async_runtime::spawn(async move {
            detect_save_dir(&app, &id).await;
        });
    }
    system::refresh_tray(app);
    // A launcher's game uses the art found on this computer, else the default art.
    if added.source.is_none() && settings.auto_artwork && !settings.steamgriddb_key.trim().is_empty() {
        match fetch(app, &added.id, false).await {
            Ok(updated) => added = updated,
            Err(e) => log::info!("automatic artwork skipped: {e}"),
        }
    }
    Ok(added)
}

/// Games another launcher (Steam, Epic, GOG, Xbox) installed on this computer, from one launcher
/// or all of them.
#[tauri::command]
pub async fn scan_stores(app: AppHandle, store: Option<Store>) -> CmdResult<Vec<stores::FoundGame>> {
    let library = app.state::<AppState>().library().games().to_vec();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(store) = store
            && !stores::launcher_installed(store)
        {
            return Err(format!(
                "{} is not installed on this PC, so there are no games to import from it.",
                capitalized(store_name(store))
            ));
        }
        Ok(stores::scan(&library, store))
    })
    .await
    .map_err(|_| "Starex could not finish looking for games. Try again.".to_owned())?
}

/// One game chosen in the import list.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportGame {
    source: StoreRef,
    name: String,
    install_dir: PathBuf,
    executable: PathBuf,
    #[serde(default)]
    artwork: Artwork,
}

/// Adds games found by `scan_stores`. Games that fail are skipped and the rest are returned;
/// when none could be added, the first reason is the error.
#[tauri::command]
pub async fn import_games(app: AppHandle, games: Vec<ImportGame>) -> CmdResult<Vec<Game>> {
    let mut added = Vec::new();
    let mut first_problem = None;
    for game in games {
        match add_store_game(&app, game).await {
            Ok(game) => added.push(game),
            Err(e) => {
                log::warn!("could not import a game: {e}");
                first_problem.get_or_insert(e);
            }
        }
    }
    match first_problem {
        Some(problem) if added.is_empty() => Err(problem),
        _ => Ok(added),
    }
}

/// Adds one launcher game to the library, with a plain reason when it cannot be.
async fn add_store_game(app: &AppHandle, game: ImportGame) -> CmdResult<Game> {
    let state = app.state::<AppState>();
    let name = game.name.trim().to_owned();
    if !game.install_dir.is_dir() {
        return Err(format!(
            "{name}'s folder is gone. It may have been uninstalled or moved."
        ));
    }
    let key = stores::store_key(&game.source);
    if state
        .library()
        .games()
        .iter()
        .any(|g| g.source.as_ref().is_some_and(|s| stores::store_key(s) == key))
    {
        return Err(format!("{name} is already in your library."));
    }
    let new = NewGame {
        name: name.clone(),
        install_dir: game.install_dir,
        executable: game.executable,
        launch_args: String::new(),
        source: Some(game.source),
    };
    let added = state.library().add(new).map_err(|e| {
        let e = e.to_string();
        if e.contains("does not exist") {
            format!("{name}'s files could not be found. Check that it finished installing.")
        } else {
            format!("{name} could not be added: {e}")
        }
    })?;
    // Imported again by hand: automatic imports may bring it back after a later removal.
    forget_removal(&state, &key);
    finish_adding(app, added, Some(game.artwork), None).await
}

/// When Starex opens and the setting is on, adds games newly installed with Steam or the Epic
/// Games Launcher. Games the user removed from the library stay out.
#[tauri::command]
pub async fn auto_import_stores(app: AppHandle) -> CmdResult<Vec<Game>> {
    let state = app.state::<AppState>();
    let settings = state.settings().clone();
    if !settings.auto_import_stores {
        return Ok(Vec::new());
    }
    let library = state.library().games().to_vec();
    let removed: std::collections::HashSet<String> = settings.removed_store_games.iter().cloned().collect();
    let found = tauri::async_runtime::spawn_blocking(move || {
        [Store::Steam, Store::Epic]
            .into_iter()
            .filter(|store| stores::launcher_installed(*store))
            .flat_map(|store| stores::scan(&library, Some(store)))
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|_| "Starex could not finish looking for new games.".to_owned())?;
    let mut added = Vec::new();
    for game in found
        .into_iter()
        .filter(|g| !g.in_library && !removed.contains(&stores::store_key(&g.source)))
    {
        let import = ImportGame {
            source: game.source,
            name: game.name,
            install_dir: game.install_dir,
            executable: game.executable,
            artwork: game.artwork,
        };
        match add_store_game(&app, import).await {
            Ok(game) => added.push(game),
            Err(e) => log::warn!("automatic import skipped a game: {e}"),
        }
    }
    Ok(added)
}

/// Remembers that the user removed a launcher game, so automatic imports leave it out.
fn remember_removal(state: &AppState, key: String) {
    let mut settings = state.settings();
    if !settings.removed_store_games.contains(&key) {
        settings.removed_store_games.push(key);
        if let Err(e) = settings.save(&state.paths.settings) {
            log::warn!("could not save settings: {e}");
        }
    }
}

fn forget_removal(state: &AppState, key: &str) {
    let mut settings = state.settings();
    if settings.removed_store_games.iter().any(|k| k == key) {
        settings.removed_store_games.retain(|k| k != key);
        if let Err(e) = settings.save(&state.paths.settings) {
            log::warn!("could not save settings: {e}");
        }
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Games imported from a launcher that the launcher no longer has installed.
#[tauri::command]
pub async fn store_status(app: AppHandle) -> CmdResult<Vec<String>> {
    let library = app.state::<AppState>().library().games().to_vec();
    tauri::async_runtime::spawn_blocking(move || stores::not_installed(&library))
        .await
        .map_err(err)
}

/// Games from another launcher belong to that launcher: Starex starts them and tracks play time,
/// and never changes their files, saves or network access.
fn not_for_store_games(game: &Game) -> CmdResult<()> {
    match &game.source {
        Some(source) => Err(format!(
            "{} is managed by {}, so Starex leaves its files and saves alone.",
            game.name,
            store_name(source.store)
        )),
        None => Ok(()),
    }
}

fn store_name(store: Store) -> &'static str {
    match store {
        Store::Steam => "Steam",
        Store::Epic => "the Epic Games Launcher",
        Store::Gog => "GOG",
        Store::Xbox => "the Xbox app",
    }
}

/// The Steam account last signed in on this computer, to fill in the Steam ID.
#[tauri::command]
pub async fn steam_account() -> CmdResult<Option<stores::SteamAccount>> {
    tauri::async_runtime::spawn_blocking(stores::steam_account)
        .await
        .map_err(err)
}

fn steam_app(state: &AppState, id: &str) -> CmdResult<u32> {
    let game = state.library().get(id).map_err(err)?.clone();
    stores::steam_app_id(&game).ok_or_else(|| "This is not a Steam game.".to_owned())
}

/// Reads the play time Steam keeps for every Steam game in the library and stores it with each
/// game: all-time and two-week play time from the Steam Web API (with a key), and when each was
/// last played (from the API, else from the Steam client's own records). Whatever Steam does not
/// give, Starex's own count stands in for. Returns how many games changed.
#[tauri::command]
pub async fn sync_steam_playtime(app: AppHandle) -> CmdResult<usize> {
    let state = app.state::<AppState>();
    let steam_games: Vec<(String, u32)> = state
        .library()
        .games()
        .iter()
        .filter(|g| g.source.as_ref().is_some_and(|s| s.store == Store::Steam))
        .filter_map(|g| Some((g.id.clone(), stores::steam_app_id(g)?)))
        .collect();
    if steam_games.is_empty() {
        return Ok(0);
    }
    let settings = state.settings().clone();
    let steam_id = if settings.steam_id.trim().is_empty() {
        tauri::async_runtime::spawn_blocking(stores::steam_account)
            .await
            .ok()
            .flatten()
            .map(|a| a.steam_id)
            .unwrap_or_default()
    } else {
        settings.steam_id.clone()
    };
    let online = if settings.steam_api_key.trim().is_empty() || steam_id.is_empty() {
        None
    } else {
        match steam::playtime(&state.http, &settings.steam_api_key, &steam_id).await {
            Ok(played) => Some(played),
            Err(e) => {
                log::info!("Steam play time from the Web API skipped: {e}");
                None
            }
        }
    };
    let local = {
        let id = steam_id.clone();
        tauri::async_runtime::spawn_blocking(move || stores::steam_last_played(&id))
            .await
            .unwrap_or_default()
    };

    let mut changed = 0;
    for (id, app_id) in steam_games {
        let steam = online.as_ref().and_then(|p| p.get(&app_id));
        let last_played = steam
            .and_then(|p| p.last_played)
            .or_else(|| local.get(&app_id).copied());
        let known = starex_core::model::StorePlaytime {
            total_secs: steam.map(|p| p.total_secs),
            recent_secs: steam.map(|p| p.recent_secs),
            last_played,
            updated_at: starex_core::now_secs(),
        };
        if known.total_secs.is_none() && known.last_played.is_none() {
            continue;
        }
        let same = state.library().get(&id).ok().is_some_and(|g| {
            g.store_playtime.as_ref().is_some_and(|p| {
                (p.total_secs, p.recent_secs, p.last_played) == (known.total_secs, known.recent_secs, known.last_played)
            })
        });
        if same {
            continue;
        }
        match state.library().update(&id, |g| g.store_playtime = Some(known.clone())) {
            Ok(updated) => {
                let _ = app.emit(GAME_EVENT, &updated);
                changed += 1;
            }
            Err(e) => log::warn!("could not save Steam play time: {e}"),
        }
    }
    Ok(changed)
}

/// The latest news posts for a Steam game.
#[tauri::command]
pub async fn steam_news(app: AppHandle, id: String) -> CmdResult<Vec<steam::NewsItem>> {
    let state = app.state::<AppState>();
    let app_id = steam_app(&state, &id)?;
    steam::news(&state.http, app_id).await
}

/// The user's achievements in a Steam game.
#[tauri::command]
pub async fn steam_achievements(app: AppHandle, id: String) -> CmdResult<steam::Achievements> {
    let state = app.state::<AppState>();
    let app_id = steam_app(&state, &id)?;
    let (key, configured) = {
        let s = state.settings();
        (s.steam_api_key.clone(), s.steam_id.clone())
    };
    let steam_id = if configured.trim().is_empty() {
        tauri::async_runtime::spawn_blocking(stores::steam_account)
            .await
            .map_err(err)?
            .map(|a| a.steam_id)
            .unwrap_or_default()
    } else {
        configured
    };
    steam::achievements(&state.http, &key, &steam_id, app_id).await
}

/// Opens one of a Steam game's news posts on the Steam store.
#[tauri::command]
pub fn open_steam_news(app: AppHandle, id: String, gid: String) -> CmdResult<()> {
    let app_id = steam_app(&app.state::<AppState>(), &id)?;
    if gid.is_empty() || !gid.chars().all(|c| c.is_ascii_digit()) {
        return Err("That news post could not be opened.".into());
    }
    app.opener()
        .open_url(
            format!("https://store.steampowered.com/news/app/{app_id}/view/{gid}"),
            None::<&str>,
        )
        .map_err(err)
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
    let source = state.library().get(&id).ok().and_then(|g| g.source.clone());
    state.library().remove(&id).map_err(err)?;
    if let Some(source) = source {
        remember_removal(&state, stores::store_key(&source));
    }
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

/// Runs on a blocking thread: a pre-launch command or a check before launch can take a while.
#[tauri::command]
pub async fn launch_game(app: AppHandle, id: String) -> CmdResult<u64> {
    tauri::async_runtime::spawn_blocking(move || sessions::launch(&app, &id))
        .await
        .map_err(err)?
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
    settings.discord_app_id = settings.discord_app_id.trim().to_owned();
    settings.global_shortcut = settings.global_shortcut.trim().to_owned();
    settings.steam_api_key = settings.steam_api_key.trim().to_owned();
    settings.steam_id = settings.steam_id.trim().to_owned();
    settings.removed_store_games = previous.removed_store_games.clone();

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
    not_for_store_games(&game)?;
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
    not_for_store_games(&game)?;
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
    not_for_store_games(&game)?;
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
    not_for_store_games(&game)?;
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
        "https://discord.com/developers/",
        "https://steamcommunity.com/dev/apikey",
        CREATOR_X,
    ];
    if !allowed.iter().any(|prefix| url.starts_with(prefix)) {
        return Err("Starex only opens its own links.".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(err)
}
