//! Builds the command that starts a game.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::binary::{self, Format};
use crate::model::Game;
use crate::{Error, Result};

/// Splits a launch-options string the way a shell would: spaces separate arguments, and double or
/// single quotes keep spaces inside one argument.
///
/// `-windowed "-profile=My Save" +set fov 90` becomes `-windowed`, `-profile=My Save`, `+set`, `fov`, `90`.
pub fn split_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in input.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (None, c) => current.push(c),
        }
    }
    if started || !current.is_empty() {
        args.push(current);
    }
    args
}

/// The command for `game`, ready to spawn. Output is discarded so a chatty game cannot block on
/// a full pipe.
///
/// On Linux and macOS a Windows game runs through `wine`, the path from [`find_wine`]. A macOS
/// `.app` bundle starts the program inside it.
pub fn command(game: &Game, wine: Option<&Path>) -> Result<Command> {
    let exe = &game.executable;
    if !exe.exists() {
        return Err(Error::invalid(format!(
            "{} is missing. Verify the game files or choose the executable again in Properties.",
            exe.display()
        )));
    }
    let target = if exe.is_dir() {
        binary::app_executable(exe)
            .ok_or_else(|| Error::invalid(format!("{} has no program inside it.", exe.display())))?
    } else {
        exe.clone()
    };
    let needs_wine = !cfg!(windows) && binary::probe(&target).is_ok_and(|b| b.format == Format::Pe);
    let cwd = working_dir(game);

    let mut cmd = if needs_wine {
        let wine = wine.ok_or_else(|| {
            Error::invalid(
                "This is a Windows game, so it needs Wine. Install Wine, or set where it is in Settings > Games.",
            )
        })?;
        let mut cmd = Command::new(wine);
        cmd.arg(&target);
        cmd
    } else {
        Command::new(&target)
    };
    cmd.args(split_args(&game.launch_args))
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for var in &game.env {
        cmd.env(var.key.trim(), &var.value);
    }
    Ok(cmd)
}

/// The folder a game starts in: its working folder if set, else its executable's folder.
fn working_dir(game: &Game) -> PathBuf {
    game.working_dir
        .clone()
        .filter(|d| d.is_dir())
        .or_else(|| game.executable.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| game.install_dir.clone())
}

/// The game's pre-launch command, run by the system shell (`cmd` on Windows, `sh` elsewhere) in
/// the game's working folder with its environment variables. `None` when it has none.
pub fn pre_launch_command(game: &Game) -> Option<Command> {
    let script = game.pre_launch.trim();
    if script.is_empty() {
        return None;
    }
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut cmd = Command::new("cmd");
        // /S keeps everything between the outer quotes exactly as typed.
        cmd.raw_arg(format!("/D /S /C \"{script}\""))
            .creation_flags(CREATE_NO_WINDOW);
        cmd
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(script);
        cmd
    };
    cmd.current_dir(working_dir(game))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for var in &game.env {
        cmd.env(var.key.trim(), &var.value);
    }
    Some(cmd)
}

/// Finds Wine: the path set in Settings if it exists, else `wine64` or `wine` on the PATH or in
/// the usual install locations (desktop apps on macOS do not inherit the shell's PATH).
pub fn find_wine(configured: &str) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        return path.is_file().then_some(path);
    }
    if cfg!(windows) {
        return None;
    }
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    dirs.extend(
        [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/usr/bin",
            "/Applications/Wine Stable.app/Contents/Resources/wine/bin",
            "/Applications/Wine Staging.app/Contents/Resources/wine/bin",
        ]
        .map(PathBuf::from),
    );
    dirs.iter()
        .flat_map(|d| [d.join("wine64"), d.join("wine")])
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_like_a_shell() {
        assert_eq!(
            split_args(r#"-windowed "-profile=My Save" +set fov 90"#),
            ["-windowed", "-profile=My Save", "+set", "fov", "90"]
        );
        assert_eq!(split_args("  "), Vec::<String>::new());
        assert_eq!(split_args(r#"-name "" -x"#), ["-name", "", "-x"]);
        assert_eq!(split_args("'a b'c"), ["a bc"]);
    }

    #[test]
    fn runs_the_pre_launch_command_in_the_game_folder() {
        let dir = tempfile::tempdir().unwrap();
        let mut game: Game = serde_json::from_value(serde_json::json!({
            "id": "g", "name": "G", "installDir": dir.path(), "executable": dir.path().join("g.exe"), "addedAt": 0
        }))
        .unwrap();
        assert!(pre_launch_command(&game).is_none());

        game.pre_launch = r#"echo "two words" > "pre launch.txt""#.into();
        let status = pre_launch_command(&game).unwrap().status().unwrap();
        assert!(status.success());
        let written = std::fs::read_to_string(dir.path().join("pre launch.txt")).unwrap();
        assert!(written.contains("two words"), "{written}");

        game.pre_launch = "exit 3".into();
        assert_eq!(pre_launch_command(&game).unwrap().status().unwrap().code(), Some(3));
    }
}
