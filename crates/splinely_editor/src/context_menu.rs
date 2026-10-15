//! Right-click menu on the canvas, zoom to selection and style copy/paste.
use super::palette::Cmd;
use super::selection::ArrangeOp;
use super::*;
use splinely_document::{Axis, Stack};

impl Editor {
    /// Right-click on a shape selects it first (unless it is already selected).
    pub(super) fn context_pick(&mut self, scene: &[SceneNode], point: Vec2) {
        let hit_id = scene
            .iter()
            .rev()
            .find(|node| {
                hit(node, point)
                    && self
                        .project
                        .nodes
                        .iter()
                        .any(|n| n.id == node.id && n.visible && !n.locked)
            })
            .map(|n| n.id.clone());
        match hit_id {
            Some(id) if !self.is_selected(&id) => self.pick_shape(&id, false, false, false),
            Some(_) => {}
            None => self.set_selection(vec![]),
        }
    }

    pub(super) fn canvas_context_menu(&mut self, ui: &mut egui::Ui) {
        let count = self.selection().len();
        let editable = self.editable_selection().len();
        let button = |ui: &mut egui::Ui, enabled: bool, label: &str, shortcut: &str| {
            ui.add_enabled(enabled, egui::Button::new(label).shortcut_text(shortcut))
                .clicked()
        };
        if count == 0 {
            if button(
                ui,
                !self.clipboard.nodes.is_empty(),
                tr("Einfügen"),
                "Strg+V",
            ) {
                self.paste_from_menu();
                ui.close_menu();
            }
            if button(
                ui,
                !self.project.nodes.is_empty(),
                tr("Alles auswählen"),
                &self.binding_label(&Cmd::SelectAll),
            ) {
                self.select_all();
                ui.close_menu();
            }
            ui.separator();
            ui.menu_button(tr("Neue Form"), |ui| {
                for (shape, label) in [
                    (shortcuts::NewShape::Rect, tr("Rechteck")),
                    (shortcuts::NewShape::Circle, tr("Kreis")),
                    (shortcuts::NewShape::Text, tr("Text")),
                    (shortcuts::NewShape::Polygon, tr("Sechseck")),
                    (shortcuts::NewShape::Triangle, tr("Dreieck")),
                    (shortcuts::NewShape::Star, tr("☆ Stern")),
                    (shortcuts::NewShape::Line, tr("Linie")),
                ] {
                    if ui.button(label).clicked() {
                        self.add_shape(shape);
                        ui.close_menu();
                    }
                }
            });
            if button(ui, true, tr("Alles einpassen"), "F") {
                self.fit_requested = true;
                ui.close_menu();
            }
            return;
        }
        ui.weak(if count == 1 {
            tr("1 Form").to_owned()
        } else {
            trf("{count} Formen", &[format!("{count}")])
        });
        if button(ui, editable > 0, tr("Ausschneiden"), "Strg+X") {
            self.copy_selection_to(ui.ctx());
            self.delete_selected();
            ui.close_menu();
        }
        if button(ui, true, tr("Kopieren"), "Strg+C") {
            self.copy_selection_to(ui.ctx());
            ui.close_menu();
        }
        if button(
            ui,
            !self.clipboard.nodes.is_empty(),
            tr("Einfügen"),
            "Strg+V",
        ) {
            self.paste_from_menu();
            ui.close_menu();
        }
        if button(
            ui,
            editable > 0,
            tr("Duplizieren"),
            &self.binding_label(&Cmd::Duplicate),
        ) {
            self.duplicate_selected();
            ui.close_menu();
        }
        if button(ui, editable > 0, tr("Löschen"), "Entf") {
            self.delete_selected();
            ui.close_menu();
        }
        ui.separator();
        if button(ui, true, tr("Stil kopieren"), "") {
            self.copy_style();
            ui.close_menu();
        }
        if button(
            ui,
            self.style_clipboard.is_some() && editable > 0,
            tr("Stil einfügen"),
            "",
        ) {
            self.paste_style();
            ui.close_menu();
        }
        if button(
            ui,
            editable > 0,
            tr("Pipette: Stil übernehmen …"),
            &self.binding_label(&Cmd::Eyedropper),
        ) {
            self.eyedropper = true;
            self.status =
                tr("Pipette: Form anklicken, deren Stil übernommen wird · Escape bricht ab.")
                    .into();
            ui.close_menu();
        }
        ui.separator();
        let mut op = None;
        let key = |op: ArrangeOp| self.binding_label(&Cmd::Arrange(op));
        let stack_keys = [Stack::Top, Stack::Raise, Stack::Lower, Stack::Bottom]
            .map(|s| key(ArrangeOp::Stack(s)));
        let flip_keys = [Axis::Horizontal, Axis::Vertical].map(|a| key(ArrangeOp::Flip(a)));
        let (group_key, ungroup_key) = (key(ArrangeOp::Group), key(ArrangeOp::Ungroup));
        ui.menu_button(tr("Anordnen"), |ui| {
            for ((label, stack), shortcut) in [
                (tr("Ganz nach vorne"), Stack::Top),
                (tr("Nach vorne"), Stack::Raise),
                (tr("Nach hinten"), Stack::Lower),
                (tr("Ganz nach hinten"), Stack::Bottom),
            ]
            .into_iter()
            .zip(&stack_keys)
            {
                if ui
                    .add(egui::Button::new(label).shortcut_text(shortcut))
                    .clicked()
                {
                    op = Some(ArrangeOp::Stack(stack));
                    ui.close_menu();
                }
            }
            ui.separator();
            if ui
                .add(egui::Button::new(tr("Horizontal spiegeln")).shortcut_text(&flip_keys[0]))
                .clicked()
            {
                op = Some(ArrangeOp::Flip(Axis::Horizontal));
                ui.close_menu();
            }
            if ui
                .add(egui::Button::new(tr("Vertikal spiegeln")).shortcut_text(&flip_keys[1]))
                .clicked()
            {
                op = Some(ArrangeOp::Flip(Axis::Vertical));
                ui.close_menu();
            }
        });
        if count == 2 && button(ui, editable > 0, tr("Morph: erste › zweite Form"), "") {
            op = Some(ArrangeOp::Morph);
            ui.close_menu();
        }
        if count > 1 && button(ui, editable > 1, tr("Gruppieren"), &group_key) {
            op = Some(ArrangeOp::Group);
            ui.close_menu();
        }
        let grouped = self.selection().iter().any(|id| {
            self.project
                .nodes
                .iter()
                .any(|n| &n.id == id && n.group.is_some())
        });
        if grouped && button(ui, true, tr("Gruppe lösen"), &ungroup_key) {
            op = Some(ArrangeOp::Ungroup);
            ui.close_menu();
        }
        if let Some(op) = op {
            self.apply_arrange(op);
        }
        ui.separator();
        if button(ui, editable > 0, tr("Sperren"), "") {
            self.set_flags(Some(true), None);
            ui.close_menu();
        }
        if button(ui, editable > 0, tr("Ausblenden"), "") {
            self.set_flags(None, Some(false));
            ui.close_menu();
        }
        if button(
            ui,
            true,
            tr("Auf Auswahl zoomen"),
            &self.binding_label(&Cmd::ZoomSelection),
        ) {
            self.zoom_selection_requested = true;
            ui.close_menu();
        }
    }

