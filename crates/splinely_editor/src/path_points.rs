//! Point-level editing of the selected path in the "Pfadpunkte" tool: point
//! selection, node types, break/join, delete, nudge and a point clipboard.
//! The geometry lives in `splinely_document::path_edit`; every change goes
//! through `commit`, so it is validated and is exactly one undo step.
use super::palette::Cmd;
use super::vector::tools::{EditablePath, Handle, HandleKind, NodeType, Segment};
use super::*;
use splinely_document::path_edit;
use std::collections::BTreeSet;

/// Commands on the selected points (bound in the keymap, see `keymap.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PointOp {
    Cusp,
    Smooth,
    Symmetric,
    Auto,
    Retract,
    Extend,
    Break,
    Join,
    ToLine,
    ToCurve,
    Reverse,
}

impl PointOp {
    pub(super) const ALL: [PointOp; 11] = [
        PointOp::Cusp,
        PointOp::Smooth,
        PointOp::Symmetric,
        PointOp::Auto,
        PointOp::Retract,
        PointOp::Extend,
        PointOp::Break,
        PointOp::Join,
        PointOp::ToLine,
        PointOp::ToCurve,
        PointOp::Reverse,
    ];
}

impl Editor {
    /// Id and parsed path whose points the Pfadpunkte tool edits right now.
    pub(super) fn point_path(&self) -> Option<(String, EditablePath)> {
        if self.tab != Tab::Canvas
            || self.preview_mode
            || self.canvas_tool() != CanvasTool::PathPoints
        {
            return None;
        }
        let id = self.selected.clone()?;
        let path = PathGesture::node_path(&self.project, &id).ok()?;
        Some((id, path))
    }

    /// Selected anchors that still exist in the edited path (canonical).
    pub(super) fn selected_points(&self) -> Option<(String, EditablePath, Vec<Handle>)> {
        let (id, path) = self.point_path()?;
        if self.point_owner.as_deref() != Some(id.as_str()) {
            return None;
        }
        let points: Vec<Handle> = self
            .point_selection
            .iter()
            .copied()
            .filter(|h| path_edit::canonical_anchor(path.segments(), *h) == Some(*h))
            .collect();
        (!points.is_empty()).then_some((id, path, points))
    }

    /// Replace the point selection of path `id` (anchors are canonicalised).
    pub(super) fn set_point_selection(
        &mut self,
        id: &str,
        segments: &[Segment],
        points: impl IntoIterator<Item = Handle>,
    ) {
        self.point_owner = Some(id.to_owned());
        self.point_selection = points
            .into_iter()
            .filter_map(|h| path_edit::canonical_anchor(segments, h))
            .collect();
    }

    /// Run a pure edit on the selected points and commit it as one step.
    fn edit_points(
        &mut self,
        done: &str,
        op: impl FnOnce(&[Segment], &[Handle]) -> Result<(Vec<Segment>, Vec<Handle>), String>,
    ) -> bool {
        let Some((id, path, points)) = self.selected_points() else {
            self.status =
                tr("Zuerst Pfadpunkte auswählen (Klick, Umschalt+Klick oder Rahmen).").into();
            return false;
        };
        let result = op(path.segments(), &points).and_then(|(segments, selected)| {
            let edited = EditablePath::from_segments(segments)?;
            let next = PathGesture::replace_candidate(&self.project, &id, &edited)?;
            Ok((next, edited, selected))
        });
        match result {
            Ok((next, edited, selected)) => {
                self.commit(next);
                self.set_point_selection(&id, edited.segments(), selected);
                self.status = done.into();
                true
            }
            Err(error) => {
                self.status = error;
                false
            }
        }
    }

    pub(super) fn point_op_allowed(&self, op: PointOp) -> bool {
        match self.selected_points() {
            Some((_, path, points)) => match op {
                PointOp::Join => points.len() == 2,
                PointOp::ToLine | PointOp::ToCurve => {
                    !path_edit::edges_between(path.segments(), &points).is_empty()
                }
                _ => true,
            },
            None => false,
        }
    }

