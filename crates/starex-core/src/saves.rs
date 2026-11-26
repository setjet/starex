//! Game saves: finding where a game keeps them, backing them up and restoring them.
//!
//! Backups live in Starex's data folder, one folder per backup:
//!
//! ```text
//! saves/<game-id>/<backup-id>/files/...        a copy of the save folder
//! saves/<game-id>/<backup-id>/backup.json      what, when and why
//! ```
//!
//! Restoring never loses data: the current saves are backed up first.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::util::{normalize, now_secs, read_json, words, write_json_atomic};
use crate::{Error, Result};

/// How many backups Starex keeps per game. The oldest go first.
pub const KEEP_BACKUPS: usize = 20;
const META: &str = "backup.json";
const FILES: &str = "files";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BackupReason {
    BeforeLaunch,
    AfterSession,
    Manual,
    BeforeRestore,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SaveBackup {
    pub id: String,
    pub at: u64,
    pub reason: BackupReason,
    pub files: u64,
    pub bytes: u64,
    /// Changes when any file's path, size or modification time changes.
    pub fingerprint: String,
}

/// A folder that probably holds a game's saves.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SaveLocation {
    pub path: PathBuf,
    pub files: u64,
    pub modified: Option<u64>,
}

/// Folder names that games use for saves inside their own folder.
const IN_GAME_NAMES: &[&str] = &[
    "save",
    "saves",
    "savegame",
    "savegames",
    "savedata",
    "saved",
    "profiles",
    "userdata",
];

/// Finds folders that look like `game_name`'s saves.
///
/// `roots` are the places games commonly save to on this system (Documents, Saved Games,
/// AppData and so on); the caller supplies them so this stays testable.
pub fn suggest_locations(game_name: &str, install_dir: &Path, roots: &[PathBuf]) -> Vec<SaveLocation> {
    let key = normalize(game_name);
    let name_words: Vec<String> = words(game_name).into_iter().filter(|w| w.len() > 2).collect();
    let matches = |dir_name: &str| {
        let dir_key = normalize(dir_name);
        if dir_key.len() < 3 || key.len() < 3 {
            return false;
        }
        if dir_key == key
            || (key.len() >= 5 && dir_key.contains(&key))
            || (dir_key.len() >= 5 && key.contains(&dir_key))
        {
            return true;
        }
        let dir_words = words(dir_name);
        let shared = name_words.iter().filter(|w| dir_words.contains(w)).count();
        shared >= 2 || (shared == 1 && name_words.len() == 1)
    };

    let mut found: Vec<PathBuf> = Vec::new();
    // Up to two levels below each root: "Documents/My Games/<game>", "LocalLow/<studio>/<game>".
    for root in roots {
        for entry in WalkDir::new(root).min_depth(1).max_depth(2).into_iter().flatten() {
            if entry.file_type().is_dir() && matches(&entry.file_name().to_string_lossy()) {
                found.push(entry.into_path());
            }
        }
    }
    // Save folders inside the game itself, e.g. "Saved/SaveGames" in Unreal Engine games.
    for entry in WalkDir::new(install_dir)
        .min_depth(1)
        .max_depth(3)
        .into_iter()
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if entry.file_type().is_dir() && IN_GAME_NAMES.contains(&name.as_str()) {
            found.push(entry.into_path());
        }
    }

    // Keep the outermost of nested matches, then rank by how recently something was saved.
    found.sort();
    found.dedup();
    let outer: Vec<PathBuf> = found
        .iter()
        .filter(|p| !found.iter().any(|other| other != *p && p.starts_with(other)))
        .cloned()
        .collect();
    let mut locations: Vec<SaveLocation> = outer
        .into_iter()
        .map(|path| {
            let (files, _, modified) = summarize(&path);
            SaveLocation { path, files, modified }
        })
        .filter(|l| l.files > 0)
        .collect();
    locations.sort_by_key(|l| std::cmp::Reverse(l.modified));
    locations.truncate(8);
    locations
}

