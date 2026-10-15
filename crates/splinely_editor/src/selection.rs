//! Multi-selection, rubber-band (marquee) selection, groups and the arrange panel.
//!
//! `selected` stays the primary shape (inspector, path tools, bones); `multi`
//! holds the additional shapes. Code that assigns `selected` directly gets a
//! single selection again on the next frame (see `sync_selection`).
use super::*;
use splinely_document::{AlignEdge, AlignTarget, Axis, BooleanOp, Stack};

/// A pending arrange command from the inspector, applied after its own edits.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum ArrangeOp {
    Align(AlignEdge),
    Distribute(Axis),
    Flip(Axis),
    Stack(Stack),
    Group,
    Ungroup,
    Boolean(BooleanOp),
    /// New scene bounds `[x, y, width, height]` for the whole selection.
    SetBounds([f64; 4], bool),
    /// Morph the primary shape into the other selected shape.
    Morph,
}

/// Marquee gesture in document coordinates.
#[derive(Clone, Copy, Debug)]
pub(super) struct Marquee {
    pub start: Vec2,
    pub end: Vec2,
    pub additive: bool,
}

/// Scene-space outline used for selection drawing and marquee hits.
pub(super) fn outline(n: &SceneNode) -> Vec<Vec2> {
    let local = match &n.shape {
        Shape::Path { bounds: b, .. } => vec![
            Vec2::new(b[0], b[1]),
            Vec2::new(b[0] + b[2], b[1]),
            Vec2::new(b[0] + b[2], b[1] + b[3]),
            Vec2::new(b[0], b[1] + b[3]),
        ],
        Shape::Rect { width, height, .. } => vec![
            Vec2::ZERO,
            Vec2::new(*width, 0.),
            Vec2::new(*width, *height),
            Vec2::new(0., *height),
        ],
        Shape::Circle { radius } => (0..32)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 32.;
                Vec2::new(a.cos() * radius, a.sin() * radius)
            })
            .collect(),
        Shape::Text { text, size } => {
            let w = text.chars().count() as f32 * size * 0.6;
            vec![
                Vec2::new(0., -*size),
                Vec2::new(w, -*size),
                Vec2::new(w, size * 0.2),
                Vec2::new(0., size * 0.2),
            ]
        }
    };
    local.into_iter().map(|p| local_to_scene(n, p)).collect()
}

pub(super) fn scene_rect(n: &SceneNode) -> Rect {
    let points: Vec<Pos2> = outline(n).into_iter().map(|p| p.to_pos2()).collect();
    Rect::from_points(&points)
}

impl Editor {
    /// Primary first, then the others; only existing nodes, no duplicates.
    pub(super) fn selection(&self) -> Vec<String> {
        let mut ids: Vec<String> = vec![];
        if let Some(primary) = &self.selected {
            ids.push(primary.clone());
            for id in &self.multi {
                if !ids.contains(id) {
                    ids.push(id.clone());
                }
            }
        }
        ids.retain(|id| self.project.nodes.iter().any(|n| &n.id == id));
        ids
    }

    pub(super) fn is_selected(&self, id: &str) -> bool {
        self.selected.as_deref() == Some(id)
            || (self.selected.is_some() && self.multi.iter().any(|m| m == id))
    }

    /// Selected nodes that may be edited (not locked), in document order.
    pub(super) fn editable_selection(&self) -> Vec<String> {
        let selection = self.selection();
        self.project
            .nodes
            .iter()
            .filter(|n| !n.locked && selection.contains(&n.id))
            .map(|n| n.id.clone())
            .collect()
    }

    pub(super) fn set_selection(&mut self, ids: Vec<String>) {
        // A shape selection takes Delete away from the timeline key.
        self.selected_key = None;
        let mut ids = ids.into_iter();
        self.selected = ids.next();
        self.multi = ids.collect();
        self.last_primary = self.selected.clone();
    }

    /// Direct assignments to `selected` (tools, panels) mean "select just this".
    pub(super) fn sync_selection(&mut self) {
        if self.selected != self.last_primary {
            self.multi.clear();
            self.selected_key = None;
            self.last_primary = self.selected.clone();
        }
        if self.selected.is_none() {
            self.multi.clear();
        }
    }

