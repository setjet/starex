//! Keeps a game offline with Windows Firewall: an outbound block rule for each of the game's
//! programs, all under one name so they can be removed together. Changing firewall rules needs
//! administrator rights, so Windows asks the user once each time.

use starex_core::model::Game;
use walkdir::WalkDir;

/// The display name every rule for this game shares.
pub fn rule_name(game_id: &str) -> String {
    format!("Starex offline {game_id}")
}

/// The game's programs: its executable and every `.exe` in its folder (up to 64).
fn programs(game: &Game) -> Vec<std::path::PathBuf> {
    let mut found: Vec<std::path::PathBuf> = WalkDir::new(&game.install_dir)
        .max_depth(6)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")))
        .take(64)
        .collect();
    // A batch launcher never opens connections itself; the programs it starts are listed above.
    let is_exe = game
        .executable
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("exe"));
    if is_exe && !found.contains(&game.executable) {
        found.push(game.executable.clone());
    }
    found
}

/// A PowerShell single-quoted string.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

pub fn block_script(game: &Game) -> String {
    let name = quote(&rule_name(&game.id));
    let list = programs(game)
        .iter()
        .map(|p| quote(&p.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "Remove-NetFirewallRule -DisplayName {name} -ErrorAction SilentlyContinue; \
         foreach ($p in @({list})) {{ New-NetFirewallRule -DisplayName {name} -Direction Outbound -Action Block -Program $p | Out-Null }}"
    )
}

pub fn allow_script(game_id: &str) -> String {
    format!(
        "Remove-NetFirewallRule -DisplayName {} -ErrorAction SilentlyContinue",
        quote(&rule_name(game_id))
    )
}

/// Turns blocking on or off. Blocks until the user answers the Windows prompt.
pub fn set(game: &Game, offline: bool) -> Result<(), String> {
    let script = if offline {
        block_script(game)
    } else {
        allow_script(&game.id)
    };
    platform::run_elevated(&script)?;
    if platform::rule_exists(&rule_name(&game.id)) != offline {
        return Err(if offline {
            "Windows Firewall did not add the rule.".into()
        } else {
            "Windows Firewall did not remove the rule.".into()
        });
    }
    Ok(())
}

#[cfg(windows)]
mod platform {
    use std::os::windows::process::CommandExt;

    use base64::Engine;
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{INFINITE, WaitForSingleObject};
    use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
    use windows::core::{HSTRING, PCWSTR, w};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// Runs a PowerShell script as administrator and waits for it. The script is passed
    /// encoded, so no path or name can break out of it.
    pub fn run_elevated(script: &str) -> Result<(), String> {
        let utf16: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let encoded = base64::engine::general_purpose::STANDARD.encode(utf16);
        let parameters = HSTRING::from(format!(
            "-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand {encoded}"
        ));
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: w!("runas"),
            lpFile: w!("powershell.exe"),
            lpParameters: PCWSTR(parameters.as_ptr()),
            nShow: SW_HIDE.0,
            ..Default::default()
        };
        // SAFETY: `info` and the strings it points to live until the call returns; the process
        // handle is waited on and closed exactly once.
        unsafe {
            if ShellExecuteExW(&mut info).is_err() {
                return Err("Windows did not get permission to change the firewall.".into());
            }
            if !info.hProcess.is_invalid() {
                if WaitForSingleObject(info.hProcess, INFINITE) != WAIT_OBJECT_0 {
                    let _ = CloseHandle(info.hProcess);
                    return Err("Changing the firewall did not finish.".into());
                }
                let _ = CloseHandle(info.hProcess);
            }
        }
        Ok(())
    }

    /// Reading rules needs no administrator rights.
    pub fn rule_exists(name: &str) -> bool {
        std::process::Command::new("netsh")
            .args(["advfirewall", "firewall", "show", "rule"])
            .arg(format!("name={name}"))
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn run_elevated(_: &str) -> Result<(), String> {
        Err("Blocking a game from going online is available on Windows.".into())
    }

    pub fn rule_exists(_: &str) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_quote_names_and_paths() {
        assert_eq!(quote("Ember & Ash's"), "'Ember & Ash''s'");
        assert_eq!(
            allow_script("abc"),
            "Remove-NetFirewallRule -DisplayName 'Starex offline abc' -ErrorAction SilentlyContinue"
        );
    }
}
