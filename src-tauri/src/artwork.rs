//! Game artwork: copying the user's own images, and fetching from SteamGridDB when the user has
//! added an API key.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use starex_core::model::{Artwork, ArtworkKind};

use crate::state::Paths;

const MAX_BYTES: u64 = 40 * 1024 * 1024;
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "avif"];
const API: &str = "https://www.steamgriddb.com/api/v2";

fn image_ext(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    IMAGE_EXTENSIONS.contains(&ext.as_str()).then_some(ext)
}

/// Removes the stored file for `kind`. Each version gets a new file name, so the webview never
/// shows a cached older image.
fn clear(dir: &Path, kind: ArtworkKind) {
    let prefix = format!("{}-", kind.as_str());
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

fn destination(paths: &Paths, game_id: &str, kind: ArtworkKind, ext: &str) -> Result<PathBuf, String> {
    let dir = paths.artwork_dir(game_id);
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    clear(&dir, kind);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    Ok(dir.join(format!("{}-{stamp}.{ext}", kind.as_str())))
}

/// Copies an image the user chose into Starex's artwork folder and returns the stored path.
pub fn import(paths: &Paths, game_id: &str, kind: ArtworkKind, source: &Path) -> Result<PathBuf, String> {
    let ext = image_ext(source).ok_or("Choose a PNG, JPEG, WebP, GIF or AVIF image.")?;
    let meta = fs::metadata(source).map_err(|e| format!("Could not read {}: {e}", source.display()))?;
    if meta.len() > MAX_BYTES {
        return Err("That image is larger than 40 MB.".into());
    }
    let dest = destination(paths, game_id, kind, &ext)?;
    fs::copy(source, &dest).map_err(|e| format!("Could not copy the image: {e}"))?;
    Ok(dest)
}

/// Copies the user's backdrop image into the artwork folder (the only folder the window may load
/// images from) and returns its new path. Earlier backdrops are deleted.
pub fn import_backdrop(paths: &Paths, source: &Path) -> Result<PathBuf, String> {
    let ext = image_ext(source).ok_or("Choose a PNG, JPEG, WebP, GIF or AVIF image.")?;
    let meta = fs::metadata(source).map_err(|e| format!("Could not read {}: {e}", source.display()))?;
    if meta.len() > MAX_BYTES {
        return Err("That image is larger than 40 MB.".into());
    }
    let dir = paths.artwork.join("_backdrop");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let dest = dir.join(format!("backdrop-{stamp}.{ext}"));
    fs::copy(source, &dest).map_err(|e| format!("Could not copy the image: {e}"))?;
    Ok(dest)
}

pub fn remove(paths: &Paths, game_id: &str, kind: ArtworkKind) {
    clear(&paths.artwork_dir(game_id), kind);
}

pub fn remove_all(paths: &Paths, game_id: &str) {
    let _ = fs::remove_dir_all(paths.artwork_dir(game_id));
}

/// Imports the artwork found inside the game folder into empty slots.
pub fn import_detected(paths: &Paths, game_id: &str, artwork: &mut Artwork, detected: &Artwork) {
    for kind in ArtworkKind::ALL {
        if artwork.get(kind).is_some() {
            continue;
        }
        if let Some(source) = detected.get(kind) {
            match import(paths, game_id, kind, source) {
                Ok(stored) => artwork.set(kind, Some(stored)),
                Err(e) => log::warn!("skipped {} artwork: {e}", kind.as_str()),
            }
        }
    }
}

#[derive(Deserialize)]
struct Envelope<T> {
    success: bool,
    data: Option<T>,
    #[serde(default)]
    errors: Vec<String>,
}

#[derive(Deserialize)]
struct SearchHit {
    id: u64,
}

#[derive(Deserialize)]
struct Asset {
    url: String,
}

async fn get<T: for<'de> Deserialize<'de>>(client: &reqwest::Client, key: &str, url: &str) -> Result<T, String> {
    let response = client
        .get(url)
        .bearer_auth(key)
        .send()
        .await
        .map_err(|e| format!("SteamGridDB could not be reached: {e}"))?;
    match response.status().as_u16() {
        401 | 403 => return Err("SteamGridDB rejected the API key. Check it in Settings > Integrations.".into()),
        404 => return Err("SteamGridDB has no artwork for this game.".into()),
        _ => {}
    }
    let envelope: Envelope<T> = response
        .json()
        .await
        .map_err(|e| format!("SteamGridDB sent an unexpected answer: {e}"))?;
    match (envelope.success, envelope.data) {
        (true, Some(data)) => Ok(data),
        _ => Err(envelope
            .errors
            .first()
            .cloned()
            .unwrap_or_else(|| "SteamGridDB has no artwork for this game.".into())),
    }
}

async fn download(client: &reqwest::Client, url: &str) -> Result<(Vec<u8>, String), String> {
    let ext = Path::new(url.split('?').next().unwrap_or(url))
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .filter(|e| IMAGE_EXTENSIONS.contains(&e.as_str()))
        .unwrap_or_else(|| "png".into());
    let response = client
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Could not download artwork: {e}"))?;
    if response.content_length().is_some_and(|n| n > MAX_BYTES) {
        return Err("The artwork file is too large.".into());
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Could not download artwork: {e}"))?;
    Ok((bytes.to_vec(), ext))
}

/// Fetches artwork for `name` from SteamGridDB. With `overwrite` false, only empty slots of
/// `current` are filled. Returns the updated artwork.
pub async fn fetch_steamgriddb(
    client: &reqwest::Client,
    paths: &Paths,
    key: &str,
    game_id: &str,
    name: &str,
    current: &Artwork,
    overwrite: bool,
) -> Result<Artwork, String> {
    if key.trim().is_empty() {
        return Err("Add a SteamGridDB API key in Settings > Integrations first.".into());
    }
    let term: String = url_encode(name.trim());
    let hits: Vec<SearchHit> = get(client, key, &format!("{API}/search/autocomplete/{term}")).await?;
    let game = hits.first().ok_or("SteamGridDB has no game with this name.")?;

    let mut artwork = current.clone();
    let mut found = 0;
    for (kind, query) in [
        (ArtworkKind::Banner, format!("heroes/game/{}?types=static", game.id)),
        (
            ArtworkKind::Cover,
            format!("grids/game/{}?dimensions=600x900,342x482,660x930&types=static", game.id),
        ),
        (ArtworkKind::Logo, format!("logos/game/{}?types=static", game.id)),
        (ArtworkKind::Icon, format!("icons/game/{}?types=static", game.id)),
    ] {
        if !overwrite && artwork.get(kind).is_some() {
            continue;
        }
        let assets: Vec<Asset> = match get(client, key, &format!("{API}/{query}")).await {
            Ok(assets) => assets,
            Err(e) => {
                log::info!("no {} artwork: {e}", kind.as_str());
                continue;
            }
        };
        let Some(asset) = assets.first() else { continue };
        let (bytes, ext) = download(client, &asset.url).await?;
        let dest = destination(paths, game_id, kind, &ext)?;
        fs::write(&dest, bytes).map_err(|e| format!("Could not save artwork: {e}"))?;
        artwork.set(kind, Some(dest));
        found += 1;
    }
    if found == 0 && !ArtworkKind::ALL.iter().any(|k| artwork.get(*k).is_some()) {
        return Err("SteamGridDB has no artwork for this game.".into());
    }
    Ok(artwork)
}

fn url_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn encodes_names() {
        assert_eq!(super::url_encode("Half-Life 2: Ep"), "Half-Life%202%3A%20Ep");
    }
}
