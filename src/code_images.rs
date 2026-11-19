use base64::{engine::general_purpose::STANDARD, Engine};
use eframe::egui;
use serde_json::Value;
use std::{collections::HashMap, io::Cursor, path::Path};

// Snapshot local files so later edits or temporary-file cleanup do not change history.
pub fn viewed_images(item: &Value, folder: &str) -> Vec<String> {
    let mut images = Vec::new();
    if item["type"] == "imageView" {
        if let Some(path) = item["path"].as_str() {
            let path = Path::new(path);
            let path = if path.is_absolute() {
                path.to_owned()
            } else {
                Path::new(folder).join(path)
            };
            if let Ok(bytes) = std::fs::read(&path) {
                images.push(format!(
                    "data:image/unknown;base64,{}",
                    STANDARD.encode(bytes)
                ));
            } else {
                // Retain the path and show an explicit unavailable state.
                images.push(path.to_string_lossy().into_owned());
            }
        }
    }
    if matches!(
        item["type"].as_str(),
        Some("mcpToolCall" | "dynamicToolCall")
    ) {
        collect_content(&item["result"], &mut images);
        collect_content(&item["contentItems"], &mut images);
    }
    images
}

fn collect_content(value: &Value, images: &mut Vec<String>) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_content(value, images);
            }
        }
        Value::Object(values) => {
            if value["type"] == "image" || value["type"] == "inputImage" {
                let source = value["data"]
                    .as_str()
                    .map(|data| {
                        format!(
                            "data:{};base64,{data}",
                            value["mimeType"].as_str().unwrap_or("image/png")
                        )
                    })
                    .or_else(|| {
                        value["image_url"]
                            .as_str()
                            .filter(|url| url.starts_with("data:"))
                            .map(str::to_owned)
                    })
                    .or_else(|| {
                        value["imageUrl"]
                            .as_str()
                            .filter(|url| url.starts_with("data:"))
                            .map(str::to_owned)
                    });
                if let Some(source) = source {
                    if !images.contains(&source) {
                        images.push(source);
                    }
                }
            } else {
                for (key, value) in values {
                    if key == "content" || key == "contentItems" {
                        collect_content(value, images);
                    }
                }
            }
        }
        _ => {}
    }
}

#[derive(Default)]
pub struct Previews {
    textures: HashMap<String, Result<egui::TextureHandle, String>>,
    selected: Option<String>,
}
impl Previews {
    fn texture(
        &mut self,
        ctx: &egui::Context,
        source: &str,
    ) -> Result<egui::TextureHandle, String> {
        self.textures
            .entry(source.to_owned())
            .or_insert_with(|| {
                let bytes = if source.starts_with("data:") {
                    STANDARD
                        .decode(source.split_once(',').ok_or("Invalid image data")?.1)
                        .map_err(|e| e.to_string())?
                } else {
                    std::fs::read(source).map_err(|e| e.to_string())?
                };
                let mut reader = image::ImageReader::new(Cursor::new(bytes))
                    .with_guessed_format()
                    .map_err(|e| e.to_string())?;
                let mut limits = image::Limits::default();
                limits.max_image_width = Some(16384);
                limits.max_image_height = Some(16384);
                limits.max_alloc = Some(256 * 1024 * 1024);
                reader.limits(limits);
                let picture = reader.decode().map_err(|e| e.to_string())?.into_rgba8();
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    [picture.width() as usize, picture.height() as usize],
                    picture.as_raw(),
                );
                Ok(ctx.load_texture("viewed-image", color, egui::TextureOptions::LINEAR))
            })
            .clone()
    }
    pub fn inline(&mut self, ui: &mut egui::Ui, source: &str) {
        match self.texture(ui.ctx(), source) {
            Ok(texture) => {
                if ui
                    .add(
                        egui::Image::new(&texture)
                            .max_width(ui.available_width().min(560.))
                            .max_height(360.)
                            .sense(egui::Sense::click()),
                    )
                    .on_hover_text("Click to enlarge")
                    .clicked()
                {
                    self.selected = Some(source.to_owned());
                }
            }
            Err(error) => {
                ui.label(format!("Image unavailable: {error}"));
            }
        }
    }
    pub fn window(&mut self, ctx: &egui::Context) {
        let Some(source) = self.selected.clone() else {
            return;
        };
        let mut open = true;
        egui::Window::new("Viewed image")
            .open(&mut open)
            .resizable(true)
            .default_size([900., 650.])
            .show(ctx, |ui| {
                if let Ok(texture) = self.texture(ctx, &source) {
                    ui.add(
                        egui::Image::new(&texture)
                            .max_width(ui.available_width())
                            .max_height(ui.available_height()),
                    );
                }
            });
        if !open {
            self.selected = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn snapshots_local_views_and_collects_tool_images() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("crop.png"), b"image bytes").unwrap();
        let images = viewed_images(
            &json!({"type":"imageView","path":"crop.png"}),
            dir.path().to_str().unwrap(),
        );
        assert_eq!(
            images,
            vec![format!(
                "data:image/unknown;base64,{}",
                STANDARD.encode(b"image bytes")
            )]
        );
        let images = viewed_images(
            &json!({"type":"mcpToolCall","result":{"content":[{"type":"image","data":"abc","mimeType":"image/png"},{"type":"text","text":"ignore"}]}}),
            "",
        );
        assert_eq!(images, vec!["data:image/png;base64,abc"]);
        let images = viewed_images(
            &json!({"type":"dynamicToolCall","contentItems":[{"type":"inputImage","imageUrl":"data:image/png;base64,abc"}]}),
            "",
        );
        assert_eq!(images, vec!["data:image/png;base64,abc"]);
    }
    #[test]
    fn previews_decode_snapshots_and_show_unavailable_for_invalid_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crop.png");
        image::RgbImage::from_pixel(4, 3, image::Rgb([10, 20, 30]))
            .save(&path)
            .unwrap();
        let images = viewed_images(&json!({"type":"imageView","path":path}), "");
        std::fs::remove_file(path).unwrap();
        let ctx = egui::Context::default();
        let mut previews = Previews::default();
        assert_eq!(previews.texture(&ctx, &images[0]).unwrap().size(), [4, 3]);
        assert!(previews
            .texture(&ctx, "data:image/png;base64,invalid")
            .is_err());
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| previews.inline(ui, &images[0]));
            previews.selected = Some(images[0].clone());
            previews.window(ctx);
        });
    }
}
