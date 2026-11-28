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
    Ok(trimmed_if_logo(kind, dest))
}

/// A logo with its empty transparent edges trimmed, or the same file when there is nothing to trim
/// (or it is not a PNG). Launchers ship logos on a large transparent canvas (Steam's are 1280x720),
/// which made them look small on the game page.
fn trimmed_if_logo(kind: ArtworkKind, stored: PathBuf) -> PathBuf {
    if kind != ArtworkKind::Logo {
        return stored;
    }
    match trim_transparent_edges(&stored) {
        Ok(Some(trimmed)) => {
            let _ = fs::remove_file(&stored);
            trimmed
        }
        Ok(None) => stored,
        Err(e) => {
            log::info!("logo left as it is: {e}");
            stored
        }
    }
}

/// Crops a PNG to the part that is not transparent, plus a hairline margin, and saves it next to
/// the original under a new name. `None` when it has no transparency or is already tight. The
/// original file is left in place.
pub fn trim_transparent_edges(path: &Path) -> Result<Option<PathBuf>, String> {
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        return Ok(None);
    }
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buffer = vec![0; reader.output_buffer_size().ok_or("the image is too large")?];
    let info = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
    let (width, height) = (info.width as usize, info.height as usize);
    // Every pixel as RGBA; images without an alpha channel have nothing to trim.
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buffer[..width * height * 4].to_vec(),
        png::ColorType::GrayscaleAlpha => buffer[..width * height * 2]
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[gray, alpha]| [gray, gray, gray, alpha])
            .collect(),
        _ => return Ok(None),
    };
    let Some((left, top, right, bottom)) = opaque_bounds(width, height, |x, y| rgba[(y * width + x) * 4 + 3]) else {
        return Ok(None);
    };
    // A hairline of space keeps soft edges and shadows whole.
    let margin = (width.max(height) / 100).max(2);
    let (left, top) = (left.saturating_sub(margin), top.saturating_sub(margin));
    let (right, bottom) = ((right + margin).min(width - 1), (bottom + margin).min(height - 1));
    let (new_width, new_height) = (right - left + 1, bottom - top + 1);
    // Already tight: trimming would save less than 3% on every side.
    if new_width * 100 >= width * 97 && new_height * 100 >= height * 97 {
        return Ok(None);
    }
    let mut cropped = Vec::with_capacity(new_width * new_height * 4);
    for y in top..=bottom {
        let row = (y * width + left) * 4;
        cropped.extend_from_slice(&rgba[row..row + new_width * 4]);
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let dest = path.with_file_name(format!("{}-{stamp}t.png", ArtworkKind::Logo.as_str()));
    let out = fs::File::create(&dest).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(out), new_width as u32, new_height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let written = encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(&cropped).and_then(|_| w.finish()));
    if let Err(e) = written {
        let _ = fs::remove_file(&dest);
        return Err(e.to_string());
    }
    Ok(Some(dest))
}

/// Trims, once, the logos stored before Starex trimmed them, in the background. A marker file in
/// the artwork folder records that it ran. Each game's old logo file stays until its logo is next
/// changed, so a window that loaded the old path never shows a missing image.
pub fn trim_stored_logos_once(app: tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    std::thread::spawn(move || {
        let state = app.state::<crate::state::AppState>();
        let marker = state.paths.artwork.join(".logos-trimmed");
        if marker.exists() {
            return;
        }
        let games = state.library().games().to_vec();
        for game in games {
            let Some(logo) = game.artwork.logo.clone() else {
                continue;
            };
            match trim_transparent_edges(&logo) {
                Ok(Some(trimmed)) => {
                    let updated = state
                        .library()
                        .update(&game.id, |g| g.artwork.logo = Some(trimmed.clone()));
                    match updated {
                        Ok(game) => {
                            let _ = app.emit(crate::sessions::GAME_EVENT, &game);
                        }
                        Err(e) => {
                            log::warn!("could not save the trimmed logo: {e}");
                            let _ = fs::remove_file(&trimmed);
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => log::info!("logo of {} left as it is: {e}", game.name),
            }
        }
        let _ = fs::create_dir_all(&state.paths.artwork);
        if let Err(e) = fs::write(&marker, b"") {
            log::warn!("could not record that logos were trimmed: {e}");
        }
    });
}

/// The smallest box around every pixel that is more than faintly visible.
fn opaque_bounds(
    width: usize,
    height: usize,
    alpha: impl Fn(usize, usize) -> u8,
) -> Option<(usize, usize, usize, usize)> {
    const VISIBLE: u8 = 8;
    let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..height {
        for x in 0..width {
            if alpha(x, y) > VISIBLE {
                left = left.min(x);
                right = right.max(x);
                top = top.min(y);
                bottom = bottom.max(y);
            }
        }
    }
    (left != usize::MAX).then_some((left, top, right, bottom))
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
        artwork.set(kind, Some(trimmed_if_logo(kind, dest)));
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
    use std::fs;

    #[test]
    fn encodes_names() {
        assert_eq!(super::url_encode("Half-Life 2: Ep"), "Half-Life%202%3A%20Ep");
    }

    /// Writes a `width` x `height` transparent PNG with an opaque block at `block`.
    fn logo(path: &std::path::Path, width: u32, height: u32, block: (u32, u32, u32, u32)) {
        let mut data = vec![0u8; (width * height * 4) as usize];
        for y in block.1..block.3 {
            for x in block.0..block.2 {
                let i = ((y * width + x) * 4) as usize;
                data[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        let file = fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&data).unwrap();
        writer.finish().unwrap();
    }

    /// A scratch folder removed when the test ends.
    struct Dir(std::path::PathBuf);
    impl Dir {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn size(path: &std::path::Path) -> (u32, u32) {
        let decoder = png::Decoder::new(std::io::BufReader::new(fs::File::open(path).unwrap()));
        let info = decoder.read_info().unwrap().info().clone();
        (info.width, info.height)
    }

    #[test]
    fn trims_the_empty_canvas_around_a_launcher_logo() {
        let dir = std::env::temp_dir().join(format!("starex-trim-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let dir = Dir(dir);
        // Like Steam: a 1280x720 canvas with the logo in a small part of it.
        let padded = dir.path().join("logo-1.png");
        logo(&padded, 1280, 720, (100, 400, 500, 600));
        let trimmed = super::trim_transparent_edges(&padded)
            .unwrap()
            .expect("padding is trimmed");
        let (w, h) = size(&trimmed);
        assert!((400..=430).contains(&w) && (200..=230).contains(&h), "{w}x{h}");

        // A logo already cropped tight stays as it is.
        let tight = dir.path().join("logo-2.png");
        logo(&tight, 400, 200, (2, 2, 398, 198));
        assert!(super::trim_transparent_edges(&tight).unwrap().is_none());
    }
}
