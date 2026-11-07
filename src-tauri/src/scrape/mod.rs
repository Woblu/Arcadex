//! Metadata providers.
//!
//! ScreenScraper is tried first because it identifies a ROM by hash, which is
//! exact. TheGamesDB is the fallback: it only matches on name, but it has no
//! per-user quota, so it keeps working once ScreenScraper's daily allowance is
//! spent.
//!
//! Both are community metadata databases that serve artwork and descriptions.
//! Neither is used to fetch games themselves — this app only ever reads
//! metadata for ROMs already on disk.

pub mod libretro;
pub mod screenscraper;
pub mod thegamesdb;

use std::collections::HashMap;
use std::path::Path;

use crate::db;
use crate::error::Result;
use crate::media;
use crate::models::Game;

#[derive(Debug, Clone, Default)]
pub struct GameMeta {
    pub title: Option<String>,
    pub description: Option<String>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    pub genre: Option<String>,
    pub release_date: Option<String>,
    pub players: Option<String>,
    /// Normalised to a 0–5 scale regardless of what the provider uses.
    pub rating: Option<f64>,
    pub cover_url: Option<String>,
    pub screenshot_url: Option<String>,
    pub logo_url: Option<String>,
    /// The system the provider says this is, as one of our slugs. Only used
    /// to fill in a platform we failed to work out ourselves.
    pub platform: Option<String>,
}

#[derive(Debug)]
pub enum Outcome {
    Found(Box<GameMeta>),
    NotFound,
    /// Provider refused because the account is out of requests for now.
    Quota(String),
    Error(String),
}

#[derive(Debug, Clone, Default)]
pub struct Credentials {
    pub ss_devid: String,
    pub ss_devpassword: String,
    pub ss_user: String,
    pub ss_password: String,
    pub tgdb_key: String,
    pub preferred_region: String,
    pub preferred_lang: String,
    /// libretro-thumbnails needs no account, so it is on unless turned off.
    pub libretro_enabled: bool,
}

impl Credentials {
    pub fn from_settings(s: &HashMap<String, String>) -> Self {
        let get = |k: &str, d: &str| s.get(k).cloned().unwrap_or_else(|| d.to_string());
        Credentials {
            ss_devid: get("ss_devid", ""),
            ss_devpassword: get("ss_devpassword", ""),
            ss_user: get("ss_user", ""),
            ss_password: get("ss_password", ""),
            tgdb_key: get("tgdb_key", ""),
            preferred_region: get("preferred_region", "us"),
            preferred_lang: get("preferred_lang", "en"),
            libretro_enabled: get("libretro_enabled", "1") != "0",
        }
    }

    /// True when anything at all can be fetched. libretro alone is enough.
    pub fn has_any(&self) -> bool {
        self.libretro_enabled || self.has_screenscraper() || self.has_tgdb()
    }

    pub fn has_screenscraper(&self) -> bool {
        !self.ss_devid.is_empty() && !self.ss_devpassword.is_empty()
    }

    pub fn has_tgdb(&self) -> bool {
        !self.tgdb_key.is_empty()
    }
}

/// What happened for one game, reported back to the UI.
pub struct ScrapeResult {
    pub status: String,
    pub source: Option<String>,
    /// Set when the whole run should stop, e.g. quota exhausted.
    pub halt: Option<String>,
}

/// How long a provider that says its allowance is spent is left alone before
/// being asked again. ScreenScraper's commonest refusal is per minute.
const QUOTA_REST: std::time::Duration = std::time::Duration::from_secs(60);

/// The providers that ran out of allowance during a run, and until when to
/// leave each one be.
///
/// One provider's limit used to stop the whole run, though the others could
/// carry on: a five-thousand-game fetch ended at the first burst limit
/// ScreenScraper hit. Now that provider sits out for a while and the rest
/// keep going, and the run only stops when nothing at all is left to ask.
#[derive(Default)]
pub struct Spent {
    screenscraper: Option<std::time::Instant>,
    tgdb: Option<std::time::Instant>,
    /// Said once in the summary at the end.
    pub notes: Vec<String>,
}

