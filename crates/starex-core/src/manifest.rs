//! File manifests: the list of files a game needs, with their sizes and BLAKE3 hashes.
//!
//! A manifest comes from one of two places:
//! - `starex.manifest.json` in the game folder, written by whoever distributes the files, or
//! - a *baseline* Starex records from a known-good copy of the game and keeps in its data folder.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::util::{now_secs, read_json, write_json_atomic};
use crate::{Error, Result};

pub const MANIFEST_FILE: &str = "starex.manifest.json";
pub const FORMAT: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: u32,
    #[serde(default)]
    pub name: Option<String>,
    /// The executable to start, relative to the game folder.
    #[serde(default)]
    pub executable: Option<String>,
    #[serde(default)]
    pub generated_at: Option<u64>,
    #[serde(default)]
    pub generator: Option<String>,
    pub files: Vec<ManifestFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFile {
    /// Relative to the game folder, `/` separated.
    pub path: String,
    pub size: u64,
    /// Lower-case hex BLAKE3 hash. Without it only presence and size are checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blake3: Option<String>,
    /// Optional files are reported when missing but do not fail verification.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Option<Self>> {
        let Some(manifest) = read_json::<Manifest>(path)? else {
            return Ok(None);
        };
        if manifest.format > FORMAT {
            return Err(Error::invalid(format!(
                "{} uses manifest format {}, which needs a newer Starex.",
                path.display(),
                manifest.format
            )));
        }
        Ok(Some(manifest))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        write_json_atomic(path, self)
    }

    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

/// Resolves a manifest path inside `root`, refusing anything that would escape it.
pub fn resolve(root: &Path, relative: &str) -> Option<PathBuf> {
    let rel = Path::new(relative);
    let safe = rel
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    (safe && !relative.is_empty()).then(|| root.join(rel))
}

/// Progress of a long file operation, reported to the UI.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done_files: u64,
    pub total_files: u64,
    pub done_bytes: u64,
    pub total_bytes: u64,
}

/// Shared counters that report progress at most every 80 ms, from any thread.
pub(crate) struct Tracker<'a> {
    total_files: u64,
    total_bytes: u64,
    files: AtomicU64,
    bytes: AtomicU64,
    started: Instant,
    last_ms: AtomicU64,
    sink: &'a (dyn Fn(Progress) + Sync),
}

impl<'a> Tracker<'a> {
    pub(crate) fn new(total_files: u64, total_bytes: u64, sink: &'a (dyn Fn(Progress) + Sync)) -> Self {
        let tracker = Self {
            total_files,
            total_bytes,
            files: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            started: Instant::now(),
            last_ms: AtomicU64::new(0),
            sink,
        };
        tracker.emit();
        tracker
    }

    pub(crate) fn add_bytes(&self, n: u64) {
        self.bytes.fetch_add(n, Ordering::Relaxed);
        self.maybe_emit();
    }

    pub(crate) fn file_done(&self) {
        self.files.fetch_add(1, Ordering::Relaxed);
        self.maybe_emit();
    }

    fn maybe_emit(&self) {
        let now = self.started.elapsed().as_millis() as u64;
        let last = self.last_ms.load(Ordering::Relaxed);
        if now >= last + 80
            && self
                .last_ms
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            self.emit();
        }
    }

    pub(crate) fn emit(&self) {
        (self.sink)(Progress {
            done_files: self.files.load(Ordering::Relaxed),
            total_files: self.total_files,
            done_bytes: self.bytes.load(Ordering::Relaxed).min(self.total_bytes),
            total_bytes: self.total_bytes,
        });
    }
}

/// Hashes a file with BLAKE3. Returns `Ok(None)` if `cancel` was set part-way.
pub(crate) fn hash_file(path: &Path, cancel: &AtomicBool, tracker: &Tracker) -> std::io::Result<Option<String>> {
    const BIG: usize = 4 * 1024 * 1024;
    let mut file = File::open(path)?;
    let len = file.metadata().map(|m| m.len()).unwrap_or(0) as usize;
    let mut buf = vec![0u8; len.clamp(16 * 1024, BIG)];
    let mut hasher = blake3::Hasher::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        if n >= 1024 * 1024 {
            hasher.update_rayon(&buf[..n]);
        } else {
            hasher.update(&buf[..n]);
        }
        tracker.add_bytes(n as u64);
    }
    Ok(Some(hasher.finalize().to_hex().to_string()))
}

