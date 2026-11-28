//! News and achievements for Steam games, from the Steam Web API.
//!
//! News needs nothing from the user. Achievements need a free Steam Web API key and the user's
//! Steam ID (found automatically from the Steam client when it is installed), and the Steam
//! profile's game details must be public. Starex sends only the game's app id, the key and the
//! Steam ID, and only when a Steam game's page is open.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const API: &str = "https://api.steampowered.com";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsItem {
    pub gid: String,
    pub title: String,
    /// The start of the post as plain text.
    pub summary: String,
    /// Unix seconds.
    pub date: u64,
    pub source: String,
}

#[derive(Deserialize)]
struct NewsResponse {
    appnews: AppNews,
}

#[derive(Deserialize)]
struct AppNews {
    #[serde(default)]
    newsitems: Vec<RawNews>,
}

#[derive(Deserialize)]
struct RawNews {
    gid: String,
    title: String,
    #[serde(default)]
    contents: String,
    date: u64,
    #[serde(default)]
    feedlabel: String,
}

/// The latest news posts for a Steam app.
pub async fn news(http: &reqwest::Client, app_id: u32) -> Result<Vec<NewsItem>, String> {
    let response: NewsResponse = http
        .get(format!("{API}/ISteamNews/GetNewsForApp/v2/"))
        .query(&[
            ("appid", app_id.to_string()),
            ("count", "6".into()),
            ("maxlength", "400".into()),
            ("format", "json".into()),
        ])
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Steam could not be reached: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Steam sent an unexpected answer: {e}"))?;
    Ok(response
        .appnews
        .newsitems
        .into_iter()
        .map(|n| NewsItem {
            gid: n.gid,
            title: n.title.trim().to_owned(),
            summary: plain_text(&n.contents),
            date: n.date,
            source: n.feedlabel,
        })
        .collect())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Achievement {
    pub name: String,
    pub description: String,
    pub icon: String,
    pub icon_gray: String,
    pub achieved: bool,
    /// Unix seconds, when unlocked.
    pub unlocked_at: Option<u64>,
    /// Share of all players who unlocked it, 0 to 100.
    pub percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Achievements {
    pub total: usize,
    pub unlocked: usize,
    /// Unlocked first (newest first), then locked (most common first).
    pub items: Vec<Achievement>,
}

/// The user's achievements in a Steam game.
pub async fn achievements(
    http: &reqwest::Client,
    key: &str,
    steam_id: &str,
    app_id: u32,
) -> Result<Achievements, String> {
    let (key, steam_id) = (key.trim(), steam_id.trim());
    if key.is_empty() {
        return Err("Add a Steam Web API key in Settings > Integrations to see achievements.".into());
    }
    if steam_id.is_empty() {
        return Err("Add your Steam ID in Settings > Integrations to see achievements.".into());
    }
    let get = |path: &str, query: Vec<(&'static str, String)>| http.get(format!("{API}/{path}")).query(&query).send();

    let schema: Value = get(
        "ISteamUserStats/GetSchemaForGame/v2/",
        vec![
            ("key", key.into()),
            ("appid", app_id.to_string()),
            ("l", "english".into()),
        ],
    )
    .await
    .map_err(|e| format!("Steam could not be reached: {e}"))?
    .error_for_status()
    .map_err(|e| match e.status().map(|s| s.as_u16()) {
        Some(401 | 403) => "Steam rejected the Web API key. Check it in Settings > Integrations.".to_owned(),
        _ => format!("Steam could not be reached: {e}"),
    })?
    .json()
    .await
    .map_err(|e| format!("Steam sent an unexpected answer: {e}"))?;
    let defined = schema
        .pointer("/game/availableGameStats/achievements")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if defined.is_empty() {
        return Ok(Achievements {
            total: 0,
            unlocked: 0,
            items: Vec::new(),
        });
    }

    let player: Value = get(
        "ISteamUserStats/GetPlayerAchievements/v1/",
        vec![
            ("key", key.into()),
            ("steamid", steam_id.into()),
            ("appid", app_id.to_string()),
        ],
    )
    .await
    .map_err(|e| format!("Steam could not be reached: {e}"))?
    .json()
    .await
    .map_err(|e| format!("Steam sent an unexpected answer: {e}"))?;
    if player.pointer("/playerstats/success").and_then(Value::as_bool) != Some(true) {
        let reason = player
            .pointer("/playerstats/error")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        return Err(if reason.to_lowercase().contains("not public") {
            "Your Steam profile's game details are private. Make them public in Steam to see achievements here.".into()
        } else {
            format!("Steam did not return your achievements ({reason}).")
        });
    }
    let unlocked: HashMap<String, Option<u64>> = player
        .pointer("/playerstats/achievements")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|a| a.get("achieved").and_then(Value::as_u64) == Some(1))
        .filter_map(|a| {
            let name = a.get("apiname")?.as_str()?.to_owned();
            let at = a.get("unlocktime").and_then(Value::as_u64).filter(|t| *t > 0);
            Some((name, at))
        })
        .collect();

    // Global percentages are a nice extra; leave them out when Steam does not answer.
    let percents: HashMap<String, f64> = match get(
        "ISteamUserStats/GetGlobalAchievementPercentagesForApp/v2/",
        vec![("gameid", app_id.to_string())],
    )
    .await
    {
        Ok(response) => response.json::<Value>().await.ok(),
        Err(_) => None,
    }
    .and_then(|v| v.pointer("/achievementpercentages/achievements").cloned())
    .and_then(|v| v.as_array().cloned())
    .into_iter()
    .flatten()
    .filter_map(|a| {
        let name = a.get("name")?.as_str()?.to_owned();
        let percent = a
            .get("percent")
            .and_then(|p| p.as_f64().or_else(|| p.as_str()?.parse().ok()))?;
        Some((name, percent))
    })
    .collect();

    let mut items: Vec<Achievement> = defined
        .iter()
        .filter_map(|a| {
            let api_name = a.get("name")?.as_str()?;
            let text = |key: &str| a.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();
            let unlocked_at = unlocked.get(api_name);
            Some(Achievement {
                name: Some(text("displayName"))
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| api_name.to_owned()),
                description: text("description"),
                icon: text("icon"),
                icon_gray: text("icongray"),
                achieved: unlocked_at.is_some(),
                unlocked_at: unlocked_at.copied().flatten(),
                percent: percents.get(api_name).copied(),
            })
        })
        .collect();
    items.sort_by(|a, b| {
        b.achieved
            .cmp(&a.achieved)
            .then(b.unlocked_at.cmp(&a.unlocked_at))
            .then(b.percent.unwrap_or(0.0).total_cmp(&a.percent.unwrap_or(0.0)))
    });
    Ok(Achievements {
        total: items.len(),
        unlocked: items.iter().filter(|a| a.achieved).count(),
        items,
    })
}

/// Play time Steam keeps for one game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Playtime {
    /// All-time play time, in seconds.
    pub total_secs: u64,
    /// Play time over the last two weeks, in seconds.
    pub recent_secs: u64,
    /// Unix seconds, when Steam knows.
    pub last_played: Option<u64>,
}

