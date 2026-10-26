//! Reusable on-LAN, phone-only input for Luma (Home Assistant, Moonlight).
//! No secrets in logs, preferences or URLs. QR links contain only a random nonce.
use rand::{RngCore, rngs::OsRng};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};
use url::form_urlencoded;

#[derive(Clone)]
pub enum FieldType {
    Text,
    Secret,
}
#[derive(Clone)]
pub struct Field {
    pub label: String,
    pub name: String,
    pub kind: FieldType,
    pub max_len: usize,
    pub placeholder: String,
}
#[derive(Clone)]
pub struct Form {
    pub title: String,
    pub hint: String,
    pub help: Option<(String, String)>,
    pub fields: Vec<Field>,
    pub submit_label: String,
    pub success_title: String,
    pub success_message: String,
    pub route: String,
}
pub enum FormResult<T> {
    Page(HttpResponse),
    Accepted(T, HttpResponse),
    Rejected(String, HttpResponse),
}
pub struct HttpResponse {
    pub status: &'static str,
    pub page: String,
}
impl HttpResponse {
    pub fn new(status: &'static str, page: String) -> Self {
        Self { status, page }
    }
}
pub fn random_hex() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut different = (a.len() ^ b.len()) as u8;
    for i in 0..a.len().max(b.len()).min(4096) {
        different |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0);
    }
    different == 0 && a.len() == b.len()
}
pub fn parse_form(data: &str) -> HashMap<String, String> {
    form_urlencoded::parse(data.as_bytes())
        .into_owned()
        .collect()
}
pub fn encode_form(pairs: &[(&str, &str)]) -> String {
    let mut s = form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
        s.append_pair(k, v);
    }
    s.finish()
}
fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
pub fn html_page(title: &str, body: &str) -> String {
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="referrer" content="no-referrer"><title>Luma</title><style>
    :root{{color-scheme:dark}}body{{background:#101115;color:#f6f6f8;font:500 19px/1.5 system-ui,sans-serif;max-width:520px;margin:10vh auto;padding:28px}}
    h1{{font-size:30px;font-weight:750;letter-spacing:-.04em;margin:0 0 12px}}
    p{{color:#c2c3cd;margin:0 0 26px}}label{{display:block;margin-top:18px;font-size:18px;font-weight:650}}
    input{{box-sizing:border-box;width:100%;margin-top:8px;padding:17px 16px;border:1px solid #62646d;border-radius:14px;background:#25262d;color:white;font-size:18px}}
    input:focus{{outline:3px solid #9b85ff}}button{{width:100%;padding:19px 24px;background:linear-gradient(105deg,#b29bff,#77c8ff);border:0;border-radius:15px;color:#101115;font-size:20px;font-weight:800;margin-top:26px}}
    a{{color:#b7caff;word-break:break-word}}.error{{color:#ffc1c1;background:#512d34;border-radius:12px;padding:12px 16px;margin:18px 0}}
    </style></head><body><h1>{}</h1>{body}</body></html>"#,
        escape_html(title)
    )
}
fn form_page(spec: &Form, nonce: &str, error: Option<&str>) -> String {
    let mut content = format!("<p>{}</p>", escape_html(&spec.hint));
    if let Some((label, url)) = &spec.help
        && (url.starts_with("https://") || url.starts_with("http://"))
    {
        content.push_str(&format!(
            "<p><a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">{}</a></p>",
            escape_html(url),
            escape_html(label)
        ));
    }
    if let Some(error) = error {
        content.push_str(&format!(
            "<div role=\"alert\" class=\"error\">{}</div>",
            escape_html(error)
        ));
    }
    content.push_str(&format!(
        "<form method=\"post\" action=\"{}\"><input type=\"hidden\" name=\"n\" value=\"{}\">",
        escape_html(&spec.route),
        nonce
    ));
    for field in &spec.fields {
        let ty = match field.kind {
            FieldType::Text => "text",
            FieldType::Secret => "password",
        };
        // All field metadata is caller-controlled, but always HTML-escaped.
        content.push_str(&format!("<label>{}<input type=\"{}\" name=\"{}\" placeholder=\"{}\" maxlength=\"{}\" required autocomplete=\"off\"></label>",
            escape_html(&field.label),ty,escape_html(&field.name),
            escape_html(&field.placeholder),field.max_len.min(4096)));
    }
    content.push_str(&format!(
        "<button type=\"submit\">{}</button></form>",
        escape_html(&spec.submit_label)
    ));
    html_page(&spec.title, &content)
}
pub struct Session<T, F: Fn(&HashMap<String, String>) -> Result<T, String>> {
    spec: Form,
    nonce: String,
    used: bool,
    validator: F,
}
impl<T, F: Fn(&HashMap<String, String>) -> Result<T, String>> Session<T, F> {
    pub fn new(spec: Form, validator: F) -> Self {
        Self {
            spec,
            nonce: random_hex(),
            used: false,
            validator,
        }
    }
    pub fn url(&self, base: &str) -> String {
        format!("{base}{}?n={}", self.spec.route, self.nonce)
    }
    pub fn route(&self) -> &str {
        &self.spec.route
    }
    fn good_nonce(&self, value: &str) -> bool {
        !self.used && constant_time_eq(value, &self.nonce)
    }
    pub fn page(&self, query: &str) -> HttpResponse {
        let nonce = parse_form(query).remove("n").unwrap_or_default();
        if !self.good_nonce(&nonce) {
            return HttpResponse::new(
                "403 Forbidden",
                html_page("Link expired", "<p>Show a new QR on your TV.</p>"),
            );
        }
        HttpResponse::new("200 OK", form_page(&self.spec, &self.nonce, None))
    }
    pub fn submit(&mut self, body: &str) -> FormResult<T> {
        let fields = parse_form(body);
        if !self.good_nonce(fields.get("n").map(String::as_str).unwrap_or("")) {
            return FormResult::Page(HttpResponse::new(
                "403 Forbidden",
                html_page("Link expired", "<p>Show a new QR on your TV.</p>"),
            ));
        }
        let mut validated = HashMap::new();
        let error = forbidden_field(&fields, &self.spec).or_else(|| {
            for field in &self.spec.fields {
                let Some(value) = fields.get(&field.name) else {
                    return Some("Please fill in every field".to_owned());
                };
                if value.is_empty()
                    || value.len() > field.max_len
                    || value.chars().any(|c| c.is_control())
                {
                    return Some(format!("Invalid {}", field.label));
                }
                validated.insert(field.name.clone(), value.clone());
            }
            None
        });
        let result = if let Some(error) = error {
            Err(error)
        } else {
            (self.validator)(&validated)
        };
        match result {
            Ok(value) => {
                self.used = true;
                FormResult::Accepted(
                    value,
                    HttpResponse::new(
                        "200 OK",
                        html_page(
                            &self.spec.success_title,
                            &format!("<p>{}</p>", escape_html(&self.spec.success_message)),
                        ),
                    ),
                )
            }
            Err(message) => FormResult::Rejected(
                message.clone(),
                HttpResponse::new(
                    "400 Bad Request",
                    form_page(&self.spec, &self.nonce, Some(&message)),
                ),
            ),
        }
    }
}
fn forbidden_field(fields: &HashMap<String, String>, spec: &Form) -> Option<String> {
    if fields.len() > spec.fields.len() + 1 {
        return Some("Invalid form fields".into());
    }
    for name in fields.keys() {
        if name != "n" && !spec.fields.iter().any(|f| &f.name == name) {
            return Some("Invalid form fields".into());
        }
    }
    None
}
pub fn lan_ip() -> Option<String> {
    if let Ok(sock) = UdpSocket::bind("0.0.0.0:0")
        && sock.connect("192.168.1.1:80").is_ok()
        && let Ok(SocketAddr::V4(addr)) = sock.local_addr()
        && !addr.ip().is_loopback()
    {
        return Some(addr.ip().to_string());
    }
    if_addrs::get_if_addrs()
        .ok()?
        .into_iter()
        .find_map(|iface| {
            if let std::net::IpAddr::V4(ip) = iface.ip()
                && ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
            {
                return Some(ip.to_string());
            }
            None
        })
}
pub fn bind_listener(ip: &str) -> Result<(TcpListener, u16), String> {
    for port in 47821..=47830 {
        if let Ok(listener) = TcpListener::bind((ip, port)) {
            listener
                .set_nonblocking(true)
                .map_err(|_| "Cannot configure phone link listener")?;
            return Ok((listener, port));
        }
    }
    Err("No available HTTP port for phone sign-in".into())
}
pub fn read_request(stream: &mut TcpStream) -> Result<(String, String, String), &'static str> {
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(3))).ok();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    let header_end = loop {
        let n = stream
            .read(&mut chunk)
            .map_err(|_| "HTTP request timeout")?;
        if n == 0 {
            return Err("Incomplete request");
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > 18000 {
            return Err("Request too large");
        }
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = std::str::from_utf8(&buf[..header_end]).map_err(|_| "Malformed headers")?;
    let mut parts = head.split("\r\n");
    let mut first = parts
        .next()
        .ok_or("Missing HTTP method")?
        .split_whitespace();
    let method = first.next().ok_or("Missing HTTP method")?.to_string();
    let path = first.next().ok_or("Missing HTTP path")?.to_string();
    if !matches!(method.as_str(), "GET" | "POST") || !path.starts_with('/') || path.len() > 4096 {
        return Err("Invalid HTTP request");
    }
    let mut size = 0usize;
    let mut content_form = false;
    for line in parts {
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("content-length") {
                size = value.trim().parse().map_err(|_| "Invalid request size")?;
            }
            if key.eq_ignore_ascii_case("transfer-encoding") {
                return Err("Chunked requests not supported");
            }
            if key.eq_ignore_ascii_case("content-type")
                && value
                    .trim()
                    .starts_with("application/x-www-form-urlencoded")
            {
                content_form = true;
            }
        }
    }
    if size > 12000 {
        return Err("Form too large");
    }
    if method == "POST" && !content_form {
        return Err("Form content type required");
    }
    while buf.len() - header_end < size {
        let n = stream.read(&mut chunk).map_err(|_| "HTTP form timeout")?;
        if n == 0 {
            return Err("Incomplete form");
        }
        if buf.len() + n > 18000 {
            return Err("Form too large");
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = std::str::from_utf8(&buf[header_end..header_end + size])
        .map_err(|_| "Malformed form")?
        .to_string();
    Ok((method, path, body))
}
pub fn write_response(stream: &mut TcpStream, response: &HttpResponse) {
    let _ = write!(
        stream,
        "HTTP/1.1 {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; form-action 'self'\r\nConnection: close\r\n\r\n{}",
        response.status,
        response.page.len(),
        response.page
    );
    let _ = stream.flush();
}
pub fn qr_matrix(content: &str) -> Vec<Vec<bool>> {
    let Ok(code) = qrcode::QrCode::new(content.as_bytes()) else {
        return Vec::new();
    };
    (0..code.width())
        .map(|y| {
            (0..code.width())
                .map(|x| code[(x, y)] == qrcode::Color::Dark)
                .collect()
        })
        .collect()
}
pub fn draw_qr(out: &mut crate::view::Scene, modules: &[Vec<bool>], rect: crate::state::Rect) {
    if modules.is_empty() {
        return;
    }
    out.shape(rect, 18., [1., 1., 1., 1.]);
    let count = modules.len();
    let module = (rect.w - 24.) / count as f32;
    for (y, row) in modules.iter().enumerate() {
        let mut x = 0;
        while x < count {
            if !row[x] {
                x += 1;
                continue;
            }
            let from = x;
            while x < count && row[x] {
                x += 1;
            }
            out.shape(
                crate::state::Rect::new(
                    rect.x + 12. + from as f32 * module,
                    rect.y + 12. + y as f32 * module,
                    (x - from) as f32 * module + 0.04,
                    module + 0.04,
                ),
                0.,
                [0.051, 0.051, 0.059, 1.],
            );
        }
    }
}
/// Standalone form server for Moonlight or any other phone-only credential.
/// The listener and verifier live on a worker thread, never on SDL/GL.
pub fn spawn_form<T, F>(
    form: Form,
    validate: F,
    on_success: Sender<T>,
    on_error: Sender<String>,
    canceled: Arc<AtomicBool>,
) -> Receiver<Result<String, String>>
where
    T: Send + 'static,
    F: Fn(&HashMap<String, String>) -> Result<T, String> + Send + 'static,
{
    let (ready_tx, ready_rx) = mpsc::channel();
    thread::spawn(move || {
        let Some(ip) = lan_ip() else {
            let _ = ready_tx.send(Err("TV has no LAN address".into()));
            return;
        };
        let (listener, port) = match bind_listener(&ip) {
            Ok(v) => v,
            Err(e) => {
                let _ = ready_tx.send(Err(e));
                return;
            }
        };
        let session = Session::new(form, validate);
        let link = session.url(&format!("http://{ip}:{port}"));
        let _ = ready_tx.send(Ok(link));
        serve_form(
            listener,
            session,
            on_success,
            on_error,
            canceled,
            Duration::from_secs(300),
        );
    });
    ready_rx
}
pub fn serve_form<T, F>(
    listener: TcpListener,
    mut session: Session<T, F>,
    success: Sender<T>,
    errors: Sender<String>,
    canceled: Arc<AtomicBool>,
    lifetime: Duration,
) where
    F: Fn(&HashMap<String, String>) -> Result<T, String>,
{
    let begin = Instant::now();
    while !canceled.load(Ordering::Relaxed) && begin.elapsed() < lifetime {
        let (mut stream, _) = match listener.accept() {
            Ok(s) => s,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(75));
                continue;
            }
            Err(_) => break,
        };
        let (method, route, body) = match read_request(&mut stream) {
            Ok(v) => v,
            Err(_) => {
                write_response(
                    &mut stream,
                    &HttpResponse::new(
                        "400 Bad Request",
                        html_page("Invalid request", "<p>Return to your TV.</p>"),
                    ),
                );
                continue;
            }
        };
        let (path, query) = route.split_once('?').unwrap_or((&route, ""));
        if method == "GET" && path == session.route() {
            write_response(&mut stream, &session.page(query));
        } else if method == "POST" && path == session.route() {
            match session.submit(&body) {
                FormResult::Accepted(value, page) => {
                    write_response(&mut stream, &page);
                    let _ = success.send(value);
                    return;
                }
                FormResult::Rejected(message, page) => {
                    let _ = errors.send(message);
                    write_response(&mut stream, &page);
                }
                FormResult::Page(page) => write_response(&mut stream, &page),
            }
        } else if method == "GET" && path == "/" {
            write_response(
                &mut stream,
                &HttpResponse::new(
                    "200 OK",
                    html_page("Luma Phone Link", "<p>Scan the QR on your TV.</p>"),
                ),
            );
        } else {
            write_response(
                &mut stream,
                &HttpResponse::new(
                    "404 Not Found",
                    html_page("Not found", "<p>Return to your TV.</p>"),
                ),
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Form {
        Form {
            title: "Set up".into(),
            hint: "Paste your key".into(),
            help: Some(("Get a key".into(), "https://example.com/".into())),
            fields: vec![Field {
                label: "API key".into(),
                name: "key".into(),
                kind: FieldType::Secret,
                max_len: 100,
                placeholder: "".into(),
            }],
            submit_label: "Send to TV".into(),
            success_title: "Accepted".into(),
            success_message: "Return to TV".into(),
            route: "/token".into(),
        }
    }
    #[test]
    fn form_validation_retry_and_one_time_nonce() {
        let mut session = Session::new(sample(), |fields: &HashMap<String, String>| {
            if fields.get("key").map(String::as_str) != Some("correct") {
                return Err("Invalid key".into());
            }
            Ok(42)
        });
        let link = session.url("http://localhost:47821");
        let nonce = link.split("n=").nth(1).unwrap();
        assert_eq!(session.page(&format!("n={nonce}")).status, "200 OK");
        let bad = format!("n={nonce}&key=bad");
        match session.submit(&bad) {
            FormResult::Rejected(_, r) => assert!(r.page.contains("Invalid key")),
            _ => panic!("expected validation error"),
        }
        assert_eq!(session.page(&format!("n={nonce}")).status, "200 OK");
        match session.submit(&format!("n={nonce}&key=correct")) {
            FormResult::Accepted(42, _) => {}
            _ => panic!("expected success"),
        }
        assert_eq!(session.page(&format!("n={nonce}")).status, "403 Forbidden");
        assert!(matches!(session.submit(&bad), FormResult::Page(_)));
    }
    #[test]
    fn nonce_and_html_escape() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
        let html = form_page(&sample(), "nonce", Some("<script>bad</script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>bad"));
        assert!(html.contains("type=\"password\""));
    }
    #[test]
    fn rejects_oversized_http_form_before_reading_content() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = thread::spawn(move || {
            let mut stream = TcpStream::connect(addr).unwrap();
            stream.write_all(b"POST /token HTTP/1.1\r\nHost: localhost\r\nContent-Length: 12001\r\nContent-Type: application/x-www-form-urlencoded\r\n\r\n").unwrap();
            stream.flush().unwrap();
        });
        let (mut stream, _) = listener.accept().unwrap();
        assert!(matches!(read_request(&mut stream), Err("Form too large")));
        client.join().unwrap();
    }
    #[test]
    fn rejects_extra_form_fields_without_running_validator() {
        let mut session = Session::new(sample(), |_values: &HashMap<String, String>| {
            panic!("Validator must never receive unexpected fields");
            #[allow(unreachable_code)]
            Ok(())
        });
        let url = session.url("http://127.0.0.1:47821");
        let nonce = url.split("n=").nth(1).unwrap();
        let body = format!("n={nonce}&key=value&extra=sneaky");
        assert!(matches!(session.submit(&body), FormResult::Rejected(_, _)));
        assert_eq!(session.page(&format!("n={nonce}")).status, "200 OK");
    }
    #[test]
    fn qr_is_generated() {
        let qr = qr_matrix("http://192.168.1.206:47821/");
        assert!(!qr.is_empty());
        assert_eq!(qr.len(), qr[0].len());
    }
}
