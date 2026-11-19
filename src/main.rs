#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod appearance;
mod bridge;
mod code_images;
mod social;
mod state;
use appearance::{button, ButtonStyle};
use eframe::egui::{self, Color32, RichText};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use serde_json::{json, Value};
use state::{Message, Mode, Store};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    time::{Duration, Instant},
};

struct Active {
    chat: usize,
    mode: Mode,
    prompt: String,
    turn: Option<String>,
    stopping: bool,
    steers: VecDeque<usize>,
}
struct Approval {
    id: Value,
    method: String,
    params: Value,
    answers: HashMap<String, String>,
}
struct App {
    models: Vec<bridge::Model>,
    store: Store,
    path: PathBuf,
    storage_ok: bool,
    bridge: Option<bridge::Bridge>,
    pending: HashMap<u64, String>,
    next_id: u64,
    steering: Option<(u64, usize, usize)>,
    code_images: code_images::Previews,
    ready: bool,
    signed_in: bool,
    active: Option<Active>,
    approvals: VecDeque<Approval>,
    status: String,
    error: String,
    login_url: String,
    dirty: bool,
    last_save: Instant,
    markdown_cache: CommonMarkCache,
    social: social::Social,
    social_tab: bool,
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        appearance::install_emoji_fonts(&cc.egui_ctx);
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.override_text_color = Some(Color32::from_rgb(224, 204, 172));
        style.visuals.panel_fill = Color32::from_rgb(30, 27, 24);
        style.visuals.window_fill = Color32::from_rgb(39, 34, 29);
        style.visuals.extreme_bg_color = Color32::from_rgb(23, 21, 19);
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(70, 57, 43);
        style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(70, 57, 43);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(103, 81, 56);
        style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(103, 81, 56);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(128, 98, 63);
        style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(128, 98, 63);
        style.visuals.selection.bg_fill = Color32::from_rgb(103, 77, 48);
        style.spacing.item_spacing = egui::vec2(10., 10.);
        style.spacing.button_padding = egui::vec2(14., 9.);
        style.visuals.widgets.noninteractive.bg_stroke =
            egui::Stroke::new(1.0_f32, appearance::BORDER);
        style.visuals.hyperlink_color = Color32::from_rgb(211, 169, 115);
        style.visuals.code_bg_color = Color32::from_rgb(28, 24, 20);
        cc.egui_ctx.set_style(style);
        let path = state::storage_path();
        let loaded = Store::load(&path);
        let storage_ok = loaded.is_ok();
        let error = loaded.as_ref().err().cloned().unwrap_or_default();
        let mut store = loaded.unwrap_or_default();
        if store.chats.is_empty() {
            store.new_chat();
        }
        store.selected = store.selected.min(store.chats.len() - 1);
        store.folder = store.chats[store.selected].folder.clone();
        let mut app = Self {
            models: Vec::new(),
            store,
            path,
            storage_ok,
            bridge: None,
            pending: HashMap::new(),
            next_id: 0,
            steering: None,
            code_images: code_images::Previews::default(),
            ready: false,
            signed_in: false,
            active: None,
            approvals: VecDeque::new(),
            status: "Connecting to Codex…".into(),
            error,
            login_url: String::new(),
            dirty: false,
            last_save: Instant::now(),
            markdown_cache: CommonMarkCache::default(),
            social: social::Social::new(),
            social_tab: false,
        };
        app.connect(&cc.egui_ctx);
        app
    }
    fn connect(&mut self, ctx: &egui::Context) {
        self.abandon_steers();
        self.models.clear();
        self.bridge = None;
        self.ready = false;
        self.signed_in = false;
        self.pending.clear();
        self.steering = None;
        self.active = None;
        self.approvals.clear();
        match bridge::Bridge::start(ctx.clone()) {
            Ok(b) => {
                self.bridge = Some(b);
                self.request("initialize",json!({"clientInfo":{"name":"sepia_chat","title":"Sepia Chat","version":"0.1.0"},"capabilities":{"experimentalApi":true}}));
            }
            Err(e) => self.error = e,
        }
    }
    fn request(&mut self, method: &str, params: Value) {
        self.next_id += 1;
        self.pending.insert(self.next_id, method.into());
        if let Some(b) = &self.bridge {
            b.request(self.next_id, method, params);
        }
    }
    fn normalize_model(&mut self) {
        let Some(model) = self
            .models
            .iter()
            .find(|m| m.model == self.store.model)
            .or_else(|| self.models.iter().find(|m| m.is_default))
            .or_else(|| self.models.first())
        else {
            return;
        };
        let before = (self.store.model.clone(), self.store.reasoning.clone());
        self.store.model = model.model.clone();
        if !model.supports(&self.store.reasoning) {
            self.store.reasoning = if model.supports(&model.default_reasoning_effort) {
                model.default_reasoning_effort.clone()
            } else {
                model
                    .supported_reasoning_efforts
                    .first()
                    .map(|option| option.reasoning_effort.clone())
                    .unwrap_or_default()
            };
        }
        self.dirty |= before != (self.store.model.clone(), self.store.reasoning.clone());
    }
    fn model_controls(&mut self, ui: &mut egui::Ui, busy: bool) {
        ui.add_enabled_ui(!busy && !self.models.is_empty(), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Model");
                let before = (self.store.model.clone(), self.store.reasoning.clone());
                let label = self
                    .models
                    .iter()
                    .find(|m| m.model == self.store.model)
                    .map(|m| m.display_name.as_str())
                    .unwrap_or("Loading…");
                egui::ComboBox::from_id_salt("code_model")
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        for model in &self.models {
                            ui.selectable_value(
                                &mut self.store.model,
                                model.model.clone(),
                                &model.display_name,
                            );
                        }
                    });
                self.normalize_model();
                ui.label("Reasoning");
                if let Some(model) = self.models.iter().find(|m| m.model == self.store.model) {
                    egui::ComboBox::from_id_salt("code_reasoning")
                        .selected_text(if self.store.reasoning.is_empty() {
                            "Default"
                        } else {
                            &self.store.reasoning
                        })
                        .show_ui(ui, |ui| {
                            for option in &model.supported_reasoning_efforts {
                                ui.selectable_value(
                                    &mut self.store.reasoning,
                                    option.reasoning_effort.clone(),
                                    &option.reasoning_effort,
                                );
                            }
                        });
                }
                ui.label("Permissions: Approve for me");
                self.dirty |= before != (self.store.model.clone(), self.store.reasoning.clone());
            });
        });
    }
    fn persist(&mut self) {
        if self.dirty && self.storage_ok {
            match self.store.save(&self.path) {
                Ok(()) => self.dirty = false,
                Err(e) => self.error = format!("Could not save chats: {e}"),
            }
        }
        self.last_save = Instant::now();
    }
    fn send(&mut self, mode: Mode) {
        let i = self.store.selected;
        let chat = &mut self.store.chats[i];
        if !std::path::Path::new(&chat.folder).is_dir() {
            self.error = "Choose an existing project folder first.".into();
            return;
        }
        let prompt = mode.prompt(&chat.draft);
        if chat.messages.is_empty() {
            chat.title = chat.draft.trim().chars().take(36).collect();
        }
        chat.messages.push(Message {
            role: format!("You · {}", mode.label()),
            text: prompt.clone(),
            item: String::new(),
            ..Default::default()
        });
        chat.draft.clear();
        let thread = chat.thread.clone();
        let folder = chat.folder.clone();
        self.active = Some(Active {
            chat: i,
            mode,
            prompt,
            turn: None,
            stopping: false,
            steers: VecDeque::new(),
        });
        self.dirty = true;
        self.persist();
        self.status = "Preparing conversation…".into();
        self.error.clear();
        let mut params = json!({"cwd":folder,"sandbox":if mode == Mode::Code {"workspace-write"} else {"read-only"},"approvalPolicy":mode.approval_policy(),"approvalsReviewer":mode.reviewer(),"model":self.store.model});
        if let Some(id) = thread {
            params["threadId"] = json!(id);
            self.request("thread/resume", params);
        } else {
            self.request("thread/start", params);
        }
    }
    fn turn_params(&self) -> Option<Value> {
        let a = self.active.as_ref()?;
        let chat = &self.store.chats[a.chat];
        Some(
            json!({"threadId":chat.thread,"cwd":chat.folder,"input":[{"type":"text","text":a.prompt}],"sandboxPolicy":a.mode.policy(&chat.folder),"approvalPolicy":a.mode.approval_policy(),"approvalsReviewer":a.mode.reviewer(),"model":self.store.model,"effort":if self.store.reasoning.is_empty() {None} else {Some(&self.store.reasoning)}}),
        )
    }
    fn start_turn(&mut self) {
        let Some(a) = &self.active else { return };
        if a.stopping {
            let chat = a.chat;
            self.mark_stopped(chat);
            self.active = None;
            self.status = "Stopped".into();
            self.persist();
            return;
        }
        let params = self.turn_params().unwrap();
        self.request("turn/start", params);
        self.status = "Codex is working…".into();
    }
    fn steer(&mut self) {
        let Some(a) = &mut self.active else { return };
        if a.stopping {
            return;
        }
        let chat = &mut self.store.chats[a.chat];
        if chat.draft.trim().is_empty() {
            return;
        }
        a.steers.push_back(chat.messages.len());
        chat.messages.push(Message {
            role: "You · Steer (pending)".into(),
            text: std::mem::take(&mut chat.draft),
            ..Default::default()
        });
        self.dirty = true;
        self.persist();
        self.flush_steer();
    }
    fn flush_steer(&mut self) {
        if self.steering.is_some() {
            return;
        }
        let Some(a) = &mut self.active else { return };
        if a.stopping || a.turn.is_none() {
            return;
        }
        let Some(index) = a.steers.pop_front() else {
            return;
        };
        let chat_index = a.chat;
        let chat = &self.store.chats[chat_index];
        let params = json!({"threadId":chat.thread,"expectedTurnId":a.turn,
            "input":[{"type":"text","text":chat.messages[index].text}]});
        self.request("turn/steer", params);
        self.steering = Some((self.next_id, chat_index, index));
    }
    fn finish_steers(&mut self) {
        if let Some(a) = &mut self.active {
            for index in a.steers.drain(..) {
                self.store.chats[a.chat].messages[index].role = "You · Steer (not sent)".into();
            }
        }
    }
    fn abandon_steers(&mut self) {
        self.finish_steers();
        if let Some((_, chat, index)) = self.steering.take() {
            self.store.chats[chat].messages[index].role = "You · Steer (delivery unknown)".into();
        }
        self.dirty = true;
    }
    fn mark_stopped(&mut self, chat: usize) {
        self.store.chats[chat].messages.push(Message {
            role: "System".into(),
            text: "Stopped".into(),
            ..Default::default()
        });
        self.dirty = true;
    }
    fn stop(&mut self) {
        self.finish_steers();
        if let Some(a) = &mut self.active {
            a.stopping = true;
            if let Some(turn) = a.turn.clone() {
                let thread = self.store.chats[a.chat].thread.clone();
                self.request("turn/interrupt", json!({"threadId":thread,"turnId":turn}));
            }
        }
        while let Some(p) = self.approvals.pop_front() {
            self.respond_approval(p, false);
        }
        self.status = "Stopping…".into();
    }
    fn respond_approval(&mut self, p: Approval, allow: bool) {
        let allow = allow
            && self
                .active
                .as_ref()
                .is_some_and(|a| a.mode == Mode::Code && !a.stopping);
        let result = if p.method == "item/tool/requestUserInput" {
            json!({"answers":{}})
        } else if p.method == "item/permissions/requestApproval" {
            json!({"permissions":if allow {p.params["permissions"].clone()} else {json!({})},"scope":"turn"})
        } else {
            json!({"decision":if allow {"accept"} else {"decline"}})
        };
        if let Some(b) = &self.bridge {
            b.send(json!({"id":p.id,"result":result}));
        }
    }
    fn event(&mut self, v: Value) {
        if let Some(method) = v["method"].as_str() {
            let p = &v["params"];
            if !v["id"].is_null() {
                let approval = Approval {
                    id: v["id"].clone(),
                    method: method.into(),
                    params: p.clone(),
                    answers: HashMap::new(),
                };
                let belongs_to_active = self.active.as_ref().is_some_and(|a| {
                    !a.stopping
                        && p["threadId"].as_str() == self.store.chats[a.chat].thread.as_deref()
                });
                if matches!(
                    method,
                    "item/commandExecution/requestApproval"
                        | "item/fileChange/requestApproval"
                        | "item/permissions/requestApproval"
                ) {
                    if belongs_to_active
                        && self.active.as_ref().is_some_and(|a| a.mode == Mode::Code)
                    {
                        self.approvals.push_back(approval);
                    } else {
                        self.respond_approval(approval, false);
                    }
                } else if method == "item/tool/requestUserInput" {
                    if belongs_to_active {
                        self.approvals.push_back(approval);
                    } else {
                        self.respond_approval(approval, false);
                    }
                } else if let Some(b) = &self.bridge {
                    b.send(json!({"id":v["id"],"error":{"code":-32601,"message":"This client does not support this request; no permission granted."}}));
                }
                return;
            }
            if method == "account/login/completed" || method == "account/updated" {
                self.request("account/read", json!({}));
            }
            let Some(a) = &self.active else {
                return;
            };
            if p["threadId"].as_str() != self.store.chats[a.chat].thread.as_deref() {
                return;
            }
            let index = a.chat;
            if let (Some(current), Some(incoming)) = (
                a.turn.as_deref(),
                p["turnId"].as_str().or(p["turn"]["id"].as_str()),
            ) {
                if current != incoming {
                    return;
                }
            }
            match method {
                "turn/started" => {
                    if let Some(a) = &mut self.active {
                        a.turn = p["turn"]["id"].as_str().map(str::to_owned);
                        if a.stopping {
                            self.stop();
                        }
                    }
                    self.flush_steer();
                }
                "item/agentMessage/delta" => {
                    let id = p["itemId"].as_str().unwrap_or("");
                    let text = p["delta"].as_str().unwrap_or("");
                    let messages = &mut self.store.chats[index].messages;
                    if let Some(m) = messages
                        .iter_mut()
                        .find(|m| m.item == id && m.role == "Codex")
                    {
                        m.text.push_str(text);
                    } else {
                        messages.push(Message {
                            role: "Codex".into(),
                            text: text.into(),
                            item: id.into(),
                            ..Default::default()
                        });
                    }
                    self.dirty = true;
                }
                "item/completed" => {
                    let item = &p["item"];
                    let images = code_images::viewed_images(item, &self.store.chats[index].folder);
                    if !images.is_empty() {
                        let id = item["id"].as_str().unwrap_or("");
                        if !self.store.chats[index]
                            .messages
                            .iter()
                            .any(|m| m.item == id && !m.images.is_empty())
                        {
                            self.store.chats[index].messages.push(Message {
                                role: "Codex · Viewed image".into(),
                                item: id.into(),
                                images,
                                ..Default::default()
                            });
                            self.dirty = true;
                        }
                    }
                    if item["type"] == "agentMessage" {
                        let id = item["id"].as_str().unwrap_or("");
                        let text = item["text"].as_str().unwrap_or("");
                        let messages = &mut self.store.chats[index].messages;
                        if let Some(m) = messages
                            .iter_mut()
                            .find(|m| m.item == id && m.role == "Codex")
                        {
                            m.text = text.into();
                        } else {
                            messages.push(Message {
                                role: "Codex".into(),
                                text: text.into(),
                                item: id.into(),
                                ..Default::default()
                            });
                        }
                        self.dirty = true;
                    }
                }
                "item/started" => {
                    self.status = format!(
                        "Working · {}",
                        p["item"]["type"].as_str().unwrap_or("thinking")
                    );
                }
                "turn/completed" => {
                    self.finish_steers();
                    if p["turn"]["status"] == "interrupted" {
                        self.mark_stopped(index);
                    }
                    self.status = format!(
                        "Turn {}",
                        p["turn"]["status"].as_str().unwrap_or("completed")
                    );
                    if p["turn"]["status"] == "interrupted" {
                        self.status = "Stopped".into();
                    }
                    if !p["turn"]["error"].is_null() {
                        self.error = p["turn"]["error"].to_string();
                    }
                    self.active = None;
                    self.approvals.clear();
                    self.dirty = true;
                    self.persist();
                }
                "error" => {
                    self.error = p["error"]["message"]
                        .as_str()
                        .unwrap_or("Codex reported an error")
                        .into()
                }
                _ => {}
            }
            return;
        }
        let Some(id) = v["id"].as_u64() else {
            return;
        };
        let Some(method) = self.pending.remove(&id) else {
            return;
        };
        if method == "turn/steer" {
            if let Some((request, chat, index)) = self.steering {
                if request == id {
                    self.store.chats[chat].messages[index].role = if v["error"].is_null() {
                        "You · Steer"
                    } else {
                        "You · Steer (not sent)"
                    }
                    .into();
                    self.steering = None;
                    self.dirty = true;
                    if v["error"].is_null() {
                        self.flush_steer();
                    } else {
                        self.finish_steers();
                    }
                }
            }
        }
        if !v["error"].is_null() {
            self.error = format!("{method}: {}", v["error"]["message"]);
            if method == "turn/interrupt" {
                if let Some(a) = &mut self.active {
                    a.stopping = false;
                }
                self.status = "Could not stop; try Stop again".into();
            }
            if matches!(
                method.as_str(),
                "thread/start" | "thread/resume" | "turn/start"
            ) {
                self.abandon_steers();
                self.active = None;
                self.status = "Request failed".into();
            }
            return;
        }
        let r = &v["result"];
        match method.as_str() {
            "initialize" => {
                if let Some(b) = &self.bridge {
                    b.send(json!({"method":"initialized","params":{}}));
                }
                self.ready = true;
                self.request("account/read", json!({}));
            }
            "account/read" => {
                self.signed_in = r["account"]["type"] == "chatgpt";
                self.status = if self.signed_in {
                    "Connected with ChatGPT"
                } else {
                    "Sign in with ChatGPT to begin"
                }
                .into();
                if self.signed_in {
                    self.login_url.clear();
                    self.models.clear();
                    self.request("model/list", json!({"includeHidden":false}));
                }
            }
            "model/list" => {
                match serde_json::from_value::<Vec<bridge::Model>>(r["data"].clone()) {
                    Ok(models) => self.models.extend(models),
                    Err(e) => {
                        self.error = format!("Could not load models: {e}");
                        return;
                    }
                }
                if let Some(cursor) = r["nextCursor"].as_str() {
                    self.request("model/list", json!({"cursor":cursor,"includeHidden":false}));
                } else {
                    self.normalize_model();
                }
            }
            "account/login/start" => {
                self.login_url = r["authUrl"].as_str().unwrap_or("").into();
                self.status = "Complete sign-in using the link below.".into();
            }
            "thread/start" | "thread/resume" => {
                if let Some(a) = &self.active {
                    if let Some(id) = r["thread"]["id"].as_str() {
                        self.store.chats[a.chat].thread = Some(id.into());
                        self.dirty = true;
                        self.persist();
                        self.start_turn();
                    } else {
                        self.error = "Codex returned no conversation ID.".into();
                        self.active = None;
                    }
                }
            }
            "turn/start" => {
                if let Some(a) = &mut self.active {
                    a.turn = r["turn"]["id"].as_str().map(str::to_owned);
                    if a.stopping {
                        self.stop();
                    }
                }
                self.flush_steer();
            }
            _ => {}
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let events: Vec<_> = self
            .bridge
            .as_ref()
            .map(|b| b.events.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                Ok(v) => self.event(v),
                Err(e) => {
                    self.error = e;
                    self.ready = false;
                    self.abandon_steers();
                    self.active = None;
                    self.approvals.clear();
                    self.bridge = None;
                }
            }
        }
        self.social.poll();
        egui::TopBottomPanel::top("navigation")
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin::symmetric(24, 10)),
            )
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.selectable_label(self.social_tab, "SoMe").clicked() {
                        self.social_tab = true;
                    }
                    if ui.selectable_label(!self.social_tab, "Code").clicked() {
                        self.social_tab = false;
                    }
                });
            });
        let busy = self.active.is_some();
        let stopping = self.active.as_ref().is_some_and(|active| active.stopping);
        egui::TopBottomPanel::top("top")
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin::symmetric(24, 16)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if button(
                        ui,
                        "Choose project folder",
                        ButtonStyle::Secondary,
                        !busy && !self.social.busy,
                    )
                    .clicked()
                    {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            let folder = folder.to_string_lossy().into_owned();
                            if folder != self.store.folder {
                                self.store.folder = folder;
                                self.store.new_chat();
                                self.dirty = true;
                            }
                        }
                    }
                    ui.add_space(6.);
                    ui.add(
                        egui::Label::new(
                            RichText::new(if self.store.folder.is_empty() {
                                "No project selected"
                            } else {
                                &self.store.folder
                            })
                            .color(appearance::MUTED),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&self.store.folder);
                });
            });
        if self.social_tab {
            self.social.project_folder = self.store.folder.clone();
            self.social.ui(ctx);
            if self.last_save.elapsed() > Duration::from_secs(1) {
                self.persist();
            }
            ctx.request_repaint_after(Duration::from_millis(250));
            return;
        }
        egui::SidePanel::left("chats")
            .default_width(240.)
            .width_range(200. ..=320.)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(26, 23, 20))
                    .inner_margin(16),
            )
            .show(ctx, |ui| {
                ui.add_space(8.);
                ui.label(
                    RichText::new("CONVERSATIONS")
                        .size(11.)
                        .color(appearance::MUTED),
                );
                ui.add_space(4.);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, chat) in self.store.chats.iter().enumerate().rev() {
                        let response = ui
                            .push_id(i, |ui| {
                                ui.add_enabled_ui(!busy, |ui| {
                                    appearance::chat_row(
                                        ui,
                                        &chat.title,
                                        &chat.folder,
                                        i == self.store.selected,
                                    )
                                })
                                .inner
                            })
                            .inner;
                        if response.clicked() {
                            self.store.selected = i;
                            self.store.folder = chat.folder.clone();
                            self.dirty = true;
                        }
                    }
                });
            });
        egui::TopBottomPanel::bottom("compose")
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(24),
            )
            .show(ctx, |ui| {
                appearance::reading_column(ui, |ui| {
                    ui.horizontal(|ui| {
                        if busy {
                            ui.add(egui::Spinner::new().size(15.).color(appearance::TEXT));
                        } else {
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(10., 10.), egui::Sense::hover());
                            ui.painter()
                                .circle_filled(rect.center(), 3., appearance::MUTED);
                        }
                        ui.label(
                            RichText::new(&self.status)
                                .size(13.)
                                .color(appearance::MUTED),
                        );
                        if busy {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if button(
                                        ui,
                                        if stopping { "Stopping…" } else { "Stop" },
                                        ButtonStyle::Stop,
                                        !stopping,
                                    )
                                    .clicked()
                                    {
                                        self.stop();
                                    }
                                },
                            );
                        }
                    });
                    if !self.error.is_empty() {
                        ui.colored_label(Color32::from_rgb(236, 158, 120), &self.error);
                    }
                    if !self.ready
                        && button(ui, "Reconnect to Codex", ButtonStyle::Secondary, true).clicked()
                    {
                        self.connect(ctx);
                    }
                    if self.ready
                        && !self.signed_in
                        && button(ui, "Sign in with ChatGPT", ButtonStyle::Secondary, true)
                            .clicked()
                    {
                        self.request("account/login/start", json!({"type":"chatgpt"}));
                    }
                    if !self.login_url.is_empty() {
                        ui.hyperlink_to("Open ChatGPT sign-in", &self.login_url);
                    }
                    ui.add_space(4.);
                    self.model_controls(ui, busy);
                    let draft = &mut self.store.chats[self.store.selected].draft;
                    if ui
                        .add(
                            egui::TextEdit::multiline(draft)
                                .desired_rows(4)
                                .desired_width(f32::INFINITY)
                                .margin(egui::vec2(16., 16.))
                                .hint_text(
                                    "Describe your idea, ask a question, or plan the next change…",
                                ),
                        )
                        .changed()
                    {
                        self.dirty = true;
                    }
                    let can_send = !busy
                        && self.ready
                        && self.signed_in
                        && self.models.iter().any(|m| m.model == self.store.model)
                        && !draft.trim().is_empty()
                        && !self.store.folder.is_empty()
                        && self.storage_ok;
                    ui.add_space(2.);
                    ui.horizontal_wrapped(|ui| {
                        if button(ui, "New Chat", ButtonStyle::Quiet, !busy).clicked() {
                            self.store.new_chat();
                            self.dirty = true;
                        }
                        let can_steer = busy
                            && !stopping
                            && self.storage_ok
                            && !self.store.chats[self.store.selected]
                                .draft
                                .trim()
                                .is_empty();
                        if button(
                            ui,
                            if busy { "Steer" } else { "No code" },
                            ButtonStyle::Secondary,
                            if busy { can_steer } else { can_send },
                        )
                        .on_hover_text(if busy {
                            "Send guidance to the current run"
                        } else {
                            Mode::NoCode.suffix()
                        })
                        .clicked()
                        {
                            if busy {
                                self.steer();
                            } else {
                                self.send(Mode::NoCode);
                            }
                        }
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(true),
                            |ui| {
                                for mode in [Mode::Code, Mode::Enhance, Mode::Ask] {
                                    let style = if mode == Mode::Code {
                                        ButtonStyle::Primary
                                    } else {
                                        ButtonStyle::Secondary
                                    };
                                    if button(ui, mode.label(), style, can_send)
                                        .on_hover_text(mode.suffix())
                                        .clicked()
                                    {
                                        self.send(mode);
                                    }
                                }
                            },
                        );
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(ctx.style().visuals.panel_fill).inner_margin(24))
            .show(ctx, |ui| {
                let chat = &self.store.chats[self.store.selected];
                egui::ScrollArea::vertical().id_salt(("messages", self.store.selected))
                    .stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                    appearance::reading_column(ui, |ui| {
                        if chat.messages.is_empty() {
                            ui.add_space(44.);
                            ui.heading(RichText::new("What would you like to create?").size(26.));
                            ui.add_space(6.);
                            ui.label(RichText::new("Explore an idea, refine it, then bring it to life.").size(16.).color(appearance::MUTED));
                            ui.add_space(16.);
                            ui.label("Ask, Enhance, and No code can read your project. Code can make changes.");
                        }
                        for (i, message) in chat.messages.iter().enumerate() {
                            ui.push_id((self.store.selected, i), |ui| {
                                egui::Frame::new().fill(if message.role == "Codex" {
                                    Color32::from_rgb(36, 31, 26)
                                } else { Color32::from_rgb(44, 36, 28) })
                                    .stroke(egui::Stroke::new(1.0_f32, appearance::BORDER)).corner_radius(10)
                                    .inner_margin(16).show(ui, |ui| {
                                    ui.set_width((ui.available_width()).max(0.));
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(&message.role).strong());
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.small_button("Copy message").clicked() { ui.ctx().copy_text(message.text.clone()); }
                                        });
                                    });
                                    ui.add_space(6.);
                                    for source in &message.images {
                                        self.code_images.inline(ui, source);
                                    }
                                    if message.role == "Codex" {
                                        CommonMarkViewer::new().show(ui, &mut self.markdown_cache, &message.text);
                                    } else { ui.add(egui::Label::new(&message.text).wrap().selectable(true)); }
                                });
                            });
                            ui.add_space(16.);
                        }
                    });
                });
            });
        self.code_images.window(ctx);
        let mut decision = None;
        if let Some(p) = self.approvals.front_mut() {
            egui::Window::new("Codex needs your input")
                .collapsible(false)
                .resizable(true)
                .default_width(620.)
                .show(ctx, |ui| {
                    if p.method == "item/tool/requestUserInput" {
                        if let Some(questions) = p.params["questions"].as_array() {
                            for q in questions {
                                let id = q["id"].as_str().unwrap_or("").to_string();
                                ui.label(q["question"].as_str().unwrap_or("Question"));
                                let answer = p.answers.entry(id).or_default();
                                if let Some(options) = q["options"].as_array() {
                                    for o in options {
                                        let label = o["label"].as_str().unwrap_or("");
                                        if ui
                                            .button(label)
                                            .on_hover_text(o["description"].as_str().unwrap_or(""))
                                            .clicked()
                                        {
                                            *answer = label.into();
                                        }
                                    }
                                }
                                ui.text_edit_singleline(answer);
                            }
                        }
                        if ui.button("Send answers").clicked() {
                            decision = Some(2);
                        }
                    } else {
                        ui.label("Review this action before allowing it for this request.");
                        egui::ScrollArea::vertical()
                            .max_height(350.)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        serde_json::to_string_pretty(&p.params).unwrap_or_default(),
                                    )
                                    .selectable(true)
                                    .wrap(),
                                );
                            });
                        ui.horizontal(|ui| {
                            if ui.button("Deny").clicked() {
                                decision = Some(0);
                            }
                            if ui.button("Allow once").clicked() {
                                decision = Some(1);
                            }
                        });
                    }
                });
        }
        if let Some(decision) = decision {
            if let Some(p) = self.approvals.pop_front() {
                if decision == 2 {
                    let answers: serde_json::Map<String, Value> = p
                        .answers
                        .into_iter()
                        .map(|(id, text)| (id, json!({"answers":[text]})))
                        .collect();
                    if let Some(b) = &self.bridge {
                        b.send(json!({"id":p.id,"result":{"answers":answers}}));
                    }
                } else {
                    self.respond_approval(p, decision == 1);
                }
            }
        }
        if self.last_save.elapsed() > Duration::from_secs(1) {
            self.persist();
        }
        ctx.request_repaint_after(Duration::from_millis(250));
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop();
        self.persist();
        self.social.save();
    }
}
fn main() -> eframe::Result {
    eframe::run_native(
        "Sepia Chat",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1120., 800.])
                .with_min_inner_size([780., 580.]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn steering_waits_for_turn_and_delivers_in_order_without_mode_suffix() {
        let mut app = app(Mode::Code);
        app.active.as_mut().unwrap().turn = None;
        for text in ["Use blue", "Keep the header"] {
            app.store.chats[0].draft = text.into();
            app.steer();
        }
        assert!(app.pending.is_empty());
        assert_eq!(app.active.as_ref().unwrap().steers.len(), 2);
        app.event(json!({"method":"turn/started","params":{"threadId":"thread-1","turn":{"id":"turn-1"}}}));
        let (id, _, index) = app.steering.unwrap();
        assert_eq!(index, 0);
        assert_eq!(app.store.chats[0].messages[0].text, "Use blue");
        assert_eq!(app.active.as_ref().unwrap().mode, Mode::Code);
        app.event(json!({"id":id,"result":{"turnId":"turn-1"}}));
        let (id, _, index) = app.steering.unwrap();
        assert_eq!(index, 1);
        app.event(json!({"id":id,"result":{"turnId":"turn-1"}}));
        assert!(app.steering.is_none());
        assert!(app.store.chats[0]
            .messages
            .iter()
            .all(|m| m.role == "You · Steer"));
        assert!(app.store.chats[0].draft.is_empty());
    }
    #[test]
    fn steering_failure_preserves_text_and_does_not_start_a_new_turn() {
        let mut app = app(Mode::Ask);
        for text in ["First", "Second"] {
            app.store.chats[0].draft = text.into();
            app.steer();
        }
        let (id, _, _) = app.steering.unwrap();
        app.event(json!({"id":id,"error":{"message":"Turn ended"}}));
        assert!(app.store.chats[0]
            .messages
            .iter()
            .all(|m| m.role.ends_with("(not sent)")));
        assert!(!app.pending.values().any(|m| m == "turn/start"));
        assert!(app.active.is_some());
    }
    #[test]
    fn stop_preserves_output_and_prevents_queued_steering() {
        let mut app = app(Mode::Code);
        app.event(json!({"method":"item/agentMessage/delta","params":{"threadId":"thread-1","itemId":"m","delta":"Partial answer"}}));
        for text in ["First", "Second"] {
            app.store.chats[0].draft = text.into();
            app.steer();
        }
        app.stop();
        assert!(app.active.as_ref().unwrap().steers.is_empty());
        assert_eq!(
            app.store.chats[0].messages[2].role,
            "You · Steer (not sent)"
        );
        let count = app.store.chats[0].messages.len();
        app.store.chats[0].draft = "Do not send".into();
        app.steer();
        assert_eq!(app.store.chats[0].messages.len(), count);
        app.event(json!({"method":"turn/completed","params":{"threadId":"thread-1","turn":{"id":"turn-1","status":"interrupted"}}}));
        assert!(app.active.is_none());
        assert_eq!(app.status, "Stopped");
        assert_eq!(app.store.chats[0].messages[0].text, "Partial answer");
        assert_eq!(app.store.chats[0].messages.last().unwrap().text, "Stopped");
    }
    #[test]
    fn image_events_are_filtered_deduplicated_and_persisted() {
        let mut app = app(Mode::Ask);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("screenshot.png");
        image::RgbImage::from_pixel(4, 3, image::Rgb([1, 2, 3]))
            .save(&path)
            .unwrap();
        let event = json!({"method":"item/completed","params":{"threadId":"thread-1","item":{"type":"imageView","id":"image-1","path":path}}});
        app.event(event.clone());
        app.event(event);
        app.event(json!({"method":"item/completed","params":{"threadId":"other","item":{"type":"imageView","id":"image-2","path":path}}}));
        std::fs::remove_file(path).unwrap();
        let messages = &app.store.chats[0].messages;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].images.len(), 1);
        let saved = serde_json::to_value(&app.store).unwrap();
        let loaded: Store = serde_json::from_value(saved).unwrap();
        assert_eq!(loaded.chats[0].messages[0].images, messages[0].images);
    }
    #[test]
    fn code_turn_uses_selected_model_effort_and_auto_review() {
        let mut app = app(Mode::Code);
        app.store.model = "test-model".into();
        app.store.reasoning = "high".into();
        let params = app.turn_params().unwrap();
        assert_eq!(params["model"], "test-model");
        assert_eq!(params["effort"], "high");
        assert_eq!(params["approvalPolicy"], "on-request");
        assert_eq!(params["approvalsReviewer"], "auto_review");
        assert_eq!(params["sandboxPolicy"]["type"], "workspaceWrite");
        for mode in [Mode::Ask, Mode::Enhance, Mode::NoCode] {
            app.active.as_mut().unwrap().mode = mode;
            let params = app.turn_params().unwrap();
            assert_eq!(params["approvalPolicy"], "never");
            assert_eq!(params["sandboxPolicy"]["type"], "readOnly");
        }
    }
    #[test]
    fn model_selection_uses_catalog_defaults_and_valid_reasoning() {
        let mut app = app(Mode::Code);
        app.models = serde_json::from_value(json!([
            {"model":"first","displayName":"First","isDefault":false,"defaultReasoningEffort":"low","supportedReasoningEfforts":[{"reasoningEffort":"low"}]},
            {"model":"second","displayName":"Second","isDefault":true,"defaultReasoningEffort":"high","supportedReasoningEfforts":[{"reasoningEffort":"high"}]}
        ])).unwrap();
        app.normalize_model();
        assert_eq!(app.store.model, "second");
        assert_eq!(app.store.reasoning, "high");
        app.store.model = "first".into();
        app.normalize_model();
        assert_eq!(app.store.reasoning, "low");
    }
    fn app(mode: Mode) -> App {
        let mut store = Store::default();
        store.new_chat();
        store.chats[0].thread = Some("thread-1".into());
        App {
            models: Vec::new(),
            store,
            path: PathBuf::new(),
            storage_ok: false,
            bridge: None,
            pending: HashMap::new(),
            next_id: 0,
            steering: None,
            code_images: code_images::Previews::default(),
            ready: true,
            signed_in: true,
            active: Some(Active {
                chat: 0,
                mode,
                prompt: "hello".into(),
                turn: Some("turn-1".into()),
                stopping: false,
                steers: VecDeque::new(),
            }),
            approvals: VecDeque::new(),
            status: String::new(),
            error: String::new(),
            login_url: String::new(),
            dirty: false,
            last_save: Instant::now(),
            markdown_cache: CommonMarkCache::default(),
            social: social::Social::new(),
            social_tab: false,
        }
    }
    #[test]
    fn streaming_updates_one_message_and_filters_other_threads() {
        let mut app = app(Mode::Ask);
        for (thread, text) in [
            ("thread-1", "Hello"),
            ("other", "wrong"),
            ("thread-1", " world"),
        ] {
            app.event(json!({"method":"item/agentMessage/delta","params":{"threadId":thread,"itemId":"message-1","delta":text}}));
        }
        app.event(json!({"method":"item/completed","params":{"threadId":"thread-1","item":{"type":"agentMessage","id":"message-1","text":"Hello world!"}}}));
        assert_eq!(app.store.chats[0].messages.len(), 1);
        assert_eq!(app.store.chats[0].messages[0].text, "Hello world!");
    }
    #[test]
    fn only_code_can_queue_approvals() {
        for mode in [Mode::Ask, Mode::Enhance, Mode::Code, Mode::NoCode] {
            let mut app = app(mode);
            app.event(json!({"id":100,"method":"item/commandExecution/requestApproval","params":{"threadId":"thread-1","command":"example"}}));
            assert_eq!(app.approvals.len(), usize::from(mode == Mode::Code));
        }
    }
    #[test]
    fn foreign_thread_cannot_request_approval() {
        let mut app = app(Mode::Code);
        app.event(json!({"id":100,"method":"item/fileChange/requestApproval","params":{"threadId":"other"}}));
        assert!(app.approvals.is_empty());
    }
    #[test]
    fn stopping_before_thread_ready_prevents_turn_start() {
        let mut app = app(Mode::Code);
        app.active.as_mut().unwrap().turn = None;
        app.stop();
        app.pending.insert(42, "thread/start".into());
        app.event(json!({"id":42,"result":{"thread":{"id":"new-thread"}}}));
        assert!(app.active.is_none());
        assert!(!app.pending.values().any(|m| m == "turn/start"));
    }
    #[test]
    fn server_failure_releases_busy_state() {
        let mut app = app(Mode::Ask);
        app.pending.insert(42, "turn/start".into());
        app.event(json!({"id":42,"error":{"message":"Sandbox unavailable"}}));
        assert!(app.active.is_none());
        assert!(app.error.contains("Sandbox unavailable"));
    }
}