/// File count, total size and newest modification time under `dir`.
fn summarize(dir: &Path) -> (u64, u64, Option<u64>) {
    let mut files = 0;
    let mut bytes = 0;
    let mut newest = None;
    for entry in WalkDir::new(dir).into_iter().flatten() {
        if let Ok(meta) = entry.metadata()
            && meta.is_file()
        {
            files += 1;
            bytes += meta.len();
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            newest = newest.max(modified);
        }
    }
    (files, bytes, newest)
}

fn fingerprint(dir: &Path) -> Result<(String, u64, u64)> {
    let mut entries = Vec::new();
    for entry in WalkDir::new(dir).sort_by_file_name() {
        let entry = entry.map_err(|e| walk_error(e, dir))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let meta = entry
            .metadata()
            .map_err(|e| Error::io("read", entry.path(), e.into()))?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        entries.push((
            entry.path().strip_prefix(dir).unwrap_or(entry.path()).to_path_buf(),
            meta.len(),
            modified,
        ));
    }
    let mut hasher = DefaultHasher::new();
    entries.hash(&mut hasher);
    let bytes = entries.iter().map(|e| e.1).sum();
    Ok((format!("{:016x}", hasher.finish()), entries.len() as u64, bytes))
}

/// Lists a game's backups, newest first. Unreadable entries are skipped.
pub fn list(backups: &Path) -> Vec<SaveBackup> {
    let Ok(entries) = fs::read_dir(backups) else {
        return Vec::new();
    };
    let mut list: Vec<SaveBackup> = entries
        .flatten()
        .filter_map(|e| read_json::<SaveBackup>(&e.path().join(META)).ok().flatten())
        .collect();
    list.sort_by(|a, b| b.at.cmp(&a.at).then(b.id.cmp(&a.id)));
    list
}

/// Copies `save_dir` into a new backup under `backups`.
///
/// Returns `Ok(None)` when there is nothing new to keep: the folder is missing or empty, or
/// nothing changed since the latest backup (unless `reason` is `Manual` or `BeforeRestore`).
pub fn backup(save_dir: &Path, backups: &Path, reason: BackupReason) -> Result<Option<SaveBackup>> {
    if !save_dir.is_dir() {
        return Ok(None);
    }
    let (fingerprint, files, bytes) = fingerprint(save_dir)?;
    if files == 0 {
        return Ok(None);
    }
    let forced = matches!(reason, BackupReason::Manual | BackupReason::BeforeRestore);
    if !forced
        && list(backups)
            .first()
            .is_some_and(|latest| latest.fingerprint == fingerprint)
    {
        return Ok(None);
    }

    let at = now_secs();
    let mut id = format!("{at}");
    let mut n = 1;
    while backups.join(&id).exists() {
        n += 1;
        id = format!("{at}-{n}");
    }
    let target = backups.join(&id);
    copy_tree(save_dir, &target.join(FILES))?;
    let record = SaveBackup {
        id,
        at,
        reason,
        files,
        bytes,
        fingerprint,
    };
    write_json_atomic(&target.join(META), &record)?;
    prune(backups, KEEP_BACKUPS);
    Ok(Some(record))
}

/// Puts a backup back into `save_dir`, after backing up what is there now.
pub fn restore(backups: &Path, backup_id: &str, save_dir: &Path) -> Result<Option<SaveBackup>> {
    if backup_id.contains(['/', '\\']) || backup_id.contains("..") {
        return Err(Error::invalid("That backup does not exist."));
    }
    let source = backups.join(backup_id).join(FILES);
    if !source.is_dir() {
        return Err(Error::invalid("That backup does not exist."));
    }
    let safety = backup(save_dir, backups, BackupReason::BeforeRestore)?;

    if save_dir.is_dir() {
        for entry in fs::read_dir(save_dir)
            .map_err(|e| Error::io("read", save_dir, e))?
            .flatten()
        {
            let path = entry.path();
            let removed = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            removed.map_err(|e| Error::io("replace", &path, e))?;
        }
    }
    copy_tree(&source, save_dir)?;
    Ok(safety)
}

