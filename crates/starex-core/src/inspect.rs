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

/// Words that mark a batch file as the game's launcher.
const LAUNCH_WORDS: &[&str] = &["launch", "launcher", "start", "play", "run", "game"];

/// Release-group and packaging tags found at the end of downloaded folder names. They are not
/// part of the game's name, so they are dropped before looking the game up.
const RELEASE_TAGS: &[&str] = &[
    "rune",
    "codex",
    "plaza",
    "skidrow",
    "reloaded",
    "flt",
    "fairlight",
    "hoodlum",
    "tenoke",
    "empress",
    "cpy",
    "darksiders",
    "razor1911",
    "prophet",
    "goldberg",
    "elamigos",
    "dodi",
    "fitgirl",
    "repack",
    "repacks",
    "gog",
    "steamrip",
    "kaos",
    "tinyiso",
    "p2p",
    "proper",
    "internal",
    "cracked",
    "crack",
    "cracks",
    "multi",
    "x64",
    "x86",
    "win64",
    "win32",
    "dlc",
    "dlcs",
    "incl",
    "hotfix",
    "rip",
    "iso",
    "rg",
    "xatab",
    "chovka",
    "decepticon",
    "anadius",
    "onlinefix",
    "egs",
    "drmfree",
    "drm",
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
    // A repack or installer: `setup.exe` next to its compressed `.bin` archives.
    let mut root_setup = false;
    let mut root_bins = 0u32;

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
        if entry.depth() == 1 {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if ext == "exe" && stem == "setup" {
                root_setup = true;
            } else if ext == "bin" {
                root_bins += 1;
            }
        }
        match ext.as_str() {
            "exe" if entry.depth() <= EXE_DEPTH && !is_skipped_exe(path) => {
                exes.push((path.to_path_buf(), size, entry.depth()));
            }
            // Ports and fan patches often start through a batch file.
            "bat" | "cmd" if cfg!(windows) && entry.depth() <= EXE_DEPTH && !is_skipped_exe(path) => {
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
    let installer = root_setup && root_bins > 0 && !candidates.iter().any(|c| c.runnable && c.score > 0);
    match package {
        Some(PackageKind::XboxContainer) if runnable => notes.push(Note::new(
            NoteLevel::Info,
            "This folder also holds Xbox console files (.xvc). Starex ignores them and starts the Windows program.",
        )),
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
    } else if installer {
        notes.push(Note::new(
            NoteLevel::Error,
            "This looks like a game installer (setup.exe with .bin archives), so install the game first. Then add the folder it installs to.",
        ));
    } else if candidates.is_empty() {
        notes.push(Note::new(
            NoteLevel::Error,
            if cfg!(windows) {
                "No program (.exe or .bat) was found in this folder."
            } else {
                "No program was found in this folder."
            },
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
            "This is a Windows game. Starex runs it through Wine, so Wine needs to be installed (see Settings > Games).",
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
    if binary::is_batch(&path) && stem_words.iter().any(|w| LAUNCH_WORDS.contains(&w.as_str())) {
        score += 10;
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
    let cleaned = clean_release_name(folder);
    let source = if cleaned.trim().is_empty() {
        folder
    } else {
        cleaned.as_str()
    };
    // Names with spaces keep their dots and hyphens ("Half-Life 2", "S.T.A.L.K.E.R. 2");
    // dotted or underscored names ("Elden.Ring", "grand_theft-auto") use them as spaces.
    let scene_style = !source.contains(' ');
    let spaced: String = source
        .chars()
        .map(|c| match c {
            '_' => ' ',
            '-' | '.' if scene_style => ' ',
            c => c,
        })
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

/// Drops what downloaded folder names add around a game's name: bracketed tags
/// (`[FitGirl Repack]`, `(GOG)`), and trailing versions, build numbers and release groups
/// (`.v1.12-RUNE`, `Build.12345-TENOKE`). Parentheses that are not tags, like `(Test)`, stay.
fn clean_release_name(folder: &str) -> String {
    // 1. Bracketed parts: [..] and {..} always, (..) only when everything inside is a tag.
    let mut kept = String::new();
    let mut chars = folder.chars().peekable();
    while let Some(c) = chars.next() {
        let close = match c {
            '[' => ']',
            '{' => '}',
            '(' => ')',
            _ => {
                kept.push(c);
                continue;
            }
        };
        let mut inner = String::new();
        let mut closed = false;
        for d in chars.by_ref() {
            if d == close {
                closed = true;
                break;
            }
            inner.push(d);
        }
        let all_tags = inner
            .split(|ch: char| !ch.is_alphanumeric() && ch != '.')
            .filter(|w| !w.is_empty())
            .all(|w| is_release_tag(w) || is_version(w) || is_year(w));
        if c == '(' && closed && !all_tags {
            kept.push('(');
            kept.push_str(&inner);
            kept.push(')');
        } else if !closed {
            kept.push(c);
            kept.push_str(&inner);
        } else {
            kept.push(' ');
        }
    }

    // 2. Trailing versions, build numbers and release groups.
    let scene_style = !kept.trim().contains(' ');
    let is_separator = |ch: char| ch == ' ' || ch == '_' || (scene_style && (ch == '.' || ch == '-'));
    let mut words: Vec<&str> = kept.split(is_separator).filter(|w| !w.is_empty()).collect();
    // A trailing " - Group" leaves a lone hyphen behind.
    let numeric = |w: &str| !w.is_empty() && w.chars().all(|ch| ch.is_ascii_digit());
    while let Some(last) = words.last().copied() {
        let lower = last.to_ascii_lowercase();
        if last == "-"
            || is_release_tag(&lower)
            || is_version(&lower)
            || lower.starts_with("multi") && lower[5..].chars().all(|ch| ch.is_ascii_digit())
        {
            words.pop();
            continue;
        }
        // A run of numbers after a version ("v1.12" split into "v1", "12") or after "Build".
        if numeric(last) {
            let mut start = words.len() - 1;
            while start > 0 && numeric(words[start - 1]) {
                start -= 1;
            }
            if start > 0 {
                let prev = words[start - 1].to_ascii_lowercase();
                if is_version(&prev) {
                    words.truncate(start);
                    continue;
                }
                if prev == "build" || prev == "update" {
                    words.truncate(start - 1);
                    continue;
                }
            }
        }
        break;
    }
    let joined = words.join(if scene_style { "." } else { " " });
    joined
        .trim_matches(|ch: char| ch == '-' || ch == ' ' || (scene_style && ch == '.'))
        .to_owned()
}

fn is_release_tag(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    RELEASE_TAGS.contains(&lower.as_str())
}

/// `v1`, `v1.12`, `V2.0.3`, or a dotted number like `1.0.2`.
fn is_version(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    let digits = lower.strip_prefix('v').unwrap_or(&lower);
    let dotted = digits.contains('.');
    !digits.is_empty()
        && digits.starts_with(|ch: char| ch.is_ascii_digit())
        && digits.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
        && (lower.starts_with('v') || dotted)
}

fn is_year(word: &str) -> bool {
    word.len() == 4 && word.chars().all(|ch| ch.is_ascii_digit()) && (word.starts_with("19") || word.starts_with("20"))
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

    #[cfg(windows)]
    #[test]
    fn offers_batch_launchers() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("GTA IV PC Port");
        fs::create_dir_all(root.join("Game")).unwrap();
        let mut big = fake_pe(0x8664, 2);
        big.resize(4 * 1_048_576, 0);
        fs::write(root.join("Game/GTAIV.exe"), &big).unwrap();
        fs::write(root.join("Launch.bat"), "@echo off\r\nstart Game\\GTAIV.exe\r\n").unwrap();
        fs::write(root.join("uninstall.bat"), "@echo off\r\n").unwrap();

        let found = inspect_folder(&root).unwrap();
        assert_eq!(found.candidates[0].relative, "Game/GTAIV.exe");
        let bat = found
            .candidates
            .iter()
            .find(|c| c.relative == "Launch.bat")
            .expect("batch offered");
        assert!(bat.runnable);
        assert!(found.candidates.iter().all(|c| c.relative != "uninstall.bat"));
    }

    #[test]
    fn explains_uninstalled_repacks() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Hollow Knight [FitGirl Repack]");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("setup.exe"), fake_pe(0x014c, 2)).unwrap();
        fs::write(root.join("fg-01.bin"), [0u8; 64]).unwrap();

        let found = inspect_folder(&root).unwrap();
        assert_eq!(found.suggested_name, "Hollow Knight");
        assert!(
            found.notes.iter().any(|n| n.text.contains("installer")),
            "{:?}",
            found.notes
        );
    }

    #[test]
    fn ignores_stray_console_files_next_to_a_program() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Port");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Port.exe"), fake_pe(0x8664, 2)).unwrap();
        fs::write(root.join("leftover.xvc"), b"xvc").unwrap();

        let found = inspect_folder(&root).unwrap();
        assert!(found.candidates[0].runnable);
        assert!(
            found.notes.iter().all(|n| n.level != NoteLevel::Error),
            "{:?}",
            found.notes
        );
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

    #[test]
    fn cleans_release_style_names() {
        assert_eq!(pretty_name("Elden.Ring.v1.12-RUNE"), "Elden Ring");
        assert_eq!(pretty_name("Hollow Knight [FitGirl Repack]"), "Hollow Knight");
        assert_eq!(pretty_name("Cyberpunk.2077.v2.1-GOG"), "Cyberpunk 2077");
        assert_eq!(pretty_name("Some.Game.Build.12345-TENOKE"), "Some Game");
        assert_eq!(pretty_name("Elden Ring v1.12 (GOG)"), "Elden Ring");
        assert_eq!(pretty_name("Hades.II.MULTi12-ElAmigos"), "Hades II");
        assert_eq!(pretty_name("Prey (2017) [DODI Repack]"), "Prey");
        assert_eq!(pretty_name("Stardew_Valley_v1.6.9_GOG"), "Stardew Valley");
        // Names that are not release-style stay as they are.
        assert_eq!(pretty_name("Half-Life 2"), "Half-Life 2");
        assert_eq!(pretty_name("S.T.A.L.K.E.R. 2"), "S.T.A.L.K.E.R. 2");
        assert_eq!(pretty_name("GTA IV PC Port"), "GTA IV PC Port");
        assert_eq!(pretty_name("Portal 2"), "Portal 2");
        assert_eq!(pretty_name("ゲーム Ünïcødé & (Test)"), "ゲーム Ünïcødé & (Test)");
        assert_eq!(pretty_name("DOOM Eternal"), "DOOM Eternal");
    }
}
