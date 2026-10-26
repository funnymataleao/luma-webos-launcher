//! Non-blocking native cinema controller. All I/O and codec calls run off UI thread.
//! On the TV playback goes through com.webos.media (uMediaServer) over LS2 so we
//! get real currentTime/duration/video-size events; the C facade is a fallback.
use crate::player::{Command, Cue, OpenRequest, parse_subtitle};
use std::{
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub status: i32,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub first_frame_ms: Option<u64>,
    pub error: Option<String>,
    pub cues: Option<Vec<Cue>>,
}
struct Service {
    tx: mpsc::Sender<Command>,
    rx: Mutex<mpsc::Receiver<Snapshot>>,
}
static SERVICE: OnceLock<Service> = OnceLock::new();
/// Set by the render loop once the NDL wallpaper has released the main scaler.
static PLANE_FREE: AtomicBool = AtomicBool::new(false);

fn service() -> &'static Service {
    SERVICE.get_or_init(|| {
        let (tx, commands) = mpsc::channel();
        let (updates, rx) = mpsc::channel();
        std::thread::spawn(move || worker(commands, updates));
        Service {
            tx,
            rx: Mutex::new(rx),
        }
    })
}
pub fn send(command: Command) {
    let _ = service().tx.send(command);
}
pub fn poll() -> Option<Snapshot> {
    SERVICE.get()?.rx.lock().ok()?.try_recv().ok()
}
pub fn set_video_plane_free(free: bool) {
    PLANE_FREE.store(free, Ordering::Relaxed);
}

fn snapshot(backend: &Backend, error: Option<String>, cues: Option<Vec<Cue>>) -> Snapshot {
    Snapshot {
        status: backend.status(),
        position_ms: backend.position(),
        duration_ms: backend.duration(),
        first_frame_ms: backend.first_frame_ms(),
        error,
        cues,
    }
}

const SEEK_SETTLE: Duration = Duration::from_millis(450);

/// `(width, height)` of the first variant an HLS master playlist lists.
#[cfg(any(test, all(feature = "webos", target_os = "linux", target_arch = "arm")))]
fn playlist_resolution(text: &str) -> Option<(i32, i32)> {
    text.lines()
        .filter(|l| l.starts_with("#EXT-X-STREAM-INF"))
        .find_map(|l| {
            let (w, h) = l
                .split("RESOLUTION=")
                .nth(1)?
                .split(',')
                .next()?
                .split_once('x')?;
            let (w, h) = (w.trim().parse::<i32>().ok()?, h.trim().parse::<i32>().ok()?);
            (w > 0 && h > 0).then_some((w, h))
        })
}

/// Frame size named by a stream's playlist, so the video keeps its aspect.
#[cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
fn stream_resolution(url: &str) -> Option<(i32, i32)> {
    if !url.starts_with("http") || !url.contains(".m3u8") {
        return None;
    }
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(4)))
        .build()
        .new_agent();
    let text = agent
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (Web0S; Linux) Luma")
        .call()
        .ok()?
        .body_mut()
        .with_config()
        .limit(200_000)
        .read_to_string()
        .ok()?;
    playlist_resolution(&text)
}

