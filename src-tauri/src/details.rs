//! Game details (description, genres, release date, developer, time to beat) from IGDB.
//!
//! IGDB is run by Twitch and needs a free Twitch developer application: the user adds its
//! client ID and secret in Settings. Starex sends a game's name to IGDB only when it looks up
//! that game's details.

use serde::Deserialize;
use starex_core::model::GameDetails;

const TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";
const API: &str = "https://api.igdb.com/v4";

#[derive(Deserialize)]
struct Token {
    access_token: String,
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
struct InvolvedCompany {
    company: Option<Named>,
    #[serde(default)]
    developer: bool,
}

#[derive(Deserialize)]
struct IgdbGame {
    id: u64,
    summary: Option<String>,
    #[serde(default)]
    genres: Vec<Named>,
    first_release_date: Option<i64>,
    #[serde(default)]
    involved_companies: Vec<InvolvedCompany>,
    url: Option<String>,
}

#[derive(Deserialize)]
struct TimeToBeat {
    normally: Option<u64>,
    completely: Option<u64>,
}

/// Looks `name` up on IGDB.
pub async fn fetch(
    http: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    name: &str,
) -> Result<GameDetails, String> {
    let (client_id, client_secret) = (client_id.trim(), client_secret.trim());
    if client_id.is_empty() || client_secret.is_empty() {
        return Err("Add your IGDB (Twitch) client ID and secret in Settings > Integrations first.".into());
    }
    let token: Token = http
        .post(TOKEN_URL)
        .query(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("grant_type", "client_credentials"),
        ])
        .send()
        .await
        .map_err(|e| format!("Twitch could not be reached: {e}"))?
        .error_for_status()
        .map_err(|_| "Twitch rejected the IGDB client ID or secret. Check them in Settings > Integrations.".to_owned())?
        .json()
        .await
        .map_err(|e| format!("Twitch sent an unexpected answer: {e}"))?;

    let query = |endpoint: &'static str, body: String| {
        http.post(format!("{API}/{endpoint}"))
            .header("Client-ID", client_id)
            .bearer_auth(&token.access_token)
            .body(body)
            .send()
    };

    let escaped = name.trim().replace(['\\', '"'], " ");
    let games: Vec<IgdbGame> = query(
        "games",
        format!(
            "search \"{escaped}\"; fields summary,genres.name,first_release_date,involved_companies.company.name,involved_companies.developer,url; where version_parent = null; limit 1;"
        ),
    )
    .await
    .and_then(|r| r.error_for_status())
    .map_err(|e| format!("IGDB could not be reached: {e}"))?
    .json()
    .await
    .map_err(|e| format!("IGDB sent an unexpected answer: {e}"))?;
    let game = games.into_iter().next().ok_or("IGDB has no game with this name.")?;

    // Time to beat comes from a separate endpoint and is often missing; that is fine.
    let times: Vec<TimeToBeat> = match query(
        "game_time_to_beats",
        format!("fields normally,completely; where game_id = {};", game.id),
    )
    .await
    {
        Ok(response) => response.json().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let time = times.into_iter().next();

    let developer = game
        .involved_companies
        .iter()
        .find(|c| c.developer)
        .or(game.involved_companies.first())
        .and_then(|c| c.company.as_ref())
        .map(|c| c.name.clone());
    Ok(GameDetails {
        igdb_id: Some(game.id),
        summary: game.summary,
        genres: game.genres.into_iter().map(|g| g.name).collect(),
        release_date: game.first_release_date,
        developer,
        time_to_beat: time.as_ref().and_then(|t| t.normally),
        time_to_complete: time.and_then(|t| t.completely),
        url: game.url,
    })
}
