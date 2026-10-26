//! Jellyfin for Home stacks. Luma signs in with Quick Connect: the TV shows a
//! code, the user approves it on the phone. Nothing is typed with the remote
//! and no password is ever seen. The access token is never logged.
use crate::media::{Detail, Episode, PlayRequest};
use crate::stacks::StackItem;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

/// Saved sign-in. The server address is what the user picked or entered.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct JellyfinLink {
    pub server: String,
    pub server_name: String,
    pub user_id: String,
    pub user_name: String,
    pub token: String,
    /// Random per-install id; Jellyfin lists Luma as its own device.
    pub device_id: String,
}

impl JellyfinLink {
    pub fn linked(&self) -> bool {
        !self.token.is_empty() && !self.server.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Server {
    pub name: String,
    pub address: String,
    pub version: String,
}

/// Row contents: (id, label). Libraries follow as "view:<id>", music
/// libraries as "music:<id>" (square album cards).
pub const COLLECTIONS: [(&str, &str); 4] = [
    ("resume", "Continue watching"),
    ("nextup", "Next up"),
    ("latest", "Recently added"),
    ("albums", "Recently added albums"),
];

/// Rows of albums: shown with square cards.
pub fn music_collection(collection: &str) -> bool {
    collection == "albums" || collection.starts_with("music:")
}

pub enum Command {
    /// Look for servers on the local network; a saved address is checked too.
    Discover(Option<String>),
    /// Start Quick Connect on this server and wait until the phone approves.
    Connect {
        address: String,
        device_id: String,
    },
    /// Stop waiting for a Quick Connect approval.
    Cancel,
    /// Cards for (stack index, collection id).
    Fetch(JellyfinLink, Vec<(usize, String)>),
    /// The user's libraries, offered as row contents.
    Views(JellyfinLink),
    Details(JellyfinLink, i64),
    Play(JellyfinLink, PlayRequest),
    /// Songs and cover of an album card.
    Album(JellyfinLink, i64),
    /// Play an album card from this song, in order or shuffled.
    PlayAlbum(JellyfinLink, i64, usize, bool),
}

pub enum Response {
    Servers(Vec<Server>),
    Code { code: String, address: String },
    Linked(JellyfinLink),
    Failed(String),
    Items(usize, Vec<StackItem>),
    Views(Vec<(String, String)>),
    Details(Box<Detail>),
    Play(Box<crate::player::OpenRequest>),
    Album(Box<crate::state::album::Album>),
}

pub struct Service {
    sender: Mutex<mpsc::Sender<Command>>,
    pub receiver: Mutex<mpsc::Receiver<Response>>,
}

/// Bumped by every Connect and Cancel; a Quick Connect wait stops when it moves.
static ATTEMPT: AtomicU64 = AtomicU64::new(0);

impl Service {
    pub fn new(cache: PathBuf) -> Self {
        let (sender, commands) = mpsc::channel::<Command>();
        let (reply, receiver) = mpsc::channel();
        thread::spawn(move || {
            let _ = std::fs::create_dir_all(&cache);
            for command in commands {
                match command {
                    Command::Discover(saved) => discover(&reply, saved),
                    Command::Connect { address, device_id } => {
                        let attempt = ATTEMPT.fetch_add(1, Ordering::SeqCst) + 1;
                        let reply = reply.clone();
                        thread::spawn(move || connect(&reply, address, device_id, attempt));
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
                    Command::Album(link, id) => album(&reply, &cache, &link, id),
                    Command::PlayAlbum(link, id, start, shuffle) => {
                        play_album(&reply, &link, id, start, shuffle)
                    }
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

/// Jellyfin ids are GUID strings; the title card and the player key items by
/// i64. Card ids carry this bit, far above any media source id, and map back here.
const CARD_TAG: i64 = 1 << 52;
static CARD_IDS: Mutex<BTreeMap<i64, String>> = Mutex::new(BTreeMap::new());
/// Cards that are albums: OK opens the album card, not the title card.
static ALBUM_IDS: Mutex<std::collections::BTreeSet<i64>> =
    Mutex::new(std::collections::BTreeSet::new());

pub fn is_album(id: i64) -> bool {
    ALBUM_IDS.lock().unwrap().contains(&id)
}

pub fn card_id(item: &str) -> i64 {
    // FNV-1a: stable across runs, so cached posters keep their file names.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in item.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    let id = (hash as i64 & (CARD_TAG - 1)) | CARD_TAG;
    CARD_IDS.lock().unwrap().insert(id, item.to_owned());
    id
}

/// Whether a title card or Play request belongs to Jellyfin.
pub fn owns(id: i64) -> bool {
    id & CARD_TAG != 0 && CARD_IDS.lock().unwrap().contains_key(&id)
}

fn item_of(id: i64) -> Option<String> {
    CARD_IDS.lock().unwrap().get(&id).cloned()
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .http_status_as_error(false)
        .build()
        .into()
}

fn auth(device_id: &str, token: &str) -> String {
    let mut header = format!(
        "MediaBrowser Client=\"Luma\", Device=\"LG TV\", DeviceId=\"{device_id}\", Version=\"{}\"",
        env!("CARGO_PKG_VERSION")
    );
    if !token.is_empty() {
        header.push_str(&format!(", Token=\"{token}\""));
    }
    header
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

fn get(agent: &ureq::Agent, link: &JellyfinLink, path: &str) -> Option<Value> {
    let response = agent
        .get(&format!("{}{path}", link.server))
        .header("Authorization", &auth(&link.device_id, &link.token))
        .call()
        .ok()?;
    read(response)
}

/// "192.168.1.5" → "http://192.168.1.5:8096"; keeps a scheme, port and path given.
pub fn normalize(address: &str) -> String {
    let address = address.trim().trim_end_matches('/');
    if address.is_empty() {
        return String::new();
    }
    if address.contains("://") {
        return address.to_owned();
    }
    let host = address.split('/').next().unwrap_or(address);
    if host.contains(':') {
        format!("http://{address}")
    } else {
        format!("http://{host}:8096{}", &address[host.len()..])
    }
}

fn public_info(agent: &ureq::Agent, address: &str) -> Option<Server> {
    let info = read(
        agent
            .get(&format!("{address}/System/Info/Public"))
            .call()
            .ok()?,
    )?;
    info.get("Id").and_then(Value::as_str)?;
    Some(Server {
        name: text(&info, "ServerName"),
        address: address.to_owned(),
        version: text(&info, "Version"),
    })
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

/// Jellyfin answers "who is JellyfinServer?" on UDP 7359 with its address.
/// A server in a container (Umbrel, Docker) names its internal address, so
/// the host that answered is tried with the same port. When nothing answers,
/// the local /24 is checked for port 8096.
fn discover(reply: &mpsc::Sender<Response>, saved: Option<String>) {
    let agent = agent();
    let mut found: Vec<Server> = vec![];
    if let Some(server) = saved.as_deref().and_then(|a| public_info(&agent, a)) {
        found.push(server);
        let _ = reply.send(Response::Servers(found.clone()));
    }
    let local = local_ipv4();
    let mut answers: Vec<(String, std::net::IpAddr)> = vec![];
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        let _ = socket.set_broadcast(true);
        let _ = socket.set_read_timeout(Some(Duration::from_millis(400)));
        let mut targets = vec!["255.255.255.255:7359".to_owned()];
        if let Some(ip) = local {
            let o = ip.octets();
            targets.push(format!("{}.{}.{}.255:7359", o[0], o[1], o[2]));
        }
        let deadline = Instant::now() + Duration::from_millis(2500);
        let mut sent = 0;
        let mut buffer = [0u8; 2048];
        while Instant::now() < deadline {
            if sent < 3 {
                for target in &targets {
                    let _ = socket.send_to(b"who is JellyfinServer?", target.as_str());
                }
                sent += 1;
            }
            if let Ok((n, from)) = socket.recv_from(&mut buffer)
                && let Ok(answer) = serde_json::from_slice::<Value>(&buffer[..n])
            {
                let address = text(&answer, "Address");
                if !address.is_empty() && !answers.iter().any(|(a, _)| *a == address) {
                    answers.push((address, from.ip()));
                }
            }
        }
    }
    for (address, from) in answers {
        let address = normalize(&address);
        for candidate in [address.clone(), with_host(&address, &from.to_string())] {
            if found.iter().any(|s| s.address == candidate) {
                break;
            }
            if reachable(&candidate)
                && let Some(server) = public_info(&agent, &candidate)
            {
                found.push(server);
                break;
            }
        }
    }
    if found.is_empty()
        && let Some(ip) = local
    {
        for address in scan(ip) {
            if let Some(server) = public_info(&agent, &address) {
                found.push(server);
            }
        }
    }
    let _ = reply.send(Response::Servers(found));
}

/// The TV's own LAN address (no packet is sent by a UDP connect).
fn local_ipv4() -> Option<std::net::Ipv4Addr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

/// Same scheme and port, another host: "http://10.21.0.15:8096" → "http://192.168.1.68:8096".
fn with_host(address: &str, host: &str) -> String {
    let (scheme, rest) = address.split_once("://").unwrap_or(("http", address));
    let authority = rest.split('/').next().unwrap_or(rest);
    let port = authority.rsplit_once(':').map(|(_, p)| p).unwrap_or("8096");
    format!("{scheme}://{host}:{port}")
}

fn socket_addr(address: &str) -> Option<std::net::SocketAddr> {
    use std::net::ToSocketAddrs;
    let (scheme, rest) = address.split_once("://")?;
    let authority = rest.split('/').next()?;
    let target = if authority.contains(':') {
        authority.to_owned()
    } else {
        format!("{authority}:{}", if scheme == "https" { 443 } else { 80 })
    };
    target.to_socket_addrs().ok()?.next()
}

/// Quick TCP check so an unreachable container address costs 600 ms, not a timeout.
fn reachable(address: &str) -> bool {
    socket_addr(address).is_some_and(|a| {
        std::net::TcpStream::connect_timeout(&a, Duration::from_millis(600)).is_ok()
    })
}

/// Hosts on the local /24 with port 8096 open.
fn scan(local: std::net::Ipv4Addr) -> Vec<String> {
    let o = local.octets();
    let (tx, rx) = mpsc::channel();
    for chunk in (1u8..=254).collect::<Vec<_>>().chunks(64) {
        let handles: Vec<_> = chunk
            .iter()
            .filter(|&&last| last != o[3])
            .map(|&last| {
                let tx = tx.clone();
                let addr = std::net::SocketAddr::from(([o[0], o[1], o[2], last], 8096));
                thread::spawn(move || {
                    if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(400))
                        .is_ok()
                    {
                        let _ = tx.send(format!("http://{}:8096", addr.ip()));
                    }
                })
            })
            .collect();
        for handle in handles {
            let _ = handle.join();
        }
    }
    drop(tx);
    rx.into_iter().collect()
}

/// Phone form check: the address answers as a Jellyfin server.
pub fn check(address: &str) -> Result<Server, String> {
    let address = normalize(address);
    if address.is_empty() {
        return Err("Type the server address".into());
    }
    public_info(&agent(), &address)
        .ok_or_else(|| format!("No Jellyfin server answered at {address}"))
}

/// Quick Connect: show the code, poll every 3 s for 10 minutes, then trade
/// the approved secret for an access token.
fn connect(reply: &mpsc::Sender<Response>, address: String, device_id: String, attempt: u64) {
    let agent = agent();
    let header = auth(&device_id, "");
    let initiate = agent
        .post(&format!("{address}/QuickConnect/Initiate"))
        .header("Authorization", &header)
        .send_empty()
        .ok()
        .and_then(read);
    let Some(initiate) = initiate else {
        let _ = reply.send(Response::Failed(
            "Quick Connect is off. Turn it on in Jellyfin: Dashboard › General".into(),
        ));
        return;
    };
    let secret = text(&initiate, "Secret");
    let code = text(&initiate, "Code");
    if secret.is_empty() || code.is_empty() {
        let _ = reply.send(Response::Failed("The server gave no code".into()));
        return;
    }
    let _ = reply.send(Response::Code {
        code,
        address: address.clone(),
    });
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        thread::sleep(Duration::from_secs(3));
        if ATTEMPT.load(Ordering::SeqCst) != attempt {
            return;
        }
        if Instant::now() > deadline {
            let _ = reply.send(Response::Failed("The code expired. Get a new one".into()));
            return;
        }
        let state = agent
            .get(&format!("{address}/QuickConnect/Connect?secret={secret}"))
            .header("Authorization", &header)
            .call()
            .ok()
            .and_then(read);
        if state
            .as_ref()
            .and_then(|s| s.get("Authenticated"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            break;
        }
    }
    let session = agent
        .post(&format!("{address}/Users/AuthenticateWithQuickConnect"))
        .header("Authorization", &header)
        .header("Content-Type", "application/json")
        .send(json!({ "Secret": secret }).to_string())
        .ok()
        .and_then(read);
    let Some(session) = session else {
        let _ = reply.send(Response::Failed(
            "Sign-in was approved but failed. Try again".into(),
        ));
        return;
    };
    let user = session.get("User").cloned().unwrap_or_default();
    let server_name = public_info(&agent, &address)
        .map(|s| s.name)
        .unwrap_or_default();
    if ATTEMPT.load(Ordering::SeqCst) != attempt {
        return;
    }
    let _ = reply.send(Response::Linked(JellyfinLink {
        server: address,
        server_name,
        user_id: text(&user, "Id"),
        user_name: text(&user, "Name"),
        token: text(&session, "AccessToken"),
        device_id,
    }));
}

const FIELDS: &str = "Fields=PrimaryImageAspectRatio&EnableImageTypes=Primary,Backdrop,Thumb";

fn list(value: Option<Value>) -> Vec<Value> {
    match value {
        Some(Value::Array(items)) => items,
        Some(v) => v
            .get("Items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        None => vec![],
    }
}

/// Up to `limit` items of one row. Episodes stand for their series.
fn collection_items(
    agent: &ureq::Agent,
    link: &JellyfinLink,
    collection: &str,
    limit: usize,
) -> Vec<Value> {
    let user = &link.user_id;
    let path = match collection {
        "resume" => {
            format!("/UserItems/Resume?userId={user}&MediaTypes=Video&Limit={limit}&{FIELDS}")
        }
        "nextup" => format!("/Shows/NextUp?userId={user}&Limit={limit}&{FIELDS}"),
        "latest" => format!(
            "/Items/Latest?userId={user}&IncludeItemTypes=Movie,Series&Limit={limit}&{FIELDS}"
        ),
        "albums" => format!(
            "/Items/Latest?userId={user}&IncludeItemTypes=MusicAlbum&Limit={limit}&{FIELDS}"
        ),
        view if view.starts_with("music:") => format!(
            "/Items?userId={user}&ParentId={}&Recursive=true&IncludeItemTypes=MusicAlbum\
             &SortBy=DateCreated&SortOrder=Descending&Limit={limit}&{FIELDS}",
            &view[6..]
        ),
        view => {
            let Some(parent) = view.strip_prefix("view:") else {
                return vec![];
            };
            format!(
                "/Items?userId={user}&ParentId={parent}&Recursive=true&IncludeItemTypes=Movie,Series\
                 &SortBy=DateCreated&SortOrder=Descending&Limit={limit}&{FIELDS}"
            )
        }
    };
    list(get(agent, link, &path))
}

/// (Jellyfin id, title) the card shows: an episode opens its series.
fn card_target(item: &Value) -> (String, String) {
    if text(item, "Type") == "MusicAlbum" {
        let id = text(item, "Id");
        ALBUM_IDS.lock().unwrap().insert(card_id(&id));
        return (id, text(item, "Name"));
    }
    if text(item, "Type") == "Episode" {
        let series = text(item, "SeriesId");
        if !series.is_empty() {
            return (series, text(item, "SeriesName"));
        }
    }
    (text(item, "Id"), text(item, "Name"))
}

/// Image endpoints need no token.
fn image_url(link: &JellyfinLink, item: &str, kind: &str, size: &str) -> String {
    format!(
        "{}/Items/{item}/Images/{kind}?{size}&quality=90",
        link.server
    )
}

/// Poster with fallbacks: some servers list a Primary tag whose file is
/// gone (404), then the Thumb or the first Backdrop is cropped instead.
fn poster(
    agent: &ureq::Agent,
    cache: &Path,
    link: &JellyfinLink,
    item: &str,
    name: &str,
) -> Option<String> {
    // The renderer crops any shape to the card, so a wide still is never
    // stretched.
    [
        ("Primary", "fillHeight=405"),
        ("Thumb", "fillHeight=405"),
        ("Backdrop/0", "fillHeight=405"),
    ]
    .iter()
    .find_map(|(kind, size)| {
        crate::media::download_as(agent, cache, name, &image_url(link, item, kind, size))
    })
}

fn fetch(
    reply: &mpsc::Sender<Response>,
    cache: &Path,
    link: &JellyfinLink,
    stacks: Vec<(usize, String)>,
) {
    let agent = agent();
    for (index, collection) in stacks {
        let mut seen = vec![];
        let mut targets = vec![];
        // Episodes fold into their series: ask for more, keep 20 cards.
        let max = crate::stacks::HOME_ROW_MAX;
        for item in collection_items(&agent, link, &collection, max * 2) {
            let (item, title) = card_target(&item);
            if item.is_empty() || seen.contains(&item) || targets.len() >= max {
                continue;
            }
            seen.push(item.clone());
            targets.push((item, title));
        }
        // Cached posters show at once; the rest arrive in one more update.
        let mut cards: Vec<StackItem> = targets
            .iter()
            .map(|(item, title)| {
                let id = card_id(item);
                let cached = cache.join(format!("jf-{id}.jpg"));
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
        for ((item, _), card) in targets.iter().zip(cards.iter_mut()) {
            if card.poster.is_none() {
                let name = format!("jf-{}", card.id);
                card.poster = if is_album(card.id) {
                    album_cover(&agent, link, item, "fillHeight=405")
                        .and_then(|url| crate::media::download_as(&agent, cache, &name, &url))
                } else {
                    poster(&agent, cache, link, item, &name)
                };
            }
        }
        let _ = reply.send(Response::Items(index, cards));
    }
}

/// Cards per Library page: eight rows of six.
const LIBRARY_PAGE: usize = 48;

/// One page of a row's whole list for the Library (View Library), newest
/// first; a search looks through every library.
pub fn library_page(
    link: &JellyfinLink,
    collection: &str,
    query: &str,
    page: u32,
) -> Option<crate::media::LibraryPage> {
    let agent = agent();
    let user = &link.user_id;
    let start = (page.max(1) as usize - 1) * LIBRARY_PAGE;
    let paged = format!("StartIndex={start}&Limit={LIBRARY_PAGE}&{FIELDS}");
    let search = if query.is_empty() {
        String::new()
    } else {
        let q: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
        format!("&searchTerm={q}")
    };
    let path = match collection {
        "resume" if query.is_empty() => {
            format!("/UserItems/Resume?userId={user}&MediaTypes=Video&{paged}")
        }
        "nextup" if query.is_empty() => format!("/Shows/NextUp?userId={user}&{paged}"),
        c => {
            let (kind, parent) = if music_collection(c) {
                ("MusicAlbum", c.strip_prefix("music:"))
            } else {
                ("Movie,Series", c.strip_prefix("view:"))
            };
            let parent = parent.map(|p| format!("&ParentId={p}")).unwrap_or_default();
            format!(
                "/Items?userId={user}{parent}&Recursive=true&IncludeItemTypes={kind}\
                 &SortBy=DateCreated,SortName&SortOrder=Descending{search}&{paged}"
            )
        }
    };
    let body = get(&agent, link, &path)?;
    let total = body
        .get("TotalRecordCount")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let mut items: Vec<(i64, String, String)> = vec![];
    for item in list(Some(body)) {
        let (item, title) = card_target(&item);
        let id = card_id(&item);
        if item.is_empty() || items.iter().any(|i| i.0 == id) {
            continue;
        }
        items.push((
            id,
            title,
            image_url(link, &item, "Primary", "fillHeight=405"),
        ));
    }
    Some(crate::media::LibraryPage {
        items,
        pages: total.div_ceil(LIBRARY_PAGE) as u32,
        total,
    })
}

/// Movie and show libraries ("view:<id>", name), music ones ("music:<id>", name).
fn views(agent: &ureq::Agent, link: &JellyfinLink) -> Vec<(String, String)> {
    list(get(
        agent,
        link,
        &format!("/UserViews?userId={}", link.user_id),
    ))
    .iter()
    .filter(|v| text(v, "Type") == "CollectionFolder")
    .filter_map(|v| {
        let kind = match v.get("CollectionType").and_then(Value::as_str) {
            Some("movies" | "tvshows" | "homevideos" | "mixed") | None => "view",
            Some("music") => "music",
            _ => return None,
        };
        Some((format!("{kind}:{}", text(v, "Id")), text(v, "Name")))
    })
    .collect()
}

/// Songs of an album in disc and track order.
fn songs(agent: &ureq::Agent, link: &JellyfinLink, album: &str) -> Vec<Value> {
    list(get(
        agent,
        link,
        &format!(
            "/Items?userId={}&ParentId={album}&IncludeItemTypes=Audio&Recursive=true\
             &SortBy=ParentIndexNumber,IndexNumber,SortName",
            link.user_id
        ),
    ))
}

/// Album art: the album's own image, else the first song's (embedded in the
/// file). Many libraries list an album image whose file is gone.
fn album_cover(
    agent: &ureq::Agent,
    link: &JellyfinLink,
    album: &str,
    size: &str,
) -> Option<String> {
    let works = |url: &str| agent.get(url).call().is_ok_and(|r| r.status().is_success());
    let own = image_url(link, album, "Primary", size);
    if works(&own) {
        return Some(own);
    }
    let first = songs(agent, link, album)
        .into_iter()
        .find(|s| s.get("ImageTags").and_then(|t| t.get("Primary")).is_some())?;
    let url = image_url(link, &text(&first, "Id"), "Primary", size);
    works(&url).then_some(url)
}

fn album_artist(item: &Value) -> String {
    let artist = text(item, "AlbumArtist");
    if artist.is_empty() {
        names(item, "Artists", 2)
    } else {
        artist
    }
}

fn album(reply: &mpsc::Sender<Response>, cache: &Path, link: &JellyfinLink, id: i64) {
    let agent = agent();
    let Some(guid) = item_of(id) else {
        return;
    };
    let Some(item) = get(
        &agent,
        link,
        &format!("/Items/{guid}?userId={}", link.user_id),
    ) else {
        let _ = reply.send(Response::Failed(
            "This album is not on the server any more".into(),
        ));
        return;
    };
    let mut detail = crate::state::album::Album {
        id,
        title: text(&item, "Name"),
        artist: album_artist(&item),
        year: item
            .get("ProductionYear")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        songs: songs(&agent, link, &guid)
            .iter()
            .map(|s| crate::state::album::Song {
                number: s.get("IndexNumber").and_then(Value::as_i64).unwrap_or(0),
                title: text(s, "Name"),
                artist: names(s, "Artists", 2),
                ms: ticks_ms(s.get("RunTimeTicks")),
            })
            .collect(),
        cover: None,
        backdrop: None,
    };
    let _ = reply.send(Response::Album(Box::new(detail.clone())));
    let name = format!("jf-{id}-cover");
    detail.cover = album_cover(&agent, link, &guid, "fillHeight=880")
        .and_then(|url| crate::media::download_as(&agent, cache, &name, &url));
    detail.backdrop = detail
        .cover
        .as_deref()
        .and_then(|c| crate::media::poster_backdrop(cache, &format!("{name}-pb"), c));
    let _ = reply.send(Response::Album(Box::new(detail)));
}

/// Audio as the TV plays it: MP3, AAC and FLAC as they are, anything else
/// converted to MP3 by the server. The URL carries the token: never logged.
fn audio_url(link: &JellyfinLink, item: &str, session: &str) -> String {
    format!(
        "{}/Audio/{item}/universal?UserId={}&DeviceId={}&ApiKey={}&PlaySessionId={session}\
         &Container=mp3,aac,m4a|aac,m4b|aac,flac,wav&TranscodingContainer=mp3\
         &TranscodingProtocol=http&AudioCodec=mp3&MaxStreamingBitrate=320000",
        link.server, link.user_id, link.device_id, link.token
    )
}

fn play_album(
    reply: &mpsc::Sender<Response>,
    link: &JellyfinLink,
    id: i64,
    start: usize,
    shuffle: bool,
) {
    let agent = agent();
    let Some(guid) = item_of(id) else {
        return;
    };
    let item = get(
        &agent,
        link,
        &format!("/Items/{guid}?userId={}", link.user_id),
    );
    let list = songs(&agent, link, &guid);
    if list.is_empty() {
        let _ = reply.send(Response::Failed("This album has no songs".into()));
        return;
    }
    let session = crate::phone_link::random_hex();
    let artist = item.as_ref().map(album_artist).unwrap_or_default();
    let songs: Vec<crate::player::MusicSong> = list
        .iter()
        .map(|s| {
            let item = text(s, "Id");
            crate::player::MusicSong {
                title: text(s, "Name"),
                artist: names(s, "Artists", 2),
                url: audio_url(link, &item, &session),
                item,
                ms: ticks_ms(s.get("RunTimeTicks")),
            }
        })
        .collect();
    let start = start.min(songs.len() - 1);
    let first = songs[start].clone();
    let title = item.as_ref().map(|i| text(i, "Name")).unwrap_or_default();
    let report = crate::player::Report {
        service: "jellyfin".into(),
        base: link.server.clone(),
        token: link.token.clone(),
        device_id: link.device_id.clone(),
        item: first.item.clone(),
        source: first.item.clone(),
        session,
        episodes: vec![],
    };
    let _ = reply.send(Response::Play(Box::new(crate::player::OpenRequest {
        url: first.url.clone(),
        title: title.clone(),
        source: crate::player::Source::Local,
        resume_ms: 0,
        duration_ms: first.ms,
        poster: None,
        audio: vec![],
        subtitles: vec![],
        qualities: vec![],
        chapters: vec![],
        item_id: Some(id),
        episode_line: None,
        prev_episode: None,
        next_episode: None,
        selected_subtitle: None,
        report: Some(report),
        music: Some(crate::player::Music {
            album: title,
            artist,
            year: item
                .as_ref()
                .and_then(|i| i.get("ProductionYear"))
                .and_then(Value::as_i64)
                .unwrap_or(0),
            cover: album_cover(&agent, link, &guid, "fillHeight=880").unwrap_or_default(),
            songs,
            start,
            shuffle,
        }),
    })));
}

fn ticks_ms(value: Option<&Value>) -> u64 {
    value.and_then(Value::as_u64).unwrap_or(0) / 10_000
}

fn names(item: &Value, key: &str, max: usize) -> String {
    item.get(key)
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter_map(Value::as_str)
                .take(max)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn people(item: &Value, kind: &str, max: usize) -> String {
    item.get("People")
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter(|p| p.get("Type").and_then(Value::as_str) == Some(kind))
                .filter_map(|p| p.get("Name").and_then(Value::as_str))
                .take(max)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn streams<'a>(item: &'a Value, kind: &str) -> Vec<&'a Value> {
    item.get("MediaStreams")
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter(|s| s.get("Type").and_then(Value::as_str) == Some(kind))
                .collect()
        })
        .unwrap_or_default()
}

fn stream_label(stream: &Value) -> String {
    let label = text(stream, "DisplayTitle");
    if label.is_empty() {
        text(stream, "Language")
    } else {
        label
    }
}

fn tags(item: &Value) -> Vec<String> {
    let mut tags = vec![];
    if let Some(video) = streams(item, "Video").first() {
        if video.get("Height").and_then(Value::as_u64).unwrap_or(0) >= 2000
            || video.get("Width").and_then(Value::as_u64).unwrap_or(0) >= 3800
        {
            tags.push("4K".to_owned());
        }
        let range = text(video, "VideoRange");
        if !range.is_empty() && range != "SDR" {
            tags.push(text(video, "VideoRangeType").replace("DOVI", "Dolby Vision"));
        }
    }
    if streams(item, "Audio")
        .iter()
        .any(|a| a.get("Channels").and_then(Value::as_u64).unwrap_or(0) >= 6)
    {
        tags.push("5.1".to_owned());
    }
    tags.retain(|t| !t.is_empty());
    tags
}

fn episodes(agent: &ureq::Agent, link: &JellyfinLink, series: &str) -> Vec<Value> {
    list(get(
        agent,
        link,
        &format!(
            "/Shows/{series}/Episodes?userId={}&Fields=Overview&EnableUserData=true",
            link.user_id
        ),
    ))
    .into_iter()
    .filter(|e| e.get("IndexNumber").is_some())
    .collect()
}

fn season_number(e: &Value) -> (i64, i64) {
    (
        e.get("ParentIndexNumber")
            .and_then(Value::as_i64)
            .unwrap_or(1),
        e.get("IndexNumber").and_then(Value::as_i64).unwrap_or(0),
    )
}

fn played(e: &Value) -> (bool, f32) {
    let data = e.get("UserData");
    let watched = data
        .and_then(|d| d.get("Played"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let share = data
        .and_then(|d| d.get("PlayedPercentage"))
        .and_then(Value::as_f64)
        .unwrap_or(0.) as f32
        / 100.;
    (watched, if watched { 1. } else { share })
}

fn details(reply: &mpsc::Sender<Response>, cache: &Path, link: &JellyfinLink, id: i64) {
    let agent = agent();
    let Some(guid) = item_of(id) else {
        return;
    };
    let Some(item) = get(
        &agent,
        link,
        &format!(
            "/Items/{guid}?userId={}&Fields=Overview,Genres,People,ProductionLocations,MediaStreams",
            link.user_id
        ),
    ) else {
        let _ = reply.send(Response::Failed(
            "This title is not on the server any more".into(),
        ));
        return;
    };
    let serial = text(&item, "Type") == "Series";
    let mut detail = Detail {
        id,
        serial,
        title: text(&item, "Name"),
        year: item
            .get("ProductionYear")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        minutes: (ticks_ms(item.get("RunTimeTicks")) / 60_000) as i64,
        genres: names(&item, "Genres", 3),
        countries: names(&item, "ProductionLocations", 2),
        plot: text(&item, "Overview"),
        director: people(&item, "Director", 2),
        cast: people(&item, "Actor", 5),
        tags: tags(&item),
        audio: streams(&item, "Audio")
            .iter()
            .map(|s| stream_label(s))
            .collect(),
        subtitles: streams(&item, "Subtitle")
            .iter()
            .filter(|s| s.get("IsTextSubtitleStream").and_then(Value::as_bool) == Some(true))
            .map(|s| (stream_label(s), String::new()))
            .collect(),
        ..Default::default()
    };
    if serial {
        let list = episodes(&agent, link, &guid);
        for e in &list {
            let (season, number) = season_number(e);
            let (watched, progress) = played(e);
            if !detail.seasons.contains(&season) {
                detail.seasons.push(season);
            }
            detail.episodes.push(Episode {
                season,
                number,
                title: text(e, "Name"),
                minutes: (ticks_ms(e.get("RunTimeTicks")) / 60_000) as i64,
                progress,
                watched,
                thumb_url: image_url(link, &text(e, "Id"), "Primary", "fillWidth=480"),
                thumb: None,
            });
        }
        detail.next = detail.episodes.iter().position(|e| !e.watched);
    }
    let _ = reply.send(Response::Details(Box::new(detail.clone())));
    // Pictures second, so the text shows without waiting for them.
    let name = format!("jf-{id}");
    detail.poster = poster(&agent, cache, link, &guid, &name);
    let has_backdrop = item
        .get("BackdropImageTags")
        .and_then(Value::as_array)
        .is_some_and(|v| !v.is_empty());
    detail.backdrop = if has_backdrop {
        crate::media::download_as(
            &agent,
            cache,
            &format!("{name}-b"),
            &image_url(link, &guid, "Backdrop/0", "fillWidth=1920"),
        )
    } else {
        None
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

/// HLS that Jellyfin remuxes (or transcodes only when it must) for the TV's
/// web video element. The URL carries the token: never logged.
fn hls_url(link: &JellyfinLink, item: &str, source: &str, session: &str) -> String {
    format!(
        "{}/Videos/{item}/master.m3u8?MediaSourceId={source}&ApiKey={}&DeviceId={}\
         &PlaySessionId={session}&VideoCodec=hevc,h264&AudioCodec=aac,ac3,eac3,mp3\
         &SegmentContainer=mp4&TranscodingMaxAudioChannels=6&MaxStreamingBitrate=120000000",
        link.server, link.token, link.device_id
    )
}

fn play(reply: &mpsc::Sender<Response>, link: &JellyfinLink, request: PlayRequest) {
    let agent = agent();
    let Some(guid) = item_of(request.id) else {
        return;
    };
    let fail = |why: &str| {
        let _ = reply.send(Response::Failed(why.to_owned()));
    };
    let Some(item) = get(
        &agent,
        link,
        &format!("/Items/{guid}?userId={}", link.user_id),
    ) else {
        return fail("This title is not on the server any more");
    };
    let series = text(&item, "Type") == "Series";
    let list = if series {
        episodes(&agent, link, &guid)
    } else {
        vec![]
    };
    let position = if series {
        let wanted = request.episode.or_else(|| {
            // Next up: the episode in progress, else the first one not watched.
            list.iter()
                .find(|e| {
                    let (watched, share) = played(e);
                    !watched && share > 0.
                })
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
    let target = position.map_or(&item, |i| &list[i]);
    let target_id = text(target, "Id");
    let info = agent
        .post(&format!(
            "{}/Items/{target_id}/PlaybackInfo?userId={}",
            link.server, link.user_id
        ))
        .header("Authorization", &auth(&link.device_id, &link.token))
        .header("Content-Type", "application/json")
        .send("{}")
        .ok()
        .and_then(read);
    let Some(source) = info
        .as_ref()
        .and_then(|i| i.get("MediaSources"))
        .and_then(Value::as_array)
        .and_then(|v| v.first())
        .cloned()
    else {
        return fail("The server has no playable file for this title");
    };
    let source_id = text(&source, "Id");
    let session = info
        .as_ref()
        .map(|i| text(i, "PlaySessionId"))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(crate::phone_link::random_hex);
    // MP4 plays as is (full quality, instant seeking); anything else goes
    // through HLS, which keeps the original streams whenever the TV can play them.
    let direct = text(&source, "Container")
        .split(',')
        .any(|c| c == "mp4" || c == "mov")
        && source.get("SupportsDirectPlay").and_then(Value::as_bool) == Some(true);
    let url = if direct {
        format!(
            "{}/Videos/{target_id}/stream?static=true&MediaSourceId={source_id}&ApiKey={}",
            link.server, link.token
        )
    } else {
        hls_url(link, &target_id, &source_id, &session)
    };
    let subtitles: Vec<crate::player::Track> = streams(&source, "Subtitle")
        .iter()
        .filter(|s| s.get("IsTextSubtitleStream").and_then(Value::as_bool) == Some(true))
        .map(|s| crate::player::Track {
            name: stream_label(s),
            url: Some(format!(
                "{}/Videos/{target_id}/{source_id}/Subtitles/{}/Stream.vtt?ApiKey={}",
                link.server,
                s.get("Index").and_then(Value::as_i64).unwrap_or(0),
                link.token
            )),
        })
        .collect();
    let selected_subtitle = request
        .subtitle
        .as_ref()
        .and_then(|want| subtitles.iter().position(|t| &t.name == want));
    let neighbour = |i: Option<usize>| i.and_then(|i| list.get(i)).map(season_number);
    let title = text(&item, "Name");
    let (title, episode_line) = match position {
        Some(i) => {
            let (season, number) = season_number(&list[i]);
            let name = text(&list[i], "Name");
            (
                format!("{title} · S{season}E{number}"),
                Some(if name.is_empty() {
                    format!("Season {season} · Episode {number}")
                } else {
                    format!("Season {season} · Episode {number} · {name}")
                }),
            )
        }
        None => (title, None),
    };
    let report = crate::player::Report {
        service: "jellyfin".into(),
        base: link.server.clone(),
        token: link.token.clone(),
        device_id: link.device_id.clone(),
        item: target_id.clone(),
        source: source_id.clone(),
        session: session.clone(),
        episodes: list
            .iter()
            .map(|e| {
                let (season, number) = season_number(e);
                let id = text(e, "Id");
                let name = text(e, "Name");
                crate::player::ReportEpisode {
                    season,
                    number,
                    url: hls_url(link, &id, &id, &session),
                    line: if name.is_empty() {
                        format!("Season {season} · Episode {number}")
                    } else {
                        format!("Season {season} · Episode {number} · {name}")
                    },
                    item: id,
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
            .get("UserData")
            .map_or(0, |d| ticks_ms(d.get("PlaybackPositionTicks"))),
        duration_ms: ticks_ms(target.get("RunTimeTicks")),
        poster: None,
        audio: streams(&source, "Audio")
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

    /// Real LAN: `cargo test discover_lan -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn discover_lan() {
        let (tx, rx) = mpsc::channel();
        discover(&tx, None);
        let Some(Response::Servers(list)) = rx.try_iter().last() else {
            panic!("no reply");
        };
        println!("{list:?}");
        assert!(!list.is_empty());
    }

    /// Real server: LUMA_JF_LINK=<file with the saved "jellyfin" JSON>
    /// `cargo test _live -- --ignored --nocapture`. Prints no token.
    #[test]
    #[ignore]
    fn library_live() {
        let path = std::env::var("LUMA_JF_LINK").expect("LUMA_JF_LINK");
        let link: JellyfinLink =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut lists = vec![
            "albums".to_owned(),
            "latest".to_owned(),
            "nextup".to_owned(),
        ];
        lists.extend(views(&agent(), &link).into_iter().map(|v| v.0));
        for list in lists {
            let first = library_page(&link, &list, "", 1).expect("page 1");
            let second = library_page(&link, &list, "", 2).expect("page 2");
            println!(
                "{list}: {} total, {} pages, page 1 {}, page 2 {}",
                first.total,
                first.pages,
                first.items.len(),
                second.items.len()
            );
        }
        let found = library_page(&link, "albums", "Claro", 1).expect("search");
        println!(
            "search Claro: {:?}",
            found.items.iter().map(|i| &i.1).collect::<Vec<_>>()
        );
    }

    #[test]
    #[ignore]
    fn album_live() {
        let path = std::env::var("LUMA_JF_LINK").expect("LUMA_JF_LINK");
        let link: JellyfinLink =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let agent = agent();
        let cards = collection_items(&agent, &link, "albums", 3);
        assert!(!cards.is_empty(), "no albums");
        let (guid, title) = card_target(&cards[0]);
        let id = card_id(&guid);
        assert!(is_album(id));
        let cache = std::env::temp_dir().join("luma-album-live");
        std::fs::create_dir_all(&cache).unwrap();
        let (tx, rx) = mpsc::channel();
        album(&tx, &cache, &link, id);
        let last = rx.try_iter().last();
        let Some(Response::Album(a)) = last else {
            panic!("no album")
        };
        println!(
            "{title}: {} · {} · cover {:?}",
            a.artist,
            a.meta(),
            a.cover.is_some()
        );
        assert!(!a.songs.is_empty() && a.cover.is_some() && a.backdrop.is_some());
        play_album(&tx, &link, id, 1, false);
        let Some(Response::Play(request)) = rx.try_iter().last() else {
            panic!("no play")
        };
        let music = request.music.unwrap();
        assert_eq!(music.start, 1);
        let response = agent
            .get(&music.songs[1].url)
            .header("Range", "bytes=0-1023")
            .call()
            .unwrap();
        let kind = response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or("").to_owned());
        // A server that lost its media folder answers 404/500 here: shown, not failed.
        println!("song 2: {} {:?}", response.status(), kind);
        assert!(
            views(&agent, &link)
                .iter()
                .any(|(k, _)| k.starts_with("music:"))
        );
    }

    #[test]
    fn container_address_takes_sender_host() {
        assert_eq!(
            with_host("http://10.21.0.15:8096", "192.168.1.68"),
            "http://192.168.1.68:8096"
        );
        assert_eq!(
            with_host("https://10.0.0.2:8920/jf", "192.168.1.5"),
            "https://192.168.1.5:8920"
        );
        assert_eq!(
            with_host("http://10.0.0.2", "192.168.1.5"),
            "http://192.168.1.5:8096"
        );
    }

    #[test]
    fn addresses_get_scheme_and_port() {
        assert_eq!(normalize("192.168.1.5"), "http://192.168.1.5:8096");
        assert_eq!(normalize("192.168.1.5:8920/"), "http://192.168.1.5:8920");
        assert_eq!(
            normalize("nas.local/jellyfin"),
            "http://nas.local:8096/jellyfin"
        );
        assert_eq!(
            normalize("https://demo.jellyfin.org/stable"),
            "https://demo.jellyfin.org/stable"
        );
        assert_eq!(normalize("  "), "");
    }

    #[test]
    fn card_ids_are_stable_and_distinct() {
        let a = card_id("f0b3381645f04afb9a0e392e74b6a1b0");
        assert_eq!(a, card_id("f0b3381645f04afb9a0e392e74b6a1b0"));
        assert!(a > 0 && owns(a));
        assert_eq!(
            item_of(a).as_deref(),
            Some("f0b3381645f04afb9a0e392e74b6a1b0")
        );
        assert!(!owns(123456));
    }

    #[test]
    fn episodes_stand_for_their_series() {
        let episode = json!({"Type": "Episode", "Id": "e1", "Name": "Pilot", "SeriesId": "s1", "SeriesName": "Show"});
        assert_eq!(card_target(&episode), ("s1".into(), "Show".into()));
        let movie = json!({"Type": "Movie", "Id": "m1", "Name": "Film"});
        assert_eq!(card_target(&movie), ("m1".into(), "Film".into()));
    }

    /// Whole flow against the public demo server; the demo user approves the
    /// code as the phone would. `cargo test jellyfin_demo -- --ignored`
    #[test]
    #[ignore]
    fn jellyfin_demo() {
        let server = "https://demo.jellyfin.org/stable".to_owned();
        let (reply, responses) = mpsc::channel();
        let device = "luma-test-device".to_owned();
        let worker = {
            let (reply, server, device) = (reply.clone(), server.clone(), device.clone());
            thread::spawn(move || connect(&reply, server, device, ATTEMPT.load(Ordering::SeqCst)))
        };
        let Ok(Response::Code { code, .. }) = responses.recv_timeout(Duration::from_secs(30))
        else {
            panic!("no code");
        };
        let agent = agent();
        let demo = agent
            .post(&format!("{server}/Users/AuthenticateByName"))
            .header("Authorization", &auth("luma-test-phone", ""))
            .header("Content-Type", "application/json")
            .send(r#"{"Username":"demo","Pw":""}"#)
            .ok()
            .and_then(read)
            .expect("demo login");
        let phone = auth("luma-test-phone", &text(&demo, "AccessToken"));
        let approved = agent
            .post(&format!("{server}/QuickConnect/Authorize?code={code}"))
            .header("Authorization", &phone)
            .send_empty()
            .expect("authorize");
        assert!(approved.status().is_success());
        let link = match responses.recv_timeout(Duration::from_secs(30)) {
            Ok(Response::Linked(link)) => link,
            _ => panic!("not linked"),
        };
        worker.join().unwrap();
        assert!(link.linked() && !link.user_id.is_empty());

        let cache = std::env::temp_dir().join("luma-jellyfin-test");
        let _ = std::fs::create_dir_all(&cache);
        fetch(
            &reply,
            &cache,
            &link,
            vec![(0, "resume".into()), (1, "latest".into())],
        );
        let mut rows = BTreeMap::new();
        while let Ok(Response::Items(i, items)) = responses.recv_timeout(Duration::from_secs(1)) {
            rows.insert(i, items);
        }
        let resume = rows.get(&0).expect("resume row");
        assert!(!resume.is_empty());
        assert!(rows.get(&1).is_some_and(|r| !r.is_empty()));
        assert!(
            resume.iter().all(|c| c.poster.is_some()),
            "posters downloaded"
        );
        assert!(!views(&agent, &link).is_empty());

        // Title card and Play for every card of the resume row.
        for card in resume {
            details(&reply, &cache, &link, card.id);
            let mut last = None;
            while let Ok(Response::Details(d)) = responses.recv_timeout(Duration::from_secs(1)) {
                last = Some(d);
            }
            let detail = last.expect("details");
            assert_eq!(detail.title, card.title);
            assert!(detail.poster.is_some() && detail.backdrop.is_some());
            if detail.serial {
                assert!(!detail.episodes.is_empty());
            }
            play(
                &reply,
                &link,
                PlayRequest {
                    id: card.id,
                    ..Default::default()
                },
            );
            let Ok(Response::Play(open)) = responses.recv_timeout(Duration::from_secs(20)) else {
                panic!("no play for {}", card.title);
            };
            let status = agent
                .get(&open.url)
                .header("Range", "bytes=0-100")
                .call()
                .unwrap()
                .status();
            assert!(status.is_success(), "{} stream {status}", card.title);
            assert!(open.report.is_some());
        }
    }
}