impl Spent {
    fn resting(until: Option<std::time::Instant>) -> bool {
        until.is_some_and(|t| std::time::Instant::now() < t)
    }

    fn rest(&mut self, which: &str, msg: &str) {
        let until = Some(std::time::Instant::now() + QUOTA_REST);
        match which {
            "ScreenScraper" => self.screenscraper = until,
            _ => self.tgdb = until,
        }
        let note = format!("{which}: {msg}");
        if !self.notes.contains(&note) {
            self.notes.push(note);
        }
    }

    /// Whether anything can still be asked about a game right now.
    fn anything_left(&self, creds: &Credentials) -> bool {
        creds.libretro_enabled
            || (creds.has_screenscraper() && !Self::resting(self.screenscraper))
            || (creds.has_tgdb() && !Self::resting(self.tgdb))
    }
}

/// Look a single game up, download its art, and write it to the library.
pub async fn scrape_game(
    client: &reqwest::Client,
    conn: &std::sync::Mutex<rusqlite::Connection>,
    creds: &Credentials,
    media_root: &Path,
    game: &Game,
    spent: &mut Spent,
) -> Result<ScrapeResult> {
    let mut meta: Option<GameMeta> = None;
    let mut source: Option<String> = None;
    let mut last_error: Option<String> = None;
    let mut halt: Option<String> = None;

    // ScreenScraper first: it matches on hash, so it is the only exact one.
    if creds.has_screenscraper() && !Spent::resting(spent.screenscraper) {
        match screenscraper::lookup(client, creds, game).await {
            Outcome::Found(m) => {
                meta = Some(*m);
                source = Some("screenscraper".into());
            }
            Outcome::NotFound => {}
            Outcome::Quota(msg) => spent.rest("ScreenScraper", &msg),
            Outcome::Error(e) => last_error = Some(e),
        }
    }

    if meta.is_none() && creds.has_tgdb() && !Spent::resting(spent.tgdb) {
        match thegamesdb::lookup(client, creds, game).await {
            Outcome::Found(m) => {
                meta = Some(*m);
                source = Some("thegamesdb".into());
            }
            Outcome::NotFound => {}
            Outcome::Quota(msg) => spent.rest("TheGamesDB", &msg),
            Outcome::Error(e) => last_error = Some(e),
        }
    }
    if !spent.anything_left(creds) {
        halt = Some(spent.notes.join("; "));
    }

    // Fill in artwork from libretro-thumbnails, which needs no credentials.
    // Only asked when we still lack a cover, so it costs nothing otherwise.
    let wants_art = meta.as_ref().map_or(true, |m| m.cover_url.is_none());
    if creds.libretro_enabled && wants_art {
        if let Some(art) = libretro::lookup(client, game).await {
            match meta.as_mut() {
                Some(existing) => {
                    existing.cover_url = art.cover_url;
                    if existing.screenshot_url.is_none() {
                        existing.screenshot_url = art.screenshot_url;
                    }
                    if existing.logo_url.is_none() {
                        existing.logo_url = art.logo_url;
                    }
                }
                None => {
                    meta = Some(art);
                    source = Some("libretro".into());
                }
            }
        }
    }

    if let Some(found) = meta {
        let src = source.clone().unwrap_or_else(|| "libretro".into());
        store(conn, media_root, client, game, &found, &src).await?;
        return Ok(ScrapeResult {
            status: "ok".into(),
            source,
            halt,
        });
    }

    let status = if last_error.is_some() { "error" } else { "notfound" };
    {
        let guard = conn.lock().unwrap();
        db::set_scrape_status(&guard, game.id, status)?;
    }

    Ok(ScrapeResult {
        status: status.into(),
        source: None,
        halt,
    })
}

