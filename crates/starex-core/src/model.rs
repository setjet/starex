use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::antivirus::ScanSummary;
use crate::verify::{CheckStatus, VerifyMode};

/// A game in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    pub name: String,
    /// The folder that holds the game's files. Verification covers everything under it.
    pub install_dir: PathBuf,
    /// Absolute path of the executable Starex starts.
    pub executable: PathBuf,
    #[serde(default)]
    pub launch_args: String,
    /// Folder the game starts in. `None` means the executable's own folder.
    #[serde(default)]
    pub working_dir: Option<PathBuf>,
    #[serde(default)]
    pub env: Vec<EnvVar>,
    #[serde(default)]
    pub artwork: Artwork,
    #[serde(default)]
    pub favorite: bool,
    pub added_at: u64,
    #[serde(default)]
    pub last_played: Option<u64>,
    #[serde(default)]
    pub playtime_secs: u64,
    #[serde(default)]
    pub sessions: u32,
    #[serde(default)]
    pub last_verification: Option<VerificationSummary>,
    /// Where the game keeps its save files, if the user has set or confirmed it.
    #[serde(default)]
    pub save_dir: Option<PathBuf>,
    /// Back up `save_dir` before the first launch and after every session.
    #[serde(default = "yes")]
    pub backup_saves: bool,
    #[serde(default)]
    pub last_scan: Option<ScanSummary>,
    /// The user's own labels, e.g. "Ported" or "Co-op".
    #[serde(default)]
    pub tags: Vec<String>,
    /// Left out of the library unless the Hidden filter is on.
    #[serde(default)]
    pub hidden: bool,
    /// The user has finished the game.
    #[serde(default)]
    pub completed: bool,
    /// Past play sessions, oldest first, at most [`MAX_HISTORY`].
    #[serde(default)]
    pub history: Vec<PlaySession>,
    /// Alternative ways to start the game (e.g. DX11, DX12, modded).
    #[serde(default)]
    pub profiles: Vec<LaunchProfile>,
    /// The profile Play uses; `None` is the game's own launch settings.
    #[serde(default)]
    pub active_profile: Option<String>,
    /// Show this game on the user's Discord profile while it runs.
    #[serde(default = "yes")]
    pub show_on_discord: bool,
    /// A firewall rule blocks the game's programs from going online.
    #[serde(default)]
    pub offline: bool,
    /// The store the game was imported from (Steam, Epic, GOG, Xbox), if any.
    #[serde(default)]
    pub source: Option<StoreRef>,
    /// Play time the game's launcher keeps (Steam), shown in place of Starex's own where known.
    #[serde(default)]
    pub store_playtime: Option<StorePlaytime>,
    /// Windows: start the game as administrator (Windows asks first).
    #[serde(default)]
    pub run_as_admin: bool,
    /// A command run before the game starts, e.g. a mod manager or a script. Empty runs nothing.
    #[serde(default)]
    pub pre_launch: String,
}

/// Play time a launcher keeps for one of its games. Each value is `None` when the launcher does
/// not say; Starex's own count stands in for it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StorePlaytime {
    /// All-time play time, in seconds.
    pub total_secs: Option<u64>,
    /// Play time over the last two weeks, in seconds.
    pub recent_secs: Option<u64>,
    /// When the game was last played, Unix seconds.
    pub last_played: Option<u64>,
    /// When Starex last read these, Unix seconds.
    pub updated_at: u64,
}

/// A launcher that installs games.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Store {
    Steam,
    Epic,
    Gog,
    Xbox,
}

/// Where a game came from: the store and the game's id in that store (a Steam app id, an Epic
/// app name, a GOG product id, an Xbox package folder).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoreRef {
    pub store: Store,
    pub id: String,
}

/// How many past sessions a game keeps.
pub const MAX_HISTORY: usize = 500;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlaySession {
    pub start: u64,
    pub end: u64,
}

/// A named variation of how a game starts. Empty fields fall back to the game's own settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LaunchProfile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub launch_args: String,
    #[serde(default)]
    pub executable: Option<PathBuf>,
    #[serde(default)]
    pub env: Vec<EnvVar>,
}

