//! On-canvas transform handles for the selection: eight scale handles and a
//! rotation handle. A gesture previews live on a copy of the start document and
//! commits once on release (one undo step). Shift keeps proportions / snaps the
//! angle to 15°, Alt scales around the centre.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum HandleKind {
    /// Which edges move: -1 = min side, 1 = max side, 0 = unchanged axis.
    Scale {
        fx: i8,
        fy: i8,
    },
    Rotate,
}

pub(super) struct TransformGesture {
    kind: HandleKind,
    start: Project,
    ids: Vec<String>,
    bounds: Rect,
    start_pointer: Vec2,
    uniform_only: bool,
}

const HANDLES: [(i8, i8); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
];

fn handle_point(bounds: Rect, fx: i8, fy: i8) -> Pos2 {
    let x = match fx {
        -1 => bounds.min.x,
        1 => bounds.max.x,
        _ => bounds.center().x,
    };
    let y = match fy {
        -1 => bounds.min.y,
        1 => bounds.max.y,
        _ => bounds.center().y,
    };
    Pos2::new(x, y)
}

fn cursor(kind: HandleKind) -> egui::CursorIcon {
    match kind {
        HandleKind::Rotate => egui::CursorIcon::Alias,
        HandleKind::Scale { fx: 0, .. } => egui::CursorIcon::ResizeVertical,
        HandleKind::Scale { fy: 0, .. } => egui::CursorIcon::ResizeHorizontal,
        HandleKind::Scale { fx, fy } if fx == fy => egui::CursorIcon::ResizeNwSe,
        HandleKind::Scale { .. } => egui::CursorIcon::ResizeNeSw,
    }
}

/// Scale factors for a handle drag in scene coordinates.
pub(super) fn scale_factors(
    bounds: Rect,
    fx: i8,
    fy: i8,
    pointer: Vec2,
    uniform: bool,
    from_center: bool,
) -> ((f32, f32), Pos2) {
    let anchor = if from_center {
        bounds.center()
    } else {
        handle_point(bounds, -fx, -fy)
    };
    let handle = handle_point(bounds, fx, fy);
    let factor = |moving: f32, anchor: f32, pointer: f32| {
        let span = moving - anchor;
        if span.abs() < 1e-6 {
            1.
        } else {
            let f = (pointer - anchor) / span;
            // Keep a minimum size; passing the anchor mirrors.
            if f.abs() < 0.01 {
                0.01_f32.copysign(f)
            } else {
                f
            }
        }
    };
    let mut sx = if fx == 0 {
        1.
    } else {
        factor(handle.x, anchor.x, pointer.x)
    };
    let mut sy = if fy == 0 {
        1.
    } else {
        factor(handle.y, anchor.y, pointer.y)
    };
    if uniform {
        let s = match (fx, fy) {
            (0, _) => sy,
            (_, 0) => sx,
            _ if (sx.abs() - 1.).abs() > (sy.abs() - 1.).abs() => sx,
            _ => sy,
        };
        sx = s;
        sy = s;
    }
    ((sx, sy), anchor)
}