/// Deletes all but the newest `keep` backups.
pub fn prune(backups: &Path, keep: usize) {
    for old in list(backups).into_iter().skip(keep) {
        let _ = fs::remove_dir_all(backups.join(&old.id));
    }
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to).map_err(|e| Error::io("create", to, e))?;
    for entry in WalkDir::new(from).min_depth(1) {
        let entry = entry.map_err(|e| walk_error(e, from))?;
        let rel = entry.path().strip_prefix(from).unwrap_or(entry.path());
        let dest = to.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&dest).map_err(|e| Error::io("create", &dest, e))?;
        } else if entry.file_type().is_file() {
            fs::copy(entry.path(), &dest).map_err(|e| Error::io("copy", entry.path(), e))?;
        }
    }
    Ok(())
}

fn walk_error(e: walkdir::Error, fallback: &Path) -> Error {
    let path = e.path().unwrap_or(fallback).to_path_buf();
    Error::io("read", &path, e.into())
}

/// Can files be created in `dir`? Games that save next to their own files need this.
pub fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".starex-write-test-{}", std::process::id()));
    match fs::write(&probe, b"") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_matching_folders_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().join("Documents");
        let game = dir.path().join("Games/Star Runner");
        fs::create_dir_all(docs.join("My Games/StarRunner/profile1")).unwrap();
        fs::write(docs.join("My Games/StarRunner/profile1/slot1.sav"), b"x").unwrap();
        fs::create_dir_all(docs.join("Unrelated")).unwrap();
        fs::write(docs.join("Unrelated/file"), b"x").unwrap();
        fs::create_dir_all(game.join("Saved/SaveGames")).unwrap();
        fs::write(game.join("Saved/SaveGames/auto.sav"), b"x").unwrap();
        fs::create_dir_all(game.join("saves")).unwrap(); // empty: not suggested

        let found = suggest_locations("Star Runner", &game, std::slice::from_ref(&docs));
        let paths: Vec<_> = found.iter().map(|l| l.path.clone()).collect();
        assert!(paths.contains(&docs.join("My Games/StarRunner")));
        assert!(paths.contains(&game.join("Saved")), "{paths:?}");
        assert!(!paths.iter().any(|p| p.ends_with("Unrelated")));
        assert!(!paths.iter().any(|p| p.ends_with("saves")));
    }

    #[test]
    fn backs_up_only_changes_and_restores_safely() {
        let dir = tempfile::tempdir().unwrap();
        let saves = dir.path().join("saves");
        let backups = dir.path().join("backups");
        fs::create_dir_all(saves.join("slot")).unwrap();
        fs::write(saves.join("slot/a.sav"), b"level 1").unwrap();

        let first = backup(&saves, &backups, BackupReason::AfterSession).unwrap().unwrap();
        assert_eq!(first.files, 1);
        assert!(
            backup(&saves, &backups, BackupReason::AfterSession).unwrap().is_none(),
            "unchanged"
        );

        fs::write(saves.join("slot/a.sav"), b"level 2 with more bytes").unwrap();
        fs::write(saves.join("new.sav"), b"x").unwrap();
        assert!(backup(&saves, &backups, BackupReason::AfterSession).unwrap().is_some());

        let safety = restore(&backups, &first.id, &saves).unwrap().unwrap();
        assert_eq!(safety.reason, BackupReason::BeforeRestore);
        assert_eq!(fs::read(saves.join("slot/a.sav")).unwrap(), b"level 1");
        assert!(!saves.join("new.sav").exists(), "restore gives back exactly the backup");
        assert_eq!(list(&backups).len(), 3);
        assert!(restore(&backups, "../escape", &saves).is_err());
    }

    #[test]
    fn prunes_old_backups_and_checks_writability() {
        let dir = tempfile::tempdir().unwrap();
        let saves = dir.path().join("s");
        let backups = dir.path().join("b");
        fs::create_dir_all(&saves).unwrap();
        for i in 0..5 {
            fs::write(saves.join("f"), vec![0u8; i + 1]).unwrap();
            backup(&saves, &backups, BackupReason::Manual).unwrap();
        }
        prune(&backups, 2);
        assert_eq!(list(&backups).len(), 2);
        assert!(is_writable(dir.path()));
        assert!(!is_writable(&dir.path().join("missing")));
    }
}