impl Game {
    /// This game as the given launch profile starts it: the profile's options, executable and
    /// environment replace the game's own where set.
    pub fn with_profile(&self, profile_id: Option<&str>) -> Game {
        let mut game = self.clone();
        let Some(profile) = profile_id.and_then(|id| self.profiles.iter().find(|p| p.id == id)) else {
            return game;
        };
        game.launch_args = profile.launch_args.clone();
        if let Some(exe) = &profile.executable {
            game.executable = exe.clone();
        }
        for var in &profile.env {
            game.env.retain(|v| v.key != var.key);
            game.env.push(var.clone());
        }
        game
    }
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

/// Paths of the artwork Starex keeps for a game, in its own data folder.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artwork {
    /// Wide hero image shown behind the game's title (ideally 1920x620 or larger).
    pub banner: Option<PathBuf>,
    /// Portrait cover shown in the library grid (ideally 600x900).
    pub cover: Option<PathBuf>,
    /// Transparent logo that replaces the title text on the banner.
    pub logo: Option<PathBuf>,
    /// Square icon shown in the sidebar.
    pub icon: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ArtworkKind {
    Banner,
    Cover,
    Logo,
    Icon,
}

impl ArtworkKind {
    pub const ALL: [ArtworkKind; 4] = [Self::Banner, Self::Cover, Self::Logo, Self::Icon];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Banner => "banner",
            Self::Cover => "cover",
            Self::Logo => "logo",
            Self::Icon => "icon",
        }
    }
}

impl Artwork {
    pub fn get(&self, kind: ArtworkKind) -> Option<&PathBuf> {
        match kind {
            ArtworkKind::Banner => self.banner.as_ref(),
            ArtworkKind::Cover => self.cover.as_ref(),
            ArtworkKind::Logo => self.logo.as_ref(),
            ArtworkKind::Icon => self.icon.as_ref(),
        }
    }

    pub fn set(&mut self, kind: ArtworkKind, path: Option<PathBuf>) {
        let slot = match kind {
            ArtworkKind::Banner => &mut self.banner,
            ArtworkKind::Cover => &mut self.cover,
            ArtworkKind::Logo => &mut self.logo,
            ArtworkKind::Icon => &mut self.icon,
        };
        *slot = path;
    }
}

/// The outcome of the most recent verification, kept with the game.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationSummary {
    pub at: u64,
    pub mode: VerifyMode,
    pub status: CheckStatus,
    pub files_checked: u64,
    pub missing: u64,
    pub modified: u64,
    pub duration_ms: u64,
}

/// What the user confirms in the Add game dialog.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewGame {
    pub name: String,
    pub install_dir: PathBuf,
    pub executable: PathBuf,
    #[serde(default)]
    pub launch_args: String,
    /// Set when the game is imported from another launcher.
    #[serde(default)]
    pub source: Option<StoreRef>,
}

/// Fields the user can change in a game's properties. `None` leaves a field as it is.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GamePatch {
    pub name: Option<String>,
    pub executable: Option<PathBuf>,
    pub launch_args: Option<String>,
    /// `Some(None)` clears the working folder.
    #[serde(default, with = "double_option")]
    pub working_dir: Option<Option<PathBuf>>,
    pub env: Option<Vec<EnvVar>>,
    pub favorite: Option<bool>,
    /// `Some(None)` forgets the save folder.
    #[serde(default, with = "double_option")]
    pub save_dir: Option<Option<PathBuf>>,
    pub backup_saves: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub hidden: Option<bool>,
    pub completed: Option<bool>,
    pub profiles: Option<Vec<LaunchProfile>>,
    /// `Some(None)` returns to the game's own launch settings.
    #[serde(default, with = "double_option")]
    pub active_profile: Option<Option<String>>,
    pub show_on_discord: Option<bool>,
    pub run_as_admin: Option<bool>,
    pub pre_launch: Option<String>,
}

mod double_option {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        Option::<T>::deserialize(de).map(Some)
    }
}
