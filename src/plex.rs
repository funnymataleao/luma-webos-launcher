//! Plex for Home stacks. Luma signs in with a plex.tv PIN: the TV shows a
//! four-letter code for plex.tv/link (or a QR with it filled in) and polls
//! until the user approves it. Tokens are never logged.
use crate::media::{Detail, Episode, PlayRequest};
use crate::stacks::StackItem;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

const PLEX_TV: &str = "https://plex.tv";

/// Saved sign-in: the plex.tv account and the media server Luma reads.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct PlexLink {
    /// Random per-install id; plex.tv lists Luma as its own device.
    pub client_id: String,
    pub account_token: String,
    pub user_name: String,
    pub server_name: String,
    pub server_id: String,
    /// Connection that answered when signing in (local first).
    pub server_uri: String,
    pub server_token: String,
}

impl PlexLink {
    pub fn linked(&self) -> bool {
        !self.server_token.is_empty() && !self.server_uri.is_empty()
    }
}

/// Row contents: (id, label). Libraries follow as "section:<key>".
pub const COLLECTIONS: [(&str, &str); 2] = [("ondeck", "On Deck"), ("recent", "Recently added")];

pub enum Command {
    /// Create a PIN and wait until it is approved on plex.tv.
    Link {
        client_id: String,
    },
    Cancel,
    Fetch(PlexLink, Vec<(usize, String)>),
    Views(PlexLink),
    Details(PlexLink, i64),
    Play(PlexLink, PlayRequest),
}

pub enum Response {
    Code { code: String, qr: String },
    Linked(PlexLink),
    Failed(String),
    Items(usize, Vec<StackItem>),
    Views(Vec<(String, String)>),
    Details(Box<Detail>),
    Play(Box<crate::player::OpenRequest>),
}

pub struct Service {
    sender: Mutex<mpsc::Sender<Command>>,
    pub receiver: Mutex<mpsc::Receiver<Response>>,
}

static ATTEMPT: AtomicU64 = AtomicU64::new(0);

impl Service {
    pub fn new(cache: PathBuf) -> Self {
        let (sender, commands) = mpsc::channel::<Command>();
        let (reply, receiver) = mpsc::channel();
        thread::spawn(move || {
            let _ = std::fs::create_dir_all(&cache);
            for command in commands {
                match command {
                    Command::Link { client_id } => {
                        let attempt = ATTEMPT.fetch_add(1, Ordering::SeqCst) + 1;
                        let reply = reply.clone();
                        thread::spawn(move || link(&reply, client_id, attempt));
                    }
                    Command::Cancel => {
                        ATTEMPT.fetch_add(1, Ordering::SeqCst);
                    }
                    Command::Fetch(link, stacks) => fetch(&reply, &cache, &link, stacks),
                    Command::Views(link) => {
                        let _ = reply.send(Response::Views(views(&agent(), &link)));
                    }
                    Command::Details(link, id) => details(&reply, &cache, &link, id),
                    Command::Play(link, request) => play(&reply, &link, request),
                }
            }
        });
        Self {
            sender: Mutex::new(sender),
            receiver: Mutex::new(receiver),
        }
    }
    pub fn send(&self, command: Command) {
        let _ = self.sender.lock().unwrap().send(command);
    }
}

/// Plex rating keys are numbers per server; card ids carry this bit so the
/// title card and Play find their way back here (Jellyfin uses 1 << 52).
const CARD_TAG: i64 = 1 << 51;
static CARD_IDS: Mutex<BTreeMap<i64, String>> = Mutex::new(BTreeMap::new());

pub fn card_id(key: &str) -> i64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    let id = (hash as i64 & (CARD_TAG - 1)) | CARD_TAG;
    CARD_IDS.lock().unwrap().insert(id, key.to_owned());
    id
}

pub fn owns(id: i64) -> bool {
    id & CARD_TAG != 0 && CARD_IDS.lock().unwrap().contains_key(&id)
}

fn key_of(id: i64) -> Option<String> {
    CARD_IDS.lock().unwrap().get(&id).cloned()
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .http_status_as_error(false)
        .build()
        .into()
}

