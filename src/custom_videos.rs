//! Video wallpapers the viewer uploads from a phone or computer on the same
//! network. Settings › Wallpaper › Upload Videos shows a QR code; the page it
//! opens sends MP4/MOV files straight to the TV, which keeps them next to the
//! preferences and plays them from disk with the hardware wallpaper. Nothing
//! leaves the local network, and every request needs the page's random nonce.
use crate::phone_link::{self, constant_time_eq, escape_html, random_hex};
use crate::video_catalog::{self, Video};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{BufWriter, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

/// Largest single upload.
const MAX_VIDEO_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Largest preview picture (the page makes them in the browser).
const MAX_IMAGE_BYTES: u64 = 4 * 1024 * 1024;
/// Uploaded clips kept at once.
pub const MAX_VIDEOS: usize = 48;
const NAME_LIMIT: usize = 60;
/// The upload page stays reachable this long after Settings opened it.
const LIFETIME: Duration = Duration::from_secs(30 * 60);
const INDEX: &str = "videos.json";
/// Catalog ids of uploaded clips carry this prefix.
const ID_PREFIX: &str = "custom-";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub hevc: bool,
}

static DIR: OnceLock<PathBuf> = OnceLock::new();
/// Serializes changes to the index between upload connections.
static INDEX_LOCK: Mutex<()> = Mutex::new(());

/// Where uploads live: a `videos` folder beside the preferences file.
pub fn init(preferences: &Path) {
    let dir = preferences
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .join("videos");
    let _ = fs::create_dir_all(&dir);
    let _ = DIR.set(dir);
    reload();
}

pub fn dir() -> Option<&'static Path> {
    DIR.get().map(PathBuf::as_path)
}

fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn video_file(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.mp4"))
}
fn image_file(dir: &Path, id: &str, still: bool) -> PathBuf {
    dir.join(format!(
        "{id}-{}.jpg",
        if still { "still" } else { "thumb" }
    ))
}

/// Saved uploads whose video file still exists.
pub fn load(dir: &Path) -> Vec<Entry> {
    let mut entries: Vec<Entry> = fs::read(dir.join(INDEX))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    entries.retain(|e| {
        valid_id(&e.id) && seen.insert(e.id.clone()) && video_file(dir, &e.id).is_file()
    });
    for e in &mut entries {
        e.name = clean_name(&e.name);
    }
    entries.truncate(MAX_VIDEOS);
    entries
}
fn save(dir: &Path, entries: &[Entry]) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(entries).map_err(|e| e.to_string())?;
    let temporary = dir.join(format!("{INDEX}.tmp"));
    fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
    fs::rename(&temporary, dir.join(INDEX)).map_err(|e| e.to_string())
}

/// Read the uploads again and put them into the wallpaper catalog.
pub fn reload() {
    let Some(dir) = dir() else {
        return;
    };
    let entries = load(dir);
    video_catalog::set_custom(
        entries
            .iter()
            .map(|e| {
                Video::custom(
                    format!("{ID_PREFIX}{}", e.id),
                    e.name.clone(),
                    &video_file(dir, &e.id),
                    e.hevc,
                )
            })
            .collect(),
    );
}

/// Catalog index of an uploaded clip.
pub fn catalog_index(id: &str) -> Option<usize> {
    let wanted = format!("{ID_PREFIX}{id}");
    video_catalog::all().iter().position(|v| v.id == wanted)
}

/// The preview picture of an uploaded clip, if the browser made one.
pub fn image(index: usize, still: bool) -> Option<String> {
    let video = video_catalog::all().get(index)?;
    let id = video.id.strip_prefix(ID_PREFIX)?;
    let path = image_file(dir()?, id, still);
    Some(if path.is_file() {
        path.to_string_lossy().into_owned()
    } else {
        String::new()
    })
}

/// True for a path the hardware wallpaper may open: an uploaded clip.
#[cfg_attr(
    not(all(feature = "webos", target_os = "linux", target_arch = "arm")),
    allow(dead_code)
)]
pub fn is_upload_path(path: &str) -> bool {
    let Some(dir) = dir() else {
        return false;
    };
    let path = Path::new(path);
    path.parent() == Some(dir)
        && path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(valid_id)
        && path.extension().is_some_and(|e| e == "mp4")
}

