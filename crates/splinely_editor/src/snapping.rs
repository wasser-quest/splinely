//! Smart guides while dragging shapes: the selection's left/centre/right and
//! top/centre/bottom snap to the same lines of other visible shapes and of the
//! document. Snapped lines are drawn as guides. Alt+drag duplicates first.
use super::*;

/// Snap targets captured when a drag starts (scene coordinates).
pub(super) struct DragSnap {
    bounds: Rect,
    xs: Vec<f32>,
    ys: Vec<f32>,
}

/// One snapped guide: vertical (`x`) or horizontal (`y`) line through `value`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Guide {
    X(f32),
    Y(f32),
}

/// Smallest adjustment that moves one of `values` onto a target, if within `threshold`.
fn snap_axis(values: [f32; 3], targets: &[f32], threshold: f32) -> Option<(f32, f32)> {
    let mut best: Option<(f32, f32)> = None;
    for v in values {
        for &t in targets {
            let d = t - v;
            if d.abs() <= threshold && best.is_none_or(|(b, _)| d.abs() < b.abs()) {
                best = Some((d, t));
            }
        }
    }
    best
}

impl DragSnap {
    pub(super) fn new(
        scene: &[SceneNode],
        selection: &[String],
        document: Vec2,
        guides: &[Guide],
    ) -> Option<Self> {
        let bounds = scene
            .iter()
            .filter(|n| selection.contains(&n.id))
            .map(selection::scene_rect)
            .reduce(|a, b| a.union(b))?;
        let mut xs = vec![0., document.x / 2., document.x];
        let mut ys = vec![0., document.y / 2., document.y];
        for guide in guides {
            match *guide {
                Guide::X(x) => xs.push(x),
                Guide::Y(y) => ys.push(y),
            }
        }
        for node in scene
            .iter()
            .filter(|n| !selection.contains(&n.id) && n.opacity > 0.01)
        {
            let r = selection::scene_rect(node);
            xs.extend([r.min.x, r.center().x, r.max.x]);
            ys.extend([r.min.y, r.center().y, r.max.y]);
        }
        Some(Self { bounds, xs, ys })
    }

    /// Snapped total delta and the guides it lines up with.
    pub(super) fn apply(&self, total: Vec2, threshold: f32) -> (Vec2, Vec<Guide>) {
        let moved = self.bounds.translate(total);
        let mut result = total;
        let mut guides = vec![];
        if let Some((d, t)) = snap_axis(
            [moved.min.x, moved.center().x, moved.max.x],
            &self.xs,
            threshold,
        ) {
            result.x += d;
            guides.push(Guide::X(t));
        }
        if let Some((d, t)) = snap_axis(
            [moved.min.y, moved.center().y, moved.max.y],
            &self.ys,
            threshold,
        ) {
            result.y += d;
            guides.push(Guide::Y(t));
        }
        (result, guides)
    }
}

impl Editor {
    /// Called when a selection drag starts on the canvas.
    pub(super) fn begin_selection_drag(&mut self, scene: &[SceneNode], duplicate: bool) {
        self.playing = false;
        self.preview_animation = false;
        // Snap targets from the original selection (copies start on top of it).
        self.drag_snap = if self.smart_guides {
            let document = Vec2::new(self.project.width as f32, self.project.height as f32);
            DragSnap::new(
                scene,
                &self.editable_selection(),
                document,
                &self.document_guides(),
            )
        } else {
            None
        };
        self.drag_start = Some(self.project.clone());
        if duplicate {
            self.duplicate_in_place();
        }
        self.drag_base = Some(self.project.clone());
        self.drag_total = Vec2::ZERO;
    }

