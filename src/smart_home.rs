//! Home Assistant REST bridge. Network work never runs on the SDL/GL thread.
//! Do not log access tokens or persist them with UI preferences.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Entity {
    pub id: String,
    pub name: String,
    pub state: String,
    pub domain: String,
    pub area: String,
    pub brightness: Option<u8>,
    pub unit: String,
    /// Diagnostic/config entity, or hidden/disabled in Home Assistant itself.
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub device_class: String,
}
/// The main stream id when `id` is a camera's secondary (low quality) stream,
/// e.g. `camera.garden_sd_stream` -> `camera.garden_hd_stream`.
pub fn stream_twin(id: &str) -> Option<String> {
    const PAIRS: [(&str, &str); 6] = [
        ("_sd_stream", "_hd_stream"),
        ("_sub_stream", "_main_stream"),
        ("_substream", "_mainstream"),
        ("_sub", "_main"),
        ("_fluent", "_clear"),
        ("_low", "_high"),
    ];
    PAIRS
        .iter()
        .find_map(|(low, high)| id.strip_suffix(low).map(|base| format!("{base}{high}")))
}

impl Entity {
    /// Shown on the Smart Home tab until the user picks entities themselves:
    /// controls, cameras, scenes and room climate, never maintenance entities.
    pub fn useful(&self) -> bool {
        if self.hidden {
            return false;
        }
        // An offline camera keeps its tile and says so; other dead entities go.
        if self.domain != "camera" && matches!(self.state.as_str(), "unavailable" | "unknown") {
            return false;
        }
        // The REST fallback has no registry categories; drop obvious maintenance names.
        let name = format!("{} {}", self.id, self.name).to_lowercase();
        if [
            "battery",
            "indicator",
            "config",
            "firmware",
            "signal",
            "rssi",
            "linkquality",
            "uptime",
            "voltage",
            "update",
            "restart",
            "identify",
        ]
        .iter()
        .any(|word| name.contains(word))
        {
            return false;
        }
        match self.domain.as_str() {
            "camera" => stream_twin(&self.id).is_none(),
            "light" | "switch" | "fan" | "cover" | "lock" | "climate" | "scene" | "script" => true,
            "sensor" => {
                matches!(self.device_class.as_str(), "temperature" | "humidity")
                    || matches!(self.unit.as_str(), "°C" | "°F")
            }
            "binary_sensor" => matches!(
                self.device_class.as_str(),
                "door" | "window" | "garage_door" | "opening" | "motion" | "occupancy"
            ),
            _ => false,
        }
    }
    pub fn actionable(&self) -> bool {
        matches!(self.domain.as_str(), "light" | "switch" | "fan" | "cover")
    }
    pub fn on(&self) -> bool {
        matches!(self.state.as_str(), "on" | "open" | "opening" | "unlocked")
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    BeginPairing {
        server: String,
    },
    EndPairing,
    CredentialsReady {
        server: String,
    },
    Disconnect,
    View {
        visible: bool,
        fullscreen: Option<String>,
        /// The cameras the tab shows, in order.
        cameras: Vec<String>,
        /// Cameras whose tile snapshots are blurred before they are saved.
        hidden: Vec<String>,
    },
    Call {
        entity_id: String,
        service: String,
        data: Value,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct DiscoveredServer {
    pub name: String,
    pub url: String,
    pub version: String,
}
fn discovered_server(
    name: &str,
    preferred: Option<&str>,
    fallback_ip: &str,
    port: u16,
    version: &str,
) -> Option<DiscoveredServer> {
    let url = preferred
        .and_then(normalize_url)
        .or_else(|| normalize_url(&format!("http://{fallback_ip}:{port}")))?;
    Some(DiscoveredServer {
        name: name.into(),
        url,
        version: version.into(),
    })
}

#[derive(Clone, Debug)]
pub enum Update {
    States(Vec<Entity>),
    Offline,
    Connected,
    Discovered(DiscoveredServer),
    Snapshot {
        id: String,
        path: String,
    },
    /// Home Assistant returned no picture for this camera.
    SnapshotFailed {
        id: String,
    },
    Error(String),
    SignedOut,
    QrReady {
        authorize: Option<String>,
        fallback: String,
    },
    QrExpired,
    PairingError(String),
    Authorized {
        server: String,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Secrets {
    pub(crate) access_token: String,
    #[serde(default)]
    pub(crate) refresh_token: Option<String>,
    #[serde(default)]
    pub(crate) client_id: Option<String>,
    #[serde(default)]
    pub(crate) expires_at: u64,
}
pub struct Service {
    sender: Sender<Command>,
    receiver: Receiver<Update>,
}
impl Service {
    pub fn new(path: PathBuf, server: String) -> Self {
        let (sender, commands) = mpsc::channel();
        let pair_commands = sender.clone();
        let (updates, receiver) = mpsc::channel();
        let discovery_updates = updates.clone();
        thread::spawn(move || {
            use mdns_sd::{ServiceDaemon, ServiceEvent};
            let Ok(discovery) = ServiceDaemon::new() else {
                return;
            };
            let Ok(events) = discovery.browse("_home-assistant._tcp.local.") else {
                return;
            };
            while let Ok(event) = events.recv() {
                if let ServiceEvent::ServiceResolved(info) = event {
                    let props = info.get_properties();
                    let name = info
                        .get_property_val_str("location_name")
                        .unwrap_or(info.get_fullname());
                    let preferred = info
                        .get_property_val_str("internal_url")
                        .or_else(|| info.get_property_val_str("base_url"));
                    let ip = info
                        .get_addresses_v4()
                        .iter()
                        .next()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| info.get_hostname().trim_end_matches('.').into());
                    let version = info.get_property_val_str("version").unwrap_or("");
                    if let Some(server) =
                        discovered_server(name, preferred, &ip, info.get_port(), version)
                    {
                        let _ = discovery_updates.send(Update::Discovered(server));
                    }
                    let _ = props;
                }
            }
        });
        thread::spawn(move || {
            let mut secrets = read_auth(&path);
            let mut url = normalize_url(&server).unwrap_or_default();
            let mut retry = Duration::from_secs(1);
            let mut next = Instant::now();
            let mut next_snapshot = Instant::now() + Duration::from_secs(8);
            let mut visible = false;
            let mut fullscreen: Option<String> = None;
            let mut known_cameras: Vec<String> = Vec::new();
            let mut shown_cameras: Vec<String> = Vec::new();
            let mut hidden_cameras: Vec<String> = Vec::new();
            let mut refresh_at = Instant::now();
            let mut refresh_retry = Duration::from_secs(2);
            let mut snapshot_count = 0u64;
            let mut websocket = crate::smart_home_ws::LiveService::new();
            if let Some(secret) = &secrets
                && !url.is_empty()
            {
                websocket.connect(url.clone(), secret.access_token.clone(), updates.clone());
            }
            let mut pairing_canceled: Option<std::sync::Arc<std::sync::atomic::AtomicBool>> = None;
            loop {
                let wait = if visible && secrets.is_some() {
                    next.min(next_snapshot)
                        .saturating_duration_since(Instant::now())
                } else {
                    next.saturating_duration_since(Instant::now())
                };
                match commands.recv_timeout(wait) {
                    Ok(Command::BeginPairing { server }) => {
                        if let Some(old) = pairing_canceled.take() {
                            old.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        let canceled =
                            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                        crate::smart_home_pairing::start(
                            server,
                            path.clone(),
                            updates.clone(),
                            pair_commands.clone(),
                            canceled.clone(),
                        );
                        pairing_canceled = Some(canceled);
                    }
                    Ok(Command::EndPairing) => {
                        if let Some(old) = pairing_canceled.take() {
                            old.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    Ok(Command::CredentialsReady { server }) => {
                        if let Some(old) = pairing_canceled.take() {
                            old.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        url = server.clone();
                        secrets = read_auth(&path);
                        if let Some(secret) = &secrets {
                            websocket.connect(
                                url.clone(),
                                secret.access_token.clone(),
                                updates.clone(),
                            );
                        }
                        next = Instant::now();
                        let _ = updates.send(Update::Authorized { server });
                    }
                    Ok(Command::View {
                        visible: show,
                        fullscreen: camera,
                        cameras,
                        hidden,
                    }) => {
                        visible = show;
                        fullscreen = camera;
                        shown_cameras = cameras;
                        hidden_cameras = hidden;
                        log_view(visible, &shown_cameras);
                        next_snapshot = Instant::now();
                    }
                    Ok(Command::Disconnect) => {
                        // Long-lived access tokens are not revocable using /auth/revoke;
                        // delete local credentials rather than falsely claiming server revocation.
                        if let Some(old) = pairing_canceled.take() {
                            old.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        if let Some(ref old) = secrets
                            && let Some(ref refresh) = old.refresh_token
                        {
                            let form = format!(
                                "token={}",
                                url::form_urlencoded::byte_serialize(refresh.as_bytes())
                                    .collect::<String>()
                            );
                            let _ = ureq::post(&format!("{url}/auth/revoke"))
                                .header("Content-Type", "application/x-www-form-urlencoded")
                                .send(form.as_bytes());
                        }
                        websocket.stop();
                        secrets = None;
                        let _ = std::fs::remove_file(&path);
                        let _ = updates.send(Update::SignedOut);
                        next = Instant::now() + Duration::from_secs(3600);
                    }
                    Ok(Command::Call {
                        entity_id,
                        service,
                        data,
                    }) => {
                        if let Some(secret) = &secrets
                            && let Some((domain, _)) = entity_id.split_once('.')
                        {
                            if !matches!(
                                domain,
                                "light" | "switch" | "fan" | "cover" | "scene" | "script"
                            ) {
                                continue;
                            }
                            let allowed = match domain {
                                "scene" | "script" => service == "turn_on",
                                "cover" => {
                                    matches!(service.as_str(), "open_cover" | "close_cover")
                                }
                                _ => matches!(service.as_str(), "turn_on" | "turn_off"),
                            };
                            if !allowed {
                                continue;
                            }
                            let body = if data.is_object() {
                                data
                            } else {
                                json!({"entity_id":entity_id})
                            };
                            if websocket.call(entity_id.clone(), service.clone(), body.clone()) {
                                continue;
                            }
                            let endpoint = format!("{url}/api/services/{domain}/{service}");
                            let result = ureq::post(&endpoint)
                                .header("Authorization", &format!("Bearer {}", secret.access_token))
                                .header("Content-Type", "application/json")
                                .send(body.to_string().as_bytes());
                            if result.is_err() {
                                let _ = updates.send(Update::Error(
                                    "Action failed. Check Home Assistant.".into(),
                                ));
                            }
                            next = Instant::now();
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if let Some(secret) = &mut secrets {
                            if secret.refresh_token.is_some()
                                && secret.expires_at > 0
                                && unix_time().saturating_add(90) >= secret.expires_at
                                && Instant::now() >= refresh_at
                            {
                                match refresh_auth(&url, secret) {
                                    Ok(new) => {
                                        refresh_retry = Duration::from_secs(2);
                                        if save_auth(&path, &new).is_ok() {
                                            websocket.connect(
                                                url.clone(),
                                                new.access_token.clone(),
                                                updates.clone(),
                                            );
                                            *secret = new;
                                            next = Instant::now();
                                        }
                                    }
                                    Err(RefreshError::Unreachable) => {
                                        // After TV wake the network comes up late; keep trying.
                                        let _ = updates.send(Update::Offline);
                                        refresh_at = Instant::now() + refresh_retry;
                                        refresh_retry =
                                            (refresh_retry * 2).min(Duration::from_secs(60));
                                        continue;
                                    }
                                    Err(RefreshError::Rejected) => {
                                        refresh_at = Instant::now() + Duration::from_secs(300);
                                        let _ = updates.send(Update::Error(
                                            "Home Assistant session expired. Sign in again.".into(),
                                        ));
                                    }
                                }
                            }
                            if url.is_empty() {
                                next = Instant::now() + Duration::from_secs(30);
                                continue;
                            }
                            if Instant::now() >= next && websocket.connected() {
                                next = Instant::now() + Duration::from_secs(30);
                            }
                            if Instant::now() >= next {
                                match fetch_states(&url, &secret.access_token) {
                                    Ok(entities) => {
                                        known_cameras = entities
                                            .iter()
                                            .filter(|e| e.domain == "camera")
                                            .map(|e| e.id.clone())
                                            .take(2)
                                            .collect();
                                        retry = Duration::from_secs(1);
                                        if !websocket.connected() {
                                            // Server is back (e.g. after a Home Assistant
                                            // restart): don't wait out the socket's backoff.
                                            websocket.connect(
                                                url.clone(),
                                                secret.access_token.clone(),
                                                updates.clone(),
                                            );
                                        }
                                        let _ = updates.send(Update::Connected);
                                        let _ = updates.send(Update::States(entities));
                                        next = Instant::now() + Duration::from_secs(7);
                                    }
                                    Err(()) => {
                                        let _ = updates.send(Update::Offline);
                                        next = Instant::now() + retry;
                                        retry = (retry * 2).min(Duration::from_secs(10));
                                    }
                                }
                            }
                            if visible && Instant::now() >= next_snapshot {
                                let ids = fullscreen
                                    .as_ref()
                                    .map(|id| vec![id.clone()])
                                    .unwrap_or_else(|| {
                                        if shown_cameras.is_empty() {
                                            known_cameras.clone()
                                        } else {
                                            shown_cameras.iter().take(4).cloned().collect()
                                        }
                                    });
                                for id in ids {
                                    if let Some(path) = fetch_snapshot(
                                        &url,
                                        &secret.access_token,
                                        &id,
                                        &path,
                                        snapshot_count,
                                        hidden_cameras.contains(&id)
                                            && fullscreen.as_ref() != Some(&id),
                                    ) {
                                        let _ = updates.send(Update::Snapshot { id, path });
                                    } else {
                                        let _ = updates.send(Update::SnapshotFailed { id });
                                    }
                                }
                                snapshot_count += 1;
                                next_snapshot = Instant::now()
                                    + Duration::from_millis(if fullscreen.is_some() {
                                        900
                                    } else {
                                        8000
                                    });
                            }
                        } else {
                            next = Instant::now() + Duration::from_secs(30);
                        }
                    }
                }
            }
        });
        Self { sender, receiver }
    }
    pub fn send(&self, action: Command) {
        let _ = self.sender.send(action);
    }
    pub fn try_recv(&self) -> Option<Update> {
        self.receiver.try_recv().ok()
    }
}
pub fn normalize_url(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches('/');
    if value.starts_with("http://") || value.starts_with("https://") {
        if value.contains('@') || value.contains('#') || value.chars().any(char::is_whitespace) {
            return None;
        }
        let host = value.split("://").nth(1)?;
        if host.is_empty() || host.starts_with('/') {
            return None;
        }
        Some(value.to_owned())
    } else {
        None
    }
}
fn fetch_states(server: &str, token: &str) -> Result<Vec<Entity>, ()> {
    let response = ureq::get(&format!("{server}/api/states"))
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|_| ())?;
    let mut body = response.into_body();
    let source = body.read_to_string().map_err(|_| ())?;
    parse_states(&source).map_err(|_| ())
}
pub fn parse_states(source: &str) -> Result<Vec<Entity>, serde_json::Error> {
    let items: Vec<Value> = serde_json::from_str(source)?;
    Ok(items
        .iter()
        .filter_map(|v| {
            let id = v.get("entity_id")?.as_str()?;
            let (domain, _) = id.split_once('.')?;
            if !matches!(
                domain,
                "light"
                    | "switch"
                    | "fan"
                    | "cover"
                    | "lock"
                    | "climate"
                    | "sensor"
                    | "binary_sensor"
                    | "camera"
                    | "scene"
                    | "script"
                    | "alarm_control_panel"
            ) {
                return None;
            }
            let attrs = v.get("attributes");
            let name = attrs
                .and_then(|a| a.get("friendly_name"))
                .and_then(Value::as_str)
                .unwrap_or(id)
                .to_owned();
            let unit = attrs
                .and_then(|a| a.get("unit_of_measurement"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let device_class = attrs
                .and_then(|a| a.get("device_class"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let brightness = attrs
                .and_then(|a| a.get("brightness"))
                .and_then(Value::as_u64)
                .map(|n| ((n.min(255) * 100) / 255) as u8);
            Some(Entity {
                id: id.into(),
                domain: domain.into(),
                name,
                area: String::new(),
                state: v
                    .get("state")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .into(),
                brightness,
                unit,
                hidden: false,
                device_class,
            })
        })
        .take(2000)
        .collect())
}
fn fetch_snapshot(
    server: &str,
    token: &str,
    id: &str,
    auth_path: &std::path::Path,
    n: u64,
    blur: bool,
) -> Option<String> {
    if !id.starts_with("camera.")
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
    {
        return None;
    }
    let data = ureq::get(&format!("{server}/api/camera_proxy/{id}"))
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .ok()?
        .into_body()
        .read_to_vec()
        .ok()?;
    if data.len() > 4_000_000 || !data.starts_with(&[0xff, 0xd8, 0xff]) {
        return None;
    }
    // A hidden camera's tile picture never reaches the disk sharp.
    let (data, kind) = if blur {
        (blur_snapshot(&data)?, "blur-")
    } else {
        (data, "")
    };
    let folder = auth_path.parent()?.join("ha-camera-cache");
    std::fs::create_dir_all(&folder).ok()?;
    let file = folder.join(format!("{id}-{kind}{n}.jpg"));
    std::fs::write(&file, data).ok()?;
    if n >= 4 {
        for old in ["", "blur-"] {
            let _ = std::fs::remove_file(folder.join(format!("{id}-{old}{}.jpg", n - 4)));
        }
    }
    Some(file.to_string_lossy().into_owned())
}
/// Shrinks and blurs a snapshot until nothing in it can be made out.
pub(crate) fn blur_snapshot(data: &[u8]) -> Option<Vec<u8>> {
    use image::imageops::FilterType;
    let picture = image::load_from_memory_with_format(data, image::ImageFormat::Jpeg).ok()?;
    let small = picture.resize_exact(48, 27, FilterType::Triangle).blur(1.5);
    let soft = small.resize_exact(480, 270, FilterType::Triangle).blur(8.);
    let mut out = Vec::new();
    soft.to_rgb8()
        .write_to(
            &mut std::io::Cursor::new(&mut out),
            image::ImageFormat::Jpeg,
        )
        .ok()?;
    Some(out)
}
/// One line per tab view change, so a missing camera can be traced on the TV.
fn log_view(visible: bool, cameras: &[String]) {
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/luma-ha-view.log")
    {
        let _ = writeln!(file, "visible={visible} cameras={}", cameras.join(","));
    }
}
pub fn auth_path(preferences: &std::path::Path) -> PathBuf {
    preferences.with_file_name("ha-auth.json")
}
fn read_auth(path: &std::path::Path) -> Option<Secrets> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).ok()?;
        }
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}
pub(crate) fn save_auth(path: &std::path::Path, secrets: &Secrets) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
        let parent = path.parent().ok_or("No credentials directory")?;
        if !parent.exists() {
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(parent)
                .map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("json.tmp");
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        f.write_all(&serde_json::to_vec(secrets).map_err(|e| e.to_string())?)
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (path, secrets);
        Err("Credential storage unavailable".into())
    }
}

pub(crate) fn unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
enum RefreshError {
    /// Home Assistant answered and refused the refresh token.
    Rejected,
    /// No answer (network not up yet, server restarting): try again later.
    Unreachable,
}
fn refresh_auth(server: &str, old: &Secrets) -> Result<Secrets, RefreshError> {
    let (Some(refresh), Some(client_id)) = (old.refresh_token.as_ref(), old.client_id.as_ref())
    else {
        return Err(RefreshError::Rejected);
    };
    let form = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "refresh_token")
        .append_pair("refresh_token", refresh)
        .append_pair("client_id", client_id)
        .finish();
    let response = match ureq::post(&format!("{server}/auth/token"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send(form.as_bytes())
    {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(400..=403)) => return Err(RefreshError::Rejected),
        Err(_) => return Err(RefreshError::Unreachable),
    };
    let text = response
        .into_body()
        .read_to_string()
        .map_err(|_| RefreshError::Unreachable)?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| RefreshError::Unreachable)?;
    let access_token = value
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or(RefreshError::Rejected)?;
    Ok(Secrets {
        access_token: access_token.into(),
        refresh_token: Some(
            value
                .get("refresh_token")
                .and_then(Value::as_str)
                .unwrap_or(refresh)
                .into(),
        ),
        client_id: old.client_id.clone(),
        expires_at: unix_time().saturating_add(
            value
                .get("expires_in")
                .and_then(Value::as_u64)
                .unwrap_or(1800),
        ),
    })
}
pub fn qr_matrix(content: &str) -> Vec<Vec<bool>> {
    crate::phone_link::qr_matrix(content)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_entities_and_actionability() {
        let entities = parse_states(r#"[{"entity_id":"light.office","state":"on","attributes":{"friendly_name":"Office","brightness":128}},{"entity_id":"sensor.temp","state":"23","attributes":{"unit_of_measurement":"°C"}},{"entity_id":"automation.fake","state":"on"}]"#).unwrap();
        assert_eq!(entities.len(), 2);
        assert_eq!(entities[0].brightness, Some(50));
        assert!(entities[0].actionable());
        assert!(!entities[1].actionable());
    }
    #[test]
    fn discovery_prefers_internal_url() {
        let server = discovered_server(
            "Living room",
            Some("http://192.168.1.80:8123/"),
            "192.168.1.45",
            8123,
            "2026.10",
        )
        .unwrap();
        assert_eq!(server.url, "http://192.168.1.80:8123");
        assert_eq!(server.name, "Living room");
        assert_eq!(server.version, "2026.10");
    }
    #[test]
    fn url_validation() {
        assert_eq!(
            normalize_url(" http://homeassistant.local:8123/ ").as_deref(),
            Some("http://homeassistant.local:8123")
        );
        assert!(normalize_url("http://user:password@host/").is_none());
        assert!(normalize_url("file:///etc/passwd").is_none());
    }
    #[cfg(unix)]
    #[test]
    fn sign_in_survives_loosened_file_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("luma-auth-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ha-auth.json");
        std::fs::write(
            &path,
            r#"{"access_token":"a","refresh_token":"r","client_id":"c","expires_at":1}"#,
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(read_auth(&path).is_some());
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