    fn copy_selection_to(&mut self, ctx: &egui::Context) {
        self.copy_selected(ctx);
    }

    fn paste_from_menu(&mut self) {
        if !self.clipboard.nodes.is_empty() {
            // Same path as Strg+V with an empty system clipboard.
            self.paste_clipboard(self.clipboard.clone());
        }
    }

    pub(super) fn copy_style(&mut self) {
        if let Some(node) = self
            .project
            .nodes
            .iter()
            .find(|n| Some(&n.id) == self.selected.as_ref())
        {
            self.style_clipboard = Some((node.style, node.gradient.clone()));
            self.status = tr("Stil kopiert · Rechtsklick › Stil einfügen überträgt Füllung/Verlauf, Rand und Deckkraft.").into();
        }
    }

    pub(super) fn paste_style(&mut self) {
        let Some((style, gradient)) = self.style_clipboard.clone() else {
            return;
        };
        let ids = self.editable_selection();
        let mut next = self.project.clone();
        for node in next.nodes.iter_mut().filter(|n| ids.contains(&n.id)) {
            node.style = style;
            node.gradient = gradient.clone();
        }
        self.commit(next);
        self.status = trf(
            "Stil auf {} Form(en) übertragen.",
            &[format!("{}", ids.len())],
        );
    }