    /// Alt+drag: copy the editable selection in place (no undo step of its own;
    /// the drag that follows commits copy and move together).
    fn duplicate_in_place(&mut self) {
        let ids = self.editable_selection();
        if ids.is_empty() {
            return;
        }
        let mut next = self.project.clone();
        let mut copies: Vec<Node> = self
            .project
            .nodes
            .iter()
            .filter(|n| ids.contains(&n.id))
            .cloned()
            .collect();
        // Copies form their own groups instead of joining the originals'.
        next.adopt_group_structure(&mut copies, &self.project);
        let new_ids = next.unused_ids("node", copies.len());
        for (copy, id) in copies.iter_mut().zip(&new_ids) {
            copy.id = id.clone();
        }
        let index = self
            .project
            .nodes
            .iter()
            .rposition(|n| ids.contains(&n.id))
            .map_or(self.project.nodes.len(), |i| i + 1);
        next.nodes.splice(index..index, copies);
        if next.validate().is_ok() {
            self.project = next;
            self.set_selection(new_ids);
            self.status = "Kopie wird verschoben · Alt+Ziehen dupliziert.".into();
        }
    }

    /// Live drag update; with smart guides the whole move is recomputed from the
    /// drag start so snapping never accumulates rounding.
    pub(super) fn drag_selection_snapped(&mut self) -> bool {
        let (Some(snap), Some(base)) = (&self.drag_snap, &self.drag_base) else {
            return false;
        };
        let (total, guides) = snap.apply(self.drag_total, self.settings.snap_distance / self.zoom);
        let ids = self.editable_selection();
        let mut next = base.clone();
        if !ids.is_empty() {
            match next.translate_nodes(&ids, total.x as f64, total.y as f64) {
                Ok(()) => self.project = next,
                Err(error) => self.status = error.to_string(),
            }
        }
        self.guides = guides;
        self.pointer_hint = Some(format!("Δx {:.1}   Δy {:.1}", total.x, total.y));
        true
    }

    /// Small label next to the pointer (move distance, scale, angle).
    pub(super) fn paint_pointer_hint(&self, ui: &egui::Ui, painter: &egui::Painter) {
        let Some(hint) = &self.pointer_hint else {
            return;
        };
        let Some(p) = ui.input(|i| i.pointer.latest_pos()) else {
            return;
        };
        let galley =
            painter.layout_no_wrap(hint.clone(), egui::FontId::monospace(11.), Color32::WHITE);
        let rect = Rect::from_min_size(p + Vec2::new(14., 14.), galley.size()).expand(4.);
        painter.rect_filled(rect, 4., Color32::from_black_alpha(200));
        painter.galley(rect.min + Vec2::splat(4.), galley, Color32::WHITE);
    }

    pub(super) fn paint_guides(&self, painter: &egui::Painter, origin: Pos2, clip: Rect) {
        let color = Color32::from_rgb(230, 70, 160);
        for guide in &self.guides {
            let (a, b) = match *guide {
                Guide::X(x) => {
                    let sx = origin.x + x * self.zoom;
                    (Pos2::new(sx, clip.min.y), Pos2::new(sx, clip.max.y))
                }
                Guide::Y(y) => {
                    let sy = origin.y + y * self.zoom;
                    (Pos2::new(clip.min.x, sy), Pos2::new(clip.max.x, sy))
                }
            };
            painter.line_segment([a, b], egui::Stroke::new(1_f32, color));
        }
    }
}

impl Guide {
    fn from_line(line: &GuideLine) -> Self {
        if line.vertical {
            Guide::X(line.position as f32)
        } else {
            Guide::Y(line.position as f32)
        }
    }
    fn to_line(self) -> GuideLine {
        match self {
            Guide::X(x) => GuideLine {
                vertical: true,
                position: x as f64,
            },
            Guide::Y(y) => GuideLine {
                vertical: false,
                position: y as f64,
            },
        }
    }
}

/// Distance in screen pixels within which an existing guide can be grabbed.
const GUIDE_GRAB: f32 = 4.;

