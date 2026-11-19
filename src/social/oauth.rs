//! Personal developer-app OAuth. Secrets and refresh tokens never enter saved chat data.
use super::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{rngs::OsRng, RngCore};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{io::Read, net::TcpListener, sync::Mutex};

pub const LOCAL_CALLBACK: &str = "http://127.0.0.1:43821/oauth/callback";
pub const CALLBACK_PAGE: &str = include_str!("../../sepia-oauth.html");
static REFRESH_LOCK: Mutex<()> = Mutex::new(());

#[derive(Default, Serialize, Deserialize)]
struct Grant {
    client_id: String,
    secret: String,
    access_token: String,
    refresh_token: String,
    expires_at: i64,
}
pub fn supported(platform: &str) -> bool {
    matches!(platform, "Tumblr" | "Threads" | "Blogger")
}
fn entry(platform: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("SepiaChat.OAuth", platform).map_err(|e| e.to_string())
}
pub fn forget(platform: &str) -> Result<(), String> {
    match entry(platform)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
fn load(platform: &str) -> Result<Option<Grant>, String> {
    match entry(platform)?.get_password() {
        Ok(s) => serde_json::from_str(&s)
            .map(Some)
            .map_err(|_| "Saved sign-in data is unreadable. Reconnect this account.".into()),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("Windows could not open the saved sign-in data.".into()),
    }
}
fn store(platform: &str, grant: &Grant) -> Result<(), String> {
    entry(platform)?
        .set_password(&serde_json::to_string(grant).map_err(|e| e.to_string())?)
        .map_err(|_| "Windows could not save the sign-in. Please reconnect.".to_string())?;
    network::save_credential(platform, &grant.access_token)
}
fn random() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
fn endpoints(platform: &str) -> Result<(&'static str, &'static str, &'static str), String> {
    Ok(match platform {
        "Tumblr" => (
            "https://www.tumblr.com/oauth2/authorize",
            "https://api.tumblr.com/v2/oauth2/token",
            "basic write offline_access",
        ),
        "Threads" => (
            "https://threads.com/oauth/authorize",
            "https://graph.threads.com/oauth/access_token",
            "threads_basic,threads_content_publish",
        ),
        "Instagram" => (
            "https://www.instagram.com/oauth/authorize",
            "https://api.instagram.com/oauth/access_token",
            "instagram_business_basic,instagram_business_content_publish",
        ),
        "X" => (
            "https://x.com/i/oauth2/authorize",
            "https://api.x.com/2/oauth2/token",
            "tweet.read tweet.write users.read media.write offline.access",
        ),
        "Blogger" => (
            "https://accounts.google.com/o/oauth2/v2/auth",
            "https://oauth2.googleapis.com/token",
            "https://www.googleapis.com/auth/blogger",
        ),
        _ => return Err("Browser sign-in is not available for this platform.".into()),
    })
}
fn redirect(platform: &str, a: &Account) -> Result<String, String> {
    if matches!(platform, "X" | "Blogger") {
        return Ok(LOCAL_CALLBACK.into());
    }
    let uri = a.redirect_uri.trim();
    let parsed = reqwest::Url::parse(uri)
        .map_err(|_| "Enter your website's HTTPS callback page address.".to_string())?;
    if !url_valid(uri) || parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(
            "Callback must be an HTTPS address on your website without a query or fragment.".into(),
        );
    }
    Ok(uri.into())
}
fn authorization_url(
    platform: &str,
    a: &Account,
    redirect: &str,
    state: &str,
    verifier: &str,
) -> Result<String, String> {
    let (auth, _, scope) = endpoints(platform)?;
    let mut url = reqwest::Url::parse(auth).map_err(|e| e.to_string())?;
    url.query_pairs_mut().extend_pairs([
        ("client_id", a.client_id.trim()),
        ("redirect_uri", redirect),
        ("response_type", "code"),
        ("scope", scope),
        ("state", state),
    ]);
    if matches!(platform, "X" | "Blogger") {
        url.query_pairs_mut()
            .append_pair(
                "code_challenge",
                &URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
            )
            .append_pair("code_challenge_method", "S256");
    }
    if platform == "Blogger" {
        url.query_pairs_mut()
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent");
    }
    Ok(url.into())
}
// Check both the redirect destination and a fresh per-attempt state. Never log this URL.
fn callback_code(input: &str, redirect: &str, state: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(input.trim())
        .map_err(|_| "Paste the full browser address after approving access.".to_string())?;
    let mut base = url.clone();
    base.set_query(None);
    base.set_fragment(None);
    if base.as_str() != redirect && base.as_str() != LOCAL_CALLBACK {
        return Err("That address is not this sign-in's callback page.".into());
    }
    let pairs: Vec<_> = url.query_pairs().collect();
    let values = |key: &str| {
        pairs
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_ref())
            .collect::<Vec<_>>()
    };
    if values("state") != [state] {
        return Err("Sign-in state did not match. Cancel and connect again.".into());
    }
    if !values("error").is_empty() {
        return Err("Access was declined by the account or provider. Check the developer app settings and reconnect.".into());
    }
    let codes = values("code");
    if codes.len() != 1 || codes[0].is_empty() {
        return Err("The callback did not contain an authorization code.".into());
    }
    Ok(codes[0].into())
}
pub fn start(
    platform: &str,
    a: Account,
    mut secret: String,
    tx: mpsc::Sender<Event>,
    ctx: egui::Context,
) -> Result<mpsc::Sender<String>, String> {
    let redirect = redirect(platform, &a)?;
    if a.client_id.trim().is_empty() {
        return Err("Enter this platform's application client ID first.".into());
    }
    if secret.trim().is_empty() {
        if let Some(saved) = load(platform)? {
            if saved.client_id == a.client_id.trim() {
                secret = saved.secret;
            }
        }
    }
    if platform != "X" && secret.trim().is_empty() {
        return Err("Enter the application client secret first.".into());
    }
    let listener = TcpListener::bind("127.0.0.1:43821").map_err(|_| {
        "Cannot open sign-in port 43821. Close another Sepia sign-in attempt and retry.".to_string()
    })?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let state = random();
    let verifier = random();
    let url = authorization_url(platform, &a, &redirect, &state, &verifier)?;
    let (send, receive) = mpsc::channel();
    let platform = platform.to_string();
    ctx.open_url(egui::OpenUrl::new_tab(url));
    std::thread::spawn(move || {
        let result = (|| {
            let code = wait_callback(
                &listener,
                &receive,
                &redirect,
                &state,
                Duration::from_secs(300),
            )?;
            exchange(&platform, &a, &secret, &redirect, &verifier, &code)
        })();
        let _ = tx.send(Event::Authorized(platform, result));
        ctx.request_repaint();
    });
    Ok(send)
}
fn wait_callback(
    listener: &TcpListener,
    receive: &mpsc::Receiver<String>,
    redirect: &str,
    state: &str,
    timeout: Duration,
) -> Result<String, String> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        match receive.try_recv() {
            Ok(text) if text.is_empty() => return Err("Connection cancelled.".into()),
            Ok(text) => return callback_code(&text, redirect, state),
            Err(mpsc::TryRecvError::Disconnected) => return Err("Connection cancelled.".into()),
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(300)));
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 1024];
            while bytes.len() < 16384 {
                match stream.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => bytes.extend_from_slice(&buffer[..n]),
                }
                if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&bytes);
            let target = request
                .lines()
                .next()
                .and_then(|l| l.strip_prefix("GET "))
                .and_then(|s| s.split_whitespace().next())
                .unwrap_or("");
            let result = callback_code(&format!("http://127.0.0.1:43821{target}"), redirect, state);
            let message = if result.is_ok() {
                "Authorization received. Return to Sepia to see whether connection succeeded."
            } else {
                "This callback was not accepted. Return to Sepia and retry the connection."
            };
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}", message.len(), message);
            if result.is_ok() {
                return result;
            }
            // Ignore unrelated/malicious local requests; a valid attempt can still finish.
            let denied =
                reqwest::Url::parse(&format!("http://127.0.0.1:43821{target}")).is_ok_and(|url| {
                    let states: Vec<_> = url
                        .query_pairs()
                        .filter(|(k, _)| k == "state")
                        .map(|(_, v)| v.into_owned())
                        .collect();
                    url.path() == "/oauth/callback"
                        && states == [state]
                        && url.query_pairs().any(|(k, _)| k == "error")
                });
            if denied {
                return result;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err("Sign-in timed out after five minutes. Click Connect to try again.".into())
}
fn string(v: &Value, key: &str) -> Result<String, String> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("The platform did not return {key}. Reconnect the account."))
}
fn identifier(v: &Value) -> String {
    v.as_str()
        .map(str::to_owned)
        .or_else(|| v.as_u64().map(|n| n.to_string()))
        .unwrap_or_default()
}
fn apply_token(grant: &mut Grant, value: &Value, default_lifetime: i64) -> Result<(), String> {
    grant.access_token = string(value, "access_token")?;
    if let Some(refresh) = value["refresh_token"].as_str() {
        grant.refresh_token = refresh.into();
    }
    grant.expires_at =
        chrono::Utc::now().timestamp() + value["expires_in"].as_i64().unwrap_or(default_lifetime);
    Ok(())
}
fn exchange(
    platform: &str,
    a: &Account,
    secret: &str,
    redirect: &str,
    verifier: &str,
    code: &str,
) -> Result<Vec<(String, String)>, String> {
    let c = network::client()?;
    let mut fields = vec![
        ("grant_type", "authorization_code"),
        ("client_id", a.client_id.trim()),
        ("redirect_uri", redirect),
        ("code", code),
    ];
    if platform != "X" {
        fields.push(("client_secret", secret.trim()));
    }
    if matches!(platform, "X" | "Blogger") {
        fields.push(("code_verifier", verifier));
    }
    let request = c.post(endpoints(platform)?.1);
    let value = if platform == "Instagram" {
        let form = fields
            .iter()
            .fold(reqwest::blocking::multipart::Form::new(), |f, (k, v)| {
                f.text((*k).to_owned(), (*v).to_owned())
            });
        network::response(request.multipart(form))?
    } else {
        network::response(request.form(&fields))?
    };
    let value = value
        .get("data")
        .and_then(Value::as_array)
        .and_then(|v| v.first())
        .unwrap_or(&value);
    let mut grant = Grant {
        client_id: a.client_id.trim().into(),
        secret: secret.trim().into(),
        ..Default::default()
    };
    apply_token(&mut grant, value, 3600)?;
    if matches!(platform, "Threads" | "Instagram") {
        let (host, kind) = if platform == "Threads" {
            (
                "https://graph.threads.com/access_token",
                "th_exchange_token",
            )
        } else {
            (
                "https://graph.instagram.com/access_token",
                "ig_exchange_token",
            )
        };
        let long = network::response(c.get(host).query(&[
            ("grant_type", kind),
            ("client_secret", grant.secret.as_str()),
            ("access_token", grant.access_token.as_str()),
        ]))?;
        apply_token(&mut grant, &long, 60 * 86400)?;
    }
    let accounts = discover(platform, &c, &grant.access_token)?;
    store(platform, &grant)?;
    Ok(accounts)
}
fn discover(
    platform: &str,
    c: &reqwest::blocking::Client,
    token: &str,
) -> Result<Vec<(String, String)>, String> {
    let url = match platform {
        "Tumblr" => "https://api.tumblr.com/v2/user/info",
        "Threads" => "https://graph.threads.com/v1.0/me?fields=id,username",
        "Instagram" => "https://graph.instagram.com/v27.0/me?fields=user_id,username",
        "X" => "https://api.x.com/2/users/me",
        "Blogger" => "https://www.googleapis.com/blogger/v3/users/self/blogs",
        _ => return Err("Unsupported platform".into()),
    };
    let value = network::response(c.get(url).bearer_auth(token))?;
    destinations(platform, &value)
}
fn destinations(platform: &str, value: &Value) -> Result<Vec<(String, String)>, String> {
    let result: Vec<_> = match platform {
        "Tumblr" => value["response"]["user"]["blogs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|v| v["admin"].as_bool() != Some(false))
            .filter_map(|v| {
                v["name"]
                    .as_str()
                    .map(|name| (format!("{name}.tumblr.com"), name.into()))
            })
            .collect(),
        "Blogger" => value["items"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| {
                (
                    identifier(&v["id"]),
                    v["name"].as_str().unwrap_or("Blog").into(),
                )
            })
            .collect(),
        "X" => vec![(
            identifier(&value["data"]["id"]),
            value["data"]["username"]
                .as_str()
                .unwrap_or("X account")
                .into(),
        )],
        _ => vec![(
            identifier(
                &value[if platform == "Instagram" {
                    "user_id"
                } else {
                    "id"
                }],
            ),
            value["username"].as_str().unwrap_or(platform).into(),
        )],
    };
    let result: Vec<_> = result
        .into_iter()
        .filter(|(id, _)| !id.is_empty())
        .collect();
    if result.is_empty() {
        Err("No publishing account or blog was found. Create one and reconnect.".into())
    } else {
        Ok(result)
    }
}
/// Called only during explicit Verify/Post actions, never on UI redraw or startup.
pub fn access_token(platform: &str, current: String) -> Result<String, String> {
    if !supported(platform) {
        return Ok(current);
    }
    let _guard = REFRESH_LOCK
        .lock()
        .map_err(|_| "Sign-in refresh is unavailable.".to_string())?;
    let Some(mut grant) = load(platform)? else {
        return Ok(current);
    };
    // A manually replaced token takes precedence over an older browser grant.
    if grant.access_token != current {
        return Ok(current);
    }
    let now = chrono::Utc::now().timestamp();
    let meta = matches!(platform, "Threads" | "Instagram");
    if grant.expires_at > now + if meta { 7 * 86400 } else { 120 } {
        return Ok(current);
    }
    let c = network::client()?;
    let value = if meta {
        let (url, kind) = if platform == "Threads" {
            (
                "https://graph.threads.com/refresh_access_token",
                "th_refresh_token",
            )
        } else {
            (
                "https://graph.instagram.com/refresh_access_token",
                "ig_refresh_token",
            )
        };
        network::response(c.get(url).query(&[
            ("grant_type", kind),
            ("access_token", grant.access_token.as_str()),
        ]))?
    } else {
        if grant.refresh_token.is_empty() {
            return Err("This sign-in has expired. Click Connect to sign in again.".into());
        }
        let mut fields = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", grant.refresh_token.as_str()),
            ("client_id", grant.client_id.as_str()),
        ];
        if platform != "X" {
            fields.push(("client_secret", grant.secret.as_str()));
        }
        network::response(c.post(endpoints(platform)?.1).form(&fields))?
    };
    apply_token(&mut grant, &value, if meta { 60 * 86400 } else { 3600 })?;
    store(platform, &grant)?;
    Ok(grant.access_token)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_rejects_wrong_origin_state_duplicates_and_denial() {
        let redirect = "https://game.example/sepia-oauth.html";
        assert_eq!(
            callback_code(
                &format!("{redirect}?code=abc%2Bdef&state=random#_"),
                redirect,
                "random"
            )
            .unwrap(),
            "abc+def"
        );
        assert!(callback_code(
            &format!("{LOCAL_CALLBACK}?code=a&state=random"),
            redirect,
            "random"
        )
        .is_ok());
        for bad in [
            "https://evil.example/?code=a&state=random",
            "https://game.example/sepia-oauth.html?code=a&state=wrong",
            "https://game.example/sepia-oauth.html?code=a&code=b&state=random",
            "https://game.example/sepia-oauth.html?code=a&state=random&state=random",
            "https://game.example/sepia-oauth.html?error=access_denied&state=random",
        ] {
            assert!(callback_code(bad, redirect, "random").is_err());
        }
    }
    #[test]
    fn pkce_and_scope_urls_do_not_include_secrets() {
        let a = Account {
            client_id: "client".into(),
            ..Default::default()
        };
        let url = authorization_url("X", &a, LOCAL_CALLBACK, "random", "verifier").unwrap();
        let u = reqwest::Url::parse(&url).unwrap();
        let q: BTreeMap<_, _> = u.query_pairs().collect();
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(
            q["code_challenge"],
            URL_SAFE_NO_PAD.encode(Sha256::digest(b"verifier"))
        );
        assert!(q["scope"].contains("media.write"));
        assert!(!url.contains("client_secret"));
        assert!(!url.contains("code_verifier"));
    }
    #[test]
    fn token_rotation_preserves_refresh_token_when_omitted() {
        let mut grant = Grant {
            refresh_token: "old".into(),
            ..Default::default()
        };
        apply_token(
            &mut grant,
            &serde_json::json!({"access_token":"new","expires_in":3600}),
            1,
        )
        .unwrap();
        assert_eq!(grant.refresh_token, "old");
        apply_token(
            &mut grant,
            &serde_json::json!({"access_token":"next","refresh_token":"rotated"}),
            1,
        )
        .unwrap();
        assert_eq!(grant.refresh_token, "rotated");
    }
    #[test]
    fn account_discovery_handles_multiple_blogs_and_numeric_ids() {
        assert_eq!(
            destinations(
                "Blogger",
                &serde_json::json!({"items":[{"id":"1","name":"One"},{"id":"2","name":"Two"}]})
            )
            .unwrap()
            .len(),
            2
        );
        assert_eq!(
            destinations(
                "Instagram",
                &serde_json::json!({"user_id":123,"username":"game"})
            )
            .unwrap()[0]
                .0,
            "123"
        );
        assert!(destinations("Blogger", &serde_json::json!({})).is_err());
    }
    #[test]
    fn loopback_receives_valid_callback_after_unrelated_request() {
        use std::net::TcpStream;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let (_send, receive) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            wait_callback(
                &listener,
                &receive,
                LOCAL_CALLBACK,
                "test-state",
                Duration::from_secs(3),
            )
        });
        for target in [
            "/favicon.ico",
            "/oauth/callback?code=accepted&state=test-state",
        ] {
            let mut stream = TcpStream::connect(address).unwrap();
            write!(stream, "GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            assert!(response.contains("HTTP/1.1 200"));
        }
        assert_eq!(thread.join().unwrap().unwrap(), "accepted");
    }
    #[test]
    fn cancel_and_timeout_stop_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let (send, receive) = mpsc::channel();
        send.send(String::new()).unwrap();
        assert!(wait_callback(
            &listener,
            &receive,
            LOCAL_CALLBACK,
            "s",
            Duration::from_secs(1)
        )
        .unwrap_err()
        .contains("cancelled"));
        assert!(
            wait_callback(&listener, &receive, LOCAL_CALLBACK, "s", Duration::ZERO)
                .unwrap_err()
                .contains("timed out")
        );
    }
}
