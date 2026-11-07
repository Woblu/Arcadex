//! ScreenScraper (screenscraper.fr) API v2.
//!
//! Games are looked up by CRC32/MD5/SHA1 of the ROM, so a correct dump matches
//! exactly rather than by fuzzy name. The API needs two sets of credentials:
//! a developer id/password issued to the application, and the end user's own
//! ScreenScraper account. Both are entered in Settings; nothing is bundled.

use serde_json::Value;

use super::{Credentials, GameMeta, Outcome};
use crate::models::Game;
use crate::platforms;

const BASE: &str = "https://api.screenscraper.fr/api2/jeuInfos.php";
const SOFTNAME: &str = "arcadex";

/// Region codes in the order we would like artwork and titles.
fn region_order(pref: &str) -> Vec<String> {
    let mut v = vec![pref.to_ascii_lowercase()];
    for r in ["wor", "us", "eu", "jp", "ss"] {
        if !v.iter().any(|x| x == r) {
            v.push(r.to_string());
        }
    }
    v
}

fn lang_order(pref: &str) -> Vec<String> {
    let mut v = vec![pref.to_ascii_lowercase()];
    for l in ["en", "fr"] {
        if !v.iter().any(|x| x == l) {
            v.push(l.to_string());
        }
    }
    v
}

/// Pick `text` from a list of `{region, text}` objects, honouring preference.
fn pick_by_region(list: Option<&Value>, regions: &[String]) -> Option<String> {
    let arr = list?.as_array()?;
    for want in regions {
        for item in arr {
            if item.get("region").and_then(|r| r.as_str()).map(|r| r.eq_ignore_ascii_case(want))
                == Some(true)
            {
                if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                    if !t.trim().is_empty() {
                        return Some(t.trim().to_string());
                    }
                }
            }
        }
    }
    arr.iter()
        .find_map(|i| i.get("text").and_then(|t| t.as_str()))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Pick `text` from a list of `{langue, text}` objects.
