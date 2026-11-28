//! Finds games that other launchers installed: Steam, Epic Games, GOG and the Xbox app.
//!
//! Only local files (and on Windows, the registry) are read. Nothing goes online, and nothing in
//! the other launchers is changed.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::Serialize;
use walkdir::WalkDir;

use crate::inspect;
use crate::model::{Artwork, Game, Store, StoreRef};

/// A game another launcher installed, ready to add to the library.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundGame {
    pub source: StoreRef,
    pub name: String,
    pub install_dir: PathBuf,
    pub executable: PathBuf,
    /// Artwork the launcher keeps on disk (Steam's library images) or the game folder ships.
    pub artwork: Artwork,
    /// Already in the Starex library (same folder or same store id).
    pub in_library: bool,
}

/// A game a launcher lists, before Starex has picked the program to start.
struct Listed {
    source: StoreRef,
    name: String,
    install_dir: PathBuf,
    executable: Option<PathBuf>,
    artwork: Artwork,
}

/// Every installed game Starex can find, from one launcher or (with `None`) all of them, sorted
/// by name. `library` marks the ones already added.
pub fn scan(library: &[Game], only: Option<Store>) -> Vec<FoundGame> {
    let wants = |store: Store| only.is_none_or(|o| o == store);
    let mut listed = Vec::new();
    if wants(Store::Steam)
        && let Some(root) = steam_root()
    {
        listed.extend(steam_games(&root));
    }
    if wants(Store::Epic) {
        listed.extend(epic_listed());
    }
    if wants(Store::Gog) {
        listed.extend(gog_games());
    }
    if wants(Store::Xbox) {
        listed.extend(xbox_games());
    }

    let mut found: Vec<FoundGame> = listed.into_par_iter().filter_map(resolve).collect();
    found.sort_by_key(|g| g.name.to_lowercase());
    let mut seen = HashSet::new();
    found.retain(|g| seen.insert(path_key(&g.install_dir)));
    for game in &mut found {
        let folder = path_key(&game.install_dir);
        game.in_library = library
            .iter()
            .any(|g| g.source.as_ref() == Some(&game.source) || path_key(&g.install_dir) == folder);
    }
    found
}

/// Picks the program when the launcher does not say, fills in artwork the game folder ships, and
/// names games the launcher lists without a name. Steam and Epic start their games themselves, so
/// one of theirs stays even when Starex cannot tell which program is the game; GOG and Xbox games
/// start from their program, so those without one this computer can run are left out.
fn resolve(mut game: Listed) -> Option<FoundGame> {
    if !game.install_dir.is_dir() {
        return None;
    }
    let launcher_starts_it = launch_url(&game.source).is_some();
    let mut inspection = None;
    let executable = match game.executable.take().filter(|e| e.is_file()) {
        Some(exe) => exe,
        None => {
            let found = inspect::inspect_folder(&game.install_dir).ok();
            let program = found
                .as_ref()
                .and_then(|i| i.candidates.iter().find(|c| c.runnable))
                .map(|c| c.path.clone());
            inspection = found;
            match program {
                Some(path) => path,
                // The folder stands in for the program: Starex follows the game by its folder.
                None if launcher_starts_it => game.install_dir.clone(),
                None => return None,
            }
        }
    };
    if let Some(art) = inspection.map(|i| i.artwork) {
        let slots = [
            (&mut game.artwork.banner, art.banner),
            (&mut game.artwork.cover, art.cover),
            (&mut game.artwork.logo, art.logo),
            (&mut game.artwork.icon, art.icon),
        ];
        for (slot, detected) in slots {
            if slot.is_none() {
                *slot = detected;
            }
        }
    }
    let mut name = game.name.trim().to_owned();
    if name.is_empty() {
        let folder = game.install_dir.file_name()?.to_string_lossy().into_owned();
        name = title_from_folder(&folder);
    }
    Some(FoundGame {
        source: game.source,
        name,
        install_dir: game.install_dir,
        executable,
        artwork: game.artwork,
        in_library: false,
    })
}

/// A readable title from an install folder: "ManifoldGarden" becomes "Manifold Garden",
/// "DOOMEternal" becomes "DOOM Eternal". The new Epic Games Launcher lists games without a name.
fn title_from_folder(folder: &str) -> String {
    let chars: Vec<char> = folder.chars().collect();
    let mut spaced = String::with_capacity(folder.len() + 8);
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && c.is_ascii_uppercase() {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
            // A word starts at "aB" or at the last capital of a run followed by lowercase ("DOOMEt").
            if prev.is_ascii_lowercase() || prev.is_ascii_digit() || (prev.is_ascii_uppercase() && next_lower) {
                spaced.push(' ');
            }
        }
        spaced.push(c);
    }
    inspect::pretty_name(&spaced)
}

/// Whether a launcher is installed on this computer, so Starex can find its games.
pub fn launcher_installed(store: Store) -> bool {
    match store {
        Store::Steam => steam_root().is_some(),
        Store::Epic => epic_manifests_dir().is_some() || epic_installations_file().is_some(),
        Store::Gog | Store::Xbox => true,
    }
}

/// Whether the system can hand a launcher its launch link. On Windows a link nobody registered
/// would open the "How do you want to open this?" prompt instead of the game, so ask first.
pub fn launcher_can_start(store: Store) -> bool {
    #[cfg(windows)]
    {
        let scheme = match store {
            Store::Steam => "steam",
            Store::Epic => "com.epicgames.launcher",
            Store::Gog | Store::Xbox => return true,
        };
        registry::string(registry::Root::Classes, &format!(r"{scheme}\shell\open\command"), "").is_some()
    }
    #[cfg(not(windows))]
    {
        launcher_installed(store)
    }
}