impl Editor {
    /// Ruler guides: drag from the top ruler for a horizontal guide, from the
    /// left ruler for a vertical one; drag a guide back onto a ruler to delete
    /// it. Returns true while a guide gesture owns the pointer.
    pub(super) fn guide_gesture(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        viewport: Rect,
        content: Rect,
        origin: Pos2,
    ) -> bool {
        let zoom = self.zoom;
        let to_doc = |p: Pos2| (p - origin) / zoom;
        let screen_of = |g: Guide| match g {
            Guide::X(x) => origin.x + x * zoom,
            Guide::Y(y) => origin.y + y * zoom,
        };
        let editing = self.tab == Tab::Canvas && !self.preview_mode && self.show_rulers;
        let near_guide = |p: Pos2, guides: &[Guide]| {
            guides.iter().position(|&g| match g {
                Guide::X(_) => (screen_of(g) - p.x).abs() <= GUIDE_GRAB,
                Guide::Y(_) => (screen_of(g) - p.y).abs() <= GUIDE_GRAB,
            })
        };
        let guides = self.document_guides();
        if self.guide_drag.is_none() && editing {
            if let Some(hover) = response.hover_pos().filter(|p| content.contains(*p)) {
                if let Some(i) = near_guide(hover, &guides) {
                    ui.ctx().set_cursor_icon(match guides[i] {
                        Guide::X(_) => egui::CursorIcon::ResizeHorizontal,
                        Guide::Y(_) => egui::CursorIcon::ResizeVertical,
                    });
                }
            }
            if response.drag_started() {
                if let Some(press) = ui.input(|i| i.pointer.press_origin()) {
                    let d = to_doc(press);
                    self.guide_drag = match rulers::ruler_hit_test(viewport, press) {
                        Some(rulers::RulerHit::Top) => Some((None, Guide::Y(d.y))),
                        Some(rulers::RulerHit::Left) => Some((None, Guide::X(d.x))),
                        None if self.canvas_tool() == CanvasTool::Selection => {
                            near_guide(press, &guides).map(|i| (Some(i), guides[i]))
                        }
                        _ => None,
                    };
                }
            }
        }
        let Some((index, guide)) = self.guide_drag else {
            return false;
        };
        let pointer = response.interact_pointer_pos().or(response.hover_pos());
        let mut guide = guide;
        if let Some(p) = pointer {
            let d = to_doc(p);
            guide = match guide {
                Guide::X(_) => Guide::X(d.x.round()),
                Guide::Y(_) => Guide::Y(d.y.round()),
            };
            self.guide_drag = Some((index, guide));
            ui.ctx().set_cursor_icon(match guide {
                Guide::X(_) => egui::CursorIcon::ResizeHorizontal,
                Guide::Y(_) => egui::CursorIcon::ResizeVertical,
            });
            self.status = match guide {
                Guide::X(x) => format!("Hilfslinie x = {x} · zurück aufs Lineal ziehen löscht sie"),
                Guide::Y(y) => format!("Hilfslinie y = {y} · zurück aufs Lineal ziehen löscht sie"),
            };
        }
        if response.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
            let on_canvas = pointer.is_some_and(|p| content.contains(p));
            // Guides are saved in the project: every change is one undo step.
            let mut next = self.project.clone();
            match (index, on_canvas) {
                (Some(i), true) => next.guides[i] = guide.to_line(),
                (Some(i), false) => {
                    next.guides.remove(i);
                    self.status = "Hilfslinie entfernt.".into();
                }
                (None, true) => next.guides.push(guide.to_line()),
                (None, false) => {}
            }
            self.guide_drag = None;
            self.commit(next);
        }
        true
    }

    pub(super) fn document_guides(&self) -> Vec<Guide> {
        self.project.guides.iter().map(Guide::from_line).collect()
    }

    pub(super) fn paint_user_guides(&self, painter: &egui::Painter, origin: Pos2, clip: Rect) {
        let color = Color32::from_rgb(40, 190, 230);
        let dragged = self.guide_drag.map(|(_, g)| g);
        let moving = self.guide_drag.and_then(|(i, _)| i);
        let all = self.document_guides();
        let guides = all
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != moving)
            .map(|(_, g)| *g)
            .chain(dragged);
        for guide in guides {
            let (a, b) = match guide {
                Guide::X(x) => {
                    let sx = origin.x + x * self.zoom;
                    (Pos2::new(sx, clip.min.y), Pos2::new(sx, clip.max.y))
                }
                Guide::Y(y) => {
                    let sy = origin.y + y * self.zoom;
                    (Pos2::new(clip.min.x, sy), Pos2::new(clip.max.x, sy))
                }
            };
            painter.line_segment([a, b], egui::Stroke::new(1_f32, color.gamma_multiply(0.8)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_the_nearest_edge_or_centre_within_the_threshold() {
        let snap = DragSnap {
            bounds: Rect::from_min_max(Pos2::new(0., 0.), Pos2::new(10., 10.)),
            xs: vec![100.],
            ys: vec![53.],
        };
        // Right edge 10+88=98 is 2 away from 100; the bottom edge 55 is closer to
        // 53 (2) than the centre 50 (3), so the bottom snaps.
        let (total, guides) = snap.apply(Vec2::new(88., 45.), 4.);
        assert_eq!(total, Vec2::new(90., 43.));
        assert_eq!(guides, vec![Guide::X(100.), Guide::Y(53.)]);
        let (total, guides) = snap.apply(Vec2::new(70., 20.), 4.);
        assert_eq!(total, Vec2::new(70., 20.));
        assert!(guides.is_empty());
    }

    #[test]
    fn snapped_drag_aligns_to_another_shape_and_is_one_undo_step() {
        let mut editor = Editor::default();
        editor.project.nodes = vec![
            Node::rect("a", 0., 0., 20., 20.),
            Node::rect("b", 100., 50., 20., 20.),
        ];
        editor.project.clips.clear();
        editor.project.state_machines.clear();
        editor.sync_buffers();
        editor.set_selection(vec!["a".into()]);
        let scene = editor.evaluate_scene(&EvaluationContext::default());
        let before = editor.project.clone();
        editor.begin_selection_drag(&scene, false);
        // Close to b's left edge (x=100) and top (y=50), not exactly on it.
        editor.drag_by(Vec2::new(60., 30.));
        editor.drag_by(Vec2::new(38., 22.));
        let t = editor.project.nodes[0].transform;
        assert_eq!((t.x, t.y), (100., 50.));
        assert_eq!(editor.guides.len(), 2);
        editor.finish_drag();
        assert!(editor.guides.is_empty());
        editor.history.undo(&mut editor.project);
        assert_eq!(editor.project, before);
    }

    #[test]
    fn user_guides_are_snap_targets() {
        let scene = vec![];
        let selection: Vec<String> = vec![];
        assert!(
            DragSnap::new(&scene, &selection, Vec2::new(100., 100.), &[Guide::X(42.)]).is_none()
        );
        let mut editor = Editor::default();
        editor.project.nodes = vec![Node::rect("a", 0., 0., 10., 10.)];
        editor.project.clips.clear();
        editor.project.state_machines.clear();
        editor.sync_buffers();
        editor.project.guides = vec![GuideLine {
            vertical: true,
            position: 42.,
        }];
        editor.set_selection(vec!["a".into()]);
        let scene = editor.evaluate_scene(&EvaluationContext::default());
        editor.begin_selection_drag(&scene, false);
        editor.drag_by(Vec2::new(30., 0.)); // right edge at 40, guide at 42
        editor.finish_drag();
        assert_eq!(editor.project.nodes[0].transform.x, 32.);
    }

    #[test]
    fn alt_drag_duplicates_and_moves_the_copy_in_one_step() {
        let mut editor = Editor::default();
        editor.project.nodes = vec![Node::rect("a", 0., 0., 20., 20.)];
        editor.project.clips.clear();
        editor.project.state_machines.clear();
        editor.sync_buffers();
        editor.smart_guides = false;
        editor.set_selection(vec!["a".into()]);
        let scene = editor.evaluate_scene(&EvaluationContext::default());
        editor.begin_selection_drag(&scene, true);
        editor.drag_by(Vec2::new(50., 0.));
        editor.finish_drag();
        assert_eq!(editor.project.nodes.len(), 2);
        assert_eq!(editor.project.nodes[0].transform.x, 0.);
        assert_eq!(editor.project.nodes[1].transform.x, 50.);
        editor.history.undo(&mut editor.project);
        assert_eq!(editor.project.nodes.len(), 1);
    }
}