fn clean_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(NAME_LIMIT)
        .collect();
    if clean.trim().is_empty() {
        "My video".into()
    } else {
        clean.trim().into()
    }
}

/// Codec of an MP4/MOV file, read from the sample description (`stsd`)
/// entries: the entry type follows `stsd`, version/flags, count and size.
#[derive(Default)]
struct Sniffer {
    head: Vec<u8>,
    tail: Vec<u8>,
    avc: bool,
    hevc: bool,
}
impl Sniffer {
    /// Bytes from the start of `stsd` to the end of the first entry type.
    const SPAN: usize = 20;
    fn feed(&mut self, chunk: &[u8]) {
        if self.head.len() < 12 {
            let take = (12 - self.head.len()).min(chunk.len());
            self.head.extend_from_slice(&chunk[..take]);
        }
        let mut window = std::mem::take(&mut self.tail);
        window.extend_from_slice(chunk);
        if window.len() >= Self::SPAN {
            for i in 0..=window.len() - Self::SPAN {
                if &window[i..i + 4] == b"stsd" {
                    match &window[i + 16..i + 20] {
                        b"avc1" | b"avc3" => self.avc = true,
                        b"hvc1" | b"hev1" => self.hevc = true,
                        _ => {}
                    }
                }
            }
        }
        let keep = window.len().min(Self::SPAN - 1);
        self.tail = window[window.len() - keep..].to_vec();
    }
    /// Ok(hevc) for a playable file.
    fn verdict(&self) -> Result<bool, &'static str> {
        // ISO BMFF/QuickTime: a box size, then a known box type.
        let container = self.head.len() >= 8
            && matches!(
                &self.head[4..8],
                b"ftyp" | b"moov" | b"mdat" | b"wide" | b"free" | b"skip"
            );
        if !container {
            return Err("Not an MP4 or MOV file.");
        }
        match (self.avc, self.hevc) {
            (true, _) => Ok(false),
            (false, true) => Ok(true),
            _ => Err("The video must be H.264 or HEVC (H.265)."),
        }
    }
}

// ------------------------------------------------------------------ server

pub enum Event {
    /// The address the QR code shows, or why there is none.
    Ready(Result<String, String>),
    /// An upload finished; the payload is its upload id.
    Added(String),
    /// A clip was deleted from the page.
    Removed,
}

pub struct Server {
    pub events: Receiver<Event>,
    canceled: Arc<AtomicBool>,
}
impl Server {
    pub fn stop(&self) {
        self.canceled.store(true, Ordering::Relaxed);
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Serve the upload page on the LAN until stopped or `LIFETIME` ends.
pub fn start() -> Server {
    let (tx, events) = mpsc::channel();
    let canceled = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&canceled);
    thread::spawn(move || {
        let Some(dir) = dir().map(Path::to_path_buf) else {
            let _ = tx.send(Event::Ready(Err("No storage for videos".into())));
            return;
        };
        let Some(ip) = phone_link::lan_ip() else {
            let _ = tx.send(Event::Ready(Err("TV has no LAN address".into())));
            return;
        };
        let (listener, port) = match phone_link::bind_listener(&ip) {
            Ok(v) => v,
            Err(e) => {
                let _ = tx.send(Event::Ready(Err(e)));
                return;
            }
        };
        // Short enough to type on a computer; it only works on this LAN for
        // half an hour.
        let nonce = random_hex()[..12].to_owned();
        let _ = tx.send(Event::Ready(Ok(format!(
            "http://{ip}:{port}/videos?n={nonce}"
        ))));
        serve(listener, &dir, &nonce, &tx, &stop);
    });
    Server { events, canceled }
}

fn serve(listener: TcpListener, dir: &Path, nonce: &str, tx: &Sender<Event>, stop: &AtomicBool) {
    let begin = Instant::now();
    let busy = Arc::new(AtomicBool::new(false));
    while !stop.load(Ordering::Relaxed) && begin.elapsed() < LIFETIME {
        let stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(75));
                continue;
            }
            Err(_) => break,
        };
        let dir = dir.to_path_buf();
        let nonce = nonce.to_owned();
        let tx = tx.clone();
        let busy = Arc::clone(&busy);
        // One thread per connection, so a long upload never blocks the page.
        thread::spawn(move || {
            let _ = stream.set_nonblocking(false);
            handle(stream, &dir, &nonce, &tx, &busy);
        });
    }
}

