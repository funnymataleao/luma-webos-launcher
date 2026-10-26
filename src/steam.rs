//! Game details for Moonlight cards from the public Steam store, matched by
//! title. No account or key; answers and artwork are cached on disk.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, SystemTime},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GameInfo {
    /// 0 when Steam has no game with this title.
    pub steam_id: u64,
    pub description: String,
    pub genres: Vec<String>,
    pub developer: String,
    pub year: String,
    pub metacritic: u32,
    pub controller: bool,
    /// Wide key art, local file.
    pub hero: Option<String>,
    /// Upright 2:3 library art (600x900), local file: the card's poster.
    pub portrait: Option<String>,
    /// Version of the artwork set; older cached answers are fetched again.
    pub art: u32,
    /// Local 16:9 thumbnails.
    pub screenshots: Vec<String>,
    /// The same screenshots at 1920x1080, for the full-screen viewer.
    pub screenshots_full: Vec<String>,
    /// HLS (H.264) trailer for the card background.
    pub trailer: String,
}

pub struct Service {
    sender: Sender<(i64, String)>,
    pub receiver: Mutex<Receiver<(i64, GameInfo)>>,
}
impl Service {
    pub fn new(cache: PathBuf) -> Self {
        let (sender, requests) = mpsc::channel::<(i64, String)>();
        let (replies, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut asked = HashSet::new();
            while let Ok((id, title)) = requests.recv() {
                // Apollo's Desktop is not the Steam game "Desktop".
                if crate::moonlight::is_launcher_app(&title) {
                    let _ = replies.send((id, GameInfo::default()));
                    continue;
                }
                if !asked.insert(title.clone()) {
                    // Already answered this session; the cache answers again.
                    if let Some(info) = cached(&cache, &title) {
                        let _ = replies.send((id, fitted(info)));
                    }
                    continue;
                }
                if let Ok(info) = lookup(&title, &cache) {
                    let _ = replies.send((id, fitted(info)));
                }
            }
        });
        Self {
            sender,
            receiver: Mutex::new(receiver),
        }
    }
    pub fn request(&self, id: i64, title: &str) {
        let _ = self.sender.send((id, title.to_owned()));
    }
}

/// The game panel is 1776x952. Steam key art is about 3:1, and the renderer
/// fills the rect it is given, so crop the middle to the panel's shape once.
const PANEL_ASPECT: f32 = 1776. / 952.;

/// Artwork set this Luma fetches (2: the upright library poster).
const ART_VERSION: u32 = 2;

/// Store art URLs of one app by asset name (library_hero, library_capsule_2x…).
fn asset_urls(agent: &ureq::Agent, id: u64) -> Option<Value> {
    let input = format!(
        "{{\"ids\":[{{\"appid\":{id}}}],\"context\":{{\"language\":\"english\",\"country_code\":\"US\"}},\"data_request\":{{\"include_assets\":true}}}}"
    );
    let v = json(
        agent,
        &format!(
            "https://api.steampowered.com/IStoreBrowseService/GetItems/v1/?input_json={}",
            url_encode(&input)
        ),
    )
    .ok()?;
    Some(v["response"]["store_items"][0]["assets"].clone())
}

fn asset_url(assets: &Value, names: &[&str]) -> Option<String> {
    let format = assets["asset_url_format"].as_str()?;
    let file = names.iter().find_map(|n| assets[*n].as_str())?;
    Some(format!(
        "https://shared.akamai.steamstatic.com/store_item_assets/{}",
        format.replace("${FILENAME}", file)
    ))
}

fn fitted(mut info: GameInfo) -> GameInfo {
    info.hero = info.hero.as_deref().and_then(fit_hero);
    info
}

fn fit_hero(path: &str) -> Option<String> {
    let out = path.replace(".jpg", "-fit.jpg");
    if Path::new(&out).is_file() {
        return Some(out);
    }
    // background_raw is WebP whatever the file is called.
    let image = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?
        .into_rgb8();
    let (w, h) = (image.width() as f32, image.height() as f32);
    let (cw, ch) = if w / h > PANEL_ASPECT {
        (h * PANEL_ASPECT, h)
    } else {
        (w, w / PANEL_ASPECT)
    };
    let x = ((w - cw) / 2.).round() as u32;
    let y = ((h - ch) / 2.).round() as u32;
    let crop = image::imageops::crop_imm(&image, x, y, cw as u32, ch as u32).to_image();
    let width = 1920.min(crop.width());
    let height = (width as f32 / PANEL_ASPECT).round() as u32;
    let fit = image::imageops::resize(&crop, width, height, image::imageops::FilterType::Triangle);
    fit.save(&out).ok()?;
    Some(out)
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(12)))
        .http_status_as_error(false)
        .build()
        .into()
}

