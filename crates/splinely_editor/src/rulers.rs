//! Optional SVG document-unit rulers; no canvas state or input is mutated.
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke};

pub const LEFT_GUTTER: f32 = 64.0;
pub const TOP_GUTTER: f32 = 24.0;
pub const MAX_TICKS: usize = 512;
/// Accepted without clamping: using a different scale would misreport coordinates.
pub const MIN_ZOOM: f32 = 0.0001;
pub const MAX_ZOOM: f32 = 10000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RulerHit {
    Corner,
    Top,
    Left,
}

/// All rectangles are screen-space and contained in the supplied viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RulerGeometry {
    pub content: Rect,
    pub top: Rect,
    pub left: Rect,
    pub corner: Rect,
}

fn valid_viewport(viewport: Rect) -> bool {
    [
        viewport.min.x,
        viewport.min.y,
        viewport.max.x,
        viewport.max.y,
    ]
    .iter()
    .all(|v| v.is_finite())
        && viewport.max.x >= viewport.min.x
        && viewport.max.y >= viewport.min.y
}

pub fn geometry(viewport: Rect) -> RulerGeometry {
    if !valid_viewport(viewport) {
        let empty = Rect::from_min_max(Pos2::ZERO, Pos2::ZERO);
        return RulerGeometry {
            content: empty,
            top: empty,
            left: empty,
            corner: empty,
        };
    }
    let split = Pos2::new(
        (viewport.min.x as f64 + LEFT_GUTTER as f64).min(viewport.max.x as f64) as f32,
        (viewport.min.y as f64 + TOP_GUTTER as f64).min(viewport.max.y as f64) as f32,
    );
    RulerGeometry {
        content: Rect::from_min_max(split, viewport.max),
        top: Rect::from_min_max(
            Pos2::new(split.x, viewport.min.y),
            Pos2::new(viewport.max.x, split.y),
        ),
        left: Rect::from_min_max(
            Pos2::new(viewport.min.x, split.y),
            Pos2::new(split.x, viewport.max.y),
        ),
        corner: Rect::from_min_max(viewport.min, split),
    }
}

/// Reserve gutters only when rulers are enabled in gutter mode.
pub fn content_rect(viewport: Rect) -> Rect {
    geometry(viewport).content
}