/// Records a baseline manifest of every file under `root`.
///
/// Returns `Ok(None)` if cancelled.
pub fn build(
    root: &Path,
    name: &str,
    executable: Option<&Path>,
    cancel: &AtomicBool,
    on_progress: &(dyn Fn(Progress) + Sync),
) -> Result<Option<Manifest>> {
    let mut entries: Vec<(String, PathBuf, u64)> = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|e| {
            let path = e.path().unwrap_or(root).to_path_buf();
            Error::io("read", &path, e.into())
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = relative(root, entry.path());
        if rel.eq_ignore_ascii_case(MANIFEST_FILE) {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        entries.push((rel, entry.into_path(), size));
    }

    let total_bytes = entries.iter().map(|e| e.2).sum();
    let tracker = Tracker::new(entries.len() as u64, total_bytes, on_progress);

    let files: std::result::Result<Vec<Option<ManifestFile>>, Error> = entries
        .par_iter()
        .map(|(rel, path, size)| {
            let hash = hash_file(path, cancel, &tracker).map_err(|e| Error::io("read", path, e))?;
            tracker.file_done();
            Ok(hash.map(|blake3| ManifestFile {
                path: rel.clone(),
                size: *size,
                blake3: Some(blake3),
                optional: false,
            }))
        })
        .collect();
    let files = files?;
    if cancel.load(Ordering::Relaxed) || files.iter().any(Option::is_none) {
        return Ok(None);
    }
    tracker.emit();

    let mut files: Vec<ManifestFile> = files.into_iter().flatten().collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Some(Manifest {
        format: FORMAT,
        name: Some(name.to_owned()),
        executable: executable
            .and_then(|e| e.strip_prefix(root).ok())
            .map(|e| e.to_string_lossy().replace('\\', "/")),
        generated_at: Some(now_secs()),
        generator: Some(format!("Starex {}", env!("CARGO_PKG_VERSION"))),
        files,
    }))
}

pub(crate) fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Removes a baseline file if it exists.
pub fn delete(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::io("delete", path, e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_refuses_escapes() {
        let root = Path::new("C:/games/x");
        assert!(resolve(root, "bin/game.exe").is_some());
        assert!(resolve(root, "../other/file").is_none());
        assert!(resolve(root, "").is_none());
        #[cfg(windows)]
        assert!(resolve(root, "C:/Windows/system32/x.dll").is_none());
        #[cfg(not(windows))]
        assert!(resolve(root, "/etc/passwd").is_none());
    }

    #[test]
    fn builds_a_baseline() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("data")).unwrap();
        fs::write(dir.path().join("game.exe"), b"binary").unwrap();
        fs::write(dir.path().join("data/pack.bin"), vec![7u8; 3 * 1024 * 1024]).unwrap();
        fs::write(dir.path().join(MANIFEST_FILE), b"{}").unwrap();

        let cancel = AtomicBool::new(false);
        let manifest = build(dir.path(), "Game", Some(&dir.path().join("game.exe")), &cancel, &|_| {})
            .unwrap()
            .unwrap();
        assert_eq!(manifest.files.len(), 2);
        assert_eq!(manifest.files[0].path, "data/pack.bin");
        assert_eq!(manifest.executable.as_deref(), Some("game.exe"));
        assert_eq!(
            manifest.files[1].blake3.as_deref(),
            Some(blake3::hash(b"binary").to_hex().as_str())
        );
        // Large files hashed with update_rayon must match a one-shot hash.
        let big = blake3::hash(&vec![7u8; 3 * 1024 * 1024]).to_hex().to_string();
        assert_eq!(manifest.files[0].blake3.as_deref(), Some(big.as_str()));
    }
}