fn quick_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(4)))
        .http_status_as_error(false)
        .build()
        .into()
}

fn read(mut response: ureq::http::Response<ureq::Body>) -> Option<Value> {
    if !(200..300).contains(&response.status().as_u16()) {
        return None;
    }
    let text = response
        .body_mut()
        .with_config()
        .limit(16 * 1024 * 1024)
        .read_to_string()
        .ok()?;
    serde_json::from_str(&text).ok()
}

fn request<B>(
    builder: ureq::RequestBuilder<B>,
    client_id: &str,
    token: &str,
) -> ureq::RequestBuilder<B> {
    let builder = builder
        .header("Accept", "application/json")
        .header("X-Plex-Product", "Luma")
        .header("X-Plex-Version", env!("CARGO_PKG_VERSION"))
        .header("X-Plex-Client-Identifier", client_id)
        .header("X-Plex-Platform", "webOS")
        .header("X-Plex-Device", "LG TV")
        .header("X-Plex-Device-Name", "Luma");
    if token.is_empty() {
        builder
    } else {
        builder.header("X-Plex-Token", token)
    }
}

fn get(agent: &ureq::Agent, link: &PlexLink, path: &str) -> Option<Value> {
    let response = request(
        agent.get(&format!("{}{path}", link.server_uri)),
        &link.client_id,
        &link.server_token,
    )
    .call()
    .ok()?;
    read(response)
}

