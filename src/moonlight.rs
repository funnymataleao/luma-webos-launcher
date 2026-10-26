//! Sunshine/GameStream discovery, PIN pairing and cached PC games.
//! Only Luma's own RSA identity is used. All network calls run off the UI thread.
use crate::stacks::StackItem;
use aes::{
    Aes128,
    cipher::{BlockDecrypt, BlockEncrypt, KeyInit, generic_array::GenericArray},
};
use base64::Engine;
use rand::{RngCore, rngs::OsRng};
use rsa::{
    RsaPrivateKey, RsaPublicKey,
    pkcs1v15::{SigningKey, VerifyingKey},
    pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, LineEnding},
    signature::{SignatureEncoding, Signer, Verifier},
};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, Error as TlsError, SignatureScheme,
    StreamOwned,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::{verify_tls12_signature, verify_tls13_signature},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, Shutdown, SocketAddrV4, TcpStream, UdpSocket},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MoonlightLink {
    pub host_name: String,
    pub host_uuid: String,
    pub address: String,
    pub mac: String,
    pub https_port: u16,
    pub server_cert_pem: String,
    pub client_cert_pem: String,
    pub client_key_pem: String,
    pub unique_id: String,
    pub paired: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Host {
    pub name: String,
    pub address: String,
    pub uuid: String,
    pub mac: String,
    pub https_port: u16,
    pub paired: bool,
    pub online: bool,
    pub current_game: i64,
}
#[derive(Clone, Debug)]
pub struct Game {
    pub id: i64,
    pub title: String,
    pub hdr: bool,
    pub poster: Option<String>,
    /// Position in Apollo's list: apps added later come later.
    pub order: usize,
}

/// What Luma remembers about games itself; Apollo has no history or dates.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GameLibrary {
    /// Newest first: (Apollo app id, unix seconds) of games started from Luma.
    pub played: Vec<(i64, u64)>,
    /// When Luma first saw each game (0 = already there when history began).
    pub first_seen: std::collections::BTreeMap<i64, u64>,
    pub favorites: Vec<i64>,
}
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
impl GameLibrary {
    pub fn record_play(&mut self, id: i64) {
        self.played.retain(|(g, _)| *g != id);
        self.played.insert(0, (id, now_secs()));
        self.played.truncate(50);
    }
    pub fn toggle_favorite(&mut self, id: i64) {
        if let Some(i) = self.favorites.iter().position(|g| *g == id) {
            self.favorites.remove(i);
        } else {
            self.favorites.insert(0, id);
        }
    }
    /// Remember new games; returns true when something changed.
    pub fn note_seen(&mut self, games: &[Game]) -> bool {
        let start = if self.first_seen.is_empty() {
            0
        } else {
            now_secs()
        };
        let mut changed = false;
        for g in games {
            if let std::collections::btree_map::Entry::Vacant(e) = self.first_seen.entry(g.id) {
                e.insert(start);
                changed = true;
            }
        }
        changed
    }
    /// Cards for one Games stack: "recent", "added", "favorites" or all.
    pub fn select(&self, collection: &str, items: &[StackItem], games: &[Game]) -> Vec<StackItem> {
        let by_id = |id: i64| items.iter().find(|i| i.id == id).cloned();
        match collection {
            "recent" => self
                .played
                .iter()
                .filter_map(|(id, _)| by_id(*id))
                .collect(),
            "favorites" => self.favorites.iter().filter_map(|id| by_id(*id)).collect(),
            "added" => {
                let mut list: Vec<&Game> = games
                    .iter()
                    .filter(|g| !is_launcher_app(&g.title))
                    .collect();
                list.sort_by_key(|g| {
                    (
                        std::cmp::Reverse(self.first_seen.get(&g.id).copied().unwrap_or(0)),
                        std::cmp::Reverse(g.order),
                    )
                });
                list.iter().take(20).filter_map(|g| by_id(g.id)).collect()
            }
            _ => items.to_vec(),
        }
    }
}
pub enum Command {
    Probe(String),
    Discover(String),
    Pair(Host, MoonlightLink),
    Cancel,
    Fetch(MoonlightLink),
    Wake(String),
    WakeAndWait(String, String),
    /// Ask Apollo (authenticated) which game is running right now.
    CheckRunning(MoonlightLink),
    /// Close the running game on the PC; the stream ends with it.
    QuitGame(MoonlightLink),
}
pub enum Update {
    Found(Host),
    DiscoveryDone,
    Pin(String),
    Identity(MoonlightLink),
    PairProgress(String),
    PairError(String),
    Paired(MoonlightLink),
    Items(Vec<StackItem>, Vec<Game>),
    Status(String),
    HostOnline(bool),
    Running(i64),
    GameQuit(Result<(), String>),
}
pub struct Service {
    sender: Sender<Command>,
    pub receiver: Mutex<Receiver<Update>>,
}
// A duplicate host request must never kill the first getservercert session.
fn claim_pair_session(active: &mut Option<String>, key: &str) -> bool {
    if active.as_deref() == Some(key) {
        return false;
    }
    *active = Some(key.to_owned());
    true
}
impl Service {
    pub fn new(cache: PathBuf) -> Self {
        let (sender, commands) = mpsc::channel();
        let (updates, receiver) = mpsc::channel();
        let discovered = updates.clone();
        thread::spawn(move || {
            use mdns_sd::{ServiceDaemon, ServiceEvent};
            if let Ok(daemon) = ServiceDaemon::new()
                && let Ok(events) = daemon.browse("_nvstream._tcp.local.")
            {
                while let Ok(event) = events.recv() {
                    if let ServiceEvent::ServiceResolved(info) = event {
                        for ip in info.get_addresses_v4().iter().take(2) {
                            let address = ip.to_string();
                            // Never publish an unverified placeholder host.
                            // Temporary HTTP failures previously overwrote valid UUIDs.
                            if let Ok(host) = probe_fast(&address, Duration::from_millis(850)) {
                                let _ = discovered.send(Update::Found(host));
                            }
                        }
                    }
                }
            }
        });
        thread::spawn(move || {
            let mut cancel: Option<Arc<AtomicBool>> = None;
            // One live session per host. Repeated OK, pointer-click, mDNS and
            // duplicate side effects must not cancel the first PIN request.
            let active_pair: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
            while let Ok(cmd) = commands.recv() {
                match cmd {
                    Command::Discover(last) => {
                        let updates = updates.clone();
                        thread::spawn(move || {
                            let started = Instant::now();
                            if !last.is_empty()
                                && let Ok(h) = probe_fast(&last, Duration::from_millis(650))
                            {
                                let _ = updates.send(Update::Found(h));
                            }
                            let mut candidates = local_subnet_candidates();
                            candidates.retain(|ip| ip != &last);
                            let pool = 32usize;
                            let next = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                            let shared = Arc::new(candidates);
                            let mut workers = Vec::new();
                            for _ in 0..pool {
                                let next = next.clone();
                                let shared = shared.clone();
                                let updates = updates.clone();
                                workers.push(thread::spawn(move || {
                                    loop {
                                        let i = next.fetch_add(1, Ordering::Relaxed);
                                        if i >= shared.len() {
                                            break;
                                        }
                                        if let Ok(host) =
                                            probe_fast(&shared[i], Duration::from_millis(390))
                                        {
                                            let _ = updates.send(Update::Found(host));
                                        }
                                    }
                                }));
                            }
                            for worker in workers {
                                let _ = worker.join();
                            }
                            log_phase(
                                0,
                                &format!(
                                    "LAN scan completed in {}ms",
                                    started.elapsed().as_millis()
                                ),
                            );
                            let _ = updates.send(Update::DiscoveryDone);
                        });
                    }
                    Command::Probe(ip) => {
                        let updates = updates.clone();
                        thread::spawn(move || match probe(&ip) {
                            Ok(host) => {
                                let _ = updates.send(Update::Found(host));
                            }
                            Err(e) => {
                                let _ = updates.send(Update::Status(e));
                            }
                        });
                    }
                    Command::Pair(host, link) => {
                        // Stable address key survives mDNS UUID refreshes or manual-IP probes.
                        let key = host.address.clone();
                        {
                            let mut guard = active_pair.lock().unwrap();
                            if !claim_pair_session(&mut guard, &key) {
                                if cancel
                                    .as_ref()
                                    .is_some_and(|flag| flag.load(Ordering::SeqCst))
                                {
                                    let _=updates.send(Update::PairError(
                                        "Sunshine is finishing the previous pairing request. Please wait before trying again.".into()
                                    ));
                                    log_phase(
                                        0,
                                        "duplicate Pair blocked after Cancel while original HTTP request remains active",
                                    );
                                } else {
                                    log_phase(
                                        0,
                                        "duplicate Pair ignored; original PIN session retained",
                                    );
                                }
                                continue;
                            }
                            // Only switch hosts on explicit user action.
                            if let Some(flag) = cancel.take() {
                                flag.store(true, Ordering::SeqCst);
                            }
                        }
                        let flag = Arc::new(AtomicBool::new(false));
                        cancel = Some(flag.clone());
                        let updates = updates.clone();
                        let active = active_pair.clone();
                        thread::spawn(move || {
                            log_phase(0, "pair worker started");
                            // A panic must never leave the host reserved and the PIN
                            // tiles empty: report it and release the session below.
                            let attempt =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    let restored = match recover_existing_pair(&host, &link) {
                                        Ok(found) => found,
                                        Err(error) => {
                                            log_phase(
                                                0,
                                                &format!(
                                                    "saved identity check failed, using PIN: {}",
                                                    error
                                                        .chars()
                                                        .filter(|c| !c.is_control())
                                                        .take(140)
                                                        .collect::<String>()
                                                ),
                                            );
                                            None
                                        }
                                    };
                                    if let Some(restored) = restored {
                                        log_phase(
                                            0,
                                            "Apollo authorized saved Luma identity; no PIN",
                                        );
                                        return Ok(restored);
                                    }
                                    if flag.load(Ordering::SeqCst) {
                                        return Err("cancelled".into());
                                    }
                                    let pin = format!("{:04}", OsRng.next_u32() % 10000);
                                    let _ = updates.send(Update::Pin(pin.clone()));
                                    pair(host, link, &pin, &flag, &updates)
                                }));
                            let result = attempt.unwrap_or_else(|_| {
                                log_phase(0, "pair worker panicked");
                                Err("Pairing failed inside Luma. Try again.".into())
                            });
                            let was_cancelled = flag.load(Ordering::SeqCst);
                            {
                                let mut guard = active.lock().unwrap();
                                if guard.as_ref() == Some(&key) {
                                    *guard = None;
                                }
                            }
                            if !was_cancelled {
                                match result {
                                    Ok(link) => {
                                        let _ = updates.send(Update::Paired(link));
                                    }
                                    Err(err) => {
                                        log_phase(0, "pairing failed; consult current step");
                                        let _ = updates.send(Update::PairError(err));
                                    }
                                }
                            }
                        });
                    }
                    Command::Cancel => {
                        // The synchronous phase-1 HTTP call may still be blocked waiting
                        // for a PIN. Do not release the active host marker until the
                        // worker actually exits, otherwise another Pair causes Sunshine
                        // 409 while the first session is still in flight.
                        if let Some(flag) = cancel.as_ref() {
                            flag.store(true, Ordering::SeqCst);
                        }
                        log_phase(
                            0,
                            "pair cancelled on UI; host remains reserved until worker exits",
                        );
                    }
                    Command::Fetch(link) => {
                        let updates = updates.clone();
                        let cache = cache.clone();
                        // The saved list shows the row at once; Apollo's answer
                        // replaces it when it comes.
                        let (items, games) = load_cached(&cache, &link.host_uuid);
                        if !items.is_empty() {
                            let _ = updates.send(Update::Items(items, games));
                        }
                        thread::spawn(move || match fetch(&link, &cache) {
                            Ok((items, mut games)) => {
                                let _ = updates.send(Update::HostOnline(true));
                                let _ = updates.send(Update::Items(items, games.clone()));
                                // Shrink new covers after the row is up, never before it.
                                let mut changed = false;
                                for g in &mut games {
                                    if let Some(png) =
                                        g.poster.clone().filter(|p| p.ends_with(".png"))
                                        && let Some(jpg) = poster_fit(Path::new(&png))
                                    {
                                        g.poster = Some(jpg);
                                        changed = true;
                                    }
                                }
                                if changed {
                                    let items = games
                                        .iter()
                                        .map(|g| StackItem {
                                            id: g.id,
                                            title: g.title.clone(),
                                            poster: g.poster.clone(),
                                        })
                                        .collect();
                                    let _ = updates.send(Update::Items(items, games));
                                }
                            }
                            Err(err) => {
                                let _ = updates.send(Update::HostOnline(false));
                                let (items, games) = load_cached(&cache, &link.host_uuid);
                                if !items.is_empty() {
                                    let _ = updates.send(Update::Items(items, games));
                                }
                                let _ = updates.send(Update::Status(err));
                            }
                        });
                    }
                    Command::WakeAndWait(mac, ip) => {
                        let updates = updates.clone();
                        thread::spawn(move || {
                            if let Err(error) = wake(&mac) {
                                let _ = updates.send(Update::Status(error));
                                return;
                            }
                            let _ = updates.send(Update::Status("Waking PC…".into()));
                            for _ in 0..10 {
                                if probe(&ip).is_ok() {
                                    let _ = updates.send(Update::HostOnline(true));
                                    return;
                                }
                                thread::sleep(Duration::from_secs(3));
                            }
                            let _ = updates
                                .send(Update::Status("PC did not wake within 30 seconds".into()));
                        });
                    }
                    Command::CheckRunning(link) => {
                        let updates = updates.clone();
                        thread::spawn(move || {
                            if let Ok(id) = running_game(&link) {
                                let _ = updates.send(Update::Running(id));
                            }
                        });
                    }
                    Command::QuitGame(link) => {
                        let updates = updates.clone();
                        thread::spawn(move || {
                            let result = quit_game(&link);
                            log_phase(
                                0,
                                if result.is_ok() {
                                    "quit game ok"
                                } else {
                                    "quit game failed"
                                },
                            );
                            let _ = updates.send(Update::GameQuit(result));
                        });
                    }
                    Command::Wake(mac) => {
                        let updates = updates.clone();
                        thread::spawn(move || {
                            let _ = updates.send(Update::Status(match wake(&mac) {
                                Ok(()) => "Wake-on-LAN sent".into(),
                                Err(e) => e,
                            }));
                        });
                    }
                }
            }
        });
        Self {
            sender,
            receiver: Mutex::new(receiver),
        }
    }
    pub fn send(&self, cmd: Command) {
        let _ = self.sender.send(cmd);
    }
}
fn xml(raw: &str, tag: &str) -> String {
    let (start, end) = (format!("<{tag}>"), format!("</{tag}>"));
    let Some(from) = raw.find(&start) else {
        return String::new();
    };
    let rest = &raw[from + start.len()..];
    let Some(to) = rest.find(&end) else {
        return String::new();
    };
    rest[..to]
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}
fn parse_server(raw: &str, address: &str) -> Host {
    Host {
        name: xml(raw, "hostname"),
        address: address.into(),
        uuid: xml(raw, "uniqueid"),
        mac: xml(raw, "mac"),
        https_port: xml(raw, "HttpsPort").parse().unwrap_or(47984),
        paired: xml(raw, "PairStatus") == "1",
        online: true,
        current_game: xml(raw, "currentgame").parse().unwrap_or(0),
    }
}
fn parse_apps(raw: &str) -> Vec<Game> {
    let mut games = Vec::new();
    for fragment in raw.split("<App>").skip(1) {
        let Some((part, _)) = fragment.split_once("</App>") else {
            continue;
        };
        let Ok(id) = xml(part, "ID").parse::<i64>() else {
            continue;
        };
        // Apollo pads titles with leading spaces to pin entries on top.
        let title = xml(part, "AppTitle").trim().to_owned();
        if title.is_empty() || is_service_app(&title) {
            continue;
        }
        let key = title.to_lowercase();
        let hdr = xml(part, "IsHdrSupported") == "1";
        // The same game can be listed twice (a pinned copy): keep one card.
        if let Some(seen) = games
            .iter_mut()
            .find(|g: &&mut Game| g.title.to_lowercase() == key)
        {
            seen.hdr |= hdr;
            continue;
        }
        let order = games.len();
        games.push(Game {
            id,
            title,
            hdr,
            poster: None,
            order,
        });
    }
    games.sort_by_key(|g| (is_launcher_app(&g.title) as u8, g.title.to_lowercase()));
    games
}
/// Apollo/Sunshine controls and Steam tools, not games.
fn is_service_app(title: &str) -> bool {
    matches!(
        title.to_lowercase().as_str(),
        "remote monitor"
            | "remote input"
            | "resume"
            | "terminate"
            | "proton experimental"
            | "steamworks common redistributables"
            | "steamvr"
            | "lossless scaling"
            | "wallpaper engine"
            | "big picture mode"
    )
}
/// Whole-PC launchers stay available, after the games.
/// The card JPEG when it was made already, else the PNG for now.
fn fitted_or_png(png: &Path) -> String {
    if png.to_string_lossy().ends_with(CARD_SUFFIX) {
        return png.to_string_lossy().into_owned();
    }
    let jpg = card_path(png);
    if jpg.exists() { jpg } else { png.to_path_buf() }
        .to_string_lossy()
        .into_owned()
}
const CARD_SUFFIX: &str = "-card.jpg";
fn card_path(png: &Path) -> PathBuf {
    let stem = png.file_stem().unwrap_or_default().to_string_lossy();
    png.with_file_name(format!("{stem}{CARD_SUFFIX}"))
}