/// The link that asks a game's launcher to start it, for Steam and Epic games. Other games (GOG,
/// Xbox) start from their own program. The launcher then handles updates, cloud saves, its
/// overlay and copy protection, as when the game is started from it.
pub fn launch_url(source: &StoreRef) -> Option<String> {
    let id = source.id.trim();
    match source.store {
        Store::Steam => id.parse::<u32>().ok().map(|app| format!("steam://rungameid/{app}")),
        Store::Epic if !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || "-_.:".contains(c)) => {
            Some(format!(
                "com.epicgames.launcher://apps/{}?action=launch&silent=true",
                id.replace(':', "%3A")
            ))
        }
        _ => None,
    }
}

/// The ids of games imported from a launcher that the launcher no longer has installed: the game
/// was uninstalled there, or its folder is gone.
pub fn not_installed(games: &[Game]) -> Vec<String> {
    let steam: Option<HashSet<String>> = steam_root().map(|root| steam_installed_ids(&root));
    let epic: Option<HashSet<String>> = launcher_installed(Store::Epic).then(|| {
        epic_listed()
            .into_iter()
            .map(|g| epic_app_name(&g.source.id).to_owned())
            .collect()
    });
    games
        .iter()
        .filter(|game| {
            let Some(source) = &game.source else { return false };
            let listed = match source.store {
                Store::Steam => steam.as_ref().is_some_and(|ids| ids.contains(&source.id)),
                Store::Epic => epic
                    .as_ref()
                    .is_some_and(|apps| apps.contains(epic_app_name(&source.id))),
                Store::Gog | Store::Xbox => true,
            };
            !listed || !game.install_dir.is_dir()
        })
        .map(|game| game.id.clone())
        .collect()
}

/// The key that names a launcher's game across imports and removals: "steam:570".
pub fn store_key(source: &StoreRef) -> String {
    let store = match source.store {
        Store::Steam => "steam",
        Store::Epic => "epic",
        Store::Gog => "gog",
        Store::Xbox => "xbox",
    };
    format!("{store}:{}", source.id)
}

/// The app name in an Epic id, which is "namespace:item:app" or just "app".
fn epic_app_name(id: &str) -> &str {
    id.rsplit(':').next().unwrap_or(id)
}

/// The Steam app id of a game: from its import, or the `steam_appid.txt` many Steam games ship.
pub fn steam_app_id(game: &Game) -> Option<u32> {
    if let Some(source) = &game.source
        && source.store == Store::Steam
    {
        return source.id.parse().ok();
    }
    let dirs = [Some(game.install_dir.as_path()), game.executable.parent()];
    dirs.into_iter().flatten().find_map(|dir| {
        let text = fs::read_to_string(dir.join("steam_appid.txt")).ok()?;
        text.split_whitespace().next()?.parse().ok()
    })
}

// Steam -------------------------------------------------------------------------------------

/// Apps Steam installs that are not games: runtimes, Proton, redistributables, SteamVR.
const STEAM_TOOLS: &[u32] = &[228980, 1070560, 1391110, 1628350, 250820, 1826330];
const STEAM_TOOL_PREFIXES: &[&str] = &["proton", "steam linux runtime", "steamworks", "steamvr"];

/// Where Steam is installed.
pub fn steam_root() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    #[cfg(windows)]
    if let Some(path) = registry::string(registry::Root::CurrentUser, r"Software\Valve\Steam", "SteamPath") {
        candidates.push(PathBuf::from(path));
    }
    if cfg!(windows) {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(dir) = std::env::var_os(var) {
                candidates.push(PathBuf::from(dir).join("Steam"));
            }
        }
    } else if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.extend([
            home.join(".steam/steam"),
            home.join(".local/share/Steam"),
            home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
            home.join("Library/Application Support/Steam"),
        ]);
    }
    candidates.into_iter().find(|p| p.join("steamapps").is_dir())
}

/// The Steam account last signed in on this computer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamAccount {
    pub steam_id: String,
    pub name: String,
}

/// When each game was last played, from the Steam client's own records for an account
/// (`userdata/<account>/config/localconfig.vdf`): app id to Unix seconds. Steam keeps no play time
/// there that Starex relies on; that comes from the Steam Web API.
pub fn steam_last_played(steam_id: &str) -> HashMap<u32, u64> {
    let Some(root) = steam_root() else {
        return HashMap::new();
    };
    // The folder is named by the account id: the SteamID64 minus the individual-account base.
    let Some(account) = steam_id
        .trim()
        .parse::<u64>()
        .ok()
        .and_then(|id| id.checked_sub(76_561_197_960_265_728))
    else {
        return HashMap::new();
    };
    let Ok(text) = fs::read_to_string(root.join(format!("userdata/{account}/config/localconfig.vdf"))) else {
        return HashMap::new();
    };
    steam_last_played_from(&text)
}

fn steam_last_played_from(localconfig: &str) -> HashMap<u32, u64> {
    let vdf = Vdf::parse(localconfig);
    let apps = ["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"]
        .iter()
        .try_fold(&vdf, |node, key| node.get(key));
    apps.map(|apps| {
        apps.entries()
            .iter()
            .filter_map(|(id, app)| {
                let at = app.text("LastPlayed")?.parse::<u64>().ok().filter(|t| *t > 0)?;
                Some((id.parse::<u32>().ok()?, at))
            })
            .collect()
    })
    .unwrap_or_default()
}

pub fn steam_account() -> Option<SteamAccount> {
    let text = fs::read_to_string(steam_root()?.join("config/loginusers.vdf")).ok()?;
    let vdf = Vdf::parse(&text);
    let users = vdf.get("users")?.entries();
    let pick = users
        .iter()
        .find(|(_, u)| u.text("MostRecent") == Some("1"))
        .or(users.first())?;
    Some(SteamAccount {
        steam_id: pick.0.clone(),
        name: pick.1.text("PersonaName").unwrap_or_default().to_owned(),
    })
}

