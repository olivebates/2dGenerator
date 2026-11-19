use super::*;
use crate::appearance::{button, ButtonStyle};
use egui::{Color32, RichText};

fn image_texture(
    ctx: &egui::Context,
    textures: &mut BTreeMap<String, egui::TextureHandle>,
    path: &str,
) -> Option<egui::TextureHandle> {
    if !textures.contains_key(path) {
        let picture = image::open(path).ok()?.into_rgba8();
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [picture.width() as usize, picture.height() as usize],
            picture.as_raw(),
        );
        textures.insert(
            path.into(),
            ctx.load_texture(path, color, egui::TextureOptions::LINEAR),
        );
    }
    textures.get(path).cloned()
}

fn guide(platform: &str) -> (&'static str, &'static str) {
    match platform {
    "Facebook"=>("Facebook Page ID + Page access token with pages_manage_posts and pages_read_engagement. If scopes are missing in Graph API Explorer, first add them in the selected Meta app: Use cases → Page management → Customize → Permissions and features. Then select that same app in Explorer and regenerate the token.","https://developers.facebook.com/docs/pages-api/posts/"),
    "Threads"=>("In your Meta app, enable Access the Threads API. Add your own Threads account as a tester and accept its invitation. Register your HTTPS callback page. Copy the Threads App ID and Threads App secret into the fields below, then click Connect Threads.","https://developers.facebook.com/docs/threads/get-started/"),
    "Instagram"=>("Manual posting. Save the generated image, copy the caption, and upload both to Instagram yourself. No professional account, Meta developer app or account connection is needed in Sepia.","https://www.instagram.com/"),
    "Bluesky"=>("Enter your Bluesky handle and an app password created in Bluesky Settings → Privacy and security → App passwords. Your main password is not needed. This connector uses bsky.social sign-in.","https://bsky.app/settings/app-passwords"),
    "Mastodon"=>("Enter your server URL (https://your.server). Create a personal application in Preferences → Development with read:accounts, write:statuses and write:media. Paste its access token. Paste your instance's additional rules below.","https://docs.joinmastodon.org/client/token/"),
    "X"=>("Manual posting. Create new post opens X with your text filled in. Attach the saved image yourself, review, and publish on X. No developer account or API credits are needed.","https://x.com/intent/tweet"),
    "Blogger"=>("Enable Blogger API in Google Cloud. In Google Auth Platform, add your Blogger email under Audience > Test users, add the Blogger scope under Data Access, and create an OAuth client of type Desktop app. Copy its client ID and secret below, then click Connect Blogger. Sepia finds your blogs automatically.","https://developers.google.com/blogger/docs/3.0/using"),
    "Tumblr"=>("Edit your Tumblr developer application. Set both Default callback URL and OAuth2 redirect URLs to your HTTPS callback page. Copy its OAuth consumer key and secret below, then click Connect Tumblr. Sepia finds your blogs automatically.","https://www.tumblr.com/oauth/apps"),
    "Medium"=>("Manual publishing. New API integrations are unavailable. Keep AI disclosure visible and publish free, not in the Partner Program paywall.","https://medium.com/new-story"),
    "Substack"=>("Manual website publishing only. In Substack's publishing screen, turn OFF sending via email and app inbox.","https://substack.com/publish"),
    "Substack Notes"=>("Manual publishing. Copy your draft and open Notes. No email campaign is created.","https://substack.com/notes"),
    _=>("Manual publishing only.","https://www.reddit.com/submit")
}
}
fn compose_url(post: &Post, account: Option<&Account>) -> String {
    if let Some(url) = account
        .map(|a| a.compose_url.trim())
        .filter(|url| url_valid(url))
    {
        return url.into();
    }
    if post.platform == "X" {
        let mut url = reqwest::Url::parse("https://x.com/intent/tweet").unwrap();
        url.query_pairs_mut().append_pair("text", &post.copy_text());
        return url.into();
    }
    if post.platform == "Reddit" {
        let mut url = reqwest::Url::parse("https://www.reddit.com/submit").unwrap();
        if !post.destination.is_empty() {
            url.path_segments_mut()
                .unwrap()
                .clear()
                .extend(["r", &post.destination, "submit"]);
        }
        url.query_pairs_mut()
            .append_pair("type", "TEXT")
            .append_pair("title", &post.title)
            .append_pair("text", &post.body);
        return url.into();
    }
    guide(&post.platform).1.into()
}
pub fn policy_url(platform: &str) -> &'static str {
    match platform {
    "Medium"=>"https://help.medium.com/hc/en-us/articles/22576852947223-Artificial-Intelligence-AI-content-policy",
    "Substack"|"Substack Notes"=>"https://substack.com/content",
    "Facebook"|"Instagram"|"Threads"=>"https://transparency.meta.com/policies/community-standards/",
    "Bluesky"=>"https://bsky.social/about/support/community-guidelines",
    "Mastodon"=>"https://docs.joinmastodon.org/user/moderating/",
    "Tumblr"=>"https://www.tumblr.com/policy/en/user-guidelines",
    "X"=>"https://help.x.com/en/rules-and-policies/x-automation",
    "Blogger"=>"https://www.blogger.com/content-policy",
    _=>"https://www.redditinc.com/policies/content-policy",
}
}
impl Social {
    fn oauth_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, platform: &str) {
        if !oauth::supported(platform) {
            return;
        }
        let account = self.data.accounts.entry(platform.into()).or_default();
        ui.add_enabled_ui(!self.busy, |ui| {
            ui.label(match platform {
                "Tumblr" => "OAuth consumer key (application client ID)",
                "Threads" => "Threads App ID (not the regular Meta App ID)",
                "Instagram" => "Instagram App ID (from Instagram business login settings)",
                _ => "OAuth 2.0 client ID",
            });
            self.dirty |= ui.text_edit_singleline(&mut account.client_id).changed();
            if !matches!(platform, "X" | "Blogger") {
                ui.label("HTTPS callback page on your own website");
                self.dirty |= ui.add(egui::TextEdit::singleline(&mut account.redirect_uri).hint_text("https://your-website/sepia-oauth.html").desired_width(f32::INFINITY)).changed();
                if button(ui, "Save callback page for my website", ButtonStyle::Secondary, true).clicked() {
                    if let Some(path) = rfd::FileDialog::new().set_file_name("sepia-oauth.html").save_file() {
                        match std::fs::write(path, oauth::CALLBACK_PAGE) {
                            Ok(()) => self.status = "Callback page saved. Upload it to your HTTPS website without adding analytics or other scripts.".into(),
                            Err(e) => self.error = e.to_string(),
                        }
                    }
                }
            } else {
                ui.label(format!("Desktop callback: {}", oauth::LOCAL_CALLBACK));
            }
            let secret = self.secrets.entry(format!("{platform} app secret")).or_default();
            if platform != "X" {
                ui.label("Application client secret (leave blank to reuse the saved secret)");
                ui.add(egui::TextEdit::singleline(secret).password(true).desired_width(f32::INFINITY));
            }
            if button(ui, &format!("Connect {platform}"), ButtonStyle::Primary, true).clicked() {
                match oauth::start(platform, account.clone(), secret.clone(), self.tx.clone(), ctx.clone()) {
                    Ok(sender) => {
                        secret.clear(); self.oauth_callback.clear();
                        self.oauth_pending = Some((platform.into(), sender));
                        self.busy = true; self.error.clear();
                        self.status = format!("Waiting for {platform} sign-in in your browser (up to five minutes)...");
                        self.dirty = true;
                    }
                    Err(e) => self.error = e,
                }
            }
        });
        if let Some(choices) = self.destinations.get(platform) {
            if choices.len() > 1 {
                ui.label("Choose the blog to publish to");
                egui::ComboBox::from_id_salt(("oauth-destination", platform))
                    .selected_text(
                        choices
                            .iter()
                            .find(|(id, _)| id == &account.identity)
                            .map(|(_, name)| name.as_str())
                            .unwrap_or("Select a blog"),
                    )
                    .show_ui(ui, |ui| {
                        for (id, name) in choices {
                            self.dirty |= ui
                                .selectable_value(&mut account.identity, id.clone(), name)
                                .changed();
                        }
                    });
            }
        }
        ui.separator();
    }
    pub fn ui(&mut self, ctx: &egui::Context) {
        self.poll();
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(24),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("some-page")
                    .show(ui, |ui| {
                        crate::appearance::reading_column(ui, |ui| {
                            if self.settings {
                                self.settings_ui(ui, ctx);
                            } else {
                                self.drafts_ui(ui, ctx);
                            }
                        });
                    });
            });
    }
    fn feedback(&self, ui: &mut egui::Ui) {
        if self.busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(&self.status);
            });
        } else if !self.status.is_empty() {
            ui.label(&self.status);
        }
        if !self.error.is_empty() {
            ui.colored_label(Color32::from_rgb(236, 158, 120), &self.error);
        }
    }
    fn drafts_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Social media");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if button(ui, "Link accounts", ButtonStyle::Secondary, !self.busy).clicked() {
                    self.settings = true;
                }
            });
        });
        ui.add_space(12.);
        self.feedback(ui);
        ui.add_enabled_ui(!self.busy,|ui|{
            ui.label("Write a post about...");
            self.dirty|=ui.add(egui::TextEdit::multiline(&mut self.data.information).desired_rows(6).desired_width(f32::INFINITY).hint_text("What changed? What can players try? Include only facts you want to share.")).changed();
            ui.label("Image prompt");
            self.dirty |= ui.add(egui::TextEdit::multiline(&mut self.data.image_prompt).desired_rows(3).desired_width(f32::INFINITY).hint_text("Describe the actual picture: subjects, actions, setting, mood and style. For example: cute foxes playing in a sunny meadow, storybook illustration.")).changed();
            ui.horizontal_wrapped(|ui| {
                if button(ui,"Upload style reference",ButtonStyle::Secondary,self.writable).clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png","jpg","jpeg","webp"]).pick_file() {
                        match images::import_reference(&path) {
                            Ok(path) => { self.data.style_reference = path.to_string_lossy().into_owned(); self.dirty = true; self.error.clear(); self.save(); },
                            Err(error) => self.error = error,
                        }
                    }
                }
                if !self.data.style_reference.is_empty() && button(ui,"Remove style reference",ButtonStyle::Quiet,self.writable).clicked() {
                    self.data.style_reference.clear(); self.dirty = true; self.save();
                }
            });
            if !self.data.style_reference.is_empty() {
                if let Some(texture) = image_texture(ctx, &mut self.images, &self.data.style_reference) {
                    ui.add(egui::Image::new(&texture).max_width(128.).max_height(128.));
                }
            }
            ui.label("Website link for this batch");
            self.dirty|=ui.add(egui::TextEdit::singleline(&mut self.data.website).desired_width(f32::INFINITY).hint_text("https://your-game.example")).changed();
            ui.horizontal_wrapped(|ui| {
                let enabled = !self.data.information.trim().is_empty() && self.writable;
                if button(ui,"Improve text",ButtonStyle::Secondary,enabled && !self.busy).clicked(){self.improve(ImproveTarget::Text,ctx);}
                if button(ui,"Improve image",ButtonStyle::Secondary,!self.busy && self.writable && (!self.data.image_prompt.trim().is_empty() || !self.data.information.trim().is_empty())).clicked(){self.improve(ImproveTarget::Image,ctx);}
                if button(ui,"Improve both",ButtonStyle::Secondary,enabled && !self.busy).clicked(){self.improve(ImproveTarget::Both,ctx);}
                if button(ui,"Generate text",ButtonStyle::Primary,enabled && !self.busy).clicked(){self.generate(false,ctx);}
                if button(ui,"Generate image",ButtonStyle::Secondary,!self.busy && self.writable && !self.data.image_prompt.trim().is_empty()).clicked(){self.generate_image(ctx);}
                if !self.data.shared_image_path.is_empty() && button(ui,"Remove image",ButtonStyle::Quiet,!self.busy && self.writable).clicked(){self.remove_image();}
            });
        });
        if !self.data.generated_images.is_empty() {
            let history = self.data.generated_images.clone();
            let selected = history
                .iter()
                .position(|image| image.path == self.data.shared_image_path);
            ui.label("Last 5 generated images");
            ui.horizontal_wrapped(|ui| {
                if button(
                    ui,
                    "Previous image",
                    ButtonStyle::Quiet,
                    !self.busy && selected.is_some_and(|i| i > 0),
                )
                .clicked()
                {
                    self.select_image(selected.unwrap() - 1);
                }
                ui.label(
                    selected
                        .map(|i| format!("{} / {}", i + 1, history.len()))
                        .unwrap_or_else(|| "No image selected".into()),
                );
                if button(
                    ui,
                    "Next image",
                    ButtonStyle::Quiet,
                    !self.busy && selected.is_some_and(|i| i + 1 < history.len()),
                )
                .clicked()
                {
                    self.select_image(selected.unwrap() + 1);
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (index, image) in history.iter().enumerate() {
                    ui.push_id(("image-history", index), |ui| {
                        if let Some(texture) = image_texture(ctx, &mut self.images, &image.path) {
                            let picture =
                                egui::Image::new(&texture).max_width(100.).max_height(100.);
                            if ui
                                .add_enabled(
                                    !self.busy,
                                    egui::ImageButton::new(picture)
                                        .selected(selected == Some(index)),
                                )
                                .on_hover_text(format!("Image {}: {}", index + 1, image.prompt))
                                .clicked()
                            {
                                self.select_image(index);
                            }
                        } else {
                            ui.label(format!("Image {} unavailable", index + 1));
                        }
                    });
                }
            });
        }
        if !self.data.shared_image_path.is_empty() {
            let path = self.data.shared_image_path.clone();
            if !self.images.contains_key(&path) {
                if let Ok(picture) = image::open(&path) {
                    let picture = picture.into_rgba8();
                    let color = egui::ColorImage::from_rgba_unmultiplied(
                        [picture.width() as usize, picture.height() as usize],
                        picture.as_raw(),
                    );
                    self.images.insert(
                        path.clone(),
                        ctx.load_texture(&path, color, egui::TextureOptions::LINEAR),
                    );
                }
            }
            if let Some(texture) = self.images.get(&path) {
                ui.add(egui::Image::new(texture).max_width(160.).max_height(160.));
            }
        }
        if self.data.posts.is_empty() {
            ui.add_space(24.);
            return;
        }
        ui.add_space(20.);
        ui.horizontal(|ui| {
            ui.heading("Post previews");
            if button(ui, "Approve all", ButtonStyle::Secondary, !self.busy).clicked() {
                for post in &mut self.data.posts {
                    if post.issues(&self.data.batch_website).is_empty()
                        && post.warnings.is_empty()
                        && (post.platform != "Reddit"
                            || self.data.rules.iter().any(|r| {
                                r.name == post.destination
                                    && !r.text.is_empty()
                                    && r.error.is_empty()
                            }))
                        && post.revision.trim().is_empty()
                        && post.state.is_empty()
                    {
                        post.approved = true;
                        post.publish_error.clear();
                    }
                }
                self.dirty = true;
            }
        });
        let website = &self.data.batch_website;
        let shared_image_path = self.data.shared_image_path.clone();
        let shared_image_alt: String = self.data.batch_image_prompt.chars().take(900).collect();
        let mut order: Vec<usize> = (0..self.data.posts.len()).collect();
        order.sort_by_key(|&index| match self.data.posts[index].platform.as_str() {
            "X" => 0,
            "Instagram" => 1,
            _ => 2,
        });
        let mut send_one = None;
        for index in order {
            let post = &mut self.data.posts[index];
            ui.push_id(index, |ui| {
                egui::Frame::new()
                    .fill(if manual(&post.platform) {
                        Color32::from_rgb(43, 41, 31)
                    } else {
                        Color32::from_rgb(38, 32, 27)
                    })
                    .stroke(egui::Stroke::new(1.0_f32, crate::appearance::BORDER))
                    .corner_radius(10)
                    .inner_margin(16)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.heading(if post.destination.is_empty() {
                                post.platform.clone()
                            } else {
                                format!("{} · r/{}", post.platform, post.destination)
                            });
                            ui.label(if manual(&post.platform) {
                                "Manual · copy only"
                            } else {
                                "Automatic · account required"
                            });
                            if post.approved {
                                ui.label("Approved");
                            }
                        });
                        ui.hyperlink_to("Platform guidelines", policy_url(&post.platform));
                        if !post.image_note.is_empty() {
                            ui.colored_label(Color32::from_rgb(233, 187, 114), &post.image_note);
                        }
                        if (post.platform == "Instagram" || post.attach_image)
                            && !post.card_path.is_empty()
                        {
                            let path = post.card_path.clone();
                            if !self.images.contains_key(&path) {
                                if let Ok(img) = image::open(&path) {
                                    let img = img.into_rgba8();
                                    let color = egui::ColorImage::from_rgba_unmultiplied(
                                        [img.width() as usize, img.height() as usize],
                                        img.as_raw(),
                                    );
                                    self.images.insert(
                                        path.clone(),
                                        ctx.load_texture(
                                            &path,
                                            color,
                                            egui::TextureOptions::LINEAR,
                                        ),
                                    );
                                }
                            }
                            if let Some(texture) = self.images.get(&path) {
                                ui.add(egui::Image::new(texture).max_width(360.).max_height(360.));
                            }
                            if ui.button("Save image…").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .set_file_name("game-update.png")
                                    .save_file()
                                {
                                    if let Err(e) = std::fs::copy(&post.card_path, path) {
                                        self.error = e.to_string();
                                    }
                                }
                            }
                            if button(
                                ui,
                                "Remove image from this post",
                                ButtonStyle::Quiet,
                                !self.busy && post.state.is_empty(),
                            )
                            .clicked()
                            {
                                generation::exclude_image(post);
                                self.dirty = true;
                            }
                        }
                        if post.image_excluded
                            && !post.blocked
                            && post.image_allowed.unwrap_or(post.platform != "Reddit")
                            && !shared_image_path.is_empty()
                            && network::image_unavailable(
                                &post.platform,
                                &network::effective_account(&self.data.accounts, &post.platform),
                            )
                            .is_none()
                            && button(
                                ui,
                                "Add image to this post",
                                ButtonStyle::Secondary,
                                !self.busy && post.state.is_empty(),
                            )
                            .clicked()
                        {
                            post.image_excluded = false;
                            post.card_path = shared_image_path.clone();
                            post.image_text = shared_image_alt.clone();
                            post.attach_image = true;
                            post.approved = false;
                            self.dirty = true;
                        }
                        ui.label(
                            RichText::new("Post text")
                                .small()
                                .color(crate::appearance::MUTED),
                        );
                        egui::Frame::new()
                            .fill(Color32::from_rgb(23, 21, 19))
                            .stroke(egui::Stroke::new(1.0_f32, crate::appearance::BORDER))
                            .corner_radius(6)
                            .inner_margin(12)
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.add_enabled_ui(
                                    !self.busy && post.state.is_empty() && !post.blocked,
                                    |ui| {
                                        let mut changed = false;
                                        if !post.title.is_empty()
                                            || matches!(
                                                post.platform.as_str(),
                                                "Medium"
                                                    | "Substack"
                                                    | "Blogger"
                                                    | "Tumblr"
                                                    | "Reddit"
                                            )
                                        {
                                            changed |= ui
                                                .add(
                                                    egui::TextEdit::singleline(&mut post.title)
                                                        .hint_text("Title")
                                                        .desired_width(f32::INFINITY),
                                                )
                                                .changed();
                                        }
                                        changed |= ui
                                            .add(
                                                egui::TextEdit::multiline(&mut post.body)
                                                    .desired_width(f32::INFINITY)
                                                    .desired_rows(8),
                                            )
                                            .changed();
                                        if changed {
                                            post.approved = false;
                                            post.publish_error.clear();
                                            self.dirty = true;
                                        }
                                    },
                                );
                            });
                        ui.label(
                            RichText::new(format!(
                                "{} / {} characters",
                                text_length(&post.platform, &post.body),
                                limit(&post.platform)
                            ))
                            .small(),
                        );
                        for warning in &post.warnings {
                            ui.colored_label(Color32::from_rgb(233, 187, 114), warning);
                        }
                        let issues = post.issues(website);
                        for issue in &issues {
                            ui.colored_label(Color32::from_rgb(236, 158, 120), issue);
                        }
                        if !post.state.is_empty() {
                            if post.state.starts_with("Check before retrying:") {
                                ui.colored_label(
                                    Color32::from_rgb(244, 112, 101),
                                    RichText::new(&post.state).strong(),
                                );
                            } else {
                                ui.label(RichText::new(&post.state).strong());
                            }
                        }
                        if !post.publish_error.is_empty()
                            && !post.state.starts_with("Check before retrying:")
                        {
                            ui.colored_label(
                                Color32::from_rgb(244, 112, 101),
                                RichText::new(&post.publish_error).strong(),
                            );
                        }
                        if !post.link.is_empty() {
                            ui.hyperlink_to("View published post", &post.link);
                        }
                        let rule_missing = post.platform == "Reddit"
                            && self
                                .data
                                .rules
                                .iter()
                                .find(|r| r.name == post.destination)
                                .is_none_or(|r| r.text.is_empty() || !r.error.is_empty());
                        let eligible = !self.busy
                            && post.state.is_empty()
                            && post.revision.trim().is_empty()
                            && issues.is_empty()
                            && !rule_missing;
                        ui.horizontal_wrapped(|ui| {
                            if button(
                                ui,
                                if post.approved {
                                    "Unapprove"
                                } else {
                                    "Approve"
                                },
                                ButtonStyle::Secondary,
                                eligible,
                            )
                            .clicked()
                            {
                                post.approved = !post.approved;
                                if post.approved {
                                    post.publish_error.clear();
                                }
                                self.dirty = true;
                            }
                            if ui.button("Copy post").clicked() {
                                ctx.copy_text(post.copy_text());
                            }
                            if manual(&post.platform) && !post.blocked {
                                if ui.button("Create new post").clicked() {
                                    ctx.open_url(egui::OpenUrl::new_tab(compose_url(
                                        post,
                                        self.data.accounts.get(&post.platform),
                                    )));
                                }
                                if button(
                                    ui,
                                    "Mark as posted",
                                    ButtonStyle::Quiet,
                                    post.approved && !self.busy,
                                )
                                .clicked()
                                {
                                    post.state = "Posted manually".into();
                                    post.approved = false;
                                    self.dirty = true;
                                }
                            } else if !manual(&post.platform)
                                && button(
                                    ui,
                                    "Send this post",
                                    ButtonStyle::Primary,
                                    eligible && post.approved,
                                )
                                .clicked()
                            {
                                send_one = Some(index);
                            }
                        });
                        if (post.state.starts_with("Check before retrying:")
                            || post.state.starts_with("Sending"))
                            && button(
                                ui,
                                "I checked the platform: not posted. Reset for retry",
                                ButtonStyle::Quiet,
                                !self.busy,
                            )
                            .clicked()
                        {
                            post.state.clear();
                            post.publish_error.clear();
                            post.approved = false;
                            self.dirty = true;
                        }
                        ui.add_enabled_ui(
                            !self.busy && post.state.is_empty() && !post.blocked,
                            |ui| {
                                ui.label("Suggest changes");
                                if ui
                                    .add(
                                        egui::TextEdit::multiline(&mut post.revision)
                                            .desired_rows(2)
                                            .desired_width(f32::INFINITY)
                                            .hint_text("What should change in this draft?"),
                                    )
                                    .changed()
                                {
                                    post.approved = false;
                                    self.dirty = true;
                                }
                            },
                        );
                    });
            });
            ui.add_space(16.);
        }
        if let Some(index) = send_one {
            self.publish(Some(index), ctx);
        }
        ui.horizontal_wrapped(|ui| {
            if button(
                ui,
                "Update",
                ButtonStyle::Secondary,
                !self.busy
                    && self
                        .data
                        .posts
                        .iter()
                        .any(|p| !p.revision.trim().is_empty() && p.state.is_empty() && !p.blocked),
            )
            .clicked()
            {
                self.generate(true, ctx);
            }
            let count = self
                .data
                .posts
                .iter()
                .filter(|p| p.approved && !manual(&p.platform) && p.state.is_empty())
                .count();
            if button(
                ui,
                &format!("Post approved ({count})"),
                ButtonStyle::Primary,
                !self.busy && count > 0,
            )
            .clicked()
            {
                self.publish(None, ctx);
            }
        });
        self.feedback(ui);
    }
    fn settings_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let before =
            serde_json::to_string(&(&self.data.accounts, &self.data.rules)).unwrap_or_default();
        if button(ui, "Back to generation", ButtonStyle::Secondary, !self.busy).clicked() {
            self.settings = false;
            self.save();
        }
        ui.heading("Link accounts & subreddit rules");
        self.feedback(ui);
        ui.collapsing("Shared image hosting (Threads and Blogger)", |ui| {
            let hosting = self.data.accounts.entry("Instagram".into()).or_default();
            ui.label("Cloudinary cloud name");
            self.dirty |= ui.text_edit_singleline(&mut hosting.cloud).changed();
            ui.label("Restricted unsigned upload preset");
            self.dirty |= ui.text_edit_singleline(&mut hosting.preset).changed();
            ui.hyperlink_to(
                "Image hosting setup",
                "https://cloudinary.com/documentation/upload_images#unsigned_upload",
            );
        });
        if let Some((platform, sender)) = &self.oauth_pending {
            ui.group(|ui| {
                ui.label(format!("Finish connecting {platform} in your browser."));
                ui.add(
                    egui::TextEdit::singleline(&mut self.oauth_callback)
                        .password(true)
                        .hint_text("Full callback address from the browser"),
                );
                ui.horizontal_wrapped(|ui| {
                    if button(
                        ui,
                        "Finish connection",
                        ButtonStyle::Primary,
                        !self.oauth_callback.trim().is_empty(),
                    )
                    .clicked()
                    {
                        let _ = sender.send(std::mem::take(&mut self.oauth_callback));
                    }
                    if button(ui, "Cancel connection", ButtonStyle::Secondary, true).clicked() {
                        let _ = sender.send(String::new());
                    }
                });
            });
        }
        ui.add_space(12.);
        for platform in PLATFORMS {
            egui::CollapsingHeader::new(format!("{platform} · {}",if manual(platform){"Manual copy"}else if self.credential_status.get(platform)==Some(&true){"Credential saved"}else{"Not linked"})).id_salt(platform).show(ui,|ui|{
                let (_,link)=guide(platform);ui.hyperlink_to("Open setup / publishing page",link);ui.hyperlink_to("Content guidelines",policy_url(platform));
                self.oauth_ui(ui,ctx,platform);
                let account=self.data.accounts.entry(platform.into()).or_default();
                if manual(platform) {
                    ui.label("Create-post page URL (optional, HTTPS)");
                    self.dirty |= ui.add(egui::TextEdit::singleline(&mut account.compose_url).desired_width(f32::INFINITY).hint_text("Paste your publication's new-post editor URL")).changed();
                    if !account.compose_url.trim().is_empty() && !url_valid(account.compose_url.trim()) { ui.colored_label(Color32::LIGHT_RED, "Enter a valid HTTPS URL without login credentials."); }
                }
                if !manual(platform){
                    ui.label(if platform=="Bluesky"{"Handle"}else{"Account / Page / blog ID (not needed for X or Mastodon)"});
                    self.dirty|=ui.text_edit_singleline(&mut account.identity).changed();
                    if platform=="Mastodon"{ui.label("Server origin");self.dirty|=ui.text_edit_singleline(&mut account.server).changed();}
                    let secret=self.secrets.entry(platform.into()).or_default();
                    ui.label(if platform=="Bluesky"{"App password"}else if oauth::supported(platform){"Optional: manually replace access token (Connect fills this securely for you)"}else{"User publishing access token"});
                    ui.add(egui::TextEdit::singleline(secret).password(true).desired_width(f32::INFINITY));
                    ui.horizontal_wrapped(|ui|{
                        if button(ui,"Save credential",ButtonStyle::Secondary,!self.busy && !secret.trim().is_empty()).clicked(){
                            match oauth::forget(platform).and_then(|()| network::save_credential(platform,secret)){Ok(())=>{secret.clear();self.credential_status.insert(platform.into(),true);self.status=format!("{platform} credential saved. Click Verify account to test access.");},Err(e)=>self.error=e}
                        }
                        if button(ui,"Verify account",ButtonStyle::Secondary,!self.busy && self.credential_status.get(platform)==Some(&true)).clicked(){
                            self.busy=true;self.error.clear();let a=account.clone();let tx=self.tx.clone();let ctx=ctx.clone();
                            std::thread::spawn(move||{let _=tx.send(Event::Connected(platform.into(),network::verify(platform,&a)));ctx.request_repaint();});
                        }
                        if button(ui,"Disconnect",ButtonStyle::Quiet,!self.busy && self.credential_status.get(platform)==Some(&true)).clicked(){match network::delete_credential(platform){Ok(())=>{self.credential_status.insert(platform.into(),false);},Err(e)=>self.error=e}}
                    });
                }
                ui.label("Additional account / community rules");
                self.dirty|=ui.add(egui::TextEdit::multiline(&mut account.notes).desired_rows(2).desired_width(f32::INFINITY)).changed();
            });
        }
        ui.add_space(20.);
        ui.heading("Reddit · manual drafts only");
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.new_subreddit)
                    .hint_text("Subreddit name, e.g. IndieGaming"),
            );
            if button(ui, "Add subreddit", ButtonStyle::Secondary, !self.busy).clicked() {
                let name = self.new_subreddit.trim().trim_start_matches("r/");
                if name.len() < 2
                    || name.len() > 30
                    || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    self.error =
                        "Enter just the subreddit name (letters, numbers, underscores).".into();
                } else if self
                    .data
                    .rules
                    .iter()
                    .any(|r| r.name.eq_ignore_ascii_case(name))
                {
                    self.error = "That subreddit is already saved.".into();
                } else {
                    self.data.rules.push(Rules {
                        name: name.into(),
                        ..Default::default()
                    });
                    self.new_subreddit.clear();
                    self.dirty = true;
                }
            }
        });
        if button(
            ui,
            "Refresh cached rules",
            ButtonStyle::Secondary,
            !self.busy && !self.data.rules.is_empty(),
        )
        .clicked()
        {
            self.busy = true;
            let rules = self.data.rules.clone();
            let tx = self.tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Event::Rules(network::refresh_rules(rules, true)));
                ctx.request_repaint();
            });
        }
        let mut remove = None;
        for (i, rule) in self.data.rules.iter_mut().enumerate() {
            ui.push_id(i,|ui|{
            ui.separator();ui.horizontal(|ui|{ui.label(RichText::new(format!("r/{}",rule.name)).strong());ui.hyperlink_to("Open rules",format!("https://www.reddit.com/r/{}/about/rules",rule.name));if ui.add_enabled(!self.busy,egui::Button::new("Remove")).clicked(){remove=Some(i);}});
            ui.label(if rule.fetched==0{"Not cached".into()}else{format!("Cached {}",chrono::DateTime::from_timestamp(rule.fetched,0).map(|d|d.format("%Y-%m-%d %H:%M UTC").to_string()).unwrap_or_default())});
            if !rule.error.is_empty(){ui.colored_label(Color32::from_rgb(236,158,120),&rule.error);}
            if ui.add_enabled(!self.busy,egui::TextEdit::multiline(&mut rule.text).desired_rows(4).desired_width(f32::INFINITY).hint_text("Paste all current rules, including link, self-promotion, emoji and AI restrictions.")).changed(){
                rule.fetched=chrono::Utc::now().timestamp();rule.source="Pasted and reviewed by user".into();rule.error.clear();self.dirty=true;
            }
        });
        }
        if let Some(i) = remove {
            self.data.rules.remove(i);
            self.dirty = true;
        }
        ui.add_space(12.);
        if button(ui, "Save settings", ButtonStyle::Primary, !self.busy).clicked() {
            self.dirty = true;
            self.save();
            self.status = "Settings saved locally.".into();
        }
        let after =
            serde_json::to_string(&(&self.data.accounts, &self.data.rules)).unwrap_or_default();
        if before != after {
            for post in &mut self.data.posts {
                post.approved = false;
                if post.state.is_empty() && !post.blocked && post.revision.is_empty() {
                    post.revision =
                        "Recheck this draft against the updated account and community rules."
                            .into();
                }
            }
            generation::attach_image(&mut self.data);
            self.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn composer_links_preserve_text_and_allow_custom_editors() {
        let post = super::Post {
            platform: "X".into(),
            body: "Foxes & friends 🦊\nhttps://game.example".into(),
            ..Default::default()
        };
        let url = reqwest::Url::parse(&super::compose_url(&post, None)).unwrap();
        assert_eq!(url.host_str(), Some("x.com"));
        assert_eq!(
            url.query_pairs().find(|(key, _)| key == "text").unwrap().1,
            post.body
        );
        let account = super::Account {
            compose_url: "https://foxes.substack.com/publish/post".into(),
            ..Default::default()
        };
        assert_eq!(
            super::compose_url(&post, Some(&account)),
            account.compose_url
        );
        assert!(!super::oauth::supported("X"));
    }
    use super::*;
    #[test]
    fn rendering_and_editing_drafts_do_not_trigger_generation_or_posting() {
        let directory = tempfile::tempdir().unwrap();
        let mut social = Social::new();
        social.path = directory.path().join("social.json");
        social.data = Data {
            information: "I added keyboard controls.".into(),
            website: "https://game.example".into(),
            ..Default::default()
        };
        let context = egui::Context::default();
        crate::appearance::install_emoji_fonts(&context);
        for settings in [false, true, false] {
            social.settings = settings;
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100., 800.),
                    )),
                    ..Default::default()
                },
                |ctx| social.ui(ctx),
            );
            assert!(!output.shapes.is_empty());
            assert!(!social.busy);
            assert!(social.data.thread.is_none());
            assert!(social.rx.try_recv().is_err());
        }
        assert!(social.data.posts.is_empty());
        assert!(context.fonts(|fonts| fonts.has_glyphs(&egui::FontId::proportional(18.), "🦊🎮😊")));
        // Exercise the expanded sign-in forms without clicking any action.
        let _ = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                for platform in ["Tumblr", "Threads", "X", "Blogger", "Instagram"] {
                    ui.push_id(platform, |ui| social.oauth_ui(ui, ctx, platform));
                }
            });
        });
        assert!(!social.busy);
        assert!(social.oauth_pending.is_none());
        assert!(social.rx.try_recv().is_err());
    }
}
