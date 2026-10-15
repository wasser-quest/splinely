//! Cached native SVG raster preview. No file/network image resolvers are enabled.
use eframe::egui;
use splinely_editor::ecs::{SceneNode, Shape};
#[derive(Default)]
pub struct Raster {
    key: String,
    font_family: String,
    texture: Option<egui::TextureHandle>,
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}
impl Raster {
    /// Rasterize only the part of `rect` (the whole document on screen) that is
    /// visible inside the painter's clip rect, at native pixel density. This keeps
    /// deep zoom sharp and avoids paying for off-screen pixels.
    pub fn paint(
        &mut self,
        painter: &egui::Painter,
        rect: egui::Rect,
        width: f64,
        height: f64,
        nodes: &[SceneNode],
    ) -> Result<(), String> {
        self.paint_tinted(painter, rect, width, height, nodes, egui::Color32::WHITE)
    }

    /// Like `paint`, multiplying the image by `tint` (onion-skin ghosts).
    pub fn paint_tinted(
        &mut self,
        painter: &egui::Painter,
        rect: egui::Rect,
        width: f64,
        height: f64,
        nodes: &[SceneNode],
        tint: egui::Color32,
    ) -> Result<(), String> {
        let visible = rect.intersect(painter.clip_rect());
        if !visible.is_positive() || width <= 0. || height <= 0. {
            return Ok(());
        }
        let mut svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">"#
        );
        for (index, n) in nodes.iter().enumerate() {
            let fill = match &n.gradient {
                Some(gradient) => {
                    let id = format!("splinely-preview-gradient-{index}");
                    svg.push_str(&format!("<defs>{}</defs>", gradient.svg_element(&id)));
                    format!("url(#{id})")
                }
                None => escape(&n.fill),
            };
            svg.push_str(&format!(r#"<g transform="translate({} {}) rotate({}) scale({} {})" fill="{}" stroke="{}" stroke-width="{}" opacity="{}">"#,n.x,n.y,n.rotation,n.scale_x,n.scale_y,fill,escape(&n.stroke),n.stroke_width,n.opacity));
            svg.push_str(&match &n.shape {
                Shape::Rect { width, height, rx } => {
                    format!(r#"<rect width="{width}" height="{height}" rx="{rx}"/>"#)
                }
                Shape::Circle { radius } => format!(r#"<circle r="{radius}"/>"#),
                Shape::Path { d, evenodd, .. } => format!(
                    r#"<path d="{}" fill-rule="{}"/>"#,
                    escape(d),
                    if *evenodd { "evenodd" } else { "nonzero" }
                ),
                Shape::Text { text, size } => {
                    format!(r#"<text font-size="{size}">{}</text>"#, escape(text))
                }
            });
            svg.push_str("</g>");
        }
        svg.push_str("</svg>");
        let ppp = painter.ctx().pixels_per_point();
        let max_side = painter.ctx().input(|i| i.max_texture_side).clamp(1, 4096) as f32;
        let w = (visible.width() * ppp).round().clamp(1., max_side) as u32;
        let h = (visible.height() * ppp).round().clamp(1., max_side) as u32;
        // Document units -> pixmap pixels, shifted so the visible corner is (0, 0).
        let sx = w as f32 / visible.width() * rect.width() / width as f32;
        let sy = h as f32 / visible.height() * rect.height() / height as f32;
        let tx = (rect.min.x - visible.min.x) * w as f32 / visible.width();
        let ty = (rect.min.y - visible.min.y) * h as f32 / visible.height();
        let key = format!("{w}:{h}:{sx}:{sy}:{tx}:{ty}:{svg}");
        if key != self.key {
            // Shared, process-wide font database and serif fallback.
            let fonts = splinely_document::system_fonts();
            if self.font_family.is_empty() {
                self.font_family = splinely_document::default_font_family(&fonts);
            }
            let options = resvg::usvg::Options {
                font_family: self.font_family.clone(),
                fontdb: fonts,
                image_href_resolver: resvg::usvg::ImageHrefResolver {
                    resolve_data: Box::new(|_, _, _| None),
                    resolve_string: Box::new(|_, _| None),
                },
                ..Default::default()
            };
            let tree = resvg::usvg::Tree::from_str(&svg, &options).map_err(|e| e.to_string())?;
            let mut pixmap =
                resvg::tiny_skia::Pixmap::new(w, h).ok_or("Preview allocation failed")?;
            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::from_row(sx, 0., 0., sy, tx, ty),
                &mut pixmap.as_mut(),
            );
            self.texture = Some(painter.ctx().load_texture(
                "splinely-preview",
                egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], pixmap.data()),
                egui::TextureOptions::LINEAR,
            ));
            self.key = key;
        }
        if let Some(texture) = &self.texture {
            painter.image(
                texture.id(),
                visible,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                tint,
            );
        }
        Ok(())
    }
}