    pub(super) fn apply_point_op(&mut self, op: PointOp) -> bool {
        let node_type =
            |ty: NodeType| move |s: &[Segment], p: &[Handle]| path_edit::set_node_type(s, p, ty);
        match op {
            PointOp::Cusp => self.edit_points(
                tr("Tangenten getrennt · Griffe sind jetzt unabhängig."),
                node_type(NodeType::Cusp),
            ),
            PointOp::Smooth => {
                self.edit_points(tr("Punkte geglättet."), node_type(NodeType::Smooth))
            }
            PointOp::Symmetric => {
                self.edit_points(tr("Punkte symmetrisch."), node_type(NodeType::Symmetric))
            }
            PointOp::Auto => self.edit_points(
                tr("Automatische Tangenten gesetzt."),
                node_type(NodeType::Auto),
            ),
            PointOp::Retract => self.edit_points(tr("Griffe eingezogen."), |s, p| {
                path_edit::retract_handles(s, p)
            }),
            PointOp::Extend => self.edit_points(tr("Griffe herausgezogen."), |s, p| {
                path_edit::extend_handles(s, p)
            }),
            PointOp::Break => self.edit_points(tr("Pfad an den Punkten aufgetrennt."), |s, p| {
                Ok((path_edit::break_at_anchors(s, p)?, vec![]))
            }),
            PointOp::Join => self.edit_points(tr("Endpunkte verbunden."), |s, p| {
                let [a, b] = p else {
                    return Err(tr("Genau zwei Endpunkte wählen, um sie zu verbinden.").into());
                };
                Ok((path_edit::join_endpoints(s, *a, *b)?, vec![]))
            }),
            PointOp::ToLine | PointOp::ToCurve => {
                let curve = op == PointOp::ToCurve;
                let done = if curve {
                    tr("Segmente in Kurven umgewandelt.")
                } else {
                    tr("Segmente in Linien umgewandelt.")
                };
                self.edit_points(done, |s, p| {
                    let mut edges = path_edit::edges_between(s, p);
                    if edges.is_empty() {
                        return Err(tr("Zwei benachbarte Punkte wählen.").into());
                    }
                    // Back to front: an implicit closing line becomes an
                    // explicit segment and shifts every later index by one.
                    edges.sort_unstable();
                    let mut segments = s.to_vec();
                    let mut selected = p.to_vec();
                    for &edge in edges.iter().rev() {
                        let before = segments.len();
                        segments = if curve {
                            path_edit::segment_to_curve(&segments, edge)?
                        } else {
                            path_edit::segment_to_line(&segments, edge)?
                        };
                        if segments.len() > before {
                            for h in &mut selected {
                                if h.segment >= edge {
                                    h.segment += segments.len() - before;
                                }
                            }
                        }
                    }
                    Ok((segments, selected))
                })
            }
            PointOp::Reverse => self.edit_points(tr("Pfadrichtung umgekehrt."), |s, p| {
                Ok((path_edit::reverse_subpaths(s, p)?, vec![]))
            }),
        }
    }

    /// Delete the selected points; deleting every point deletes the shape.
    pub(super) fn delete_points(&mut self) -> bool {
        let Some((_, path, points)) = self.selected_points() else {
            return false;
        };
        if path_edit::anchors(path.segments()).len() <= points.len() {
            self.point_selection.clear();
            self.delete_selected();
            return true;
        }
        self.edit_points(tr("Punkte gelöscht."), |s, p| {
            Ok((path_edit::delete_anchors(s, p)?, vec![]))
        })
    }

    /// Converts a document-space offset into the path's local space.
    fn local_offset(&self, id: &str, delta: Vec2) -> Result<[f64; 2], String> {
        let node = self
            .project
            .nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or(tr("Pfad fehlt"))?;
        let map = CanvasMap::from_transform(Pos2::ZERO, 1., node.transform);
        let zero = map.to_local(Pos2::ZERO)?;
        let moved = map.to_local(Pos2::new(delta.x, delta.y))?;
        Ok([moved[0] - zero[0], moved[1] - zero[1]])
    }

    /// Arrow keys: move the selected points (with their handles).
    pub(super) fn nudge_points(&mut self, delta: Vec2) -> bool {
        let Some((id, _, _)) = self.selected_points() else {
            return false;
        };
        if self.path_drag.is_some() {
            return false;
        }
        match self.local_offset(&id, delta) {
            Ok(offset) => self.edit_points(tr("Punkte verschoben."), |s, p| {
                Ok((path_edit::translate_anchors(s, p, offset)?, p.to_vec()))
            }),
            Err(error) => {
                self.status = error;
                false
            }
        }
    }

    pub(super) fn select_all_points(&mut self) -> bool {
        let Some((id, path)) = self.point_path() else {
            return false;
        };
        let all: Vec<Handle> = path_edit::anchors(path.segments())
            .into_iter()
            .map(|(h, _)| h)
            .collect();
        self.set_point_selection(&id, path.segments(), all);
        true
    }

    /// Copy runs of adjacent selected points as path data.
    pub(super) fn copy_points(&mut self, ctx: &egui::Context) -> bool {
        let Some((_, path, points)) = self.selected_points() else {
            return false;
        };
        match path_edit::copy_anchors(path.segments(), &points) {
            Ok(clip) if !clip.is_empty() => {
                ctx.copy_text(path_edit::path_data(&clip));
                self.point_clipboard = clip;
                self.status = tr("Punkte kopiert · Strg+V fügt sie als Teilpfad ein.").into();
                true
            }
            Ok(_) => {
                self.status = tr("Mindestens zwei benachbarte Punkte zum Kopieren wählen.").into();
                false
            }
            Err(error) => {
                self.status = error;
                false
            }
        }
    }

