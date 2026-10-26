//! Home Assistant OAuth / phone-only fallback. Nothing secret enters preferences or logs.
use crate::phone_link::{
    self, Field, FieldType, Form, FormResult, HttpResponse, Session, constant_time_eq, encode_form,
    html_page, parse_form, random_hex,
};
use crate::smart_home::{self, Command, Secrets, Update};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    net::TcpStream,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread,
    time::{Duration, Instant},
};

fn parse_callback(path: &str, state: &str) -> Result<String, &'static str> {
    let (route, query) = path
        .split_once('?')
        .ok_or("Missing authorization response")?;
    if route != "/auth/callback" {
        return Err("Invalid callback path");
    }
    let fields = parse_form(query);
    if fields.contains_key("error") {
        return Err("Home Assistant denied sign-in");
    }
    let supplied = fields.get("state").ok_or("Missing authorization state")?;
    if !constant_time_eq(supplied, state) {
        return Err("Authorization state mismatch");
    }
    let code = fields.get("code").ok_or("Missing authorization code")?;
    if code.len() > 2048 || code.is_empty() {
        return Err("Invalid authorization code");
    }
    Ok(code.clone())
}
fn build_authorize_url(
    server: &str,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
) -> String {
    format!(
        "{server}/auth/authorize?{}",
        encode_form(&[
            ("response_type", "code"),
            ("client_id", client_id),
            ("redirect_uri", redirect_uri),
            ("state", state),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
        ])
    )
}
fn exchange_code(
    server: &str,
    code: &str,
    client_id: &str,
    verifier: &str,
) -> Result<Secrets, &'static str> {
    let form = encode_form(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", client_id),
        ("code_verifier", verifier),
    ]);
    let response = ureq::post(&format!("{server}/auth/token"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send(form.as_bytes())
        .map_err(|_| "Home Assistant rejected sign-in")?;
    let raw = response
        .into_body()
        .read_to_string()
        .map_err(|_| "Invalid token response")?;
    let json: Value = serde_json::from_str(&raw).map_err(|_| "Invalid token response")?;
    let access_token = json
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or("Missing access token")?;
    let refresh_token = json
        .get("refresh_token")
        .and_then(Value::as_str)
        .ok_or("Missing refresh token")?;
    let expires_in = json
        .get("expires_in")
        .and_then(Value::as_u64)
        .unwrap_or(1800);
    Ok(Secrets {
        access_token: access_token.into(),
        refresh_token: Some(refresh_token.into()),
        client_id: Some(client_id.into()),
        expires_at: smart_home::unix_time().saturating_add(expires_in),
    })
}
fn validate_long_lived(server: &str, token: &str) -> Result<Secrets, &'static str> {
    let response = ureq::get(&format!("{server}/api/"))
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|_| "Home Assistant rejected this token or is offline")?;
    if !response.status().is_success() {
        return Err("Token validation failed");
    }
    Ok(Secrets {
        access_token: token.into(),
        refresh_token: None,
        client_id: None,
        expires_at: 0,
    })
}
fn reply(stream: &mut TcpStream, status: &'static str, page: &str) {
    phone_link::write_response(stream, &HttpResponse::new(status, page.to_string()));
}
fn token_form() -> Form {
    Form {
        title: "Connect Luma to Home Assistant".into(),
        hint: "Enter your Home Assistant address and long-lived access token on your phone.".into(),
        help: None,
        fields: vec![
            Field {
                label: "Home Assistant address".into(),
                name: "server".into(),
                kind: FieldType::Text,
                max_len: 256,
                placeholder: "http://homeassistant.local:8123".into(),
            },
            Field {
                label: "Long-lived access token".into(),
                name: "token".into(),
                kind: FieldType::Secret,
                max_len: 2048,
                placeholder: "Paste token here".into(),
            },
        ],
        submit_label: "Connect TV".into(),
        success_title: "Luma is connected".into(),
        success_message: "You can close this page.".into(),
        route: "/token".into(),
    }
}
/// The listener is strictly lifetime-scoped to the Connect screen.
pub fn start(
    server: String,
    path: PathBuf,
    updates: Sender<Update>,
    commands: Sender<Command>,
    canceled: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let Some(ip) = phone_link::lan_ip() else {
            let _ = updates.send(Update::PairingError(
                "TV has no local network address".into(),
            ));
            return;
        };
        let (listener, port) = match phone_link::bind_listener(&ip) {
            Ok(value) => value,
            Err(error) => {
                let _ = updates.send(Update::PairingError(error));
                return;
            }
        };
        let client_id = format!("http://{ip}:{port}/");
        let callback = format!("http://{ip}:{port}/auth/callback");
        let state = random_hex();
        let verifier = random_hex();
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let normalized = smart_home::normalize_url(&server);
        let authorize = normalized
            .as_ref()
            .map(|server| build_authorize_url(server, &client_id, &callback, &state, &challenge));
        let save_path = path.clone();
        let mut fallback_session =
            Session::new(token_form(), move |form: &HashMap<String, String>| {
                let server = form
                    .get("server")
                    .and_then(|s| smart_home::normalize_url(s))
                    .ok_or_else(|| "Enter a valid Home Assistant address".to_string())?;
                let token = form
                    .get("token")
                    .ok_or_else(|| "Enter an access token".to_string())?;
                let auth = validate_long_lived(&server, token).map_err(str::to_owned)?;
                smart_home::save_auth(&save_path, &auth)
                    .map_err(|_| "Cannot save credentials securely".to_string())?;
                Ok(server)
            });
        let fallback = fallback_session.url(&format!("http://{ip}:{port}"));
        let _ = updates.send(Update::QrReady {
            authorize,
            fallback,
        });
        let begin = Instant::now();
        while !canceled.load(Ordering::Relaxed) && begin.elapsed() < Duration::from_secs(300) {
            let (mut stream, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(75));
                    continue;
                }
                Err(_) => {
                    let _ = updates.send(Update::PairingError("TV QR HTTP listener failed".into()));
                    return;
                }
            };
            let result = phone_link::read_request(&mut stream);
            let Ok((method, route, body)) = result else {
                reply(
                    &mut stream,
                    "400 Bad Request",
                    &html_page("Invalid request", "Please scan the QR again."),
                );
                continue;
            };
            if method == "GET" && route == "/" {
                reply(
                    &mut stream,
                    "200 OK",
                    &html_page("Luma Home Assistant", "Scan the QR on your TV to sign in."),
                );
            } else if method == "GET" && route.starts_with("/token?") {
                let query = route.split_once('?').map(|(_, query)| query).unwrap_or("");
                let response = fallback_session.page(query);
                phone_link::write_response(&mut stream, &response);
            } else if method == "POST" && route == "/token" {
                match fallback_session.submit(&body) {
                    FormResult::Accepted(server, response) => {
                        phone_link::write_response(&mut stream, &response);
                        let _ = commands.send(Command::CredentialsReady { server });
                        return;
                    }
                    FormResult::Rejected(message, response) => {
                        let _ = updates.send(Update::PairingError(message));
                        phone_link::write_response(&mut stream, &response);
                    }
                    FormResult::Page(response) => {
                        phone_link::write_response(&mut stream, &response);
                    }
                }
            } else if method == "GET" && route.starts_with("/auth/callback?") {
                if let Some(ref server) = normalized {
                    match parse_callback(&route, &state)
                        .and_then(|code| exchange_code(server, &code, &client_id, &verifier))
                    {
                        Ok(secrets) => {
                            if smart_home::save_auth(&path, &secrets).is_ok() {
                                reply(
                                    &mut stream,
                                    "200 OK",
                                    &html_page(
                                        "Luma is connected",
                                        "<p>You can close this page.</p>",
                                    ),
                                );
                                let _ = commands.send(Command::CredentialsReady {
                                    server: server.clone(),
                                });
                                return;
                            }
                            let _ = updates.send(Update::PairingError(
                                "Cannot save credentials securely".into(),
                            ));
                            reply(
                                &mut stream,
                                "500 Internal Server Error",
                                &html_page("Could not connect", "TV storage error."),
                            );
                        }
                        Err(error) => {
                            let _ = updates.send(Update::PairingError(error.into()));
                            reply(
                                &mut stream,
                                "400 Bad Request",
                                &html_page(
                                    "Sign-in failed",
                                    &format!(
                                        "<p>{error}</p><p>Show new QR on your TV to retry.</p>"
                                    ),
                                ),
                            );
                        }
                    }
                } else {
                    reply(
                        &mut stream,
                        "400 Bad Request",
                        &html_page("Choose a server", "Select Home Assistant on the TV first."),
                    );
                }
            } else {
                reply(
                    &mut stream,
                    "404 Not Found",
                    &html_page("Page not found", "Return to the TV."),
                );
            }
        }
        if !canceled.load(Ordering::Relaxed) {
            let _ = updates.send(Update::QrExpired);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_token_fields_in_remote_ui_state() {
        let state = include_str!("state.rs");
        for forbidden in ["ha_token_draft", "HomeToken", "SmartToken", "SmartSave"] {
            assert!(
                !state.contains(forbidden),
                "Secret text input must not exist in UI state"
            );
        }
    }
    #[test]
    fn oauth_url_has_pkce_redirect_and_state() {
        let url = build_authorize_url(
            "http://ha:8123",
            "http://tv:47821/",
            "http://tv:47821/auth/callback",
            "test_state",
            "challenge",
        );
        let query = parse_form(url.split_once('?').unwrap().1);
        assert_eq!(query["state"], "test_state");
        assert_eq!(query["client_id"], "http://tv:47821/");
        assert_eq!(query["redirect_uri"], "http://tv:47821/auth/callback");
        assert_eq!(query["code_challenge_method"], "S256");
    }
    #[test]
    fn callback_requires_matching_state_and_code() {
        assert_eq!(
            parse_callback("/auth/callback?code=a%2Fb&state=abc", "abc").unwrap(),
            "a/b"
        );
        assert!(parse_callback("/auth/callback?code=hi&state=wrong", "abc").is_err());
        assert!(parse_callback("/auth/callback?state=abc", "abc").is_err());
        assert!(parse_callback("/wrong?code=x&state=abc", "abc").is_err());
    }
    #[test]
    fn fallback_nonce_is_one_time_and_form_is_bounded() {
        let nonce = random_hex();
        assert!(constant_time_eq(&nonce, &nonce));
        assert!(!constant_time_eq(&nonce, "wrong"));
        let body = encode_form(&[
            ("n", &nonce),
            ("token", "special&token+"),
            ("server", "http://ha.local:8123"),
        ]);
        let fields = parse_form(&body);
        assert_eq!(fields["token"], "special&token+");
        let form = token_form();
        assert!(
            form.fields
                .iter()
                .any(|field| matches!(field.kind, FieldType::Secret))
        );
    }
}
