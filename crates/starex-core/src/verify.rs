//! Checks that a game has everything it needs to start.

use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::binary::{self, Format};
use crate::manifest::{self, MANIFEST_FILE, Manifest, Progress, Tracker};
use crate::model::{Game, VerificationSummary};
use crate::pe::{HostSupport, PeProblem};
use crate::saves;
use crate::util::now_secs;

/// How many individual file problems a report lists. The counts stay exact.
const MAX_ISSUES: usize = 200;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum VerifyMode {
    /// Presence and size of every file. Takes seconds even for very large games.
    Quick,
    /// Also hashes every file and compares it with the manifest.
    Full,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum CheckStatus {
    Info,
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub id: &'static str,
    pub title: String,
    pub detail: String,
    pub status: CheckStatus,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum IssueKind {
    Missing,
    SizeMismatch,
    HashMismatch,
    Unreadable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileIssue {
    pub path: String,
    pub kind: IssueKind,
    pub optional: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ManifestSource {
    /// `starex.manifest.json` shipped in the game folder.
    GameFolder,
    /// A baseline Starex recorded earlier.
    Baseline,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub game_id: String,
    pub mode: VerifyMode,
    pub status: CheckStatus,
    pub checks: Vec<Check>,
    pub issues: Vec<FileIssue>,
    pub manifest_source: Option<ManifestSource>,
    pub files_checked: u64,
    pub bytes_checked: u64,
    pub missing: u64,
    pub modified: u64,
    pub duration_ms: u64,
    pub cancelled: bool,
    pub at: u64,
}

impl VerifyReport {
    pub fn summary(&self) -> VerificationSummary {
        VerificationSummary {
            at: self.at,
            mode: self.mode,
            status: self.status,
            files_checked: self.files_checked,
            missing: self.missing,
            modified: self.modified,
            duration_ms: self.duration_ms,
        }
    }
}

/// Verifies `game`. `baseline` is where Starex keeps this game's recorded baseline, if any.
pub fn verify(
    game: &Game,
    baseline: &Path,
    mode: VerifyMode,
    cancel: &AtomicBool,
    on_progress: &(dyn Fn(Progress) + Sync),
) -> VerifyReport {
    let started = Instant::now();
    let root = game.install_dir.as_path();
    let mut checks = Vec::new();
    let mut issues = Vec::new();
    let (mut files_checked, mut bytes_checked, mut missing, mut modified) = (0, 0, 0, 0);
    let mut manifest_source = None;
    let mut cancelled = false;

    // 1. The folder.
    let folder_ok = root.is_dir();
    checks.push(Check {
        id: "folder",
        title: "Game folder".into(),
        detail: if folder_ok {
            root.display().to_string()
        } else {
            format!(
                "{} is missing. Was the drive disconnected or the game moved?",
                root.display()
            )
        },
        status: if folder_ok {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        },
    });

    // 2. The executable.
    checks.push(executable_check(&game.executable));

    // 3. Console packages that cannot run here.
    if folder_ok && contains_xvc(root) {
        checks.push(Check {
            id: "package",
            title: "Platform".into(),
            detail: "The folder holds Xbox console containers (.xvc), which only an Xbox can open.".into(),
            status: CheckStatus::Fail,
        });
    }

    // 4. Somewhere to save.
    if folder_ok {
        checks.push(saves_check(game));
    }

    // 5. Files against the manifest.
    if folder_ok {
        let shipped = root.join(MANIFEST_FILE);
        let loaded = match Manifest::load(&shipped) {
            Ok(Some(m)) => Ok(Some((m, ManifestSource::GameFolder))),
            Ok(None) => Manifest::load(baseline).map(|m| m.map(|m| (m, ManifestSource::Baseline))),
            Err(e) => Err(e),
        };
        match loaded {
            Ok(Some((manifest, source))) => {
                manifest_source = Some(source);
                let outcome = check_files(root, &manifest, mode, cancel, on_progress);
                files_checked = outcome.files;
                bytes_checked = outcome.bytes;
                missing = outcome.missing;
                modified = outcome.modified;
                cancelled = outcome.cancelled;
                checks.push(files_check(&manifest, source, mode, &outcome));
                issues = outcome.issues;
            }
            Ok(None) => checks.push(Check {
                id: "files",
                title: "Game files".into(),
                detail: "No file list yet. Record a baseline while the game works, and Starex will catch missing or changed files from then on.".into(),
                status: CheckStatus::Info,
            }),
            Err(e) => checks.push(Check {
                id: "files",
                title: "Game files".into(),
                detail: e.to_string(),
                status: CheckStatus::Warn,
            }),
        }
    }

    let status = checks
        .iter()
        .map(|c| c.status)
        .filter(|s| *s != CheckStatus::Info)
        .max()
        .unwrap_or(CheckStatus::Pass);

    VerifyReport {
        game_id: game.id.clone(),
        mode,
        status,
        checks,
        issues,
        manifest_source,
        files_checked,
        bytes_checked,
        missing,
        modified,
        duration_ms: started.elapsed().as_millis() as u64,
        cancelled,
        at: now_secs(),
    }
}

fn executable_check(exe: &Path) -> Check {
    let name = exe
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| exe.display().to_string());
    let (status, detail) = if !exe.exists() {
        (CheckStatus::Fail, format!("{name} is missing."))
    } else {
        match binary::probe(exe) {
            Ok(info) if info.is_library => (CheckStatus::Fail, format!("{name} is a library, not a program.")),
            Ok(info) => {
                let arch = info.arch.label();
                match binary::support(&info) {
                    HostSupport::Native if info.format == Format::Script => {
                        (CheckStatus::Pass, format!("{name} · launch script"))
                    }
                    HostSupport::Native => (CheckStatus::Pass, format!("{name} · {arch}")),
                    HostSupport::Emulated if cfg!(target_os = "macos") => {
                        (CheckStatus::Pass, format!("{name} · {arch} (runs through Rosetta)"))
                    }
                    HostSupport::Emulated => (
                        CheckStatus::Pass,
                        format!("{name} · {arch} (runs through Windows emulation)"),
                    ),
                    HostSupport::Compat => (
                        CheckStatus::Pass,
                        format!("{name} · Windows {arch} program (runs through Wine)"),
                    ),
                    HostSupport::Unsupported => (
                        CheckStatus::Fail,
                        format!(
                            "{name} is built for {arch} on another operating system, which this computer cannot run."
                        ),
                    ),
                }
            }
            Err(PeProblem::Unreadable) => (
                CheckStatus::Fail,
                format!("{name} cannot be opened. Is it in use or blocked?"),
            ),
            Err(PeProblem::NotExecutable) => (
                CheckStatus::Fail,
                format!("{name} is not a readable program. It may be encrypted or built for another platform."),
            ),
        }
    };
    Check {
        id: "executable",
        title: "Executable".into(),
        detail,
        status,
    }
}

/// Games save either next to their files or in a save folder; both must accept new files.
fn saves_check(game: &Game) -> Check {
    let (status, detail) = match &game.save_dir {
        Some(dir) if !dir.is_dir() => (
            CheckStatus::Warn,
            format!("The save folder {} is missing. Choose it again in Properties > Saves.", dir.display()),
        ),
        Some(dir) if !saves::is_writable(dir) => (
            CheckStatus::Warn,
            format!("Starex cannot write to {}, so the game may fail to save there.", dir.display()),
        ),
        Some(dir) => (CheckStatus::Pass, format!("Saves go to {}", dir.display())),
        None if !saves::is_writable(&game.install_dir) => (
            CheckStatus::Warn,
            "The game folder is read-only. Games that save next to their own files will fail to save. Move the game to a folder you own, such as one outside Program Files.".into(),
        ),
        None => (CheckStatus::Pass, "The game folder accepts new files, so the game can save.".into()),
    };
    Check {
        id: "saves",
        title: "Saving".into(),
        detail,
        status,
    }
}

fn contains_xvc(root: &Path) -> bool {
    fs::read_dir(root)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .any(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("xvc")))
        })
        .unwrap_or(false)
}

