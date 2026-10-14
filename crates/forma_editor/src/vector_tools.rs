//! Canvas path tools. All points are local SVG coordinates; callers own history.
//! Parsing uses usvg's svgtypes-backed geometry parser after document validation.
//! H/V/S/T become explicit segments, arcs become cubics, coordinates use the
//! renderer's f32 precision. The original text is retained until an actual edit.
use eframe::egui::{self, Color32, Pos2, Stroke};
use forma_document::{Node, NodeKind, Project};
use resvg::{tiny_skia, usvg};
use std::fmt::Write;

pub type Point = [f64; 2];

#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    Move(Point),
    Line(Point),
    Quadratic {
        control: Point,
        end: Point,
    },
    Cubic {
        first: Point,
        second: Point,
        end: Point,
    },
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleKind {
    Anchor,
    FirstControl,
    SecondControl,
}

/// Gesture-only tangent coupling; no automatic modes are persisted in SVG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TangentLink {
    Independent,
    Smooth,
    Symmetric,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handle {
    pub segment: usize,
    pub kind: HandleKind,
}

#[derive(Clone, Debug)]
pub struct EditablePath {
    segments: Vec<Segment>,
    source: Option<String>,
    changed: bool,
}

fn valid_point(p: Point) -> Result<(), String> {
    if p.iter().any(|v| !v.is_finite() || v.abs() > 1e7) {
        Err("Path coordinates must be finite and within +/- 10000000".into())
    } else {
        Ok(())
    }
}
fn point(p: tiny_skia::Point) -> Point {
    [p.x as f64, p.y as f64]
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn bezier_coefficients(points: &[Point]) -> Vec<Point> {
    let mut row = points.to_vec();
    let degree = row.len() - 1;
    let mut out = vec![row[0]];
    let mut binomial = 1.;
    for i in 1..=degree {
        for j in 0..row.len() - 1 {
            row[j] = [row[j + 1][0] - row[j][0], row[j + 1][1] - row[j][1]];
        }
        row.pop();
        binomial *= (degree + 1 - i) as f64 / i as f64;
        out.push([row[0][0] * binomial, row[0][1] * binomial]);
    }
    out
}

// Isolate all stationary points by recursively partitioning at derivative
// roots. Unlike coarse sampling this also finds narrow loops and repeated roots.
fn polynomial_roots(coefficients: &[f64]) -> Vec<f64> {
    let scale = coefficients.iter().fold(0_f64, |m, c| m.max(c.abs()));
    if scale == 0. {
        return vec![];
    }
    let mut c: Vec<f64> = coefficients.iter().map(|v| v / scale).collect();
    while c.len() > 1 && c.last() == Some(&0.) {
        c.pop();
    }
    if c.len() <= 1 {
        return vec![];
    }
    let derivative: Vec<f64> = c
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, v)| i as f64 * v)
        .collect();
    let mut partitions = polynomial_roots(&derivative);
    partitions.extend([0., 1.]);
    partitions.sort_by(f64::total_cmp);
    let eval = |t: f64| c.iter().rev().fold(0., |v, c| v * t + c);
    let mut roots: Vec<f64> = partitions
        .iter()
        .copied()
        .filter(|t| eval(*t).abs() < 1e-12)
        .collect();
    for pair in partitions.windows(2) {
        let (mut lo, mut hi) = (pair[0], pair[1]);
        let mut left = eval(lo);
        if left * eval(hi) >= 0. {
            continue;
        }
        for _ in 0..60 {
            let mid = (lo + hi) * 0.5;
            let value = eval(mid);
            if left * value <= 0. {
                hi = mid;
            } else {
                lo = mid;
                left = value;
            }
        }
        roots.push((lo + hi) * 0.5);
    }
    roots.sort_by(f64::total_cmp);
    roots.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
    roots
}

impl EditablePath {
    pub fn parse(d: &str) -> Result<Self, String> {
        // Reject malformed/oversized input before passing it to the SVG renderer.
        forma_document::path_bounds(d).map_err(|e| e.to_string())?;
        // Escape the attribute even though valid path grammar cannot contain XML.
        let escaped = d
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;");
        let svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"><path d=\"{escaped}\"/></svg>");
        let tree =
            usvg::Tree::from_str(&svg, &usvg::Options::default()).map_err(|e| e.to_string())?;
        let path = match tree.root().children() {
            [usvg::Node::Path(path)] => path,
            _ => return Err("SVG parser did not produce a single editable path".into()),
        };
        use tiny_skia::PathSegment::*;
        let segments = path
            .data()
            .segments()
            .map(|s| match s {
                MoveTo(p) => Segment::Move(point(p)),
                LineTo(p) => Segment::Line(point(p)),
                QuadTo(c, p) => Segment::Quadratic {
                    control: point(c),
                    end: point(p),
                },
                CubicTo(a, b, p) => Segment::Cubic {
                    first: point(a),
                    second: point(b),
                    end: point(p),
                },
                Close => Segment::Close,
            })
            .collect();
        Ok(Self {
            segments,
            source: Some(d.into()),
            changed: false,
        })
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    pub fn handles(&self) -> Vec<(Handle, Point)> {
        let mut out = Vec::new();
        for (segment, s) in self.segments.iter().enumerate() {
            let mut add = |kind, p| out.push((Handle { segment, kind }, p));
            match *s {
                Segment::Move(p) | Segment::Line(p) => add(HandleKind::Anchor, p),
                Segment::Quadratic { control, end } => {
                    add(HandleKind::FirstControl, control);
                    add(HandleKind::Anchor, end);
                }
                Segment::Cubic { first, second, end } => {
                    add(HandleKind::FirstControl, first);
                    add(HandleKind::SecondControl, second);
                    add(HandleKind::Anchor, end);
                }
                Segment::Close => {}
            }
        }
        out
    }

