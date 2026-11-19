use eframe::egui::{self, Color32, RichText};

pub const TEXT: Color32 = Color32::from_rgb(224, 204, 172);
pub const MUTED: Color32 = Color32::from_rgb(163, 145, 121);
pub const BORDER: Color32 = Color32::from_rgb(64, 54, 43);

/// Keep normal text fonts, with Windows emoji/symbol fonts as glyph fallbacks.
pub fn install_emoji_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let windows =
        std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:/Windows".into()));
    for (name, file) in [
        ("windows-emoji", "seguiemj.ttf"),
        ("windows-symbols", "seguisym.ttf"),
    ] {
        if let Ok(bytes) = std::fs::read(windows.join("Fonts").join(file)) {
            fonts
                .font_data
                .insert(name.into(), egui::FontData::from_owned(bytes).into());
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push(name.into());
            }
        }
    }
    ctx.set_fonts(fonts);
}

#[derive(Clone, Copy)]
pub enum ButtonStyle {
    Primary,
    Secondary,
    Quiet,
    Stop,
}

fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    Color32::from_rgb(
        egui::lerp(a.r() as f32..=b.r() as f32, t) as u8,
        egui::lerp(a.g() as f32..=b.g() as f32, t) as u8,
        egui::lerp(a.b() as f32..=b.b() as f32, t) as u8,
    )
}

pub fn button(ui: &mut egui::Ui, label: &str, style: ButtonStyle, enabled: bool) -> egui::Response {
    let id = ui.make_persistent_id(("soft-button", label));
    let hovered = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(id).unwrap_or(false));
    let amount = ui
        .ctx()
        .animate_bool_with_time(id.with("hover"), hovered && enabled, 0.14);
    let (base, hover, text) = match style {
        ButtonStyle::Primary => (
            Color32::from_rgb(176, 136, 89),
            Color32::from_rgb(201, 159, 109),
            Color32::from_rgb(29, 24, 18),
        ),
        ButtonStyle::Secondary => (
            Color32::from_rgb(64, 52, 40),
            Color32::from_rgb(88, 70, 51),
            TEXT,
        ),
        ButtonStyle::Quiet => (
            Color32::from_rgb(35, 30, 25),
            Color32::from_rgb(59, 48, 37),
            MUTED,
        ),
        ButtonStyle::Stop => (
            Color32::from_rgb(89, 49, 37),
            Color32::from_rgb(118, 62, 45),
            Color32::from_rgb(247, 209, 180),
        ),
    };
    let response = ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).color(text))
            .fill(blend(base, hover, amount))
            .stroke(egui::Stroke::new(1.0_f32, BORDER))
            .corner_radius(8)
            .min_size(egui::vec2(0., 38.)),
    );
    // Read focus before locking context data: has_focus() also accesses context memory.
    let highlighted = response.hovered() || response.has_focus();
    ui.ctx().data_mut(|data| data.insert_temp(id, highlighted));
    response
}

/// Shared readable measure for the transcript and composer, including wide windows.
pub fn reading_column(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    let width = ui.available_width().min(850.);
    let inset = ((ui.available_width() - width) / 2.).max(0.);
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
        ui.spacing_mut().item_spacing.x = 0.;
        ui.add_space(inset);
        ui.vertical(|ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.x = 10.;
            contents(ui);
        });
    });
}

pub fn folder_name(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("No project selected")
}

pub fn chat_row(ui: &mut egui::Ui, title: &str, folder: &str, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 68.), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::SelectableLabel, ui.is_enabled(), title)
    });
    let amount = ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered() || response.has_focus(),
        0.14,
    );
    let base = if selected {
        Color32::from_rgb(60, 47, 34)
    } else {
        ui.visuals().panel_fill
    };
    ui.painter()
        .rect_filled(rect, 8, blend(base, Color32::from_rgb(70, 55, 39), amount));
    if selected {
        let accent = egui::Rect::from_min_size(rect.min + egui::vec2(0., 15.), egui::vec2(3., 38.));
        ui.painter()
            .rect_filled(accent, 2, Color32::from_rgb(176, 136, 89));
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            8,
            egui::Stroke::new(1.0_f32, TEXT),
            egui::StrokeKind::Inside,
        );
    }
    for (text, size, color, y) in [
        (title, 14., TEXT, 13.),
        (folder_name(folder), 12., MUTED, 38.),
    ] {
        let mut job = egui::text::LayoutJob::simple(
            text.into(),
            egui::FontId::proportional(size),
            color,
            rect.width() - 28.,
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('…');
        let galley = ui.fonts(|fonts| fonts.layout_job(job));
        ui.painter()
            .galley(rect.min + egui::vec2(14., y), galley, color);
    }
    response.on_hover_text(format!("{title}\n{folder}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

    #[test]
    fn animated_button_handles_keyboard_focus_and_clicks() {
        let context = egui::Context::default();
        let mut bounds = egui::Rect::NOTHING;
        let mut clicked = false;
        let mut render = |events| {
            let _ = context.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let response = button(ui, "Code", ButtonStyle::Primary, true);
                        bounds = response.rect;
                        response.request_focus();
                        clicked = response.clicked();
                    });
                },
            );
            (bounds, clicked)
        };
        let (bounds, _) = render(vec![]);
        render(vec![]);
        let pos = bounds.center();
        render(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        let (_, clicked) = render(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(clicked);
    }

    #[test]
    fn markdown_code_copy_targets_the_selected_block() {
        let context = egui::Context::default();
        let mut cache = CommonMarkCache::default();
        let markdown = "# A heading\n\nA **bold** answer.\n\n```rust\nlet first = 1;\n```\n\n```text\nsecond block\n```";
        let mut render = |events| {
            context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 700.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        reading_column(ui, |ui| {
                            assert!(ui.available_width() <= 850.);
                            CommonMarkViewer::new().show(ui, &mut cache, markdown);
                        });
                    });
                },
            )
        };
        render(vec![]);
        let output = render(vec![]);
        let copy_icons: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape {
                    (text.galley.job.text == "🗐")
                        .then_some(text.pos + text.galley.rect.center().to_vec2())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            copy_icons.len(),
            2,
            "Each fenced code block needs its own copy control"
        );
        let pos = copy_icons[1];
        render(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        let clicked = render(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(clicked.platform_output.commands.iter().any(|command| {
            matches!(command, egui::OutputCommand::CopyText(text) if text == "second block")
        }));
    }
}
