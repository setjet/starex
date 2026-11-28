//! Desktop and Start menu shortcuts that start a game through Starex, and the
//! `starex://play/<game>` links that do the same from anywhere (a browser, a Stream Deck).

use std::path::PathBuf;

use serde::Deserialize;
use starex_core::model::Game;
use starex_core::saves;

pub const SCHEME: &str = if starex_core::TEST_BUILD.is_some() {
    "starex-test"
} else {
    "starex"
};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Place {
    Desktop,
    StartMenu,
}

/// The link that starts `game` through Starex.
pub fn link(game_id: &str) -> String {
    format!("{SCHEME}://play/{game_id}")
}

/// The game a launch request names: `--play <id>` on the command line, or a `starex://play/<id>`
/// link. Only plain game ids (letters and digits) are accepted.
pub fn requested_game<I: IntoIterator<Item = String>>(args: I) -> Option<String> {
    let args: Vec<String> = args.into_iter().collect();
    let id = args
        .iter()
        .position(|a| a == "--play")
        .and_then(|i| args.get(i + 1).cloned())
        .or_else(|| args.iter().find_map(|a| game_from_link(a)))?;
    valid_id(&id).then_some(id)
}

pub fn game_from_link(url: &str) -> Option<String> {
    let rest = url.strip_prefix(&format!("{SCHEME}://play/"))?;
    let id = rest.trim_end_matches('/').to_owned();
    valid_id(&id).then_some(id)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Starex's own program: the AppImage on Linux, the running executable elsewhere.
fn starex_program() -> Result<PathBuf, String> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Ok(PathBuf::from(appimage));
    }
    std::env::current_exe().map_err(|e| format!("Could not find Starex's own program: {e}"))
}

/// Creates a shortcut that starts `game` through Starex and returns where it was saved.
pub fn create(game: &Game, place: Place) -> Result<PathBuf, String> {
    let name = saves::folder_name(&game.name);
    platform::create(game, place, &name, &starex_program()?)
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::path::Path;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::{HSTRING, Interface};

    pub fn create(game: &Game, place: Place, name: &str, starex: &Path) -> Result<PathBuf, String> {
        let folder = match place {
            Place::Desktop => dirs::desktop_dir().ok_or("Could not find your desktop folder.")?,
            Place::StartMenu => dirs::data_dir()
                .ok_or("Could not find your Start menu folder.")?
                .join(r"Microsoft\Windows\Start Menu\Programs\Starex Games"),
        };
        std::fs::create_dir_all(&folder).map_err(|e| format!("Could not create {}: {e}", folder.display()))?;
        let file = folder.join(format!("{name}.lnk"));

        // Shell links are COM objects: create them on a thread of their own.
        let (file_c, starex_c, game_c) = (file.clone(), starex.to_path_buf(), game.clone());
        std::thread::spawn(move || write_link(&file_c, &starex_c, &game_c))
            .join()
            .map_err(|_| "Creating the shortcut failed unexpectedly.".to_owned())??;
        Ok(file)
    }

    fn write_link(file: &Path, starex: &Path, game: &Game) -> Result<(), String> {
        // SAFETY: COM is initialised for this thread and released at the end; every pointer
        // passed in is a valid, null-terminated HSTRING that outlives the call.
        unsafe {
            let initialised = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            let result = (|| -> windows::core::Result<()> {
                let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
                link.SetPath(&HSTRING::from(starex.as_os_str()))?;
                link.SetArguments(&HSTRING::from(format!("--play {}", game.id)))?;
                link.SetDescription(&HSTRING::from(format!("Play {} with Starex", game.name)))?;
                if let Some(dir) = starex.parent() {
                    link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
                }
                // The game's own icon, from its executable.
                let icon = if game.executable.is_file() {
                    &game.executable
                } else {
                    starex
                };
                link.SetIconLocation(&HSTRING::from(icon.as_os_str()), 0)?;
                link.cast::<IPersistFile>()?
                    .Save(&HSTRING::from(file.as_os_str()), true)
            })();
            if initialised {
                CoUninitialize();
            }
            result.map_err(|e| format!("Could not create the shortcut: {e}"))
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::path::Path;

    pub fn create(game: &Game, place: Place, name: &str, starex: &Path) -> Result<PathBuf, String> {
        let (folder, file_name) = match place {
            Place::Desktop => (
                dirs::desktop_dir().ok_or("Could not find your desktop folder.")?,
                format!("{name}.desktop"),
            ),
            Place::StartMenu => (
                dirs::data_dir()
                    .ok_or("Could not find your applications folder.")?
                    .join("applications"),
                format!("starex-{}.desktop", game.id),
            ),
        };
        std::fs::create_dir_all(&folder).map_err(|e| format!("Could not create {}: {e}", folder.display()))?;
        let icon = game
            .artwork
            .icon
            .as_ref()
            .or(game.artwork.cover.as_ref())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "starex".into());
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName={}\nComment=Play {} with Starex\nExec=\"{}\" --play {}\nIcon={icon}\nTerminal=false\nCategories=Game;\n",
            game.name.replace('\n', " "),
            game.name.replace('\n', " "),
            starex.display(),
            game.id
        );
        let file = folder.join(file_name);
        std::fs::write(&file, entry).map_err(|e| format!("Could not save the shortcut: {e}"))?;
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755));
        Ok(file)
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::*;
    use std::path::Path;

    pub fn create(_: &Game, _: Place, _: &str, _: &Path) -> Result<PathBuf, String> {
        Err("On macOS, use Copy launch link and open it from Spotlight, a browser or a Stream Deck.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_launch_requests() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            requested_game(args(&["starex.exe", "--play", "abc123"])),
            Some("abc123".into())
        );
        assert_eq!(
            requested_game(args(&["starex.exe", "starex://play/abc123/"])),
            Some("abc123".into())
        );
        assert_eq!(requested_game(args(&["starex.exe", "--play", "../evil"])), None);
        assert_eq!(requested_game(args(&["starex.exe"])), None);
        assert_eq!(link("abc"), "starex://play/abc");
    }
}
