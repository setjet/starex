//! Where games keep their saves, from the community-maintained
//! [Ludusavi manifest](https://github.com/mtkennerly/ludusavi-manifest) (10,000+ games).
//!
//! The manifest is a large YAML file. Starex downloads it, keeps only the save locations that
//! apply to this operating system in a small JSON index, and looks games up by name or by the
//! name of their install folder.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::util::{normalize, now_secs, read_json, write_json_atomic};
use crate::{Error, Result};

pub const MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml";

/// The index Starex keeps on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDb {
    pub fetched_at: u64,
    /// Normalised game name -> save path templates for this system.
    pub games: HashMap<String, Vec<String>>,
    /// Normalised install folder name -> normalised game name.
    pub folders: HashMap<String, String>,
}

#[derive(Deserialize)]
struct RawGame {
    #[serde(default)]
    files: Option<HashMap<String, Option<RawFile>>>,
    #[serde(default, rename = "installDir")]
    install_dir: Option<HashMap<String, serde::de::IgnoredAny>>,
}

#[derive(Deserialize, Default)]
struct RawFile {
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    when: Option<Vec<RawWhen>>,
}

#[derive(Deserialize)]
struct RawWhen {
    #[serde(default)]
    os: Option<String>,
}

fn this_os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "mac",
        other => other,
    }
}

impl SaveDb {
    /// Builds the index from the Ludusavi manifest, keeping save locations for `os`
    /// (`windows`, `linux` or `mac`).
    pub fn from_manifest(reader: impl Read, os: &str) -> Result<Self> {
        let raw: HashMap<String, RawGame> = serde_yaml_ng::from_reader(reader)
            .map_err(|e| Error::invalid(format!("The save database could not be read: {e}")))?;
        let mut db = SaveDb {
            fetched_at: now_secs(),
            ..Default::default()
        };
        for (name, game) in raw {
            let key = normalize(&name);
            if key.is_empty() {
                continue;
            }
            let mut paths: Vec<String> = game
                .files
                .unwrap_or_default()
                .into_iter()
                .filter(|(_, file)| {
                    let file = file.as_ref();
                    let is_save = file
                        .and_then(|f| f.tags.as_ref())
                        .is_some_and(|tags| tags.iter().any(|t| t == "save"));
                    let for_os = file.and_then(|f| f.when.as_ref()).is_none_or(|when| {
                        when.is_empty() || when.iter().any(|w| w.os.as_deref().is_none_or(|o| o == os))
                    });
                    is_save && for_os
                })
                .map(|(path, _)| path)
                .collect();
            if paths.is_empty() {
                continue;
            }
            paths.sort();
            for folder in game.install_dir.unwrap_or_default().into_keys() {
                let folder_key = normalize(&folder);
                if !folder_key.is_empty() {
                    db.folders.entry(folder_key).or_insert_with(|| key.clone());
                }
            }
            db.games.insert(key, paths);
        }
        Ok(db)
    }

    pub fn from_manifest_for_this_os(reader: impl Read) -> Result<Self> {
        Self::from_manifest(reader, this_os())
    }

    pub fn load(path: &Path) -> Option<Self> {
        read_json(path).ok().flatten()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        write_json_atomic(path, self)
    }

    /// Is this index older than `max_age_secs`?
    pub fn is_stale(&self, max_age_secs: u64) -> bool {
        now_secs().saturating_sub(self.fetched_at) > max_age_secs
    }