/// Every game's play time on a Steam account (`IPlayerService/GetOwnedGames`): app id to play time.
/// Needs a Web API key, and the profile's game details must be public.
pub async fn playtime(http: &reqwest::Client, key: &str, steam_id: &str) -> Result<HashMap<u32, Playtime>, String> {
    let (key, steam_id) = (key.trim(), steam_id.trim());
    if key.is_empty() || steam_id.is_empty() {
        return Err("A Steam Web API key and Steam ID are needed for Steam play time.".into());
    }
    let response: Value = http
        .get(format!("{API}/IPlayerService/GetOwnedGames/v1/"))
        .query(&[
            ("key", key),
            ("steamid", steam_id),
            ("include_played_free_games", "1"),
            ("format", "json"),
        ])
        .send()
        .await
        .map_err(|e| format!("Steam could not be reached: {e}"))?
        .error_for_status()
        .map_err(|e| match e.status().map(|s| s.as_u16()) {
            Some(401 | 403) => "Steam rejected the Web API key. Check it in Settings > Integrations.".to_owned(),
            _ => format!("Steam could not be reached: {e}"),
        })?
        .json()
        .await
        .map_err(|e| format!("Steam sent an unexpected answer: {e}"))?;
    playtime_from(&response)
}

fn playtime_from(response: &Value) -> Result<HashMap<u32, Playtime>, String> {
    // A private profile gets an empty answer rather than an error.
    let games = response
        .pointer("/response/games")
        .and_then(Value::as_array)
        .ok_or("Your Steam profile's game details are private, so Steam did not share play time.")?;
    Ok(games
        .iter()
        .filter_map(|g| {
            let minutes = |key: &str| g.get(key).and_then(Value::as_u64).unwrap_or(0) * 60;
            Some((
                u32::try_from(g.get("appid")?.as_u64()?).ok()?,
                Playtime {
                    total_secs: minutes("playtime_forever"),
                    recent_secs: minutes("playtime_2weeks"),
                    last_played: g.get("rtime_last_played").and_then(Value::as_u64).filter(|t| *t > 0),
                },
            ))
        })
        .collect())
}

/// Steam news is BBCode or HTML with links and images; keep a short plain-text summary.
fn plain_text(contents: &str) -> String {
    let mut out = String::with_capacity(contents.len());
    let mut skip_until: Option<char> = None;
    for c in contents.chars() {
        if let Some(end) = skip_until {
            if c == end {
                skip_until = None;
                out.push(' ');
            }
            continue;
        }
        match c {
            '[' => skip_until = Some(']'),
            '<' => skip_until = Some('>'),
            c => out.push(c),
        }
    }
    let words: Vec<&str> = out
        .split_whitespace()
        .filter(|w| !w.starts_with("http://") && !w.starts_with("https://") && !w.starts_with("{STEAM_CLAN_IMAGE}"))
        .collect();
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_play_time_from_the_owned_games_answer() {
        let answer = serde_json::json!({ "response": { "game_count": 2, "games": [
            { "appid": 570, "playtime_forever": 125, "playtime_2weeks": 30, "rtime_last_played": 1764000000 },
            { "appid": 4000, "playtime_forever": 0, "rtime_last_played": 0 }
        ] } });
        let played = playtime_from(&answer).unwrap();
        assert_eq!(
            played[&570],
            Playtime {
                total_secs: 7500,
                recent_secs: 1800,
                last_played: Some(1_764_000_000)
            }
        );
        assert_eq!(played[&4000].last_played, None);
        assert!(
            playtime_from(&serde_json::json!({ "response": {} })).is_err(),
            "private profile"
        );
    }

    #[test]
    fn news_becomes_plain_text() {
        assert_eq!(
            plain_text(
                "[h1]Patch 1.2[/h1]\n[img]{STEAM_CLAN_IMAGE}/x.png[/img] Fixed <b>crashes</b>. https://x.y/z ..."
            ),
            "Patch 1.2 Fixed crashes . ..."
        );
    }
}
