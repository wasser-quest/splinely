//! Staged freehand input. Call once per canvas frame, including when disabled.
//! No history or selection mutation: only a release returns a validated candidate.
use eframe::egui::{self, Color32, Event, Pos2, Rect, Stroke, TouchDeviceId, TouchId, TouchPhase};
use splinely_document::{Color, Node, NodeKind, Project};
use std::fmt::Write;

/// Hard resource bound, before simplification (never silently truncate a stroke).
pub const MAX_POINTS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub screen: Pos2,
    pub document: [f64; 2],
    /// Missing/invalid force is full-width pointer fallback, not measured pressure.
    pub pressure: f64,
    pub measured_pressure: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Viewport {
    origin: Pos2,
    zoom: f64,
}
impl Viewport {
    fn document(self, screen: Pos2) -> Result<[f64; 2], String> {
        if !self.zoom.is_finite()
            || self.zoom <= 0.0
            || !self.origin.x.is_finite()
            || !self.origin.y.is_finite()
        {
            return Err("Invalid freehand viewport".into());
        }
        let p = [
            (screen.x as f64 - self.origin.x as f64) / self.zoom,
            (screen.y as f64 - self.origin.y as f64) / self.zoom,
        ];
        valid_point(p)?;
        Ok(p)
    }
}
fn valid_point(p: [f64; 2]) -> Result<(), String> {
    if p.iter().any(|v| !v.is_finite() || v.abs() > 1e7) {
        Err("Freehand coordinates must be finite and within +/- 10000000".into())
    } else {
        Ok(())
    }
}
fn pressure(force: Option<f32>) -> (f64, bool) {
    match force.filter(|v| v.is_finite()) {
        Some(v) => (v.clamp(0.0, 1.0) as f64, true),
        None => (1.0, false),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreehandOptions {
    /// SVG document units, independent of zoom.
    pub width: f64,
    /// Maximum centerline deviation in document units.
    pub tolerance: f64,
    /// Measured touch force creates a filled editable outline. Otherwise a stroke.
    pub pressure_outline: bool,
    /// Force a constant-width filled outline even without measured pressure.
    pub outline: bool,
    pub color: Color,
}
impl Default for FreehandOptions {
    fn default() -> Self {
        Self {
            width: 3.0,
            tolerance: 0.5,
            pressure_outline: true,
            outline: false,
            color: Color::rgb(30, 30, 30),
        }
    }
}
impl FreehandOptions {
    fn validate(self) -> Result<(), String> {
        if !self.width.is_finite()
            || self.width <= 0.0
            || self.width > 1e7
            || !self.tolerance.is_finite()
            || !(0.0..=1e7).contains(&self.tolerance)
            || [self.color.r, self.color.g, self.color.b, self.color.a]
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("Invalid freehand options".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Source {
    Pointer,
    Touch(TouchDeviceId, TouchId),
}
struct Draft {
    before: Project,
    viewport: Viewport,
    options: FreehandOptions,
    source: Source,
    points: Vec<Sample>,
}
impl Draft {
    fn push(&mut self, screen: Pos2, force: Option<f32>) -> Result<(), String> {
        let document = self.viewport.document(screen)?;
        let (pressure, measured_pressure) = pressure(force);
        let sample = Sample {
            screen,
            document,
            pressure,
            measured_pressure,
        };
        if let Some(last) = self.points.last_mut() {
            if last.document == document {
                // Touch-up often has no force and repeats the last position.
                // Do not erase a measured pressure at an unchanged location.
                if measured_pressure || !last.measured_pressure {
                    *last = sample;
                }
                return Ok(());
            }
        }
        if self.points.len() >= MAX_POINTS {
            return Err(format!(
                "Freihand ist auf {MAX_POINTS} Punkte begrenzt · Strich abgebrochen"
            ));
        }
        self.points.push(sample);
        Ok(())
    }
}

#[derive(Default)]
pub struct FreehandTool {
    pub options: FreehandOptions,
    draft: Option<Draft>,
}
impl FreehandTool {
    pub fn cancel(&mut self) {
        self.draft = None;
    }
    pub fn is_drawing(&self) -> bool {
        self.draft.is_some()
    }
    pub fn samples(&self) -> &[Sample] {
        self.draft.as_ref().map_or(&[], |d| d.points.as_slice())
    }

    /// `origin` includes canvas pan; `zoom` is positive screen/document scale.
    /// Supply a drag-sensitive canvas response. Caller must pass enabled=false
    /// for pan, playback, modal UI, or other tools BEFORE calling this method.
    /// On release, commit the candidate once and select the returned node id.
    /// Errors, Escape, disabled frames, lost pointer, and touch Cancel discard.
    // Explicit canvas context keeps pointer routing and document mutations separate.
    #[allow(clippy::too_many_arguments)]
    pub fn handle(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        response: &egui::Response,
        origin: Pos2,
        zoom: f64,
        project: &Project,
        enabled: bool,
    ) -> Result<Option<(Project, String)>, String> {
        if !enabled
            || !ui.is_enabled()
            || !response.enabled()
            || ui.input(|i| i.key_pressed(egui::Key::Escape))
        {
            self.cancel();
            return Ok(None);
        }
        let events = ui.input(|i| i.events.clone());
        let viewport = Viewport { origin, zoom };
        // Respect egui's hit testing (including occluding widgets), not just bounds.
        let may_start = response.contains_pointer() || response.is_pointer_button_down_on();
        let result = self.events(&events, response.rect, may_start, viewport, project);
        if result.is_err() {
            self.cancel();
        } else if let Some(draft) = &self.draft {
            let color = Color32::from_rgba_unmultiplied(
                (draft.options.color.r * 255.0) as u8,
                (draft.options.color.g * 255.0) as u8,
                (draft.options.color.b * 255.0) as u8,
                (draft.options.color.a * 255.0) as u8,
            );
            let painter = painter.with_clip_rect(painter.clip_rect().intersect(response.rect));
            for pair in draft.points.windows(2) {
                let force = if draft.options.pressure_outline {
                    (pair[0].pressure + pair[1].pressure) * 0.5
                } else {
                    1.0
                };
                let width =
                    (draft.options.width * force.max(0.05) * zoom).clamp(0.1, 10000.0) as f32;
                painter.line_segment([pair[0].screen, pair[1].screen], Stroke::new(width, color));
            }
        }
        result
    }

    fn begin(
        &mut self,
        source: Source,
        pos: Pos2,
        force: Option<f32>,
        viewport: Viewport,
        project: &Project,
    ) -> Result<(), String> {
        self.options.validate()?;
        viewport.document(pos)?;
        project.validate().map_err(|e| e.to_string())?;
        let mut draft = Draft {
            before: project.clone(),
            viewport,
            options: self.options,
            source,
            points: Vec::new(),
        };
        draft.push(pos, force)?;
        self.draft = Some(draft);
        Ok(())
    }
    fn finish(&mut self, project: &Project) -> Result<Option<(Project, String)>, String> {
        let Some(draft) = self.draft.take() else {
            return Ok(None);
        };
        if draft.points.len() < 2 {
            return Ok(None); // A tap is not a stroke.
        }
        if draft.before != *project {
            return Err("Document changed during freehand gesture".into());
        }
        // IDs share the document namespace, not just the nodes namespace.
        let mut index = 1;
        let id = loop {
            let id = format!("freehand_{index}");
            if !project.nodes.iter().any(|n| n.id == id)
                && !project.bones.iter().any(|n| n.id == id)
                && !project.clips.iter().any(|n| n.id == id)
                && !project.state_machines.iter().any(|n| n.id == id)
                && !project.audio_assets.iter().any(|n| n.id == id)
                && !project.audio_cues.iter().any(|n| n.id == id)
            {
                break id;
            }
            index += 1;
        };
        let next = stroke_candidate(project, &id, &draft.points, draft.options)?;
        Ok(Some((next, id)))
    }
    fn events(
        &mut self,
        events: &[Event],
        rect: Rect,
        may_start: bool,
        viewport: Viewport,
        project: &Project,
    ) -> Result<Option<(Project, String)>, String> {
        viewport.document(viewport.origin)?;
        if let Some(d) = &self.draft {
            if d.viewport != viewport || d.before != *project {
                self.cancel();
                return Err(
                    "Ansicht oder Dokument hat sich während des Freihandstrichs geändert".into(),
                );
            }
        }
        // Touch and its synthetic pointer events often coexist. Never record both.
        let captured_pointer = self
            .draft
            .as_ref()
            .is_some_and(|d| d.source == Source::Pointer);
        let touch_frame = !captured_pointer
            && (events.iter().any(|e| matches!(e, Event::Touch { .. }))
                || self
                    .draft
                    .as_ref()
                    .is_some_and(|d| matches!(d.source, Source::Touch(..))));
        for event in events {
            match *event {
                Event::Touch {
                    device_id,
                    id,
                    phase,
                    pos,
                    force,
                } => {
                    let source = Source::Touch(device_id, id);
                    let active = self.draft.as_ref().is_some_and(|d| d.source == source);
                    match phase {
                        TouchPhase::Start
                            if self.draft.is_none() && may_start && rect.contains(pos) =>
                        {
                            self.begin(source, pos, force, viewport, project)?;
                        }
                        TouchPhase::Move if active => {
                            self.draft.as_mut().unwrap().push(pos, force)?
                        }
                        TouchPhase::End if active => {
                            self.draft.as_mut().unwrap().push(pos, force)?;
                            return self.finish(project);
                        }
                        TouchPhase::Cancel if active => {
                            self.cancel();
                            return Ok(None);
                        }
                        _ => {} // Ignore secondary contacts; this is not driver-level palm rejection.
                    }
                }
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    ..
                } if !touch_frame => {
                    if pressed && self.draft.is_none() && may_start && rect.contains(pos) {
                        self.begin(Source::Pointer, pos, None, viewport, project)?;
                    } else if !pressed && self.draft.is_some() {
                        self.draft.as_mut().unwrap().push(pos, None)?;
                        return self.finish(project);
                    }
                }
                Event::PointerMoved(pos) if !touch_frame => {
                    if let Some(d) = &mut self.draft {
                        d.push(pos, None)?;
                    }
                }
                Event::PointerGone if !touch_frame => {
                    self.cancel();
                    return Ok(None);
                }
                Event::WindowFocused(false) => {
                    self.cancel();
                    return Ok(None);
                }
                _ => {}
            }
        }
        Ok(None)
    }
}

/// Iterative Ramer–Douglas–Peucker: bounded memory, no recursive stack growth.
/// Endpoints are retained. Pressure changes are retained to half-width tolerance.
fn simplify(points: &[Sample], tolerance: f64, pressure_width: f64) -> Vec<Sample> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0, points.len() - 1)];
    while let Some((first, last)) = stack.pop() {
        let a = points[first].document;
        let b = points[last].document;
        let delta = [b[0] - a[0], b[1] - a[1]];
        let length2 = delta[0] * delta[0] + delta[1] * delta[1];
        let mut worst = tolerance;
        let mut split = None;
        for i in first + 1..last {
            let p = points[i].document;
            let t = if length2 == 0.0 {
                0.0
            } else {
                (((p[0] - a[0]) * delta[0] + (p[1] - a[1]) * delta[1]) / length2).clamp(0.0, 1.0)
            };
            let distance = (p[0] - a[0] - t * delta[0]).hypot(p[1] - a[1] - t * delta[1]);
            let expected = points[first].pressure * (1.0 - t) + points[last].pressure * t;
            let error = distance.max((points[i].pressure - expected).abs() * pressure_width * 0.5);
            if error > worst {
                worst = error;
                split = Some(i);
            }
        }
        if let Some(i) = split {
            keep[i] = true;
            stack.push((first, i));
            stack.push((i, last));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(p, keep)| keep.then_some(*p))
        .collect()
}

/// Build an editable SVG candidate without ever mutating `project`.
/// Rejects invalid samples, resource overflow, duplicate IDs, and invalid projects.
pub fn stroke_candidate(
    project: &Project,
    id: &str,
    samples: &[Sample],
    options: FreehandOptions,
) -> Result<Project, String> {
    options.validate()?;
    if samples.len() < 2 || samples.len() > MAX_POINTS {
        return Err(format!("Freehand needs 2..={MAX_POINTS} points"));
    }
    for s in samples {
        valid_point(s.document)?;
        if !s.screen.x.is_finite()
            || !s.screen.y.is_finite()
            || !s.pressure.is_finite()
            || !(0.0..=1.0).contains(&s.pressure)
        {
            return Err("Invalid freehand sample".into());
        }
    }
    let variable = options.pressure_outline && samples.iter().any(|s| s.measured_pressure);
    let filled = options.outline || variable;
    let points = simplify(
        samples,
        options.tolerance,
        if variable { options.width } else { 0.0 },
    );
    // Remove adjacent duplicate geometry, including caller-supplied samples.
    let mut points = points;
    points.dedup_by(|a, b| a.document == b.document);
    if points.len() < 2 {
        return Err("Freehand stroke has no length".into());
    }
    let radius =
        |p: &Sample| options.width * if variable { p.pressure.max(0.05) } else { 1.0 } * 0.5;
    let mut d = String::new();
    if filled {
        // Same-winding overlapping pieces use nonzero coverage, not even-odd
        // cancellation. Retraces and crossings therefore stay filled.
        for pair in points.windows(2) {
            let a = pair[0].document;
            let b = pair[1].document;
            let dx = b[0] - a[0];
            let dy = b[1] - a[1];
            let len = dx.hypot(dy);
            let n = [-dy / len, dx / len];
            let ra = radius(&pair[0]);
            let rb = radius(&pair[1]);
            for (i, p) in [
                [a[0] + n[0] * ra, a[1] + n[1] * ra],
                [b[0] + n[0] * rb, b[1] + n[1] * rb],
                [b[0] - n[0] * rb, b[1] - n[1] * rb],
                [a[0] - n[0] * ra, a[1] - n[1] * ra],
            ]
            .iter()
            .enumerate()
            {
                valid_point(*p)?;
                write!(d, "{}{} {} ", if i == 0 { 'M' } else { 'L' }, p[0], p[1]).unwrap();
            }
            d.push_str("Z ");
        }
        for p in &points {
            let [x, y] = p.document;
            let r = radius(p);
            let k = r * 0.552_284_749_830_793_6;
            for point in [[x - r, y - r], [x + r, y + r]] {
                valid_point(point)?;
            }
            write!(d, "M{} {} C{} {} {} {} {} {} C{} {} {} {} {} {} C{} {} {} {} {} {} C{} {} {} {} {} {} Z ",
                x+r,y, x+r,y-k,x+k,y-r,x,y-r, x-k,y-r,x-r,y-k,x-r,y,
                x-r,y+k,x-k,y+r,x,y+r, x+k,y+r,x+r,y+k,x+r,y).unwrap();
        }
        d.pop();
    } else {
        for (i, p) in points.iter().enumerate() {
            write!(
                d,
                "{}{} {} ",
                if i == 0 { 'M' } else { 'L' },
                p.document[0],
                p.document[1]
            )
            .unwrap();
        }
    }
    let bounds = splinely_document::path_bounds(&d).map_err(|e| e.to_string())?;
    let mut node = Node::new(
        id,
        NodeKind::Path {
            d,
            bounds,
            evenodd: false,
        },
    );
    if filled {
        node.style.fill = options.color;
    } else {
        node.style.fill = Color {
            a: 0.0,
            ..options.color
        };
        node.style.stroke = Some(options.color);
        node.style.stroke_width = options.width;
    }
    let mut next = project.clone();
    next.nodes.push(node);
    next.validate()
        .map_err(|e| format!("Freihandpfad abgelehnt: {e}"))?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(x: f64, y: f64) -> Sample {
        Sample {
            screen: egui::pos2(x as f32, y as f32),
            document: [x, y],
            pressure: 1.0,
            measured_pressure: false,
        }
    }
    fn viewport() -> Viewport {
        Viewport {
            origin: Pos2::ZERO,
            zoom: 1.0,
        }
    }
    fn rect() -> Rect {
        Rect::from_min_max(Pos2::ZERO, egui::pos2(100.0, 100.0))
    }
    fn button(x: f32, pressed: bool) -> Event {
        Event::PointerButton {
            pos: egui::pos2(x, 10.0),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    fn touch(id: u64, phase: TouchPhase, x: f32, force: Option<f32>) -> Event {
        Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(id),
            phase,
            pos: egui::pos2(x, 10.0),
            force,
        }
    }
    #[test]
    fn straight_curved_noisy_and_pressure_simplification() {
        let line: Vec<_> = (0..100).map(|i| sample(i as f64, 0.0)).collect();
        assert_eq!(simplify(&line, 0.5, 0.0).len(), 2);
        let noise: Vec<_> = (0..100)
            .map(|i| sample(i as f64, if i % 2 == 0 { 0.1 } else { -0.1 }))
            .collect();
        assert_eq!(simplify(&noise, 0.5, 0.0).len(), 2);
        let curve: Vec<_> = (0..100)
            .map(|i| sample(i as f64, (i as f64 * 0.08).sin() * 10.0))
            .collect();
        let reduced = simplify(&curve, 0.25, 0.0);
        assert!(reduced.len() > 4 && reduced.len() < curve.len());
        assert_eq!(reduced.first(), curve.first());
        assert_eq!(reduced.last(), curve.last());
        // Every original point is within the requested tolerance of a retained segment.
        for p in &curve {
            let distance = reduced
                .windows(2)
                .map(|w| {
                    let a = w[0].document;
                    let b = w[1].document;
                    let p = p.document;
                    let dx = b[0] - a[0];
                    let dy = b[1] - a[1];
                    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy))
                        .clamp(0.0, 1.0);
                    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
                })
                .fold(f64::INFINITY, f64::min);
            assert!(distance <= 0.250001);
        }
        let mut pressure_line = vec![sample(0.0, 0.0), sample(10.0, 0.0), sample(20.0, 0.0)];
        pressure_line[1].pressure = 0.1;
        assert_eq!(simplify(&pressure_line, 0.5, 10.0).len(), 3);
    }
    #[test]
    fn pressure_fallback_and_outline_geometry() {
        assert_eq!(pressure(None), (1.0, false));
        assert_eq!(pressure(Some(f32::NAN)), (1.0, false));
        assert_eq!(pressure(Some(-1.0)), (0.0, true));
        assert_eq!(pressure(Some(2.0)), (1.0, true));
        let project = Project::default();
        let mut points = vec![sample(10.0, 10.0), sample(30.0, 10.0)];
        let next = stroke_candidate(&project, "ink", &points, Default::default()).unwrap();
        assert_eq!(next.nodes[0].style.stroke_width, 3.0);
        assert_eq!(next.nodes[0].style.fill.a, 0.0);
        points[0].pressure = 0.2;
        points[0].measured_pressure = true;
        let next = stroke_candidate(&project, "ink", &points, Default::default()).unwrap();
        next.validate().unwrap();
        assert!(next.nodes[0].style.stroke.is_none());
        assert!(
            matches!(&next.nodes[0].kind, NodeKind::Path { d, evenodd: false, .. } if d.ends_with('Z') && d.contains("10 10.3"))
        );
        let options = FreehandOptions {
            outline: true,
            ..Default::default()
        };
        let outline = stroke_candidate(&project, "ink", &points, options).unwrap();
        assert!(matches!(&outline.nodes[0].kind, NodeKind::Path { d, .. } if d.ends_with('Z')));
        assert!(project.nodes.is_empty());
    }
    #[test]
    fn secondary_touch_does_not_swallow_pointer_termination() {
        let project = Project::default();
        let mut tool = FreehandTool::default();
        tool.events(&[button(10.0, true)], rect(), true, viewport(), &project)
            .unwrap();
        let result = tool
            .events(
                &[touch(2, TouchPhase::Start, 40.0, None), button(30.0, false)],
                rect(),
                true,
                viewport(),
                &project,
            )
            .unwrap();
        assert!(result.is_some());
        assert!(!tool.is_drawing());
        tool.events(&[button(10.0, true)], rect(), true, viewport(), &project)
            .unwrap();
        tool.events(
            &[touch(2, TouchPhase::Move, 40.0, None), Event::PointerGone],
            rect(),
            true,
            viewport(),
            &project,
        )
        .unwrap();
        assert!(!tool.is_drawing());
    }
    #[test]
    fn pressure_retrace_keeps_middle_filled() {
        let mut points = vec![sample(10.0, 10.0), sample(30.0, 10.0), sample(10.0, 10.0)];
        for p in &mut points {
            p.measured_pressure = true;
        }
        let project = stroke_candidate(
            &Project::default(),
            "ink",
            &points,
            FreehandOptions::default(),
        )
        .unwrap();
        let svg = project.to_static_svg(0.0).unwrap();
        let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).unwrap();
        let mut pixmap = resvg::tiny_skia::Pixmap::new(50, 20).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        assert!(pixmap.pixel(20, 10).unwrap().alpha() > 240);
    }
    #[test]
    fn transforms_and_invalid_viewport() {
        let map = Viewport {
            origin: egui::pos2(40.0, -20.0),
            zoom: 2.0,
        };
        assert_eq!(map.document(egui::pos2(60.0, 20.0)).unwrap(), [10.0, 20.0]);
        for zoom in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e-300] {
            assert!(Viewport { zoom, ..map }
                .document(egui::pos2(60.0, 20.0))
                .is_err());
        }
        assert!(Viewport {
            origin: egui::pos2(f32::NAN, 0.0),
            ..map
        }
        .document(Pos2::ZERO)
        .is_err());
        assert!(map.document(egui::pos2(f32::INFINITY, 0.0)).is_err());
    }
    #[test]
    fn pointer_atomic_release_outside_start_and_cancel() {
        let project = Project::default();
        let mut tool = FreehandTool::default();
        assert!(tool
            .events(
                &[
                    button(-10.0, true),
                    Event::PointerMoved(egui::pos2(20.0, 10.0)),
                    button(30.0, false)
                ],
                rect(),
                true,
                viewport(),
                &project
            )
            .unwrap()
            .is_none());
        assert!(tool
            .events(&[button(10.0, true)], rect(), false, viewport(), &project)
            .unwrap()
            .is_none());
        assert!(!tool.is_drawing());
        assert!(tool
            .events(
                &[
                    button(10.0, true),
                    Event::PointerMoved(egui::pos2(20.0, 10.0))
                ],
                rect(),
                true,
                viewport(),
                &project
            )
            .unwrap()
            .is_none());
        assert!(project.nodes.is_empty());
        let (next, id) = tool
            .events(&[button(130.0, false)], rect(), false, viewport(), &project)
            .unwrap()
            .unwrap();
        assert_eq!(id, "freehand_1");
        assert_eq!(next.nodes.len(), 1);
        assert!(!tool.is_drawing());
        tool.events(&[button(10.0, true)], rect(), true, viewport(), &project)
            .unwrap();
        tool.cancel();
        assert!(tool
            .events(&[button(30.0, false)], rect(), true, viewport(), &project)
            .unwrap()
            .is_none());
        tool.events(
            &[button(10.0, true), Event::PointerGone],
            rect(),
            true,
            viewport(),
            &project,
        )
        .unwrap();
        assert!(!tool.is_drawing());
    }
    #[test]
    fn touch_dedup_identity_cancel_and_fallback() {
        let project = Project::default();
        let mut tool = FreehandTool::default();
        tool.events(
            &[
                button(10.0, true),
                touch(1, TouchPhase::Start, 10.0, Some(0.25)),
                touch(2, TouchPhase::Start, 50.0, Some(1.0)),
                touch(2, TouchPhase::Move, 60.0, None),
            ],
            rect(),
            true,
            viewport(),
            &project,
        )
        .unwrap();
        assert_eq!(tool.samples().len(), 1);
        assert_eq!(tool.samples()[0].pressure, 0.25);
        tool.events(
            &[
                touch(1, TouchPhase::Move, 20.0, None),
                Event::PointerMoved(egui::pos2(90.0, 90.0)),
            ],
            rect(),
            true,
            viewport(),
            &project,
        )
        .unwrap();
        assert_eq!(tool.samples().len(), 2);
        assert_eq!(tool.samples()[1].pressure, 1.0);
        let (next, _) = tool
            .events(
                &[
                    button(30.0, false),
                    touch(1, TouchPhase::End, 30.0, Some(0.5)),
                ],
                rect(),
                true,
                viewport(),
                &project,
            )
            .unwrap()
            .unwrap();
        assert!(next.nodes[0].style.stroke.is_none());
        tool.events(
            &[touch(1, TouchPhase::Start, 10.0, None)],
            rect(),
            true,
            viewport(),
            &project,
        )
        .unwrap();
        assert!(tool
            .events(
                &[
                    touch(1, TouchPhase::Cancel, 20.0, None),
                    button(30.0, false)
                ],
                rect(),
                true,
                viewport(),
                &project
            )
            .unwrap()
            .is_none());
        assert!(!tool.is_drawing());
    }
    #[test]
    fn rejects_resources_invalid_geometry_and_no_partial_mutation() {
        let project = Project::default();
        let saved = project.clone();
        let points = vec![sample(0.0, 0.0), sample(10.0, 10.0)];
        assert!(stroke_candidate(&project, "bad id", &points, Default::default()).is_err());
        assert!(stroke_candidate(
            &project,
            "ink",
            &vec![sample(0.0, 0.0); MAX_POINTS + 1],
            Default::default()
        )
        .is_err());
        let mut bad = points.clone();
        bad[1].document[0] = f64::NAN;
        assert!(stroke_candidate(&project, "ink", &bad, Default::default()).is_err());
        assert!(stroke_candidate(
            &project,
            "ink",
            &points,
            FreehandOptions {
                width: f64::INFINITY,
                ..Default::default()
            }
        )
        .is_err());
        let next = stroke_candidate(&project, "ink", &points, Default::default()).unwrap();
        assert!(stroke_candidate(&next, "ink", &points, Default::default()).is_err());
        let mut invalid = project.clone();
        invalid.width = 0.0;
        assert!(stroke_candidate(&invalid, "ink", &points, Default::default()).is_err());
        assert_eq!(project, saved);
        let mut tool = FreehandTool::default();
        tool.events(&[button(10.0, true)], rect(), true, viewport(), &project)
            .unwrap();
        for i in 1..MAX_POINTS {
            tool.draft
                .as_mut()
                .unwrap()
                .push(egui::pos2(i as f32, 10.0), None)
                .unwrap();
        }
        assert!(tool
            .draft
            .as_mut()
            .unwrap()
            .push(egui::pos2(5000.0, 10.0), None)
            .is_err());
        assert_eq!(tool.samples().len(), MAX_POINTS);
        assert!(tool
            .events(
                &[],
                rect(),
                true,
                Viewport {
                    zoom: 2.0,
                    ..viewport()
                },
                &project
            )
            .is_err());
        assert!(!tool.is_drawing());
        tool.events(&[button(10.0, true)], rect(), true, viewport(), &project)
            .unwrap();
        assert!(tool
            .events(&[button(20.0, false)], rect(), true, viewport(), &next)
            .is_err());
        assert!(!tool.is_drawing());
    }
    #[test]
    fn duplicate_touch_up_retains_force_and_focus_loss_cancels() {
        let project = Project::default();
        let mut tool = FreehandTool::default();
        tool.events(
            &[
                touch(1, TouchPhase::Start, 10.0, Some(0.25)),
                touch(1, TouchPhase::Move, 20.0, Some(0.5)),
                touch(1, TouchPhase::Move, 20.0, None),
            ],
            rect(),
            true,
            viewport(),
            &project,
        )
        .unwrap();
        assert_eq!(tool.samples()[1].pressure, 0.5);
        assert!(tool.samples()[1].measured_pressure);
        tool.events(
            &[Event::WindowFocused(false)],
            rect(),
            true,
            viewport(),
            &project,
        )
        .unwrap();
        assert!(!tool.is_drawing());
    }
    #[test]
    fn public_handle_release_and_resource_error_are_atomic() {
        let ctx = egui::Context::default();
        let project = Project::default();
        let mut tool = FreehandTool::default();
        let mut canvas = Rect::NOTHING;
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (response, _) =
                    ui.allocate_painter(egui::vec2(200.0, 200.0), egui::Sense::drag());
                canvas = response.rect;
            });
        });
        let start = canvas.min + egui::vec2(20.0, 20.0);
        let end = start + egui::vec2(50.0, 30.0);
        let pointer = |pos, pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut candidate = None;
        for events in [
            vec![Event::PointerMoved(start), pointer(start, true)],
            vec![Event::PointerMoved(end)],
            vec![pointer(end, false)],
        ] {
            let _ = ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let (response, painter) =
                            ui.allocate_painter(egui::vec2(200.0, 200.0), egui::Sense::drag());
                        let result = tool
                            .handle(ui, &painter, &response, canvas.min, 2.0, &project, true)
                            .unwrap();
                        if result.is_some() {
                            candidate = result;
                        }
                    });
                },
            );
        }
        let (next, _) = candidate.unwrap();
        next.validate().unwrap();
        assert_eq!(next.nodes.len(), 1);
        assert!(
            matches!(&next.nodes[0].kind,NodeKind::Path { bounds,.. } if *bounds == [10.0,10.0,25.0,15.0])
        );
        assert!(project.nodes.is_empty());
        tool.begin(Source::Pointer, start, None, viewport(), &project)
            .unwrap();
        for i in 1..MAX_POINTS {
            tool.draft
                .as_mut()
                .unwrap()
                .push(egui::pos2(i as f32, 10.0), None)
                .unwrap();
        }
        let _ = ctx.run(
            egui::RawInput {
                events: vec![Event::PointerMoved(egui::pos2(5000.0, 10.0))],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (response, painter) =
                        ui.allocate_painter(egui::vec2(200.0, 200.0), egui::Sense::drag());
                    assert!(tool
                        .handle(ui, &painter, &response, Pos2::ZERO, 1.0, &project, true)
                        .unwrap_err()
                        .contains("Punkte begrenzt"));
                });
            },
        );
        assert!(!tool.is_drawing());
        assert!(tool
            .events(&[button(30.0, false)], rect(), true, viewport(), &project)
            .unwrap()
            .is_none());
    }
    #[test]
    fn disabled_and_escape_cancel_through_public_handle() {
        let ctx = egui::Context::default();
        let project = Project::default();
        let mut tool = FreehandTool::default();
        for escape in [false, true] {
            tool.begin(
                Source::Pointer,
                egui::pos2(10.0, 10.0),
                None,
                viewport(),
                &project,
            )
            .unwrap();
            let input = egui::RawInput {
                events: if escape {
                    vec![Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }]
                } else {
                    vec![button(20.0, false)]
                },
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (response, painter) =
                        ui.allocate_painter(egui::vec2(200.0, 200.0), egui::Sense::drag());
                    assert!(tool
                        .handle(ui, &painter, &response, Pos2::ZERO, 1.0, &project, escape)
                        .unwrap()
                        .is_none());
                });
            });
            assert!(!tool.is_drawing());
            assert!(tool
                .events(&[button(30.0, false)], rect(), true, viewport(), &project)
                .unwrap()
                .is_none());
        }
    }
}
