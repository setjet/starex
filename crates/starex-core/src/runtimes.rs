//! Finds the system runtimes a Windows game needs (Visual C++, legacy DirectX, .NET) and
//! whether this computer has them, so a missing one is installed before the game crashes.
//!
//! The check reads which DLLs the game's programs import, then looks for each DLL next to the
//! game and in Windows' system folder. It also finds the matching installer in the game's own
//! redistributables folder (`_CommonRedist` and similar), when the game ships one.

use std::path::{Path, PathBuf};

use serde::Serialize;
use walkdir::WalkDir;

use crate::pe::{self, Arch};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    /// Stable id, e.g. `vc2015-x64`, `directx`, `dotnet-8`.
    pub id: String,
    pub name: String,
    /// DLLs the game imports that this runtime provides and that were not found.
    pub missing: Vec<String>,
    pub installed: bool,
    /// An installer for this runtime inside the game folder.
    pub installer: Option<PathBuf>,
    /// Microsoft's official download page or installer.
    pub download: Option<String>,
}

struct Family {
    key: &'static str,
    name: &'static str,
    matches: fn(&str) -> bool,
}

const FAMILIES: &[Family] = &[
    Family {
        key: "vc2015",
        name: "Visual C++ 2015–2022",
        matches: |dll| {
            matches!(
                dll,
                "vcruntime140.dll"
                    | "vcruntime140_1.dll"
                    | "msvcp140.dll"
                    | "msvcp140_1.dll"
                    | "msvcp140_2.dll"
                    | "msvcp140_atomic_wait.dll"
                    | "concrt140.dll"
                    | "vccorlib140.dll"
            )
        },
    },
    Family {
        key: "vc2013",
        name: "Visual C++ 2013",
        matches: |dll| matches!(dll, "msvcr120.dll" | "msvcp120.dll"),
    },
    Family {
        key: "vc2012",
        name: "Visual C++ 2012",
        matches: |dll| matches!(dll, "msvcr110.dll" | "msvcp110.dll"),
    },
    Family {
        key: "vc2010",
        name: "Visual C++ 2010",
        matches: |dll| matches!(dll, "msvcr100.dll" | "msvcp100.dll"),
    },
    Family {
        key: "directx",
        name: "DirectX End-User Runtime (June 2010)",
        matches: |dll| {
            dll.starts_with("d3dx9_")
                || dll.starts_with("d3dx10")
                || dll.starts_with("d3dx11")
                || dll.starts_with("xapofx1_")
                || dll.starts_with("x3daudio1_")
                || matches!(
                    dll,
                    "xinput1_1.dll"
                        | "xinput1_2.dll"
                        | "xinput1_3.dll"
                        | "xaudio2_0.dll"
                        | "xaudio2_1.dll"
                        | "xaudio2_2.dll"
                        | "xaudio2_3.dll"
                        | "xaudio2_4.dll"
                        | "xaudio2_5.dll"
                        | "xaudio2_6.dll"
                        | "xaudio2_7.dll"
                        | "d3dcompiler_43.dll"
                        | "d3dcompiler_42.dll"
                        | "d3dcompiler_41.dll"
                )
        },
    },
];

fn arch_tag(arch: Arch) -> &'static str {
    match arch {
        Arch::X86 => "x86",
        Arch::Arm64 => "arm64",
        _ => "x64",
    }
}

fn download_for(key: &str, arch: Arch) -> Option<String> {
    let tag = arch_tag(arch);
    Some(match key {
        "vc2015" => format!("https://aka.ms/vs/17/release/vc_redist.{tag}.exe"),
        "vc2013" => format!(
            "https://aka.ms/highdpimfc2013{}enu",
            if tag == "x86" { "x86" } else { "x64" }
        ),
        "vc2012" => "https://www.microsoft.com/en-us/download/details.aspx?id=30679".into(),
        "vc2010" => "https://www.microsoft.com/en-us/download/details.aspx?id=26999".into(),
        "directx" => "https://www.microsoft.com/en-us/download/details.aspx?id=35".into(),
        _ => return None,
    })
}

/// Where Windows keeps system DLLs for programs built for `arch`.
fn system_dir(arch: Arch) -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()));
    match arch {
        Arch::X86 | Arch::Arm => Some(root.join("SysWOW64"))
            .filter(|d| d.is_dir())
            .or(Some(root.join("System32"))),
        _ => Some(root.join("System32")),
    }
}

