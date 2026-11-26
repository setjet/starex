//! Looks at a folder the user picked and works out what game it holds and how to start it.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use walkdir::WalkDir;

use crate::binary::{self, Format};
use crate::manifest::MANIFEST_FILE;
use crate::pe::{Arch, HostSupport, PeProblem, Subsystem};
use crate::util::{normalize, words};

/// How deep to look for executables and artwork. Games rarely bury the main binary deeper.
const EXE_DEPTH: usize = 6;
const ART_DEPTH: usize = 3;
const MAX_CANDIDATES: usize = 12;

/// Folders that hold installers and runtimes, never the game itself.
const SKIP_DIRS: &[&str] = &[
    "_commonredist",
    "commonredist",
    "redist",
    "redists",
    "redistributables",
    "_redist",
    "directx",
    "vcredist",
    "__installer",
    "installer",
    "installers",
    "prereqs",
    "prerequisites",
    "support",
    "dotnet",
    "easyanticheat",
    "battleye",
    "$recycle.bin",
];

/// Executables that ship with games but are not the game.
const SKIP_EXE: &[&str] = &[
    "unins",
    "uninstall",
    "vcredist",
    "vc_redist",
    "dxsetup",
    "dxwebsetup",
    "crashhandler",
    "crashreport",
    "crashpad",
    "crashsender",
    "bugsplat",
    "dotnetfx",
    "ndp4",
    "oalinst",
    "physx",
    "installer",
    "cefprocess",
    "cefsubprocess",
    "webhelper",
    "quicksfv",
    "eossdk",
    "easyanticheat",
    "beservice",
    "dxdiag",
    "redist",
];

