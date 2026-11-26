//! Keeps the Ludusavi save database fresh and uses it to find games' save folders.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use starex_core::model::Game;
use starex_core::savedb::{self, Places, SaveDb};
use starex_core::saves;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// Refresh the database when the copy on disk is older than a week.
const MAX_AGE_SECS: u64 = 7 * 24 * 60 * 60;

/// The database, loading it from disk and downloading it when missing or stale. `None` when
/// turned off in Settings or not available (offline on first use).
pub async fn get(app: &AppHandle) -> Option<Arc<SaveDb>> {
    let state = app.state::<AppState>();
    if !state.settings().save_database {
        return None;
    }
    if let Some(db) = state.savedb().clone()
        && !db.is_stale(MAX_AGE_SECS)
    {
        return Some(db);
    }
    let path = state.paths.savedb.clone();
    let on_disk = tauri::async_runtime::spawn_blocking({
        let path = path.clone();
        move || SaveDb::load(&path)
    })
    .await
    .ok()
    .flatten();
    if let Some(db) = on_disk
        && !db.is_stale(MAX_AGE_SECS)
    {
        let db = Arc::new(db);
        *state.savedb() = Some(db.clone());
        return Some(db);
    }
    match download(&state.http, &path).await {
        Ok(db) => {
            let db = Arc::new(db);
            *state.savedb() = Some(db.clone());
            Some(db)
        }
        Err(e) => {
            log::warn!("save database not updated: {e}");
            // An old copy is still far better than none.
            let old = SaveDb::load(&path).map(Arc::new);
            *state.savedb() = old.clone();
            old
        }
    }
}

async fn download(http: &reqwest::Client, target: &Path) -> Result<SaveDb, String> {
    log::info!("downloading the save database");
    let bytes = http
        .get(savedb::MANIFEST_URL)
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    let target = target.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let db = SaveDb::from_manifest_for_this_os(bytes.as_ref()).map_err(|e| e.to_string())?;
        db.save(&target).map_err(|e| e.to_string())?;
        log::info!("save database ready: {} games", db.games.len());
        Ok(db)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The folders on this computer that `places` describes.
fn places(game: &Game) -> Places {
    let home = dirs::home_dir();
    Places {
        install_dir: game.install_dir.clone(),
        app_data: dirs::data_dir(),
        local_app_data: dirs::data_local_dir(),
        documents: dirs::document_dir(),
        public: std::env::var_os("PUBLIC").map(PathBuf::from),
        program_data: std::env::var_os("ProgramData").map(PathBuf::from),
        windows: std::env::var_os("SystemRoot").map(PathBuf::from),
        xdg_data: dirs::data_dir().filter(|_| cfg!(target_os = "linux")),
        xdg_config: dirs::config_dir().filter(|_| cfg!(target_os = "linux")),
        user_name: std::env::var("USERNAME").or_else(|_| std::env::var("USER")).ok(),
        home,
    }
}

/// Existing save folders the database knows for `game`, best first.
pub fn save_dirs(db: &SaveDb, game: &Game) -> Vec<PathBuf> {
    let places = places(game);
    let mut found: Vec<PathBuf> = db
        .lookup(&game.name, &game.install_dir)
        .iter()
        .filter_map(|template| savedb::expand(template, &places))
        .collect();
    found.dedup();
    found
}

/// Copies a game's save backups to the cloud folder from Settings, if one is set.
pub fn mirror_to_cloud(app: &AppHandle, game: &Game) {
    let state = app.state::<AppState>();
    let cloud = state.settings().cloud_backup_dir.trim().to_owned();
    if cloud.is_empty() {
        return;
    }
    let dest = Path::new(&cloud)
        .join("Starex saves")
        .join(saves::folder_name(&game.name));
    match saves::mirror(&state.paths.save_backups(&game.id), &dest) {
        Ok(0) => {}
        Ok(n) => log::info!("copied {n} save backups to the cloud folder"),
        Err(e) => log::warn!("could not copy save backups to the cloud folder: {e}"),
    }
}