fn text(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn metadata(value: Option<Value>) -> Vec<Value> {
    value
        .as_ref()
        .and_then(|v| v.get("MediaContainer"))
        .and_then(|c| c.get("Metadata").or_else(|| c.get("Directory")))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The QR opens Plex's own sign-in with this PIN filled in.
fn auth_url(client_id: &str, code: &str) -> String {
    format!(
        "https://app.plex.tv/auth#?clientID={client_id}&code={code}\
         &context%5Bdevice%5D%5Bproduct%5D=Luma"
    )
}

fn link(reply: &mpsc::Sender<Response>, client_id: String, attempt: u64) {
    let agent = agent();
    let pin = request(
        agent.post(&format!("{PLEX_TV}/api/v2/pins?strong=false")),
        &client_id,
        "",
    )
    .send_empty()
    .ok()
    .and_then(read);
    let Some(pin) = pin else {
        let _ = reply.send(Response::Failed(
            "plex.tv did not answer. Check the internet".into(),
        ));
        return;
    };
    let id = text(&pin, "id");
    let code = text(&pin, "code");
    if id.is_empty() || code.is_empty() {
        let _ = reply.send(Response::Failed("plex.tv gave no code".into()));
        return;
    }
    let _ = reply.send(Response::Code {
        qr: auth_url(&client_id, &code),
        code,
    });
    let deadline = Instant::now() + Duration::from_secs(890);
    let token = loop {
        thread::sleep(Duration::from_secs(2));
        if ATTEMPT.load(Ordering::SeqCst) != attempt {
            return;
        }
        if Instant::now() > deadline {
            let _ = reply.send(Response::Failed("The code expired. Get a new one".into()));
            return;
        }
        let state = request(
            agent.get(&format!("{PLEX_TV}/api/v2/pins/{id}")),
            &client_id,
            "",
        )
        .call()
        .ok()
        .and_then(read);
        let token = state
            .as_ref()
            .map(|s| text(s, "authToken"))
            .unwrap_or_default();
        if !token.is_empty() {
            break token;
        }
    };
    let user = request(
        agent.get(&format!("{PLEX_TV}/api/v2/user")),
        &client_id,
        &token,
    )
    .call()
    .ok()
    .and_then(read)
    .map(|u| {
        let name = text(&u, "friendlyName");
        if name.is_empty() {
            text(&u, "username")
        } else {
            name
        }
    })
    .unwrap_or_default();
    match pick_server(&agent, &client_id, &token) {
        Some(mut link) => {
            if ATTEMPT.load(Ordering::SeqCst) != attempt {
                return;
            }
            link.user_name = user;
            let _ = reply.send(Response::Linked(link));
        }
        None => {
            let _ = reply.send(Response::Failed(
                "Signed in, but no Plex Media Server answered. Is it on?".into(),
            ));
        }
    }
}

/// The first server (owned ones first) with a connection that answers:
/// local addresses, then remote ones, relays last.
fn pick_server(agent: &ureq::Agent, client_id: &str, token: &str) -> Option<PlexLink> {
    let resources = request(
        agent.get(&format!(
            "{PLEX_TV}/api/v2/resources?includeHttps=1&includeRelay=1"
        )),
        client_id,
        token,
    )
    .call()
    .ok()
    .and_then(read)?;
    let mut servers: Vec<&Value> = resources
        .as_array()?
        .iter()
        .filter(|r| text(r, "provides").split(',').any(|p| p == "server"))
        .collect();
    servers.sort_by_key(|r| r.get("owned").and_then(Value::as_bool) != Some(true));
    let quick = quick_agent();
    for server in servers {
        let server_token = text(server, "accessToken");
        let mut connections: Vec<&Value> = server
            .get("connections")
            .and_then(Value::as_array)
            .map(|c| c.iter().collect())
            .unwrap_or_default();
        connections.sort_by_key(|c| {
            let flag = |k: &str| c.get(k).and_then(Value::as_bool) == Some(true);
            (flag("relay"), !flag("local"))
        });
        for connection in connections {
            let uri = text(connection, "uri");
            let answered = request(
                quick.get(&format!("{uri}/identity")),
                client_id,
                &server_token,
            )
            .call()
            .is_ok_and(|r| r.status().is_success());
            if answered {
                return Some(PlexLink {
                    client_id: client_id.to_owned(),
                    account_token: token.to_owned(),
                    user_name: String::new(),
                    server_name: text(server, "name"),
                    server_id: text(server, "clientIdentifier"),
                    server_uri: uri,
                    server_token,
                });
            }
        }
    }
    None
}

/// Up to `limit` items of one row.
fn collection_items(
    agent: &ureq::Agent,
    link: &PlexLink,
    collection: &str,
    limit: usize,
) -> Vec<Value> {
    let size = format!("X-Plex-Container-Start=0&X-Plex-Container-Size={limit}");
    let path = match collection {
        "ondeck" => format!("/library/onDeck?{size}"),
        "recent" => format!("/library/recentlyAdded?{size}"),
        section => {
            let Some(key) = section.strip_prefix("section:") else {
                return vec![];
            };
            format!("/library/sections/{key}/all?sort=addedAt:desc&{size}")
        }
    };
    metadata(get(agent, link, &path))
}

/// (rating key, title, poster path) the card shows: episodes and seasons
/// open their show.
fn card_target(item: &Value) -> (String, String, String) {
    match text(item, "type").as_str() {
        "episode" => (
            text(item, "grandparentRatingKey"),
            text(item, "grandparentTitle"),
            text(item, "grandparentThumb"),
        ),
        "season" => (
            text(item, "parentRatingKey"),
            text(item, "parentTitle"),
            text(item, "parentThumb"),
        ),
        _ => (
            text(item, "ratingKey"),
            text(item, "title"),
            text(item, "thumb"),
        ),
    }
}

/// Server-side resize of a library image. Carries the token: never logged.
fn image_url(link: &PlexLink, path: &str, width: u32, height: u32) -> String {
    let path: String = url::form_urlencoded::byte_serialize(path.as_bytes()).collect();
    format!(
        "{}/photo/:/transcode?width={width}&height={height}&minSize=1&upscale=1&url={path}&X-Plex-Token={}",
        link.server_uri, link.server_token
    )
}

fn fetch(
    reply: &mpsc::Sender<Response>,
    cache: &Path,
    link: &PlexLink,
    stacks: Vec<(usize, String)>,
) {
    let agent = agent();
    for (index, collection) in stacks {
        let mut seen = vec![];
        let mut targets = vec![];
        // Episodes fold into their show: ask for more, keep 20 cards.
        let max = crate::stacks::HOME_ROW_MAX;
        for item in collection_items(&agent, link, &collection, max * 2) {
            let target = card_target(&item);
            if target.0.is_empty() || seen.contains(&target.0) || targets.len() >= max {
                continue;
            }
            seen.push(target.0.clone());
            targets.push(target);
        }
        let mut cards: Vec<StackItem> = targets
            .iter()
            .map(|(key, title, _)| {
                let id = card_id(key);
                let cached = cache.join(format!("plex-{id}.jpg"));
                StackItem {
                    title: title.clone(),
                    poster: cached
                        .exists()
                        .then(|| cached.to_string_lossy().into_owned()),
                    id,
                }
            })
            .collect();
        let _ = reply.send(Response::Items(index, cards.clone()));
        if cards.iter().all(|c| c.poster.is_some()) {
            continue;
        }
        for ((_, _, thumb), card) in targets.iter().zip(cards.iter_mut()) {
            if card.poster.is_none() && !thumb.is_empty() {
                card.poster = crate::media::download_as(
                    &agent,
                    cache,
                    &format!("plex-{}", card.id),
                    &image_url(link, thumb, 270, 405),
                );
            }
        }
        let _ = reply.send(Response::Items(index, cards));
    }
}

/// Cards per Library page: eight rows of six.
const LIBRARY_PAGE: usize = 48;

/// One page of a row's whole list for the Library (View Library).
pub fn library_page(
    link: &PlexLink,
    collection: &str,
    query: &str,
    page: u32,
) -> Option<crate::media::LibraryPage> {
    let agent = agent();
    let start = (page.max(1) as usize - 1) * LIBRARY_PAGE;
    let size = format!("X-Plex-Container-Start={start}&X-Plex-Container-Size={LIBRARY_PAGE}");
    let q: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    let section = collection.strip_prefix("section:");
    let path = match (collection, section) {
        (_, Some(key)) if !query.is_empty() => {
            format!("/library/sections/{key}/all?title={q}&{size}")
        }
        _ if !query.is_empty() => format!("/search?query={q}&{size}"),
        ("ondeck", _) => format!("/library/onDeck?{size}"),
        ("recent", _) => format!("/library/recentlyAdded?{size}"),
        (_, Some(key)) => format!("/library/sections/{key}/all?sort=addedAt:desc&{size}"),
        _ => return None,
    };
    let body = get(&agent, link, &path)?;
    let container = body.get("MediaContainer");
    let total = container
        .and_then(|c| c.get("totalSize").or_else(|| c.get("size")))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let mut items: Vec<(i64, String, String)> = vec![];
    for item in metadata(Some(body)) {
        let (key, title, thumb) = card_target(&item);
        let id = card_id(&key);
        if key.is_empty() || items.iter().any(|i| i.0 == id) {
            continue;
        }
        let poster = if thumb.is_empty() {
            String::new()
        } else {
            image_url(link, &thumb, 270, 405)
        };
        items.push((id, title, poster));
    }
    Some(crate::media::LibraryPage {
        items,
        pages: total.div_ceil(LIBRARY_PAGE) as u32,
        total,
    })
}

/// Movie and show libraries: ("section:<key>", title).
fn views(agent: &ureq::Agent, link: &PlexLink) -> Vec<(String, String)> {
    metadata(get(agent, link, "/library/sections"))
        .iter()
        .filter(|s| matches!(text(s, "type").as_str(), "movie" | "show"))
        .map(|s| (format!("section:{}", text(s, "key")), text(s, "title")))
        .collect()
}

fn tags(item: &Value, key: &str, max: usize) -> String {
    item.get(key)
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter_map(|t| t.get("tag").and_then(Value::as_str))
                .take(max)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn media(item: &Value) -> Option<&Value> {
    item.get("Media")
        .and_then(Value::as_array)
        .and_then(|m| m.first())
}

fn part(item: &Value) -> Option<&Value> {
    media(item)?
        .get("Part")
        .and_then(Value::as_array)
        .and_then(|p| p.first())
}

/// Streams of the first part: 1 video, 2 audio, 3 subtitles.
fn streams(item: &Value, kind: i64) -> Vec<&Value> {
    part(item)
        .and_then(|p| p.get("Stream"))
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter(|s| s.get("streamType").and_then(Value::as_i64) == Some(kind))
                .collect()
        })
        .unwrap_or_default()
}

fn stream_label(stream: &Value) -> String {
    let label = text(stream, "extendedDisplayTitle");
    if label.is_empty() {
        text(stream, "displayTitle")
    } else {
        label
    }
}

fn badges(item: &Value) -> Vec<String> {
    let mut out = vec![];
    if let Some(m) = media(item) {
        if text(m, "videoResolution") == "4k" {
            out.push("4K".to_owned());
        }
        if m.get("audioChannels").and_then(Value::as_i64).unwrap_or(0) >= 6 {
            out.push("5.1".to_owned());
        }
    }
    if streams(item, 1)
        .iter()
        .any(|s| s.get("DOVIPresent").and_then(Value::as_bool) == Some(true))
    {
        out.push("Dolby Vision".to_owned());
    } else if streams(item, 1)
        .iter()
        .any(|s| matches!(text(s, "colorTrc").as_str(), "smpte2084" | "arib-std-b67"))
    {
        out.push("HDR".to_owned());
    }
    out
}

fn episodes(agent: &ureq::Agent, link: &PlexLink, show: &str) -> Vec<Value> {
    metadata(get(
        agent,
        link,
        &format!("/library/metadata/{show}/allLeaves"),
    ))
    .into_iter()
    .filter(|e| e.get("index").is_some())
    .collect()
}

fn season_number(e: &Value) -> (i64, i64) {
    (
        e.get("parentIndex").and_then(Value::as_i64).unwrap_or(1),
        e.get("index").and_then(Value::as_i64).unwrap_or(0),
    )
}

/// (watched, share watched).
fn played(e: &Value) -> (bool, f32) {
    let watched = e.get("viewCount").and_then(Value::as_i64).unwrap_or(0) > 0;
    let offset = e.get("viewOffset").and_then(Value::as_f64).unwrap_or(0.);
    let duration = e.get("duration").and_then(Value::as_f64).unwrap_or(0.);
    let share = if duration > 0. {
        (offset / duration) as f32
    } else {
        0.
    };
    (
        watched && offset <= 0.,
        if watched && offset <= 0. { 1. } else { share },
    )
}

fn minutes(item: &Value) -> i64 {
    item.get("duration").and_then(Value::as_i64).unwrap_or(0) / 60_000
}

fn details(reply: &mpsc::Sender<Response>, cache: &Path, link: &PlexLink, id: i64) {
    let agent = agent();
    let Some(key) = key_of(id) else {
        return;
    };
    let Some(item) = metadata(get(&agent, link, &format!("/library/metadata/{key}")))
        .into_iter()
        .next()
    else {
        let _ = reply.send(Response::Failed(
            "This title is not on the server any more".into(),
        ));
        return;
    };
    let serial = text(&item, "type") == "show";
    let mut detail = Detail {
        id,
        serial,
        title: text(&item, "title"),
        year: item.get("year").and_then(Value::as_i64).unwrap_or(0),
        minutes: minutes(&item),
        genres: tags(&item, "Genre", 3),
        countries: tags(&item, "Country", 2),
        plot: text(&item, "summary"),
        director: tags(&item, "Director", 2),
        cast: tags(&item, "Role", 5),
        tags: badges(&item),
        audio: streams(&item, 2).iter().map(|s| stream_label(s)).collect(),
        subtitles: streams(&item, 3)
            .iter()
            .filter(|s| !text(s, "key").is_empty())
            .map(|s| (stream_label(s), String::new()))
            .collect(),
        ..Default::default()
    };
    if serial {
        for e in episodes(&agent, link, &key) {
            let (season, number) = season_number(&e);
            let (watched, progress) = played(&e);
            if !detail.seasons.contains(&season) {
                detail.seasons.push(season);
            }
            let thumb = text(&e, "thumb");
            detail.episodes.push(Episode {
                season,
                number,
                title: text(&e, "title"),
                minutes: minutes(&e),
                progress,
                watched,
                thumb_url: if thumb.is_empty() {
                    String::new()
                } else {
                    image_url(link, &thumb, 480, 270)
                },
                thumb: None,
            });
        }
        detail.next = detail.episodes.iter().position(|e| !e.watched);
    }
    let _ = reply.send(Response::Details(Box::new(detail.clone())));
    let name = format!("plex-{id}");
    let thumb = text(&item, "thumb");
    if !thumb.is_empty() {
        detail.poster =
            crate::media::download_as(&agent, cache, &name, &image_url(link, &thumb, 270, 405));
    }
    let art = text(&item, "art");
    detail.backdrop = if art.is_empty() {
        None
    } else {
        crate::media::download_as(
            &agent,
            cache,
            &format!("{name}-b"),
            &image_url(link, &art, 1920, 1080),
        )
    }
    .or_else(|| {
        detail
            .poster
            .as_deref()
            .and_then(|p| crate::media::poster_backdrop(cache, &format!("{name}-pb"), p))
    });
    crate::media::season_stills(&agent, cache, &mut detail);
    let _ = reply.send(Response::Details(Box::new(detail)));
}

/// HLS from Plex's universal transcoder; it copies streams the profile allows
/// (direct stream) and transcodes only the rest. Carries the token.
fn hls_url(link: &PlexLink, key: &str, session: &str) -> String {
    let path: String =
        url::form_urlencoded::byte_serialize(format!("/library/metadata/{key}").as_bytes())
            .collect();
    format!(
        "{}/video/:/transcode/universal/start.m3u8?path={path}&mediaIndex=0&partIndex=0\
         &protocol=hls&fastSeek=1&directPlay=0&directStream=1&directStreamAudio=1\
         &videoQuality=100&maxVideoBitrate=120000&videoResolution=3840x2160\
         &session={session}&X-Plex-Session-Identifier={session}&X-Plex-Client-Identifier={}\
         &X-Plex-Product=Luma&X-Plex-Platform=Chrome&X-Plex-Token={}",
        link.server_uri, link.client_id, link.server_token
    )
}

fn play(reply: &mpsc::Sender<Response>, link: &PlexLink, request: PlayRequest) {
    let agent = agent();
    let Some(key) = key_of(request.id) else {
        return;
    };
    let fail = |why: &str| {
        let _ = reply.send(Response::Failed(why.to_owned()));
    };
    let Some(item) = metadata(get(&agent, link, &format!("/library/metadata/{key}")))
        .into_iter()
        .next()
    else {
        return fail("This title is not on the server any more");
    };
    let series = text(&item, "type") == "show";
    let list = if series {
        episodes(&agent, link, &key)
    } else {
        vec![]
    };
    let position = if series {
        let wanted = request.episode.or_else(|| {
            list.iter()
                .find(|e| e.get("viewOffset").and_then(Value::as_i64).unwrap_or(0) > 0)
                .or_else(|| list.iter().find(|e| !played(e).0))
                .or(list.first())
                .map(season_number)
        });
        let Some(i) = wanted.and_then(|w| list.iter().position(|e| season_number(e) == w)) else {
            return fail("No episode to play");
        };
        Some(i)
    } else {
        None
    };
    // allLeaves has no stream details: read the chosen episode itself.
    let target = match position {
        Some(i) => metadata(get(
            &agent,
            link,
            &format!("/library/metadata/{}", text(&list[i], "ratingKey")),
        ))
        .into_iter()
        .next()
        .unwrap_or_else(|| list[i].clone()),
        None => item.clone(),
    };
    let target_key = text(&target, "ratingKey");
    let Some(file) = part(&target)
        .map(|p| text(p, "key"))
        .filter(|k| !k.is_empty())
    else {
        return fail("The server has no playable file for this title");
    };
    let session = crate::phone_link::random_hex()[..24].to_owned();
    let container = media(&target)
        .map(|m| text(m, "container"))
        .unwrap_or_default();
    let url = if matches!(container.as_str(), "mp4" | "mov") {
        format!(
            "{}{file}?X-Plex-Token={}",
            link.server_uri, link.server_token
        )
    } else {
        hls_url(link, &target_key, &session)
    };
    let subtitles: Vec<crate::player::Track> = streams(&target, 3)
        .iter()
        .filter(|s| {
            !text(s, "key").is_empty()
                && matches!(text(s, "codec").as_str(), "srt" | "vtt" | "webvtt")
        })
        .map(|s| crate::player::Track {
            name: stream_label(s),
            url: Some(format!(
                "{}{}?X-Plex-Token={}",
                link.server_uri,
                text(s, "key"),
                link.server_token
            )),
        })
        .collect();
    let selected_subtitle = request
        .subtitle
        .as_ref()
        .and_then(|want| subtitles.iter().position(|t| &t.name == want));
    let neighbour = |i: Option<usize>| i.and_then(|i| list.get(i)).map(season_number);
    let line = |e: &Value| {
        let (season, number) = season_number(e);
        let name = text(e, "title");
        if name.is_empty() {
            format!("Season {season} · Episode {number}")
        } else {
            format!("Season {season} · Episode {number} · {name}")
        }
    };
    let title = text(&item, "title");
    let (title, episode_line) = match position {
        Some(i) => {
            let (season, number) = season_number(&list[i]);
            (
                format!("{title} · S{season}E{number}"),
                Some(line(&list[i])),
            )
        }
        None => (title, None),
    };
    let report = crate::player::Report {
        service: "plex".into(),
        base: link.server_uri.clone(),
        token: link.server_token.clone(),
        device_id: link.client_id.clone(),
        item: target_key.clone(),
        source: String::new(),
        session: session.clone(),
        episodes: list
            .iter()
            .map(|e| {
                let (season, number) = season_number(e);
                let key = text(e, "ratingKey");
                crate::player::ReportEpisode {
                    season,
                    number,
                    url: hls_url(link, &key, &session),
                    line: line(e),
                    item: key,
                }
            })
            .collect(),
    };
    let _ = reply.send(Response::Play(Box::new(crate::player::OpenRequest {
        url,
        title,
        source: if series {
            crate::player::Source::Series
        } else {
            crate::player::Source::Movie
        },
        resume_ms: target
            .get("viewOffset")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        duration_ms: target.get("duration").and_then(Value::as_u64).unwrap_or(0),
        poster: None,
        audio: streams(&target, 2)
            .iter()
            .map(|s| crate::player::Track {
                name: stream_label(s),
                url: None,
            })
            .collect(),
        subtitles,
        qualities: vec![],
        chapters: vec![],
        item_id: Some(request.id),
        episode_line,
        prev_episode: neighbour(position.and_then(|i| i.checked_sub(1))),
        next_episode: neighbour(position.map(|i| i + 1)),
        selected_subtitle,
        report: Some(report),
        music: None,
    })));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn episodes_and_seasons_open_their_show() {
        let episode = json!({"type": "episode", "ratingKey": "9", "grandparentRatingKey": "5",
            "grandparentTitle": "Show", "grandparentThumb": "/t/5"});
        assert_eq!(
            card_target(&episode),
            ("5".into(), "Show".into(), "/t/5".into())
        );
        let season = json!({"type": "season", "ratingKey": 7, "parentRatingKey": 5,
            "parentTitle": "Show", "parentThumb": "/t/5"});
        assert_eq!(
            card_target(&season),
            ("5".into(), "Show".into(), "/t/5".into())
        );
        let movie = json!({"type": "movie", "ratingKey": "3", "title": "Film", "thumb": "/t/3"});
        assert_eq!(
            card_target(&movie),
            ("3".into(), "Film".into(), "/t/3".into())
        );
    }

    #[test]
    fn plex_and_jellyfin_ids_never_meet() {
        let plex = card_id("1234");
        assert!(owns(plex) && !crate::jellyfin::owns(plex));
        assert_eq!(key_of(plex).as_deref(), Some("1234"));
    }

    #[test]
    fn progress_counts_partial_rewatch() {
        let done = json!({"viewCount": 1, "duration": 1000});
        assert_eq!(played(&done), (true, 1.));
        let half = json!({"viewOffset": 500, "duration": 1000});
        assert_eq!(played(&half), (false, 0.5));
    }

    #[test]
    fn qr_fills_the_code_in() {
        let url = auth_url("abc", "QDS3");
        assert!(url.starts_with("https://app.plex.tv/auth#?clientID=abc&code=QDS3"));
    }
}
