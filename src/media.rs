//! Shared media cards and artwork for Jellyfin and Plex.
/// What Play on the title card asks for. Empty fields keep the defaults.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayRequest {
    pub id: i64,
    /// (season, episode) numbers; None plays the next unwatched episode.
    pub episode: Option<(i64, i64)>,
    /// File quality label such as "1080p"; None picks the best one.
    pub quality: Option<String>,
    /// Subtitle label from the card (e.g. "English"), matched in the played
    /// file's own list; None plays without.
    pub subtitle: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Episode {
    pub season: i64,
    pub number: i64,
    pub title: String,
    pub minutes: i64,
    /// Watched share of the episode, 0..1.
    pub progress: f32,
    pub watched: bool,
    pub thumb_url: String,
    pub thumb: Option<String>,
}

/// A media title from a linked Jellyfin or Plex server, reduced to display-ready fields.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Detail {
    pub id: i64,
    pub serial: bool,
    pub title: String,
    pub year: i64,
    pub minutes: i64,
    pub genres: String,
    pub countries: String,
    pub plot: String,
    pub director: String,
    pub cast: String,
    pub imdb: Option<f64>,
    pub kinopoisk: Option<f64>,
    /// "4K", "5.1" and similar badges.
    pub tags: Vec<String>,
    pub poster: Option<String>,
    pub backdrop: Option<String>,
    pub audio: Vec<String>,
    /// (label, file url)
    pub subtitles: Vec<(String, String)>,
    pub qualities: Vec<String>,
    pub trailer: Option<String>,
    pub seasons: Vec<i64>,
    pub episodes: Vec<Episode>,
    /// Index into episodes: the first one not watched to the end.
    pub next: Option<usize>,
}

/// A 16:9 backdrop cut from the upper part of a portrait poster and blurred,
/// so the crop's low resolution never shows.
pub(crate) fn poster_backdrop(cache: &std::path::Path, name: &str, poster: &str) -> Option<String> {
    let path = cache.join(format!("{name}.jpg"));
    if path.exists() {
        return Some(path.to_string_lossy().into_owned());
    }
    let image = crate::renderer::open_image(poster).ok()?;
    let (w, h) = (image.width(), image.height());
    let crop_h = (w * 9 / 16).min(h);
    let top = (h - crop_h) / 3;
    let wide = image
        .crop_imm(0, top, w, crop_h)
        .resize_exact(480, 270, image::imageops::FilterType::Triangle)
        .into_rgb8();
    image::imageops::blur(&wide, 6.).save(&path).ok()?;
    Some(path.to_string_lossy().into_owned())
}

/// Stills of the season the title card opens on (the next episode's, else
/// the first): the card asks only for the other seasons itself.
pub(crate) fn season_stills(agent: &ureq::Agent, cache: &std::path::Path, detail: &mut Detail) {
    let id = detail.id;
    let season = detail
        .next
        .and_then(|i| detail.episodes.get(i))
        .map(|e| e.season)
        .or_else(|| detail.seasons.first().copied());
    for (index, episode) in detail.episodes.iter_mut().enumerate() {
        if Some(episode.season) == season && !episode.thumb_url.is_empty() {
            episode.thumb =
                download_as(agent, cache, &format!("{id}-e{index}"), &episode.thumb_url);
        }
    }
}

pub(crate) fn download_as(
    agent: &ureq::Agent,
    cache: &std::path::Path,
    name: &str,
    url: &str,
) -> Option<String> {
    if !url.starts_with("http") {
        return None;
    }
    let path = cache.join(format!("{name}.jpg"));
    if path.exists() {
        return Some(path.to_string_lossy().into_owned());
    }
    let mut response = agent.get(url).call().ok()?;
    if response.status().as_u16() != 200 {
        return None;
    }
    let bytes = response
        .body_mut()
        .with_config()
        .limit(4 * 1024 * 1024)
        .read_to_vec()
        .ok()?;
    std::fs::write(&path, bytes).ok()?;
    Some(path.to_string_lossy().into_owned())
}

/// One page of a Library grid.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LibraryPage {
    /// (item id, title, poster url)
    pub items: Vec<(i64, String, String)>,
    /// Pages in the whole list; 0 when the API did not say.
    pub pages: u32,
    pub total: usize,
}

/// Fetch a Library poster into its cache directory.
pub fn library_poster(dir: &std::path::Path, id: i64, url: &str) -> Option<String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .build()
        .new_agent();
    download_as(&agent, dir, &id.to_string(), url)
}

/// A title id with its (episode index, still URL or cached path) list.
type Thumbs = (i64, Vec<(usize, String)>);

/// Download episode stills without blocking the UI thread.
struct ThumbService {
    sender: std::sync::mpsc::Sender<Thumbs>,
    receiver: std::sync::Mutex<std::sync::mpsc::Receiver<Thumbs>>,
}
fn thumb_service() -> &'static ThumbService {
    static SERVICE: std::sync::OnceLock<ThumbService> = std::sync::OnceLock::new();
    SERVICE.get_or_init(|| {
        let (sender, inbox) = std::sync::mpsc::channel::<Thumbs>();
        let (outbox, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(20)))
                .build()
                .new_agent();
            let cache = std::env::temp_dir().join("luma-media-stills");
            let _ = std::fs::create_dir_all(&cache);
            for (id, list) in inbox {
                let done = list
                    .into_iter()
                    .filter_map(|(i, url)| {
                        download_as(&agent, &cache, &format!("{id}-e{i}"), &url)
                            .map(|path| (i, path))
                    })
                    .collect();
                let _ = outbox.send((id, done));
            }
        });
        ThumbService {
            sender,
            receiver: std::sync::Mutex::new(receiver),
        }
    })
}
pub fn thumbs_request(id: i64, list: Vec<(usize, String)>) {
    let _ = thumb_service().sender.send((id, list));
}
pub fn thumbs_poll() -> Vec<Thumbs> {
    thumb_service()
        .receiver
        .lock()
        .unwrap()
        .try_iter()
        .collect()
}