fn worker(commands: mpsc::Receiver<Command>, updates: mpsc::Sender<Snapshot>) {
    let mut backend = Backend::new();
    let mut current: Option<OpenRequest> = None;
    let mut resume_pending: Option<u64> = None;
    let mut initial_subtitle_pending: Option<usize> = None;
    // Held D-pad or wheel steps arrive as a burst; only the last one is sent,
    // since every uMediaServer seek restarts buffering from scratch.
    let mut seek_pending: Option<(u64, Instant)> = None;
    let mut last = Instant::now();
    loop {
        match commands.recv_timeout(Duration::from_millis(40)) {
            Ok(Command::Open(req)) => {
                // The wallpaper decoder must let go of the main scaler first,
                // otherwise uMediaServer routes video to the (hidden) sub scaler.
                let wait = Instant::now();
                while !PLANE_FREE.load(Ordering::Relaxed)
                    && wait.elapsed() < Duration::from_millis(1500)
                {
                    std::thread::sleep(Duration::from_millis(20));
                }
                let result = backend.open(&req.url);
                resume_pending = (req.resume_ms > 0).then_some(req.resume_ms);
                initial_subtitle_pending = req.selected_subtitle;
                current = Some(*req);
                let mut s = snapshot(&backend, result.err(), None);
                if s.error.is_some() {
                    s.status = -1;
                }
                let _ = updates.send(s);
            }
            Ok(Command::Close) => {
                backend.close();
                current = None;
                resume_pending = None;
                let _ = updates.send(Snapshot {
                    status: 0,
                    position_ms: 0,
                    duration_ms: 0,
                    first_frame_ms: None,
                    error: None,
                    cues: None,
                });
            }
            Ok(Command::Pause) => backend.pause(),
            Ok(Command::Play) => backend.play(),
            Ok(Command::Seek(ms)) => seek_pending = Some((ms, Instant::now())),
            Ok(Command::Audio(n)) => backend.select_track("audio", n as i64),
            Ok(Command::Subtitle(choice)) => {
                let mut cues = vec![];
                if let Some(url) =
                    choice.and_then(|i| current.as_ref()?.subtitles.get(i)?.url.as_ref())
                {
                    cues = download_cues(url);
                } else {
                    backend.select_track("text", choice.map_or(-1, |i| i as i64));
                }
                let _ = updates.send(snapshot(&backend, None, Some(cues)));
            }
            Ok(Command::Episode(..)) => {}
            Ok(Command::Quality(n)) => {
                if let Some(url) = current
                    .as_ref()
                    .and_then(|r| r.qualities.get(n))
                    .and_then(|v| v.url.clone())
                {
                    let pos = backend.position();
                    if backend.open(&url).is_ok() && pos > 0 {
                        resume_pending = Some(pos);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if let Some((ms, at)) = seek_pending
            && at.elapsed() >= SEEK_SETTLE
        {
            seek_pending = None;
            backend.seek(ms);
        }
        backend.tick();
        if backend.status() == 2 && current.is_some() {
            if let Some(ms) = resume_pending.take() {
                backend.seek(ms);
            }
            if let Some(index) = initial_subtitle_pending.take()
                && let Some(url) = current
                    .as_ref()
                    .and_then(|c| c.subtitles.get(index))
                    .and_then(|s| s.url.as_ref())
            {
                let cues = download_cues(url);
                let _ = updates.send(snapshot(&backend, None, Some(cues)));
            }
        }
        if current.is_some() && last.elapsed() >= Duration::from_millis(200) {
            last = Instant::now();
            let error = backend.take_error();
            let mut s = snapshot(&backend, error, None);
            if s.error.is_some() {
                s.status = -1;
            }
            let _ = updates.send(s);
        }
    }
}

fn download_cues(url: &str) -> Vec<Cue> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return vec![];
    }
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .build()
        .new_agent();
    agent
        .get(url)
        .call()
        .ok()
        .and_then(|mut response| {
            response
                .body_mut()
                .with_config()
                .limit(1_000_000)
                .read_to_string()
                .ok()
        })
        .map_or_else(Vec::new, |text| parse_subtitle(&text))
}

/// Pull `(width, height)` of the video out of any uMediaServer event shape.
#[cfg(any(test, all(feature = "webos", target_os = "linux", target_arch = "arm")))]
fn video_size(event: &serde_json::Value) -> Option<(i32, i32)> {
    use serde_json::Value;
    fn wh(v: &Value) -> Option<(i32, i32)> {
        let w = v.get("width")?.as_i64()?;
        let h = v.get("height")?.as_i64()?;
        (w > 0 && h > 0 && w <= 8192 && h <= 8192).then_some((w as i32, h as i32))
    }
    if let Some(v) = event.get("videoInfo").and_then(wh) {
        return Some(v);
    }
    let info = event.get("sourceInfo")?;
    info.get("video_streams")
        .and_then(|s| s.as_array()?.iter().find_map(wh))
        .or_else(|| {
            info.get("programInfo")?
                .as_array()?
                .iter()
                .find_map(|p| p.get("video_streams")?.as_array()?.iter().find_map(wh))
        })
        .or_else(|| wh(info))
}

#[cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
mod native {
    //! Thin dlopen wrapper around libluma_cinema.so (window helpers + C facade).
    use std::ffi::{CStr, CString, c_char, c_void};
    pub struct Native {
        library: *mut c_void,
        pub handle: *mut c_void,
    }
    impl Native {
        pub fn load() -> Self {
            let library = std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|p| p.join("lib/libluma_cinema.so")))
                .and_then(|p| CString::new(p.to_string_lossy().as_bytes()).ok())
                .map_or(std::ptr::null_mut(), |p| unsafe {
                    libc::dlopen(p.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL)
                });
            Self {
                library,
                handle: std::ptr::null_mut(),
            }
        }
        pub fn sym<T: Copy>(&self, name: &[u8]) -> Option<T> {
            if self.library.is_null() {
                return None;
            }
            let p = unsafe { libc::dlsym(self.library, name.as_ptr().cast()) };
            (!p.is_null()).then(|| unsafe { std::mem::transmute_copy(&p) })
        }
        /// Create the C facade lazily: only the fallback path needs it.
        pub fn facade(&mut self) -> *mut c_void {
            if self.handle.is_null()
                && let Some(f) =
                    self.sym::<unsafe extern "C" fn() -> *mut c_void>(b"luma_cinema_create\0")
            {
                self.handle = unsafe { f() };
            }
            self.handle
        }
        pub fn window_create(&self) -> Option<CString> {
            let f = self
                .sym::<unsafe extern "C" fn() -> *const c_char>(b"luma_cinema_window_create\0")?;
            let id = unsafe { f() };
            (!id.is_null()).then(|| unsafe { CStr::from_ptr(id) }.to_owned())
        }
        pub fn window_fit(&self, id: &CStr, w: i32, h: i32) -> bool {
            self.sym::<unsafe extern "C" fn(*const c_char, i32, i32) -> i32>(
                b"luma_cinema_window_fit\0",
            )
            .is_some_and(|f| unsafe { f(id.as_ptr(), w, h) } != 0)
        }
        pub fn window_destroy(&self, id: &CStr) {
            if let Some(f) =
                self.sym::<unsafe extern "C" fn(*const c_char)>(b"luma_cinema_window_destroy\0")
            {
                unsafe { f(id.as_ptr()) }
            }
        }
    }
    impl Drop for Native {
        fn drop(&mut self) {
            unsafe {
                if !self.handle.is_null()
                    && let Some(f) =
                        self.sym::<unsafe extern "C" fn(*mut c_void)>(b"luma_cinema_destroy\0")
                {
                    f(self.handle)
                }
                if !self.library.is_null() {
                    libc::dlclose(self.library);
                }
            }
        }
    }
}