struct Request {
    method: String,
    path: String,
    query: std::collections::HashMap<String, String>,
    length: u64,
    /// Body bytes read together with the headers.
    early: Vec<u8>,
}

fn read_head(stream: &mut TcpStream) -> Result<Request, &'static str> {
    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(30))).ok();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let end = loop {
        let n = stream.read(&mut chunk).map_err(|_| "Request timeout")?;
        if n == 0 {
            return Err("Incomplete request");
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if buf.len() > 16 * 1024 {
            return Err("Headers too large");
        }
    };
    let head = std::str::from_utf8(&buf[..end]).map_err(|_| "Malformed headers")?;
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split_whitespace();
    let method = first.next().ok_or("Missing method")?.to_owned();
    let target = first.next().ok_or("Missing path")?;
    if !target.starts_with('/') || target.len() > 4096 {
        return Err("Invalid path");
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut length = 0u64;
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().map_err(|_| "Invalid length")?;
            }
            if key.eq_ignore_ascii_case("transfer-encoding") {
                return Err("Chunked uploads are not supported");
            }
        }
    }
    Ok(Request {
        method,
        path: path.to_owned(),
        query: phone_link::parse_form(query),
        length,
        early: buf[end..].to_vec(),
    })
}

fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; connect-src 'self'; media-src blob:; img-src blob: data:\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}
fn json(stream: &mut TcpStream, status: &str, value: serde_json::Value) {
    respond(stream, status, "application/json", &value.to_string());
}
fn fail(stream: &mut TcpStream, status: &str, message: &str) {
    json(stream, status, serde_json::json!({ "error": message }));
}

/// Copy the request body into `file`, up to `limit` bytes.
fn receive(
    stream: &mut TcpStream,
    request: &Request,
    file: &Path,
    limit: u64,
    mut inspect: impl FnMut(&[u8]),
) -> Result<(), String> {
    if request.length == 0 {
        return Err("Empty upload.".into());
    }
    if request.length > limit {
        return Err(format!(
            "The file is too large (limit {} MB).",
            limit / 1024 / 1024
        ));
    }
    let mut out = BufWriter::new(fs::File::create(file).map_err(|e| e.to_string())?);
    let mut remaining = request.length;
    let early = &request.early[..request.early.len().min(remaining as usize)];
    inspect(early);
    out.write_all(early).map_err(|e| e.to_string())?;
    remaining -= early.len() as u64;
    let mut chunk = vec![0u8; 256 * 1024];
    while remaining > 0 {
        let want = chunk.len().min(remaining as usize);
        let n = stream
            .read(&mut chunk[..want])
            .map_err(|_| "The upload stopped.".to_owned())?;
        if n == 0 {
            return Err("The upload stopped.".into());
        }
        inspect(&chunk[..n]);
        out.write_all(&chunk[..n])
            .map_err(|_| "The TV is out of storage space.".to_owned())?;
        remaining -= n as u64;
    }
    out.flush()
        .map_err(|_| "The TV is out of storage space.".to_owned())
}

