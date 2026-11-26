//! Release notes for the changelog, read from Starex's GitHub releases.

use serde::{Deserialize, Serialize};

const API: &str = "https://api.github.com/repos/setjet/starex/releases?per_page=30";

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    html_url: String,
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// One published version, newest first.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    version: String,
    name: String,
    notes: String,
    url: String,
    /// RFC 3339, as GitHub sends it.
    published_at: Option<String>,
    prerelease: bool,
}

/// Every published release of Starex, newest first. An empty list when there are none yet.
pub async fn fetch(http: &reqwest::Client) -> Result<Vec<Release>, String> {
    let response = http
        .get(API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|_| "GitHub could not be reached. Check your connection and try again.".to_owned())?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    if response.status() == reqwest::StatusCode::FORBIDDEN
        || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return Err("GitHub is limiting requests right now. Try again in a few minutes.".into());
    }
    let releases: Vec<GithubRelease> = response
        .error_for_status()
        .map_err(|e| format!("GitHub answered with an error: {e}"))?
        .json()
        .await
        .map_err(|e| format!("GitHub sent an unexpected answer: {e}"))?;

    Ok(releases
        .into_iter()
        .filter(|r| !r.draft)
        .map(|r| Release {
            version: r.tag_name.trim_start_matches('v').to_owned(),
            name: r.name.unwrap_or_default(),
            notes: r.body.unwrap_or_default(),
            url: r.html_url,
            published_at: r.published_at,
            prerelease: r.prerelease,
        })
        .collect())
}