fn pick_by_lang(list: Option<&Value>, langs: &[String]) -> Option<String> {
    let arr = list?.as_array()?;
    for want in langs {
        for item in arr {
            if item.get("langue").and_then(|l| l.as_str()).map(|l| l.eq_ignore_ascii_case(want))
                == Some(true)
            {
                if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                    if !t.trim().is_empty() {
                        return Some(t.trim().to_string());
                    }
                }
            }
        }
    }
    arr.iter()
        .find_map(|i| i.get("text").and_then(|t| t.as_str()))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn text_field(jeu: &Value, key: &str) -> Option<String> {
    jeu.get(key)
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Genres arrive as a list of objects each holding their own localised names.
fn genres(jeu: &Value, langs: &[String]) -> Option<String> {
    let arr = jeu.get("genres")?.as_array()?;
    let names: Vec<String> = arr
        .iter()
        .filter_map(|g| pick_by_lang(g.get("noms"), langs))
        .collect();
    if names.is_empty() {
        None
    } else {
        Some(names.join(", "))
    }
}

/// Find the best media URL for a set of acceptable media types.
fn media_url(jeu: &Value, kinds: &[&str], regions: &[String]) -> Option<String> {
    let arr = jeu.get("medias")?.as_array()?;
    for kind in kinds {
        // Prefer a matching region, then anything of the right type.
        for want in regions {
            for m in arr {
                let t = m.get("type").and_then(|t| t.as_str()).unwrap_or("");
                let r = m.get("region").and_then(|r| r.as_str()).unwrap_or("");
                if t.eq_ignore_ascii_case(kind) && r.eq_ignore_ascii_case(want) {
                    if let Some(u) = m.get("url").and_then(|u| u.as_str()) {
                        return Some(u.to_string());
                    }
                }
            }
        }
        for m in arr {
            let t = m.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if t.eq_ignore_ascii_case(kind) {
                if let Some(u) = m.get("url").and_then(|u| u.as_str()) {
                    return Some(u.to_string());
                }
            }
        }
    }
    None
}

fn looks_like_quota(body: &str) -> bool {
    let b = body.to_ascii_lowercase();
    b.contains("quota")
        || b.contains("maximum threads")
        || b.contains("closed for non-registered")
        || b.contains("scraping quota")
        || b.contains("api totalement fermé")
        || b.contains("nombre de requetes")
}

pub async fn lookup(client: &reqwest::Client, creds: &Credentials, game: &Game) -> Outcome {
    // The name the scraper indexes is the ROM's own name, which for a zipped
    // dump is the entry inside the archive.
    let rom_name = game
        .inner_name
        .clone()
        .unwrap_or_else(|| game.filename.clone());

    let mut query: Vec<(&str, String)> = vec![
        ("devid", creds.ss_devid.clone()),
        ("devpassword", creds.ss_devpassword.clone()),
        ("softname", SOFTNAME.to_string()),
        ("output", "json".to_string()),
        ("romnom", rom_name),
    ];

    if !creds.ss_user.is_empty() {
        query.push(("ssid", creds.ss_user.clone()));
        query.push(("sspassword", creds.ss_password.clone()));
    }
    if let Some(crc) = &game.crc32 {
        query.push(("crc", crc.to_uppercase()));
    }
    if let Some(md5) = &game.md5 {
        query.push(("md5", md5.to_uppercase()));
    }
    if let Some(sha1) = &game.sha1 {
        query.push(("sha1", sha1.to_uppercase()));
    }
    if game.size > 0 {
        query.push(("romtaille", game.size.to_string()));
    }
    // A console game too large to have been hashed has its serial instead,
    // the one identifier it carries that is not a guess. Only sent then: a
    // checksum is the better key, and where there is one it is used alone.
    if game.crc32.is_none() {
        if let Some(serial) = &game.serial {
            query.push(("serialnum", serial.clone()));
        }
    }

    let resp = match client.get(BASE).query(&query).send().await {
        Ok(r) => r,
        Err(e) => return Outcome::Error(format!("request failed: {e}")),
    };

    let status = resp.status();
    let body = match resp.text().await {
        Ok(b) => b,
        Err(e) => return Outcome::Error(format!("unreadable response: {e}")),
    };

    if status.as_u16() == 404 {
        return Outcome::NotFound;
    }
    if status.as_u16() == 429 || status.as_u16() == 430 || status.as_u16() == 431 {
        return Outcome::Quota("request quota reached".into());
    }
    if looks_like_quota(&body) {
        return Outcome::Quota("request quota reached".into());
    }
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Outcome::Error("credentials rejected".into());
    }

    let json: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => {
            let snippet = body.chars().take(120).collect::<String>();
            if snippet.trim().is_empty() {
                return Outcome::NotFound;
            }
            return Outcome::Error(snippet.trim().to_string());
        }
    };

    let Some(jeu) = json.get("response").and_then(|r| r.get("jeu")) else {
        return Outcome::NotFound;
    };

    let regions = region_order(&creds.preferred_region);
    let langs = lang_order(&creds.preferred_lang);

    let rating = text_field(jeu, "note")
        .and_then(|n| n.parse::<f64>().ok())
        // ScreenScraper scores out of 20; the UI shows stars out of 5.
        .map(|n| (n / 4.0).clamp(0.0, 5.0));

    let meta = GameMeta {
        title: pick_by_region(jeu.get("noms"), &regions),
        description: pick_by_lang(jeu.get("synopsis"), &langs),
        developer: text_field(jeu, "developpeur"),
        publisher: text_field(jeu, "editeur"),
        genre: genres(jeu, &langs),
        release_date: pick_by_region(jeu.get("dates"), &regions),
        players: text_field(jeu, "joueurs"),
        platform: text_field(jeu, "systeme")
            .as_deref()
            .and_then(platforms::match_alias)
            .map(|p| p.slug.to_string()),
        rating,
        cover_url: media_url(jeu, &["box-2D", "box-3D", "mixrbv1"], &regions),
        screenshot_url: media_url(jeu, &["ss", "sstitle"], &regions),
        logo_url: media_url(jeu, &["wheel-hd", "wheel", "screenmarquee"], &regions),
    };

    Outcome::Found(Box::new(meta))
}