/// Stateless hit test; caller routes/excludes ruler input when enabled.
pub fn ruler_hit_test(viewport: Rect, pos: Pos2) -> Option<RulerHit> {
    if !valid_viewport(viewport)
        || !pos.x.is_finite()
        || !pos.y.is_finite()
        || pos.x < viewport.min.x
        || pos.x >= viewport.max.x
        || pos.y < viewport.min.y
        || pos.y >= viewport.max.y
    {
        return None;
    }
    let g = geometry(viewport);
    match (pos.x < g.content.min.x, pos.y < g.content.min.y) {
        (true, true) => Some(RulerHit::Corner),
        (false, true) => Some(RulerHit::Top),
        (true, false) => Some(RulerHit::Left),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tick {
    pub screen: f32,
    pub document: f64,
    pub major: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RulerTicks {
    pub horizontal: Vec<Tick>,
    pub vertical: Vec<Tick>,
}

/// Smallest 1/2/5 decade interval at least `minimum`, or None for invalid input.
pub fn nice_interval(minimum: f64) -> Option<f64> {
    if !minimum.is_finite() || minimum <= 0.0 {
        return None;
    }
    let decade = 10.0_f64.powf(minimum.log10().floor());
    for multiplier in [1.0, 2.0, 5.0, 10.0] {
        let step = decade * multiplier;
        if step.is_finite() && step >= minimum {
            return Some(step);
        }
    }
    None
}

fn axis_ticks(min: f32, max: f32, origin: f32, zoom: f64) -> Vec<Tick> {
    let span = max as f64 - min as f64;
    if span <= 0.0 {
        return Vec::new();
    }
    // At most 50 major intervals per axis, with five minor intervals each.
    let Some(major) = nice_interval(60.0_f64.max(span / 50.0) / zoom) else {
        return Vec::new();
    };
    let minor = major / 5.0;
    let first = (((min as f64 - origin as f64) / zoom) / minor).ceil();
    let last = (((max as f64 - origin as f64) / zoom) / minor).floor();
    // Beyond exact integer indexing, tick increments can repeat or disappear.
    if !first.is_finite()
        || !last.is_finite()
        || first.abs().max(last.abs()) > (1_u64 << 52) as f64
        || last < first
    {
        return Vec::new();
    }
    let count = ((last - first + 1.0) as usize).min(MAX_TICKS / 2);
    let mut ticks = Vec::with_capacity(count);
    for offset in 0..count {
        let index = first + offset as f64;
        let document = index * minor;
        let screen = document * zoom + origin as f64;
        if screen.is_finite()
            && document.is_finite()
            && screen >= min as f64
            && screen <= max as f64
        {
            let screen = screen as f32;
            if ticks
                .last()
                .is_some_and(|tick: &Tick| tick.screen >= screen)
            {
                continue;
            }
            ticks.push(Tick {
                screen,
                document: if document == 0.0 { 0.0 } else { document },
                major: index % 5.0 == 0.0,
            });
        }
    }
    ticks
}

/// Pure geometry: screen = origin + document * zoom, including pan and negatives.
/// Invalid/non-finite inputs or scales outside the accepted range produce no ticks.
pub fn ticks(viewport: Rect, origin: Pos2, zoom: f32) -> RulerTicks {
    if !valid_viewport(viewport)
        || !origin.x.is_finite()
        || !origin.y.is_finite()
        || !zoom.is_finite()
        || !(MIN_ZOOM..=MAX_ZOOM).contains(&zoom)
    {
        return RulerTicks::default();
    }
    let g = geometry(viewport);
    RulerTicks {
        horizontal: axis_ticks(g.top.min.x, g.top.max.x, origin.x, zoom as f64),
        vertical: axis_ticks(g.left.min.y, g.left.max.y, origin.y, zoom as f64),
    }
}

fn label(value: f64) -> String {
    if value == 0.0 {
        "0".into()
    } else if !(0.01..1e6).contains(&value.abs()) {
        format!("{value:.1e}")
    } else {
        format!("{value:.4}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .into()
    }
}

/// Paint inside the viewport and existing painter clip. Returns geometry, not input.
/// `origin` is the actual screen position of SVG (0,0), never adjusted here.
pub fn paint(painter: &egui::Painter, viewport: Rect, origin: Pos2, zoom: f32) -> RulerGeometry {
    let g = geometry(viewport);
    if !valid_viewport(viewport) {
        return g;
    }
    let painter = painter.with_clip_rect(viewport.intersect(painter.clip_rect()));
    // Follow the light/dark theme.
    let dark = painter.ctx().style().visuals.dark_mode;
    let (background, foreground, minor) = if dark {
        (
            Color32::from_rgb(35, 38, 42),
            Color32::from_gray(190),
            Color32::from_gray(95),
        )
    } else {
        (
            Color32::from_rgb(232, 234, 237),
            Color32::from_gray(60),
            Color32::from_gray(160),
        )
    };
    for rect in [g.top, g.left, g.corner] {
        painter.rect_filled(rect, 0.0, background);
    }
    let ticks = ticks(viewport, origin, zoom);
    for (horizontal, rect, ticks) in [
        (true, g.top, ticks.horizontal),
        (false, g.left, ticks.vertical),
    ] {
        let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
        let mut previous_end = f32::NEG_INFINITY;
        for tick in ticks {
            let length = if tick.major { 8.0 } else { 4.0 };
            let (start, end, anchor) = if horizontal {
                (
                    Pos2::new(tick.screen, rect.max.y - length),
                    Pos2::new(tick.screen, rect.max.y),
                    Pos2::new(tick.screen, rect.min.y + 7.0),
                )
            } else {
                (
                    Pos2::new(rect.max.x - length, tick.screen),
                    Pos2::new(rect.max.x, tick.screen),
                    Pos2::new(rect.center().x - 3.0, tick.screen),
                )
            };
            painter.line_segment(
                [start, end],
                Stroke::new(1.0_f32, if tick.major { foreground } else { minor }),
            );
            if !tick.major {
                continue;
            }
            let galley =
                painter.layout_no_wrap(label(tick.document), FontId::monospace(10.0), foreground);
            let bounds = Align2::CENTER_CENTER.anchor_size(anchor, galley.size());
            let (begin, end) = if horizontal {
                (bounds.min.x, bounds.max.x)
            } else {
                (bounds.min.y, bounds.max.y)
            };
            if rect.contains_rect(bounds) && begin >= previous_end + 4.0 {
                painter.galley(bounds.min, galley, foreground);
                previous_end = end;
            }
        }
    }
    let corner_painter = painter.with_clip_rect(g.corner.intersect(painter.clip_rect()));
    corner_painter.text(
        g.corner.center(),
        Align2::CENTER_CENTER,
        "SVG u",
        FontId::monospace(10.0),
        foreground,
    );
    g
}

#[cfg(test)]
mod tests {
    use super::*;
    fn viewport() -> Rect {
        Rect::from_min_max(Pos2::ZERO, Pos2::new(664.0, 624.0))
    }

    #[test]
    fn nice_decades() {
        for (minimum, expected) in [
            (0.1, 0.1),
            (0.11, 0.2),
            (0.3, 0.5),
            (1.0, 1.0),
            (2.0, 2.0),
            (2.1, 5.0),
            (5.1, 10.0),
            (60.0, 100.0),
        ] {
            assert_eq!(nice_interval(minimum), Some(expected));
        }
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(nice_interval(bad), None);
        }
    }

    #[test]
    fn exact_pan_zoom_and_negative_zero() {
        let origin = Pos2::new(264.0, 224.0);
        let t = ticks(viewport(), origin, 2.0);
        assert!(t
            .horizontal
            .iter()
            .any(|t| t.document == -50.0 && t.screen == 164.0 && t.major));
        assert!(t
            .vertical
            .iter()
            .any(|t| t.document == -50.0 && t.screen == 124.0 && t.major));
        assert!(t
            .horizontal
            .iter()
            .any(|t| t.document == 0.0 && t.screen == origin.x && t.major));
        for tick in &t.horizontal {
            assert_eq!(tick.screen as f64, origin.x as f64 + tick.document * 2.0);
        }
        for tick in &t.vertical {
            assert_eq!(tick.screen as f64, origin.y as f64 + tick.document * 2.0);
        }
        let panned = ticks(viewport(), origin + egui::vec2(20.0, -20.0), 2.0);
        assert!(panned
            .horizontal
            .iter()
            .any(|t| t.document == -50.0 && t.screen == 184.0));
        let zero = ticks(
            Rect::from_min_max(Pos2::new(-400.0, -400.0), Pos2::new(400.0, 400.0)),
            Pos2::ZERO,
            1.0,
        );
        assert!(zero.horizontal.iter().any(|t| t.document < 0.0));
        assert!(zero
            .horizontal
            .iter()
            .any(|t| t.document == 0.0 && !t.document.is_sign_negative()));
    }

    #[test]
    fn bounded_and_finite() {
        for zoom in [MIN_ZOOM, 0.1, 1.0, 3.0, MAX_ZOOM] {
            for extent in [1.0, 1000.0, 1e10, f32::MAX] {
                let v = Rect::from_min_max(Pos2::ZERO, Pos2::new(extent, extent));
                let t = ticks(v, Pos2::new(-1e10, 0.0), zoom);
                assert!(t.horizontal.len() + t.vertical.len() <= MAX_TICKS);
                for axis in [&t.horizontal, &t.vertical] {
                    assert!(axis
                        .iter()
                        .all(|t| t.screen.is_finite() && t.document.is_finite()));
                    for pair in axis.windows(2) {
                        assert!(pair[0].screen < pair[1].screen);
                    }
                    let major: Vec<_> = axis.iter().filter(|t| t.major).collect();
                    for pair in major.windows(2) {
                        assert!(pair[1].screen as f64 - pair[0].screen as f64 >= 59.99);
                    }
                }
            }
        }
        for zoom in [
            0.0,
            -1.0,
            f32::NAN,
            f32::INFINITY,
            MIN_ZOOM / 2.0,
            MAX_ZOOM * 2.0,
        ] {
            assert_eq!(ticks(viewport(), Pos2::ZERO, zoom), RulerTicks::default());
        }
        assert_eq!(
            ticks(viewport(), Pos2::new(f32::MAX, f32::MAX), 1.0),
            RulerTicks::default()
        );
        assert_eq!(
            ticks(viewport(), Pos2::new(f32::NAN, 0.0), 1.0),
            RulerTicks::default()
        );
        assert_eq!(ticks(Rect::NOTHING, Pos2::ZERO, 1.0), RulerTicks::default());
    }

    #[test]
    fn gutters_and_hit_testing() {
        assert_eq!(content_rect(viewport()).min, Pos2::new(64.0, 24.0));
        assert_eq!(
            ruler_hit_test(viewport(), Pos2::new(10.0, 10.0)),
            Some(RulerHit::Corner)
        );
        assert_eq!(
            ruler_hit_test(viewport(), Pos2::new(100.0, 10.0)),
            Some(RulerHit::Top)
        );
        assert_eq!(
            ruler_hit_test(viewport(), Pos2::new(10.0, 100.0)),
            Some(RulerHit::Left)
        );
        for p in [
            Pos2::new(64.0, 24.0),
            Pos2::new(-1.0, 10.0),
            viewport().max,
            Pos2::new(f32::NAN, 0.0),
        ] {
            assert_eq!(ruler_hit_test(viewport(), p), None);
        }
        let tiny = Rect::from_min_max(Pos2::ZERO, Pos2::new(10.0, 10.0));
        assert_eq!(content_rect(tiny).min, tiny.max);
        assert!(ticks(tiny, Pos2::ZERO, 1.0).horizontal.is_empty());
    }
}