#[cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
struct Backend {
    native: native::Native,
    bus: Option<crate::luna::Bus>,
    media: Option<String>,
    window: Option<std::ffi::CString>,
    legacy: bool,
    /// The LS2 hub refused com.webos.media for this app (dev apps get only
    /// the "public" group); go straight to the C facade from then on.
    denied: bool,
    loaded: bool,
    state: i32,
    position: u64,
    duration: u64,
    fitted: bool,
    opened_at: Instant,
    first_frame: Option<u64>,
    error: Option<String>,
    log: Option<std::fs::File>,
}

#[cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
impl Backend {
    const MEDIA: &'static str = "luna://com.webos.media/";
    fn new() -> Self {
        // Fresh log per run; append mode because the native side writes too.
        let path = "/media/developer/com.denisefremov.luma/luma-cinema.log";
        let _ = std::fs::write(path, "");
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok();
        Self {
            native: native::Native::load(),
            bus: None,
            media: None,
            window: None,
            legacy: false,
            // Off until proven: a direct load from Luma crashed umediaserver
            // (strlen in libums_connector, 25.10 12:06) and the TV rebooted.
            denied: true,
            loaded: false,
            state: 0,
            position: 0,
            duration: 0,
            fitted: false,
            opened_at: Instant::now(),
            first_frame: None,
            error: None,
            log,
        }
    }
    /// Diagnostic log for the TV. Never receives URLs, tokens or headers.
    fn note(&mut self, line: impl AsRef<str>) {
        use std::io::Write;
        if let Some(f) = self.log.as_mut() {
            let _ = writeln!(
                f,
                "{:>7} {}",
                self.opened_at.elapsed().as_millis(),
                line.as_ref()
            );
        }
    }
    fn window_mode() -> &'static str {
        // Escape hatch for on-TV diagnosis without a rebuild.
        match std::fs::read_to_string("/media/developer/com.denisefremov.luma/cinema-window") {
            Ok(s) if s.trim() == "none" => "none",
            _ => "export",
        }
    }
    fn call(&mut self, method: &str, extra: serde_json::Value) -> bool {
        let (Some(bus), Some(media)) = (self.bus.as_mut(), self.media.clone()) else {
            return false;
        };
        let mut payload = serde_json::json!({ "mediaId": media });
        if let (Some(p), Some(e)) = (payload.as_object_mut(), extra.as_object()) {
            p.extend(e.clone());
        }
        let reply = bus.request(&format!("{}{method}", Self::MEDIA), &payload);
        let ok = reply
            .as_ref()
            .is_ok_and(|v| v.get("returnValue").and_then(|r| r.as_bool()) != Some(false));
        if !ok {
            let detail = match &reply {
                Ok(v) => v
                    .get("errorText")
                    .and_then(|t| t.as_str())
                    .unwrap_or("refused")
                    .to_owned(),
                Err(e) => e.clone(),
            };
            self.note(format!("{method} failed: {detail}"));
        }
        ok
    }
    fn release(&mut self) {
        if self.media.is_some() {
            self.call("unload", serde_json::json!({}));
        }
        if let Some(bus) = self.bus.as_mut() {
            bus.unsubscribe_all();
        }
        self.media = None;
        if let Some(id) = self.window.take() {
            self.native.window_destroy(&id);
        }
        if self.legacy {
            self.legacy_control(5, 0);
            self.legacy = false;
        }
    }
    fn open(&mut self, url: &str) -> Result<(), String> {
        self.release();
        self.opened_at = Instant::now();
        self.state = 1;
        self.position = 0;
        self.duration = 0;
        self.loaded = false;
        self.fitted = false;
        self.first_frame = None;
        self.error = None;
        if self.denied {
            return self.open_legacy(url);
        }
        let mode = Self::window_mode();
        let mut option = serde_json::json!({ "appId": "com.denisefremov.luma" });
        if mode == "export" {
            self.window = self.native.window_create();
            if let Some(id) = self.window.as_ref() {
                option["windowId"] = serde_json::Value::String(id.to_string_lossy().into_owned());
            }
        }
        self.note(format!("open mode={mode} window={}", self.window.is_some()));
        if self.bus.is_none() {
            match crate::luna::Bus::open() {
                Ok(bus) => self.bus = Some(bus),
                Err(e) => self.note(format!("bus: {e}")),
            }
        }
        let payload =
            serde_json::json!({ "uri": url, "type": "media", "payload": { "option": option } });
        let reply = self
            .bus
            .as_mut()
            .map(|b| b.request(&format!("{}load", Self::MEDIA), &payload));
        match reply {
            Some(Ok(v)) if v.get("mediaId").and_then(|m| m.as_str()).is_some() => {
                let id = v["mediaId"].as_str().unwrap_or_default().to_owned();
                self.note(format!("load ok media={id}"));
                self.media = Some(id.clone());
                if let Some(bus) = self.bus.as_mut()
                    && let Err(e) = bus.subscribe(
                        &format!("{}subscribe", Self::MEDIA),
                        &serde_json::json!({ "mediaId": id }),
                    )
                {
                    self.note(format!("subscribe: {e}"));
                }
                Ok(())
            }
            other => {
                let detail = match other {
                    Some(Ok(v)) => v
                        .get("errorText")
                        .and_then(|t| t.as_str())
                        .unwrap_or("no mediaId")
                        .to_owned(),
                    Some(Err(e)) => e,
                    None => "no LS2 bus".into(),
                };
                self.note(format!("load failed: {detail}; falling back to C facade"));
                // The facade makes its own exported window; ours would sit unused.
                if let Some(id) = self.window.take() {
                    self.native.window_destroy(&id);
                }
                self.denied = true;
                self.open_legacy(url)
            }
        }
    }
    fn open_legacy(&mut self, url: &str) -> Result<(), String> {
        let h = self.native.facade();
        let c = std::ffi::CString::new(url).map_err(|_| "Invalid media URL".to_string())?;
        let size = stream_resolution(url).unwrap_or((0, 0));
        self.note(format!("open via C facade, video {}x{}", size.0, size.1));
        if let Some(f) = self
            .native
            .sym::<unsafe extern "C" fn(*mut libc::c_void, i32, i32)>(b"luma_cinema_video_size\0")
        {
            unsafe { f(h, size.0, size.1) };
        }
        let ok = !h.is_null()
            && self
                .native
                .sym::<unsafe extern "C" fn(*mut libc::c_void, *const libc::c_char) -> i32>(
                    b"luma_cinema_open\0",
                )
                .is_some_and(|f| unsafe { f(h, c.as_ptr()) } != 0);
        self.legacy = ok;
        if ok {
            Ok(())
        } else {
            Err("uMediaServer declined playback".into())
        }
    }
    fn tick(&mut self) {
        if self.legacy {
            return;
        }
        let events = self.bus.as_mut().map(|b| b.pump()).unwrap_or_default();
        for event in events {
            if let Some(t) = event.get("currentTime").and_then(|v| v.as_i64()) {
                self.position = t.max(0) as u64;
                if self.first_frame.is_none() && t > 0 {
                    self.first_frame = Some(self.opened_at.elapsed().as_millis() as u64);
                }
                continue;
            }
            let shown: Vec<String> = event
                .as_object()
                .map(|o| {
                    o.keys()
                        .filter(|k| *k != "mediaId" && *k != "returnValue")
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            self.note(format!("event {}", shown.join(",")));
            if event.get("loadCompleted").is_some() {
                self.loaded = true;
                self.play();
            }
            if let Some(info) = event.get("sourceInfo") {
                if let Some(d) = info
                    .get("duration")
                    .and_then(|v| v.as_i64())
                    .filter(|d| *d > 0)
                {
                    self.duration = d as u64;
                }
                let text: String = info.to_string().chars().take(600).collect();
                self.note(format!("sourceInfo {text}"));
            }
            if let Some((w, h)) = video_size(&event) {
                self.note(format!("video {w}x{h}"));
                self.fit(w, h);
            }
            if event.get("endOfStream").is_some() {
                self.state = 4;
            }
            if event.get("paused").is_some() {
                self.state = 3;
            }
            if event.get("playing").is_some() {
                self.state = 2;
            }
            if let Some(err) = event.get("error") {
                let text = err
                    .get("errorText")
                    .and_then(|t| t.as_str())
                    .unwrap_or("Playback error")
                    .to_owned();
                self.note(format!("error {err}"));
                self.error = Some(text);
            }
        }
        // Some pipelines never report loadCompleted for HLS; nudge play once.
        if self.media.is_some()
            && !self.loaded
            && self.state == 1
            && self.opened_at.elapsed() > Duration::from_secs(3)
        {
            self.loaded = true;
            self.note("no loadCompleted after 3s, playing anyway");
            self.play();
        }
        // Without a size event, fit a 16:9 frame so the plane is never left at 0x0.
        if self.window.is_some()
            && !self.fitted
            && self.opened_at.elapsed() > Duration::from_secs(4)
        {
            self.fit(1920, 1080);
        }
    }
    fn fit(&mut self, w: i32, h: i32) {
        if let Some(id) = self.window.clone() {
            let ok = self.native.window_fit(&id, w, h);
            self.fitted = true;
            self.note(format!("crop {w}x{h} -> window ok={ok}"));
        }
    }
    fn legacy_control(&mut self, op: i32, value: i64) -> bool {
        let h = self.native.facade();
        self.native
            .sym::<unsafe extern "C" fn(*mut libc::c_void, i32, i64) -> i32>(
                b"luma_cinema_control\0",
            )
            .is_some_and(|f| unsafe { f(h, op, value) } != 0)
    }
    fn play(&mut self) {
        if self.legacy {
            self.legacy_control(0, 0);
        } else if self.call("play", serde_json::json!({})) {
            self.state = 2;
        }
    }
    fn pause(&mut self) {
        if self.legacy {
            self.legacy_control(1, 0);
        } else if self.call("pause", serde_json::json!({})) {
            self.state = 3;
        }
    }
    fn seek(&mut self, ms: u64) {
        if self.legacy {
            self.legacy_control(2, ms as i64);
        } else if self.call("seek", serde_json::json!({ "position": ms })) {
            self.position = ms;
        }
    }
    fn select_track(&mut self, kind: &str, index: i64) {
        if self.legacy {
            self.legacy_control(if kind == "audio" { 3 } else { 4 }, index);
        } else if index >= 0 {
            self.call(
                "selectTrack",
                serde_json::json!({ "type": kind, "index": index }),
            );
        }
    }
    fn close(&mut self) {
        self.release();
        self.state = 0;
        self.position = 0;
        self.duration = 0;
    }
    fn legacy_value(&self, name: &[u8]) -> i64 {
        self.native
            .sym::<unsafe extern "C" fn(*mut libc::c_void) -> i64>(name)
            .map_or(-1, |f| unsafe { f(self.native.handle) })
    }
    fn status(&self) -> i32 {
        if self.legacy {
            return self
                .native
                .sym::<unsafe extern "C" fn(*mut libc::c_void) -> i32>(b"luma_cinema_state\0")
                .map_or(-1, |f| unsafe { f(self.native.handle) });
        }
        self.state
    }
    fn position(&self) -> u64 {
        if self.legacy {
            self.legacy_value(b"luma_cinema_position\0").max(0) as u64
        } else {
            self.position
        }
    }
    fn duration(&self) -> u64 {
        if self.legacy {
            self.legacy_value(b"luma_cinema_duration\0").max(0) as u64
        } else {
            self.duration
        }
    }
    fn first_frame_ms(&self) -> Option<u64> {
        self.first_frame
    }
    fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }
}