/// "HOT WHEELS™ Infinite Rush" and "Hot Wheels Infinite Rush" are one game.
pub fn normalize(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with(' ') && !matches!(c, '™' | '®' | '©' | '\'' | '’') {
            out.push(' ');
        }
    }
    out.trim().to_owned()
}

fn cache_path(cache: &Path, title: &str) -> PathBuf {
    cache.join(format!(
        "steam-{}.json",
        hex::encode(normalize(title).as_bytes())
    ))
}

fn cached(cache: &Path, title: &str) -> Option<GameInfo> {
    let path = cache_path(cache, title);
    let age = fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())?;
    let info: GameInfo = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
    // Misses are retried daily, hits refreshed weekly; a hit without key art
    // or full screenshots (older Luma) is fetched again now.
    if info.steam_id > 0
        && (info.hero.is_none()
            || info.screenshots_full.len() < info.screenshots.len()
            || info.art < ART_VERSION)
    {
        return None;
    }
    let fresh = if info.steam_id == 0 {
        86_400
    } else {
        7 * 86_400
    };
    (age.as_secs() < fresh).then_some(info)
}

fn json(agent: &ureq::Agent, url: &str) -> Result<Value, String> {
    let mut response = agent.get(url).call().map_err(|e| e.to_string())?;
    if response.status() != 200 {
        return Err(format!("Steam HTTP {}", response.status()));
    }
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn download(agent: &ureq::Agent, url: &str, path: &Path) -> Option<String> {
    if path.exists() {
        return Some(path.to_string_lossy().into());
    }
    let mut response = agent.get(url).call().ok()?;
    if response.status() != 200 {
        return None;
    }
    let mut data = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(8_000_000)
        .read_to_end(&mut data)
        .ok()?;
    if !data.starts_with(&[0xff, 0xd8]) && !data.starts_with(b"\x89PNG") {
        return None;
    }
    let temp = path.with_extension("part");
    fs::write(&temp, data).ok()?;
    fs::rename(&temp, path).ok()?;
    Some(path.to_string_lossy().into())
}

/// The best store match for a title: exact after normalising, else a name
/// that starts with it ("Cyberpunk 2077" → "Cyberpunk 2077: Ultimate").
pub fn best_match(items: &[Value], title: &str) -> Option<u64> {
    let want = normalize(title);
    let apps: Vec<(u64, String)> = items
        .iter()
        .filter(|i| i["type"] == "app")
        .filter_map(|i| Some((i["id"].as_u64()?, normalize(i["name"].as_str()?))))
        .collect();
    apps.iter()
        .find(|(_, n)| *n == want)
        .or_else(|| apps.iter().find(|(_, n)| n.starts_with(&want)))
        .map(|(id, _)| *id)
}

fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in text.chars() {
        match c {
            '<' => tag = true,
            '>' => tag = false,
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .trim()
        .to_owned()
}

pub fn lookup(title: &str, cache: &Path) -> Result<GameInfo, String> {
    if let Some(info) = cached(cache, title) {
        return Ok(info);
    }
    let _ = fs::create_dir_all(cache);
    let agent = agent();
    let term: String = url_encode(title);
    let search = json(
        &agent,
        &format!("https://store.steampowered.com/api/storesearch/?term={term}&cc=us&l=en"),
    )?;
    let items = search["items"].as_array().cloned().unwrap_or_default();
    let mut info = GameInfo::default();
    if let Some(id) = best_match(&items, title) {
        let details = json(
            &agent,
            &format!("https://store.steampowered.com/api/appdetails?appids={id}&l=en"),
        )?;
        let d = &details[id.to_string()]["data"];
        info.steam_id = id;
        info.description = plain(d["short_description"].as_str().unwrap_or(""));
        info.genres = d["genres"]
            .as_array()
            .map(|g| {
                g.iter()
                    .filter_map(|x| x["description"].as_str().map(str::to_owned))
                    .take(3)
                    .collect()
            })
            .unwrap_or_default();
        info.developer = d["developers"][0].as_str().unwrap_or("").to_owned();
        let date = d["release_date"]["date"].as_str().unwrap_or("");
        info.year = date
            .split(|c: char| !c.is_ascii_digit())
            .rfind(|p| p.len() == 4)
            .unwrap_or("")
            .to_owned();
        info.metacritic = d["metacritic"]["score"].as_u64().unwrap_or(0) as u32;
        info.controller = d["controller_support"] == "full";
        info.trailer = d["movies"][0]["hls_h264"].as_str().unwrap_or("").to_owned();
        let hero = cache.join(format!("steam-{id}-hero.jpg"));
        // Newer games keep their art under a hashed folder; the store browse
        // API (no key) names it. The old unhashed path still serves older games.
        let assets = asset_urls(&agent, id).unwrap_or(Value::Null);
        let portrait = cache.join(format!("steam-{id}-portrait.jpg"));
        info.art = ART_VERSION;
        info.portrait = asset_url(&assets, &["library_capsule_2x", "library_capsule"])
            .and_then(|url| download(&agent, &url, &portrait))
            .or_else(|| {
                ["library_600x900_2x.jpg", "library_600x900.jpg"]
                    .iter()
                    .find_map(|name| {
                        download(
                            &agent,
                            &format!(
                                "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/{id}/{name}"
                            ),
                            &portrait,
                        )
                    })
            });
        info.hero = asset_url(&assets, &["library_hero"])
            .and_then(|url| download(&agent, &url, &hero))
            .or_else(|| {
                download(
                    &agent,
                    &format!(
                        "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/{id}/library_hero.jpg"
                    ),
                    &hero,
                )
            })
            .or_else(|| download(&agent, d["background_raw"].as_str()?, &hero));
        if let Some(shots) = d["screenshots"].as_array() {
            for (n, shot) in shots.iter().take(3).enumerate() {
                if let Some(url) = shot["path_thumbnail"].as_str() {
                    let path = cache.join(format!("steam-{id}-s{n}.jpg"));
                    if let Some(local) = download(&agent, url, &path) {
                        info.screenshots.push(local);
                        let full = cache.join(format!("steam-{id}-s{n}-full.jpg"));
                        let big = shot["path_full"]
                            .as_str()
                            .and_then(|url| download(&agent, url, &full));
                        info.screenshots_full
                            .push(big.unwrap_or_else(|| info.screenshots[n].clone()));
                    }
                }
            }
        }
    }
    if let Ok(bytes) = serde_json::to_vec(&info) {
        let _ = fs::write(cache_path(cache, title), bytes);
    }
    Ok(info)
}

fn url_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hero_is_cropped_to_the_panel_not_stretched() {
        let dir = std::env::temp_dir().join("luma-steam-fit-test");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("steam-1-hero.jpg");
        image::RgbImage::new(3840, 1240).save(&path).unwrap();
        let _ = fs::remove_file(dir.join("steam-1-hero-fit.jpg"));
        let out = fit_hero(path.to_str().unwrap()).unwrap();
        let fit = image::open(&out).unwrap();
        assert_eq!(fit.width(), 1920);
        let aspect = fit.width() as f32 / fit.height() as f32;
        assert!((aspect - PANEL_ASPECT).abs() < 0.01, "{aspect}");
    }
    #[test]
    fn titles_match_across_trademarks_and_editions() {
        assert_eq!(
            normalize("HOT WHEELS™ Infinite Rush"),
            "hot wheels infinite rush"
        );
        assert_eq!(normalize("Marvel's Spider-Man 2"), "marvels spider man 2");
        let items: Vec<Value> = serde_json::from_str(
            r#"[{"type":"app","name":"007 First Light - Deluxe Edition Upgrade","id":2},{"type":"app","name":"007 First Light","id":1}]"#,
        )
        .unwrap();
        assert_eq!(best_match(&items, "007 First Light"), Some(1));
        assert_eq!(best_match(&items, "Something else"), None);
    }
    #[test]
    #[ignore]
    fn live_steam_lookup() {
        let dir = std::env::temp_dir().join("luma-steam-test");
        let _ = fs::remove_dir_all(&dir);
        let title = std::env::var("LUMA_STEAM_TITLE").unwrap_or_else(|_| "007 First Light".into());
        let info = lookup(&title, &dir).unwrap();
        println!(
            "STEAM id={} year={} genres={:?} dev={} mc={} hero={} shots={} trailer={}",
            info.steam_id,
            info.year,
            info.genres,
            info.developer,
            info.metacritic,
            info.hero.is_some(),
            info.screenshots.len(),
            !info.trailer.is_empty()
        );
        assert!(info.steam_id > 0);
    }
}
