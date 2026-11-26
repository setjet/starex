use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{Error, Result};

/// Seconds since the Unix epoch.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// Reads a JSON file. `Ok(None)` means the file does not exist.
pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(Error::io("read", path, err)),
    };
    serde_json::from_slice(&bytes).map(Some).map_err(|source| Error::Json {
        path: path.to_path_buf(),
        source,
    })
}

/// Writes JSON through a temporary file and a rename, so a crash never leaves a half-written file.
pub(crate) fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io("create", parent, e))?;
    }
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(value).map_err(|source| Error::Json {
        path: path.to_path_buf(),
        source,
    })?;
    {
        let mut file = fs::File::create(&tmp).map_err(|e| Error::io("write", &tmp, e))?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| Error::io("write", &tmp, e))?;
    }
    fs::rename(&tmp, path).map_err(|e| Error::io("save", path, e))
}

/// Lower-case letters and digits only: "Grand_Theft-Auto VI" -> "grandtheftautovi".
pub(crate) fn normalize(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Lower-case alphanumeric words, splitting on separators and camelCase boundaries.
pub(crate) fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut prev_lower = false;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            if c.is_ascii_uppercase() && prev_lower && !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
            current.push(c.to_ascii_lowercase());
        } else {
            prev_lower = false;
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_words() {
        assert_eq!(words("GrandTheft_Auto-VI"), ["grand", "theft", "auto", "vi"]);
        assert_eq!(normalize("Half-Life 2"), "halflife2");
    }

    #[test]
    fn atomic_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/value.json");
        write_json_atomic(&path, &vec![1, 2, 3]).unwrap();
        let back: Option<Vec<i32>> = read_json(&path).unwrap();
        assert_eq!(back, Some(vec![1, 2, 3]));
        assert!(
            read_json::<Vec<i32>>(&dir.path().join("missing.json"))
                .unwrap()
                .is_none()
        );
    }
}