fn handle(mut stream: TcpStream, dir: &Path, nonce: &str, tx: &Sender<Event>, busy: &AtomicBool) {
    let request = match read_head(&mut stream) {
        Ok(r) => r,
        Err(e) => return fail(&mut stream, "400 Bad Request", e),
    };
    if request.method == "GET" && request.path == "/" {
        return respond(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            &phone_link::html_page("Luma", "<p>Scan the QR code on your TV.</p>"),
        );
    }
    let authorized = request
        .query
        .get("n")
        .is_some_and(|n| constant_time_eq(n, nonce));
    if !authorized {
        return fail(
            &mut stream,
            "403 Forbidden",
            "This link expired. Scan the QR code again.",
        );
    }
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/videos") => {
            let entries = {
                let _guard = INDEX_LOCK.lock();
                load(dir)
            };
            respond(
                &mut stream,
                "200 OK",
                "text/html; charset=utf-8",
                &page(&entries, nonce),
            );
        }
        ("PUT", "/videos/upload") => {
            if busy.swap(true, Ordering::AcqRel) {
                return fail(&mut stream, "409 Conflict", "Another upload is running.");
            }
            let result = upload(&mut stream, &request, dir);
            busy.store(false, Ordering::Release);
            match result {
                Ok(id) => json(&mut stream, "200 OK", serde_json::json!({ "id": id })),
                Err(e) => fail(&mut stream, "422 Unprocessable Content", &e),
            }
        }
        ("PUT", "/videos/image") => {
            let id = request.query.get("id").cloned().unwrap_or_default();
            let still = request.query.get("kind").is_some_and(|k| k == "still");
            if !valid_id(&id) || !video_file(dir, &id).is_file() {
                return fail(&mut stream, "404 Not Found", "Unknown video.");
            }
            let target = image_file(dir, &id, still);
            let temporary = target.with_extension("part");
            let mut jpeg = None;
            let result = receive(&mut stream, &request, &temporary, MAX_IMAGE_BYTES, |b| {
                if jpeg.is_none() && b.len() >= 2 {
                    jpeg = Some(b[0] == 0xff && b[1] == 0xd8);
                }
            });
            match result {
                Ok(()) if jpeg == Some(true) => {
                    let _ = fs::rename(&temporary, &target);
                    json(&mut stream, "200 OK", serde_json::json!({}))
                }
                Ok(()) => {
                    let _ = fs::remove_file(&temporary);
                    fail(
                        &mut stream,
                        "422 Unprocessable Content",
                        "Not a JPEG picture.",
                    )
                }
                Err(e) => {
                    let _ = fs::remove_file(&temporary);
                    fail(&mut stream, "422 Unprocessable Content", &e)
                }
            }
        }
        ("POST", "/videos/done") => {
            let id = request.query.get("id").cloned().unwrap_or_default();
            if valid_id(&id) {
                let _ = tx.send(Event::Added(id));
            }
            json(&mut stream, "200 OK", serde_json::json!({}));
        }
        ("POST", "/videos/delete") => {
            let id = request.query.get("id").cloned().unwrap_or_default();
            if !valid_id(&id) {
                return fail(&mut stream, "404 Not Found", "Unknown video.");
            }
            {
                let _guard = INDEX_LOCK.lock();
                let mut entries = load(dir);
                entries.retain(|e| e.id != id);
                let _ = save(dir, &entries);
            }
            for path in [
                video_file(dir, &id),
                image_file(dir, &id, false),
                image_file(dir, &id, true),
            ] {
                let _ = fs::remove_file(path);
            }
            let _ = tx.send(Event::Removed);
            json(&mut stream, "200 OK", serde_json::json!({}));
        }
        _ => fail(&mut stream, "404 Not Found", "Not found."),
    }
}

fn upload(stream: &mut TcpStream, request: &Request, dir: &Path) -> Result<String, String> {
    {
        let _guard = INDEX_LOCK.lock();
        if load(dir).len() >= MAX_VIDEOS {
            return Err(format!(
                "You can keep up to {MAX_VIDEOS} videos. Delete one first."
            ));
        }
    }
    let name = clean_name(request.query.get("name").map_or("", String::as_str));
    let id = random_hex()[..32].to_owned();
    let temporary = dir.join(format!("{id}.part"));
    let mut sniffer = Sniffer::default();
    let received = receive(stream, request, &temporary, MAX_VIDEO_BYTES, |b| {
        sniffer.feed(b)
    });
    let hevc = match received.and_then(|()| sniffer.verdict().map_err(str::to_owned)) {
        Ok(hevc) => hevc,
        Err(e) => {
            let _ = fs::remove_file(&temporary);
            return Err(e);
        }
    };
    let _guard = INDEX_LOCK.lock();
    fs::rename(&temporary, video_file(dir, &id)).map_err(|e| e.to_string())?;
    let mut entries = load(dir);
    entries.push(Entry {
        id: id.clone(),
        name,
        hevc,
    });
    if let Err(e) = save(dir, &entries) {
        let _ = fs::remove_file(video_file(dir, &id));
        return Err(e);
    }
    Ok(id)
}

