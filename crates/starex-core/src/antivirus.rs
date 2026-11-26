//! Virus scanning with the antivirus already on the computer.
//!
//! Starex does not ship its own virus definitions. It hands the game's code files to:
//! - **Windows**: the Antimalware Scan Interface (AMSI), which passes them to Microsoft Defender
//!   or whichever antivirus is registered with Windows;
//! - **Linux and macOS**: ClamAV (`clamscan`), when it is installed.
//!
//! Only files that can run code are scanned (programs, libraries, drivers and scripts). Game data
//! such as textures, audio and video cannot run by themselves, and skipping them keeps a scan of
//! a 100 GB game down to seconds.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::manifest::{Progress, Tracker, relative};
use crate::util::now_secs;

/// Files larger than this are skipped and reported as skipped.
const MAX_FILE: u64 = 512 * 1024 * 1024;

/// Extensions of files that can run code.
const CODE_EXTENSIONS: &[&str] = &[
    "exe", "dll", "sys", "com", "scr", "cpl", "ocx", "drv", "efi", "msi", "msp", "bat", "cmd", "ps1", "psm1", "vbs",
    "vbe", "js", "jse", "wsf", "wsh", "hta", "jar", "lnk", "reg", "so", "dylib", "sh", "command", "app",
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ScanStatus {
    Clean,
    Threats,
    /// No antivirus could be reached.
    Unavailable,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Threat {
    /// Relative to the game folder.
    pub path: String,
    /// The antivirus's name for the threat, when it gives one.
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub game_id: String,
    pub status: ScanStatus,
    pub engine: String,
    pub files_scanned: u64,
    pub bytes_scanned: u64,
    pub skipped: u64,
    pub threats: Vec<Threat>,
    /// A sentence for the user when the scan could not run or was incomplete.
    pub message: Option<String>,
    pub duration_ms: u64,
    pub at: u64,
}

/// What the library keeps from the latest scan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub at: u64,
    pub status: ScanStatus,
    pub threats: u64,
    pub files_scanned: u64,
}

impl ScanReport {
    pub fn summary(&self) -> ScanSummary {
        ScanSummary {
            at: self.at,
            status: self.status,
            threats: self.threats.len() as u64,
            files_scanned: self.files_scanned,
        }
    }
}

/// The code files under `root`, with their sizes.
pub fn targets(root: &Path) -> Vec<(PathBuf, u64)> {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            let size = e.metadata().ok()?.len();
            is_code(e.path(), &e).then(|| (e.into_path(), size))
        })
        .collect()
}