    /// Pipette click: take style and gradient from the shape under the pointer.
    /// Returns true when the click was consumed by the pipette.
    pub(super) fn eyedropper_click(&mut self, scene: &[SceneNode], point: Vec2) -> bool {
        if !self.eyedropper {
            return false;
        }
        self.eyedropper = false;
        let source = scene
            .iter()
            .rev()
            .find(|n| hit(n, point))
            .and_then(|n| self.project.nodes.iter().find(|m| m.id == n.id))
            .map(|n| (n.style, n.gradient.clone()));
        match source {
            Some(style) => {
                let previous = self.style_clipboard.replace(style);
                self.paste_style();
                self.style_clipboard = previous;
            }
            None => self.status = tr("Pipette: keine Form getroffen.").into(),
        }
        true
    }

    /// Lock and/or show/hide the editable selection as one undo step.
    fn set_flags(&mut self, locked: Option<bool>, visible: Option<bool>) {
        let ids = self.editable_selection();
        let mut next = self.project.clone();
        for node in next.nodes.iter_mut().filter(|n| ids.contains(&n.id)) {
            if let Some(locked) = locked {
                node.locked = locked;
            }
            if let Some(visible) = visible {
                node.visible = visible;
            }
        }
        self.commit(next);
        if locked == Some(true) || visible == Some(false) {
            self.set_selection(vec![]);
            self.status = if locked.is_some() {
                tr("Gesperrt · Entsperren über das Schloss in der Ebenenliste.").into()
            } else {
                tr("Ausgeblendet · Einblenden über das Auge in der Ebenenliste.").into()
            };
        }
    }

    /// Zoom and pan so the selection fills most of the canvas.
    pub(super) fn apply_zoom_to_selection(&mut self, content: Rect, scene: &[SceneNode]) {
        self.zoom_selection_requested = false;
        let ids = self.selection();
        let Some(bounds) = scene
            .iter()
            .filter(|n| ids.contains(&n.id))
            .map(selection::scene_rect)
            .reduce(|a, b| a.union(b))
        else {
            self.status = tr("Nichts ausgewählt.").into();
            return;
        };
        let size = bounds.size().max(Vec2::splat(1.));
        let zoom = ((content.width() * 0.8) / size.x)
            .min((content.height() * 0.8) / size.y)
            .clamp(MIN_ZOOM, MAX_ZOOM);
        let document = Vec2::new(self.project.width as f32, self.project.height as f32);
        self.zoom = zoom;
        self.view_zoom = zoom;
        self.pan_offset = document * zoom * 0.5 - bounds.center().to_vec2() * zoom;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor_with(nodes: Vec<Node>) -> Editor {
        let mut editor = Editor::default();
        editor.project.nodes = nodes;
        editor.project.clips.clear();
        editor.project.state_machines.clear();
        editor.sync_buffers();
        editor
    }

    #[test]
    fn style_copy_paste_lock_hide_and_zoom_to_selection() {
        let mut a = Node::rect("a", 0., 0., 10., 10.);
        a.style.fill = Color::rgb(255, 0, 0);
        a.style.stroke = Some(Color::rgb(0, 0, 255));
        let mut editor = editor_with(vec![a, Node::circle("b", 300., 200., 10.)]);
        editor.set_selection(vec!["a".into()]);
        editor.copy_style();
        editor.set_selection(vec!["b".into()]);
        editor.paste_style();
        assert_eq!(editor.project.nodes[1].style, editor.project.nodes[0].style);

        let scene = editor.evaluate_scene(&EvaluationContext::default());
        let content = Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.));
        editor.apply_zoom_to_selection(content, &scene);
        // The circle centre lands in the middle of the canvas.
        let document = Vec2::new(editor.project.width as f32, editor.project.height as f32);
        let origin = canvas_origin(content, document, editor.zoom, editor.pan_offset);
        let centre = origin + Vec2::new(300., 200.) * editor.zoom;
        assert!((centre - content.center()).length() < 1e-2);
        assert!(editor.zoom > 10.);