    /// Paste copied points (or SVG path data from the system clipboard) into
    /// the edited path as new subpaths. Returns false if there was nothing.
    pub(super) fn paste_points(&mut self, text: Option<&str>) -> bool {
        let Some((id, path)) = self.point_path() else {
            return false;
        };
        let from_text = text
            .map(str::trim)
            .filter(|t| t.starts_with(['M', 'm']))
            .and_then(|t| path_edit::parse(t).ok())
            .filter(|clip| !clip.is_empty());
        let clip = from_text.unwrap_or_else(|| self.point_clipboard.clone());
        if clip.is_empty() {
            return false;
        }
        let offset = self.settings.paste_offset as f32;
        let result = self
            .local_offset(&id, Vec2::new(offset, offset))
            .and_then(|offset| path_edit::paste_subpaths(path.segments(), &clip, offset))
            .and_then(|(segments, pasted)| {
                let edited = EditablePath::from_segments(segments)?;
                let next = PathGesture::replace_candidate(&self.project, &id, &edited)?;
                Ok((next, edited, pasted))
            });
        match result {
            Ok((next, edited, pasted)) => {
                self.commit(next);
                self.set_point_selection(&id, edited.segments(), pasted);
                // Repeated pastes cascade like shape pastes.
                if let Ok(offset) = self.local_offset(&id, Vec2::new(offset, offset)) {
                    if let Ok(moved) = path_edit::translate_anchors(
                        &clip,
                        &path_edit::anchors(&clip)
                            .into_iter()
                            .map(|(h, _)| h)
                            .collect::<Vec<_>>(),
                        offset,
                    ) {
                        self.point_clipboard = moved;
                    }
                }
                self.status = tr("Punkte eingefügt.").into();
            }
            Err(error) => self.status = error,
        }
        true
    }

    /// Right-click on a point selects it (keeping a selection it belongs to).
    pub(super) fn point_context_pick(&mut self, screen: Pos2) {
        let Some((id, path)) = self.point_path() else {
            return;
        };
        let Some((_, map)) = self.point_map.clone().filter(|(owner, _)| *owner == id) else {
            return;
        };
        let current = if self.point_owner.as_deref() == Some(id.as_str()) {
            self.point_selection.clone()
        } else {
            BTreeSet::new()
        };
        if let Some(hit) = path
            .hit_test_points(screen, &map, 8., &current)
            .filter(|h| h.kind == HandleKind::Anchor)
        {
            if !current.contains(&hit) {
                self.set_point_selection(&id, path.segments(), [hit]);
            }
        }
    }

    /// Context menu of the Pfadpunkte tool.
    pub(super) fn points_context_menu(&mut self, ui: &mut egui::Ui) {
        let count = self
            .selected_points()
            .map_or(0, |(_, _, points)| points.len());
        ui.weak(if count == 1 {
            tr("1 Punkt").to_owned()
        } else {
            trf("{count} Punkte", &[format!("{count}")])
        });
        let button = |ui: &mut egui::Ui, enabled: bool, label: &str, shortcut: &str| {
            ui.add_enabled(enabled, egui::Button::new(label).shortcut_text(shortcut))
                .clicked()
        };
        if button(ui, count > 0, tr("Ausschneiden"), "Strg+X") {
            if self.copy_points(ui.ctx()) {
                self.delete_points();
            }
            ui.close_menu();
        }
        if button(ui, count > 0, tr("Kopieren"), "Strg+C") {
            self.copy_points(ui.ctx());
            ui.close_menu();
        }
        if button(
            ui,
            !self.point_clipboard.is_empty(),
            tr("Einfügen"),
            "Strg+V",
        ) {
            self.paste_points(None);
            ui.close_menu();
        }
        if button(ui, count > 0, tr("Punkte löschen"), "Entf") {
            self.delete_points();
            ui.close_menu();
        }
        if button(
            ui,
            self.point_path().is_some(),
            tr("Alle Punkte auswählen"),
            &self.binding_label(&Cmd::SelectAll),
        ) {
            self.select_all_points();
            ui.close_menu();
        }
        ui.separator();
        for op in PointOp::ALL {
            if matches!(op, PointOp::Retract | PointOp::Break | PointOp::ToLine) {
                ui.separator();
            }
            let cmd = Cmd::Points(op);
            if button(
                ui,
                self.point_op_allowed(op),
                palette::command_label(&cmd),
                &self.binding_label(&cmd),
            ) {
                self.apply_point_op(op);
                ui.close_menu();
            }
        }
    }
}
