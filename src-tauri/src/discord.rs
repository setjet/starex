//! Discord Rich Presence: shows the game being played on the user's Discord profile.
//!
//! Starex talks to the Discord desktop app over its local IPC channel (a named pipe on Windows,
//! a Unix socket elsewhere). Nothing goes over the internet from Starex; Discord shows the
//! status to the user's friends as usual. When Discord is not running, nothing happens.

use std::io::{Read, Write};

use serde_json::json;

/// The Discord application Starex reports as: the maintainer's "Starex" application at
/// https://discord.com/developers/applications. The ID is public, not a secret. Users can still
/// set their own in Settings.
pub const DEFAULT_APP_ID: &str = "1575991160806056006";

const HANDSHAKE: u32 = 0;
const FRAME: u32 = 1;

trait Pipe: Read + Write + Send {}
impl<T: Read + Write + Send> Pipe for T {}

/// An open presence. Dropping it closes the connection, and Discord clears the status.
pub struct Presence {
    pipe: Box<dyn Pipe>,
}

fn write_frame(pipe: &mut dyn Pipe, opcode: u32, payload: &serde_json::Value) -> std::io::Result<()> {
    let body = payload.to_string().into_bytes();
    let mut frame = Vec::with_capacity(8 + body.len());
    frame.extend_from_slice(&opcode.to_le_bytes());
    frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
    frame.extend_from_slice(&body);
    pipe.write_all(&frame)?;
    pipe.flush()
}

fn read_frame(pipe: &mut dyn Pipe) -> std::io::Result<serde_json::Value> {
    let mut header = [0u8; 8];
    pipe.read_exact(&mut header)?;
    let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if len > 1 << 20 {
        return Err(std::io::Error::other("Discord sent an oversized message"));
    }
    let mut body = vec![0u8; len];
    pipe.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(std::io::Error::other)
}

#[cfg(windows)]
fn open_pipe(index: u8) -> Option<Box<dyn Pipe>> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(format!(r"\\.\pipe\discord-ipc-{index}"))
        .ok()
        .map(|f| Box::new(f) as Box<dyn Pipe>)
}

#[cfg(unix)]
fn open_pipe(index: u8) -> Option<Box<dyn Pipe>> {
    let dirs = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .iter()
        .filter_map(std::env::var_os)
        .map(std::path::PathBuf::from)
        .chain([std::path::PathBuf::from("/tmp")]);
    for dir in dirs {
        for sub in ["", "app/com.discordapp.Discord", "snap.discord"] {
            let path = dir.join(sub).join(format!("discord-ipc-{index}"));
            if let Ok(stream) = std::os::unix::net::UnixStream::connect(&path) {
                return Some(Box::new(stream));
            }
        }
    }
    None
}

impl Presence {
    /// Shows `game_name` as being played since `started_at` (Unix seconds). `None` when Discord
    /// is not running or refused the application ID.
    /// `image` is a web address of the game's own art; without one, Discord shows the Starex logo
    /// (the application's `starex` art asset). Discord can only load images from the internet.
    pub fn show(app_id: &str, game_name: &str, started_at: u64, image: Option<&str>) -> Option<Self> {
        let app_id = app_id.trim();
        if app_id.is_empty() {
            return None;
        }
        let mut pipe = (0..10).find_map(open_pipe)?;
        write_frame(pipe.as_mut(), HANDSHAKE, &json!({ "v": 1, "client_id": app_id })).ok()?;
        let ready = read_frame(pipe.as_mut()).ok()?;
        if ready.get("evt").and_then(|e| e.as_str()) != Some("READY") {
            log::info!("Discord did not accept the application ID");
            return None;
        }
        let activity = json!({
            "cmd": "SET_ACTIVITY",
            "nonce": uuid_like(),
            "args": {
                "pid": std::process::id(),
                "activity": {
                    "details": game_name,
                    "state": "Playing with Starex",
                    "timestamps": { "start": started_at },
                    "assets": match image {
                        Some(url) => json!({ "large_image": url, "large_text": game_name }),
                        None => json!({ "large_image": "starex", "large_text": "Starex" }),
                    },
                    "instance": false
                }
            }
        });
        write_frame(pipe.as_mut(), FRAME, &activity).ok()?;
        let _ = read_frame(pipe.as_mut());
        log::info!("Discord shows {game_name}");
        Some(Self { pipe })
    }
}

impl Drop for Presence {
    fn drop(&mut self) {
        let clear = json!({
            "cmd": "SET_ACTIVITY",
            "nonce": uuid_like(),
            "args": { "pid": std::process::id() }
        });
        let _ = write_frame(self.pipe.as_mut(), FRAME, &clear);
    }
}

fn uuid_like() -> String {
    format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let mut buffer: std::io::Cursor<Vec<u8>> = std::io::Cursor::new(Vec::new());
        write_frame(&mut buffer, FRAME, &json!({ "evt": "READY" })).unwrap();
        buffer.set_position(0);
        assert_eq!(read_frame(&mut buffer).unwrap()["evt"], "READY");
        assert!(
            Presence::show("", "Game", 0, None).is_none(),
            "no application ID, no presence"
        );
    }
}