    /// The save path templates for a game, matched by its name or its install folder's name.
    pub fn lookup(&self, game_name: &str, install_dir: &Path) -> &[String] {
        let by_name = self.games.get(&normalize(game_name));
        let by_folder = || {
            let folder = install_dir.file_name()?.to_string_lossy().into_owned();
            let key = self.folders.get(&normalize(&folder))?;
            self.games.get(key)
        };
        by_name.or_else(by_folder).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// The real folders behind the manifest's placeholders, for one game on this computer.
#[derive(Debug, Clone, Default)]
pub struct Places {
    pub install_dir: PathBuf,
    pub home: Option<PathBuf>,
    pub app_data: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub documents: Option<PathBuf>,
    pub public: Option<PathBuf>,
    pub program_data: Option<PathBuf>,
    pub windows: Option<PathBuf>,
    pub xdg_data: Option<PathBuf>,
    pub xdg_config: Option<PathBuf>,
    pub user_name: Option<String>,
}

/// Turns a manifest path template into the existing folder that holds the saves.
///
/// Wildcards are resolved by stopping at the last folder before them; a path to a single file
/// becomes its folder. Folders too broad to back up as a whole (the game folder itself, home,
/// Documents, AppData) are rejected.
pub fn expand(template: &str, places: &Places) -> Option<PathBuf> {
    let p = |path: &Option<PathBuf>| path.as_ref().map(|p| p.to_string_lossy().replace('\\', "/"));
    let local_low = places.home.as_ref().map(|h| h.join("AppData").join("LocalLow"));
    let game_folder = places.install_dir.file_name().map(|n| n.to_string_lossy().into_owned());
    let replacements: [(&str, Option<String>); 15] = [
        ("<base>", p(&Some(places.install_dir.clone()))),
        ("<root>", p(&places.install_dir.parent().map(Path::to_path_buf))),
        ("<game>", game_folder),
        ("<home>", p(&places.home)),
        ("<winAppData>", p(&places.app_data)),
        ("<winLocalAppData>", p(&places.local_app_data)),
        ("<winLocalAppDataLow>", p(&local_low)),
        ("<winDocuments>", p(&places.documents)),
        ("<winPublic>", p(&places.public)),
        ("<winProgramData>", p(&places.program_data)),
        ("<winDir>", p(&places.windows)),
        ("<xdgData>", p(&places.xdg_data)),
        ("<xdgConfig>", p(&places.xdg_config)),
        ("<osUserName>", places.user_name.clone()),
        ("<storeUserId>", Some("*".into())),
    ];
    let mut path = template.to_owned();
    for (placeholder, value) in &replacements {
        if path.contains(placeholder) {
            path = path.replace(placeholder, value.as_deref()?);
        }
    }
    // Any placeholder Starex does not know (store ids, registry) acts as a wildcard.
    let mut cleaned = String::with_capacity(path.len());
    let mut in_placeholder = false;
    for c in path.chars() {
        match c {
            '<' => in_placeholder = true,
            '>' if in_placeholder => {
                in_placeholder = false;
                cleaned.push('*');
            }
            _ if !in_placeholder => cleaned.push(c),
            _ => {}
        }
    }

    let mut folder = PathBuf::new();
    let segments: Vec<&str> = cleaned.split('/').collect();
    let mut stopped = false;
    for (i, segment) in segments.iter().enumerate() {
        if segment.contains(['*', '?', '[', '{']) {
            stopped = true;
            break;
        }
        if i == 0 && segment.ends_with(':') {
            folder.push(format!("{segment}\\"));
        } else if i == 0 && segment.is_empty() {
            folder.push("/");
        } else if !segment.is_empty() {
            folder.push(segment);
        }
    }
    if !stopped && folder.is_file() {
        folder.pop();
    }
    if !folder.is_dir() {
        return None;
    }
    let too_broad = [
        Some(places.install_dir.clone()),
        places.install_dir.parent().map(Path::to_path_buf),
        places.home.clone(),
        places.app_data.clone(),
        places.local_app_data.clone(),
        local_low,
        places.documents.clone(),
        places.public.clone(),
        places.program_data.clone(),
        places.xdg_data.clone(),
        places.xdg_config.clone(),
    ];
    let key = |p: &Path| {
        p.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase()
    };
    let same = |a: &Path, b: &Path| key(a) == key(b);
    if too_broad.iter().flatten().any(|broad| same(&folder, broad)) {
        return None;
    }
    Some(folder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const MANIFEST: &str = r#"
"Star Runner":
  files:
    "<winDocuments>/My Games/Star Runner/Saves":
      tags:
        - save
      when:
        - os: windows
    "<base>/settings.ini":
      tags:
        - config
    "<xdgData>/star-runner/saves":
      tags:
        - save
      when:
        - os: linux
  installDir:
    StarRunnerGame: {}
"No Saves Game":
  steam:
    id: 1
"#;

    #[test]
    fn indexes_save_locations_for_one_system() {
        let windows = SaveDb::from_manifest(MANIFEST.as_bytes(), "windows").unwrap();
        assert_eq!(windows.games.len(), 1, "games without saves are left out");
        let paths = windows.lookup("STAR runner", Path::new("C:/x"));
        assert_eq!(paths, ["<winDocuments>/My Games/Star Runner/Saves"]);
        // Found through the install folder's name as well.
        assert_eq!(
            windows
                .lookup("Unknown title", Path::new("D:/Games/StarRunnerGame"))
                .len(),
            1
        );

        let linux = SaveDb::from_manifest(MANIFEST.as_bytes(), "linux").unwrap();
        assert_eq!(
            linux.lookup("Star Runner", Path::new("/g")),
            ["<xdgData>/star-runner/saves"]
        );
    }

    #[test]
    fn expands_templates_to_existing_folders() {
        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().join("Documents");
        let game = dir.path().join("Games/StarRunner");
        fs::create_dir_all(docs.join("My Games/Star Runner/Saves/profile1")).unwrap();
        fs::create_dir_all(game.join("save")).unwrap();
        let places = Places {
            install_dir: game.clone(),
            documents: Some(docs.clone()),
            ..Default::default()
        };
        assert_eq!(
            expand("<winDocuments>/My Games/Star Runner/Saves/*", &places),
            Some(docs.join("My Games/Star Runner/Saves"))
        );
        assert_eq!(expand("<base>/save/*.sav", &places), Some(game.join("save")));
        assert_eq!(
            expand("<base>/*.sav", &places),
            None,
            "the whole game folder is too broad"
        );
        assert_eq!(expand("<winDocuments>/Missing/Saves", &places), None);
        assert_eq!(expand("<winAppData>/x", &places), None, "unknown places do not resolve");
    }
}
