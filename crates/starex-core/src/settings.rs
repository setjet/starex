use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::util::{read_json, write_json_atomic};

/// What the launcher window does once a game has started.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum LaunchBehavior {
    KeepOpen,
    #[default]
    Minimize,
    /// Hide the window until the game exits.
    Hide,
}

/// What fills the window behind the library and game pages.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Backdrop {
    #[default]
    None,
    /// A soft, slowly moving light field.
    Aurora,
    /// The selected game's art, blurred.
    Game,
    /// An image the user chose.
    Custom,
}

/// The colour of Play and other highlights.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Accent {
    #[default]
    Blue,
    Violet,
    Pink,
    Green,
    Orange,
    White,
}

/// Light or dark interface, or whichever the system uses.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    System,
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum LibrarySort {
    #[default]
    Recent,
    Name,
    Playtime,
}

/// User preferences, stored in `settings.json`. Unknown or missing fields fall back to defaults,
/// so older and newer versions of Starex can share the file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub launch_behavior: LaunchBehavior,
    /// Bring the launcher back when the game closes.
    pub restore_on_exit: bool,
    /// Run a quick verification before every launch and stop if required files are missing.
    pub verify_before_launch: bool,
    /// Download a new version in the background as soon as it is available; it installs when the
    /// user restarts Starex from the update prompt.
    pub auto_download_updates: bool,
    pub library_sort: LibrarySort,
    /// Fetch artwork from SteamGridDB for new games. Needs `steamgriddb_key`.
    pub auto_artwork: bool,
    pub steamgriddb_key: String,
    pub reduce_motion: bool,
    /// Scan a game's code files for viruses when it is added.
    pub scan_new_games: bool,
    /// Linux and macOS: the Wine program used for Windows games. Empty means find it.
    pub wine_path: String,
    /// Look up save folders in the Ludusavi save database (downloaded from GitHub).
    pub save_database: bool,
    /// A synced folder (OneDrive, Dropbox, Syncthing…) that receives a copy of every save backup.
    pub cloud_backup_dir: String,
    /// Show the game being played on the user's Discord profile.
    pub discord_presence: bool,
    /// The Discord application Starex reports as. Empty uses the one built into Starex.
    pub discord_app_id: String,
    /// The close button keeps Starex running in the system tray.
    pub close_to_tray: bool,
    /// Start Starex, in the tray, when the user signs in.
    pub start_with_system: bool,
    /// A system-wide key combination that brings Starex up, e.g. "Alt+Shift+S". Empty turns it off.
    pub global_shortcut: String,
    pub backdrop: Backdrop,
    /// The custom backdrop image, copied into Starex's data folder.
    pub backdrop_image: String,
    /// How strongly the backdrop shows, 0 to 100.
    pub backdrop_strength: u8,
    pub accent: Accent,
    /// Light, dark, or following the system.
    pub theme: Theme,
    /// Play the star animation when Starex opens.
    pub launch_animation: bool,
    /// The welcome message has been shown once.
    pub welcome_seen: bool,
    /// A Steam Web API key, for achievements in Steam games.
    pub steam_api_key: String,
    /// The user's SteamID64. Empty uses the account last signed in to the Steam client.
    pub steam_id: String,
    /// When Starex opens, add games newly installed with Steam or the Epic Games Launcher.
    pub auto_import_stores: bool,
    /// Launcher games ("steam:570") the user removed from the library, which automatic imports
    /// leave out. Kept by the app, not edited in Settings.
    pub removed_store_games: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            launch_behavior: LaunchBehavior::default(),
            restore_on_exit: true,
            verify_before_launch: false,
            auto_download_updates: true,
            library_sort: LibrarySort::default(),
            auto_artwork: true,
            steamgriddb_key: String::new(),
            reduce_motion: false,
            scan_new_games: true,
            wine_path: String::new(),
            save_database: true,
            cloud_backup_dir: String::new(),
            discord_presence: true,
            discord_app_id: String::new(),
            close_to_tray: false,
            start_with_system: false,
            // A test build leaves the shortcut to the real Starex unless one is set in its Settings.
            global_shortcut: if crate::TEST_BUILD.is_some() {
                String::new()
            } else {
                "Alt+Shift+S".to_owned()
            },
            backdrop: Backdrop::None,
            backdrop_image: String::new(),
            backdrop_strength: 60,
            accent: Accent::Blue,
            theme: Theme::Dark,
            launch_animation: true,
            welcome_seen: false,
            steam_api_key: String::new(),
            steam_id: String::new(),
            auto_import_stores: false,
            removed_store_games: Vec::new(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        match read_json::<Settings>(path) {
            Ok(Some(settings)) => settings,
            Ok(None) => Self::default(),
            Err(err) => {
                log::warn!("using default settings: {err}");
                Self::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        write_json_atomic(path, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_files_keep_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{ "launchBehavior": "hide", "futureField": 1 }"#).unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.launch_behavior, LaunchBehavior::Hide);
        assert!(s.restore_on_exit);
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
    }
}
