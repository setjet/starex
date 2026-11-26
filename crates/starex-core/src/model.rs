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