/// Words that make an executable less likely to be the one to start.
const DEMOTE_WORDS: &[&str] = &[
    "server",
    "dedicated",
    "editor",
    "benchmark",
    "config",
    "configuration",
    "settings",
    "setup",
    "helper",
    "report",
    "update",
    "updater",
    "tool",
    "tools",
    "test",
    "debug",
    "sdk",
    "modmanager",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderInspection {
    pub root: PathBuf,
    pub suggested_name: String,
    /// Best first. The first one that is `runnable` is the suggested executable.
    pub candidates: Vec<ExecutableCandidate>,
    pub package: Option<PackageKind>,
    pub artwork: DetectedArtwork,
    pub has_manifest: bool,
    pub file_count: u64,
    pub total_bytes: u64,
    /// Plain-language findings the user should know about before adding the game.
    pub notes: Vec<Note>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableCandidate {
    pub path: PathBuf,
    /// Path relative to the inspected folder, with `/` separators.
    pub relative: String,
    pub size: u64,
    pub format: Option<Format>,
    pub arch: Option<Arch>,
    pub subsystem: Option<Subsystem>,
    pub problem: Option<PeProblem>,
    pub host: Option<HostSupport>,
    pub runnable: bool,
    pub score: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PackageKind {
    /// Microsoft GDK layout (`MicrosoftGame.config`): Xbox PC and console builds use it.
    MicrosoftGdk,
    /// MSIX / UWP layout (`AppxManifest.xml`).
    Msix,
    /// Xbox console container files (`.xvc`), which only an Xbox can open.
    XboxContainer,
    Steam,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedArtwork {
    pub banner: Option<PathBuf>,
    pub cover: Option<PathBuf>,
    pub logo: Option<PathBuf>,
    pub icon: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub level: NoteLevel,
    pub text: String,
}

impl Note {
    fn new(level: NoteLevel, text: impl Into<String>) -> Self {
        Self {
            level,
            text: text.into(),
        }
    }
}

pub fn inspect_folder(root: &Path) -> std::io::Result<FolderInspection> {
    let meta = fs::metadata(root)?;
    if !meta.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            "choose a folder, not a file",
        ));
    }

    let folder_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut file_count = 0u64;
    let mut total_bytes = 0u64;
    let mut exes: Vec<(PathBuf, u64, usize)> = Vec::new();
    let mut art = ArtCollector::default();
    let mut xvc = false;

    let mut walker = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !(e.file_type().is_dir() && e.depth() > 0 && is_skipped_dir(e.path())));
    while let Some(entry) = walker.next() {
        let Ok(entry) = entry else { continue };
        // A macOS app bundle is one program: offer it and do not look inside.
        if entry.file_type().is_dir()
            && entry.depth() > 0
            && entry.path().extension().is_some_and(|e| e.eq_ignore_ascii_case("app"))
        {
            if !cfg!(windows) && entry.depth() <= EXE_DEPTH {
                exes.push((entry.path().to_path_buf(), dir_size(entry.path()), entry.depth()));
            }
            walker.skip_current_dir();
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        file_count += 1;
        total_bytes += size;

        let path = entry.path();
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "exe" if entry.depth() <= EXE_DEPTH && !is_skipped_exe(path) => {
                exes.push((path.to_path_buf(), size, entry.depth()));
            }
            _ if !cfg!(windows)
                && entry.depth() <= EXE_DEPTH
                && !is_skipped_exe(path)
                && is_native_program(path, &entry) =>
            {
                exes.push((path.to_path_buf(), size, entry.depth()));
            }
            "xvc" => xvc = true,
            "png" | "jpg" | "jpeg" | "webp" if entry.depth() <= ART_DEPTH => art.offer(path, size),
            _ => {}
        }
    }

    let package = if xvc {
        Some(PackageKind::XboxContainer)
    } else if root.join("MicrosoftGame.config").is_file() {
        Some(PackageKind::MicrosoftGdk)
    } else if root.join("AppxManifest.xml").is_file() || root.join("appxmanifest.xml").is_file() {
        Some(PackageKind::Msix)
    } else if root.join("steam_appid.txt").is_file() || root.join("steam_api64.dll").is_file() {
        Some(PackageKind::Steam)
    } else {
        None
    };

    let display_name = match package {
        Some(PackageKind::MicrosoftGdk) => gdk_display_name(&root.join("MicrosoftGame.config")),
        _ => None,
    };
    let suggested_name = display_name.unwrap_or_else(|| pretty_name(&folder_name));
    let name_key = normalize(&suggested_name);
    let name_words = words(&suggested_name);

    let mut candidates: Vec<ExecutableCandidate> = exes
        .into_iter()
        .map(|(path, size, depth)| score_candidate(root, path, size, depth, &name_key, &name_words))
        .collect();
    candidates.sort_by(|a, b| b.score.cmp(&a.score).then(a.relative.cmp(&b.relative)));
    candidates.truncate(MAX_CANDIDATES);

    let mut notes = Vec::new();
    let runnable = candidates.iter().any(|c| c.runnable);
    match package {
        Some(PackageKind::XboxContainer) => notes.push(Note::new(
            NoteLevel::Error,
            "This folder holds Xbox console containers (.xvc). Only an Xbox can open them; Starex can launch Windows executables only.",
        )),
        Some(PackageKind::MicrosoftGdk) if !runnable => notes.push(Note::new(
            NoteLevel::Error,
            "This is a Microsoft GDK package, but none of its executables is a readable Windows program. Console and Store builds are encrypted and need a PC version of the game.",
        )),
        Some(PackageKind::MicrosoftGdk) => notes.push(Note::new(
            NoteLevel::Info,
            "Microsoft GDK layout detected. Some GDK games also need the Gaming Services runtime installed.",
        )),
        Some(PackageKind::Msix) => notes.push(Note::new(
            NoteLevel::Warn,
            "This looks like an MSIX/UWP package. Those usually have to be installed through Windows before they can run.",
        )),
        _ => {}
    }
    // A console package note already explains why nothing here can run.
    let explained = !runnable && matches!(package, Some(PackageKind::XboxContainer | PackageKind::MicrosoftGdk));
    if explained {
        // Nothing to add.
    } else if candidates.is_empty() {
        notes.push(Note::new(
            NoteLevel::Error,
            "No executable (.exe) was found in this folder.",
        ));
    } else if !runnable {
        let encrypted = candidates.iter().any(|c| c.problem == Some(PeProblem::NotExecutable));
        notes.push(Note::new(
            NoteLevel::Error,
            if encrypted {
                "The executables here are not readable Windows programs. They may be encrypted or built for another platform."
            } else {
                "None of the executables here can run on this computer's processor."
            },
        ));
    } else if let Some(best) = candidates.iter().find(|c| c.runnable)
        && best.host == Some(HostSupport::Compat)
    {
        notes.push(Note::new(
            NoteLevel::Info,
            "This is a Windows game. Starex runs it through Wine, so Wine needs to be installed (see Settings > General).",
        ));
    } else if candidates
        .iter()
        .find(|c| c.runnable)
        .is_some_and(|c| c.host == Some(HostSupport::Emulated) && cfg!(windows))
    {
        notes.push(Note::new(
            NoteLevel::Info,
            "This game is built for x64. Windows on ARM runs it through emulation, so expect lower performance.",
        ));
    }

    Ok(FolderInspection {
        root: root.to_path_buf(),
        suggested_name,
        candidates,
        package,
        artwork: art.finish(),
        has_manifest: root.join(MANIFEST_FILE).is_file(),
        file_count,
        total_bytes,
        notes,
    })
}