async fn store(
    conn: &std::sync::Mutex<rusqlite::Connection>,
    media_root: &Path,
    client: &reqwest::Client,
    game: &Game,
    meta: &GameMeta,
    source: &str,
) -> Result<()> {
    let cover = match &meta.cover_url {
        Some(u) => media::download_image(client, u, media_root, game.id, "cover").await?,
        None => None,
    };
    let screenshot = match &meta.screenshot_url {
        Some(u) => media::download_image(client, u, media_root, game.id, "screenshot").await?,
        None => None,
    };
    let logo = match &meta.logo_url {
        Some(u) => media::download_image(client, u, media_root, game.id, "logo").await?,
        None => None,
    };

    let record = db::Metadata {
        title: meta.title.clone(),
        description: meta.description.clone(),
        developer: meta.developer.clone(),
        publisher: meta.publisher.clone(),
        genre: meta.genre.clone(),
        release_date: meta.release_date.clone(),
        players: meta.players.clone(),
        rating: meta.rating,
        cover_path: cover,
        screenshot_path: screenshot,
        logo_path: logo,
        source: source.to_string(),
    };

    let guard = conn.lock().unwrap();
    // A provider that identified the game knows its system, so let it settle a
    // platform we could not work out from the file. Only when we have nothing:
    // a platform already on the record may have been set by hand, and a
    // name-matched provider is not evidence enough to overrule that.
    if game.platform == "unknown" {
        if let Some(slug) = meta.platform.as_deref() {
            // Sets scrape_status back to 'pending'; apply_metadata below puts
            // it straight to 'ok', so this does not queue a second scrape.
            db::set_platform(&guard, game.id, slug)?;
        }
    }
    db::apply_metadata(&guard, game.id, &record)?;
    Ok(())
}

// -------------------------------------------------- credential checking

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub provider: String,
    /// False when nothing has been entered for this provider yet.
    pub configured: bool,
    pub ok: bool,
    pub message: String,
    /// What the provider says is left of your allowance, when it reports it.
    pub quota: Option<String>,
}

/// Ask each provider whether the stored credentials actually work, so a bad
/// key surfaces here rather than as a wall of failures mid-scrape.
pub async fn check_credentials(
    client: &reqwest::Client,
    creds: &Credentials,
) -> Vec<ProviderStatus> {
    vec![
        check_libretro(client, creds).await,
        check_screenscraper(client, creds).await,
        check_tgdb(client, creds).await,
    ]
}

async fn check_screenscraper(client: &reqwest::Client, creds: &Credentials) -> ProviderStatus {
    let mut status = ProviderStatus {
        provider: "ScreenScraper".into(),
        configured: creds.has_screenscraper(),
        ok: false,
        message: "No developer credentials entered".into(),
        quota: None,
    };
    if !status.configured {
        return status;
    }

    let mut query: Vec<(&str, String)> = vec![
        ("devid", creds.ss_devid.clone()),
        ("devpassword", creds.ss_devpassword.clone()),
        ("softname", "arcadex".to_string()),
        ("output", "json".to_string()),
    ];
    if !creds.ss_user.is_empty() {
        query.push(("ssid", creds.ss_user.clone()));
        query.push(("sspassword", creds.ss_password.clone()));
    }

    let resp = client
        .get("https://api.screenscraper.fr/api2/ssuserInfos.php")
        .query(&query)
        .send()
        .await;

    let Ok(resp) = resp else {
        status.message = "Could not reach screenscraper.fr".into();
        return status;
    };

    let code = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if code.as_u16() == 401 || code.as_u16() == 403 {
        status.message = "Credentials rejected — check the developer id and password".into();
        return status;
    }

    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(json) => {
            if let Some(user) = json.get("response").and_then(|r| r.get("ssuser")) {
                let today = user
                    .get("requeststoday")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let max = user
                    .get("maxrequestsperday")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                status.ok = true;
                status.message = "Developer key and account accepted".into();
                status.quota = Some(format!("{today} of {max} requests used today"));
            } else {
                status.message = "Responded, but without account details — \
                                  the developer key may not be approved yet"
                    .into();
            }
        }
        Err(_) => {
            let snippet = body.chars().take(120).collect::<String>();
            status.message = if snippet.trim().is_empty() {
                format!("Unexpected response ({code})")
            } else {
                snippet.trim().to_string()
            };
        }
    }

    status
}

