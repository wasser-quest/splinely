//! Raster output: PNG of one moment and animated GIF of the autoplay clips.
//! Both render the same static SVG as an export (no scripts, local fonts).
use crate::*;

const MAX_SIDE: u32 = 8192;
const MAX_GIF_FRAMES: usize = 600;

fn svg_options() -> usvg::Options<'static> {
    let fonts = crate::system_fonts();
    usvg::Options {
        font_family: crate::default_font_family(&fonts),
        fontdb: fonts,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    }
}

impl Project {
    /// Rasterize the project at `time` to premultiplied RGBA of `width` pixels
    /// (height keeps the aspect ratio). Returns (width, height, pixels).
    pub fn render_rgba(&self, time: f64, width: u32) -> Result<(u32, u32, Vec<u8>), DocumentError> {
        if !(time.is_finite() && time >= 0.) {
            return Err(DocumentError(
                "render time must be finite and nonnegative".into(),
            ));
        }
        if !(1..=MAX_SIDE).contains(&width) {
            return Err(DocumentError(format!("width must be within 1..{MAX_SIDE}")));
        }
        let svg = self.to_static_svg(time)?;
        let tree = usvg::Tree::from_str(&svg, &svg_options())
            .map_err(|e| DocumentError(format!("render: {e}")))?;
        let size = tree.size();
        let scale = width as f32 / size.width();
        let height = ((size.height() * scale).ceil() as u32).clamp(1, MAX_SIDE);
        let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| DocumentError("cannot allocate image".into()))?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        Ok((width, height, pixmap.take()))
    }

    /// PNG bytes of the project at `time`.
    pub fn render_png(&self, time: f64, width: u32) -> Result<Vec<u8>, DocumentError> {
        let (w, h, data) = self.render_rgba(time, width)?;
        let pixmap = resvg::tiny_skia::Pixmap::from_vec(
            data,
            resvg::tiny_skia::IntSize::from_wh(w, h).unwrap(),
        )
        .ok_or_else(|| DocumentError("cannot build image".into()))?;
        pixmap
            .encode_png()
            .map_err(|e| DocumentError(format!("png: {e}")))
    }

    /// Longest autoplay clip (what an export plays on its own), 0 if none.
    pub fn animation_duration(&self) -> f64 {
        self.clips
            .iter()
            .filter(|c| c.autoplay)
            .map(|c| c.duration)
            .fold(0., f64::max)
    }

    /// Looping animated GIF of `duration` seconds at `fps` frames per second,
    /// composited on `background` (GIF has only 1-bit transparency).
    pub fn render_gif(
        &self,
        duration: f64,
        fps: f64,
        width: u32,
        background: Color,
    ) -> Result<Vec<u8>, DocumentError> {
        if !(fps.is_finite() && (1. ..=50.).contains(&fps)) {
            return Err(DocumentError("fps must be within 1..50".into()));
        }
        if !(duration.is_finite() && duration >= 0.) {
            return Err(DocumentError(
                "duration must be finite and nonnegative".into(),
            ));
        }
        let frames = ((duration * fps).round() as usize).max(1);
        if frames > MAX_GIF_FRAMES {
            return Err(DocumentError(format!(
                "at most {MAX_GIF_FRAMES} GIF frames (lower fps or duration)"
            )));
        }
        let (w, h, _) = self.render_rgba(0., width)?;
        if w > u16::MAX as u32 || h > u16::MAX as u32 {
            return Err(DocumentError(
                "GIF is limited to 65535 pixels per side".into(),
            ));
        }
        let mut out = vec![];
        {
            let mut encoder = gif::Encoder::new(&mut out, w as u16, h as u16, &[])
                .map_err(|e| DocumentError(format!("gif: {e}")))?;
            encoder
                .set_repeat(gif::Repeat::Infinite)
                .map_err(|e| DocumentError(format!("gif: {e}")))?;
            let bg =
                [background.r, background.g, background.b].map(|v| (v.clamp(0., 1.) * 255.) as f32);
            // GIF delays are in 1/100 s; distribute rounding over the frames.
            let mut elapsed_cs = 0u32;
            for i in 0..frames {
                let time = i as f64 / fps;
                let (_, _, data) = self.render_rgba(time, width)?;
                // Premultiplied RGBA over an opaque background.
                let mut rgba: Vec<u8> = data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|p| {
                        let a = 1. - p[3] as f32 / 255.;
                        [
                            (p[0] as f32 + bg[0] * a).round().min(255.) as u8,
                            (p[1] as f32 + bg[1] * a).round().min(255.) as u8,
                            (p[2] as f32 + bg[2] * a).round().min(255.) as u8,
                            255,
                        ]
                    })
                    .collect();
                let mut frame = gif::Frame::from_rgba_speed(w as u16, h as u16, &mut rgba, 10);
                let end_cs = (((i + 1) as f64 / fps) * 100.).round() as u32;
                frame.delay = (end_cs - elapsed_cs).max(2) as u16;
                elapsed_cs = end_cs;
                encoder
                    .write_frame(&frame)
                    .map_err(|e| DocumentError(format!("gif: {e}")))?;
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_and_gif_outputs() {
        let mut p = Project::default();
        p.nodes.push(Node::rect("r", 0., 0., 100., 100.));
        p.clips.push(Clip {
            id: "move".into(),
            name: String::new(),
            duration: 1.,
            looping: true,
            autoplay: true,
            blend_mode: BlendMode::Override,
            weight: 1.,
            tracks: vec![Track {
                node_id: "r".into(),
                property: Property::X,
                keyframes: vec![
                    Keyframe {
                        time: 0.,
                        value: AnimValue::Number(0.),
                        easing: Easing::Linear,
                    },
                    Keyframe {
                        time: 1.,
                        value: AnimValue::Number(500.),
                        easing: Easing::Linear,
                    },
                ],
            }],
        });
        let png = p.render_png(0.5, 80).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        let (w, h, _) = p.render_rgba(0., 80).unwrap();
        assert_eq!((w, h), (80, 60));
        assert_eq!(p.animation_duration(), 1.);
        let gif = p
            .render_gif(1., 10., 40, Color::rgb(255, 255, 255))
            .unwrap();
        assert!(gif.starts_with(b"GIF89a"));
        let decoder = gif::DecodeOptions::new().read_info(&gif[..]).unwrap();
        let frames = decoder.into_iter().count();
        assert_eq!(frames, 10);
        assert!(
            p.render_gif(100., 50., 40, Color::rgb(0, 0, 0)).is_err(),
            "frame limit"
        );
        assert!(p.render_png(0., 0).is_err());
    }
}
