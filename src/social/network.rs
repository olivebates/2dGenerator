use super::*;
use reqwest::blocking::{Client, RequestBuilder};
use serde_json::{json, Value};

pub fn client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(45))
        .user_agent("SepiaChat/0.1 (personal game publisher)")
        .build()
        .map_err(|e| e.to_string())
}
pub fn effective_account(accounts: &BTreeMap<String, Account>, platform: &str) -> Account {
    let mut account = accounts.get(platform).cloned().unwrap_or_default();
    if let Some(hosting) = accounts.get("Instagram") {
        if account.cloud.is_empty() {
            account.cloud = hosting.cloud.clone();
        }
        if account.preset.is_empty() {
            account.preset = hosting.preset.clone();
        }
    }
    account
}
pub fn image_unavailable(platform: &str, account: &Account) -> Option<&'static str> {
    if matches!(platform, "Threads" | "Blogger")
        && (account.cloud.is_empty() || account.preset.is_empty())
    {
        Some("Text only: configure Shared image hosting to attach pictures on this platform.")
    } else {
        None
    }
}
pub fn credential(platform: &str) -> Result<String, String> {
    oauth::access_token(platform, stored_credential(platform)?)
}
pub fn stored_credential(platform: &str) -> Result<String, String> {
    keyring::Entry::new("SepiaChat.SoMe", platform)
        .map_err(|e| e.to_string())?
        .get_password()
        .map_err(|_| "No saved account credential. Open Link accounts.".into())
}
pub fn save_credential(platform: &str, value: &str) -> Result<(), String> {
    keyring::Entry::new("SepiaChat.SoMe", platform)
        .map_err(|e| e.to_string())?
        .set_password(value.trim())
        .map_err(|e| e.to_string())
}
pub fn delete_credential(platform: &str) -> Result<(), String> {
    oauth::forget(platform)?;
    keyring::Entry::new("SepiaChat.SoMe", platform)
        .map_err(|e| e.to_string())?
        .delete_credential()
        .map_err(|e| e.to_string())
}
pub fn response(request: RequestBuilder) -> Result<Value, String> {
    let r = request.send().map_err(|_| {
        "Network request failed or timed out. Check the platform before retrying.".to_string()
    })?;
    let status = r.status();
    let retry = r
        .headers()
        .get("retry-after")
        .and_then(|s| s.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let bytes = r
        .bytes()
        .map_err(|_| "Could not read the platform response. Check before retrying.".to_string())?;
    if !status.is_success() {
        let hint = match status.as_u16() {
            401 => "Authentication rejected. The saved token may be invalid, expired or for a different account. Reconnect or replace the service credential.",
            402 => "Payment/API credits required. Check billing and available credits in the service's developer console.",
            403 => "Access denied. Check publishing permissions, account restrictions and API access.",
            429 => "Rate limit reached. Wait before retrying.",
            _ => "Check credentials, permissions, request details and service limits.",
        };
        let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        let code = value["error"]["code"]
            .as_i64()
            .map(|code| format!("; provider code {code}"))
            .unwrap_or_default();
        let subcode = value["error"]["error_subcode"]
            .as_i64()
            .map(|code| format!("; subcode {code}"))
            .unwrap_or_default();
        return Err(format!("Platform returned HTTP {}{}{}{}. {hint} Check the platform before retrying a posting request.",status.as_u16(),code,subcode,if retry.is_empty(){String::new()}else{format!("; retry-after {retry}")}));
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| "Platform returned an unexpected response. Check before retrying.".into())
}
fn id(value: &str) -> Result<&str, String> {
    if !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        Ok(value)
    } else {
        Err("Enter a valid account or blog ID.".into())
    }
}
fn instance(a: &Account) -> Result<String, String> {
    let value = a.server.trim_end_matches('/');
    if !url_valid(value) {
        return Err("Enter the server's https:// URL.".into());
    }
    let u = reqwest::Url::parse(value).map_err(|e| e.to_string())?;
    if u.path() != "/" && !u.path().is_empty() {
        return Err("Use the server origin without a path.".into());
    }
    Ok(value.into())
}
pub fn ready(platform: &str, a: &Account) -> Result<(), String> {
    if manual(platform) {
        return Err("Manual publishing only.".into());
    }
    stored_credential(platform)?;
    match platform {
        "Facebook" | "Threads" | "Blogger" | "Instagram" | "Tumblr" => {
            id(&a.identity)?;
        }
        "Mastodon" => {
            instance(a)?;
        }
        "Bluesky" if a.identity.trim().is_empty() => {
            return Err("Enter your Bluesky handle.".into())
        }
        _ => {}
    }
    if platform == "Instagram" && (a.cloud.is_empty() || a.preset.is_empty()) {
        return Err("Configure Cloudinary hosting for the generated image card.".into());
    }
    Ok(())
}
pub fn verify(platform: &str, a: &Account) -> Result<String, String> {
    ready(platform, a)?;
    let c = client()?;
    let token = credential(platform)?;
    let v = match platform {
        "Facebook" => response(
            c.get(format!(
                "https://graph.facebook.com/v27.0/{}",
                id(&a.identity)?
            ))
            .query(&[("fields", "id,name")])
            .bearer_auth(token),
        )?,
        "Threads" => response(
            c.get("https://graph.threads.com/v1.0/me")
                .query(&[("fields", "id,username")])
                .bearer_auth(token),
        )?,
        "Instagram" => response(
            c.get(format!(
                "https://graph.instagram.com/v27.0/{}",
                id(&a.identity)?
            ))
            .query(&[("fields", "id,username")])
            .bearer_auth(token),
        )?,
        "Mastodon" => response(
            c.get(format!(
                "{}/api/v1/accounts/verify_credentials",
                instance(a)?
            ))
            .bearer_auth(token),
        )?,
        "Bluesky" => response(
            c.post("https://bsky.social/xrpc/com.atproto.server.createSession")
                .json(&json!({"identifier":a.identity,"password":token})),
        )?,
        "Tumblr" => response(
            c.get("https://api.tumblr.com/v2/user/info")
                .bearer_auth(token),
        )?,
        "Blogger" => response(
            c.get(format!(
                "https://www.googleapis.com/blogger/v3/blogs/{}",
                id(&a.identity)?
            ))
            .bearer_auth(token),
        )?,
        "X" => response(c.get("https://api.x.com/2/users/me").bearer_auth(token))?,
        _ => return Err("Unsupported destination".into()),
    };
    let label = v["name"]
        .as_str()
        .or(v["username"].as_str())
        .or(v["handle"].as_str())
        .or(v["data"]["username"].as_str())
        .unwrap_or("Account verified (publishing permission is checked when posting)");
    Ok(label.into())
}
// Detailed generated pictures are larger than the former text cards. Resize only
// the Bluesky upload copy to fit its blob budget; retain the original PNG locally.
fn bluesky_image(path: &str) -> Result<(Vec<u8>, u32, u32), String> {
    let mut picture =
        image::open(path).map_err(|_| "Could not open the shared image.".to_string())?;
    if picture.width() > 4000 || picture.height() > 4000 {
        picture = picture.thumbnail(4000, 4000);
    }
    for _ in 0..16 {
        let mut bytes = std::io::Cursor::new(Vec::new());
        picture
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|_| "Could not encode the Bluesky image.".to_string())?;
        if bytes.get_ref().len() <= 2_000_000 {
            return Ok((bytes.into_inner(), picture.width(), picture.height()));
        }
        picture = picture.thumbnail(
            (picture.width() * 4 / 5).max(1),
            (picture.height() * 4 / 5).max(1),
        );
    }
    Err("Could not fit the generated image within Bluesky's upload limit.".into())
}
fn html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\n', "<br>\n")
}
fn html_with_links(text: &str) -> String {
    text.split_inclusive(char::is_whitespace)
        .map(|part| {
            let link = part.trim_end();
            if url_valid(link) {
                format!(
                    "<a href=\"{}\">{}</a>{}",
                    html(link),
                    html(link),
                    html(&part[link.len()..])
                )
            } else {
                html(part)
            }
        })
        .collect()
}
fn require_string(v: &Value, key: &str) -> Result<String, String> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Platform did not return {key}. Check its website before retrying."))
}
#[derive(Debug)]
pub struct PublishOutcome {
    pub state: String,
    pub link: String,
    pub image_attached: bool,
    pub image_note: String,
}
impl From<(String, String)> for PublishOutcome {
    fn from((state, link): (String, String)) -> Self {
        Self {
            state,
            link,
            image_attached: false,
            image_note: String::new(),
        }
    }
}
pub fn publish(post: &Post, a: &Account) -> Result<PublishOutcome, String> {
    let mut actual = post.clone();
    let (state, link) = publish_inner(&mut actual, a)?;
    Ok(PublishOutcome {
        state,
        link,
        image_attached: actual.attach_image,
        image_note: actual.image_note,
    })
}
fn image_failed(post: &mut Post, error: &str) {
    post.attach_image = false;
    post.image_note = format!("Posted text only: image preparation/upload failed. {error}");
}
fn publish_inner(post: &mut Post, a: &Account) -> Result<(String, String), String> {
    ready(&post.platform, a)?;
    let c = client()?;
    let token = credential(&post.platform)?;
    let public_image = if (post.attach_image
        && matches!(post.platform.as_str(), "Threads" | "Blogger"))
        || post.platform == "Instagram"
    {
        match upload_public_image(&c, post, a) {
            Ok(url) => Some(url),
            Err(error) => {
                image_failed(post, &error);
                None
            }
        }
    } else {
        None
    };
    let link = match post.platform.as_str() {
        "Facebook" => {
            if post.attach_image {
                let form = file_form("source", &post.card_path)?
                    .text("caption", post.body.clone())
                    .text("published", "true");
                let v = response(
                    c.post(format!(
                        "https://graph.facebook.com/v27.0/{}/photos",
                        id(&a.identity)?
                    ))
                    .bearer_auth(token)
                    .multipart(form),
                )?;
                let photo = v["post_id"]
                    .as_str()
                    .or(v["id"].as_str())
                    .ok_or("No Facebook photo ID returned. Check before retrying.")?;
                return Ok((
                    "Published".into(),
                    format!("https://www.facebook.com/{photo}"),
                ));
            }
            let v = response(
                c.post(format!(
                    "https://graph.facebook.com/v27.0/{}/feed",
                    id(&a.identity)?
                ))
                .bearer_auth(token)
                .form(&[("message", post.body.as_str())]),
            )?;
            format!("https://www.facebook.com/{}", require_string(&v, "id")?)
        }
        "Threads" => {
            let mut form = vec![
                (
                    "media_type",
                    if post.attach_image {
                        "IMAGE".to_owned()
                    } else {
                        "TEXT".to_owned()
                    },
                ),
                ("text", post.body.clone()),
            ];
            if let Some(url) = &public_image {
                form.push(("image_url", url.clone()));
            }
            form.push(("is_made_with_ai", "true".into()));
            let v = response(
                c.post(format!(
                    "https://graph.threads.com/v1.0/{}/threads",
                    id(&a.identity)?
                ))
                .bearer_auth(&token)
                .form(&form),
            )?;
            let container = require_string(&v, "id")?;
            if post.attach_image {
                let mut finished = false;
                for _ in 0..20 {
                    let state = response(
                        c.get(format!("https://graph.threads.com/v1.0/{container}"))
                            .bearer_auth(&token)
                            .query(&[("fields", "status")]),
                    )?;
                    match state["status"].as_str() {
                        Some("FINISHED") => {
                            finished = true;
                            break;
                        }
                        Some("ERROR" | "EXPIRED") => return Err(
                            "Threads rejected the image container. No publish request was sent."
                                .into(),
                        ),
                        _ => std::thread::sleep(Duration::from_secs(2)),
                    }
                }
                if !finished {
                    return Err(
                        "Threads image processing timed out. No publish request was sent.".into(),
                    );
                }
            }
            let v = response(
                c.post(format!(
                    "https://graph.threads.com/v1.0/{}/threads_publish",
                    id(&a.identity)?
                ))
                .bearer_auth(&token)
                .form(&[("creation_id", container)]),
            )?;
            let published = require_string(&v, "id")?;
            let detail = response(
                c.get(format!("https://graph.threads.com/v1.0/{published}"))
                    .bearer_auth(token)
                    .query(&[("fields", "permalink")]),
            );
            detail
                .ok()
                .and_then(|v| v["permalink"].as_str().map(str::to_owned))
                .unwrap_or_else(|| "https://www.threads.net/".into())
        }
        "Mastodon" => {
            let mut payload = json!({"status":post.body,"visibility":"public"});
            if post.attach_image {
                match mastodon_image(&c, post, a, &token) {
                    Ok(media) => payload["media_ids"] = json!([media]),
                    Err(error) => image_failed(post, &error),
                }
            }
            let v = response(
                c.post(format!("{}/api/v1/statuses", instance(a)?))
                    .bearer_auth(token)
                    .json(&payload),
            )?;
            require_string(&v, "url")?
        }
        "X" => {
            let mut payload = json!({"text":post.body});
            if post.attach_image {
                let form = file_form("media", &post.card_path)?
                    .text("media_category", "tweet_image")
                    .text("media_type", "image/png");
                let uploaded = response(
                    c.post("https://api.x.com/2/media/upload")
                        .bearer_auth(&token)
                        .multipart(form),
                )
                .and_then(|uploaded| require_string(&uploaded["data"], "id"));
                match uploaded {
                    Ok(media) => payload["media"] = json!({"media_ids":[media]}),
                    Err(error) => image_failed(post, &error),
                }
            }
            let v = response(
                c.post("https://api.x.com/2/tweets")
                    .bearer_auth(token)
                    .json(&payload),
            )?;
            format!(
                "https://x.com/i/web/status/{}",
                require_string(&v["data"], "id")?
            )
        }
        "Blogger" => {
            let image = public_image
                .as_ref()
                .map(|url| {
                    format!(
                        "<p><img src=\"{}\" alt=\"{}\" width=\"1080\" /></p>",
                        html(url),
                        html(&post.image_text)
                    )
                })
                .unwrap_or_default();
            let v=response(c.post(format!("https://www.googleapis.com/blogger/v3/blogs/{}/posts",id(&a.identity)?)).bearer_auth(token).query(&[("isDraft","false")]).json(&json!({"kind":"blogger#post","title":post.title,"content":format!("{image}<p>{}</p>",html_with_links(&post.body))})))?;
            require_string(&v, "url")?
        }
        "Tumblr" => {
            let mut content = vec![];
            if !post.title.is_empty() {
                content.push(json!({"type":"text","text":post.title,"subtype":"heading1"}));
            }
            if post.attach_image {
                let (width, height) = image::image_dimensions(&post.card_path)
                    .map_err(|_| "Could not read generated image dimensions.".to_string())?;
                content.push(json!({"type":"image","media":[{"identifier":"sepia-image","type":"image/png","width":width,"height":height}],"alt_text":post.image_text}));
            }
            content.push(json!({"type":"text","text":post.body}));
            let request = c
                .post(format!(
                    "https://api.tumblr.com/v2/blog/{}/posts",
                    id(&a.identity)?
                ))
                .bearer_auth(token);
            let payload = json!({"content":content,"state":"published"});
            let request = if post.attach_image {
                request.multipart(tumblr_form(post, &payload)?)
            } else {
                request.json(&payload)
            };
            let v = response(request)?;
            let post_id = tumblr_post_id(&v)?;
            format!(
                "https://www.tumblr.com/{}/{post_id}",
                a.identity.trim_end_matches(".tumblr.com")
            )
        }
        "Bluesky" => {
            let session = response(
                c.post("https://bsky.social/xrpc/com.atproto.server.createSession")
                    .json(&json!({"identifier":a.identity,"password":token})),
            )?;
            let access = require_string(&session, "accessJwt")?;
            let did = require_string(&session, "did")?;
            let pds = session["didDoc"]["service"]
                .as_array()
                .and_then(|services| {
                    services.iter().find(|s| {
                        s["id"]
                            .as_str()
                            .is_some_and(|id| id.ends_with("#atproto_pds"))
                    })
                })
                .and_then(|s| s["serviceEndpoint"].as_str())
                .unwrap_or("https://bsky.social");
            if !url_valid(pds) {
                return Err("Bluesky returned an invalid server.".into());
            }
            let facets = link_facets(&post.body);
            let mut record = json!({"$type":"app.bsky.feed.post","text":post.body,"createdAt":chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis,true),"facets":facets});
            if post.attach_image {
                let (bytes, width, height) = bluesky_image(&post.card_path)?;
                let blob = response(
                    c.post(format!(
                        "{}/xrpc/com.atproto.repo.uploadBlob",
                        pds.trim_end_matches('/')
                    ))
                    .bearer_auth(&access)
                    .header("Content-Type", "image/png")
                    .body(bytes),
                );
                match blob {
                    Ok(blob) if !blob["blob"].is_null() => {
                        record["embed"] = json!({"$type":"app.bsky.embed.images","images":[{"alt":post.image_text,"image":blob["blob"],"aspectRatio":{"width":width,"height":height}}]});
                    }
                    Ok(_) => image_failed(post, "Bluesky returned no image blob."),
                    Err(error) => image_failed(post, &error),
                }
            }
            let v = response(
                c.post(format!(
                    "{}/xrpc/com.atproto.repo.createRecord",
                    pds.trim_end_matches('/')
                ))
                .bearer_auth(access)
                .json(&json!({"repo":did,"collection":"app.bsky.feed.post","record":record})),
            )?;
            let uri = require_string(&v, "uri")?;
            format!(
                "https://bsky.app/profile/{}/post/{}",
                require_string(&session, "handle")?,
                uri.rsplit('/').next().ok_or("Invalid Bluesky post URI")?
            )
        }
        "Instagram" => publish_instagram(
            &c,
            post,
            a,
            &token,
            public_image.as_deref().ok_or("Public image URL missing")?,
        )?,
        _ => return Err("This destination is manual-only. Use Copy post.".into()),
    };
    Ok(("Published".into(), link))
}
fn tumblr_post_id(value: &Value) -> Result<String, String> {
    let response = &value["response"];
    response["id"].as_str().or(response["id_string"].as_str())
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        .map(str::to_owned)
        .or_else(|| response["id"].as_u64().map(|id| id.to_string()))
        .ok_or_else(|| "Tumblr returned no readable post ID. The post may already be live; check your blog before retrying.".into())
}
fn tumblr_form(post: &Post, payload: &Value) -> Result<reqwest::blocking::multipart::Form, String> {
    let json = reqwest::blocking::multipart::Part::text(payload.to_string())
        .mime_str("application/json")
        .map_err(|e| e.to_string())?;
    let image = reqwest::blocking::multipart::Part::file(&post.card_path)
        .map_err(|_| "Could not read the Tumblr image.".to_string())?
        .mime_str("image/png")
        .map_err(|e| e.to_string())?;
    Ok(reqwest::blocking::multipart::Form::new()
        .part("json", json)
        .part("sepia-image", image))
}
pub fn link_facets(text: &str) -> Vec<Value> {
    let mut facets = Vec::new();
    let mut offset = 0;
    for part in text.split_inclusive(char::is_whitespace) {
        let link = part.trim_end();
        if link.starts_with("https://") {
            facets.push(json!({"index":{"byteStart":offset,"byteEnd":offset+link.len()},"features":[{"$type":"app.bsky.richtext.facet#link","uri":link}]}));
        }
        offset += part.len();
    }
    facets
}
fn file_form(
    field: &'static str,
    path: &str,
) -> Result<reqwest::blocking::multipart::Form, String> {
    reqwest::blocking::multipart::Form::new()
        .file(field, path)
        .map_err(|_| "Could not read the shared image card.".into())
}
fn mastodon_image(c: &Client, p: &Post, a: &Account, token: &str) -> Result<String, String> {
    let form = file_form("file", &p.card_path)?.text("description", p.image_text.clone());
    let media = response(
        c.post(format!("{}/api/v2/media", instance(a)?))
            .bearer_auth(token)
            .multipart(form),
    )?;
    let media_id = require_string(&media, "id")?;
    if media["url"].is_string() {
        return Ok(media_id);
    }
    for _ in 0..20 {
        std::thread::sleep(Duration::from_secs(2));
        let result = response(
            c.get(format!("{}/api/v1/media/{media_id}", instance(a)?))
                .bearer_auth(token),
        )?;
        if result["url"].is_string() {
            return Ok(media_id);
        }
    }
    Err("Mastodon image processing timed out. No status was posted.".into())
}
fn upload_public_image(c: &Client, p: &Post, a: &Account) -> Result<String, String> {
    if a.cloud.is_empty() || a.preset.is_empty() {
        return Err(
            "Configure Shared image hosting in Link accounts for public image hosting.".into(),
        );
    }
    let form = reqwest::blocking::multipart::Form::new()
        .text("upload_preset", a.preset.clone())
        .file("file", &p.card_path)
        .map_err(|e| e.to_string())?;
    let uploaded = response(
        c.post(format!(
            "https://api.cloudinary.com/v1_1/{}/image/upload",
            id(&a.cloud)?
        ))
        .multipart(form),
    )?;
    require_string(&uploaded, "secure_url")
}
fn publish_instagram(
    c: &Client,
    p: &Post,
    a: &Account,
    token: &str,
    image_url: &str,
) -> Result<String, String> {
    let container = response(
        c.post(format!(
            "https://graph.instagram.com/v27.0/{}/media",
            id(&a.identity)?
        ))
        .bearer_auth(token)
        .form(&[
            ("image_url", image_url.to_owned()),
            ("caption", p.body.clone()),
            ("alt_text", format!("Game update card: {}", p.image_text)),
        ]),
    )?;
    let container_id = require_string(&container, "id")?;
    let mut finished = false;
    for _ in 0..20 {
        let status = response(
            c.get(format!("https://graph.instagram.com/v27.0/{container_id}"))
                .bearer_auth(token)
                .query(&[("fields", "status_code")]),
        )?;
        match status["status_code"].as_str() {
            Some("FINISHED") => {
                finished = true;
                break;
            }
            Some("ERROR" | "EXPIRED") => {
                return Err("Instagram rejected the image container.".into())
            }
            _ => std::thread::sleep(Duration::from_secs(2)),
        }
    }
    if !finished {
        return Err("Instagram image processing timed out; no publish request was sent.".into());
    }
    let v = response(
        c.post(format!(
            "https://graph.instagram.com/v27.0/{}/media_publish",
            id(&a.identity)?
        ))
        .bearer_auth(token)
        .form(&[("creation_id", container_id)]),
    )?;
    let published = require_string(&v, "id")?;
    let detail = response(
        c.get(format!("https://graph.instagram.com/v27.0/{published}"))
            .bearer_auth(token)
            .query(&[("fields", "permalink")]),
    );
    Ok(detail
        .ok()
        .and_then(|v| v["permalink"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "https://www.instagram.com/".into()))
}
pub fn refresh_rules(mut rules: Vec<Rules>, force: bool) -> Vec<Rules> {
    let Ok(c) = client() else {
        return rules;
    };
    let now = chrono::Utc::now().timestamp();
    for rule in &mut rules {
        if !force && !rule.text.is_empty() && now - rule.fetched < 7 * 24 * 3600 {
            continue;
        }
        let url = format!("https://www.reddit.com/r/{}/about/rules.json", rule.name);
        match response(c.get(&url)) {
            Ok(v)=>if let Some(items)=v["rules"].as_array(){
                rule.text=items.iter().map(|r|format!("{}: {}",r["short_name"].as_str().unwrap_or(""),r["description"].as_str().unwrap_or(""))).collect::<Vec<_>>().join("\n\n");
                if rule.text.is_empty(){rule.error="No rules returned. Paste the current rules before generating.".into();}else{rule.fetched=now;rule.source=url;rule.error.clear();}
            }else{rule.error="Rules unavailable. Paste the current subreddit rules.".into();},
            Err(_)=>rule.error="Could not fetch rules. Open the subreddit rules and paste them below. Existing cache is preserved.".into(),
        }
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tumblr_success_accepts_string_and_legacy_numeric_post_ids() {
        for response in [
            json!({"id":"1234567891234567"}),
            json!({"id":1234567891234567u64}),
            json!({"id_string":"1234567891234567"}),
        ] {
            assert_eq!(
                tumblr_post_id(&json!({"meta":{"status":201},"response":response})).unwrap(),
                "1234567891234567"
            );
        }
        assert!(tumblr_post_id(&json!({"response":{}}))
            .unwrap_err()
            .contains("already be live"));
    }
    #[test]
    fn tumblr_upload_contains_json_and_binary_image_parts() {
        use std::io::{Read, Write};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.png");
        image::RgbImage::from_pixel(2, 2, image::Rgb([20, 30, 40]))
            .save(&path)
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                header.push(byte[0]);
            }
            let header = String::from_utf8(header).unwrap();
            let length = header
                .lines()
                .find_map(|line| {
                    line.to_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|length| length.trim().parse::<usize>().ok())
                })
                .unwrap();
            let mut body = vec![0u8; length];
            stream.read_exact(&mut body).unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .unwrap();
            (header, body)
        });
        let post = Post {
            card_path: path.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let payload = json!({"content":[{"type":"image","media":[{"identifier":"sepia-image","type":"image/png"}]}],"state":"published"});
        response(
            client()
                .unwrap()
                .post(format!("http://{address}/posts"))
                .multipart(tumblr_form(&post, &payload).unwrap()),
        )
        .unwrap();
        let (header, body) = server.join().unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(header.contains("multipart/form-data"));
        assert!(text.contains("name=\"json\"") && text.contains("application/json"));
        assert!(text.contains("name=\"sepia-image\"") && text.contains("image/png"));
        assert!(body.windows(8).any(|bytes| bytes == b"\x89PNG\r\n\x1a\n"));
    }
    #[test]
    fn large_generated_art_gets_a_bluesky_upload_copy_without_changing_original() {
        use rand::{RngCore, SeedableRng};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("generated.png");
        let mut pixels = vec![0u8; 1536 * 1024 * 3];
        rand::rngs::StdRng::seed_from_u64(42).fill_bytes(&mut pixels);
        image::RgbImage::from_raw(1536, 1024, pixels)
            .unwrap()
            .save(&path)
            .unwrap();
        let original_size = std::fs::metadata(&path).unwrap().len();
        assert!(original_size > 2_000_000);
        let (bytes, width, height) = bluesky_image(path.to_str().unwrap()).unwrap();
        assert!(bytes.len() <= 2_000_000);
        assert!(width < 1536 && height < 1024);
        assert_eq!(image::load_from_memory(&bytes).unwrap().width(), width);
        assert_eq!(std::fs::metadata(path).unwrap().len(), original_size);
    }
    #[test]
    fn links_use_utf8_byte_offsets() {
        let body = "🎮 Play https://game.example\nJoin https://discord.gg/dCmMS2Rhep";
        for facet in link_facets(body) {
            let start = facet["index"]["byteStart"].as_u64().unwrap() as usize;
            let end = facet["index"]["byteEnd"].as_u64().unwrap() as usize;
            assert_eq!(
                &body[start..end],
                facet["features"][0]["uri"].as_str().unwrap()
            );
        }
    }
    #[test]
    fn html_is_escaped() {
        assert_eq!(html("<script>\n&"), "&lt;script&gt;<br>\n&amp;");
    }
    #[test]
    fn manual_publish_is_rejected_without_network() {
        for platform in ["Reddit", "Instagram", "X"] {
            let p = Post {
                platform: platform.into(),
                ..Default::default()
            };
            assert!(publish(&p, &Account::default())
                .unwrap_err()
                .contains("Manual"));
        }
    }
    #[test]
    fn provider_errors_never_expose_tokens_or_echoed_secrets() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0; 2048];
            let _ = stream.read(&mut bytes);
            let body = r#"{"error":"sensitive-provider-detail-123"}"#;
            write!(
                stream,
                "HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let error = response(
            client()
                .unwrap()
                .get(format!("http://{address}/"))
                .bearer_auth("private-token-123"),
        )
        .unwrap_err();
        server.join().unwrap();
        assert!(error.contains("401"));
        assert!(!error.contains("sensitive"));
        assert!(!error.contains("private-token"));
    }
    #[test]
    fn windows_credentials_roundtrip_and_delete() {
        let name = format!(
            "test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        );
        save_credential(&name, "temporary-test-value").unwrap();
        let read = credential(&name);
        let removed = delete_credential(&name);
        assert_eq!(read.unwrap(), "temporary-test-value");
        removed.unwrap();
        assert!(credential(&name).is_err());
    }
    #[test]
    fn recent_subreddit_rules_are_reused_without_fetching() {
        let rule = Rules {
            name: "cached".into(),
            text: "Current rules".into(),
            fetched: chrono::Utc::now().timestamp(),
            ..Default::default()
        };
        let result = refresh_rules(vec![rule], false);
        assert_eq!(result[0].text, "Current rules");
        assert!(result[0].error.is_empty());
    }
    #[test]
    fn blogger_links_are_clickable_and_untrusted_html_is_escaped() {
        let text =
            html_with_links("<b>Play</b> https://game.example\nJoin https://discord.gg/dCmMS2Rhep");
        assert!(text.contains("&lt;b&gt;"));
        assert!(text.contains("href=\"https://game.example\""));
        assert!(text.contains("href=\"https://discord.gg/dCmMS2Rhep\""));
    }
}