    /// Move exactly one point; independent controls deliberately do not auto-smooth.
    pub fn move_handle(&mut self, handle: Handle, p: Point) -> Result<(), String> {
        valid_point(p)?;
        let slot = match (self.segments.get_mut(handle.segment), handle.kind) {
            (Some(Segment::Move(p) | Segment::Line(p)), HandleKind::Anchor) => p,
            (
                Some(Segment::Quadratic { end, .. } | Segment::Cubic { end, .. }),
                HandleKind::Anchor,
            ) => end,
            (Some(Segment::Quadratic { control, .. }), HandleKind::FirstControl) => control,
            (Some(Segment::Cubic { first, .. }), HandleKind::FirstControl) => first,
            (Some(Segment::Cubic { second, .. }), HandleKind::SecondControl) => second,
            _ => return Err("Path handle no longer exists".into()),
        };
        if *slot != p {
            *slot = p;
            self.changed = true;
        }
        Ok(())
    }

    /// Return a validated split without mutating this path. `t` is interior.
    pub fn split_segment(&self, index: usize, t: f64) -> Result<Self, String> {
        if !t.is_finite() || t <= 0. || t >= 1. {
            return Err("Split parameter must be inside (0, 1)".into());
        }
        let (start, home) = self.segment_start(index)?;
        let pieces = match self.segments[index] {
            Segment::Line(end) => vec![Segment::Line(lerp(start, end, t)), Segment::Line(end)],
            Segment::Close => vec![Segment::Line(lerp(start, home, t)), Segment::Close],
            Segment::Quadratic { control, end } => {
                let a = lerp(start, control, t);
                let b = lerp(control, end, t);
                let mid = lerp(a, b, t);
                vec![
                    Segment::Quadratic {
                        control: a,
                        end: mid,
                    },
                    Segment::Quadratic { control: b, end },
                ]
            }
            Segment::Cubic { first, second, end } => {
                let a = lerp(start, first, t);
                let b = lerp(first, second, t);
                let c = lerp(second, end, t);
                let d = lerp(a, b, t);
                let e = lerp(b, c, t);
                let mid = lerp(d, e, t);
                vec![
                    Segment::Cubic {
                        first: a,
                        second: d,
                        end: mid,
                    },
                    Segment::Cubic {
                        first: e,
                        second: c,
                        end,
                    },
                ]
            }
            Segment::Move(_) => return Err("Cannot split a move command".into()),
        };
        let mut next = self.clone();
        next.segments.splice(index..=index, pieces);
        next.changed = true;
        forma_document::path_bounds(&next.path_data()).map_err(|e| e.to_string())?;
        Ok(next)
    }

    fn segment_start(&self, index: usize) -> Result<(Point, Point), String> {
        if index >= self.segments.len() {
            return Err("Path segment no longer exists".into());
        }
        let mut current = None;
        let mut home = None;
        for s in &self.segments[..index] {
            match *s {
                Segment::Move(p) => {
                    current = Some(p);
                    home = Some(p);
                }
                Segment::Line(p)
                | Segment::Quadratic { end: p, .. }
                | Segment::Cubic { end: p, .. } => current = Some(p),
                Segment::Close => current = home,
            }
        }
        Ok((
            current.ok_or("Missing segment start")?,
            home.ok_or("Missing subpath start")?,
        ))
    }