    /// Canvas click on a shape. `toggle` (Shift) adds/removes; `isolate` (Ctrl)
    /// ignores the shape's group; dragging an already selected shape keeps the set.
    pub(super) fn pick_shape(&mut self, id: &str, toggle: bool, isolate: bool, dragging: bool) {
        let members = if isolate {
            vec![id.to_owned()]
        } else {
            self.project.group_members(id)
        };
        let mut current = self.selection();
        if toggle {
            if current.iter().any(|c| c == id) {
                current.retain(|c| !members.contains(c));
            } else {
                // The clicked shape becomes primary so the inspector follows it.
                current.retain(|c| !members.contains(c));
                let mut next = vec![id.to_owned()];
                next.extend(members.into_iter().filter(|m| m != id));
                next.extend(current);
                current = next;
            }
            self.set_selection(current);
        } else if !(dragging && self.is_selected(id)) {
            let mut next = vec![id.to_owned()];
            next.extend(members.into_iter().filter(|m| m != id));
            self.set_selection(next);
        }
    }

    pub(super) fn select_all(&mut self) {
        let ids = self
            .project
            .nodes
            .iter()
            .rev()
            .filter(|n| n.visible && !n.locked)
            .map(|n| n.id.clone())
            .collect();
        self.set_selection(ids);
    }

    /// Finish a marquee: every visible, unlocked shape touching the rectangle.
    pub(super) fn finish_marquee(&mut self, marquee: Marquee, scene: &[SceneNode]) {
        let area = Rect::from_two_pos(marquee.start.to_pos2(), marquee.end.to_pos2());
        let mut hits: Vec<String> = vec![];
        for node in scene.iter().rev() {
            let editable = self
                .project
                .nodes
                .iter()
                .any(|n| n.id == node.id && n.visible && !n.locked);
            if editable && node.opacity > 0.01 && scene_rect(node).intersects(area) {
                for member in self.project.group_members(&node.id) {
                    if !hits.contains(&member) {
                        hits.push(member);
                    }
                }
            }
        }
        let mut next = if marquee.additive {
            self.selection()
        } else {
            vec![]
        };
        for id in hits {
            if !next.contains(&id) {
                next.push(id);
            }
        }
        let count = next.len();
        self.set_selection(next);
        if count > 1 {
            self.status = trf(
                "{count} Formen ausgewählt · Anordnen rechts, Strg+G gruppiert.",
                &[format!("{count}")],
            );
        }
    }

    pub(super) fn paint_marquee(&self, painter: &egui::Painter, origin: Pos2, marquee: &Marquee) {
        let rect = Rect::from_two_pos(
            origin + marquee.start * self.zoom,
            origin + marquee.end * self.zoom,
        );
        let accent = Color32::from_rgb(42, 160, 180);
        painter.rect_filled(rect, 0., accent.gamma_multiply(0.12));
        painter.rect_stroke(
            rect,
            0.,
            egui::Stroke::new(1_f32, accent),
            egui::StrokeKind::Inside,
        );
    }

    /// Union box around a multi-selection.
    pub(super) fn paint_selection_bounds(
        &self,
        painter: &egui::Painter,
        origin: Pos2,
        scene: &[SceneNode],
    ) {
        let selection = self.selection();
        if selection.len() < 2 {
            return;
        }
        let rects: Vec<Rect> = scene
            .iter()
            .filter(|n| selection.contains(&n.id))
            .map(scene_rect)
            .collect();
        let Some(union) = rects.iter().copied().reduce(|a, b| a.union(b)) else {
            return;
        };
        let rect = Rect::from_min_max(
            origin + union.min.to_vec2() * self.zoom,
            origin + union.max.to_vec2() * self.zoom,
        )
        .expand(6.);
        let accent = Color32::from_rgb(42, 160, 180);
        for (a, b) in [
            (rect.left_top(), rect.right_top()),
            (rect.right_top(), rect.right_bottom()),
            (rect.right_bottom(), rect.left_bottom()),
            (rect.left_bottom(), rect.left_top()),
        ] {
            painter.add(egui::Shape::dashed_line(
                &[a, b],
                egui::Stroke::new(1_f32, accent),
                6.,
                4.,
            ));
        }
    }

    /// Move every editable selected shape by a scene delta (live drag).
    pub(super) fn drag_selection(&mut self, delta: Vec2) {
        let ids = self.editable_selection();
        if ids.is_empty() {
            return;
        }
        if let Err(error) = self
            .project
            .translate_nodes(&ids, delta.x as f64, delta.y as f64)
        {
            self.status = error.to_string();
        }
    }