fn is_code(path: &Path, _entry: &walkdir::DirEntry) -> bool {
    let by_extension = path
        .extension()
        .map(|x| CODE_EXTENSIONS.contains(&x.to_string_lossy().to_ascii_lowercase().as_str()))
        .unwrap_or(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        by_extension
            || _entry
                .metadata()
                .map(|m| m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
    }
    #[cfg(not(unix))]
    by_extension
}

/// Scans the code files of the game in `root`.
pub fn scan(game_id: &str, root: &Path, cancel: &AtomicBool, on_progress: &(dyn Fn(Progress) + Sync)) -> ScanReport {
    let started = Instant::now();
    let all = targets(root);
    let (files, too_big): (Vec<_>, Vec<_>) = all.into_iter().partition(|(_, size)| *size <= MAX_FILE);
    let total_bytes = files.iter().map(|f| f.1).sum();
    let tracker = Tracker::new(files.len() as u64, total_bytes, on_progress);

    let outcome = backend::scan(root, &files, cancel, &tracker);
    tracker.emit();

    let (status, engine, threats, scanned, bytes, mut message) = match outcome {
        Outcome::Done {
            engine,
            threats,
            scanned,
            bytes,
        } => {
            let status = if cancel.load(Ordering::Relaxed) {
                ScanStatus::Cancelled
            } else if threats.is_empty() {
                ScanStatus::Clean
            } else {
                ScanStatus::Threats
            };
            (status, engine, threats, scanned, bytes, None)
        }
        Outcome::Unavailable { engine, message } => (ScanStatus::Unavailable, engine, Vec::new(), 0, 0, Some(message)),
    };
    if !too_big.is_empty() && status != ScanStatus::Unavailable {
        message = Some(format!(
            "{} larger than 512 MB {} skipped.",
            too_big.len(),
            if too_big.len() == 1 { "file was" } else { "files were" }
        ));
    }

    ScanReport {
        game_id: game_id.to_owned(),
        status,
        engine,
        files_scanned: scanned,
        bytes_scanned: bytes,
        skipped: too_big.len() as u64,
        threats,
        message,
        duration_ms: started.elapsed().as_millis() as u64,
        at: now_secs(),
    }
}

enum Outcome {
    Done {
        engine: String,
        threats: Vec<Threat>,
        scanned: u64,
        bytes: u64,
    },
    Unavailable {
        engine: String,
        message: String,
    },
}

#[cfg(windows)]
mod backend {
    use super::*;
    use windows::Win32::System::Antimalware::{
        AMSI_RESULT_DETECTED, AmsiInitialize, AmsiScanBuffer, AmsiUninitialize, HAMSICONTEXT,
    };
    use windows::core::{PCWSTR, w};

    const ENGINE: &str = "Windows antivirus (AMSI)";

    /// An AMSI context, released on drop.
    pub(super) struct Amsi(HAMSICONTEXT);

    impl Amsi {
        pub(super) fn new() -> Option<Self> {
            // SAFETY: AmsiInitialize takes a valid, null-terminated app name.
            unsafe { AmsiInitialize(w!("Starex")) }.ok().map(Self)
        }

        /// `Ok(true)` when the antivirus says `data` is malicious.
        pub(super) fn is_malicious(&self, data: &[u8], name: &str) -> windows::core::Result<bool> {
            let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let length = u32::try_from(data.len()).unwrap_or(u32::MAX);
            // SAFETY: `data` and `wide` outlive the call; AMSI only reads them.
            let result = unsafe { AmsiScanBuffer(self.0, data.as_ptr().cast(), length, PCWSTR(wide.as_ptr()), None) }?;
            Ok(result.0 >= AMSI_RESULT_DETECTED.0)
        }
    }

    impl Drop for Amsi {
        fn drop(&mut self) {
            // SAFETY: the context came from AmsiInitialize and is released once.
            unsafe { AmsiUninitialize(self.0) }
        }
    }

    pub(super) fn scan(root: &Path, files: &[(PathBuf, u64)], cancel: &AtomicBool, tracker: &Tracker) -> Outcome {
        let Some(amsi) = Amsi::new() else {
            return Outcome::Unavailable {
                engine: ENGINE.into(),
                message: "Windows could not start a virus scan. Check that Microsoft Defender or another antivirus is turned on.".into(),
            };
        };
        let mut threats = Vec::new();
        let (mut scanned, mut bytes, mut failures) = (0u64, 0u64, 0u64);
        for (path, size) in files {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let rel = relative(root, path);
            match std::fs::read(path) {
                Ok(data) => match amsi.is_malicious(&data, &rel) {
                    Ok(true) => threats.push(Threat { path: rel, name: None }),
                    Ok(false) => {}
                    Err(_) => failures += 1,
                },
                // An antivirus that already quarantined or locked the file refuses the read.
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    threats.push(Threat {
                        path: rel,
                        name: Some("Blocked by your antivirus".into()),
                    });
                }
                Err(_) => failures += 1,
            }
            scanned += 1;
            bytes += size;
            tracker.add_bytes(*size);
            tracker.file_done();
        }
        if scanned > 0 && failures == scanned {
            return Outcome::Unavailable {
                engine: ENGINE.into(),
                message: "Your antivirus did not answer. Check that it is turned on, then scan again.".into(),
            };
        }
        Outcome::Done {
            engine: ENGINE.into(),
            threats,
            scanned: scanned - failures,
            bytes,
        }
    }
}

#[cfg(not(windows))]
mod backend {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    const ENGINE: &str = "ClamAV";

    /// Finds `clamscan`. Desktop apps on macOS do not inherit the shell's PATH, so the usual
    /// Homebrew and system locations are checked too.
    fn clamscan() -> Option<PathBuf> {
        let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        dirs.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
        dirs.into_iter().map(|d| d.join("clamscan")).find(|p| p.is_file())
    }