    /// Nearest point in screen pixels, including evaluated affine transforms.
    /// Misses and existing endpoints return None; the source remains untouched.
    pub fn insert_nearest(
        &self,
        screen: Pos2,
        map: &CanvasMap,
        max_distance: f32,
    ) -> Result<Option<Self>, String> {
        map.validate()?;
        if !screen.x.is_finite()
            || !screen.y.is_finite()
            || !max_distance.is_finite()
            || max_distance < 0.
        {
            return Err("Invalid insertion hit radius or screen point".into());
        }
        let mut best: Option<(f64, usize, f64)> = None;
        let mut current = [0., 0.];
        let mut home = current;
        for (index, segment) in self.segments.iter().enumerate() {
            let points = match *segment {
                Segment::Move(p) => {
                    current = p;
                    home = p;
                    continue;
                }
                Segment::Line(end) => vec![current, end],
                Segment::Close => vec![current, home],
                Segment::Quadratic { control, end } => vec![current, control, end],
                Segment::Cubic { first, second, end } => vec![current, first, second, end],
            };
            current = *points.last().unwrap();
            let [a, b, c, d, x, y] = map.affine;
            let points: Vec<Point> = points
                .into_iter()
                .map(|p| {
                    map.to_screen(p)?;
                    Ok([
                        map.origin.x as f64 + map.zoom * (a * p[0] + c * p[1] + x)
                            - screen.x as f64,
                        map.origin.y as f64 + map.zoom * (b * p[0] + d * p[1] + y)
                            - screen.y as f64,
                    ])
                })
                .collect::<Result<_, String>>()?;
            let coefficients = bezier_coefficients(&points);
            let mut derivative = vec![0.; 2 * coefficients.len() - 2];
            for (i, p) in coefficients.iter().enumerate() {
                for (j, q) in coefficients.iter().enumerate().skip(1) {
                    derivative[i + j - 1] += j as f64 * (p[0] * q[0] + p[1] * q[1]);
                }
            }
            let mut candidates = polynomial_roots(&derivative);
            candidates.extend([0., 1.]);
            for t in candidates {
                let p = coefficients
                    .iter()
                    .rev()
                    .fold([0., 0.], |v, c| [v[0] * t + c[0], v[1] * t + c[1]]);
                let distance = p[0] * p[0] + p[1] * p[1];
                if best.is_none_or(|old| distance < old.0) {
                    best = Some((distance, index, t));
                }
            }
        }
        match best {
            Some((distance, index, t))
                if distance <= (max_distance as f64).powi(2) && t > 0. && t < 1. =>
            {
                self.split_segment(index, t).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// Link only an existing opposite control at the same anchor. Lines, cusps
    /// with missing controls, and open endpoints never gain synthetic handles.
    pub fn move_tangent(
        &mut self,
        handle: Handle,
        p: Point,
        link: TangentLink,
    ) -> Result<(), String> {
        let mut next = self.clone();
        next.move_handle(handle, p)?;
        if handle.kind != HandleKind::Anchor && link != TangentLink::Independent {
            let (start, _) = self.segment_start(handle.segment)?;
            let end = match self.segments[handle.segment] {
                Segment::Quadratic { end, .. } | Segment::Cubic { end, .. } => end,
                _ => return Err("Not a tangent handle".into()),
            };
            // Quadratics share one control between both endpoint tangents.
            let sides: &[bool] =
                if matches!(self.segments[handle.segment], Segment::Quadratic { .. }) {
                    &[false, true]
                } else if handle.kind == HandleKind::SecondControl {
                    &[true]
                } else {
                    &[false]
                };
            for &incoming in sides {
                let anchor = if incoming { end } else { start };
                let opposite = if incoming {
                    self.segments.get(handle.segment + 1).and_then(|s| match s {
                        Segment::Quadratic { .. } | Segment::Cubic { .. } => Some(Handle {
                            segment: handle.segment + 1,
                            kind: HandleKind::FirstControl,
                        }),
                        _ => None,
                    })
                } else {
                    handle
                        .segment
                        .checked_sub(1)
                        .and_then(|i| match self.segments[i] {
                            Segment::Cubic { .. } => Some(Handle {
                                segment: i,
                                kind: HandleKind::SecondControl,
                            }),
                            Segment::Quadratic { .. } => Some(Handle {
                                segment: i,
                                kind: HandleKind::FirstControl,
                            }),
                            _ => None,
                        })
                };
                // A zero-length closing edge joins the final incoming control to
                // the first outgoing control. Nonzero Z edges are lines, not tangents.
                let opposite = opposite
                    .or_else(|| {
                        let begin = (0..=handle.segment)
                            .rev()
                            .find(|&i| matches!(self.segments[i], Segment::Move(_)))?;
                        let close = (begin + 1..self.segments.len())
                            .take_while(|&i| !matches!(self.segments[i], Segment::Move(_)))
                            .find(|&i| matches!(self.segments[i], Segment::Close))?;
                        let Segment::Move(home) = self.segments[begin] else {
                            return None;
                        };
                        let last = close.checked_sub(1)?;
                        let last_end = match self.segments[last] {
                            Segment::Cubic { end, .. } | Segment::Quadratic { end, .. } => end,
                            _ => return None,
                        };
                        if last_end != home {
                            return None;
                        }
                        if incoming && handle.segment == last {
                            match self.segments.get(begin + 1)? {
                                Segment::Cubic { .. } | Segment::Quadratic { .. } => Some(Handle {
                                    segment: begin + 1,
                                    kind: HandleKind::FirstControl,
                                }),
                                _ => None,
                            }
                        } else if !incoming && handle.segment == begin + 1 {
                            Some(Handle {
                                segment: last,
                                kind: if matches!(self.segments[last], Segment::Cubic { .. }) {
                                    HandleKind::SecondControl
                                } else {
                                    HandleKind::FirstControl
                                },
                            })
                        } else {
                            None
                        }
                    })
                    .filter(|h| *h != handle);
                if let Some(opposite) = opposite {
                    let old = self
                        .handles()
                        .into_iter()
                        .find(|(h, _)| *h == opposite)
                        .unwrap()
                        .1;
                    let delta = [p[0] - anchor[0], p[1] - anchor[1]];
                    let length = delta[0].hypot(delta[1]);
                    let scale = match link {
                        TangentLink::Smooth if length > 0. => {
                            (old[0] - anchor[0]).hypot(old[1] - anchor[1]) / length
                        }
                        TangentLink::Smooth => 0.,
                        _ => 1.,
                    };
                    next.move_handle(
                        opposite,
                        [anchor[0] - delta[0] * scale, anchor[1] - delta[1] * scale],
                    )?;
                }
            }
        }
        forma_document::path_bounds(&next.path_data()).map_err(|e| e.to_string())?;
        *self = next;
        Ok(())
    }

    pub fn path_data(&self) -> String {
        if !self.changed {
            if let Some(source) = &self.source {
                return source.clone();
            }
        }
        let mut d = String::new();
        for s in &self.segments {
            match *s {
                Segment::Move([x, y]) => write!(d, "M{x} {y} "),
                Segment::Line([x, y]) => write!(d, "L{x} {y} "),
                Segment::Quadratic {
                    control: [x1, y1],
                    end: [x, y],
                } => write!(d, "Q{x1} {y1} {x} {y} "),
                Segment::Cubic {
                    first: [x1, y1],
                    second: [x2, y2],
                    end: [x, y],
                } => write!(d, "C{x1} {y1} {x2} {y2} {x} {y} "),
                Segment::Close => write!(d, "Z "),
            }
            .expect("writing to String cannot fail");
        }
        d.trim_end().into()
    }

    /// Hit radius is in screen pixels, including at different zoom levels.
    pub fn hit_test(&self, screen: Pos2, map: &CanvasMap, radius: f32) -> Option<Handle> {
        if !radius.is_finite() || radius < 0.0 {
            return None;
        }
        self.handles()
            .into_iter()
            .filter_map(|(h, p)| {
                let distance = map.to_screen(p).ok()?.distance(screen);
                (distance <= radius).then_some((h, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(h, _)| h)
    }

    /// Draw a stroke-only live preview, including unfinished Pen geometry.
    /// Call after the raster scene, before handles. No document mutation.
    pub fn paint_outline(
        &self,
        painter: &egui::Painter,
        map: &CanvasMap,
        stroke: Stroke,
    ) -> Result<(), String> {
        let mut current = None;
        let mut start = None;
        for segment in &self.segments {
            match *segment {
                Segment::Move(p) => {
                    current = Some(p);
                    start = Some(p);
                }
                Segment::Line(end) => {
                    if let Some(p) = current {
                        painter.line_segment([map.to_screen(p)?, map.to_screen(end)?], stroke);
                    }
                    current = Some(end);
                }
                Segment::Quadratic { control, end } => {
                    if let Some(p) = current {
                        painter.add(egui::epaint::QuadraticBezierShape::from_points_stroke(
                            [
                                map.to_screen(p)?,
                                map.to_screen(control)?,
                                map.to_screen(end)?,
                            ],
                            false,
                            Color32::TRANSPARENT,
                            stroke,
                        ));
                    }
                    current = Some(end);
                }
                Segment::Cubic { first, second, end } => {
                    if let Some(p) = current {
                        painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                            [
                                map.to_screen(p)?,
                                map.to_screen(first)?,
                                map.to_screen(second)?,
                                map.to_screen(end)?,
                            ],
                            false,
                            Color32::TRANSPARENT,
                            stroke,
                        ));
                    }
                    current = Some(end);
                }
                Segment::Close => {
                    if let (Some(p), Some(end)) = (current, start) {
                        painter.line_segment([map.to_screen(p)?, map.to_screen(end)?], stroke);
                    }
                    current = start;
                }
            }
        }
        Ok(())
    }

    /// Paint after the scene. Does not consume egui input or mutate the project.
    pub fn paint_handles(
        &self,
        painter: &egui::Painter,
        map: &CanvasMap,
        selected: Option<Handle>,
    ) -> Result<(), String> {
        let stroke = Stroke::new(1.0_f32, Color32::from_rgb(80, 130, 210));
        let mut current = None;
        let mut start = None;
        for s in &self.segments {
            match *s {
                Segment::Move(p) => {
                    current = Some(p);
                    start = Some(p);
                }
                Segment::Line(p) => current = Some(p),
                Segment::Quadratic { control, end } => {
                    if let Some(p) = current {
                        painter.line_segment([map.to_screen(p)?, map.to_screen(control)?], stroke);
                    }
                    painter.line_segment([map.to_screen(control)?, map.to_screen(end)?], stroke);
                    current = Some(end);
                }
                Segment::Cubic { first, second, end } => {
                    if let Some(p) = current {
                        painter.line_segment([map.to_screen(p)?, map.to_screen(first)?], stroke);
                    }
                    painter.line_segment([map.to_screen(second)?, map.to_screen(end)?], stroke);
                    current = Some(end);
                }
                Segment::Close => current = start,
            }
        }
        for (h, p) in self.handles() {
            let color = if Some(h) == selected {
                Color32::YELLOW
            } else {
                Color32::WHITE
            };
            let center = map.to_screen(p)?;
            if h.kind == HandleKind::Anchor {
                painter.rect_filled(
                    egui::Rect::from_center_size(center, egui::vec2(8., 8.)),
                    0.,
                    color,
                );
                painter.rect_stroke(
                    egui::Rect::from_center_size(center, egui::vec2(8., 8.)),
                    0.,
                    stroke,
                    egui::StrokeKind::Outside,
                );
            } else {
                painter.circle_filled(center, 4., color);
                painter.circle_stroke(center, 4., stroke);
            }
        }
        Ok(())
    }
}

/// General local-to-screen affine mapping. Supply the evaluated scene transform
/// (including bones), then canvas origin/zoom. Never edit in animated preview.
#[derive(Clone, Copy, Debug)]
pub struct CanvasMap {
    pub origin: Pos2,
    pub zoom: f64,
    pub affine: [f64; 6],
}
impl CanvasMap {
    pub fn from_transform(origin: Pos2, zoom: f64, t: forma_document::Transform) -> Self {
        let (s, c) = t.rotation.to_radians().sin_cos();
        Self {
            origin,
            zoom,
            affine: [
                c * t.scale_x,
                s * t.scale_x,
                -s * t.scale_y,
                c * t.scale_y,
                t.x,
                t.y,
            ],
        }
    }
    fn validate(&self) -> Result<(), String> {
        if self.zoom <= 0.
            || !self.zoom.is_finite()
            || !self.origin.x.is_finite()
            || !self.origin.y.is_finite()
            || self.affine.iter().any(|v| !v.is_finite())
        {
            return Err("Invalid canvas transform".into());
        }
        let [a, b, c, d, _, _] = self.affine;
        let det = a * d - b * c;
        if !det.is_finite() || det.abs() < 1e-12 {
            return Err("Cannot edit a singular canvas transform".into());
        }
        Ok(())
    }
    pub fn to_screen(self, p: Point) -> Result<Pos2, String> {
        self.validate()?;
        valid_point(p)?;
        let [a, b, c, d, x, y] = self.affine;
        let sx = self.origin.x as f64 + self.zoom * (a * p[0] + c * p[1] + x);
        let sy = self.origin.y as f64 + self.zoom * (b * p[0] + d * p[1] + y);
        if !sx.is_finite()
            || !sy.is_finite()
            || sx.abs() > f32::MAX as f64
            || sy.abs() > f32::MAX as f64
        {
            return Err("Canvas coordinates overflow".into());
        }
        Ok(egui::pos2(sx as f32, sy as f32))
    }
    pub fn to_local(self, p: Pos2) -> Result<Point, String> {
        self.validate()?;
        let [a, b, c, d, x, y] = self.affine;
        let sx = (p.x as f64 - self.origin.x as f64) / self.zoom - x;
        let sy = (p.y as f64 - self.origin.y as f64) / self.zoom - y;
        let det = a * d - b * c;
        let out = [(d * sx - c * sy) / det, (-b * sx + a * sy) / det];
        valid_point(out)?;
        Ok(out)
    }
}

/// One gesture = one candidate = one undo entry. The live project is never
/// mutated. Drop to cancel. Refuses commit if the document changed meanwhile.
#[derive(Clone)]
pub struct PathGesture {
    before: Project,
    id: String,
    pub path: EditablePath,
    pub handle: Handle,
    grab_offset: Point,
    initial_path: EditablePath,
}
impl PathGesture {
    pub fn begin(
        project: &Project,
        id: &str,
        screen: Pos2,
        map: &CanvasMap,
    ) -> Result<Option<Self>, String> {
        let node = project
            .nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or("Pfad fehlt")?;
        if node.locked || !node.visible {
            return Err("Pfad ist gesperrt oder ausgeblendet".into());
        }
        let NodeKind::Path { d, .. } = &node.kind else {
            return Err("Pfad wählen, um Punkte zu bearbeiten".into());
        };
        let path = EditablePath::parse(d)?;
        map.validate()?;
        let local = map.to_local(screen)?;
        Ok(path.hit_test(screen, map, 8.).map(|handle| {
            let picked = path
                .handles()
                .into_iter()
                .find(|(h, _)| *h == handle)
                .expect("hit_test returns an existing handle")
                .1;
            Self {
                before: project.clone(),
                id: id.into(),
                initial_path: path.clone(),
                path,
                handle,
                grab_offset: [picked[0] - local[0], picked[1] - local[1]],
            }
        }))
    }
    pub fn update(&mut self, screen: Pos2, map: &CanvasMap) -> Result<(), String> {
        self.update_linked(screen, map, TangentLink::Independent)
    }
    pub fn update_linked(
        &mut self,
        screen: Pos2,
        map: &CanvasMap,
        link: TangentLink,
    ) -> Result<(), String> {
        let local = map.to_local(screen)?;
        let mut next = self.initial_path.clone();
        next.move_tangent(
            self.handle,
            [
                local[0] + self.grab_offset[0],
                local[1] + self.grab_offset[1],
            ],
            link,
        )?;
        self.path = next;
        Ok(())
    }
    /// One validated insertion candidate; caller owns the single undo commit.
    pub fn insert_candidate(
        project: &Project,
        id: &str,
        screen: Pos2,
        map: &CanvasMap,
        max_distance: f32,
    ) -> Result<Option<Project>, String> {
        let node = project
            .nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or("Pfad fehlt")?;
        if node.locked || !node.visible {
            return Err("Pfad ist gesperrt oder ausgeblendet".into());
        }
        let NodeKind::Path { d, .. } = &node.kind else {
            return Err("Pfad wählen, um einen Anker einzufügen".into());
        };
        let Some(path) = EditablePath::parse(d)?.insert_nearest(screen, map, max_distance)? else {
            return Ok(None);
        };
        let mut next = project.clone();
        super::apply_vector_command(
            &mut next,
            id,
            super::VectorCommand::SetPath(path.path_data()),
        )?;
        Ok(Some(next))
    }
    pub fn candidate(&self, current: &Project) -> Result<Project, String> {
        if *current != self.before {
            return Err(
                "Dokument hat sich während der Pfadgeste geändert · abbrechen und erneut versuchen"
                    .into(),
            );
        }
        let mut next = current.clone();
        super::apply_vector_command(
            &mut next,
            &self.id,
            super::VectorCommand::SetPath(self.path.path_data()),
        )?;
        Ok(next)
    }
}

/// Staged Pen path. Click adds a line; drag supplies an outgoing tangent point
/// and creates a smooth cubic connection. Nothing enters the document until finish.
#[derive(Clone, Debug)]
pub struct PenPath {
    path: EditablePath,
    outgoing: Option<Point>,
    closed: bool,
}
impl Default for PenPath {
    fn default() -> Self {
        Self {
            path: EditablePath {
                segments: vec![],
                source: None,
                changed: true,
            },
            outgoing: None,
            closed: false,
        }
    }
}
impl PenPath {
    pub fn path(&self) -> &EditablePath {
        &self.path
    }
    pub fn add_anchor(&mut self, anchor: Point, outgoing: Option<Point>) -> Result<(), String> {
        valid_point(anchor)?;
        if let Some(p) = outgoing {
            valid_point(p)?;
        }
        if self.closed {
            return Err("Pen-Pfad ist geschlossen · fertigstellen oder verwerfen".into());
        }
        if self.path.segments.len() >= 50_000 {
            return Err("SVG path segment limit is 50000".into());
        }
        if self.path.segments.is_empty() {
            self.path.segments.push(Segment::Move(anchor));
        } else if self.outgoing.is_some() || outgoing.is_some() {
            let previous = self
                .path
                .handles()
                .into_iter()
                .rev()
                .find(|(h, _)| h.kind == HandleKind::Anchor)
                .ok_or("Missing previous Pen anchor")?
                .1;
            let second = outgoing
                .map(|p| [2. * anchor[0] - p[0], 2. * anchor[1] - p[1]])
                .unwrap_or(anchor);
            valid_point(second)?;
            self.path.segments.push(Segment::Cubic {
                first: self.outgoing.unwrap_or(previous),
                second,
                end: anchor,
            });
        } else {
            self.path.segments.push(Segment::Line(anchor));
        }
        self.outgoing = outgoing;
        Ok(())
    }
    pub fn close(&mut self) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        if self.path.segments.len() < 2 {
            return Err("Vor dem Schließen mindestens zwei Anker setzen".into());
        }
        if self.path.segments.len() >= 50_000 {
            return Err("SVG path segment limit is 50000".into());
        }
        self.path.segments.push(Segment::Close);
        self.closed = true;
        Ok(())
    }
    /// Candidate includes a new identity-transformed node. Existing ids are errors.
    pub fn candidate(&self, project: &Project, id: &str) -> Result<Project, String> {
        if project.nodes.iter().any(|n| n.id == id) {
            return Err("Pen node id already exists".into());
        }
        let d = self.path.path_data();
        let bounds = forma_document::path_bounds(&d).map_err(|e| e.to_string())?;
        let mut next = project.clone();
        next.nodes.push(Node::new(
            id,
            NodeKind::Path {
                d,
                bounds,
                evenodd: false,
            },
        ));
        next.validate()
            .map_err(|e| format!("Pen-Pfad abgelehnt: {e}"))?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project(d: &str) -> Project {
        let mut p = Project::default();
        p.nodes.push(Node::new(
            "path",
            NodeKind::Path {
                d: d.into(),
                bounds: forma_document::path_bounds(d).unwrap(),
                evenodd: false,
            },
        ));
        p
    }
    fn evaluate(points: &[Point], t: f64) -> Point {
        let mut row = points.to_vec();
        while row.len() > 1 {
            row = row.windows(2).map(|p| lerp(p[0], p[1], t)).collect();
        }
        row[0]
    }
    fn segment_points(path: &EditablePath, index: usize) -> Vec<Point> {
        let (start, home) = path.segment_start(index).unwrap();
        match path.segments[index] {
            Segment::Line(end) => vec![start, end],
            Segment::Close => vec![start, home],
            Segment::Quadratic { control, end } => vec![start, control, end],
            Segment::Cubic { first, second, end } => vec![start, first, second, end],
            _ => panic!("not an edge"),
        }
    }
    #[test]
    fn splits_preserve_sampled_geometry_and_source() {
        for d in [
            "M0 0 L100 60",
            "M0 0 Q30 100 100 0",
            "M0 0 C100 130 -60 50 100 0",
            "M0 0 L100 60 Z",
        ] {
            let path = EditablePath::parse(d).unwrap();
            let index = path.segments.len() - 1;
            for split in [0.13, 0.5, 0.82] {
                let next = path.split_segment(index, split).unwrap();
                for sample in 0..=100 {
                    let t = sample as f64 / 100.;
                    let expected = evaluate(&segment_points(&path, index), t);
                    let (i, u) = if t <= split {
                        (index, t / split)
                    } else {
                        (index + 1, (t - split) / (1. - split))
                    };
                    let actual = evaluate(&segment_points(&next, i), u);
                    assert!((actual[0] - expected[0]).hypot(actual[1] - expected[1]) < 1e-10);
                }
                if d.ends_with('Z') {
                    assert!(next.path_data().ends_with('Z'));
                }
            }
            assert_eq!(path.path_data(), d);
            assert!(path.split_segment(index, 0.).is_err());
            assert!(path.split_segment(99, 0.5).is_err());
        }
    }
    #[test]
    fn nearest_is_screen_bounded_mirrored_and_subpath_aware() {
        let path = EditablePath::parse("M0 0 L100 0 M0 100 Q50 200 100 100").unwrap();
        let map = CanvasMap::from_transform(
            egui::pos2(20., 30.),
            2.,
            forma_document::Transform {
                scale_x: -2.,
                scale_y: 0.5,
                ..Default::default()
            },
        );
        let hit = map.to_screen([50., 150.]).unwrap();
        let next = path.insert_nearest(hit, &map, 1.).unwrap().unwrap();
        assert_eq!(next.segments.len(), path.segments.len() + 1);
        assert_eq!(&next.segments[..3], &path.segments[..3]);
        assert!(path
            .insert_nearest(hit + egui::vec2(0., 10.), &map, 2.)
            .unwrap()
            .is_none());
        assert!(path
            .insert_nearest(map.to_screen([0., 0.]).unwrap(), &map, 8.)
            .unwrap()
            .is_none());
        let mut bad = map;
        bad.zoom = 0.;
        assert!(path.insert_nearest(hit, &bad, 8.).is_err());
        bad = map;
        bad.affine[0] = 0.;
        bad.affine[1] = 0.;
        assert!(path.insert_nearest(hit, &bad, 8.).is_err());
        assert_eq!(path.path_data(), "M0 0 L100 0 M0 100 Q50 200 100 100");
    }
    #[test]
    fn nearest_cubic_and_closing_edge_split_at_the_hit() {
        let map = CanvasMap::from_transform(
            egui::pos2(20., 30.),
            3.,
            forma_document::Transform {
                rotation: 27.,
                scale_x: -1.5,
                scale_y: 0.7,
                ..Default::default()
            },
        );
        for (d, index, t) in [
            ("M0 0 C10 80 70 -30 100 50", 1, 0.37),
            ("M0 0 L100 0 L100 100 Z", 3, 0.5),
        ] {
            let path = EditablePath::parse(d).unwrap();
            let local = evaluate(&segment_points(&path, index), t);
            let screen = map.to_screen(local).unwrap();
            let next = path.insert_nearest(screen, &map, 0.1).unwrap().unwrap();
            let end = *segment_points(&next, index).last().unwrap();
            assert!(map.to_screen(end).unwrap().distance(screen) < 1e-3);
            assert_eq!(next.segments.len(), path.segments.len() + 1);
            if d.ends_with('Z') {
                assert!(next.path_data().ends_with('Z'));
            }
        }
    }
    #[test]
    fn large_screen_edges_do_not_exclude_small_interior_parameters() {
        let path = EditablePath::parse("M0 0 L10000000 0").unwrap();
        let map = CanvasMap::from_transform(Pos2::ZERO, 100., Default::default());
        let next = path
            .insert_nearest(egui::pos2(50., 0.), &map, 1.)
            .unwrap()
            .unwrap();
        assert!(matches!(next.segments[1], Segment::Line([x, 0.]) if (x-0.5).abs() < 1e-8));
    }
    #[test]
    fn quadratic_links_both_neighbors_without_crossing_subpaths() {
        let mut path = EditablePath::parse(
            "M0 0 C10 0 20 0 30 0 Q40 10 50 0 C60 0 70 0 80 0 M100 0 C110 0 120 0 130 0",
        )
        .unwrap();
        let before = path.segments.clone();
        path.move_tangent(
            Handle {
                segment: 2,
                kind: HandleKind::FirstControl,
            },
            [40., 20.],
            TangentLink::Symmetric,
        )
        .unwrap();
        assert!(matches!(
            path.segments[1],
            Segment::Cubic {
                second: [20., -20.],
                ..
            }
        ));
        assert!(matches!(
            path.segments[3],
            Segment::Cubic {
                first: [60., -20.],
                ..
            }
        ));
        assert_eq!(path.segments[5], before[5]);
        path.move_tangent(
            Handle {
                segment: 5,
                kind: HandleKind::FirstControl,
            },
            [100., 10.],
            TangentLink::Symmetric,
        )
        .unwrap();
        assert_eq!(
            path.segments[3].clone(),
            Segment::Cubic {
                first: [60., -20.],
                second: [70., 0.],
                end: [80., 0.]
            }
        );
    }
    #[test]
    fn gesture_link_toggle_uses_original_handles_and_failed_updates_keep_preview() {
        let p = project("M0 0 C10 0 20 0 30 0 C40 0 50 0 60 0");
        let map = CanvasMap::from_transform(Pos2::ZERO, 1., Default::default());
        let mut g = PathGesture::begin(&p, "path", egui::pos2(40., 0.), &map)
            .unwrap()
            .unwrap();
        g.update_linked(egui::pos2(30., 20.), &map, TangentLink::Symmetric)
            .unwrap();
        assert!(matches!(
            g.path.segments[1],
            Segment::Cubic {
                second: [30., -20.],
                ..
            }
        ));
        g.update(egui::pos2(30., 20.), &map).unwrap();
        assert!(matches!(
            g.path.segments[1],
            Segment::Cubic {
                second: [20., 0.],
                ..
            }
        ));
        let before = g.path.path_data();
        assert!(g
            .update_linked(egui::pos2(f32::NAN, 0.), &map, TangentLink::Smooth)
            .is_err());
        assert_eq!(g.path.path_data(), before);
    }
    #[test]
    fn tangent_links_keep_handles_and_are_atomic() {
        let d = "M0 0 C10 0 20 0 30 0 C40 0 50 0 60 0";
        let source = EditablePath::parse(d).unwrap();
        let handle = Handle {
            segment: 2,
            kind: HandleKind::FirstControl,
        };
        for (mode, expected) in [
            (TangentLink::Independent, [20., 0.]),
            (TangentLink::Smooth, [30., -10.]),
            (TangentLink::Symmetric, [30., -20.]),
        ] {
            let mut path = source.clone();
            path.move_tangent(handle, [30., 20.], mode).unwrap();
            assert!(
                matches!(path.segments[1], Segment::Cubic { second, end: [30., 0.], .. } if second == expected)
            );
            assert_eq!(path.handles().len(), source.handles().len());
        }
        let mut path = source.clone();
        path.move_tangent(
            Handle {
                segment: 1,
                kind: HandleKind::FirstControl,
            },
            [0., 10.],
            TangentLink::Symmetric,
        )
        .unwrap();
        assert_eq!(path.segments[2], source.segments[2]); // open endpoint
        let before = path.path_data();
        assert!(path
            .move_tangent(handle, [f64::NAN, 0.], TangentLink::Symmetric)
            .is_err());
        assert_eq!(path.path_data(), before);
        let mut cusp = EditablePath::parse("M0 0 L30 0 C40 0 50 0 60 0").unwrap();
        let count = cusp.handles().len();
        cusp.move_tangent(handle, [30., 20.], TangentLink::Symmetric)
            .unwrap();
        assert_eq!(cusp.handles().len(), count);
        assert_eq!(cusp.segments[1], Segment::Line([30., 0.]));
        let mut closed = EditablePath::parse("M0 0 C10 0 20 10 30 10 C20 10 -10 0 0 0 Z").unwrap();
        closed
            .move_tangent(
                Handle {
                    segment: 1,
                    kind: HandleKind::FirstControl,
                },
                [0., 20.],
                TangentLink::Symmetric,
            )
            .unwrap();
        assert!(matches!(
            closed.segments[2],
            Segment::Cubic {
                second: [0., -20.],
                ..
            }
        ));
    }
    #[test]
    fn insertion_candidates_are_atomic_and_clear_provenance() {
        let mut p = Project::default();
        p.nodes.push(Node::rect("path", 0., 0., 100., 100.));
        super::super::apply_vector_command(
            &mut p,
            "path",
            super::super::VectorCommand::ConvertToPath,
        )
        .unwrap();
        let before = p.clone();
        let map = CanvasMap::from_transform(Pos2::ZERO, 1., Default::default());
        let next = PathGesture::insert_candidate(&p, "path", egui::pos2(50., 0.), &map, 8.)
            .unwrap()
            .unwrap();
        next.validate().unwrap();
        assert!(next.nodes[0].original_kind.is_none());
        assert_eq!(p, before);
        assert!(
            PathGesture::insert_candidate(&p, "path", egui::pos2(300., 300.), &map, 8.)
                .unwrap()
                .is_none()
        );
        p.nodes[0].locked = true;
        assert!(PathGesture::insert_candidate(&p, "path", egui::pos2(50., 0.), &map, 8.).is_err());
    }
    #[test]
    fn parse_all_commands_and_preserve_source() {
        let d = "m10 10 l20 0 h5 v10 q5 5 10 0 t10 0 c1 2 3 4 5 6 s5 5 10 0 a5 5 0 0 1 10 10 z m80 80 l2 2";
        let path = EditablePath::parse(d).unwrap();
        assert_eq!(path.path_data(), d);
        assert!(path
            .segments()
            .iter()
            .any(|s| matches!(s, Segment::Quadratic { .. })));
        assert!(path
            .segments()
            .iter()
            .any(|s| matches!(s, Segment::Cubic { .. })));
        assert!(path.segments().iter().any(|s| matches!(s, Segment::Close)));
    }
    #[test]
    fn edits_controls_and_recalculates_bounds_transactionally() {
        let p = project("M0 0 Q10 20 30 0 C40 10 50 10 60 0 Z");
        let map = CanvasMap::from_transform(Pos2::ZERO, 1., Default::default());
        let mut g = PathGesture::begin(&p, "path", egui::pos2(10., 20.), &map)
            .unwrap()
            .unwrap();
        g.update(egui::pos2(10., 40.), &map).unwrap();
        let next = g.candidate(&p).unwrap();
        next.validate().unwrap();
        assert_ne!(next, p);
        assert!(matches!(&next.nodes[0].kind, NodeKind::Path { bounds, .. } if bounds[3] >= 20.));
        assert!(g.candidate(&next).is_err());
        assert_eq!(g.before, p);
    }
    #[test]
    fn malformed_and_invalid_handles_do_not_mutate() {
        for d in [
            "",
            "M0",
            "M0 0 LNaN 0",
            "M0 0 L10000001 0",
            "M0 0 L1 1 garbage",
        ] {
            assert!(EditablePath::parse(d).is_err(), "{d}");
        }
        let mut p = EditablePath::parse("M0 0 L10 10").unwrap();
        assert!(p
            .move_handle(
                Handle {
                    segment: 1,
                    kind: HandleKind::Anchor
                },
                [f64::NAN, 0.]
            )
            .is_err());
        assert!(p
            .move_handle(
                Handle {
                    segment: 99,
                    kind: HandleKind::Anchor
                },
                [0., 0.]
            )
            .is_err());
        assert_eq!(p.path_data(), "M0 0 L10 10");
    }
    #[test]
    fn transform_round_trip_and_singular_rejection() {
        let map = CanvasMap::from_transform(
            egui::pos2(25., 60.),
            2.,
            forma_document::Transform {
                x: 8.,
                y: -5.,
                rotation: 35.,
                scale_x: -2.,
                scale_y: 0.5,
            },
        );
        let p = [30., 40.];
        let q = map.to_local(map.to_screen(p).unwrap()).unwrap();
        assert!((p[0] - q[0]).abs() < 1e-4 && (p[1] - q[1]).abs() < 1e-4);
        let mut bad = map;
        bad.affine[0] = 0.;
        bad.affine[1] = 0.;
        assert!(bad.to_local(Pos2::ZERO).is_err());
    }
    #[test]
    fn pen_is_staged_validated_and_supports_cubic_close() {
        let p = Project::default();
        let mut pen = PenPath::default();
        assert!(pen.candidate(&p, "pen").is_err());
        pen.add_anchor([10., 10.], Some([20., 0.])).unwrap();
        pen.add_anchor([50., 50.], Some([60., 40.])).unwrap();
        pen.close().unwrap();
        assert!(pen.add_anchor([0., 0.], None).is_err());
        let next = pen.candidate(&p, "pen").unwrap();
        next.validate().unwrap();
        assert!(
            matches!(&next.nodes[0].kind, NodeKind::Path { d, .. } if d.contains('C') && d.ends_with('Z'))
        );
        assert!(pen.candidate(&next, "pen").is_err());
        assert!(p.nodes.is_empty());
    }
    #[test]
    fn locked_and_noop_gestures() {
        let mut p = project("M0 0 L10 10");
        let map = CanvasMap::from_transform(Pos2::ZERO, 1., Default::default());
        let g = PathGesture::begin(&p, "path", Pos2::ZERO, &map)
            .unwrap()
            .unwrap();
        assert_eq!(g.candidate(&p).unwrap(), p);
        p.nodes[0].locked = true;
        assert!(PathGesture::begin(&p, "path", Pos2::ZERO, &map).is_err());
    }
}