struct FileOutcome {
    files: u64,
    bytes: u64,
    missing: u64,
    missing_required: u64,
    modified: u64,
    unreadable: u64,
    cancelled: bool,
    issues: Vec<FileIssue>,
}

fn check_files(
    root: &Path,
    manifest: &Manifest,
    mode: VerifyMode,
    cancel: &AtomicBool,
    on_progress: &(dyn Fn(Progress) + Sync),
) -> FileOutcome {
    let hashing = mode == VerifyMode::Full;
    let total_bytes = if hashing { manifest.total_bytes() } else { 0 };
    let tracker = Tracker::new(manifest.files.len() as u64, total_bytes, on_progress);
    let issues = Mutex::new(Vec::new());
    let counts = [
        AtomicU64::new(0), // checked
        AtomicU64::new(0), // missing
        AtomicU64::new(0), // missing required
        AtomicU64::new(0), // modified
        AtomicU64::new(0), // unreadable
        AtomicU64::new(0), // bytes
    ];
    let report = |path: &str, kind: IssueKind, optional: bool| {
        let mut list = issues.lock().unwrap_or_else(|e| e.into_inner());
        if list.len() < MAX_ISSUES {
            list.push(FileIssue {
                path: path.to_owned(),
                kind,
                optional,
            });
        }
    };

    manifest.files.par_iter().for_each(|file| {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let Some(path) = manifest::resolve(root, &file.path) else {
            return;
        };
        let size = match fs::metadata(&path) {
            Ok(meta) if meta.is_file() => meta.len(),
            _ => {
                counts[1].fetch_add(1, Ordering::Relaxed);
                if !file.optional {
                    counts[2].fetch_add(1, Ordering::Relaxed);
                }
                report(&file.path, IssueKind::Missing, file.optional);
                tracker.add_bytes(if hashing { file.size } else { 0 });
                tracker.file_done();
                return;
            }
        };
        counts[0].fetch_add(1, Ordering::Relaxed);
        if size != file.size {
            counts[3].fetch_add(1, Ordering::Relaxed);
            report(&file.path, IssueKind::SizeMismatch, file.optional);
            tracker.add_bytes(if hashing { file.size } else { 0 });
        } else if let (true, Some(expected)) = (hashing, &file.blake3) {
            match manifest::hash_file(&path, cancel, &tracker) {
                Ok(Some(actual)) => {
                    counts[5].fetch_add(size, Ordering::Relaxed);
                    if !actual.eq_ignore_ascii_case(expected) {
                        counts[3].fetch_add(1, Ordering::Relaxed);
                        report(&file.path, IssueKind::HashMismatch, file.optional);
                    }
                }
                Ok(None) => return,
                Err(_) => {
                    counts[4].fetch_add(1, Ordering::Relaxed);
                    report(&file.path, IssueKind::Unreadable, file.optional);
                }
            }
        }
        tracker.file_done();
    });
    tracker.emit();

    let mut issues = issues.into_inner().unwrap_or_else(|e| e.into_inner());
    issues.sort_by(|a, b| a.path.cmp(&b.path));
    let get = |i: usize| counts[i].load(Ordering::Relaxed);
    FileOutcome {
        files: get(0),
        missing: get(1),
        missing_required: get(2),
        modified: get(3),
        unreadable: get(4),
        bytes: get(5),
        cancelled: cancel.load(Ordering::Relaxed),
        issues,
    }
}