    pub(super) fn apply_arrange(&mut self, op: ArrangeOp) {
        let ids = self.editable_selection();
        if ids.is_empty() {
            self.status =
                tr("Nichts Bearbeitbares ausgewählt (gesperrte Ebenen bleiben unverändert).")
                    .into();
            return;
        }
        if let ArrangeOp::Boolean(boolean) = op {
            if !self.machine_source_clean() {
                self.status =
                    tr("Zuerst vorgemerktes Zustands-JSON übernehmen oder neu laden.").into();
                return;
            }
            let mut next = self.project.clone();
            match next.boolean_nodes(&ids, boolean) {
                Ok(id) => {
                    self.commit(next);
                    self.set_selection(vec![id]);
                    self.machine_buffer =
                        serde_json::to_string_pretty(&self.project.state_machines)
                            .unwrap_or_default();
                    self.status = trf(
                        "{} Formen zu einem Pfad kombiniert · Strg+Z macht es rückgängig.",
                        &[format!("{}", ids.len())],
                    );
                }
                Err(error) => self.status = error.to_string(),
            }
            return;
        }
        if op == ArrangeOp::Morph {
            let selection = self.selection();
            let (Some(source), Some(target)) = (selection.first(), selection.get(1)) else {
                return;
            };
            if selection.len() != 2 {
                self.status = tr("Für einen Morph genau zwei Formen wählen.").into();
                return;
            }
            let mut next = self.project.clone();
            match next.create_morph(source, target, 1.) {
                Ok(clip) => {
                    let source = source.clone();
                    self.commit(next);
                    self.active_clip = self
                        .project
                        .clips
                        .iter()
                        .position(|c| c.id == clip)
                        .unwrap_or(0);
                    self.timeline_visible = true;
                    self.time = 0.;
                    self.set_selection(vec![source]);
                    self.status = tr(
                        "Morph-Clip erstellt (1 s) · Leertaste spielt ab · Zielform ausgeblendet.",
                    )
                    .into();
                }
                Err(error) => self.status = error.to_string(),
            }
            return;
        }
        if let ArrangeOp::SetBounds(target, proportional) = op {
            let mut next = self.project.clone();
            let result = next.bounds_of(&ids).and_then(|b| {
                let (mut sx, mut sy) = (target[2] / b[2].max(1e-9), target[3] / b[3].max(1e-9));
                if proportional {
                    // The field that changed wins.
                    let s = if (sx - 1.).abs() >= (sy - 1.).abs() {
                        sx
                    } else {
                        sy
                    };
                    (sx, sy) = (s, s);
                }
                if (sx - 1.).abs() > 1e-12 || (sy - 1.).abs() > 1e-12 {
                    next.scale_nodes(&ids, sx, sy, (b[0], b[1]))?;
                }
                next.translate_nodes(&ids, target[0] - b[0], target[1] - b[1])
            });
            match result {
                Ok(()) => self.commit(next),
                Err(error) => self.status = error.to_string(),
            }
            return;
        }
        let mut next = self.project.clone();
        let result = match &op {
            ArrangeOp::Align(edge) => next.align_nodes(&ids, *edge, AlignTarget::Selection),
            ArrangeOp::Distribute(axis) => next.distribute_nodes(&ids, *axis),
            ArrangeOp::Flip(axis) => next.flip_nodes(&ids, *axis),
            ArrangeOp::Stack(stack) => next.restack_nodes(&ids, *stack),
            ArrangeOp::Group => next.group_nodes(&ids, None).map(|_| ()),
            ArrangeOp::Ungroup => next.ungroup_nodes(&ids),
            ArrangeOp::Boolean(_) | ArrangeOp::SetBounds(..) | ArrangeOp::Morph => {
                unreachable!("handled above")
            }
        };
        match result {
            Ok(()) => {
                self.commit(next);
                self.status = match op {
                    ArrangeOp::Group => trf(
                        "{} Formen gruppiert · Strg+Umschalt+G löst die Gruppe.",
                        &[format!("{}", ids.len())],
                    ),
                    ArrangeOp::Ungroup => tr("Gruppe aufgelöst.").into(),
                    ArrangeOp::Align(_) => tr("Ausgerichtet.").into(),
                    ArrangeOp::Distribute(_) => tr("Gleichmäßig verteilt.").into(),
                    ArrangeOp::Flip(_) => tr("Gespiegelt.").into(),
                    ArrangeOp::Stack(_) => tr("Reihenfolge geändert.").into(),
                    ArrangeOp::Boolean(_) | ArrangeOp::SetBounds(..) | ArrangeOp::Morph => {
                        unreachable!("handled above")
                    }
                };
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    /// Inspector panel; returns the clicked operation (applied by the caller).
    pub(super) fn arrange_ui(&self, ui: &mut egui::Ui) -> Option<ArrangeOp> {
        let selection = self.selection();
        if selection.is_empty() {
            return None;
        }
        let editable = self.editable_selection().len();
        let mut op = None;
        let count = selection.len();
        let title = if count > 1 {
            trf("Anordnen · {count} Formen", &[format!("{count}")])
        } else {
            tr("Anordnen").into()
        };
        // Single shapes have their own position fields; keep the panel compact.
        egui::CollapsingHeader::new(title)
            .id_salt(if count > 1 { "arrange_panel_multi" } else { "arrange_panel_single" })
            .default_open(count > 1)
            .show(ui, |ui| {
                ui.add_enabled_ui(editable > 0, |ui| {
                    let ids = self.editable_selection();
                    if let Ok(b) = self.project.bounds_of(&ids) {
                        let mut values = b;
                        let mut changed = false;
                        egui::Grid::new("selection_bounds").num_columns(4).show(ui, |ui| {
                            for (row, labels) in [(0, ["X", "Y"]), (2, ["B", "H"])] {
                                for (i, label) in labels.iter().enumerate() {
                                    ui.label(*label);
                                    let mut drag = egui::DragValue::new(&mut values[row + i])
                                        .speed(1.)
                                        .max_decimals(2);
                                    if row == 2 {
                                        drag = drag.range(0.01..=1e6);
                                    }
                                    changed |= ui.add(drag).changed();
                                }
                                ui.end_row();
                            }
                        });
                        let id = ui.make_persistent_id("bounds_proportional");
                        let mut proportional = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(true);
                        if ui
                            .checkbox(&mut proportional, tr("Proportional"))
                            .on_hover_text(tr("Breite und Höhe gemeinsam ändern. Schräg gedrehte Formen lassen sich nur proportional skalieren."))
                            .changed()
                        {
                            ui.data_mut(|d| d.insert_temp(id, proportional));
                        }
                        if changed && values != b {
                            op = Some(ArrangeOp::SetBounds(values, proportional));
                        }
                    }
                    let target = if count > 1 { tr("an der Auswahl") } else { tr("an der Zeichenfläche") };
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.;
                        for (icon, edge, hint) in [
                            (Icon::Align(false, 0.), AlignEdge::Left, tr("Links ausrichten")),
                            (Icon::Align(false, 0.5), AlignEdge::HorizontalCenter, tr("Horizontal zentrieren")),
                            (Icon::Align(false, 1.), AlignEdge::Right, tr("Rechts ausrichten")),
                            (Icon::Align(true, 0.), AlignEdge::Top, tr("Oben ausrichten")),
                            (Icon::Align(true, 0.5), AlignEdge::VerticalCenter, tr("Vertikal zentrieren")),
                            (Icon::Align(true, 1.), AlignEdge::Bottom, tr("Unten ausrichten")),
                        ] {
                            if icon_button(ui, icon, true)
                                .on_hover_text(format!("{hint} {target}"))
                                .clicked()
                            {
                                op = Some(ArrangeOp::Align(edge));
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.;
                        let enough = editable >= 3;
                        for (icon, axis, hint) in [
                            (Icon::Distribute(false), Axis::Horizontal, tr("Horizontal gleiche Abstände")),
                            (Icon::Distribute(true), Axis::Vertical, tr("Vertikal gleiche Abstände")),
                        ] {
                            if icon_button(ui, icon, enough)
                                .on_hover_text(hint)
                                .on_disabled_hover_text(trf("{hint} · mindestens drei Formen auswählen", &[hint.to_string()]))
                                .clicked()
                            {
                                op = Some(ArrangeOp::Distribute(axis));
                            }
                        }
                        ui.add_space(6.);
                        for (icon, axis, hint) in [
                            (Icon::Flip(false), Axis::Horizontal, tr("Horizontal spiegeln · Umschalt+H")),
                            (Icon::Flip(true), Axis::Vertical, tr("Vertikal spiegeln · Umschalt+V")),
                        ] {
                            if icon_button(ui, icon, true).on_hover_text(hint).clicked() {
                                op = Some(ArrangeOp::Flip(axis));
                            }
                        }
                        ui.add_space(6.);
                        for (icon, stack, hint) in [
                            (Icon::Stack(true, true), Stack::Top, tr("Ganz nach vorne · Umschalt+Bild⬆")),
                            (Icon::Stack(true, false), Stack::Raise, tr("Eine Ebene nach vorne · Bild⬆")),
                            (Icon::Stack(false, false), Stack::Lower, tr("Eine Ebene nach hinten · Bild⬇")),
                            (Icon::Stack(false, true), Stack::Bottom, tr("Ganz nach hinten · Umschalt+Bild⬇")),
                        ] {
                            if icon_button(ui, icon, true).on_hover_text(hint).clicked() {
                                op = Some(ArrangeOp::Stack(stack));
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.;
                        let enough = editable >= 2;
                        for (icon, boolean, hint) in [
                            (Icon::Boolean(BooleanOp::Union), BooleanOp::Union, tr("Vereinigen")),
                            (Icon::Boolean(BooleanOp::Difference), BooleanOp::Difference, tr("Abziehen: unterste Form minus die anderen")),
                            (Icon::Boolean(BooleanOp::Intersection), BooleanOp::Intersection, tr("Schnittmenge")),
                            (Icon::Boolean(BooleanOp::Exclusion), BooleanOp::Exclusion, tr("Ausschluss (nur nicht überlappende Bereiche)")),
                        ] {
                            if icon_button(ui, icon, enough)
                                .on_hover_text(trf("{hint} · ergibt einen Pfad mit dem Stil der untersten Form; Kurven werden geglättet", &[hint.to_string()]))
                                .on_disabled_hover_text(trf("{hint} · mindestens zwei Formen (kein Text) auswählen", &[hint.to_string()]))
                                .clicked()
                            {
                                op = Some(ArrangeOp::Boolean(boolean));
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        let grouped = selection.iter().any(|id| {
                            self.project.nodes.iter().any(|n| &n.id == id && n.group.is_some())
                        });
                        if ui
                            .add_enabled(count == 2, egui::Button::new(tr("Morph")))
                            .on_hover_text(tr("Die zuerst gewählte Form verwandelt sich in die zweite (neuer 1-s-Clip, Zielform wird ausgeblendet)."))
                            .on_disabled_hover_text(tr("Genau zwei Formen wählen"))
                            .clicked()
                        {
                            op = Some(ArrangeOp::Morph);
                        }
                        if ui
                            .add_enabled(count > 1, egui::Button::new(tr("Gruppieren")))
                            .on_hover_text(trf("{} · Klick wählt dann die ganze Gruppe, Strg+Klick eine einzelne Form", &[self.binding_label(&super::palette::Cmd::Arrange(ArrangeOp::Group)).to_string()]))
                            .clicked()
                        {
                            op = Some(ArrangeOp::Group);
                        }
                        if ui
                            .add_enabled(grouped, egui::Button::new(tr("Gruppe lösen")))
                            .on_hover_text(self.binding_label(&super::palette::Cmd::Arrange(ArrangeOp::Ungroup)))
                            .clicked()
                        {
                            op = Some(ArrangeOp::Ungroup);
                        }
                    });
                });
                if editable < count {
                    ui.weak(trf("{} gesperrte Form(en) bleiben unverändert.", &[format!("{}", count - editable)]));
                }
            });
        op
    }
}

/// Small vector icons; egui's bundled fonts lack most arrangement glyphs.
#[derive(Clone, Copy)]
enum Icon {
    /// (vertical axis, position of the guide line 0 / 0.5 / 1)
    Align(bool, f32),
    /// vertical?
    Distribute(bool),
    /// vertical?
    Flip(bool),
    /// (up, all the way)
    Stack(bool, bool),
    Boolean(BooleanOp),
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, enabled: bool) -> egui::Response {
    let size = egui::vec2(28., 26.);
    let (rect, response) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let enabled = enabled && ui.is_enabled();
    let visuals = if enabled {
        *ui.style().interact(&response)
    } else {
        ui.visuals().widgets.noninteractive
    };
    ui.painter().rect_filled(
        rect,
        4.,
        if enabled {
            visuals.weak_bg_fill
        } else {
            Color32::TRANSPARENT
        },
    );
    let color = if enabled {
        visuals.fg_stroke.color
    } else {
        ui.visuals().weak_text_color()
    };
    let stroke = egui::Stroke::new(1.5_f32, color);
    let painter = ui.painter();
    let c = rect.center();
    // Draw in a horizontal frame of reference; `t` transposes for vertical icons.
    let t = |vertical: bool, x: f32, y: f32| {
        if vertical {
            c + egui::vec2(y, x)
        } else {
            c + egui::vec2(x, y)
        }
    };
    match icon {
        Icon::Align(vertical, at) => {
            let x = -8. + 16. * at;
            painter.line_segment([t(vertical, x, -9.), t(vertical, x, 9.)], stroke);
            for (y, len) in [(-4., 12.), (4., 7.)] {
                let start = -8. + (16. - len) * at;
                let a = t(vertical, start, y - 2.5);
                let b = t(vertical, start + len, y + 2.5);
                painter.rect_filled(
                    egui::Rect::from_two_pos(a, b),
                    1.,
                    color.gamma_multiply(0.7),
                );
            }
        }
        Icon::Distribute(vertical) => {
            for x in [-8., -1., 6.] {
                let a = t(vertical, x, -6.);
                let b = t(vertical, x + 3., 6.);
                painter.rect_filled(
                    egui::Rect::from_two_pos(a, b),
                    1.,
                    color.gamma_multiply(0.8),
                );
            }
        }
        Icon::Flip(vertical) => {
            painter.add(egui::Shape::dashed_line(
                &[t(vertical, 0., -9.), t(vertical, 0., 9.)],
                egui::Stroke::new(1_f32, color),
                2.,
                2.,
            ));
            painter.add(egui::Shape::convex_polygon(
                vec![
                    t(vertical, -2., -6.),
                    t(vertical, -2., 6.),
                    t(vertical, -9., 6.),
                ],
                color.gamma_multiply(0.8),
                egui::Stroke::NONE,
            ));
            painter.add(egui::Shape::closed_line(
                vec![
                    t(vertical, 2., -6.),
                    t(vertical, 9., 6.),
                    t(vertical, 2., 6.),
                ],
                egui::Stroke::new(1_f32, color),
            ));
        }
        Icon::Boolean(boolean) => {
            // Two overlapping squares; the filled parts show the result.
            let a = egui::Rect::from_min_size(c + egui::vec2(-9., -8.), egui::vec2(11., 11.));
            let b = egui::Rect::from_min_size(c + egui::vec2(-2., -3.), egui::vec2(11., 11.));
            let overlap = a.intersect(b);
            let fill = color.gamma_multiply(0.8);
            let thin = egui::Stroke::new(1_f32, color);
            match boolean {
                BooleanOp::Union => {
                    painter.rect_filled(a, 0., fill);
                    painter.rect_filled(b, 0., fill);
                }
                BooleanOp::Difference => {
                    painter.rect_filled(a, 0., fill);
                    painter.rect_filled(overlap, 0., visuals.weak_bg_fill);
                    painter.rect_stroke(b, 0., thin, egui::StrokeKind::Inside);
                }
                BooleanOp::Intersection => {
                    painter.rect_stroke(a, 0., thin, egui::StrokeKind::Inside);
                    painter.rect_stroke(b, 0., thin, egui::StrokeKind::Inside);
                    painter.rect_filled(overlap, 0., fill);
                }
                BooleanOp::Exclusion => {
                    painter.rect_filled(a, 0., fill);
                    painter.rect_filled(b, 0., fill);
                    painter.rect_filled(overlap, 0., visuals.weak_bg_fill);
                }
            }
        }
        Icon::Stack(up, all) => {
            let dir = if up { -1. } else { 1. };
            let tip = c + egui::vec2(0., 6. * dir);
            let tail = c + egui::vec2(0., -6. * dir);
            painter.line_segment([tail, tip], stroke);
            painter.line_segment([tip, tip + egui::vec2(-4., -4. * dir)], stroke);
            painter.line_segment([tip, tip + egui::vec2(4., -4. * dir)], stroke);
            if all {
                let y = 9. * dir;
                painter.line_segment([c + egui::vec2(-6., y), c + egui::vec2(6., y)], stroke);
            }
        }
    }
    response
}