        // Pipette: take a's style back onto b.
        editor.project.nodes[1].style.fill = Color::rgb(0, 255, 0);
        editor.set_selection(vec!["b".into()]);
        editor.eyedropper = true;
        let scene = editor.evaluate_scene(&EvaluationContext::default());
        assert!(editor.eyedropper_click(&scene, Vec2::new(5., 5.)));
        assert_eq!(editor.project.nodes[1].style.fill, Color::rgb(255, 0, 0));
        assert!(!editor.eyedropper);
        assert!(!editor.eyedropper_click(&scene, Vec2::new(5., 5.)));

        editor.set_flags(None, Some(false));
        assert!(!editor.project.nodes[1].visible);
        assert!(editor.selection().is_empty());
        editor.set_selection(vec!["a".into()]);
        editor.set_flags(Some(true), None);
        assert!(editor.project.nodes[0].locked);
        editor.history.undo(&mut editor.project);
        assert!(!editor.project.nodes[0].locked);
    }

    #[test]
    fn right_click_selects_the_shape_under_the_pointer() {
        let mut editor = editor_with(vec![
            Node::rect("a", 0., 0., 10., 10.),
            Node::rect("b", 50., 0., 10., 10.),
        ]);
        editor.set_selection(vec!["a".into(), "b".into()]);
        let scene = editor.evaluate_scene(&EvaluationContext::default());
        editor.context_pick(&scene, Vec2::new(55., 5.));
        assert_eq!(
            editor.selection().len(),
            2,
            "keeps an existing multi-selection"
        );
        editor.set_selection(vec!["a".into()]);
        editor.context_pick(&scene, Vec2::new(55., 5.));
        assert_eq!(editor.selection(), vec!["b".to_string()]);
        editor.context_pick(&scene, Vec2::new(500., 500.));
        assert!(editor.selection().is_empty());
    }
}

#[cfg(test)]
mod bounds_tests {
    use super::*;

    #[test]
    fn typed_selection_bounds_move_and_resize() {
        let mut editor = Editor::default();
        editor.project.nodes = vec![
            Node::rect("a", 0., 0., 10., 10.),
            Node::rect("b", 30., 0., 10., 20.),
        ];
        editor.project.clips.clear();
        editor.project.state_machines.clear();
        editor.sync_buffers();
        let ids = vec!["a".to_string(), "b".to_string()];
        editor.set_selection(ids.clone());
        editor.apply_arrange(ArrangeOp::SetBounds([100., 50., 80., 20.], false));
        let b = editor.project.bounds_of(&ids).unwrap();
        assert!((b[0] - 100.).abs() < 1e-9 && (b[1] - 50.).abs() < 1e-9);
        assert!((b[2] - 80.).abs() < 1e-9 && (b[3] - 20.).abs() < 1e-9);
        // Proportional: changing only the width scales the height too.
        editor.apply_arrange(ArrangeOp::SetBounds([100., 50., 40., 20.], true));
        let b = editor.project.bounds_of(&ids).unwrap();
        assert!((b[2] - 40.).abs() < 1e-9 && (b[3] - 10.).abs() < 1e-9);
    }
}
