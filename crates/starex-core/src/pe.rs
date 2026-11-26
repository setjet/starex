//! Reads just enough of a Windows executable's header to know whether it can run here.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    X86,
    X64,
    Arm64,
    Arm,
    /// A macOS universal binary with code for several processors.
    Universal,
    Unknown,
}

impl Arch {
    fn from_machine(machine: u16) -> Self {
        match machine {
            0x014c => Self::X86,
            0x8664 => Self::X64,
            0xaa64 => Self::Arm64,
            0x01c0 | 0x01c4 => Self::Arm,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::X86 => "32-bit x86",
            Self::X64 => "64-bit x64",
            Self::Arm64 => "ARM64",
            Self::Arm => "32-bit ARM",
            Self::Universal => "Universal",
            Self::Unknown => "an unknown processor",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Subsystem {
    Gui,
    Console,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeInfo {
    pub arch: Arch,
    pub subsystem: Subsystem,
    pub is_dll: bool,
}

/// Why a file is not a usable Windows executable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PeProblem {
    /// The file could not be opened.
    Unreadable,
    /// No `MZ`/`PE` signature. Encrypted console executables look like this.
    NotExecutable,
}

/// Can this computer run code for `arch`, and does it need emulation to do so?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HostSupport {
    Native,
    Emulated,
    Unsupported,
    /// A Windows program on Linux or macOS, run through Wine.
    Compat,
}

pub fn host_support(arch: Arch) -> HostSupport {
    host_support_on(std::env::consts::ARCH, arch)
}

pub(crate) fn host_support_on(host: &str, arch: Arch) -> HostSupport {
    use HostSupport::*;
    match (host, arch) {
        ("x86_64", Arch::X64 | Arch::X86) => Native,
        ("x86", Arch::X86) => Native,
        // Windows on ARM runs x86 and x64 programs through its built-in emulator.
        ("aarch64", Arch::Arm64) => Native,
        ("aarch64", Arch::X64 | Arch::X86) => Emulated,
        _ => Unsupported,
    }
}

pub fn read_pe(path: &Path) -> Result<PeInfo, PeProblem> {
    let mut file = File::open(path).map_err(|_| PeProblem::Unreadable)?;
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos).map_err(|_| PeProblem::NotExecutable)?;
    if &dos[0..2] != b"MZ" {
        return Err(PeProblem::NotExecutable);
    }
    let offset = u32::from_le_bytes([dos[60], dos[61], dos[62], dos[63]]) as u64;
    // A COFF header plus the start of the optional header is 24 + 70 bytes.
    let mut nt = [0u8; 94];
    file.seek(SeekFrom::Start(offset))
        .and_then(|_| file.read_exact(&mut nt))
        .map_err(|_| PeProblem::NotExecutable)?;
    if &nt[0..4] != b"PE\0\0" {
        return Err(PeProblem::NotExecutable);
    }
    let u16_at = |i: usize| u16::from_le_bytes([nt[i], nt[i + 1]]);
    let machine = u16_at(4);
    let characteristics = u16_at(22);
    // The subsystem sits 68 bytes into the optional header for both PE32 and PE32+.
    let subsystem = match u16_at(24 + 68) {
        2 => Subsystem::Gui,
        3 => Subsystem::Console,
        _ => Subsystem::Other,
    };
    Ok(PeInfo {
        arch: Arch::from_machine(machine),
        subsystem,
        is_dll: characteristics & 0x2000 != 0,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Builds a minimal PE header for tests.
    pub(crate) fn fake_pe(machine: u16, subsystem: u16) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x80 + 94 + 32];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&0x80u32.to_le_bytes());
        let nt = 0x80;
        bytes[nt..nt + 4].copy_from_slice(b"PE\0\0");
        bytes[nt + 4..nt + 6].copy_from_slice(&machine.to_le_bytes());
        bytes[nt + 22..nt + 24].copy_from_slice(&0x0102u16.to_le_bytes());
        bytes[nt + 24..nt + 26].copy_from_slice(&0x020bu16.to_le_bytes());
        bytes[nt + 24 + 68..nt + 24 + 70].copy_from_slice(&subsystem.to_le_bytes());
        bytes
    }

    #[test]
    fn reads_headers() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("a.exe");
        std::fs::write(&exe, fake_pe(0x8664, 2)).unwrap();
        let info = read_pe(&exe).unwrap();
        assert_eq!(info.arch, Arch::X64);
        assert_eq!(info.subsystem, Subsystem::Gui);
        assert!(!info.is_dll);

        let enc = dir.path().join("enc.exe");
        std::fs::write(&enc, vec![0x5a; 4096]).unwrap();
        assert_eq!(read_pe(&enc), Err(PeProblem::NotExecutable));
        assert_eq!(read_pe(&dir.path().join("none.exe")), Err(PeProblem::Unreadable));
    }

    #[test]
    fn host_matrix() {
        assert_eq!(host_support_on("aarch64", Arch::X64), HostSupport::Emulated);
        assert_eq!(host_support_on("x86_64", Arch::Arm64), HostSupport::Unsupported);
        assert_eq!(host_support_on("x86_64", Arch::X86), HostSupport::Native);
    }
}