fn steam_libraries(root: &Path) -> Vec<PathBuf> {
    let mut libraries = vec![root.to_path_buf()];
    let Ok(text) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) else {
        return libraries;
    };
    let vdf = Vdf::parse(&text);
    let Some(folders) = vdf.get("libraryfolders") else {
        return libraries;
    };
    for (key, entry) in folders.entries() {
        if !key.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        // Newer files hold a block with "path"; older ones the path itself.
        let path = match entry {
            Vdf::Text(path) => Some(path.as_str()),
            Vdf::Map(_) => entry.text("path"),
        };
        if let Some(path) = path.map(PathBuf::from)
            && !libraries.iter().any(|l| path_key(l) == path_key(&path))
        {
            libraries.push(path);
        }
    }
    libraries
}

fn steam_games(root: &Path) -> Vec<Listed> {
    let mut games = Vec::new();
    for library in steam_libraries(root) {
        let apps = library.join("steamapps");
        let Ok(entries) = fs::read_dir(&apps) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = fs::read_to_string(entry.path()) else {
                continue;
            };
            let vdf = Vdf::parse(&text);
            let Some(state) = vdf.get("AppState") else { continue };
            let (Some(app_id), Some(title), Some(dir)) =
                (state.text("appid"), state.text("name"), state.text("installdir"))
            else {
                continue;
            };
            let installed = steam_playable(state);
            let tool = app_id.parse::<u32>().is_ok_and(|id| STEAM_TOOLS.contains(&id))
                || STEAM_TOOL_PREFIXES.iter().any(|p| title.to_lowercase().starts_with(p));
            if !installed || tool {
                continue;
            }
            games.push(Listed {
                source: StoreRef {
                    store: Store::Steam,
                    id: app_id.to_owned(),
                },
                name: title.to_owned(),
                install_dir: apps.join("common").join(dir),
                executable: None,
                artwork: steam_artwork(root, app_id),
            });
        }
    }
    games
}

/// Whether Steam has the game on disk: fully installed (StateFlags bit 4), or an installed game
/// waiting for, or in the middle of, an update (bits 2, 256, 512, 1024). An update clears bit 4
/// while the game stays on disk; a first download has no installed depots yet.
fn steam_playable(state: &Vdf) -> bool {
    let flags = state
        .text("StateFlags")
        .and_then(|f| f.parse::<u32>().ok())
        .unwrap_or(0);
    if flags & 4 != 0 {
        return true;
    }
    let updating = flags & (2 | 256 | 512 | 1024) != 0;
    let has_depots = state.get("InstalledDepots").is_some_and(|d| !d.entries().is_empty());
    updating && has_depots
}

/// The app ids of every game Steam lists with its folder still on disk, whatever its update state.
fn steam_installed_ids(root: &Path) -> HashSet<String> {
    let mut ids = HashSet::new();
    for library in steam_libraries(root) {
        let apps = library.join("steamapps");
        let Ok(entries) = fs::read_dir(&apps) else { continue };
        for entry in entries.flatten() {
            let Ok(text) = fs::read_to_string(entry.path()) else {
                continue;
            };
            if !entry
                .file_name()
                .to_string_lossy()
                .to_lowercase()
                .starts_with("appmanifest_")
            {
                continue;
            }
            let vdf = Vdf::parse(&text);
            let Some(state) = vdf.get("AppState") else { continue };
            if let (Some(id), Some(dir)) = (state.text("appid"), state.text("installdir"))
                && apps.join("common").join(dir).is_dir()
            {
                ids.insert(id.to_owned());
            }
        }
    }
    ids
}

/// The library images Steam keeps on disk: `librarycache/<appid>/...` in current versions, or
/// `librarycache/<appid>_...` in older ones.
fn steam_artwork(root: &Path, app_id: &str) -> Artwork {
    let cache = root.join("appcache/librarycache");
    let folder = cache.join(app_id);
    // Files can sit in a hash-named subfolder, and a client in another language may add its name
    // ("library_600x900_japanese.jpg"). Exact names win over localized ones.
    let files: Vec<PathBuf> = WalkDir::new(&folder)
        .max_depth(2)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect();
    let find = |names: &[&str], flat: &[String]| -> Option<PathBuf> {
        let file_name = |p: &PathBuf| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default()
        };
        let exact = names
            .iter()
            .find_map(|n| files.iter().find(|p| file_name(p) == n.to_lowercase()));
        let localized = || {
            names.iter().find_map(|n| {
                let (stem, ext) = n.rsplit_once('.')?;
                files.iter().find(|p| {
                    let name = file_name(p);
                    name.strip_prefix(&format!("{stem}_"))
                        .and_then(|rest| rest.strip_suffix(&format!(".{ext}")))
                        .is_some_and(|language| {
                            language != "blur"
                                && !language.is_empty()
                                && language.chars().all(|c| c.is_ascii_lowercase())
                        })
                })
            })
        };
        exact
            .or_else(localized)
            .cloned()
            .or_else(|| flat.iter().map(|n| cache.join(n)).find(|p| p.is_file()))
    };
    // The icon is the one file named by its hash.
    let icon = fs::read_dir(&folder).ok().and_then(|entries| {
        entries.flatten().map(|e| e.path()).find(|p| {
            p.extension().is_some_and(|e| e.eq_ignore_ascii_case("jpg"))
                && p.file_stem()
                    .is_some_and(|s| s.len() == 40 && s.to_string_lossy().chars().all(|c| c.is_ascii_hexdigit()))
        })
    });
    Artwork {
        cover: find(
            &["library_600x900_2x.jpg", "library_600x900.jpg"],
            &[
                format!("{app_id}_library_600x900_2x.jpg"),
                format!("{app_id}_library_600x900.jpg"),
            ],
        ),
        banner: find(&["library_hero.jpg"], &[format!("{app_id}_library_hero.jpg")]),
        logo: find(&["logo.png"], &[format!("{app_id}_logo.png")]),
        icon: icon.or_else(|| Some(cache.join(format!("{app_id}_icon.jpg"))).filter(|p| p.is_file())),
    }
}