impl Editor {
    /// Draws handles and runs a handle gesture. Returns true if this frame's
    /// pointer interaction belongs to the handles (the caller then skips picking).
    pub(super) fn transform_handles(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        painter: &egui::Painter,
        origin: Pos2,
        scene: &[SceneNode],
    ) -> bool {
        let to_scene = |p: Pos2| (p - origin) / self.zoom;
        if let Some(gesture) = &self.transform_gesture {
            ui.ctx().set_cursor_icon(cursor(gesture.kind));
            if response.dragged() {
                if let Some(p) = response.interact_pointer_pos() {
                    self.update_transform(to_scene(p), ui.input(|i| i.modifiers));
                }
            }
            if response.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
                self.finish_transform();
            }
            self.paint_handles(painter, origin, scene);
            return true;
        }
        if self.tab != Tab::Canvas
            || self.preview_mode
            || self.playing
            || self.preview_animation
            || self.canvas_tool() != CanvasTool::Selection
        {
            return false;
        }
        let Some((bounds, uniform_only)) = self.handle_bounds(scene) else {
            return false;
        };
        let screen = |p: Pos2| origin + p.to_vec2() * self.zoom;
        let hover = response.hover_pos();
        let press = if response.drag_started() {
            ui.input(|i| i.pointer.press_origin())
        } else {
            None
        };
        let mut handles: Vec<(HandleKind, Pos2)> = HANDLES
            .iter()
            .filter(|(fx, fy)| !uniform_only || (*fx != 0 && *fy != 0))
            .map(|&(fx, fy)| {
                (
                    HandleKind::Scale { fx, fy },
                    screen(handle_point(bounds, fx, fy)),
                )
            })
            .collect();
        let top = screen(Pos2::new(bounds.center().x, bounds.min.y));
        handles.push((HandleKind::Rotate, top - Vec2::new(0., 26.)));
        let near = |p: Pos2| {
            handles
                .iter()
                .filter(|(_, h)| h.distance(p) <= 8.)
                .min_by(|a, b| a.1.distance(p).total_cmp(&b.1.distance(p)))
                .map(|(k, _)| *k)
        };
        if let Some(kind) = hover.and_then(near) {
            ui.ctx().set_cursor_icon(cursor(kind));
        }
        self.paint_handles(painter, origin, scene);
        if let Some(kind) = press.and_then(near) {
            let start_pointer = to_scene(press.unwrap());
            self.transform_gesture = Some(TransformGesture {
                kind,
                start: self.project.clone(),
                ids: self.editable_selection(),
                bounds,
                start_pointer,
                uniform_only,
            });
            return true;
        }
        false
    }

    /// Scene bounds of the editable selection and whether only uniform scaling
    /// is possible (some shape is rotated by a non-multiple of 90°).
    fn handle_bounds(&self, scene: &[SceneNode]) -> Option<(Rect, bool)> {
        let ids = self.editable_selection();
        let nodes: Vec<&SceneNode> = scene.iter().filter(|n| ids.contains(&n.id)).collect();
        let bounds = nodes
            .iter()
            .map(|n| selection::scene_rect(n))
            .reduce(|a, b| a.union(b))?;
        let uniform_only = nodes.iter().any(|n| {
            let q = n.rotation.rem_euclid(90.);
            q > 1e-3 && 90. - q > 1e-3
        });
        Some((bounds, uniform_only))
    }

    fn paint_handles(&self, painter: &egui::Painter, origin: Pos2, scene: &[SceneNode]) {
        let Some((bounds, uniform_only)) = self.handle_bounds(scene) else {
            return;
        };
        let accent = Color32::from_rgb(42, 160, 180);
        let screen = |p: Pos2| origin + p.to_vec2() * self.zoom;
        let rect = Rect::from_two_pos(screen(bounds.min), screen(bounds.max));
        painter.rect_stroke(
            rect,
            0.,
            egui::Stroke::new(1_f32, accent),
            egui::StrokeKind::Middle,
        );
        let top = Pos2::new(rect.center().x, rect.min.y);
        let knob = top - Vec2::new(0., 26.);
        painter.line_segment([top, knob], egui::Stroke::new(1_f32, accent));
        painter.circle(knob, 5., Color32::WHITE, egui::Stroke::new(1.5_f32, accent));
        for &(fx, fy) in &HANDLES {
            if uniform_only && (fx == 0 || fy == 0) {
                continue;
            }
            let p = screen(handle_point(bounds, fx, fy));
            painter.rect(
                Rect::from_center_size(p, Vec2::splat(8.)),
                1.,
                Color32::WHITE,
                egui::Stroke::new(1.5_f32, accent),
                egui::StrokeKind::Middle,
            );
        }
    }

    fn update_transform(&mut self, pointer: Vec2, modifiers: egui::Modifiers) {
        let Some(gesture) = &self.transform_gesture else {
            return;
        };
        let mut next = gesture.start.clone();
        let result = match gesture.kind {
            HandleKind::Scale { fx, fy } => {
                let ((sx, sy), anchor) = scale_factors(
                    gesture.bounds,
                    fx,
                    fy,
                    pointer,
                    modifiers.shift || gesture.uniform_only,
                    modifiers.alt,
                );
                self.status = tr("Skalieren · Umschalt: proportional · Alt: von der Mitte").into();
                self.pointer_hint = Some(format!("{:.0} % × {:.0} %", sx * 100., sy * 100.));
                next.scale_nodes(
                    &gesture.ids,
                    sx as f64,
                    sy as f64,
                    (anchor.x as f64, anchor.y as f64),
                )
            }
            HandleKind::Rotate => {
                let c = gesture.bounds.center().to_vec2();
                let a0 = (gesture.start_pointer - c).angle();
                let a1 = (pointer - c).angle();
                let mut degrees = (a1 - a0).to_degrees() as f64;
                if modifiers.shift {
                    degrees = (degrees / 15.).round() * 15.;
                }
                self.status = tr("Drehen · Umschalt: 15°-Schritte").into();
                self.pointer_hint = Some(trf("{degrees:.1}°", &[format!("{degrees:.1}")]));
                next.rotate_nodes(&gesture.ids, degrees, (c.x as f64, c.y as f64))
            }
        };
        match result {
            Ok(()) => self.project = next,
            Err(error) => self.status = error.to_string(),
        }
    }

    fn finish_transform(&mut self) {
        self.pointer_hint = None;
        if let Some(gesture) = self.transform_gesture.take() {
            let after = std::mem::replace(&mut self.project, gesture.start);
            self.commit(after);
        }
    }

    /// Abandon a running handle gesture without changing the document.
    pub(super) fn cancel_transform(&mut self) {
        self.pointer_hint = None;
        if let Some(gesture) = self.transform_gesture.take() {
            self.project = gesture.start;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> Rect {
        Rect::from_min_max(Pos2::new(10., 10.), Pos2::new(110., 60.))
    }

    #[test]
    fn corner_drag_scales_from_the_opposite_corner() {
        let ((sx, sy), anchor) = scale_factors(bounds(), 1, 1, Vec2::new(210., 110.), false, false);
        assert_eq!(anchor, Pos2::new(10., 10.));
        assert_eq!((sx, sy), (2., 2.));
        let ((sx, sy), _) = scale_factors(bounds(), 1, 1, Vec2::new(210., 35.), true, false);
        assert_eq!((sx, sy), (2., 2.));
    }

    #[test]
    fn edge_drag_changes_one_axis_and_alt_uses_the_centre() {
        let ((sx, sy), _) = scale_factors(bounds(), 0, -1, Vec2::new(500., -40.), false, false);
        assert_eq!((sx, sy), (1., 2.));
        let ((sx, _), anchor) = scale_factors(bounds(), 1, 0, Vec2::new(160., 0.), false, true);
        assert_eq!(anchor, bounds().center());
        assert_eq!(sx, 2.);
        // Crossing the anchor mirrors but never collapses to zero.
        let ((sx, _), _) = scale_factors(bounds(), 1, 0, Vec2::new(10., 0.), false, false);
        assert_eq!(sx, 0.01);
    }

    #[test]
    fn handle_gesture_is_one_undo_step() {
        let mut editor = Editor::default();
        let id = editor
            .project
            .nodes
            .iter()
            .find(|n| !n.locked)
            .unwrap()
            .id
            .clone();
        editor.set_selection(vec![id.clone()]);
        let scene = editor.evaluate_scene(&EvaluationContext::default());
        let (bounds, _) = editor.handle_bounds(&scene).unwrap();
        editor.transform_gesture = Some(TransformGesture {
            kind: HandleKind::Scale { fx: 1, fy: 1 },
            start: editor.project.clone(),
            ids: vec![id.clone()],
            bounds,
            start_pointer: bounds.max.to_vec2(),
            uniform_only: false,
        });
        let before = editor.project.clone();
        let target = bounds.min.to_vec2() + (bounds.max - bounds.min) * 2.;
        editor.update_transform(target, egui::Modifiers::NONE);
        editor.update_transform(target, egui::Modifiers::NONE);
        let scaled = editor
            .project
            .nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap()
            .transform;
        assert_eq!((scaled.scale_x, scaled.scale_y), (2., 2.));
        editor.finish_transform();
        editor.history.undo(&mut editor.project);
        assert_eq!(editor.project, before);
    }
}
