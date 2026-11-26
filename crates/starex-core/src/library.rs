use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::{Game, GamePatch, NewGame};
use crate::util::{now_secs, read_json, write_json_atomic};
use crate::{Error, Result};

const FORMAT: u32 = 1;

#[derive(Serialize, Deserialize)]
struct LibraryFile {
    format: u32,
    games: Vec<Game>,
}

/// The user's game library, persisted as `library.json` in the Starex data folder.
///
/// Every change is written to disk before the call returns.
pub struct Library {
    path: PathBuf,
    games: Vec<Game>,
}

impl Library {
    /// Opens the library at `path`. A missing file is an empty library. A file that cannot be
    /// parsed is moved aside to `library.corrupt-<time>.json` so the user's data is never lost.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let games = match read_json::<LibraryFile>(&path) {
            Ok(Some(file)) => file.games,
            Ok(None) => Vec::new(),
            Err(Error::Json { .. }) => {
                let aside = path.with_file_name(format!("library.corrupt-{}.json", now_secs()));
                log::warn!("library is unreadable, moving it to {}", aside.display());
                fs::rename(&path, &aside).map_err(|e| Error::io("move", &path, e))?;
                Vec::new()
            }
            Err(err) => return Err(err),
        };
        Ok(Self { path, games })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn games(&self) -> &[Game] {
        &self.games
    }

    pub fn get(&self, id: &str) -> Result<&Game> {
        self.games.iter().find(|g| g.id == id).ok_or(Error::GameNotFound)
    }

    /// Adds a game and returns it.
    pub fn add(&mut self, new: NewGame) -> Result<Game> {
        let name = new.name.trim();
        if name.is_empty() {
            return Err(Error::invalid("Give the game a name."));
        }
        if !new.executable.exists() {
            return Err(Error::invalid(format!("{} does not exist.", new.executable.display())));
        }
        let game = Game {
            id: uuid::Uuid::new_v4().simple().to_string(),
            name: name.to_owned(),
            install_dir: new.install_dir,
            executable: new.executable,
            launch_args: new.launch_args.trim().to_owned(),
            working_dir: None,
            env: Vec::new(),
            artwork: Default::default(),
            favorite: false,
            added_at: now_secs(),
            last_played: None,
            playtime_secs: 0,
            sessions: 0,
            last_verification: None,
            save_dir: None,
            backup_saves: true,
            last_scan: None,
        };
        self.games.push(game.clone());
        self.save()?;
        Ok(game)
    }

    /// Applies a user edit.
    pub fn patch(&mut self, id: &str, patch: GamePatch) -> Result<Game> {
        if let Some(name) = &patch.name
            && name.trim().is_empty()
        {
            return Err(Error::invalid("Give the game a name."));
        }
        if let Some(exe) = &patch.executable
            && !exe.exists()
        {
            return Err(Error::invalid(format!("{} does not exist.", exe.display())));
        }
        self.update(id, |game| {
            if let Some(name) = patch.name {
                game.name = name.trim().to_owned();
            }
            if let Some(exe) = patch.executable {
                game.executable = exe;
            }
            if let Some(args) = patch.launch_args {
                game.launch_args = args.trim().to_owned();
            }
            if let Some(dir) = patch.working_dir {
                game.working_dir = dir;
            }
            if let Some(env) = patch.env {
                game.env = env.into_iter().filter(|v| !v.key.trim().is_empty()).collect();
            }
            if let Some(favorite) = patch.favorite {
                game.favorite = favorite;
            }
            if let Some(dir) = patch.save_dir {
                game.save_dir = dir;
            }
            if let Some(backup) = patch.backup_saves {
                game.backup_saves = backup;
            }
        })
    }

    /// Runs `change` on a game, saves, and returns the updated game.
    pub fn update(&mut self, id: &str, change: impl FnOnce(&mut Game)) -> Result<Game> {
        let game = self.games.iter_mut().find(|g| g.id == id).ok_or(Error::GameNotFound)?;
        change(game);
        let updated = game.clone();
        self.save()?;
        Ok(updated)
    }

    /// Records a finished play session.
    pub fn record_session(&mut self, id: &str, started_at: u64, ended_at: u64) -> Result<Game> {
        self.update(id, |game| {
            game.sessions = game.sessions.saturating_add(1);
            game.last_played = Some(started_at);
            game.playtime_secs = game.playtime_secs.saturating_add(ended_at.saturating_sub(started_at));
        })
    }

    pub fn remove(&mut self, id: &str) -> Result<Game> {
        let index = self.games.iter().position(|g| g.id == id).ok_or(Error::GameNotFound)?;
        let game = self.games.remove(index);
        self.save()?;
        Ok(game)
    }

    fn save(&self) -> Result<()> {
        write_json_atomic(
            &self.path,
            &LibraryFile {
                format: FORMAT,
                games: self.games.clone(),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, NewGame) {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("game.exe");
        fs::write(&exe, b"MZ").unwrap();
        let new = NewGame {
            name: "  Test Game ".into(),
            install_dir: dir.path().to_path_buf(),
            executable: exe,
            launch_args: "-windowed".into(),
        };
        (dir, new)
    }

    #[test]
    fn add_patch_record_remove_persist() {
        let (dir, new) = fixture();
        let path = dir.path().join("data/library.json");
        let mut lib = Library::open(&path).unwrap();
        let game = lib.add(new).unwrap();
        assert_eq!(game.name, "Test Game");

        lib.patch(
            &game.id,
            GamePatch {
                favorite: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        lib.record_session(&game.id, 100, 160).unwrap();

        let reopened = Library::open(&path).unwrap();
        let stored = reopened.get(&game.id).unwrap();
        assert!(stored.favorite);
        assert_eq!(stored.playtime_secs, 60);
        assert_eq!(stored.sessions, 1);

        let mut lib = reopened;
        lib.remove(&game.id).unwrap();
        assert!(Library::open(&path).unwrap().games().is_empty());
    }

    #[test]
    fn rejects_empty_name_and_missing_exe() {
        let (dir, mut new) = fixture();
        let mut lib = Library::open(dir.path().join("library.json")).unwrap();
        new.name = "   ".into();
        assert!(lib.add(new.clone()).is_err());
        new.name = "Ok".into();
        new.executable = dir.path().join("nope.exe");
        assert!(lib.add(new).is_err());
    }

    #[test]
    fn corrupt_library_is_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        fs::write(&path, b"{ not json").unwrap();
        let lib = Library::open(&path).unwrap();
        assert!(lib.games().is_empty());
        let aside = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().starts_with("library.corrupt-"));
        assert!(aside);
    }
}