// Epic Games --------------------------------------------------------------------------------

/// Every game the Epic Games Launcher lists, from the classic launcher's `.item` manifests and
/// the new launcher's `installations.jsonl` (it replaced the manifests in 2026).
fn epic_listed() -> Vec<Listed> {
    let mut games = Vec::new();
    if let Some(dir) = epic_manifests_dir() {
        games.extend(epic_games(&dir));
    }
    if let Some(file) = epic_installations_file()
        && let Ok(text) = fs::read_to_string(&file)
    {
        games.extend(epic_installations(&text));
    }
    games
}

/// The classic launcher's manifests: `<AppDataPath>\Manifests`, from the registry, or the default.
fn epic_manifests_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    #[cfg(windows)]
    if let Some(data) = registry::string(
        registry::Root::LocalMachine,
        r"SOFTWARE\WOW6432Node\Epic Games\EpicGamesLauncher",
        "AppDataPath",
    ) {
        candidates.push(PathBuf::from(data).join("Manifests"));
    }
    if cfg!(windows) {
        if let Some(data) = std::env::var_os("ProgramData") {
            candidates.push(PathBuf::from(data).join(r"Epic\EpicGamesLauncher\Data\Manifests"));
        }
    } else if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join("Library/Application Support/Epic/EpicGamesLauncher/Data/Manifests"));
    }
    candidates.into_iter().find(|d| d.is_dir())
}

/// The new launcher's install log: `<AppDataPath>\installations.jsonl`, where `AppDataPath` (in
/// the user's registry) is `%LOCALAPPDATA%\com.epicgames.eglv2` by default.
fn epic_installations_file() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    #[cfg(windows)]
    if let Some(data) = registry::string(
        registry::Root::CurrentUser,
        r"Software\Epic Games\EpicGamesLauncher",
        "AppDataPath",
    ) {
        candidates.push(PathBuf::from(data).join("installations.jsonl"));
    }
    if cfg!(windows) {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            candidates.push(PathBuf::from(local).join(r"com.epicgames.eglv2\installations.jsonl"));
        }
    } else if let Some(home) = std::env::var_os("HOME") {
        candidates
            .push(PathBuf::from(home).join("Library/Application Support/com.epicgames.eglv2/installations.jsonl"));
    }
    candidates.into_iter().find(|f| f.is_file())
}

/// Reads the new launcher's install log. It only grows: an install writes a line when it starts
/// (`"complete": false`) and another when it finishes, so the last line of each `installationId`
/// is its current state. The log has no game names; `resolve` names them from their folder.
fn epic_installations(log: &str) -> Vec<Listed> {
    let mut latest: Vec<(String, serde_json::Value)> = Vec::new();
    for line in log.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(id) = entry.get("installationId").and_then(|v| v.as_str()).map(str::to_owned) else {
            continue; // The header line.
        };
        match latest.iter_mut().find(|(known, _)| *known == id) {
            Some(slot) => slot.1 = entry,
            None => latest.push((id, entry)),
        }
    }
    latest
        .into_iter()
        .filter_map(|(_, entry)| {
            let text = |key: &str| {
                entry
                    .get(key)
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            };
            if entry.get("complete").and_then(|v| v.as_bool()) != Some(true) {
                return None;
            }
            let install_dir = PathBuf::from(text("dir")?);
            let app = text("artifactId")?;
            let id = match (text("namespace"), text("itemId")) {
                (Some(namespace), Some(item)) => format!("{namespace}:{item}:{app}"),
                _ => app.to_owned(),
            };
            Some(Listed {
                source: StoreRef { store: Store::Epic, id },
                name: String::new(),
                executable: text("launchExecutable").map(|exe| install_dir.join(exe)),
                install_dir,
                artwork: Artwork::default(),
            })
        })
        .collect()
}

fn epic_games(dir: &Path) -> Vec<Listed> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("item")))
        .filter_map(|e| epic_game(&fs::read_to_string(e.path()).ok()?))
        .collect()
}

fn epic_game(manifest: &str) -> Option<Listed> {
    let item: serde_json::Value = serde_json::from_str(manifest).ok()?;
    let text = |key: &str| item.get(key).and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty());
    if item.get("bIsIncompleteInstall").and_then(|v| v.as_bool()) == Some(true) {
        return None;
    }
    // Add-ons (DLC) name the game they belong to.
    if let (Some(main), Some(app)) = (text("MainGameAppName"), text("AppName"))
        && main != app
    {
        return None;
    }
    // The Unreal Engine and other tools are listed too; keep games only.
    if let Some(categories) = item.get("AppCategories").and_then(|v| v.as_array())
        && !categories.iter().any(|c| c.as_str() == Some("games"))
    {
        return None;
    }
    let install_dir = PathBuf::from(text("InstallLocation")?);
    let executable = text("LaunchExecutable").map(|exe| install_dir.join(exe));
    // The launcher starts a game by "namespace:item:app", the form its own shortcuts use.
    let app = text("AppName")?;
    let id = match (text("CatalogNamespace"), text("CatalogItemId")) {
        (Some(namespace), Some(item)) => format!("{namespace}:{item}:{app}"),
        _ => app.to_owned(),
    };
    Some(Listed {
        source: StoreRef { store: Store::Epic, id },
        name: text("DisplayName")?.to_owned(),
        install_dir,
        executable,
        artwork: Artwork::default(),
    })
}