#[cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
impl Drop for Backend {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(not(all(feature = "webos", target_os = "linux", target_arch = "arm")))]
struct Backend;
#[cfg(not(all(feature = "webos", target_os = "linux", target_arch = "arm")))]
impl Backend {
    fn new() -> Self {
        Self
    }
    fn open(&mut self, _: &str) -> Result<(), String> {
        Err("Native cinema is available on the TV only".into())
    }
    fn tick(&mut self) {}
    fn play(&mut self) {}
    fn pause(&mut self) {}
    fn seek(&mut self, _: u64) {}
    fn select_track(&mut self, _: &str, _: i64) {}
    fn close(&mut self) {}
    fn status(&self) -> i32 {
        -1
    }
    fn position(&self) -> u64 {
        0
    }
    fn duration(&self) -> u64 {
        0
    }
    fn first_frame_ms(&self) -> Option<u64> {
        None
    }
    fn take_error(&mut self) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::video_size;
    use serde_json::json;
    #[test]
    fn resolution_from_master_playlist() {
        let text = "#EXTM3U\n#EXT-X-STREAM-INF:PROGRAM-ID=1,BANDWIDTH=3862565,VIDEO-RANGE=SDR,RESOLUTION=1920x800,CODECS=\"avc1.640028,mp4a.40.2\"\nv.m3u8\n";
        assert_eq!(super::playlist_resolution(text), Some((1920, 800)));
        assert_eq!(
            super::playlist_resolution("#EXTM3U\n#EXTINF:20,\na.ts\n"),
            None
        );
    }
    #[test]
    fn video_size_from_events() {
        assert_eq!(
            video_size(&json!({"videoInfo":{"width":3840,"height":2160}})),
            Some((3840, 2160))
        );
        assert_eq!(
            video_size(
                &json!({"sourceInfo":{"programInfo":[{"video_streams":[{"width":1920,"height":800}]}]}})
            ),
            Some((1920, 800))
        );
        assert_eq!(video_size(&json!({"currentTime":5})), None);
    }
}