async fn check_tgdb(client: &reqwest::Client, creds: &Credentials) -> ProviderStatus {
    let mut status = ProviderStatus {
        provider: "TheGamesDB".into(),
        configured: creds.has_tgdb(),
        ok: false,
        message: "No API key entered".into(),
        quota: None,
    };
    if !status.configured {
        return status;
    }

    let resp = client
        .get("https://api.thegamesdb.net/v1/Genres")
        .query(&[("apikey", creds.tgdb_key.as_str())])
        .send()
        .await;

    let Ok(resp) = resp else {
        status.message = "Could not reach thegamesdb.net".into();
        return status;
    };

    let code = resp.status();
    if code.as_u16() == 401 || code.as_u16() == 403 {
        status.message = "API key rejected".into();
        return status;
    }

    match resp.json::<serde_json::Value>().await {
        Ok(json) => {
            let has_data = json
                .get("data")
                .and_then(|d| d.get("genres"))
                .is_some();
            if has_data {
                status.ok = true;
                status.message = "API key accepted".into();
                if let Some(left) = json
                    .get("remaining_monthly_allowance")
                    .and_then(|v| v.as_i64())
                {
                    status.quota = Some(format!("{left} requests left this month"));
                }
            } else {
                status.message = json
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("Unexpected response")
                    .to_string();
            }
        }
        Err(_) => status.message = format!("Unexpected response ({code})"),
    }

    status
}

/// libretro has no account to verify, so this just proves the server is
/// reachable and reports that it needs no setup.
async fn check_libretro(client: &reqwest::Client, creds: &Credentials) -> ProviderStatus {
    let mut status = ProviderStatus {
        provider: "libretro thumbnails".into(),
        configured: creds.libretro_enabled,
        ok: false,
        message: "Switched off in Settings".into(),
        quota: None,
    };
    if !creds.libretro_enabled {
        return status;
    }

    // A file known to exist, used purely as a reachability probe.
    const PROBE: &str = "https://thumbnails.libretro.com/Nintendo%20-%20Super%20Nintendo%20Entertainment%20System/Named_Boxarts/Super%20Mario%20World%20%28USA%29.png";

    match client.head(PROBE).send().await {
        Ok(r) if r.status().is_success() => {
            status.ok = true;
            status.message = "Reachable — no account or key needed".into();
            status.quota = Some("Unlimited".into());
        }
        Ok(r) => status.message = format!("Server returned {}", r.status()),
        Err(_) => status.message = "Could not reach thumbnails.libretro.com".into(),
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds(ss: bool, tgdb: bool, libretro: bool) -> Credentials {
        Credentials {
            ss_devid: if ss { "id".into() } else { String::new() },
            ss_devpassword: if ss { "pw".into() } else { String::new() },
            tgdb_key: if tgdb { "key".into() } else { String::new() },
            libretro_enabled: libretro,
            ..Default::default()
        }
    }

    /// One provider's limit rests that provider; the run goes on while
    /// anything is left to ask, and only then stops.
    #[test]
    fn a_spent_provider_rests_while_the_others_carry_on() {
        let both = creds(true, true, false);
        let mut spent = Spent::default();
        spent.rest("ScreenScraper", "request quota reached");
        assert!(Spent::resting(spent.screenscraper));
        assert!(spent.anything_left(&both), "TheGamesDB can still answer");

        spent.rest("TheGamesDB", "monthly allowance used");
        assert!(!spent.anything_left(&both), "nothing left: the run stops");
        assert_eq!(spent.notes.len(), 2);

        // libretro has no allowance to spend, so it always has something.
        assert!(spent.anything_left(&creds(true, true, true)));
    }
}
