//! Recognises the programs Starex can start on each platform.
//!
//! | Format | Runs on |
//! | --- | --- |
//! | Windows PE (`.exe`) | Windows natively; Linux and macOS through Wine |
//! | ELF | Linux |
//! | Mach-O and `.app` bundles | macOS (x64 programs through Rosetta on Apple silicon) |
//! | Scripts starting with `#!` | Linux and macOS |
//! | Batch launchers (`.bat`, `.cmd`) | Windows |

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::pe::{self, Arch, HostSupport, PeProblem, Subsystem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Format {
    Pe,
    Elf,
    MachO,
    AppBundle,
    Script,
    /// A Windows batch launcher (`.bat` or `.cmd`), run by the command prompt.
    Batch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Binary {
    pub format: Format,
    pub arch: Arch,
    /// Windows programs only: window or console.
    pub subsystem: Option<Subsystem>,
    /// A library (DLL, shared object) rather than a program.
    pub is_library: bool,
}

/// Reads enough of `path` to know what kind of program it is.
pub fn probe(path: &Path) -> Result<Binary, PeProblem> {
    if is_app_bundle(path) {
        let inner = app_executable(path).ok_or(PeProblem::NotExecutable)?;
        let binary = probe(&inner)?;
        return Ok(Binary {
            format: Format::AppBundle,
            ..binary
        });
    }
    if is_batch(path) {
        File::open(path).map_err(|_| PeProblem::Unreadable)?;
        return Ok(Binary {
            format: Format::Batch,
            arch: Arch::Unknown,
            subsystem: None,
            is_library: false,
        });
    }
    let mut head = [0u8; 64];
    let n = File::open(path)
        .and_then(|mut f| f.read(&mut head))
        .map_err(|_| PeProblem::Unreadable)?;
    let head = &head[..n];

    if head.starts_with(b"MZ") {
        let info = pe::read_pe(path)?;
        return Ok(Binary {
            format: Format::Pe,
            arch: info.arch,
            subsystem: Some(info.subsystem),
            is_library: info.is_dll,
        });
    }
    if head.starts_with(b"\x7fELF") && head.len() >= 20 {
        let little = head[5] == 1;
        let read16 = |i: usize| {
            if little {
                u16::from_le_bytes([head[i], head[i + 1]])
            } else {
                u16::from_be_bytes([head[i], head[i + 1]])
            }
        };
        let arch = match read16(18) {
            0x3e => Arch::X64,
            0xb7 => Arch::Arm64,
            0x03 => Arch::X86,
            0x28 => Arch::Arm,
            _ => Arch::Unknown,
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        return Ok(Binary {
            format: Format::Elf,
            arch,
            subsystem: None,
            is_library: name.contains(".so"),
        });
    }
    if head.len() >= 8 {
        let magic_le = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        let magic_be = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
        if magic_le == 0xfeed_facf || magic_le == 0xfeed_face {
            let arch = match u32::from_le_bytes([head[4], head[5], head[6], head[7]]) {
                0x0100_0007 => Arch::X64,
                0x0100_000c => Arch::Arm64,
                7 => Arch::X86,
                _ => Arch::Unknown,
            };
            let is_library = path.extension().is_some_and(|e| e == "dylib");
            return Ok(Binary {
                format: Format::MachO,
                arch,
                subsystem: None,
                is_library,
            });
        }
        // A universal binary holds several architectures, normally including this Mac's.
        if magic_be == 0xcafe_babe && u32::from_be_bytes([head[4], head[5], head[6], head[7]]) < 20 {
            return Ok(Binary {
                format: Format::MachO,
                arch: Arch::Universal,
                subsystem: None,
                is_library: path.extension().is_some_and(|e| e == "dylib"),
            });
        }
    }
    if head.starts_with(b"#!") {
        return Ok(Binary {
            format: Format::Script,
            arch: Arch::Unknown,
            subsystem: None,
            is_library: false,
        });
    }
    Err(PeProblem::NotExecutable)
}

/// Whether this computer can run `binary`, and how.
pub fn support(binary: &Binary) -> HostSupport {
    support_on(std::env::consts::OS, std::env::consts::ARCH, binary)
}

pub(crate) fn support_on(os: &str, host_arch: &str, binary: &Binary) -> HostSupport {
    use HostSupport::*;
    if binary.is_library {
        return Unsupported;
    }
    match (os, binary.format) {
        ("windows", Format::Pe) => pe::host_support_on(host_arch, binary.arch),
        ("windows", Format::Batch) => Native,
        ("windows", _) => Unsupported,
        (_, Format::Pe) => Compat,
        (_, Format::Script) => Native,
        ("linux", Format::Elf) => match (host_arch, binary.arch) {
            ("x86_64", Arch::X64 | Arch::X86) | ("aarch64", Arch::Arm64) => Native,
            _ => Unsupported,
        },
        ("macos", Format::MachO | Format::AppBundle) => match (host_arch, binary.arch) {
            (_, Arch::Universal) => Native,
            ("aarch64", Arch::Arm64) | ("x86_64", Arch::X64) => Native,
            ("aarch64", Arch::X64) => Emulated,
            _ => Unsupported,
        },
        _ => Unsupported,
    }
}

/// A `.bat` or `.cmd` file: how ports and fan patches often start a game on Windows.
pub fn is_batch(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("bat") || e.eq_ignore_ascii_case("cmd"))
}

fn is_app_bundle(path: &Path) -> bool {
    path.is_dir() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("app"))
}

/// The program inside a macOS `.app` bundle: `CFBundleExecutable` from `Info.plist`, or the
/// only file in `Contents/MacOS`.
pub fn app_executable(bundle: &Path) -> Option<PathBuf> {
    let macos = bundle.join("Contents").join("MacOS");
    if let Ok(plist) = std::fs::read_to_string(bundle.join("Contents").join("Info.plist")) {
        let key = "<key>CFBundleExecutable</key>";
        if let Some(rest) = plist.find(key).map(|i| &plist[i + key.len()..])
            && let (Some(start), Some(end)) = (rest.find("<string>"), rest.find("</string>"))
        {
            let name = rest[start + 8..end].trim();
            let candidate = macos.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let mut files = std::fs::read_dir(&macos).ok()?.flatten().filter(|e| e.path().is_file());
    let first = files.next()?.path();
    files.next().is_none().then_some(first)
}

/// What a launcher shows for how a program runs here.
pub fn runs_via(binary: &Binary) -> Option<&'static str> {
    match support(binary) {
        HostSupport::Emulated if cfg!(target_os = "macos") => Some("Rosetta"),
        HostSupport::Emulated => Some("emulation"),
        HostSupport::Compat => Some("Wine"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pe::tests::fake_pe;
    use std::fs;

    fn elf(machine: u16) -> Vec<u8> {
        let mut b = vec![0u8; 64];
        b[..4].copy_from_slice(b"\x7fELF");
        b[4] = 2;
        b[5] = 1;
        b[18..20].copy_from_slice(&machine.to_le_bytes());
        b
    }

    fn macho(cpu: u32) -> Vec<u8> {
        let mut b = vec![0u8; 64];
        b[..4].copy_from_slice(&0xfeed_facf_u32.to_le_bytes());
        b[4..8].copy_from_slice(&cpu.to_le_bytes());
        b
    }

    #[test]
    fn recognises_formats() {
        let dir = tempfile::tempdir().unwrap();
        let cases: [(&str, Vec<u8>, Format, Arch); 4] = [
            ("game.exe", fake_pe(0x8664, 2), Format::Pe, Arch::X64),
            ("game.x86_64", elf(0x3e), Format::Elf, Arch::X64),
            ("game", macho(0x0100_000c), Format::MachO, Arch::Arm64),
            (
                "start.sh",
                b"#!/bin/sh\nexec ./game\n".to_vec(),
                Format::Script,
                Arch::Unknown,
            ),
        ];
        for (name, bytes, format, arch) in cases {
            let path = dir.path().join(name);
            fs::write(&path, bytes).unwrap();
            let b = probe(&path).unwrap();
            assert_eq!((b.format, b.arch), (format, arch), "{name}");
        }
        fs::write(dir.path().join("data.pak"), [0u8; 64]).unwrap();
        assert_eq!(probe(&dir.path().join("data.pak")), Err(PeProblem::NotExecutable));
        fs::write(dir.path().join("libfoo.so.1"), elf(0x3e)).unwrap();
        assert!(probe(&dir.path().join("libfoo.so.1")).unwrap().is_library);
    }

    #[test]
    fn reads_app_bundles() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Game.app");
        fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        fs::write(
            app.join("Contents/Info.plist"),
            "<dict><key>CFBundleExecutable</key>\n<string>RealGame</string></dict>",
        )
        .unwrap();
        fs::write(app.join("Contents/MacOS/RealGame"), macho(0x0100_0007)).unwrap();
        fs::write(app.join("Contents/MacOS/helper"), b"x").unwrap();
        assert_eq!(app_executable(&app), Some(app.join("Contents/MacOS/RealGame")));
        let b = probe(&app).unwrap();
        assert_eq!((b.format, b.arch), (Format::AppBundle, Arch::X64));
    }

    #[test]
    fn support_matrix() {
        let bin = |format, arch| Binary {
            format,
            arch,
            subsystem: None,
            is_library: false,
        };
        assert_eq!(
            support_on("windows", "x86_64", &bin(Format::Pe, Arch::X64)),
            HostSupport::Native
        );
        assert_eq!(
            support_on("windows", "x86_64", &bin(Format::Elf, Arch::X64)),
            HostSupport::Unsupported
        );
        assert_eq!(
            support_on("linux", "x86_64", &bin(Format::Pe, Arch::X64)),
            HostSupport::Compat
        );
        assert_eq!(
            support_on("linux", "x86_64", &bin(Format::Elf, Arch::X64)),
            HostSupport::Native
        );
        assert_eq!(
            support_on("linux", "x86_64", &bin(Format::Elf, Arch::Arm64)),
            HostSupport::Unsupported
        );
        assert_eq!(
            support_on("macos", "aarch64", &bin(Format::AppBundle, Arch::X64)),
            HostSupport::Emulated
        );
        assert_eq!(
            support_on("macos", "aarch64", &bin(Format::MachO, Arch::Universal)),
            HostSupport::Native
        );
        assert_eq!(
            support_on("macos", "x86_64", &bin(Format::Script, Arch::Unknown)),
            HostSupport::Native
        );
    }
}