// GOG ---------------------------------------------------------------------------------------

#[cfg(windows)]
fn gog_games() -> Vec<Listed> {
    use registry::Root;
    let mut games = Vec::new();
    for base in [r"SOFTWARE\WOW6432Node\GOG.com\Games", r"SOFTWARE\GOG.com\Games"] {
        for id in registry::subkeys(Root::LocalMachine, base) {
            let key = format!(r"{base}\{id}");
            let value = |name: &str| registry::string(Root::LocalMachine, &key, name).filter(|v| !v.trim().is_empty());
            // DLC share their game's folder and say which game they belong to.
            if value("dependsOn").is_some() {
                continue;
            }
            let (Some(name), Some(path)) = (value("gameName"), value("path")) else {
                continue;
            };
            games.push(Listed {
                source: StoreRef {
                    store: Store::Gog,
                    id: id.clone(),
                },
                name,
                install_dir: PathBuf::from(path),
                executable: value("exe").map(PathBuf::from),
                artwork: Artwork::default(),
            });
        }
    }
    games
}

#[cfg(not(windows))]
fn gog_games() -> Vec<Listed> {
    Vec::new()
}

// Xbox app ----------------------------------------------------------------------------------

/// Games the Xbox app installed to `<drive>:\XboxGames`. Only builds whose program Windows lets
/// other launchers start are found; protected ones have no readable executable.
fn xbox_games() -> Vec<Listed> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let mut games = Vec::new();
    for drive in 'A'..='Z' {
        let root = PathBuf::from(format!(r"{drive}:\XboxGames"));
        let Ok(entries) = fs::read_dir(&root) else { continue };
        for entry in entries.flatten() {
            let folder = entry.path();
            if !folder.is_dir() {
                continue;
            }
            let content = Some(folder.join("Content"))
                .filter(|c| c.is_dir())
                .unwrap_or(folder.clone());
            let config = content.join("MicrosoftGame.config");
            let folder_name = entry.file_name().to_string_lossy().into_owned();
            games.push(Listed {
                source: StoreRef {
                    store: Store::Xbox,
                    id: folder_name.clone(),
                },
                name: inspect::gdk_display_name(&config).unwrap_or(folder_name),
                executable: gdk_executable(&config).map(|exe| content.join(exe)),
                install_dir: content,
                artwork: Artwork::default(),
            });
        }
    }
    games
}

/// The program a GDK `MicrosoftGame.config` starts (`<Executable Name="Game.exe" ...>`).
fn gdk_executable(config: &Path) -> Option<String> {
    let text = fs::read_to_string(config).ok()?;
    let tag = text.find("<Executable ")?;
    let rest = &text[tag..];
    let start = rest.find("Name=\"")? + 6;
    let end = rest[start..].find('"')? + start;
    let name = rest[start..end].trim();
    (!name.is_empty()).then(|| name.to_owned())
}

// Helpers -----------------------------------------------------------------------------------

/// A path in a form that compares the way the system does.
fn path_key(path: &Path) -> String {
    let s = path.to_string_lossy().replace('/', "\\");
    let s = s.trim_end_matches('\\');
    if cfg!(windows) { s.to_lowercase() } else { s.to_owned() }
}

/// Valve's KeyValues text format (`.vdf`, `.acf`): quoted keys followed by a quoted value or a
/// `{ ... }` block.
#[derive(Debug)]
enum Vdf {
    Text(String),
    Map(Vec<(String, Vdf)>),
}

impl Vdf {
    fn parse(text: &str) -> Vdf {
        let tokens = tokenize(text);
        let mut iter = tokens.into_iter();
        Vdf::Map(parse_entries(&mut iter))
    }

    fn get(&self, key: &str) -> Option<&Vdf> {
        self.entries()
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    }

    fn text(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Vdf::Text(t) => Some(t),
            Vdf::Map(_) => None,
        }
    }

    fn entries(&self) -> &[(String, Vdf)] {
        match self {
            Vdf::Map(entries) => entries,
            Vdf::Text(_) => &[],
        }
    }
}

enum Token {
    Str(String),
    Open,
    Close,
}

fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => tokens.push(Token::Open),
            '}' => tokens.push(Token::Close),
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(other) => s.push(other),
                            None => {}
                        },
                        c => s.push(c),
                    }
                }
                tokens.push(Token::Str(s));
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {}
            c => {
                let mut s = String::from(c);
                while let Some(&next) = chars.peek() {
                    if next.is_whitespace() || next == '{' || next == '}' || next == '"' {
                        break;
                    }
                    s.push(next);
                    chars.next();
                }
                tokens.push(Token::Str(s));
            }
        }
    }
    tokens
}

fn parse_entries(tokens: &mut impl Iterator<Item = Token>) -> Vec<(String, Vdf)> {
    let mut entries = Vec::new();
    while let Some(token) = tokens.next() {
        let key = match token {
            Token::Str(key) => key,
            Token::Close => break,
            Token::Open => continue,
        };
        match tokens.next() {
            Some(Token::Str(value)) => entries.push((key, Vdf::Text(value))),
            Some(Token::Open) => entries.push((key, Vdf::Map(parse_entries(tokens)))),
            Some(Token::Close) | None => break,
        }
    }
    entries
}