/// Apollo covers are big PNGs (often 3:4), and 3:4 looked squashed in a 2:3
/// card. The renderer keeps 48 MB of textures: 100 covers at twice the card
/// size did not fit, so a long Games row decoded covers again while gliding
/// back. Keep a 2:3 JPEG at card size (270x405), like the movie posters.
pub fn poster_fit(png: &std::path::Path) -> Option<String> {
    let out = card_path(png);
    if !out.exists() {
        let image = image::open(png).ok()?.into_rgb8();
        let (w, h) = (image.width() as f32, image.height() as f32);
        let aspect = 2. / 3.;
        let (cw, ch) = if w / h > aspect {
            (h * aspect, h)
        } else {
            (w, w / aspect)
        };
        let x = ((w - cw) / 2.).round() as u32;
        let y = ((h - ch) / 2.).round() as u32;
        let crop = image::imageops::crop_imm(&image, x, y, cw as u32, ch as u32).to_image();
        let width = 270.min(crop.width());
        let height = (width as f32 * 1.5).round() as u32;
        image::imageops::resize(&crop, width, height, image::imageops::FilterType::Triangle)
            .save(&out)
            .ok()?;
    }
    Some(out.to_string_lossy().into())
}

pub fn is_launcher_app(title: &str) -> bool {
    matches!(
        title.to_lowercase().as_str(),
        "desktop" | "steam big picture" | "xbox game pass"
    )
}
fn address(raw: &str) -> Result<String, String> {
    let ip: Ipv4Addr = raw
        .trim()
        .parse()
        .map_err(|_| "Enter a valid PC IPv4 address".to_string())?;
    if ip.is_unspecified() || ip.is_loopback() || ip.is_multicast() || ip.is_broadcast() {
        return Err("Invalid LAN host address".into());
    }
    Ok(ip.to_string())
}
fn rand_bytes(n: usize) -> Vec<u8> {
    let mut b = vec![0; n];
    OsRng.fill_bytes(&mut b);
    b
}
fn query(id: &str) -> String {
    format!("uniqueid={id}&uuid={}", hex::encode(rand_bytes(16)))
}
fn http(ip: &str, path: &str, timeout: Duration) -> Result<Vec<u8>, String> {
    let url = format!("http://{ip}:47989{path}");
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .new_agent();
    let mut response = agent
        .get(&url)
        .call()
        // Avoid leaking the query string containing hex-encoded client cert.
        .map_err(|err| match err {
            ureq::Error::Timeout(_) => "Sunshine request timed out".into(),
            ureq::Error::StatusCode(code) => format!("Sunshine HTTP {code}"),
            _ => "Cannot reach Sunshine".into(),
        })?;
    response
        .body_mut()
        .read_to_vec()
        .map_err(|_| "Sunshine response could not be read".to_string())
}
pub fn local_subnet_candidates() -> Vec<String> {
    let mut result = Vec::new();
    let mut bases = std::collections::HashSet::new();
    if let Ok(mut interfaces) = if_addrs::get_if_addrs() {
        interfaces.sort_by_key(|iface| {
            let name = iface.name.to_ascii_lowercase();
            if name.starts_with("eth") || name.starts_with("wlan") || name.starts_with("en") {
                0
            } else {
                1
            }
        });
        for iface in interfaces {
            if let if_addrs::IfAddr::V4(v4) = iface.addr {
                let ip = v4.ip;
                if !ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified()
                {
                    continue;
                }
                // Only local IPv4, no public address scans or wider-than-/24 sweeps.
                let n = ip.octets();
                let base = (n[0], n[1], n[2]);
                if bases.is_empty() && bases.insert(base) {
                    for last in 1..255u16 {
                        if last as u8 != n[3] {
                            result.push(format!("{}.{}.{}.{}", n[0], n[1], n[2], last))
                        }
                    }
                }
            }
        }
    }
    result
}
fn probe_fast(ip: &str, timeout: Duration) -> Result<Host, String> {
    let ip = address(ip)?;
    let raw = http(&ip, "/serverinfo?uniqueid=0000000000000000&uuid=0", timeout)?;
    let host = parse_server(&String::from_utf8_lossy(&raw), &ip);
    if host.name.is_empty() {
        return Err("Not a Sunshine PC".into());
    }
    Ok(host)
}
pub fn probe(ip: &str) -> Result<Host, String> {
    let ip = address(ip)?;
    let raw = http(
        &ip,
        "/serverinfo?uniqueid=0000000000000000&uuid=0",
        Duration::from_secs(4),
    )?;
    let host = parse_server(&String::from_utf8_lossy(&raw), &ip);
    if host.name.is_empty() {
        return Err("Host did not return Sunshine serverinfo".into());
    }
    Ok(host)
}
fn identity() -> Result<MoonlightLink, String> {
    let private = RsaPrivateKey::new(&mut OsRng, 2048).map_err(|e| e.to_string())?;
    let pem = private
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| e.to_string())?
        .to_string();
    let kp = rcgen::KeyPair::from_pem(&pem).map_err(|e| e.to_string())?;
    let mut params =
        rcgen::CertificateParams::new(vec!["Luma".into()]).map_err(|e| e.to_string())?;
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "Luma");
    let cert = params.self_signed(&kp).map_err(|e| e.to_string())?;
    Ok(MoonlightLink {
        client_cert_pem: cert.pem(),
        client_key_pem: pem,
        unique_id: hex::encode(rand_bytes(8)),
        ..Default::default()
    })
}
fn der(pem: &str) -> Result<Vec<u8>, String> {
    rustls_pemfile::certs(&mut BufReader::new(pem.as_bytes()))
        .next()
        .ok_or("Certificate missing".to_string())?
        .map(|c| c.to_vec())
        .map_err(|e| e.to_string())
}
fn cert_signature(pem: &str) -> Result<Vec<u8>, String> {
    let data = der(pem)?;
    let (_, cert) = x509_parser::parse_x509_certificate(&data).map_err(|e| e.to_string())?;
    Ok(cert.signature_value.data.to_vec())
}
fn verify_secret(pem: &str, secret: &[u8], signature: &[u8]) -> Result<(), String> {
    let data = der(pem)?;
    let (_, cert) = x509_parser::parse_x509_certificate(&data).map_err(|e| e.to_string())?;
    let public =
        RsaPublicKey::from_public_key_der(cert.public_key().raw).map_err(|e| e.to_string())?;
    let signature = rsa::pkcs1v15::Signature::try_from(signature).map_err(|e| e.to_string())?;
    VerifyingKey::<Sha256>::new(public)
        .verify(secret, &signature)
        .map_err(|_| "Sunshine certificate signature mismatch".into())
}
fn aes_ecb(input: &[u8], key: &[u8], decrypt: bool) -> Result<Vec<u8>, String> {
    if !input.len().is_multiple_of(16) || key.len() != 16 {
        return Err("Invalid AES block".into());
    }
    let aes = Aes128::new_from_slice(key).map_err(|e| e.to_string())?;
    let mut out = input.to_vec();
    for block in out.chunks_exact_mut(16) {
        let b = GenericArray::from_mut_slice(block);
        if decrypt {
            aes.decrypt_block(b)
        } else {
            aes.encrypt_block(b)
        }
    }
    Ok(out)
}
fn aes_key(salt: &[u8], pin: &str) -> [u8; 16] {
    let hash = Sha256::digest([salt, pin.as_bytes()].concat());
    let mut key = [0; 16];
    key.copy_from_slice(&hash[..16]);
    key
}
#[derive(Debug)]
struct Pin {
    der: Vec<u8>,
    // Legacy recovery only: capture Apollo certificate before strict pinning.
    observed: Option<Arc<Mutex<Option<Vec<u8>>>>>,
}
impl ServerCertVerifier for Pin {
    fn verify_server_cert(
        &self,
        cert: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        if cert.as_ref() == self.der {
            Ok(ServerCertVerified::assertion())
        } else if self.der.is_empty() {
            // Only the legacy, already-authorized identity recovery path.
            if let Some(observed) = &self.observed {
                *observed.lock().unwrap() = Some(cert.as_ref().to_vec());
                Ok(ServerCertVerified::assertion())
            } else {
                Err(TlsError::InvalidCertificate(
                    rustls::CertificateError::UnknownIssuer,
                ))
            }
        } else {
            Err(TlsError::InvalidCertificate(
                rustls::CertificateError::UnknownIssuer,
            ))
        }
    }
    fn verify_tls12_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls12_signature(
            msg,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls13_signature(
            msg,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
// Parse HTTP framing instead of waiting for a persistent TLS socket to close.
// Apollo/Sunshine can keep HTTPS alive after sending Content-Length or chunks.
const HTTPS_MAX_BODY: usize = 16 * 1024 * 1024;
fn read_https_response<R: Read>(source: &mut R) -> Result<Vec<u8>, String> {
    let mut reader = BufReader::new(source);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("HTTPS status read: {e}"))?;
    if !line.starts_with("HTTP/1.1 200 ") && !line.starts_with("HTTP/1.0 200 ") {
        return Err(format!(
            "HTTPS server returned {}",
            line.split_whitespace().nth(1).unwrap_or("invalid status")
        ));
    }
    let mut header_bytes = line.len();
    let mut content_length = None;
    let mut is_chunked = false;
    loop {
        line.clear();
        reader
            .read_line(&mut line)
            .map_err(|e| format!("HTTPS headers: {e}"))?;
        header_bytes += line.len();
        if line.is_empty() || header_bytes > 16 * 1024 {
            return Err("HTTPS header missing or too large".into());
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            let val = value.trim();
            if key.eq_ignore_ascii_case("content-length") {
                content_length = Some(val.parse::<usize>().map_err(|_| "Bad Content-Length")?);
            } else if key.eq_ignore_ascii_case("transfer-encoding") {
                is_chunked |= val
                    .split(',')
                    .any(|v| v.trim().eq_ignore_ascii_case("chunked"));
            }
        }
    }
    if is_chunked {
        let mut out = Vec::new();
        loop {
            line.clear();
            reader
                .read_line(&mut line)
                .map_err(|e| format!("HTTPS chunk header: {e}"))?;
            if line.len() > 128 || line.is_empty() {
                return Err("Invalid HTTPS chunk header".into());
            }
            let size = usize::from_str_radix(line.trim().split(';').next().unwrap_or(""), 16)
                .map_err(|_| "Invalid HTTPS chunk size")?;
            if size > HTTPS_MAX_BODY.saturating_sub(out.len()) {
                return Err("HTTPS response too large".into());
            }
            if size == 0 {
                // Consume trailers. Never wait for the server to close keep-alive.
                loop {
                    line.clear();
                    reader
                        .read_line(&mut line)
                        .map_err(|e| format!("HTTPS trailer: {e}"))?;
                    if line == "\r\n" || line == "\n" {
                        return Ok(out);
                    }
                    if line.is_empty() || line.len() > 16 * 1024 {
                        return Err("Invalid HTTPS trailer".into());
                    }
                }
            }
            let offset = out.len();
            out.resize(offset + size, 0);
            reader
                .read_exact(&mut out[offset..])
                .map_err(|e| format!("HTTPS chunk data: {e}"))?;
            let mut terminator = [0u8; 2];
            reader
                .read_exact(&mut terminator)
                .map_err(|e| format!("HTTPS chunk terminator: {e}"))?;
            if terminator != *b"\r\n" {
                return Err("Bad HTTPS chunk terminator".into());
            }
        }
    }
    let mut body = Vec::new();
    if let Some(size) = content_length {
        if size > HTTPS_MAX_BODY {
            return Err("HTTPS response too large".into());
        }
        body.resize(size, 0);
        reader
            .read_exact(&mut body)
            .map_err(|e| format!("HTTPS body: {e}"))?;
    } else {
        reader
            .take((HTTPS_MAX_BODY + 1) as u64)
            .read_to_end(&mut body)
            .map_err(|e| format!("HTTPS body without framing: {e}"))?;
        if body.len() > HTTPS_MAX_BODY {
            return Err("HTTPS response too large".into());
        }
    }
    Ok(body)
}
fn https(link: &MoonlightLink, path: &str, timeout: Duration) -> Result<Vec<u8>, String> {
    https_inner(link, path, timeout, false).map(|(body, _)| body)
}
fn https_inner(
    link: &MoonlightLink,
    path: &str,
    timeout: Duration,
    legacy_recovery: bool,
) -> Result<(Vec<u8>, Option<Vec<u8>>), String> {
    let deadline = Instant::now() + timeout;
    let ip = address(&link.address)?;
    let cert = der(&link.client_cert_pem)?;
    let key = rustls_pemfile::private_key(&mut BufReader::new(link.client_key_pem.as_bytes()))
        .map_err(|e| e.to_string())?
        .ok_or("Missing Luma RSA key")?;
    let observed = Arc::new(Mutex::new(None));
    let pinned = if legacy_recovery && link.server_cert_pem.is_empty() {
        Vec::new()
    } else {
        der(&link.server_cert_pem)?
    };
    // Both ring and aws-lc-rs are compiled in (ureq pulls ring), so the
    // process-default ClientConfig::builder() panics. Always name the provider.
    let config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|e| format!("TLS provider: {e}"))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Pin {
                der: pinned,
                observed: if legacy_recovery {
                    Some(observed.clone())
                } else {
                    None
                },
            }))
            .with_client_auth_cert(vec![CertificateDer::from(cert)], key)
            .map_err(|e| format!("TLS client identity: {e}"))?;
    let addr = SocketAddrV4::new(ip.parse().map_err(|e| format!("{e}"))?, link.https_port);
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err("HTTPS deadline expired before connect".into());
    }
    let tcp = TcpStream::connect_timeout(&addr.into(), remaining)
        .map_err(|e| format!("HTTPS connect: {e}"))?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err("HTTPS deadline expired after connect".into());
    }
    tcp.set_read_timeout(Some(remaining))
        .map_err(|e| e.to_string())?;
    tcp.set_write_timeout(Some(remaining))
        .map_err(|e| e.to_string())?;

    // Absolute wall-clock deadline, including TLS handshakes and any repeated
    // socket reads inside rustls. Shutting down a clone unblocks a stuck reader.
    let watchdog_socket = tcp.try_clone().map_err(|e| e.to_string())?;
    let (done, watch) = mpsc::channel::<()>();
    let expired = Arc::new(AtomicBool::new(false));
    let timed_out = expired.clone();
    let watchdog = thread::spawn(move || {
        if watch.recv_timeout(remaining).is_err() {
            timed_out.store(true, Ordering::SeqCst);
            let _ = watchdog_socket.shutdown(Shutdown::Both);
        }
    });
    let result = (|| {
        // An IPv4 ServerName is IpAddress and does not send hostname SNI.
        let conn = ClientConnection::new(
            Arc::new(config),
            ServerName::try_from(ip.clone()).map_err(|e| format!("TLS IP: {e}"))?,
        )
        .map_err(|e| format!("TLS setup: {e}"))?;
        let mut stream = StreamOwned::new(conn, tcp);
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: {ip}\r\nConnection: close\r\nAccept: */*\r\n\r\n"
        )
        .map_err(|e| format!("HTTPS write/TLS handshake: {e}"))?;
        stream.flush().map_err(|e| format!("HTTPS flush: {e}"))?;
        read_https_response(&mut stream)
    })();
    let _ = done.send(());
    let _ = watchdog.join();
    if expired.load(Ordering::SeqCst) {
        Err(format!("HTTPS deadline exceeded ({}s)", timeout.as_secs()))
    } else {
        result.map(|payload| (payload, observed.lock().unwrap().clone()))
    }
}
fn running_game(link: &MoonlightLink) -> Result<i64, String> {
    if !link.paired {
        return Err("not paired".into());
    }
    let payload = https(
        link,
        &format!("/serverinfo?{}", query(&link.unique_id)),
        Duration::from_secs(4),
    )?;
    let raw = String::from_utf8_lossy(&payload);
    Ok(xml(&raw, "currentgame").parse().unwrap_or(0))
}
fn quit_game(link: &MoonlightLink) -> Result<(), String> {
    let payload = https(
        link,
        &format!("/cancel?{}", query(&link.unique_id)),
        Duration::from_secs(8),
    )?;
    let raw = String::from_utf8_lossy(&payload);
    if xml_attr(&raw, "status_code") != "200" {
        return Err(format!("{} refused to close the game", link.host_name));
    }
    Ok(())
}
fn authenticated_serverinfo_paired(link: &MoonlightLink) -> Result<bool, String> {
    if link.server_cert_pem.is_empty()
        || link.client_cert_pem.is_empty()
        || link.client_key_pem.is_empty()
    {
        return Err("No pinned Sunshine certificate or Luma identity".into());
    }
    let payload = https(
        link,
        &format!("/serverinfo?{}", query(&link.unique_id)),
        Duration::from_secs(8),
    )?;
    let xml_text = String::from_utf8_lossy(&payload);
    let status = xml_attr(&xml_text, "status_code");
    let paired = xml(&xml_text, "PairStatus") == "1";
    log_phase(
        5,
        &format!("authenticated serverinfo status={status} paired={paired}"),
    );
    if status != "200" {
        return Err(format!("HTTPS serverinfo status={status}"));
    }
    Ok(paired)
}

// Legacy 2.2.94 stored the client identity, but lost the server pin before
// phase 5. Trust the first local TLS certificate only when Apollo accepts our
// EXISTING client identity and returns the same previously recorded host UUID.
// Immediately pin it and verify through a second, strictly pinned TLS request.
fn recover_existing_pair(
    host: &Host,
    link: &MoonlightLink,
) -> Result<Option<MoonlightLink>, String> {
    if link.paired
        || link.host_uuid.is_empty()
        || link.host_uuid != host.uuid
        || link.address != host.address
        || link.unique_id.is_empty()
        || link.client_cert_pem.is_empty()
        || link.client_key_pem.is_empty()
    {
        return Ok(None);
    }
    let mut restored = link.clone();
    restored.https_port = host.https_port.max(1);
    if restored.server_cert_pem.is_empty() {
        let (body, observed) = https_inner(
            &restored,
            &format!("/serverinfo?{}", query(&restored.unique_id)),
            Duration::from_secs(4),
            true,
        )?;
        let raw = String::from_utf8_lossy(&body);
        if xml_attr(&raw, "status_code") != "200"
            || xml(&raw, "PairStatus") != "1"
            || xml(&raw, "uniqueid") != host.uuid
        {
            return Ok(None);
        }
        let certificate = observed.ok_or("Apollo TLS certificate not received")?;
        x509_parser::parse_x509_certificate(&certificate)
            .map_err(|_| "Apollo TLS certificate is invalid")?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(certificate);
        let mut pem = String::from("-----BEGIN CERTIFICATE-----\n");
        for chunk in encoded.as_bytes().chunks(64) {
            pem.push_str(std::str::from_utf8(chunk).map_err(|_| "Invalid certificate encoding")?);
            pem.push('\n');
        }
        pem.push_str("-----END CERTIFICATE-----\n");
        restored.server_cert_pem = pem;
        log_phase(
            0,
            "Apollo accepted saved client identity; checking strict certificate pin",
        );
    }
    if !authenticated_serverinfo_paired(&restored)? {
        return Ok(None);
    }
    restored.paired = true;
    Ok(Some(restored))
}
fn log_phase(stage: usize, message: &str) {
    // Only non-sensitive stage numbers, HTTP codes and field lengths belong here.
    // NEVER write keys, PIN, certificates, pairing query strings or XML values.
    let root = if Path::new("/media/developer/com.denisefremov.luma").exists() {
        PathBuf::from("/media/developer/com.denisefremov.luma")
    } else {
        std::env::temp_dir()
    };
    if let Ok(mut file) = OpenOptions::new()
        .append(true)
        .create(true)
        .open(root.join("luma-moonlight.log"))
    {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = writeln!(file, "{time} stage={stage} {message}");
    }
}
fn phase(
    link: &MoonlightLink,
    stage: usize,
    opts: &str,
    secure: bool,
    timeout: Duration,
) -> Result<String, String> {
    let path = format!("/pair?{}&{opts}", query(&link.unique_id));
    log_phase(stage, if secure { "begin HTTPS" } else { "begin HTTP" });
    let payload = if secure {
        https(link, &path, timeout)
    } else {
        http(&link.address, &path, timeout)
    }
    .map_err(|e| {
        let lower = e.to_lowercase();
        let msg = if lower.contains("timed out") || lower.contains("deadline") {
            if stage == 1 {
                "Sunshine PIN approval timed out"
            } else {
                "request timed out"
            }
        } else if lower.contains("certificate") || lower.contains("tls") {
            "secure connection/TLS error"
        } else if e.contains("HTTP 503") {
            "Sunshine is busy with other pairing sessions"
        } else {
            "network/transport error"
        };
        // Error kind and transport context only, never request path, keys or certificates.
        log_phase(
            stage,
            &format!(
                "HTTPS/HTTP transport failure: {}",
                e.chars()
                    .filter(|c| !c.is_control())
                    .take(160)
                    .collect::<String>()
            ),
        );
        format!("Pairing step {stage}/5: {msg}. Check Sunshine, or try again.")
    })?;
    let text = String::from_utf8_lossy(&payload).into_owned();
    let status = xml_attr(&text, "status_code");
    let paired = xml(&text, "paired");
    let lengths = match stage {
        1 => xml(&text, "plaincert").len(),
        2 => xml(&text, "challengeresponse").len(),
        3 => xml(&text, "pairingsecret").len(),
        _ => 0,
    };
    log_phase(
        stage,
        &format!(
            "HTTP/XML status={} paired={} field_hex_len={}",
            status, paired, lengths
        ),
    );
    if status != "200" || paired != "1" {
        let reason = xml_attr(&text, "status_message");
        let reason = if reason.is_empty() {
            "Sunshine rejected this step".into()
        } else {
            reason.chars().take(100).collect::<String>()
        };
        if stage == 1 && status == "409" {
            return Err("Pairing already in progress on the PC — enter the PIN shown here".into());
        }
        return Err(format!("Pairing step {stage}/5: {reason}. Try again."));
    }
    Ok(text)
}
fn xml_attr(raw: &str, name: &str) -> String {
    let pattern = format!("{name}={}", '"');
    if let Some(from) = raw.find(&pattern) {
        let v = &raw[from + pattern.len()..];
        if let Some(end) = v.find('"') {
            return v[..end].chars().take(100).collect();
        }
    }
    String::new()
}
fn pair(
    host: Host,
    mut link: MoonlightLink,
    pin: &str,
    cancel: &AtomicBool,
    updates: &Sender<Update>,
) -> Result<MoonlightLink, String> {
    if link.client_key_pem.is_empty() {
        link = identity()?
    }
    link.host_name = host.name;
    link.host_uuid = host.uuid;
    link.address = address(&host.address)?;
    link.mac = host.mac;
    link.https_port = host.https_port.max(1);
    // Persist RSA identity before starting: retries must reuse the same
    // certificate/unique_id instead of polluting Sunshine with fresh devices.
    let _ = updates.send(Update::Identity(link.clone()));
    let salt = rand_bytes(16);
    let key = aes_key(&salt, pin);
    let cert_hex = hex::encode(link.client_cert_pem.as_bytes());
    let _ = updates.send(Update::PairProgress(
        "Step 1/5: waiting for Sunshine PIN approval…".into(),
    ));
    let first = phase(
        &link,
        1,
        &format!(
            "devicename=roth&updateState=1&phrase=getservercert&salt={}&clientcert={}",
            hex::encode(salt),
            cert_hex
        ),
        false,
        Duration::from_secs(260),
    )?;
    if cancel.load(Ordering::SeqCst) {
        return Err("Pairing cancelled".into());
    }
    link.server_cert_pem =
        String::from_utf8(hex::decode(xml(&first, "plaincert")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    // Persist the verified identity + pinned certificate before later phases:
    // Apollo can authorize the client at stage 4 and leave stage 5 unfinished.
    let _ = updates.send(Update::Identity(link.clone()));
    let challenge = rand_bytes(16);
    let _ = updates.send(Update::PairProgress(
        "Step 2/5: exchanging challenge…".into(),
    ));
    let second = phase(
        &link,
        2,
        &format!(
            "devicename=roth&updateState=1&clientchallenge={}",
            hex::encode(aes_ecb(&challenge, &key, false)?)
        ),
        false,
        Duration::from_secs(12),
    )?;
    let data = aes_ecb(
        &hex::decode(xml(&second, "challengeresponse")).map_err(|e| e.to_string())?,
        &key,
        true,
    )?;
    if data.len() < 48 {
        return Err("Short Sunshine challenge".into());
    }
    let expected_hash = &data[..32];
    let secret = rand_bytes(16);
    let mut response = data[32..48].to_vec();
    response.extend_from_slice(&cert_signature(&link.client_cert_pem)?);
    response.extend_from_slice(&secret);
    let _ = updates.send(Update::PairProgress(
        "Step 3/5: verifying host identity…".into(),
    ));
    let third = phase(
        &link,
        3,
        &format!(
            "devicename=roth&updateState=1&serverchallengeresp={}",
            hex::encode(aes_ecb(&Sha256::digest(&response), &key, false)?)
        ),
        false,
        Duration::from_secs(12),
    )?;
    let pairing_secret = hex::decode(xml(&third, "pairingsecret")).map_err(|e| e.to_string())?;
    if pairing_secret.len() <= 16 {
        return Err("Bad Sunshine pairing secret".into());
    }
    verify_secret(
        &link.server_cert_pem,
        &pairing_secret[..16],
        &pairing_secret[16..],
    )?;
    let mut hash_input = challenge;
    hash_input.extend_from_slice(&cert_signature(&link.server_cert_pem)?);
    hash_input.extend_from_slice(&pairing_secret[..16]);
    if Sha256::digest(&hash_input).as_slice() != expected_hash {
        return Err("Incorrect PIN or spoofed Sunshine host".into());
    }
    if cancel.load(Ordering::SeqCst) {
        return Err("Pairing cancelled".into());
    }
    let private = RsaPrivateKey::from_pkcs8_pem(&link.client_key_pem).map_err(|e| e.to_string())?;
    let mut signed = secret.clone();
    signed.extend_from_slice(&SigningKey::<Sha256>::new(private).sign(&secret).to_vec());
    let _ = updates.send(Update::PairProgress("Step 4/5: authorizing Luma…".into()));
    phase(
        &link,
        4,
        &format!(
            "devicename=roth&updateState=1&clientpairingsecret={}",
            hex::encode(signed)
        ),
        false,
        Duration::from_secs(12),
    )?;
    let _ = updates.send(Update::PairProgress(
        "Step 5/5: confirming secure connection…".into(),
    ));
    let challenge_result = phase(
        &link,
        5,
        "devicename=roth&updateState=1&phrase=pairchallenge",
        true,
        Duration::from_secs(15),
    );
    if let Err(challenge_error) = challenge_result {
        log_phase(
            5,
            "pairchallenge failed; trying authenticated HTTPS serverinfo",
        );
        match authenticated_serverinfo_paired(&link) {
            Ok(true) => log_phase(5, "already paired confirmed with pinned HTTPS serverinfo"),
            Ok(false) => {
                return Err(format!(
                    "{challenge_error} Apollo has not confirmed this device. Try again."
                ));
            }
            Err(error) => {
                log_phase(
                    5,
                    &format!(
                        "serverinfo fallback failed: {}",
                        error
                            .chars()
                            .filter(|c| !c.is_control())
                            .take(120)
                            .collect::<String>()
                    ),
                );
                return Err(format!(
                    "{challenge_error} Could not confirm Apollo pairing: {error}. Try again."
                ));
            }
        }
    }
    link.paired = true;
    Ok(link)
}
#[derive(Serialize, Deserialize)]
struct Cached {
    items: Vec<CachedGame>,
}
#[derive(Serialize, Deserialize)]
struct CachedGame {
    id: i64,
    title: String,
    hdr: bool,
    poster: Option<String>,
    #[serde(default)]
    order: usize,
}
fn cache_file(root: &Path, uuid: &str) -> PathBuf {
    root.join(format!("{}-games.json", hex::encode(uuid.as_bytes())))
}
fn load_cached(root: &Path, uuid: &str) -> (Vec<StackItem>, Vec<Game>) {
    let saved = fs::read(cache_file(root, uuid))
        .ok()
        .and_then(|data| serde_json::from_slice::<Cached>(&data).ok());
    let Some(saved) = saved else {
        return (vec![], vec![]);
    };
    // Caches written before 2.2.102 still hold Apollo service entries and pins.
    let mut games: Vec<Game> = Vec::new();
    for (order, g) in saved.items.into_iter().enumerate() {
        let title = g.title.trim().to_owned();
        if is_service_app(&title)
            || games
                .iter()
                .any(|seen| seen.title.eq_ignore_ascii_case(&title))
        {
            continue;
        }
        games.push(Game {
            id: g.id,
            title,
            hdr: g.hdr,
            poster: g.poster.map(|p| fitted_or_png(Path::new(&p))),
            order: if g.order > 0 { g.order } else { order },
        });
    }
    let items = games
        .iter()
        .map(|g| StackItem {
            id: g.id,
            title: g.title.clone(),
            poster: g.poster.clone(),
        })
        .collect();
    (items, games)
}
fn fetch(link: &MoonlightLink, root: &Path) -> Result<(Vec<StackItem>, Vec<Game>), String> {
    if !link.paired {
        return Err("Luma is not paired with Sunshine".into());
    }
    let raw = https(
        link,
        &format!("/applist?{}", query(&link.unique_id)),
        Duration::from_secs(10),
    )?;
    let mut games = parse_apps(&String::from_utf8_lossy(&raw));
    let _ = fs::create_dir_all(root);
    for g in &mut games {
        let path = root.join(format!(
            "{}-{}.png",
            hex::encode(link.host_uuid.as_bytes()),
            g.id
        ));
        if !path.exists() {
            let req = format!(
                "/appasset?{}&appid={}&AssetType=2&AssetIdx=0",
                query(&link.unique_id),
                g.id
            );
            if let Ok(data) = https(link, &req, Duration::from_secs(7))
                && data.starts_with(b"\x89PNG")
                && data.len() < 12_000_000
            {
                let _ = fs::write(&path, data);
            }
        }
        if path.exists() {
            g.poster = Some(fitted_or_png(&path));
        }
    }
    let cached = Cached {
        items: games
            .iter()
            .map(|g| CachedGame {
                id: g.id,
                title: g.title.clone(),
                hdr: g.hdr,
                poster: g.poster.clone(),
                order: g.order,
            })
            .collect(),
    };
    if let Ok(bytes) = serde_json::to_vec(&cached) {
        let path = cache_file(root, &link.host_uuid);
        let temp = path.with_extension("tmp");
        if fs::write(&temp, bytes).is_ok() {
            let _ = fs::rename(&temp, path);
        }
    }
    let items = games
        .iter()
        .map(|g| StackItem {
            id: g.id,
            title: g.title.clone(),
            poster: g.poster.clone(),
        })
        .collect();
    Ok((items, games))
}
pub fn wake(mac: &str) -> Result<(), String> {
    let bytes = mac
        .split([':', '-'])
        .map(|b| u8::from_str_radix(b, 16))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "PC MAC address not available".to_string())?;
    if bytes.len() != 6 {
        return Err("Invalid PC MAC address".into());
    }
    let mut packet = vec![255; 6];
    for _ in 0..16 {
        packet.extend_from_slice(&bytes);
    }
    let socket = UdpSocket::bind(("0.0.0.0", 0)).map_err(|e| e.to_string())?;
    socket.set_broadcast(true).map_err(|e| e.to_string())?;
    socket
        .send_to(&packet, ("255.255.255.255", 9))
        .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn launch_params(link: &MoonlightLink, id: i64) -> serde_json::Value {
    serde_json::json!({"host_uuid":link.host_uuid,"host_app_id":id})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serverinfo() {
        let h = parse_server(
            "<hostname>BEAST</hostname><uniqueid>X</uniqueid><HttpsPort>47984</HttpsPort><PairStatus>1</PairStatus><currentgame>8</currentgame>",
            "192.168.1.5",
        );
        assert_eq!(h.name, "BEAST");
        assert!(h.paired);
        assert_eq!(h.current_game, 8);
    }
    #[test]
    fn apollo_cover_becomes_a_small_two_by_three_jpeg() {
        let dir = std::env::temp_dir().join("luma-poster-fit-test");
        let _ = std::fs::create_dir_all(&dir);
        let png = dir.join("cover.png");
        image::RgbImage::new(600, 800).save(&png).unwrap();
        let _ = std::fs::remove_file(dir.join("cover-card.jpg"));
        let out = poster_fit(&png).unwrap();
        let fit = image::open(&out).unwrap();
        assert_eq!((fit.width(), fit.height()), (270, 405));
    }

    #[test]
    fn apollo_service_entries_and_pinned_duplicates_are_hidden() {
        let g = parse_apps(
            "<App><AppTitle>     007 First Light</AppTitle><ID>5</ID></App><App><AppTitle>  Resume</AppTitle><ID>6</ID></App><App><AppTitle>Remote Input</AppTitle><ID>7</ID></App><App><AppTitle>Desktop</AppTitle><ID>1</ID></App><App><AppTitle>007 First Light</AppTitle><ID>9</ID><IsHdrSupported>1</IsHdrSupported></App><App><AppTitle>Atomic Heart</AppTitle><ID>10</ID></App>",
        );
        let titles: Vec<_> = g.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, ["007 First Light", "Atomic Heart", "Desktop"]);
        assert!(g[0].hdr);
    }
    #[test]
    fn apps() {
        let g = parse_apps(
            "<App><AppTitle>Desktop</AppTitle><ID>1</ID></App><App><AppTitle>Game &amp; More</AppTitle><ID>8</ID><IsHdrSupported>1</IsHdrSupported></App>",
        );
        assert_eq!(g[0].title, "Game & More");
        assert!(g[0].hdr);
    }
    #[test]
    fn https_builds_tls_config_without_process_default_provider() {
        // ring + aws-lc-rs are both compiled in; ClientConfig::builder()
        // panicked here and silently killed the pairing worker on the TV.
        let mut link = identity().unwrap();
        link.address = "127.0.0.1".into();
        link.https_port = 9;
        let result = https_inner(&link, "/serverinfo", Duration::from_secs(2), true);
        assert!(result.is_err());
    }
    #[test]
    #[ignore]
    fn live_beast_tls_handshake() {
        let mut link = identity().unwrap();
        link.address = std::env::var("LUMA_BEAST").unwrap_or("192.168.1.140".into());
        link.https_port = 47984;
        let result = https_inner(
            &link,
            &format!("/serverinfo?{}", query(&link.unique_id)),
            Duration::from_secs(5),
            true,
        );
        match &result {
            Ok((body, cert)) => println!(
                "LIVE_TLS_OK body={} cert={}",
                body.len(),
                cert.as_ref().map(|c| c.len()).unwrap_or(0)
            ),
            Err(e) => println!("LIVE_TLS_ERR {e}"),
        }
    }
    #[test]
    fn https_content_length_finishes_without_connection_close() {
        // The final read must never be attempted: a real keep-alive server
        // leaves its TLS socket open indefinitely.
        struct KeepAlive {
            bytes: Vec<u8>,
            pos: usize,
        }
        impl Read for KeepAlive {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                assert!(
                    self.pos < self.bytes.len(),
                    "read past framed response into keep-alive"
                );
                let n = out.len().min(self.bytes.len() - self.pos);
                out[..n].copy_from_slice(&self.bytes[self.pos..self.pos + n]);
                self.pos += n;
                Ok(n)
            }
        }
        let response=b"HTTP/1.1 200 OK\r\nContent-Length: 16\r\nConnection: keep-alive\r\n\r\n<PairStatus>1</>";
        let mut source = KeepAlive {
            bytes: response.to_vec(),
            pos: 0,
        };
        assert_eq!(
            read_https_response(&mut source).unwrap(),
            b"<PairStatus>1</>"
        );
    }
    #[test]
    fn https_chunked_finishes_without_connection_close() {
        struct NoEof(std::io::Cursor<Vec<u8>>);
        impl Read for NoEof {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                if self.0.position() as usize == self.0.get_ref().len() {
                    panic!("chunked parser waited for EOF");
                }
                self.0.read(out)
            }
        }
        let body = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n<ok>\r\n0\r\n\r\n";
        let mut source = NoEof(std::io::Cursor::new(body.to_vec()));
        assert_eq!(read_https_response(&mut source).unwrap(), b"<ok>");
    }
    #[test]
    fn https_rejects_truncated_body() {
        let mut body =
            std::io::Cursor::new(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nshort".to_vec());
        assert!(read_https_response(&mut body).is_err());
    }
    #[test]
    fn aes_pin() {
        let salt = [0; 16];
        let hash = Sha256::digest([salt.as_slice(), b"1234"].concat());
        assert_eq!(&aes_key(&salt, "1234"), &hash[..16]);
        // Independent SHA256 vector, 16 zero bytes followed by ASCII "1234".
        assert_eq!(
            hex::encode(aes_key(&salt, "1234")),
            "e4ae0c82639990744974cc3495a82432"
        );
        assert_eq!(aes_ecb(&[0; 16], &[0; 16], false).unwrap().len(), 16);
    }
    #[test]
    fn rsa_identity() {
        let l = identity().expect("pure Rust RSA/X.509 generation");
        assert_eq!(l.unique_id.len(), 16);
        assert!(!cert_signature(&l.client_cert_pem).unwrap().is_empty());
        let private = RsaPrivateKey::from_pkcs8_pem(&l.client_key_pem).unwrap();
        let msg = b"Luma test challenge";
        let signed = SigningKey::<Sha256>::new(private).sign(msg).to_vec();
        verify_secret(&l.client_cert_pem, msg, &signed).unwrap();
    }
    #[test]
    fn game_stacks_pick_their_category() {
        let games: Vec<Game> = ["Alpha", "Beta", "Gamma", "Desktop"]
            .iter()
            .enumerate()
            .map(|(i, t)| Game {
                id: i as i64 + 1,
                title: (*t).into(),
                hdr: false,
                poster: None,
                order: i,
            })
            .collect();
        let items: Vec<StackItem> = games
            .iter()
            .map(|g| StackItem {
                id: g.id,
                title: g.title.clone(),
                poster: None,
            })
            .collect();
        let mut lib = GameLibrary::default();
        assert!(lib.note_seen(&games));
        assert!(!lib.note_seen(&games));
        lib.record_play(2);
        lib.record_play(1);
        lib.record_play(2);
        lib.toggle_favorite(3);
        let ids = |lib: &GameLibrary, c: &str| {
            lib.select(c, &items, &games)
                .iter()
                .map(|i| i.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&lib, "recent"), [2, 1]);
        assert_eq!(ids(&lib, "favorites"), [3]);
        // Same first-seen time: later in Apollo's list = added later; launchers excluded.
        assert_eq!(ids(&lib, "added"), [3, 2, 1]);
        assert_eq!(ids(&lib, "all").len(), 4);
        lib.toggle_favorite(3);
        assert!(ids(&lib, "favorites").is_empty());
    }
    #[test]
    fn backwards_prefs_and_stack_options() {
        let prefs: crate::model::Preferences = serde_json::from_str("{}").unwrap();
        assert!(!prefs.moonlight.paired);
        // Category, Card shape, Show, Move up, Move down, Unpair, Remove.
        assert_eq!(crate::stacks::option_count("moonlight"), 7);
        assert_eq!(crate::stacks::option_count("moonlight"), 7);
        let l = MoonlightLink::default();
        assert_eq!(
            serde_json::from_str::<MoonlightLink>(&serde_json::to_string(&l).unwrap()).unwrap(),
            l
        );
    }
    #[test]
    fn pairing_screen_back_cancels() {
        use crate::state::{Effect, Page, STACKS_SECTION, SettingsLevel, State, Target};
        let mut state = State::new(crate::model::Preferences::default());
        state.page = Page::Settings;
        state.section = STACKS_SECTION;
        state.settings_level = SettingsLevel::MoonlightHosts;
        state.focus = Target::MoonlightHost(0);
        state.moonlight_hosts.push(Host {
            name: "BEAST".into(),
            address: "192.168.1.140".into(),
            online: true,
            ..Default::default()
        });
        let effects = state.activate();
        assert!(matches!(effects.as_slice(), [Effect::MoonlightPair(_)]));
        assert_eq!(state.settings_level, SettingsLevel::MoonlightPin);
        // Remote sends a second OK while panel animation is in progress.
        assert!(
            state.activate().is_empty(),
            "double OK must not cancel PIN or re-pair"
        );
        assert_eq!(state.settings_level, SettingsLevel::MoonlightPin);
        assert_eq!(
            state.moonlight_status.as_deref(),
            Some("Waiting for Sunshine PIN approval…")
        );
        let effects = state.back();
        assert!(matches!(effects.as_slice(), [Effect::MoonlightCancel]));
        assert_eq!(state.settings_level, SettingsLevel::MoonlightHosts);
    }
    #[test]
    fn sunshine_xml_http_status() {
        assert_eq!(
            xml_attr(
                r#"<root status_code="200" status_message="OK">"#,
                "status_code"
            ),
            "200"
        );
        assert_eq!(
            xml_attr(
                r#"<root status_code="409" status_message="Session busy">"#,
                "status_message"
            ),
            "Session busy"
        );
    }
    #[test]
    fn lan_candidates_exclude_local_and_broadcast() {
        let all = local_subnet_candidates();
        assert!(all.len() <= 253);
        assert!(!all.iter().any(|s| s.ends_with(".0") || s.ends_with(".255")));
        for ip in &all {
            assert!(ip.parse::<Ipv4Addr>().unwrap().is_private());
        }
    }
    #[test]
    fn ui_discovery_timeout() {
        use crate::state::{Page, SettingsLevel, State};
        let mut state = State::new(crate::model::Preferences::default());
        state.page = Page::Settings;
        state.settings_level = SettingsLevel::MoonlightHosts;
        state.moonlight_status = Some("Searching for PCs…".into());
        state.moonlight_discovery_start = Some(Instant::now() - Duration::from_secs(9));
        state.tick();
        assert!(state.moonlight_status.unwrap().starts_with("No PCs found"));
    }
    #[test]
    fn duplicate_pair_same_host_keeps_original_request() {
        let mut active: Option<String> = None;
        assert!(claim_pair_session(&mut active, "BEAST"));
        assert!(!claim_pair_session(&mut active, "BEAST"));
        assert_eq!(active.as_deref(), Some("BEAST"));
        // Second OK cannot cancel the original request. Explicit retry only
        // becomes available after the active worker returns and clears state.
        active = None;
        assert!(claim_pair_session(&mut active, "BEAST"));
    }
    #[test]
    fn simultaneous_pair_events_start_one_http_worker() {
        use std::sync::atomic::AtomicUsize;
        let active = Arc::new(Mutex::new(None));
        let begins = Arc::new(AtomicUsize::new(0));
        let workers = (0..32)
            .map(|_| {
                let active = active.clone();
                let begins = begins.clone();
                thread::spawn(move || {
                    if claim_pair_session(&mut active.lock().unwrap(), "192.168.1.140") {
                        // The service starts HTTP only after this gate grants the host.
                        begins.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(begins.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn cancelling_pending_http_cannot_start_second_session() {
        let mut active: Option<String> = None;
        let flag = AtomicBool::new(false);
        assert!(claim_pair_session(&mut active, "192.168.1.140"));
        // Back arrives while HTTP GET /pair is waiting for Sunshine PIN.
        flag.store(true, Ordering::SeqCst);
        // The worker still holds the guard until the GET returns.
        assert!(!claim_pair_session(&mut active, "192.168.1.140"));
        assert_eq!(active.as_deref(), Some("192.168.1.140"));
        // Only actual completion can release it for a new pairing attempt.
        active = None;
        assert!(claim_pair_session(&mut active, "192.168.1.140"));
    }
    #[test]
    fn live_existing_apollo_pair_without_pin() {
        let Ok(path) = std::env::var("LUMA_TEST_EXISTING_IDENTITY") else {
            return;
        };
        let data = std::fs::read(path).unwrap();
        let link: MoonlightLink = serde_json::from_slice(&data).unwrap();
        let host = probe(&link.address).unwrap();
        assert!(recover_existing_pair(&host, &link).unwrap().is_some());
        println!("EXISTING_PAIR_RECOVERY_PASS host={} pinned=true", host.name);
    }
    #[test]
    fn live_pair_beast() {
        let Ok(ip) = std::env::var("LUMA_TEST_LIVE_PAIR_IP") else {
            return;
        };
        let host = probe(&ip).expect("BEAST Sunshine");
        let (updates, receiver) = mpsc::channel();
        let pin = format!("{:04}", OsRng.next_u32() % 10000);
        println!("LIVE_SUNSHINE_PIN={pin} HOST={} START", host.name);
        std::io::stdout().flush().unwrap();
        let link = pair(
            host,
            MoonlightLink::default(),
            &pin,
            &AtomicBool::new(false),
            &updates,
        );
        for msg in receiver.try_iter() {
            if let Update::PairProgress(x) = msg {
                println!("PAIR_PROGRESS {x}");
            }
        }
        match link {
            Ok(l) => {
                println!("LIVE_PAIR_SUCCESS host={} paired={}", l.host_name, l.paired);
                assert!(l.paired);
            }
            Err(e) => panic!("LIVE_PAIR_ERROR {e}"),
        }
    }
    #[test]
    fn params() {
        let l = MoonlightLink {
            host_uuid: "abc".into(),
            ..Default::default()
        };
        assert_eq!(
            launch_params(&l, 55),
            serde_json::json!({"host_uuid":"abc","host_app_id":55})
        );
    }
}