/// The runtimes `exe` needs on this computer. Empty on Linux and macOS, where Wine brings its own.
pub fn check(exe: &Path, install_dir: &Path) -> Vec<Runtime> {
    if !cfg!(windows) || exe.is_dir() {
        return Vec::new();
    }
    check_with(exe, install_dir, &system_dir, &dotnet_installed)
}

/// The check, with the system lookups passed in so it can be tested anywhere.
pub(crate) fn check_with(
    exe: &Path,
    search_root: &Path,
    system_dir: &dyn Fn(Arch) -> Option<PathBuf>,
    dotnet: &dyn Fn(&str, &str) -> bool,
) -> Vec<Runtime> {
    let Ok(info) = pe::read_pe(exe) else {
        return Vec::new();
    };
    let arch = info.arch;
    let exe_dir = exe.parent().unwrap_or(Path::new("."));

    // The program and the libraries next to it.
    let mut binaries = vec![exe.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(exe_dir) {
        binaries.extend(
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dll")))
                .take(96),
        );
    }
    let mut imported: Vec<String> = binaries.iter().filter_map(|b| pe::imports(b).ok()).flatten().collect();
    imported.sort();
    imported.dedup();

    let present =
        |dll: &str| exe_dir.join(dll).is_file() || system_dir(arch).is_some_and(|dir| dir.join(dll).is_file());
    let installers = find_installers(search_root);

    let mut runtimes = Vec::new();
    for family in FAMILIES {
        let needed: Vec<&String> = imported.iter().filter(|d| (family.matches)(d)).collect();
        if needed.is_empty() {
            continue;
        }
        let missing: Vec<String> = needed.iter().filter(|d| !present(d)).map(|d| (*d).clone()).collect();
        let (id, name) = if family.key == "directx" {
            ("directx".to_owned(), family.name.to_owned())
        } else {
            (
                format!("{}-{}", family.key, arch_tag(arch)),
                format!("{} ({})", family.name, arch_tag(arch)),
            )
        };
        runtimes.push(Runtime {
            installed: missing.is_empty(),
            missing,
            installer: installers
                .iter()
                .find(|(k, a, _)| *k == family.key && (family.key == "directx" || *a == arch_tag(arch)))
                .map(|i| i.2.clone()),
            download: download_for(family.key, arch),
            id,
            name: name.to_owned(),
        });
    }

    // .NET (Core / 5+): a `<program>.runtimeconfig.json` names the framework it needs.
    let config = exe.with_extension("runtimeconfig.json");
    if let Ok(text) = std::fs::read_to_string(&config)
        && let Some((framework, version)) = runtimeconfig_framework(&text)
    {
        let major = version.split('.').next().unwrap_or("").to_owned();
        let installed = dotnet(&framework, &major);
        runtimes.push(Runtime {
            id: format!("dotnet-{major}"),
            name: format!(
                ".NET {major} ({})",
                if framework.contains("Desktop") {
                    "desktop runtime"
                } else {
                    "runtime"
                }
            ),
            missing: if installed {
                Vec::new()
            } else {
                vec![format!("{framework} {version}")]
            },
            installed,
            installer: installers.iter().find(|(k, _, _)| *k == "dotnet").map(|i| i.2.clone()),
            download: Some(format!("https://dotnet.microsoft.com/download/dotnet/{major}.0")),
        });
    }
    runtimes
}

/// Reads `"name"` and `"version"` of the framework from a `.runtimeconfig.json`.
fn runtimeconfig_framework(text: &str) -> Option<(String, String)> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let options = value.get("runtimeOptions")?;
    let framework = options
        .get("framework")
        .or_else(|| options.get("frameworks").and_then(|f| f.get(0)))?;
    Some((
        framework.get("name")?.as_str()?.to_owned(),
        framework.get("version")?.as_str()?.to_owned(),
    ))
}

/// Is a .NET shared framework of this major version installed?
fn dotnet_installed(framework: &str, major: &str) -> bool {
    ["ProgramFiles", "ProgramFiles(x86)"].iter().any(|var| {
        let Some(base) = std::env::var_os(var) else {
            return false;
        };
        let dir = PathBuf::from(base).join("dotnet").join("shared").join(framework);
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .flatten()
                    .any(|e| e.file_name().to_string_lossy().starts_with(&format!("{major}.")))
            })
            .unwrap_or(false)
    })
}

