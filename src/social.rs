mod generation;
mod images;
mod network;
mod oauth;
mod ui;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::Write,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

pub const DISCORD: &str = "https://discord.gg/dCmMS2Rhep";
pub const BLOCKED: &str = "This subreddit doesnt accept promotions";
#[derive(Clone, Copy)]
pub enum ImproveTarget {
    Text,
    Image,
    Both,
}
pub const PLATFORMS: [&str; 11] = [
    "Medium",
    "Substack",
    "Substack Notes",
    "Facebook",
    "Tumblr",
    "Mastodon",
    "Threads",
    "Bluesky",
    "X",
    "Blogger",
    "Instagram",
];
pub fn manual(platform: &str) -> bool {
    matches!(
        platform,
        "Medium" | "Substack" | "Substack Notes" | "Reddit" | "Instagram" | "X"
    )
}
pub fn url_valid(text: &str) -> bool {
    reqwest::Url::parse(text).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
    })
}
pub fn limit(platform: &str) -> usize {
    match platform {
        "Bluesky" => 300,
        "Threads" | "Mastodon" => 500,
        "X" => 280,
        "Instagram" => 2200,
        _ => 40000,
    }
}
pub fn text_length(platform: &str, body: &str) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    if platform == "X" {
        body.split_inclusive(char::is_whitespace)
            .map(|word| {
                let trimmed = word.trim_end();
                if trimmed.starts_with("https://") {
                    23 + word.len() - trimmed.len()
                } else {
                    word.chars()
                        .map(|c| if (c as u32) > 0x10ff { 2 } else { 1 })
                        .sum()
                }
            })
            .sum()
    } else {
        body.graphemes(true).count()
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Account {
    pub compose_url: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub identity: String,
    pub server: String,
    pub cloud: String,
    pub preset: String,
    pub notes: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Rules {
    pub name: String,
    pub text: String,
    pub fetched: i64,
    pub source: String,
    pub error: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Post {
    pub platform: String,
    pub destination: String,
    pub title: String,
    pub body: String,
    pub revision: String,
    pub approved: bool,
    pub blocked: bool,
    pub warnings: Vec<String>,
    pub state: String,
    pub link: String,
    pub card_path: String,
    pub image_text: String,
    pub emoji_allowed: bool,
    pub attach_image: bool,
    pub image_allowed: Option<bool>,
    pub image_excluded: bool,
    pub image_note: String,
    pub publish_error: String,
}
impl Post {
    pub fn key(&self) -> String {
        format!("{}:{}", self.platform, self.destination)
    }
    pub fn copy_text(&self) -> String {
        if self.title.is_empty() {
            self.body.clone()
        } else {
            format!("{}\n\n{}", self.title, self.body)
        }
    }
    pub fn issues(&self, website: &str) -> Vec<String> {
        let mut issues = Vec::new();
        if self.blocked {
            issues.push("This destination is blocked by its rules.".into());
            return issues;
        }
        if self.body.trim().is_empty() {
            issues.push("Empty post.".into());
        }
        if !self.body.contains(website) || website.is_empty() {
            issues.push("The website link is missing.".into());
        }
        if !self.body.trim_end().ends_with(DISCORD) {
            issues.push("The closing Discord invitation is missing.".into());
        }
        if text_length(&self.platform, &self.body) > limit(&self.platform) {
            issues.push(format!(
                "Over the {} character limit.",
                limit(&self.platform)
            ));
        }
        if self.attach_image
            && (self.card_path.is_empty() || !std::path::Path::new(&self.card_path).is_file())
        {
            issues.push("The shared image is missing. Generate a new batch.".into());
        }
        if self.platform == "Instagram" && self.image_allowed == Some(false) {
            issues.push("Instagram requires an image, but the supplied account rules disallow this graphic.".into());
        }
        use unicode_segmentation::UnicodeSegmentation;
        let count = self
            .body
            .graphemes(true)
            .filter(|part| emojis::get(part).is_some())
            .count();
        if count != usize::from(self.emoji_allowed) {
            issues.push(
                if self.emoji_allowed {
                    "Use exactly one emoji."
                } else {
                    "This destination does not allow emoji."
                }
                .into(),
            );
        }
        if self.platform == "Medium" && !self.body.to_lowercase().contains("ai assistance") {
            issues.push("Medium requires a visible AI-assistance disclosure.".into());
        }
        issues
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneratedImage {
    pub path: String,
    pub prompt: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Data {
    pub thread_folder: String,
    pub information: String,
    pub image_prompt: String,
    pub batch_image_prompt: String,
    pub shared_image_path: String,
    pub style_reference: String,
    pub generated_images: Vec<GeneratedImage>,
    pub website: String,
    pub batch_website: String,
    pub batch_information: String,
    pub posts: Vec<Post>,
    pub accounts: BTreeMap<String, Account>,
    pub rules: Vec<Rules>,
    pub thread: Option<String>,
}
pub enum Event {
    Improved(ImproveTarget, Result<(String, String), String>),
    ImageGenerated(Result<(String, String), String>),
    Generated(Result<Box<Data>, String>),
    Published(usize, Result<network::PublishOutcome, String>),
    Rules(Vec<Rules>),
    CachedRules(Vec<Rules>),
    Connected(String, Result<String, String>),
    Authorized(String, Result<Vec<(String, String)>, String>),
    Done,
}

fn prepare_publish(
    data: &mut Data,
    selected: Option<usize>,
    mut ready: impl FnMut(&str, &Account) -> Result<(), String>,
) -> Vec<(usize, Post, Account)> {
    let mut jobs = Vec::new();
    for (index, post) in data.posts.iter_mut().enumerate() {
        if selected.is_some_and(|selected| selected != index) {
            continue;
        }
        if !post.approved || manual(&post.platform) || !post.state.is_empty() {
            continue;
        }
        let account = network::effective_account(&data.accounts, &post.platform);
        post.publish_error.clear();
        if post.attach_image {
            let reason = network::image_unavailable(&post.platform, &account)
                .map(str::to_owned)
                .or_else(|| {
                    (!std::path::Path::new(&post.card_path).is_file())
                        .then(|| "The image file is missing; sending text only.".into())
                });
            if let Some(reason) = reason {
                post.attach_image = false;
                post.card_path.clear();
                post.image_note = reason;
            }
        }
        let mut issues = post.issues(&data.batch_website);
        if !post.revision.trim().is_empty() {
            issues.push("Apply or clear the pending revision before posting.".into());
        }
        let result = if issues.is_empty() {
            ready(&post.platform, &account)
        } else {
            Err(issues.join(" "))
        };
        post.approved = false;
        match result {
            Ok(()) => {
                post.state = "Sending — verify on the platform if the app closes.".into();
                jobs.push((index, post.clone(), account));
            }
            Err(error) => post.publish_error = format!("Not sent: {error}"),
        }
    }
    jobs
}
pub struct Social {
    pub project_folder: String,
    pub data: Data,
    pub settings: bool,
    pub busy: bool,
    pub status: String,
    pub error: String,
    pub new_subreddit: String,
    pub secrets: BTreeMap<String, String>,
    pub images: BTreeMap<String, egui::TextureHandle>,
    pub credential_status: BTreeMap<String, bool>,
    pub oauth_pending: Option<(String, mpsc::Sender<String>)>,
    pub oauth_callback: String,
    pub destinations: BTreeMap<String, Vec<(String, String)>>,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    path: PathBuf,
    writable: bool,
    last_saved: Instant,
    dirty: bool,
}
impl Social {
    pub fn new() -> Self {
        let path = super::state::storage_path().with_file_name("social.json");
        let (mut data, error, writable) = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(data) => (data, String::new(), true),
                Err(e) => (
                    Data::default(),
                    format!("Saved SoMe data could not be read: {e}. Original file preserved."),
                    false,
                ),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (Data::default(), String::new(), true)
            }
            Err(e) => (Data::default(), e.to_string(), false),
        };
        let clean_prompt = images::without_suffix(&data.image_prompt);
        let prompt_migrated = clean_prompt != data.image_prompt;
        data.image_prompt = clean_prompt;
        let image_migrated = generation::restore_image(&mut data) || prompt_migrated;
        let (tx, rx) = mpsc::channel();
        let credential_status = PLATFORMS
            .into_iter()
            .map(|p| (p.into(), network::stored_credential(p).is_ok()))
            .collect();
        Self {
            project_folder: String::new(),
            data,
            settings: false,
            busy: false,
            status: String::new(),
            error,
            new_subreddit: String::new(),
            secrets: BTreeMap::new(),
            images: BTreeMap::new(),
            credential_status,
            oauth_pending: None,
            oauth_callback: String::new(),
            destinations: BTreeMap::new(),
            tx,
            rx,
            path,
            writable,
            last_saved: Instant::now(),
            dirty: image_migrated,
        }
    }
    pub fn save(&mut self) {
        if !self.writable || !self.dirty {
            return;
        }
        let result = (|| -> Result<(), String> {
            let parent = self.path.parent().ok_or("Invalid storage folder")?;
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
            file.write_all(&serde_json::to_vec_pretty(&self.data).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            file.as_file().sync_all().map_err(|e| e.to_string())?;
            file.persist(&self.path).map_err(|e| e.to_string())?;
            Ok(())
        })();
        match result {
            Ok(()) => self.dirty = false,
            Err(e) => self.error = format!("Could not save SoMe data: {e}"),
        }
        self.last_saved = Instant::now();
    }
    pub fn poll(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::ImageGenerated(result) => {
                    self.busy = false;
                    match result {
                        Ok((path, prompt)) => {
                            generation::remember_image(&mut self.data, path, prompt);
                            self.images.clear();
                            self.status =
                                "Image ready. Review and approve any changed drafts again.".into();
                            self.dirty = true;
                        }
                        Err(error) => self.error = error,
                    }
                }
                Event::Improved(target, result) => {
                    self.busy = false;
                    match result {
                        Ok((information, image_prompt)) => {
                            if matches!(target, ImproveTarget::Text | ImproveTarget::Both) {
                                self.data.information = information;
                            }
                            if matches!(target, ImproveTarget::Image | ImproveTarget::Both) {
                                self.data.image_prompt = images::without_suffix(&image_prompt);
                            }
                            self.status =
                                "Selected prompts improved. Review them, then generate text or an image.".into();
                            self.dirty = true;
                        }
                        Err(error) => self.error = error,
                    }
                }
                Event::Generated(result) => {
                    self.busy = false;
                    match result {
                        Ok(data) => {
                            self.data = *data;
                            self.images.clear();
                            self.status = "Drafts ready. Review each post before approving.".into();
                            self.dirty = true;
                        }
                        Err(e) => self.error = e,
                    }
                }
                Event::Published(index, result) => {
                    if let Some(post) = self.data.posts.get_mut(index) {
                        match result {
                            Ok(outcome) => {
                                post.state = outcome.state;
                                post.link = outcome.link;
                                post.image_note = outcome.image_note;
                                post.attach_image = outcome.image_attached;
                                if !post.attach_image {
                                    post.card_path.clear();
                                }
                                post.publish_error.clear();
                                post.approved = false;
                            }
                            Err(e) => {
                                post.state = format!("Check before retrying: {e}");
                                post.publish_error = e;
                                post.approved = false;
                            }
                        }
                    }
                    self.dirty = true;
                    self.save();
                }
                Event::Rules(rules) => {
                    self.data.rules = rules;
                    for post in &mut self.data.posts {
                        if post.platform == "Reddit" && post.state.is_empty() {
                            post.approved = false;
                            if !post.blocked {
                                post.revision =
                                    "Recheck this draft against the refreshed subreddit rules."
                                        .into();
                            }
                        }
                    }
                    self.dirty = true;
                    self.busy = false;
                    self.status = "Subreddit rules refreshed. Review any fetch errors.".into();
                }
                Event::CachedRules(rules) => {
                    self.data.rules = rules;
                    for post in &mut self.data.posts {
                        if post.platform == "Reddit" {
                            post.approved = false;
                        }
                    }
                    self.dirty = true;
                }
                Event::Connected(platform, result) => {
                    self.busy = false;
                    match result {
                        Ok(label) => {
                            self.status = format!("{platform}: {label}");
                            self.credential_status.insert(platform, true);
                        }
                        Err(e) => self.error = e,
                    }
                }
                Event::Authorized(platform, result) => {
                    self.busy = false;
                    self.oauth_pending = None;
                    self.oauth_callback.clear();
                    match result {
                        Ok(destinations) => {
                            let account = self.data.accounts.entry(platform.clone()).or_default();
                            if !destinations.iter().any(|(id, _)| id == &account.identity) {
                                account.identity = destinations[0].0.clone();
                            }
                            self.status = format!("{platform} connected. Check the selected account or blog below before posting.");
                            self.destinations.insert(platform.clone(), destinations);
                            for post in &mut self.data.posts {
                                if post.platform == platform && post.state.is_empty() {
                                    post.approved = false;
                                }
                            }
                            self.credential_status.insert(platform, true);
                            self.dirty = true;
                        }
                        Err(error) => self.error = error,
                    }
                }
                Event::Done => {
                    self.busy = false;
                    let published = self
                        .data
                        .posts
                        .iter()
                        .filter(|p| p.state == "Published")
                        .count();
                    let failed = self
                        .data
                        .posts
                        .iter()
                        .filter(|p| !p.publish_error.is_empty())
                        .count();
                    self.status = format!("Posting finished: {published} published; {failed} need attention. Manual posts still require copying.");
                    if failed > 0 {
                        self.error = format!("{failed} post(s) could not be sent. See the red errors on their previews.");
                    }
                }
            }
        }
        if self.dirty && self.last_saved.elapsed() > Duration::from_secs(1) {
            self.save();
        }
    }
    fn improve(&mut self, target: ImproveTarget, ctx: &egui::Context) {
        self.busy = true;
        self.error.clear();
        self.status = match target {
            ImproveTarget::Text => "Improving text prompt...",
            ImproveTarget::Image => "Improving image prompt...",
            ImproveTarget::Both => "Improving both prompts...",
        }
        .into();
        let information = self.data.information.clone();
        let image_prompt = self.data.image_prompt.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Improved(
                target,
                generation::improve_selected(information, image_prompt, target, &ctx),
            ));
            ctx.request_repaint();
        });
    }
    fn generate_image(&mut self, ctx: &egui::Context) {
        if self.data.image_prompt.trim().is_empty() {
            self.error = "Describe the picture in Image prompt first.".into();
            return;
        }
        self.busy = true;
        self.error.clear();
        self.status = "Generating the image; existing post text will be kept…".into();
        let prompt = self.data.image_prompt.clone();
        let reference = self.data.style_reference.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = images::generate(
                &prompt,
                (!reference.is_empty()).then_some(reference.as_str()),
                &ctx,
            )
            .map(|path| (path.to_string_lossy().into_owned(), prompt));
            let _ = tx.send(Event::ImageGenerated(result));
            ctx.request_repaint();
        });
    }
    fn remove_image(&mut self) {
        self.error.clear();
        generation::remove_image(&mut self.data);
        self.images.clear();
        self.dirty = true;
        self.status = "Image removed from unposted drafts. Their text is unchanged; review and approve again.".into();
        self.save();
    }
    fn select_image(&mut self, index: usize) {
        match generation::select_image(&mut self.data, index) {
            Ok(()) => {
                self.error.clear();
                self.dirty = true;
                self.status = "Saved image selected. Review any changed attachments again.".into();
                self.save();
            }
            Err(error) => self.error = error,
        }
    }
    fn generate(&mut self, update: bool, ctx: &egui::Context) {
        let folder = PathBuf::from(&self.project_folder);
        if self.project_folder.is_empty() || !folder.is_dir() {
            self.error = "Choose an existing project folder at the top left first.".into();
            return;
        }
        if !url_valid(self.data.website.trim()) {
            self.error = "Enter a full https:// website URL.".into();
            return;
        }
        if update
            && (self.data.website != self.data.batch_website
                || self.data.information != self.data.batch_information)
        {
            self.error = "The batch information changed. Use Generate text for a new batch.".into();
            return;
        }
        self.busy = true;
        self.error.clear();
        self.status = if update {
            "Applying your revision requests…"
        } else {
            "Generating text drafts…"
        }
        .into();
        let mut data = self.data.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            if !update {
                data.rules = network::refresh_rules(data.rules, false);
                let _ = tx.send(Event::CachedRules(data.rules.clone()));
            }
            let result =
                generation::generate_in_folder(data, update, &ctx, Some(&folder)).map(Box::new);
            let _ = tx.send(Event::Generated(result));
            ctx.request_repaint();
        });
    }
    fn publish(&mut self, selected: Option<usize>, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        // Approval applies to the displayed preview, independent of composer edits.
        let jobs = prepare_publish(&mut self.data, selected, network::ready);
        self.dirty = true;
        self.save();
        if self.dirty {
            self.error = "Publishing cancelled because its send record could not be saved.".into();
            return;
        }
        self.busy = true;
        self.error.clear();
        self.status = "Publishing approved posts…".into();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            for (i, p, a) in jobs {
                let result = network::publish(&p, &a);
                let _ = tx.send(Event::Published(i, result));
                ctx.request_repaint();
            }
            let _ = tx.send(Event::Done);
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn individual_send_leaves_other_approvals_and_errors_untouched() {
        let mut data = Data {
            batch_website: "https://game.example".into(),
            ..Default::default()
        };
        for platform in ["Facebook", "Bluesky", "X"] {
            data.posts.push(Post {
                platform: platform.into(),
                body: format!("Play https://game.example\nJoin Discord: {DISCORD}"),
                approved: true,
                publish_error: "Keep this status".into(),
                ..Default::default()
            });
        }
        let before = serde_json::to_string(&data.posts[0]).unwrap();
        let jobs = prepare_publish(&mut data, Some(1), |_, _| Ok(()));
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].0, 1);
        assert_eq!(serde_json::to_string(&data.posts[0]).unwrap(), before);
        assert!(data.posts[2].approved);
        assert!(prepare_publish(&mut data, Some(2), |_, _| panic!(
            "X must never publish automatically"
        ))
        .is_empty());
    }
    #[test]
    fn one_setup_failure_does_not_block_ready_posts_and_hosting_is_optional() {
        let mut data = Data {
            batch_website: "https://game.example".into(),
            information: "Edited composer".into(),
            batch_information: "Approved preview".into(),
            ..Default::default()
        };
        for platform in ["Facebook", "Threads", "Bluesky", "Instagram"] {
            data.posts.push(Post {
                platform: platform.into(),
                body: format!("Play https://game.example\nJoin my Discord: {DISCORD}"),
                approved: true,
                attach_image: platform == "Threads",
                card_path: if platform == "Threads" {
                    "picture.png".into()
                } else {
                    String::new()
                },
                ..Default::default()
            });
        }
        let jobs = prepare_publish(&mut data, None, |platform, _| {
            if platform == "Facebook" {
                Err("Reconnect Facebook".into())
            } else {
                Ok(())
            }
        });
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].1.platform, "Threads");
        assert!(!jobs[0].1.attach_image);
        assert!(!data.posts[1].image_note.is_empty());
        assert_eq!(jobs[1].1.platform, "Bluesky");
        assert!(data.posts[0].publish_error.contains("Reconnect Facebook"));
        assert!(data.posts[0].state.is_empty());
        assert!(data.posts[3].approved && data.posts[3].state.is_empty());
    }
    #[test]
    fn a_failed_publish_stays_visible_while_other_results_are_recorded() {
        let directory = tempfile::tempdir().unwrap();
        let mut social = Social::new();
        social.path = directory.path().join("social.json");
        social.data = Data {
            posts: vec![Post::default(), Post::default()],
            ..Default::default()
        };
        social
            .tx
            .send(Event::Published(
                0,
                Err("HTTP 403: publishing permission missing".into()),
            ))
            .unwrap();
        social
            .tx
            .send(Event::Published(
                1,
                Ok(("Published".into(), "https://example.com/post".into()).into()),
            ))
            .unwrap();
        social.tx.send(Event::Done).unwrap();
        social.poll();
        assert!(social.data.posts[0].publish_error.contains("403"));
        assert!(social.data.posts[0]
            .state
            .starts_with("Check before retrying:"));
        assert_eq!(social.data.posts[1].link, "https://example.com/post");
        assert_eq!(social.data.posts[1].state, "Published");
        assert!(social.status.contains("1 published; 1 need attention"));
    }
    #[test]
    fn selective_improvement_keeps_the_other_field_and_style_reference_persists() {
        let directory = tempfile::tempdir().unwrap();
        let mut social = Social::new();
        social.path = directory.path().join("social.json");
        social.data = Data {
            information: "Original text".into(),
            image_prompt: "Original image".into(),
            style_reference: "saved-reference.png".into(),
            ..Default::default()
        };
        social
            .tx
            .send(Event::Improved(
                ImproveTarget::Text,
                Ok(("Better text".into(), "Must not apply".into())),
            ))
            .unwrap();
        social.poll();
        assert_eq!(social.data.information, "Better text");
        assert_eq!(social.data.image_prompt, "Original image");
        social
            .tx
            .send(Event::Improved(
                ImproveTarget::Image,
                Ok(("Must not apply".into(), "Better image".into())),
            ))
            .unwrap();
        social.poll();
        assert_eq!(social.data.information, "Better text");
        assert_eq!(social.data.image_prompt, "Better image");
        social.save();
        let reloaded: Data = serde_json::from_slice(&std::fs::read(&social.path).unwrap()).unwrap();
        assert_eq!(reloaded.style_reference, "saved-reference.png");
    }
    #[test]
    fn missing_links_and_overlong_posts_cannot_pass() {
        let mut p = Post {
            platform: "Bluesky".into(),
            body: "hello".into(),
            ..Default::default()
        };
        assert_eq!(p.issues("https://game.example").len(), 2);
        p.body = format!(
            "{} https://game.example\nJoin my Discord: {DISCORD}",
            "x".repeat(300)
        );
        assert_eq!(p.issues("https://game.example").len(), 1);
        p.body = format!("Play: https://game.example\nJoin my Discord: {DISCORD}");
        assert!(p.issues("https://game.example").is_empty());
    }
    #[test]
    fn secrets_are_not_part_of_saved_data() {
        let data = serde_json::to_string(&Data::default()).unwrap();
        assert!(!data.contains("token"));
        assert!(!data.contains("password"));
    }
    #[test]
    fn failed_improvement_keeps_inputs_and_success_only_updates_prompts() {
        let directory = tempfile::tempdir().unwrap();
        let mut social = Social::new();
        social.path = directory.path().join("social.json");
        social.data = Data::default();
        social.data.information = "Original update".into();
        social.data.image_prompt = "Original image".into();
        social.data.posts.push(Post {
            body: "Existing draft".into(),
            ..Default::default()
        });
        social
            .tx
            .send(Event::Improved(
                ImproveTarget::Both,
                Err("Connection failed".into()),
            ))
            .unwrap();
        social.poll();
        assert_eq!(social.data.information, "Original update");
        assert_eq!(social.data.image_prompt, "Original image");
        social
            .tx
            .send(Event::Improved(
                ImproveTarget::Both,
                Ok(("Improved update".into(), "Improved image".into())),
            ))
            .unwrap();
        social.poll();
        assert_eq!(social.data.information, "Improved update");
        assert_eq!(social.data.image_prompt, "Improved image");
        assert!(social.data.shared_image_path.is_empty());
        assert_eq!(social.data.posts[0].body, "Existing draft");
        assert!(social.data.thread.is_none());
    }
    #[test]
    fn reconnect_requires_new_approval_and_preserves_published_records() {
        let directory = tempfile::tempdir().unwrap();
        let mut social = Social::new();
        social.path = directory.path().join("social.json");
        social.data.posts = vec![
            Post {
                platform: "Blogger".into(),
                approved: true,
                ..Default::default()
            },
            Post {
                platform: "Blogger".into(),
                state: "Published".into(),
                link: "https://example.com/post".into(),
                ..Default::default()
            },
            Post {
                platform: "X".into(),
                approved: true,
                ..Default::default()
            },
        ];
        social
            .tx
            .send(Event::Authorized(
                "Blogger".into(),
                Ok(vec![("123".into(), "Game blog".into())]),
            ))
            .unwrap();
        social.poll();
        assert!(!social.data.posts[0].approved);
        assert_eq!(social.data.posts[1].link, "https://example.com/post");
        assert!(social.data.posts[2].approved);
        assert_eq!(social.data.accounts["Blogger"].identity, "123");
    }
    #[test]
    fn manual_destinations_are_never_automatic() {
        for p in ["Medium", "Substack", "Substack Notes", "Reddit", "X"] {
            assert!(manual(p));
        }
        assert!(manual("Instagram"));
    }
    #[test]
    fn rule_cache_survives_a_generation_failure() {
        let directory = tempfile::tempdir().unwrap();
        let mut social = Social::new();
        social.path = directory.path().join("social.json");
        social.data = Data::default();
        social.data.posts.push(Post {
            platform: "Bluesky".into(),
            body: "keep existing draft".into(),
            ..Default::default()
        });
        social
            .tx
            .send(Event::CachedRules(vec![Rules {
                name: "test".into(),
                text: "Current rules".into(),
                ..Default::default()
            }]))
            .unwrap();
        social
            .tx
            .send(Event::Generated(Err("Mock generation failure".into())))
            .unwrap();
        social.poll();
        assert_eq!(social.data.rules[0].text, "Current rules");
        assert_eq!(social.data.posts[0].body, "keep existing draft");
        assert!(!social.busy);
    }
}