    pub(super) fn scan(root: &Path, files: &[(PathBuf, u64)], cancel: &AtomicBool, tracker: &Tracker) -> Outcome {
        let Some(binary) = clamscan() else {
            return Outcome::Unavailable {
                engine: ENGINE.into(),
                message: if cfg!(target_os = "macos") {
                    "No virus scanner found. Install ClamAV (brew install clamav) and Starex will use it."
                } else {
                    "No virus scanner found. Install ClamAV (for example sudo apt install clamav) and Starex will use it."
                }
                .into(),
            };
        };
        if files.is_empty() {
            return Outcome::Done {
                engine: ENGINE.into(),
                threats: Vec::new(),
                scanned: 0,
                bytes: 0,
            };
        }

        let list = std::env::temp_dir().join(format!("starex-scan-{}.txt", std::process::id()));
        let written = std::fs::File::create(&list).and_then(|mut f| {
            for (path, _) in files {
                writeln!(f, "{}", path.display())?;
            }
            Ok(())
        });
        if let Err(e) = written {
            return Outcome::Unavailable {
                engine: ENGINE.into(),
                message: format!("Could not prepare the scan: {e}"),
            };
        }

        let child = Command::new(&binary)
            .arg("--no-summary")
            .arg("--infected")
            .arg(format!("--file-list={}", list.display()))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                let _ = std::fs::remove_file(&list);
                return Outcome::Unavailable {
                    engine: ENGINE.into(),
                    message: format!("ClamAV could not start: {e}"),
                };
            }
        };
        loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                break;
            }
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => break,
                Ok(None) => std::thread::sleep(Duration::from_millis(150)),
            }
        }
        let output = child.wait_with_output();
        let _ = std::fs::remove_file(&list);
        let Ok(output) = output else {
            return Outcome::Unavailable {
                engine: ENGINE.into(),
                message: "ClamAV stopped unexpectedly.".into(),
            };
        };
        // Exit codes: 0 clean, 1 threats found, anything else an error.
        match output.status.code() {
            Some(0) | Some(1) => {}
            _ if cancel.load(Ordering::Relaxed) => {}
            _ => {
                let err = String::from_utf8_lossy(&output.stderr);
                return Outcome::Unavailable {
                    engine: ENGINE.into(),
                    message: format!(
                        "ClamAV could not scan: {}",
                        err.lines()
                            .next()
                            .unwrap_or("unknown error. Try running freshclam to update it.")
                    ),
                };
            }
        }
        let threats = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let line = line.strip_suffix(" FOUND")?;
                let (path, name) = line.rsplit_once(": ")?;
                Some(Threat {
                    path: relative(root, Path::new(path)),
                    name: Some(name.to_owned()),
                })
            })
            .collect();
        let bytes: u64 = files.iter().map(|f| f.1).sum();
        tracker.add_bytes(bytes);
        Outcome::Done {
            engine: ENGINE.into(),
            threats,
            scanned: files.len() as u64,
            bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scans_only_code_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("bin")).unwrap();
        fs::write(dir.path().join("bin/game.exe"), b"MZ").unwrap();
        fs::write(dir.path().join("bin/engine.DLL"), b"MZ").unwrap();
        fs::write(dir.path().join("data.pak"), vec![0u8; 1024]).unwrap();
        fs::write(dir.path().join("intro.bk2"), b"video").unwrap();
        let mut found: Vec<String> = targets(dir.path())
            .iter()
            .map(|(p, _)| relative(dir.path(), p))
            .collect();
        found.sort();
        assert_eq!(found, ["bin/engine.DLL", "bin/game.exe"]);
    }

    #[test]
    fn a_clean_folder_is_clean_or_reports_why_not() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("tool.bat"), b"@echo hello").unwrap();
        let report = scan("g", dir.path(), &AtomicBool::new(false), &|_| {});
        assert!(report.threats.is_empty());
        match report.status {
            ScanStatus::Clean => assert_eq!(report.files_scanned, 1),
            ScanStatus::Unavailable => assert!(report.message.is_some()),
            other => panic!("unexpected {other:?}"),
        }
    }

    /// Uses the EICAR test string, which every antivirus flags on purpose. It is built in memory
    /// and never written to disk. Needs a working antivirus, so it only runs on request:
    /// `cargo test -p starex-core -- --ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn windows_antivirus_flags_the_eicar_test_string() {
        let reversed = "*H+H$!ELIF-TSET-SURIVITNA-DRADNATS-RACIE$}7)CC7)^P(45XZP\\4[PA@%P!O5X";
        let eicar: String = reversed.chars().rev().collect();
        let amsi = backend::Amsi::new().expect("AMSI is available");
        assert!(amsi.is_malicious(eicar.as_bytes(), "eicar.txt").unwrap());
        assert!(!amsi.is_malicious(b"hello", "hello.txt").unwrap());
    }
}