/// On Linux and macOS: a native program or launch script (by name, or by the executable bit and
/// a recognised header). Libraries are left out.
fn is_native_program(path: &Path, entry: &walkdir::DirEntry) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if name.contains(".so") || name.ends_with(".dylib") || name.ends_with(".dll") {
        return false;
    }
    let named = [".sh", ".x86_64", ".x86", ".appimage", ".command", ".arm64"]
        .iter()
        .any(|ext| name.ends_with(ext));
    #[cfg(unix)]
    let executable_bit = {
        use std::os::unix::fs::PermissionsExt;
        entry
            .metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    };
    #[cfg(not(unix))]
    let executable_bit = {
        let _ = entry;
        false
    };
    (named || executable_bit) && binary::probe(path).is_ok_and(|b| !b.is_library && b.format != Format::Pe)
}

fn dir_size(dir: &Path) -> u64 {
    WalkDir::new(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

fn is_skipped_dir(path: &Path) -> bool {
    path.file_name()
        .map(|n| SKIP_DIRS.contains(&n.to_string_lossy().to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn is_skipped_exe(path: &Path) -> bool {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    stem == "setup" || SKIP_EXE.iter().any(|skip| stem.contains(skip))
}

fn score_candidate(
    root: &Path,
    path: PathBuf,
    size: u64,
    depth: usize,
    name_key: &str,
    name_words: &[String],
) -> ExecutableCandidate {
    let relative = path
        .strip_prefix(root)
        .unwrap_or(&path)
        .to_string_lossy()
        .replace('\\', "/");
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem_key = normalize(&stem);
    let stem_words = words(&stem);

    let mut score: i32 = 0;
    if !name_key.is_empty() && !stem_key.is_empty() {
        if stem_key == name_key {
            score += 60;
        } else if stem_key.contains(name_key) || name_key.contains(&stem_key) {
            score += 35;
        }
    }
    score += 10 * stem_words.iter().filter(|w| name_words.contains(w)).count() as i32;
    score -= 8 * depth.saturating_sub(1) as i32;
    let mb = (size as f64 / 1_048_576.0).max(1.0);
    score += (mb.log2() * 4.0).min(32.0) as i32;
    if stem_words.iter().any(|w| DEMOTE_WORDS.contains(&w.as_str())) {
        score -= 25;
    }
    if stem_key.ends_with("shipping") {
        score += 5;
    }

    let (format, arch, subsystem, problem, host, runnable) = match binary::probe(&path) {
        Ok(info) => {
            let host = binary::support(&info);
            score += match info.subsystem {
                Some(Subsystem::Gui) => 10,
                Some(Subsystem::Console) => -10,
                _ => 0,
            };
            // Prefer a native build over a Windows one that would need Wine.
            score += match host {
                HostSupport::Native => 15,
                HostSupport::Emulated => 8,
                _ => 0,
            };
            let runnable = host != HostSupport::Unsupported;
            if !runnable {
                score -= 40;
            }
            (
                Some(info.format),
                Some(info.arch),
                info.subsystem,
                None,
                Some(host),
                runnable,
            )
        }
        Err(problem) => {
            score -= 60;
            (None, None, None, Some(problem), None, false)
        }
    };

    ExecutableCandidate {
        path,
        relative,
        size,
        format,
        arch,
        subsystem,
        problem,
        host,
        runnable,
        score,
    }
}

/// "grand_theft-auto.VI" -> "Grand Theft Auto VI".
fn pretty_name(folder: &str) -> String {
    let spaced: String = folder
        .chars()
        .map(|c| if matches!(c, '_' | '-' | '.') { ' ' } else { c })
        .collect();
    let parts: Vec<&str> = spaced.split_whitespace().collect();
    if parts.is_empty() {
        return "New game".to_owned();
    }
    let all_lower = spaced.chars().all(|c| !c.is_ascii_uppercase());
    parts
        .iter()
        .map(|w| {
            if all_lower {
                let mut chars = w.chars();
                chars
                    .next()
                    .map(|f| f.to_uppercase().collect::<String>() + chars.as_str())
                    .unwrap_or_default()
            } else {
                (*w).to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads `DefaultDisplayName` from a GDK `MicrosoftGame.config` without a full XML parser.
fn gdk_display_name(config: &Path) -> Option<String> {
    let text = fs::read_to_string(config).ok()?;
    let key = "DefaultDisplayName=\"";
    let start = text.find(key)? + key.len();
    let end = text[start..].find('"')? + start;
    let name = text[start..end].trim();
    // Localised configs use "ms-resource:" references that are not human-readable.
    (!name.is_empty() && !name.starts_with("ms-resource")).then(|| name.to_owned())
}

#[derive(Default)]
struct ArtCollector {
    banner: Option<(PathBuf, u64)>,
    cover: Option<(PathBuf, u64)>,
    logo: Option<(PathBuf, u64)>,
    icon: Option<(PathBuf, u64)>,
}

impl ArtCollector {
    fn offer(&mut self, path: &Path, size: u64) {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let has = |keys: &[&str]| keys.iter().any(|k| stem.contains(k));
        let slot = if has(&["hero", "banner", "background", "splash", "keyart", "key_art"]) {
            &mut self.banner
        } else if has(&["cover", "boxart", "box_art", "capsule", "poster", "grid"]) {
            &mut self.cover
        } else if has(&["logo", "wordmark", "title"]) && !has(&["store", "small", "square44"]) {
            &mut self.logo
        } else if has(&["icon", "square150", "storelogo"]) {
            &mut self.icon
        } else {
            return;
        };
        // Prefer the largest file: it is usually the highest resolution.
        if slot.as_ref().is_none_or(|(_, s)| size > *s) {
            *slot = Some((path.to_path_buf(), size));
        }
    }

    fn finish(self) -> DetectedArtwork {
        DetectedArtwork {
            banner: self.banner.map(|(p, _)| p),
            cover: self.cover.map(|(p, _)| p),
            logo: self.logo.map(|(p, _)| p),
            icon: self.icon.map(|(p, _)| p),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pe::tests::fake_pe;

    #[test]
    fn picks_the_game_executable() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("star_runner");
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("_CommonRedist")).unwrap();
        let big = {
            let mut b = fake_pe(0x8664, 2);
            b.resize(4 * 1_048_576, 0);
            b
        };
        fs::write(root.join("bin/StarRunner.exe"), &big).unwrap();
        fs::write(root.join("bin/StarRunnerServer.exe"), fake_pe(0x8664, 3)).unwrap();
        fs::write(root.join("unins000.exe"), fake_pe(0x014c, 2)).unwrap();
        fs::write(root.join("_CommonRedist/vc_redist.x64.exe"), fake_pe(0x8664, 2)).unwrap();
        fs::write(root.join("hero_banner.jpg"), [0u8; 10]).unwrap();

        let found = inspect_folder(&root).unwrap();
        assert_eq!(found.suggested_name, "Star Runner");
        assert_eq!(found.candidates[0].relative, "bin/StarRunner.exe");
        assert!(found.candidates[0].runnable);
        assert_eq!(found.candidates.len(), 2, "uninstaller and redist are skipped");
        assert!(found.artwork.banner.is_some());
        assert_eq!(found.file_count, 4);
    }

    #[test]
    fn flags_encrypted_console_builds() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("ConsoleBuild");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("MicrosoftGame.config"),
            r#"<Game><ShellVisuals DefaultDisplayName="Console Build" /></Game>"#,
        )
        .unwrap();
        fs::write(root.join("Game.exe"), vec![0xA7u8; 8192]).unwrap();

        let found = inspect_folder(&root).unwrap();
        assert_eq!(found.package, Some(PackageKind::MicrosoftGdk));
        assert_eq!(found.suggested_name, "Console Build");
        assert!(!found.candidates[0].runnable);
        assert_eq!(found.notes.len(), 1, "one clear explanation, not two");
        assert_eq!(found.notes[0].level, NoteLevel::Error);
    }

    #[test]
    fn pretty_names() {
        assert_eq!(pretty_name("grand_theft-auto.vi"), "Grand Theft Auto Vi");
        assert_eq!(pretty_name("DOOM Eternal"), "DOOM Eternal");
        assert_eq!(pretty_name("___"), "New game");
    }
}