fn page(entries: &[Entry], nonce: &str) -> String {
    let mut list = String::new();
    for e in entries {
        list.push_str(&format!(
            r#"<li><span>{}</span><button class="del" data-id="{}">Delete</button></li>"#,
            escape_html(&e.name),
            e.id
        ));
    }
    if list.is_empty() {
        list.push_str("<li class=\"empty\">No videos yet.</li>");
    }
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="referrer" content="no-referrer"><title>Luma videos</title><style>
:root{{color-scheme:dark}}body{{background:#101115;color:#f6f6f8;font:500 18px/1.5 system-ui,sans-serif;max-width:560px;margin:6vh auto;padding:24px}}
h1{{font-size:30px;font-weight:750;letter-spacing:-.04em;margin:0 0 10px}}h2{{font-size:21px;margin:34px 0 8px}}
p{{color:#c2c3cd;margin:0 0 20px}}label.pick{{display:block;text-align:center;padding:19px;background:linear-gradient(105deg,#b29bff,#77c8ff);border-radius:15px;color:#101115;font-size:20px;font-weight:800;cursor:pointer}}
input[type=file]{{display:none}}ul{{list-style:none;padding:0;margin:0}}li{{display:flex;align-items:center;justify-content:space-between;gap:12px;padding:12px 16px;background:#1d1e24;border-radius:12px;margin:8px 0}}
li span{{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}li.empty{{color:#9a9ba5}}button.del{{background:#3a2a30;color:#ffc1c1;border:0;border-radius:10px;padding:8px 14px;font-size:16px;font-weight:700}}
progress{{width:100%;height:14px;margin-top:14px}}#status{{min-height:28px;margin-top:10px;color:#c2c3cd}}.error{{color:#ffc1c1}}
</style></head><body><h1>Luma video wallpapers</h1>
<p>Send your own videos to the TV. MP4 or MOV, H.264 or HEVC. 1080p H.264 plays most reliably. Up to {limit} GB each.</p>
<label class="pick">Choose videos<input id="file" type="file" accept="video/mp4,video/quicktime,.mp4,.mov,.m4v" multiple></label>
<progress id="bar" max="1" value="0" hidden></progress><div id="status"></div>
<h2>On your TV</h2><ul id="list">{list}</ul>
<script>
const nonce={nonce:?};
const status=document.getElementById('status'),bar=document.getElementById('bar');
function say(t,bad){{status.textContent=t;status.className=bad?'error':'';}}
function send(method,url,body,progress){{return new Promise((ok,no)=>{{const x=new XMLHttpRequest();x.open(method,url);
if(progress)x.upload.onprogress=e=>{{if(e.lengthComputable)progress(e.loaded/e.total);}};
x.onload=()=>{{let r={{}};try{{r=JSON.parse(x.responseText||'{{}}');}}catch(e){{}}x.status<300?ok(r):no(new Error(r.error||('Error '+x.status)));}};
x.onerror=()=>no(new Error('Connection lost. Is the TV still on Upload Videos?'));x.send(body);}});}}
function frame(file,w,h){{return new Promise(ok=>{{const v=document.createElement('video');v.muted=true;v.playsInline=true;v.preload='auto';
const url=URL.createObjectURL(file);const done=b=>{{URL.revokeObjectURL(url);ok(b);}};const t=setTimeout(()=>done(null),15000);
v.onerror=()=>{{clearTimeout(t);done(null);}};v.onloadedmetadata=()=>{{v.currentTime=Math.min(1,(v.duration||2)/2);}};
v.onseeked=()=>{{clearTimeout(t);const c=document.createElement('canvas');c.width=w;c.height=h;const g=c.getContext('2d');
const s=Math.max(w/v.videoWidth,h/v.videoHeight),dw=v.videoWidth*s,dh=v.videoHeight*s;g.drawImage(v,(w-dw)/2,(h-dh)/2,dw,dh);
c.toBlob(b=>done(b),'image/jpeg',0.86);}};v.src=url;}});}}
async function one(file,i,n){{const name=file.name.replace(/\.[^.]+$/,'');const tag=n>1?` (${{i+1}} of ${{n}})`:'';
bar.hidden=false;bar.value=0;say('Uploading '+name+tag+'…');
const r=await send('PUT','/videos/upload?n='+nonce+'&name='+encodeURIComponent(name),file,p=>{{bar.value=p;}});
say('Making a preview…');
for(const [kind,w,h] of [['thumb',480,270],['still',1920,1080]]){{const b=await frame(file,w,h);
if(b){{try{{await send('PUT','/videos/image?n='+nonce+'&id='+r.id+'&kind='+kind,b);}}catch(e){{}}}}}}
await send('POST','/videos/done?n='+nonce+'&id='+r.id,'');}}
document.getElementById('file').onchange=async e=>{{const files=[...e.target.files];e.target.value='';
try{{for(let i=0;i<files.length;i++)await one(files[i],i,files.length);say('Done. Your video is playing on the TV.');setTimeout(()=>location.reload(),1200);}}
catch(err){{bar.hidden=true;say(err.message,true);}}}};
for(const b of document.querySelectorAll('button.del'))b.onclick=async()=>{{if(!confirm('Delete this video from the TV?'))return;
try{{await send('POST','/videos/delete?n='+nonce+'&id='+b.dataset.id,'');location.reload();}}catch(err){{say(err.message,true);}}}};
</script></body></html>"#,
        limit = MAX_VIDEO_BYTES / 1024 / 1024 / 1024,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffer_accepts_mp4_with_h264_or_hevc_only() {
        // stsd, version/flags, entry count, entry size, then the codec tag.
        let entry = |tag: &[u8]| {
            let mut v = b"\0\0\0\x40stsd\0\0\0\0\0\0\0\x01\0\0\0\x30".to_vec();
            v.extend_from_slice(tag);
            v
        };
        let mut s = Sniffer::default();
        s.feed(b"\0\0\0\x18ftypisom\0\0\0\0 random avc1 bytes");
        let avc = entry(b"avc1");
        // Split across chunks, as a network read would.
        s.feed(&avc[..7]);
        s.feed(&avc[7..]);
        assert_eq!(s.verdict(), Ok(false));
        let mut s = Sniffer::default();
        s.feed(b"\0\0\0\x18ftypqt  ");
        s.feed(&entry(b"mp4a"));
        s.feed(&entry(b"hvc1"));
        assert_eq!(s.verdict(), Ok(true));
        // A stray tag outside stsd does not count.
        let mut s = Sniffer::default();
        s.feed(b"\0\0\0\x18ftypisom....avc1....");
        s.feed(&entry(b"vp09"));
        assert!(s.verdict().is_err());
        let mut s = Sniffer::default();
        s.feed(b"\x1aE\xdf\xa3 webm");
        s.feed(&entry(b"avc1"));
        assert!(s.verdict().is_err());
    }

    /// One HTTP request against the server; returns status line and body.
    fn call(port: u16, method: &str, target: &str, body: &[u8]) -> (String, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            stream,
            "{method} {target} HTTP/1.1\r\nHost: tv\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(body).unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        let status = reply.lines().next().unwrap_or("").to_owned();
        let body = reply.split_once("\r\n\r\n").map_or("", |p| p.1).to_owned();
        (status, body)
    }

    #[test]
    fn upload_page_receives_lists_and_deletes_videos() {
        let dir = std::env::temp_dir().join(format!("luma-upload-{}", random_hex()));
        fs::create_dir_all(&dir).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, events) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let server = {
            let (dir, stop) = (dir.clone(), Arc::clone(&stop));
            thread::spawn(move || serve(listener, &dir, "secret", &tx, &stop))
        };
        // Wrong or missing nonce.
        assert!(call(port, "GET", "/videos?n=nope", b"").0.contains("403"));
        // Not a video.
        let (status, body) = call(
            port,
            "PUT",
            "/videos/upload?n=secret&name=x",
            b"hello world!!!",
        );
        assert!(status.contains("422"), "{status}");
        assert!(body.contains("MP4"));
        // A small H.264 MP4: ftyp, then an stsd entry.
        let mut mp4 = b"\0\0\0\x18ftypisom\0\0\0\0isomavc1".to_vec();
        mp4.extend_from_slice(b"\0\0\0\x40stsd\0\0\0\0\0\0\0\x01\0\0\0\x30avc1");
        mp4.extend(std::iter::repeat_n(7u8, 300_000));
        let (status, body) = call(
            port,
            "PUT",
            "/videos/upload?n=secret&name=Beach%20day",
            &mp4,
        );
        assert!(status.contains("200"), "{status} {body}");
        let id = serde_json::from_str::<serde_json::Value>(&body).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(fs::read(video_file(&dir, &id)).unwrap(), mp4);
        let saved = load(&dir);
        assert_eq!(saved.len(), 1);
        assert_eq!(
            (saved[0].name.as_str(), saved[0].hevc),
            ("Beach day", false)
        );
        // Preview pictures must be JPEG.
        let target = format!("/videos/image?n=secret&id={id}&kind=thumb");
        assert!(call(port, "PUT", &target, b"GIF89a....").0.contains("422"));
        assert!(
            call(port, "PUT", &target, b"\xff\xd8\xff\xe0 jpeg")
                .0
                .contains("200")
        );
        assert!(image_file(&dir, &id, false).is_file());
        // The page lists it, escaped.
        let (status, page) = call(port, "GET", "/videos?n=secret", b"");
        assert!(status.contains("200"));
        assert!(page.contains("Beach day") && page.contains(&id));
        // Done tells the TV to play it.
        call(port, "POST", &format!("/videos/done?n=secret&id={id}"), b"");
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(5)),
            Ok(Event::Added(added)) if added == id
        ));
        // Delete removes the files and the entry.
        call(
            port,
            "POST",
            &format!("/videos/delete?n=secret&id={id}"),
            b"",
        );
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(5)),
            Ok(Event::Removed)
        ));
        assert!(load(&dir).is_empty());
        assert!(!video_file(&dir, &id).exists());
        assert!(!image_file(&dir, &id, false).exists());
        stop.store(true, Ordering::Relaxed);
        server.join().unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_and_ids_are_cleaned() {
        assert_eq!(clean_name("  Beach\n trip  "), "Beach trip");
        assert_eq!(clean_name(""), "My video");
        assert_eq!(clean_name(&"x".repeat(100)).len(), NAME_LIMIT);
        assert!(valid_id(&"a".repeat(32)));
        assert!(!valid_id("../../etc/passwd"));
        assert!(!valid_id(&"A".repeat(32)));
    }

    #[test]
    fn index_skips_missing_files_and_bad_ids() {
        let dir = std::env::temp_dir().join(format!("luma-videos-{}", random_hex()));
        fs::create_dir_all(&dir).unwrap();
        let good = "0123456789abcdef0123456789abcdef";
        fs::write(video_file(&dir, good), b"x").unwrap();
        let entries = vec![
            Entry {
                id: good.into(),
                name: "Good".into(),
                hevc: false,
            },
            Entry {
                id: "fedcba9876543210fedcba9876543210".into(),
                name: "Missing".into(),
                hevc: false,
            },
            Entry {
                id: "../escape".into(),
                name: "Bad".into(),
                hevc: false,
            },
        ];
        save(&dir, &entries).unwrap();
        assert_eq!(load(&dir), entries[..1].to_vec());
        let _ = fs::remove_dir_all(&dir);
    }
}
