//! Edit text shapes in place: double-click a text on the canvas, type, then
//! click elsewhere or press Strg+Enter to apply (one undo step); Escape cancels.
use super::*;

/// Text being edited on the canvas.
pub(super) struct TextDraft {
    pub id: String,
    pub text: String,
    pub focused: bool,
}

impl Editor {
    /// Open the in-place editor for a text shape and select it (double-click
    /// on the canvas, or Enter on a keyboard-selected text).
    pub(super) fn begin_text_draft(&mut self, id: &str) {
        if self.text_draft.is_some() || self.preview_mode {
            return;
        }
        if let Some(NodeKind::Text { text, .. }) = self
            .project
            .nodes
            .iter()
            .find(|m| m.id == id && !m.locked)
            .map(|m| &m.kind)
        {
            self.text_draft = Some(TextDraft {
                id: id.to_owned(),
                text: text.clone(),
                focused: false,
            });
            self.set_selection(vec![id.to_owned()]);
        }
    }

    /// Start editing on double-click; draw the editor while active.
    pub(super) fn canvas_text_edit(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        origin: Pos2,
        scene: &[SceneNode],
    ) {
        if response.double_clicked()
            && self.text_draft.is_none()
            && !self.preview_mode
            && self.canvas_tool() == CanvasTool::Selection
        {
            if let Some(p) = response.interact_pointer_pos() {
                let point = (p - origin) / self.zoom;
                let hit_text = scene.iter().rev().find(|n| {
                    matches!(n.shape, Shape::Text { .. })
                        && hit(n, point)
                        && self.project.nodes.iter().any(|m| m.id == n.id && !m.locked)
                });
                if let Some(node) = hit_text {
                    self.begin_text_draft(&node.id.clone());
                }
            }
        }
        let Some(draft) = &mut self.text_draft else {
            return;
        };
        let Some(node) = scene.iter().find(|n| n.id == draft.id) else {
            self.text_draft = None;
            return;
        };
        let Shape::Text { size, .. } = node.shape else {
            self.text_draft = None;
            return;
        };
        let font = (size * node.scale_y.abs() * self.zoom).clamp(8., 200.);
        let pos = origin + Vec2::new(node.x, node.y - size * node.scale_y.abs()) * self.zoom;
        let mut finish: Option<bool> = None; // Some(true) = apply, Some(false) = cancel
        egui::Area::new(egui::Id::new("splinely_text_edit"))
            .fixed_pos(pos - Vec2::new(4., 4.))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let width = (draft.text.chars().count().max(4) as f32 * font * 0.62).min(900.);
                    let edit = ui.add(
                        egui::TextEdit::multiline(&mut draft.text)
                            .font(egui::FontId::proportional(font))
                            .desired_rows(1)
                            .desired_width(width),
                    );
                    if !draft.focused {
                        edit.request_focus();
                        draft.focused = true;
                    }
                    let (escape, apply) = ui.input(|i| {
                        (
                            i.key_pressed(egui::Key::Escape),
                            i.modifiers.command && i.key_pressed(egui::Key::Enter),
                        )
                    });
                    if escape {
                        finish = Some(false);
                    } else if apply || (draft.focused && edit.lost_focus()) {
                        finish = Some(true);
                    }
                    ui.weak(tr(
                        "Strg+Enter oder Klick daneben: übernehmen · Esc: abbrechen",
                    ));
                });
            });
        match finish {
            Some(true) => {
                if let Some(draft) = self.text_draft.take() {
                    self.apply_text(&draft.id, &draft.text);
                }
            }
            Some(false) => self.text_draft = None,
            None => {}
        }
    }

    pub(super) fn apply_text(&mut self, id: &str, text: &str) {
        let mut next = self.project.clone();
        if let Some(NodeKind::Text { text: t, .. }) = next
            .nodes
            .iter_mut()
            .find(|n| n.id == id)
            .map(|n| &mut n.kind)
        {
            if t == text {
                return;
            }
            *t = text.to_owned();
        }
        self.commit(next);
        self.status = tr("Text geändert · Strg+Z macht es rückgängig.").into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applying_text_is_one_undo_step() {
        let mut editor = Editor::default();
        let id = editor
            .project
            .nodes
            .iter()
            .find(|n| matches!(n.kind, NodeKind::Text { .. }))
            .unwrap()
            .id
            .clone();
        editor.apply_text(&id, "Hallo Welt");
        let text = |e: &Editor| match &e.project.nodes.iter().find(|n| n.id == id).unwrap().kind {
            NodeKind::Text { text, .. } => text.clone(),
            _ => unreachable!(),
        };
        assert_eq!(text(&editor), "Hallo Welt");
        editor.history.undo(&mut editor.project);
        assert_ne!(text(&editor), "Hallo Welt");
    }
}
