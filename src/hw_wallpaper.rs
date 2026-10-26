//! Zero-RGB-copy hardware wallpaper for rooted webOS 5+ LG TVs.
//! Online H.264 is demuxed, not decoded by the CPU; Starfish renders into an
//! exported SDL video plane underneath Luma's transparent GLES foreground.
//! The code never downloads all 289 files or caches the entire video collection.
#![cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
use std::{
    ffi::{CString, c_void},
    fs::OpenOptions,
    io::{Read, Write},
    os::raw::{c_char, c_int},
    process::{Child, Command, Stdio},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
type Open = unsafe extern "C" fn(*const c_char, c_int, c_int, *const c_char) -> *mut c_void;
type Feed = unsafe extern "C" fn(*mut c_void, *const c_void, usize, i64) -> c_int;
type Close = unsafe extern "C" fn(*mut c_void);
type Ended = unsafe extern "C" fn(*mut c_void) -> c_int;
type Rendered = unsafe extern "C" fn(*mut c_void) -> i64;
type SetDrop = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type Metrics = unsafe extern "C" fn(
    *mut c_void,
    c_int,
    *const c_char,
    *const c_char,
    u32,
    u32,
    u32,
    f64,
    f64,
    usize,
);

// Developer-only timing experiments. Absent config exactly preserves the
// existing 2.2.13 H.264 path; HEVC/Amazon never reads these settings.
#[derive(Clone, Copy)]
struct H264Tuning {
    lead_ms: u64,
    use_decode_pts: bool,
    retry_ms: u64,
    drop_threshold: Option<i32>,
    fixed_30fps: bool,
}
impl Default for H264Tuning {
    fn default() -> Self {
        Self {
            lead_ms: 250,
            use_decode_pts: false,
            retry_ms: 5,
            drop_threshold: None,
            fixed_30fps: false,
        }
    }
}
impl H264Tuning {
    fn load() -> Self {
        let mut tuning = Self::default();
        let Ok(content) =
            std::fs::read_to_string("/media/developer/com.denisefremov.luma/video-tuning.json")
        else {
            return tuning;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
            return tuning;
        };
        if let Some(ms) = value["lead_ms"].as_u64().filter(|v| *v <= 1500) {
            tuning.lead_ms = ms;
        }
        if let Some(ms) = value["retry_ms"].as_u64().filter(|v| *v <= 20) {
            tuning.retry_ms = ms;
        }
        tuning.use_decode_pts = value["pts_mode"].as_str() == Some("decode");
        tuning.drop_threshold = value["drop_threshold"]
            .as_i64()
            .filter(|v| (0..=5000).contains(v))
            .map(|v| v as i32);
        tuning.fixed_30fps = value["fixed_30fps"].as_bool() == Some(true);
        tuning
    }
}

static TRANSITION_EPOCH: OnceLock<Instant> = OnceLock::new();

/// Write bounded, monotonic transition timing without touching the UI thread's
/// rendering path. This is diagnostic evidence only and is safe to rotate with
/// the other per-device logs.
pub fn log_transition_event(index: usize, event: &str) {
    let start = TRANSITION_EPOCH.get_or_init(Instant::now);
    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/media/developer/com.denisefremov.luma/luma-video-transitions.log")
    else {
        return;
    };
    let _ = writeln!(
        file,
        "t_ms={} idx={} event={}",
        start.elapsed().as_millis(),
        index,
        event
    );
}

// A single compressed demux process opens the next source ahead of time.
// No additional NDL decoder or RGB texture is created.
struct Prepared {
    index: usize,
    child: Option<Child>,
    spawned: Instant,
}

/// Remote presses can skip through several clips; only the clip the viewer
/// settles on is opened, so intermediate clips never cost a decoder session.
const SETTLE: Duration = Duration::from_millis(250);
/// The UI paints the next clip's still over the plane before the old session
/// closes; give it a presented frame so the swap never shows a black gap.
const COVER: Duration = Duration::from_millis(50);
/// An idle prefetched demuxer holds an open HTTP stream; refresh it before
/// the server can time it out.
const PREPARED_MAX_AGE: Duration = Duration::from_secs(40);
impl Drop for Prepared {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
pub struct Player {
    lib: *mut c_void,
    open: Open,
    feed: Feed,
    close: Close,
    ended: Ended,
    rendered: Rendered,
    metrics: Metrics,
    set_drop: SetDrop,
    handle: *mut c_void,
    child: Option<Child>,
    reader: Option<JoinHandle<()>>,
    index: Option<usize>,
    ready: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    retry_at: Instant,
    first_playing: Option<Instant>,
    prepared: Option<Prepared>,
    next_choice_cache: crate::video_catalog::NextChoiceCache,
    /// Wanted clip that differs from the open one, and since when.
    pending: Option<(usize, Instant)>,
    /// Set once the UI was told to cover the plane for a clip swap.
    cover_requested: Option<Instant>,
    /// The open clip played to its end; the next one starts without settling.
    clip_ended: bool,
    /// Clip whose last start failed; only it waits for `retry_at`.
    failed: Option<usize>,
    /// Viewer is on the Next Video control: keep the next clip's stream ready.
    eager_next: bool,
}
impl Player {
    pub fn new() -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let path = exe
            .parent()
            .ok_or("No application directory")?
            .join("lib/libluma_hwplayer.so");
        let c = CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        let lib = unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if lib.is_null() {
            return Err(format!("Cannot load hardware wallpaper: {}", dlerror()));
        }
        unsafe fn function(lib: *mut c_void, name: &str) -> Result<*mut c_void, String> {
            let c = CString::new(name).map_err(|e| e.to_string())?;
            let ptr = unsafe { libc::dlsym(lib, c.as_ptr()) };
            if ptr.is_null() {
                return Err(format!("{name}: {}", dlerror()));
            }
            Ok(ptr)
        }
        let resolved = unsafe {
            let a = function(lib, "luma_hw_open");
            let b = function(lib, "luma_hw_feed");
            let c = function(lib, "luma_hw_close");
            let d = function(lib, "luma_hw_ended");
            let e = function(lib, "luma_hw_frames_rendered");
            let f = function(lib, "luma_hw_metrics");
            let g = function(lib, "luma_hw_set_drop_threshold");
            match (a, b, c, d, e, f, g) {
                (Ok(a), Ok(b), Ok(c), Ok(d), Ok(e), Ok(f), Ok(g)) => Ok((
                    std::mem::transmute::<*mut c_void, Open>(a),
                    std::mem::transmute::<*mut c_void, Feed>(b),
                    std::mem::transmute::<*mut c_void, Close>(c),
                    std::mem::transmute::<*mut c_void, Ended>(d),
                    std::mem::transmute::<*mut c_void, Rendered>(e),
                    std::mem::transmute::<*mut c_void, Metrics>(f),
                    std::mem::transmute::<*mut c_void, SetDrop>(g),
                )),
                other => Err(format!(
                    "Symbols missing: {:?}",
                    other
                        .0
                        .err()
                        .or_else(|| other.1.err())
                        .or_else(|| other.2.err())
                        .or_else(|| other.3.err())
                        .or_else(|| other.4.err())
                        .or_else(|| other.5.err())
                        .or_else(|| other.6.err())
                )),
            }
        };
        let (open, feed, close, ended, rendered, metrics, set_drop) = match resolved {
            Ok(f) => f,
            Err(e) => {
                unsafe { libc::dlclose(lib) };
                return Err(e);
            }
        };
        Ok(Self {
            lib,
            open,
            feed,
            close,
            ended,
            rendered,
            metrics,
            set_drop,
            handle: std::ptr::null_mut(),
            child: None,
            reader: None,
            index: None,
            ready: Arc::new(AtomicBool::new(false)),
            stopping: Arc::new(AtomicBool::new(false)),
            finished: Arc::new(AtomicBool::new(false)),
            retry_at: Instant::now(),
            first_playing: None,
            prepared: None,
            next_choice_cache: crate::video_catalog::NextChoiceCache::default(),
            pending: None,
            cover_requested: None,
            clip_ended: false,
            failed: None,
            eager_next: false,
        })
    }
    /// True while the live plane should be visible: it has a frame and no
    /// clip swap is about to close it.
    pub fn showing(&self) -> bool {
        self.active() && self.cover_requested.is_none()
    }
    /// A wanted clip is waiting to settle or for its cover frame.
    pub fn swap_pending(&self) -> bool {
        self.pending
            .is_some_and(|(index, _)| self.failed != Some(index))
            || self.cover_requested.is_some()
    }
    pub fn set_eager_next(&mut self, eager: bool) {
        self.eager_next = eager;
    }
    pub fn name(&self, index: usize) -> &str {
        crate::video_catalog::all()
            .get(index)
            .map_or("Video wallpaper", |v| v.name.as_str())
    }
    pub fn len(&self) -> usize {
        crate::video_catalog::all().len()
    }
    pub fn active(&self) -> bool {
        !self.handle.is_null() && unsafe { (self.rendered)(self.handle) } > 0
    }
    pub fn stop(&mut self) {
        self.prepared = None;
        self.stop_session();
    }
    fn stop_session(&mut self) {
        let closing_index = self.index;
        self.stopping.store(true, Ordering::Relaxed);
        if let Some(index) = closing_index {
            log_transition_event(index, "close_begin");
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if !self.handle.is_null() {
            unsafe {
                (self.close)(self.handle);
            }
            if let Some(index) = closing_index {
                log_transition_event(index, "close_end");
            }
            self.handle = std::ptr::null_mut();
        }
        self.index = None;
        self.ready.store(false, Ordering::Relaxed);
        self.finished.store(false, Ordering::Relaxed);
        self.first_playing = None;
        self.cover_requested = None;
        self.clip_ended = false;
    }
    fn spawn_demux(index: usize) -> Result<Child, String> {
        let entry = crate::video_catalog::all()
            .get(index)
            .ok_or("Invalid video")?;
        if !entry.url.starts_with("https://")
            || entry.url.len() > 2048
            || entry.url.contains(['\n', '\r', '\0'])
        {
            return Err("Invalid streaming URL".into());
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let demux = exe
            .parent()
            .ok_or("No application directory")?
            .join("luma-demux");
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open("/media/developer/com.denisefremov.luma/luma-demux.log")
            .ok();
        let mut cmd = Command::new(demux);
        cmd.arg(if entry.source == "Amazon" {
            "h265"
        } else {
            "h264"
        })
        .arg(&entry.url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(log.map_or_else(Stdio::null, Stdio::from));
        if entry.source != "Amazon" && H264Tuning::load().fixed_30fps {
            cmd.env("LUMA_H264_FIXED_30", "1");
        }
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        cmd.spawn().map_err(|e| e.to_string())
    }
    fn maybe_prefetch(
        &mut self,
        index: usize,
        excluded: &std::collections::BTreeSet<String>,
        shuffle: bool,
        seed: u64,
    ) {
        let key = crate::video_catalog::NextChoiceKey::new(index, excluded, shuffle, seed);
        let key_changed = self.next_choice_cache.key() != Some(key);
        let next = self.next_choice_cache.resolve(key, || {
            crate::video_catalog::next_choice(index, excluded, shuffle, seed)
        });
        // A changed selection/shuffle/seed invalidates any child prepared for
        // the previous playlist, so it cannot be used after preferences change.
        if key_changed
            || self
                .prepared
                .as_ref()
                .is_some_and(|p| p.spawned.elapsed() >= PREPARED_MAX_AGE)
        {
            self.prepared = None;
        }
        if self.prepared.is_some() {
            return;
        }
        // Amazon HEVC: no speculative requests or timing changes.
        if crate::video_catalog::all()[index].source == "Amazon" {
            return;
        }
        let Some((next, _)) = next else {
            return;
        };
        if next == index || crate::video_catalog::all()[next].source == "Amazon" {
            return;
        }
        let (Some(started), Some(duration)) = (
            self.first_playing,
            crate::video_catalog::duration_seconds(index),
        ) else {
            return;
        };
        // Near the end for the automatic swap; at once while the viewer is
        // on Next Video, so a press skips the HTTP and demux startup.
        let near_end = duration > 15.0 && started.elapsed().as_secs_f64() >= duration - 7.0;
        if !near_end && !self.eager_next {
            return;
        }
        if let Ok(child) = Self::spawn_demux(next) {
            self.prepared = Some(Prepared {
                index: next,
                child: Some(child),
                spawned: Instant::now(),
            });
            eprintln!("Prefetched H.264 stream {next}");
        }
    }
    fn start(&mut self, index: usize) -> Result<(), String> {
        let mut cached = None;
        if let Some(mut prepared) = self.prepared.take() {
            if prepared.index == index {
                cached = prepared.child.take();
            }
        }
        self.stop_session();
        log_transition_event(index, "open_begin");
        let entry = crate::video_catalog::all()
            .get(index)
            .ok_or("Invalid video")?;
        let url = &entry.url;
        if !url.starts_with("https://") || url.len() > 2048 || url.contains(['\n', '\r', '\0']) {
            return Err("Invalid streaming URL".into());
        }
        let app = CString::new("com.denisefremov.luma").unwrap();
        let codec = if entry.source == "Amazon" {
            "H265"
        } else {
            "H264"
        };
        let codec_c = CString::new(codec).unwrap();
        let tuning = if codec == "H264" {
            H264Tuning::load()
        } else {
            H264Tuning::default()
        };
        let handle = unsafe { (self.open)(app.as_ptr(), 1920, 1080, codec_c.as_ptr()) };
        if handle.is_null() {
            if let Some(mut child) = cached {
                let _ = child.kill();
                let _ = child.wait();
            }
            return Err("Hardware SMP exported-window Load failed".into());
        }
        if codec == "H264" {
            if let Some(threshold) = tuning.drop_threshold {
                let rc = unsafe { (self.set_drop)(handle, threshold) };
                eprintln!("H264 NDL drop threshold={threshold} rc={rc}");
            }
            eprintln!(
                "H264 tuning: lead={}ms decode_pts={} retry={}ms fixed_30fps={}",
                tuning.lead_ms, tuning.use_decode_pts, tuning.retry_ms, tuning.fixed_30fps
            );
        }
        self.handle = handle;
        log_transition_event(index, "load_ok");
        let mut child = match cached {
            Some(child) => child,
            None => Self::spawn_demux(index)?,
        };
        let mut stdout = child.stdout.take().ok_or("No video stream pipe")?;
        self.stopping = Arc::new(AtomicBool::new(false));
        self.finished = Arc::new(AtomicBool::new(false));
        self.ready = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&self.stopping);
        let finished = Arc::clone(&self.finished);
        let ready = Arc::clone(&self.ready);
        let feed = self.feed;
        let metrics = self.metrics;
        let metric_source = CString::new(entry.source.clone()).unwrap();
        let metric_codec = codec_c;
        let handle = handle as usize;
        let metric_index = index as c_int;
        self.reader = Some(thread::spawn(move || {
            let mut magic = [0u8; 4];
            if stdout.read_exact(&mut magic).is_err() {
                log_transition_event(metric_index as usize, "pipe_eof");
            } else if &magic == b"LVID" || &magic == b"LVI2" {
                let has_separate_pts = &magic == b"LVI2";
                let started = Instant::now();
                let mut window_start = started;
                let mut au_count = 0u32;
                let mut first_feed_emitted = false;
                let mut retry_count = 0u32;
                let mut error_count = 0u32;
                let mut lateness_total = 0.0f64;
                let mut lateness_max = 0.0f64;
                let mut max_au = 0usize;
                while !stopping.load(Ordering::Relaxed) {
                    let mut hdr = [0u8; 12];
                    if stdout.read_exact(&mut hdr).is_err() {
                        log_transition_event(metric_index as usize, "pipe_eof");
                        break;
                    }
                    let len = u32::from_le_bytes(hdr[..4].try_into().unwrap()) as usize;
                    let decode_ns = u64::from_le_bytes(hdr[4..].try_into().unwrap());
                    let mut pts = decode_ns;
                    if has_separate_pts {
                        let mut display = [0u8; 8];
                        if stdout.read_exact(&mut display).is_err() {
                            break;
                        }
                        pts = u64::from_le_bytes(display);
                    }
                    if len == 0
                        || len > 2 * 1024 * 1024
                        || pts > 1000 * 3600 * 1_000_000_000
                        || decode_ns > 1000 * 3600 * 1_000_000_000
                    {
                        log_transition_event(metric_index as usize, "pipe_eof");
                        break;
                    }
                    let mut access_unit = vec![0u8; len];
                    if stdout.read_exact(&mut access_unit).is_err() {
                        log_transition_event(metric_index as usize, "pipe_eof");
                        break;
                    }
                    // Source PTS is normalized by the demuxer. Pace compressed
                    // packets by their natural timestamps, never force 15 fps.
                    // Only H.264 changes: pace by monotonically increasing
                    // decode time, pass the original presentation time to NDL.
                    // Keep ~250 ms queued to absorb network/thread jitter.
                    // HEVC/Amazon stays bit-for-bit on its proven schedule.
                    let schedule_ns = crate::video_catalog::video_feed_schedule_ns(
                        decode_ns,
                        has_separate_pts,
                        tuning.lead_ms,
                    );
                    let wait_until = started + Duration::from_nanos(schedule_ns);
                    while Instant::now() < wait_until && !stopping.load(Ordering::Relaxed) {
                        thread::sleep(Duration::from_millis(3));
                    }
                    if stopping.load(Ordering::Relaxed) {
                        break;
                    }
                    let lateness_ms = Instant::now()
                        .saturating_duration_since(wait_until)
                        .as_secs_f64()
                        * 1000.0;
                    lateness_total += lateness_ms;
                    lateness_max = lateness_max.max(lateness_ms);
                    max_au = max_au.max(len);
                    loop {
                        let rc = unsafe {
                            feed(
                                handle as *mut c_void,
                                access_unit.as_ptr().cast(),
                                access_unit.len(),
                                if has_separate_pts && tuning.use_decode_pts {
                                    decode_ns as i64
                                } else {
                                    pts as i64
                                },
                            )
                        };
                        if rc == 0 {
                            if !first_feed_emitted {
                                log_transition_event(metric_index as usize, "first_feed");
                                first_feed_emitted = true;
                            }
                            au_count += 1;
                            ready.store(true, Ordering::Relaxed);
                            break;
                        }
                        if rc < 0 {
                            error_count += 1;
                            stopping.store(true, Ordering::Relaxed);
                            break;
                        }
                        if stopping.load(Ordering::Relaxed) {
                            break;
                        }
                        retry_count += 1;
                        if has_separate_pts && tuning.retry_ms == 0 {
                            thread::yield_now();
                        } else {
                            thread::sleep(Duration::from_millis(if has_separate_pts {
                                tuning.retry_ms
                            } else {
                                5
                            }));
                        }
                    }
                    if window_start.elapsed() >= Duration::from_secs(1) {
                        unsafe {
                            metrics(
                                handle as *mut c_void,
                                metric_index,
                                metric_source.as_ptr(),
                                metric_codec.as_ptr(),
                                au_count,
                                retry_count,
                                error_count,
                                lateness_total / f64::from(au_count.max(1)),
                                lateness_max,
                                max_au,
                            );
                        }
                        window_start = Instant::now();
                        au_count = 0;
                        retry_count = 0;
                        error_count = 0;
                        lateness_total = 0.0;
                        lateness_max = 0.0;
                        max_au = 0;
                    }
                }
            }
            finished.store(true, Ordering::Relaxed);
        }));
        self.child = Some(child);
        self.index = Some(index);
        eprintln!(
            "Luma SMP hardware stream started: {} ({}); original video bits",
            index, entry.name
        );
        Ok(())
    }
    pub fn tick(
        &mut self,
        wanted: Option<usize>,
        excluded: &std::collections::BTreeSet<String>,
        shuffle: bool,
        seed: u64,
    ) -> Option<usize> {
        let Some(index) = wanted else {
            if self.index.is_some() {
                self.stop();
            }
            return None;
        };
        if self.index != Some(index) {
            let now = Instant::now();
            let since = match self.pending {
                Some((pending, since)) if pending == index => since,
                _ => {
                    self.pending = Some((index, now));
                    now
                }
            };
            // A live clip keeps playing until the viewer stops skipping;
            // nothing else delays a swap. Only a failed clip backs off.
            let live = self.index.is_some() && !self.clip_ended;
            if live && now.duration_since(since) < SETTLE {
                return None;
            }
            if self.failed == Some(index) && now < self.retry_at {
                return None;
            }
            if self.index.is_some() && self.active() {
                let covered = *self.cover_requested.get_or_insert(now);
                if now.duration_since(covered) < COVER {
                    return None;
                }
            }
            match self.start(index) {
                Ok(()) => self.failed = None,
                Err(e) => {
                    eprintln!("Hardware wallpaper start failed: {e}");
                    self.failed = Some(index);
                    self.retry_at = Instant::now() + Duration::from_secs(12);
                }
            }
            self.pending = None;
            return None;
        }
        self.pending = None;
        if self.clip_ended {
            return None;
        }
        if self.active() && self.first_playing.is_none() {
            self.first_playing = Some(Instant::now());
        }
        if self.first_playing.is_some() {
            self.maybe_prefetch(index, excluded, shuffle, seed);
        }
        if self.finished.load(Ordering::Relaxed)
            || (!self.handle.is_null() && unsafe { (self.ended)(self.handle) } != 0)
        {
            let completed = self.active();
            let next = completed
                .then(|| crate::video_catalog::next_index(index, excluded, shuffle, seed))
                .flatten()
                .filter(|next| *next != index);
            if next.is_some() {
                // Leave the finished clip's last frame on the plane; the swap
                // covers it with the next still before closing the session.
                self.clip_ended = true;
                return next;
            }
            self.stop_session();
            if !completed {
                self.prepared = None;
                self.failed = Some(index);
                self.retry_at = Instant::now() + Duration::from_secs(12);
            }
        }
        None
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
        unsafe { libc::dlclose(self.lib) };
    }
}
fn dlerror() -> String {
    unsafe {
        let s = libc::dlerror();
        if s.is_null() {
            "Unknown loader error".into()
        } else {
            std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned()
        }
    }
}