/// Reading the Windows registry, for where Steam and GOG games are installed.
#[cfg(windows)]
mod registry {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_SZ, RegCloseKey,
        RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
    };
    use windows::core::{HSTRING, PWSTR};

    pub enum Root {
        CurrentUser,
        LocalMachine,
        /// Link schemes (`steam://`) and file types, from the user's and the machine's settings.
        Classes,
    }

    fn hkey(root: &Root) -> HKEY {
        match root {
            Root::CurrentUser => HKEY_CURRENT_USER,
            Root::LocalMachine => HKEY_LOCAL_MACHINE,
            Root::Classes => HKEY_CLASSES_ROOT,
        }
    }

    /// A text value, or `None` when the key or value is missing.
    pub fn string(root: Root, key: &str, value: &str) -> Option<String> {
        let (key, value) = (HSTRING::from(key), HSTRING::from(value));
        let mut size = 0u32;
        // SAFETY: the strings outlive the call; a size query passes no buffer.
        let status = unsafe { RegGetValueW(hkey(&root), &key, &value, RRF_RT_REG_SZ, None, None, Some(&mut size)) };
        if status != ERROR_SUCCESS || size == 0 {
            return None;
        }
        let mut buffer = vec![0u16; size as usize / 2 + 1];
        let mut size = (buffer.len() * 2) as u32;
        // SAFETY: `buffer` holds `size` bytes.
        let status = unsafe {
            RegGetValueW(
                hkey(&root),
                &key,
                &value,
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let len = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..len]))
    }

    /// The names of a key's subkeys.
    pub fn subkeys(root: Root, key: &str) -> Vec<String> {
        let mut handle = HKEY::default();
        // SAFETY: `handle` receives an open key, closed below.
        let status = unsafe { RegOpenKeyExW(hkey(&root), &HSTRING::from(key), None, KEY_READ, &mut handle) };
        if status != ERROR_SUCCESS {
            return Vec::new();
        }
        let mut names = Vec::new();
        for index in 0.. {
            let mut buffer = [0u16; 256];
            let mut len = buffer.len() as u32;
            // SAFETY: `buffer` holds `len` characters.
            let status = unsafe {
                RegEnumKeyExW(
                    handle,
                    index,
                    Some(PWSTR(buffer.as_mut_ptr())),
                    &mut len,
                    None,
                    None,
                    None,
                    None,
                )
            };
            if status != ERROR_SUCCESS {
                break;
            }
            names.push(String::from_utf16_lossy(&buffer[..len as usize]));
        }
        // SAFETY: opened above and closed once.
        unsafe {
            let _ = RegCloseKey(handle);
        }
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_steam_library_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Steam");
        let other = dir.path().join("Library2");
        fs::create_dir_all(root.join("steamapps/common/Star Runner")).unwrap();
        fs::create_dir_all(other.join("steamapps/common/Proton 9.0")).unwrap();
        fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
                root.display().to_string().replace('\\', "\\\\"),
                other.display().to_string().replace('\\', "\\\\"),
            ),
        )
        .unwrap();
        fs::write(
            root.join("steamapps/appmanifest_4000.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"4000\"\n\t\"name\"\t\t\"Star Runner\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"Star Runner\"\n}\n",
        )
        .unwrap();
        fs::write(
            other.join("steamapps/appmanifest_2805730.acf"),
            "\"AppState\" { \"appid\" \"2805730\" \"name\" \"Proton 9.0\" \"StateFlags\" \"4\" \"installdir\" \"Proton 9.0\" }",
        )
        .unwrap();
        fs::create_dir_all(root.join("appcache/librarycache/4000")).unwrap();
        fs::write(root.join("appcache/librarycache/4000/library_600x900.jpg"), b"x").unwrap();

        assert_eq!(steam_libraries(&root).len(), 2);
        let games = steam_games(&root);
        assert_eq!(games.len(), 1, "Proton is a tool, not a game");
        assert_eq!(games[0].name, "Star Runner");
        assert_eq!(games[0].source.id, "4000");
        assert!(games[0].install_dir.ends_with("steamapps/common/Star Runner"));
        assert!(games[0].artwork.cover.is_some());
    }

    #[test]
    fn reads_vdf_escapes_and_comments() {
        let vdf = Vdf::parse("// comment\n\"a\" { \"path\" \"C:\\\\Games\\\\x\" \"q\" \"say \\\"hi\\\"\" }");
        let a = vdf.get("A").unwrap();
        assert_eq!(a.text("path"), Some(r"C:\Games\x"));
        assert_eq!(a.text("q"), Some("say \"hi\""));
    }

    #[test]
    fn reads_epic_manifests() {
        let game = epic_game(
            r#"{"DisplayName":"Tiny Tina","InstallLocation":"D:\\Epic\\Tina","LaunchExecutable":"Tina.exe","AppName":"tina","AppCategories":["public","games","applications"]}"#,
        )
        .unwrap();
        assert_eq!(game.name, "Tiny Tina");
        assert_eq!(game.source.id, "tina");
        assert!(game.executable.unwrap().ends_with("Tina.exe"));
        let full = epic_game(
            r#"{"DisplayName":"Fall","InstallLocation":"D:\\Fall","AppName":"fall","CatalogNamespace":"ns1","CatalogItemId":"item2"}"#,
        )
        .unwrap();
        assert_eq!(full.source.id, "ns1:item2:fall");
        assert_eq!(epic_app_name(&full.source.id), "fall");
        let engine = r#"{"DisplayName":"Unreal Engine","InstallLocation":"D:\\UE","AppName":"UE_5","AppCategories":["engines"]}"#;
        assert!(epic_game(engine).is_none());
        let partial = r#"{"DisplayName":"X","InstallLocation":"D:\\X","AppName":"x","bIsIncompleteInstall":true}"#;
        assert!(epic_game(partial).is_none());
    }

    /// Link schemes are read from the registry's default value, which every Windows has for .exe.
    #[cfg(windows)]
    #[test]
    fn reads_link_handlers_from_the_registry() {
        assert!(registry::string(registry::Root::Classes, r"exefile\shell\open\command", "").is_some());
        assert!(registry::string(registry::Root::Classes, r"starex-no-such-scheme\shell\open\command", "").is_none());
    }

    #[test]
    fn reads_last_played_from_steams_local_records() {
        let config = r#""UserLocalConfigStore"
{
	"Software"
	{
		"Valve"
		{
			"Steam"
			{
				"apps"
				{
					"570"
					{
						"LastPlayed"		"1764000000"
						"cloud"		{ "last_sync_state"		"synchronized" }
					}
					"4000"
					{
						"LastPlayed"		"0"
					}
				}
			}
		}
	}
}"#;
        let played = steam_last_played_from(config);
        assert_eq!(played.get(&570), Some(&1_764_000_000));
        assert!(!played.contains_key(&4000), "never played");
    }

    #[test]
    fn builds_launcher_links() {
        let link = |store, id: &str| launch_url(&StoreRef { store, id: id.into() });
        assert_eq!(link(Store::Steam, "570").as_deref(), Some("steam://rungameid/570"));
        assert_eq!(link(Store::Steam, "570; calc").as_deref(), None);
        assert_eq!(
            link(Store::Epic, "ns:item:fall").as_deref(),
            Some("com.epicgames.launcher://apps/ns%3Aitem%3Afall?action=launch&silent=true")
        );
        assert_eq!(link(Store::Epic, "x&y").as_deref(), None);
        assert_eq!(link(Store::Gog, "1207658924").as_deref(), None);
    }

    #[test]
    fn reads_the_gdk_executable() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("MicrosoftGame.config");
        fs::write(
            &config,
            r#"<Game><ExecutableList><Executable Name="gamelaunchhelper.exe" Id="Game" /></ExecutableList></Game>"#,
        )
        .unwrap();
        assert_eq!(gdk_executable(&config).as_deref(), Some("gamelaunchhelper.exe"));
    }

    #[test]
    fn finds_the_steam_app_id_in_the_game_folder() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("steam_appid.txt"), "480\n").unwrap();
        let mut game: Game = serde_json::from_value(serde_json::json!({
            "id": "g", "name": "G", "installDir": dir.path(), "executable": dir.path().join("g.exe"), "addedAt": 0
        }))
        .unwrap();
        assert_eq!(steam_app_id(&game), Some(480));
        game.source = Some(StoreRef {
            store: Store::Steam,
            id: "570".into(),
        });
        assert_eq!(steam_app_id(&game), Some(570));
    }
    /// The new Epic Games Launcher's log, as reported for version 0.115 (BoilR issue 641): a header
    /// line, then a "started" and a "finished" line per install.
    #[test]
    fn reads_the_new_epic_install_log() {
        let dir = tempfile::tempdir().unwrap();
        let garden = dir.path().join("ManifoldGarden");
        let log = format!(
            concat!(
                r#"{{"version":1,"writtenAt":"2026-11-16T10:00:00Z","entries":0}}"#,
                "\n",
                r#"{{"installationId":"75E63BE729F16907B92C8CB1FAAB3329","complete":false,"dir":"{dir}","platform":"Windows","namespace":"015763e579d4479cb1ef7780bf91804f","itemId":"0914d46a6eba49a2aa18887b512dce6a","artifactId":"Lorikeet","pendingVersion":"1.1.0.15817","launchExecutable":"ManifoldGarden.exe"}}"#,
                "\n",
                r#"{{"installationId":"75E63BE729F16907B92C8CB1FAAB3329","complete":true,"dir":"{dir}","platform":"Windows","namespace":"015763e579d4479cb1ef7780bf91804f","itemId":"0914d46a6eba49a2aa18887b512dce6a","artifactId":"Lorikeet","version":"1.1.0.15817","launchExecutable":"ManifoldGarden.exe"}}"#,
                "\n",
                r#"{{"installationId":"AAAA","complete":false,"dir":"D:\\Epic\\Downloading","namespace":"n","itemId":"i","artifactId":"Cuckoo"}}"#,
                "\n",
                "not json\n",
            ),
            dir = garden.display().to_string().replace('\\', "\\\\"),
        );
        let games = epic_installations(&log);
        assert_eq!(games.len(), 1, "an install still downloading is left out");
        let game = &games[0];
        assert_eq!(
            game.source.id,
            "015763e579d4479cb1ef7780bf91804f:0914d46a6eba49a2aa18887b512dce6a:Lorikeet"
        );
        assert_eq!(game.install_dir, garden);
        assert!(game.executable.as_ref().unwrap().ends_with("ManifoldGarden.exe"));
        // The exact link reported to start this game with the new launcher.
        assert_eq!(
            launch_url(&game.source).as_deref(),
            Some(
                "com.epicgames.launcher://apps/015763e579d4479cb1ef7780bf91804f%3A0914d46a6eba49a2aa18887b512dce6a%3ALorikeet?action=launch&silent=true"
            )
        );

        // The log has no names: the folder gives one.
        fs::create_dir_all(&garden).unwrap();
        fs::write(garden.join("ManifoldGarden.exe"), b"MZ").unwrap();
        let found = resolve(games.into_iter().next().unwrap()).unwrap();
        assert_eq!(found.name, "Manifold Garden");
    }

    #[test]
    fn names_games_from_their_folder() {
        assert_eq!(title_from_folder("ManifoldGarden"), "Manifold Garden");
        assert_eq!(title_from_folder("DOOMEternal"), "DOOM Eternal");
        assert_eq!(title_from_folder("GTA Vice City"), "GTA Vice City");
        assert_eq!(title_from_folder("RocketLeague"), "Rocket League");
        assert_eq!(title_from_folder("Fortnite"), "Fortnite");
    }

    #[test]
    fn skips_epic_add_ons() {
        let dlc = r#"{"DisplayName":"Season Pass","InstallLocation":"D:\\Epic\\Game","AppName":"pass","MainGameAppName":"game","AppCategories":["public","games","addons"]}"#;
        assert!(epic_game(dlc).is_none());
        let base = r#"{"DisplayName":"Game","InstallLocation":"D:\\Epic\\Game","AppName":"game","MainGameAppName":"game","AppCategories":["public","games","applications"]}"#;
        assert!(epic_game(base).is_some());
    }

    #[test]
    fn keeps_steam_games_waiting_for_an_update() {
        let state = |flags: &str, depots: &str| {
            Vdf::parse(&format!(
                "\"AppState\" {{ \"appid\" \"1\" \"StateFlags\" \"{flags}\" \"InstalledDepots\" {{ {depots} }} }}"
            ))
        };
        let depot = "\"2\" { \"manifest\" \"123\" \"size\" \"9\" }";
        let playable = |v: Vdf| steam_playable(v.get("AppState").unwrap());
        assert!(playable(state("4", depot)), "fully installed");
        assert!(playable(state("6", depot)), "installed, update required");
        assert!(playable(state("1026", depot)), "installed, update started");
        assert!(!playable(state("1026", "")), "first download, nothing installed yet");
        assert!(!playable(state("1", "")), "uninstalled");
    }

    #[test]
    fn reads_modern_and_legacy_steam_library_lists() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Steam");
        fs::create_dir_all(root.join("steamapps")).unwrap();
        let second = dir.path().join("SteamLibrary");
        let escaped = |p: &Path| p.display().to_string().replace('\\', "\\\\");
        // Current clients: a block per library with its apps.
        fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t\t\"label\"\t\t\"\"\n\t\t\"contentid\"\t\t\"1\"\n\t\t\"totalsize\"\t\t\"0\"\n\t\t\"apps\"\n\t\t{{\n\t\t\t\"228980\"\t\t\"1\"\n\t\t}}\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t\t\"apps\"\n\t\t{{\n\t\t}}\n\t}}\n}}\n",
                escaped(&root),
                escaped(&second)
            ),
        )
        .unwrap();
        assert_eq!(steam_libraries(&root).len(), 2);
        // Older clients: "LibraryFolders" with plain paths next to bookkeeping values.
        fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"LibraryFolders\"\n{{\n\t\"TimeNextStatsReport\"\t\t\"1600000000\"\n\t\"ContentStatsID\"\t\t\"-1\"\n\t\"1\"\t\t\"{}\"\n}}\n",
                escaped(&second)
            ),
        )
        .unwrap();
        let libraries = steam_libraries(&root);
        assert_eq!(libraries.len(), 2);
        assert_eq!(path_key(&libraries[1]), path_key(&second));
    }

    #[test]
    fn finds_steam_art_in_subfolders_and_other_languages() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let app = root.join("appcache/librarycache/570");
        fs::create_dir_all(app.join("a1b2c3")).unwrap();
        fs::write(app.join("a1b2c3/library_600x900_japanese.jpg"), b"x").unwrap();
        fs::write(app.join("library_hero_blur.jpg"), b"x").unwrap();
        fs::write(app.join("library_hero.jpg"), b"x").unwrap();
        fs::write(app.join("logo_schinese.png"), b"x").unwrap();
        let art = steam_artwork(root, "570");
        assert!(art.cover.unwrap().ends_with("library_600x900_japanese.jpg"));
        assert!(
            art.banner.unwrap().ends_with("library_hero.jpg"),
            "the blurred hero is not the banner"
        );
        assert!(art.logo.unwrap().ends_with("logo_schinese.png"));

        // The exact name wins over a localized one.
        fs::write(app.join("library_600x900.jpg"), b"x").unwrap();
        assert!(
            steam_artwork(root, "570")
                .cover
                .unwrap()
                .ends_with("library_600x900.jpg")
        );
    }

    #[test]
    fn a_steam_game_stays_installed_while_its_folder_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Steam");
        fs::create_dir_all(root.join("steamapps/common/Star Runner")).unwrap();
        fs::write(
            root.join("steamapps/appmanifest_4000.acf"),
            "\"AppState\" { \"appid\" \"4000\" \"name\" \"Star Runner\" \"StateFlags\" \"1026\" \"installdir\" \"Star Runner\" }",
        )
        .unwrap();
        fs::write(
            root.join("steamapps/appmanifest_4100.acf"),
            "\"AppState\" { \"appid\" \"4100\" \"name\" \"Gone\" \"StateFlags\" \"4\" \"installdir\" \"Gone\" }",
        )
        .unwrap();
        let ids = steam_installed_ids(&root);
        assert!(ids.contains("4000"), "updating still counts as installed");
        assert!(!ids.contains("4100"), "its folder is gone");
    }

    #[test]
    fn keeps_a_launcher_game_whose_program_is_unclear() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("readme.txt"), "hi").unwrap();
        let listed = |store| Listed {
            source: StoreRef {
                store,
                id: "4000".into(),
            },
            name: "Star Runner".into(),
            install_dir: dir.path().to_path_buf(),
            executable: None,
            artwork: Artwork::default(),
        };
        let steam = resolve(listed(Store::Steam)).expect("Steam starts it, so it stays");
        assert_eq!(steam.executable, dir.path());
        assert!(
            resolve(listed(Store::Gog)).is_none(),
            "a GOG game needs a program Starex can start"
        );
    }
}