/// Runtime installers shipped with a game: (family key, architecture tag, path).
pub(crate) fn find_installers(root: &Path) -> Vec<(&'static str, &'static str, PathBuf)> {
    let mut found = Vec::new();
    for entry in WalkDir::new(root).max_depth(5).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if !name.ends_with(".exe") {
            continue;
        }
        let path = entry.path().to_string_lossy().to_ascii_lowercase();
        let arch = if name.contains("arm64") {
            "arm64"
        } else if name.contains("x86") {
            "x86"
        } else {
            "x64"
        };
        let key = if name == "dxsetup.exe" {
            Some("directx")
        } else if name.starts_with("ndp") || name.starts_with("dotnet") || name.starts_with("windowsdesktop-runtime") {
            Some("dotnet")
        } else if name.contains("redist") && (name.starts_with("vc") || name.starts_with("vcredist")) {
            if path.contains("2010") {
                Some("vc2010")
            } else if path.contains("2012") {
                Some("vc2012")
            } else if path.contains("2013") {
                Some("vc2013")
            } else {
                // "VC_redist" is the 2015-and-later name; dated folders say so too.
                Some("vc2015")
            }
        } else {
            None
        };
        if let Some(key) = key {
            found.push((key, arch, entry.into_path()));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_redist_installers() {
        let dir = tempfile::tempdir().unwrap();
        let redist = dir.path().join("_CommonRedist");
        fs::create_dir_all(redist.join("vcredist/2019")).unwrap();
        fs::create_dir_all(redist.join("vcredist/2013")).unwrap();
        fs::create_dir_all(redist.join("DirectX/Jun2010")).unwrap();
        fs::write(redist.join("vcredist/2019/VC_redist.x64.exe"), b"").unwrap();
        fs::write(redist.join("vcredist/2019/VC_redist.x86.exe"), b"").unwrap();
        fs::write(redist.join("vcredist/2013/vcredist_x64.exe"), b"").unwrap();
        fs::write(redist.join("DirectX/Jun2010/DXSETUP.exe"), b"").unwrap();
        let found = find_installers(dir.path());
        let has = |k: &str, a: &str| found.iter().any(|(fk, fa, _)| *fk == k && *fa == a);
        assert!(has("vc2015", "x64") && has("vc2015", "x86") && has("vc2013", "x64") && has("directx", "x64"));
    }

    #[test]
    fn reads_dotnet_runtimeconfig() {
        let text =
            r#"{"runtimeOptions":{"tfm":"net8.0","framework":{"name":"Microsoft.NETCore.App","version":"8.0.0"}}}"#;
        assert_eq!(
            runtimeconfig_framework(text),
            Some(("Microsoft.NETCore.App".into(), "8.0.0".into()))
        );
    }

    /// Rust programs built with MSVC load vcruntime140.dll, so this test's own executable is a
    /// real program that needs Visual C++ 2015–2022.
    #[cfg(all(windows, target_env = "msvc"))]
    #[test]
    fn reports_a_missing_runtime_and_its_installer() {
        let exe = std::env::current_exe().unwrap();
        if exe.parent().is_some_and(|d| d.join("vcruntime140.dll").is_file()) {
            return; // Shipped next to the program, so nothing would be missing.
        }
        let dir = tempfile::tempdir().unwrap();
        let empty_system = dir.path().join("system");
        fs::create_dir_all(&empty_system).unwrap();
        let redist = dir.path().join("_CommonRedist/vcredist/2022");
        fs::create_dir_all(&redist).unwrap();
        fs::write(redist.join("VC_redist.x64.exe"), b"").unwrap();
        fs::write(redist.join("VC_redist.arm64.exe"), b"").unwrap();

        let found = check_with(&exe, dir.path(), &|_| Some(empty_system.clone()), &|_, _| true);
        let vc = found
            .iter()
            .find(|r| r.id.starts_with("vc2015"))
            .expect("Visual C++ is needed");
        assert!(!vc.installed);
        assert!(vc.missing.iter().any(|d| d == "vcruntime140.dll"));
        assert!(vc.installer.is_some(), "the installer in _CommonRedist is offered");
        assert!(vc.download.as_deref().is_some_and(|u| u.starts_with("https://aka.ms/")));
    }

    #[cfg(windows)]
    #[test]
    fn a_system_program_needs_nothing_missing() {
        let notepad = Path::new(r"C:\Windows\System32\notepad.exe");
        if notepad.is_file() {
            let found = check_with(notepad, Path::new("."), &system_dir, &|_, _| true);
            assert!(found.iter().all(|r| r.installed), "{found:?}");
        }
    }
}
