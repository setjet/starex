use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use starex_core::library::Library;
use starex_core::settings::Settings;

/// Where Starex keeps its data: `%APPDATA%\Starex` on Windows.
pub struct Paths {
    pub root: PathBuf,
    pub library: PathBuf,
    pub settings: PathBuf,
    pub baselines: PathBuf,
    pub artwork: PathBuf,
}

impl Paths {
    fn new() -> Self {
        let root = std::env::var_os("STAREX_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| dirs::data_dir().map(|d| d.join("Starex")))
            .unwrap_or_else(|| PathBuf::from("Starex"));
        Self {
            library: root.join("library.json"),
            settings: root.join("settings.json"),
            baselines: root.join("baselines"),
            artwork: root.join("artwork"),
            root,
        }
    }

    pub fn baseline(&self, game_id: &str) -> PathBuf {
        self.baselines.join(format!("{game_id}.json"))
    }

    pub fn artwork_dir(&self, game_id: &str) -> PathBuf {
        self.artwork.join(game_id)
    }

    /// Where a game's save backups are kept. They survive removing the game from the library.
    pub fn save_backups(&self, game_id: &str) -> PathBuf {
        self.root.join("saves").join(game_id)
    }
}

/// Places games commonly keep their saves on this system, for save folder suggestions.
pub fn save_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(docs) = dirs::document_dir() {
        roots.push(docs.join("My Games"));
        roots.push(docs);
    }
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Saved Games"));
        roots.push(home.join("AppData").join("LocalLow"));
        roots.push(home.join(".local").join("share"));
    }
    roots.extend(
        [dirs::data_dir(), dirs::data_local_dir(), dirs::config_dir()]
            .into_iter()
            .flatten(),
    );
    roots.sort();
    roots.dedup();
    roots.retain(|r| r.is_dir());
    roots
}

/// A game that is running, as far as Starex can tell.
pub struct Session {
    pub started_at: u64,
    pub stop: Arc<AtomicBool>,
}

pub struct AppState {
    pub paths: Paths,
    pub library: Mutex<Library>,
    pub settings: Mutex<Settings>,
    pub sessions: Mutex<HashMap<String, Session>>,
    /// Cancellation flags of running verifications and baselines, by game id.
    pub tasks: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub http: reqwest::Client,
}

impl AppState {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let paths = Paths::new();
        std::fs::create_dir_all(&paths.artwork)?;
        std::fs::create_dir_all(&paths.baselines)?;
        let library = Library::open(&paths.library)?;
        let settings = Settings::load(&paths.settings);
        let http = reqwest::Client::builder()
            .user_agent(concat!("Starex/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()?;
        log::info!("data folder: {}", paths.root.display());
        Ok(Self {
            paths,
            library: Mutex::new(library),
            settings: Mutex::new(settings),
            sessions: Mutex::new(HashMap::new()),
            tasks: Mutex::new(HashMap::new()),
            http,
        })
    }

    pub fn library(&self) -> MutexGuard<'_, Library> {
        self.library.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn settings(&self) -> MutexGuard<'_, Settings> {
        self.settings.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn sessions(&self) -> MutexGuard<'_, HashMap<String, Session>> {
        self.sessions.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn tasks(&self) -> MutexGuard<'_, HashMap<String, Arc<AtomicBool>>> {
        self.tasks.lock().unwrap_or_else(|e| e.into_inner())
    }
}