fn files_check(manifest: &Manifest, source: ManifestSource, mode: VerifyMode, o: &FileOutcome) -> Check {
    let total = manifest.files.len();
    let from = match source {
        ManifestSource::GameFolder => "the game's manifest",
        ManifestSource::Baseline => "your baseline",
    };
    let plural = |n: u64, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let (status, detail) = if o.cancelled {
        (CheckStatus::Warn, "Verification was cancelled.".to_owned())
    } else if o.missing_required > 0 {
        (
            CheckStatus::Fail,
            format!(
                "{} missing out of {total} in {from}.",
                plural(o.missing_required, "required file is", "required files are")
            ),
        )
    } else if o.modified > 0 || o.unreadable > 0 || o.missing > 0 {
        let mut parts = Vec::new();
        if o.modified > 0 {
            parts.push(plural(o.modified, "file differs", "files differ"));
        }
        if o.unreadable > 0 {
            parts.push(plural(
                o.unreadable,
                "file could not be read",
                "files could not be read",
            ));
        }
        if o.missing > 0 {
            parts.push(plural(
                o.missing,
                "optional file is missing",
                "optional files are missing",
            ));
        }
        (
            CheckStatus::Warn,
            format!(
                "{} from {from}. Updates and settings files can cause this.",
                parts.join(", ")
            ),
        )
    } else {
        let how = match mode {
            VerifyMode::Quick => "present with the right sizes",
            VerifyMode::Full => "present and intact",
        };
        (
            CheckStatus::Pass,
            format!("All {total} files are {how}, checked against {from}."),
        )
    };
    Check {
        id: "files",
        title: "Game files".into(),
        detail,
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pe::tests::fake_pe;

    fn game(root: &Path) -> Game {
        Game {
            id: "g".into(),
            name: "G".into(),
            install_dir: root.to_path_buf(),
            executable: root.join("game.exe"),
            launch_args: String::new(),
            working_dir: None,
            env: vec![],
            artwork: Default::default(),
            favorite: false,
            added_at: 0,
            last_played: None,
            playtime_secs: 0,
            sessions: 0,
            last_verification: None,
            save_dir: None,
            backup_saves: true,
            last_scan: None,
        }
    }

    #[test]
    fn detects_missing_and_modified_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("game");
        fs::create_dir_all(root.join("data")).unwrap();
        fs::write(root.join("game.exe"), fake_pe(0x8664, 2)).unwrap();
        fs::write(root.join("data/a.pak"), b"aaaa").unwrap();
        fs::write(root.join("data/b.pak"), b"bbbb").unwrap();
        let g = game(&root);
        let baseline = dir.path().join("baseline.json");
        let cancel = AtomicBool::new(false);

        let report = verify(&g, &baseline, VerifyMode::Full, &cancel, &|_| {});
        assert_eq!(report.status, CheckStatus::Pass, "no manifest is informational");

        manifest::build(&root, "G", Some(&g.executable), &cancel, &|_| {})
            .unwrap()
            .unwrap()
            .save(&baseline)
            .unwrap();
        let ok = verify(&g, &baseline, VerifyMode::Full, &cancel, &|_| {});
        assert_eq!(ok.status, CheckStatus::Pass);
        assert_eq!(ok.files_checked, 3);
        assert_eq!(ok.manifest_source, Some(ManifestSource::Baseline));

        fs::write(root.join("data/a.pak"), b"abcd").unwrap(); // same size, different bytes
        let quick = verify(&g, &baseline, VerifyMode::Quick, &cancel, &|_| {});
        assert_eq!(quick.status, CheckStatus::Pass, "quick mode only checks sizes");
        let full = verify(&g, &baseline, VerifyMode::Full, &cancel, &|_| {});
        assert_eq!(full.status, CheckStatus::Warn);
        assert_eq!(full.modified, 1);

        fs::remove_file(root.join("data/b.pak")).unwrap();
        let broken = verify(&g, &baseline, VerifyMode::Quick, &cancel, &|_| {});
        assert_eq!(broken.status, CheckStatus::Fail);
        assert_eq!(broken.missing, 1);
        assert_eq!(broken.issues[0].kind, IssueKind::Missing);
    }

    #[test]
    fn shipped_manifest_wins_over_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::write(root.join("game.exe"), fake_pe(0x8664, 2)).unwrap();
        let shipped = Manifest {
            format: 1,
            name: None,
            executable: None,
            generated_at: None,
            generator: None,
            files: vec![manifest::ManifestFile {
                path: "missing.dat".into(),
                size: 1,
                blake3: None,
                optional: true,
            }],
        };
        shipped.save(&root.join(MANIFEST_FILE)).unwrap();
        let report = verify(
            &game(&root),
            &root.join("none.json"),
            VerifyMode::Quick,
            &AtomicBool::new(false),
            &|_| {},
        );
        assert_eq!(report.manifest_source, Some(ManifestSource::GameFolder));
        assert_eq!(report.status, CheckStatus::Warn, "an optional missing file only warns");
    }

    #[test]
    fn encrypted_executable_fails() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("game.exe"), vec![0x11u8; 512]).unwrap();
        let report = verify(
            &game(dir.path()),
            &dir.path().join("b.json"),
            VerifyMode::Quick,
            &AtomicBool::new(false),
            &|_| {},
        );
        assert_eq!(report.status, CheckStatus::Fail);
    }
}
