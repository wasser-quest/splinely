//! Keyboard shortcuts and the shape clipboard. Every document change goes through
//! `commit`, so validation and undo behave exactly like the equivalent buttons.
//! Editing commands act on the whole selection (see `selection.rs`).
use super::*;

/// Copied shapes plus the group hierarchy they belong to.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct ShapeClipboard {
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub groups: Vec<splinely_document::Group>,
}

/// Shapes that the "+" buttons and R/E/T shortcuts can add.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum NewShape {
    Rect,
    Circle,
    Text,
    Polygon,
    Triangle,
    Star,
    Line,
}

impl Editor {
    /// Adds a shape centred in the visible part of the document and selects it.
    pub(super) fn add_shape(&mut self, kind: NewShape) {
        self.choose_tool(CanvasTool::Selection);
        let [cx, cy] = self
            .view_center
            .unwrap_or([self.project.width / 2., self.project.height / 2.]);
        let id = self.new_id("node");
        let generated = match kind {
            NewShape::Rect => Ok(Node::rect(id.clone(), cx - 70., cy - 45., 140., 90.)),
            NewShape::Circle => Ok(Node::circle(id.clone(), cx, cy, 50.)),
            NewShape::Text => Ok(Node::text(id.clone(), cx - 30., cy, tr("Text"))),
            NewShape::Polygon => Node::polygon(id.clone(), cx, cy, 55., 6),
            NewShape::Triangle => Node::polygon(id.clone(), cx, cy, 60., 3),
            NewShape::Star => Node::star(id.clone(), cx, cy, 60., 5, 0.45),
            NewShape::Line => Node::line(id.clone(), cx - 60., cy, cx + 60., cy),
        };
        let mut node = match generated {
            Ok(node) => node,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        node.name = match kind {
            NewShape::Rect => tr("Rechteck"),
            NewShape::Circle => tr("Kreis"),
            NewShape::Text => tr("Text"),
            NewShape::Polygon => tr("Sechseck"),
            NewShape::Triangle => tr("Dreieck"),
            NewShape::Star => tr("Stern"),
            NewShape::Line => tr("Linie"),
        }
        .into();
        let mut p = self.project.clone();
        p.nodes.push(node);
        self.commit(p);
        if self.project.nodes.iter().any(|n| n.id == id) {
            self.set_selection(vec![id]);
        }
    }

    pub(super) fn delete_selected(&mut self) {
        let ids = self.editable_selection();
        if ids.is_empty() {
            return;
        }
        if !self.machine_source_clean() {
            self.status = tr("Zuerst vorgemerktes Zustands-JSON übernehmen oder neu laden.").into();
            return;
        }
        let mut p = self.project.clone();
        for id in &ids {
            if let Err(error) = p.remove_node_with_references(id) {
                self.status = error.to_string();
                return;
            }
        }
        self.commit(p);
        if !self.project.nodes.iter().any(|n| ids.contains(&n.id)) {
            self.set_selection(vec![]);
            self.machine_buffer =
                serde_json::to_string_pretty(&self.project.state_machines).unwrap_or_default();
            self.status = if ids.len() == 1 {
                tr("Form gelöscht · Strg+Z stellt sie wieder her.").into()
            } else {
                trf(
                    "{} Formen gelöscht · Strg+Z stellt sie wieder her.",
                    &[format!("{}", ids.len())],
                )
            };
        }
    }

    /// Copies of `nodes` with fresh IDs, offset by 10, inserted above the
    /// topmost source. Groups are re-labelled so copies form their own group.
    /// `groups` describes the copied nodes' group hierarchy (from the document
    /// or the clipboard); copies get a fresh, equivalent hierarchy.
    fn insert_copies(
        &mut self,
        mut nodes: Vec<Node>,
        groups: &[splinely_document::Group],
        keep_tracks: bool,
        offset: (f64, f64),
    ) {
        if nodes.is_empty() {
            return;
        }
        let mut p = self.project.clone();
        let source = Project {
            groups: groups.to_vec(),
            ..Project::default()
        };
        p.adopt_group_structure(&mut nodes, &source);
        let new_ids = p.unused_ids("node", nodes.len());
        let index = self
            .selection()
            .iter()
            .filter_map(|id| p.nodes.iter().position(|n| &n.id == id))
            .max()
            .map_or(p.nodes.len(), |i| i + 1);
        let mut copies = vec![];
        for (mut node, new_id) in nodes.into_iter().zip(&new_ids) {
            let old_id = node.id.clone();
            node.id = new_id.clone();
            node.locked = false;
            node.visible = true;
            if node
                .bone_id
                .as_ref()
                .is_some_and(|b| !p.bones.iter().any(|bone| &bone.id == b))
            {
                node.bone_id = None;
            }
            if keep_tracks {
                for clip in &mut p.clips {
                    let tracks: Vec<_> = clip
                        .tracks
                        .iter()
                        .filter(|t| t.node_id == old_id)
                        .cloned()
                        .map(|mut t| {
                            t.node_id = node.id.clone();
                            t
                        })
                        .collect();
                    clip.tracks.extend(tracks);
                }
            }
            copies.push(node);
        }
        let at = index.min(p.nodes.len());
        p.nodes.splice(at..at, copies);
        // Scene-space offset (also correct for rotated bones).
        if let Err(error) = p.translate_nodes(&new_ids, offset.0, offset.1) {
            self.status = error.to_string();
            return;
        }
        self.commit(p);
        if self.project.nodes.iter().any(|n| new_ids.contains(&n.id)) {
            self.set_selection(new_ids);
        }
    }

    fn selected_nodes(&self, editable_only: bool) -> Vec<Node> {
        let ids = if editable_only {
            self.editable_selection()
        } else {
            self.selection()
        };
        self.project
            .nodes
            .iter()
            .filter(|n| ids.contains(&n.id))
            .cloned()
            .collect()
    }

    pub(super) fn duplicate_selected(&mut self) {
        let nodes = self.selected_nodes(true);
        let groups = self.project.groups.clone();
        let ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let Ok(source) = self.project.bounds_of(&ids) else {
            return;
        };
        let centre = |b: splinely_document::Bounds| (b[0] + b[2] / 2., b[1] + b[3] / 2.);
        // Duplicating the previous copies again repeats how far they were moved.
        let offset = match &self.last_duplicate {
            Some((copies, from)) if *copies == ids => {
                let now = centre(source);
                (now.0 - from.0, now.1 - from.1)
            }
            _ => (
                self.settings.duplicate_offset,
                self.settings.duplicate_offset,
            ),
        };
        let offset = if offset.0.abs() < 1e-9 && offset.1.abs() < 1e-9 {
            (
                self.settings.duplicate_offset,
                self.settings.duplicate_offset,
            )
        } else {
            offset
        };
        self.insert_copies(nodes, &groups, true, offset);
        let copies = self.editable_selection();
        self.last_duplicate = Some((copies, centre(source)));
    }

    /// Copy the selection (with its groups) to the internal and system clipboard.
    pub(super) fn copy_selected(&mut self, ctx: &egui::Context) -> bool {
        let nodes = self.selected_nodes(false);
        if nodes.is_empty() {
            return false;
        }
        self.status = if nodes.len() == 1 {
            trf(
                "„{}“ kopiert · Strg+V fügt ein.",
                &[display_name(&nodes[0]).to_string()],
            )
        } else {
            trf(
                "{} Formen kopiert · Strg+V fügt ein.",
                &[format!("{}", nodes.len())],
            )
        };
        self.clipboard = ShapeClipboard {
            nodes,
            groups: self.project.groups.clone(),
        };
        if let Ok(json) = serde_json::to_string(&self.clipboard) {
            ctx.copy_text(json);
        }
        true
    }

    fn paste(&mut self, text: Option<&str>) {
        // Prefer Splinely shapes from the system clipboard (also from another window).
        let parsed = text.and_then(|t| {
            serde_json::from_str::<ShapeClipboard>(t)
                .ok()
                .or_else(|| {
                    serde_json::from_str::<Vec<Node>>(t)
                        .ok()
                        .map(|nodes| ShapeClipboard {
                            nodes,
                            groups: vec![],
                        })
                })
                .or_else(|| {
                    serde_json::from_str::<Node>(t)
                        .ok()
                        .map(|n| ShapeClipboard {
                            nodes: vec![n],
                            groups: vec![],
                        })
                })
        });
        let clipboard = parsed
            .filter(|c| !c.nodes.is_empty())
            .unwrap_or_else(|| self.clipboard.clone());
        self.paste_clipboard(clipboard);
    }

    /// Insert copies from a clipboard and remember them for cascading pastes.
    pub(super) fn paste_clipboard(&mut self, clipboard: ShapeClipboard) {
        let offset = self.settings.paste_offset;
        self.insert_copies(clipboard.nodes, &clipboard.groups, false, (offset, offset));
        // Repeated pastes cascade instead of stacking exactly on top.
        let pasted = self.selected_nodes(false);
        if !pasted.is_empty() {
            self.clipboard = ShapeClipboard {
                nodes: pasted,
                groups: self.project.groups.clone(),
            };
        }
    }

    fn nudge(&mut self, delta: Vec2) {
        let ids = self.editable_selection();
        if ids.is_empty() || self.drag_start.is_some() {
            return;
        }
        let mut p = self.project.clone();
        match p.translate_nodes(&ids, delta.x as f64, delta.y as f64) {
            Ok(()) => {
                self.commit(p);
                self.kbd.ring = true;
                if let Some(text) = self.describe_selection() {
                    self.status = trf("Verschoben: {}", &[text]);
                }
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    pub(super) fn undo_redo(&mut self, redo: bool) {
        self.finish_drag();
        self.finish_edit();
        let changed = if redo {
            self.history.redo(&mut self.project)
        } else {
            self.history.undo(&mut self.project)
        };
        if changed {
            self.dirty = true;
            self.sync_buffers();
        }
    }

    /// Global shortcuts; ignored while a text field has keyboard focus. The
    /// configurable ones go through the keymap, the fixed ones are handled here.
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) {
        self.focus_canvas_key(ctx);
        self.dispatch_keys(ctx);
        if self.typing(ctx) || self.modal_open() || self.editing_keymap() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        let mut pasted = None;
        let mut copy = false;
        let mut cut = false;
        for event in &events {
            match event {
                egui::Event::Copy => copy = true,
                egui::Event::Cut => cut = true,
                egui::Event::Paste(text) => pasted = Some(text.clone()),
                _ => {}
            }
        }
        // "?" opens the shortcut overview like F1 (typed character, any layout).
        if events
            .iter()
            .any(|e| matches!(e, egui::Event::Text(t) if t == "?"))
        {
            self.shortcuts_open = true;
        }
        let (command, shift) = ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
        let pressed = |k: egui::Key| ctx.input(|i| i.key_pressed(k));
        if self.preview_mode {
            return;
        }
        // Pfadpunkte tool: clipboard, Delete and arrows act on the points.
        let point_mode = self.point_path().is_some();
        let has_points = point_mode && self.selected_points().is_some();
        if let Some(key) = self.selected_key.filter(|_| copy || cut) {
            // A selected timeline key takes Ctrl+C/X before shapes do.
            self.copy_key(key);
            if cut {
                self.selected_key = None;
                self.delete_key(key);
            }
        } else if (copy || cut) && has_points {
            if self.copy_points(ctx) {
                self.paste_key_next = false;
                if cut {
                    self.delete_points();
                }
            }
        } else if (copy || cut) && self.copy_selected(ctx) {
            self.paste_key_next = false;
            if cut {
                self.delete_selected();
            }
        }
        let paste = pasted.is_some() || (command && pressed(egui::Key::V));
        if paste && self.paste_key_next && self.key_clipboard.is_some() {
            self.paste_key();
        } else if paste && point_mode && self.paste_points(pasted.as_deref()) {
            // Pasted as a new subpath of the edited path.
        } else if let Some(text) = pasted {
            self.paste(Some(&text));
        } else if command && pressed(egui::Key::V) {
            self.paste(None);
        }
        if command {
            return;
        }
        if has_points && self.selected_key.is_none() {
            if pressed(egui::Key::Delete) || pressed(egui::Key::Backspace) {
                self.delete_points();
            }
            for k in [
                egui::Key::ArrowLeft,
                egui::Key::ArrowRight,
                egui::Key::ArrowUp,
                egui::Key::ArrowDown,
            ] {
                let delta = keyboard_nav::nudge_delta(
                    k,
                    shift,
                    self.settings.nudge_small,
                    self.settings.nudge_large,
                );
                if let (true, Some(delta)) = (pressed(k), delta) {
                    self.nudge_points(delta);
                }
            }
        }
        if self.canvas_tool() == CanvasTool::Selection && self.tab == Tab::Canvas {
            // With a timeline key selected, Delete belongs to the timeline.
            if (pressed(egui::Key::Delete) || pressed(egui::Key::Backspace))
                && self.selected_key.is_none()
            {
                self.delete_selected();
            }
            for k in [
                egui::Key::ArrowLeft,
                egui::Key::ArrowRight,
                egui::Key::ArrowUp,
                egui::Key::ArrowDown,
            ] {
                let delta = keyboard_nav::nudge_delta(
                    k,
                    shift,
                    self.settings.nudge_small,
                    self.settings.nudge_large,
                );
                if let (true, Some(delta)) = (pressed(k), delta) {
                    self.nudge(delta);
                }
            }
            self.keyboard_navigation(ctx);
            if pressed(egui::Key::Escape) {
                if self.eyedropper {
                    self.eyedropper = false;
                    self.status = tr("Pipette abgebrochen.").into();
                } else {
                    if self.selected.is_some() {
                        self.status = tr("Auswahl aufgehoben.").into();
                    }
                    self.kbd.scope = None;
                    self.set_selection(vec![]);
                }
            }
        }
    }
}

pub(super) fn display_name(node: &Node) -> &str {
    if node.name.is_empty() {
        &node.id
    } else {
        &node.name
    }
}

#[cfg(test)]
mod tests {
    use super::super::selection::ArrangeOp;
    use super::*;

    fn key(editor: &mut Editor, ctx: &egui::Context, key: egui::Key, modifiers: egui::Modifiers) {
        let events = [true, false]
            .map(|pressed| egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            })
            .to_vec();
        send(editor, ctx, events, modifiers);
    }

    fn send(
        editor: &mut Editor,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                events,
                modifiers,
                ..Default::default()
            },
            |ctx| editor.shortcuts(ctx),
        );
    }

    fn select_first(editor: &mut Editor) -> String {
        let id = editor
            .project
            .nodes
            .iter()
            .find(|n| !n.locked)
            .unwrap()
            .id
            .clone();
        editor.selected = Some(id.clone());
        id
    }

    #[test]
    fn delete_duplicate_and_undo() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        let count = editor.project.nodes.len();
        let id = select_first(&mut editor);
        key(&mut editor, &ctx, egui::Key::D, egui::Modifiers::COMMAND);
        assert_eq!(editor.project.nodes.len(), count + 1);
        assert_ne!(editor.selected.as_deref(), Some(id.as_str()));
        key(&mut editor, &ctx, egui::Key::Delete, egui::Modifiers::NONE);
        assert_eq!(editor.project.nodes.len(), count);
        assert!(editor.selected.is_none());
        editor.history.undo(&mut editor.project);
        assert_eq!(editor.project.nodes.len(), count + 1);
    }

    #[test]
    fn arrows_nudge_one_or_ten_units_as_single_undo_steps() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        let id = select_first(&mut editor);
        let x = |e: &Editor| {
            e.project
                .nodes
                .iter()
                .find(|n| n.id == id)
                .unwrap()
                .transform
                .x
        };
        let start = x(&editor);
        key(
            &mut editor,
            &ctx,
            egui::Key::ArrowRight,
            egui::Modifiers::NONE,
        );
        assert_eq!(x(&editor), start + 1.);
        key(
            &mut editor,
            &ctx,
            egui::Key::ArrowLeft,
            egui::Modifiers::SHIFT,
        );
        assert_eq!(x(&editor), start - 9.);
        editor.history.undo(&mut editor.project);
        assert_eq!(x(&editor), start + 1.);
    }

    #[test]
    fn copy_paste_creates_offset_copy_with_fresh_id() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        let id = select_first(&mut editor);
        send(
            &mut editor,
            &ctx,
            vec![egui::Event::Copy],
            egui::Modifiers::COMMAND,
        );
        let copied = serde_json::to_string(&editor.clipboard.nodes[0]).unwrap();
        send(
            &mut editor,
            &ctx,
            vec![egui::Event::Paste(copied)],
            egui::Modifiers::COMMAND,
        );
        let pasted = editor.selected.clone().unwrap();
        assert_ne!(pasted, id);
        let original = editor
            .project
            .nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap()
            .clone();
        let copy = editor
            .project
            .nodes
            .iter()
            .find(|n| n.id == pasted)
            .unwrap();
        assert_eq!(copy.transform.x, original.transform.x + 10.);
        assert_eq!(copy.kind, original.kind);
        // Plain text in the clipboard falls back to the internal shape clipboard.
        send(
            &mut editor,
            &ctx,
            vec![egui::Event::Paste("hello".into())],
            egui::Modifiers::COMMAND,
        );
        assert_ne!(editor.selected.as_deref(), Some(pasted.as_str()));
    }

    #[test]
    fn letter_keys_switch_tools_and_add_shapes_but_not_while_typing() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        key(&mut editor, &ctx, egui::Key::B, egui::Modifiers::NONE);
        assert_eq!(editor.canvas_tool(), CanvasTool::Freehand);
        key(&mut editor, &ctx, egui::Key::V, egui::Modifiers::NONE);
        assert_eq!(editor.canvas_tool(), CanvasTool::Selection);
        let count = editor.project.nodes.len();
        key(&mut editor, &ctx, egui::Key::R, egui::Modifiers::NONE);
        assert_eq!(editor.project.nodes.len(), count + 1);
        assert!(editor.selected.is_some());
        key(&mut editor, &ctx, egui::Key::Escape, egui::Modifiers::NONE);
        assert!(editor.selected.is_none());
    }

    #[test]
    fn locked_shapes_are_not_deleted_or_moved() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        let id = select_first(&mut editor);
        editor
            .project
            .nodes
            .iter_mut()
            .find(|n| n.id == id)
            .unwrap()
            .locked = true;
        let before = editor.project.clone();
        key(&mut editor, &ctx, egui::Key::Delete, egui::Modifiers::NONE);
        key(&mut editor, &ctx, egui::Key::ArrowUp, egui::Modifiers::NONE);
        assert_eq!(editor.project, before);
    }

    fn unlocked_ids(editor: &Editor) -> Vec<String> {
        editor
            .project
            .nodes
            .iter()
            .filter(|n| !n.locked)
            .map(|n| n.id.clone())
            .collect()
    }

    #[test]
    fn select_all_group_and_multi_delete_with_one_undo() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        let count = editor.project.nodes.len();
        key(&mut editor, &ctx, egui::Key::A, egui::Modifiers::COMMAND);
        let all = unlocked_ids(&editor);
        assert_eq!(editor.selection().len(), all.len());
        assert!(all.len() >= 2);
        key(&mut editor, &ctx, egui::Key::G, egui::Modifiers::COMMAND);
        let group = editor.project.nodes[0].group.clone().unwrap();
        assert!(editor
            .project
            .nodes
            .iter()
            .filter(|n| !n.locked)
            .all(|n| n.group.as_ref() == Some(&group)));
        // Clicking one member selects the whole group; Ctrl isolates.
        editor.set_selection(vec![]);
        editor.pick_shape(&all[0], false, false, false);
        assert_eq!(editor.selection().len(), all.len());
        editor.pick_shape(&all[0], false, true, false);
        assert_eq!(editor.selection(), vec![all[0].clone()]);
        // Shift toggles the second one in.
        editor.pick_shape(&all[1], true, true, false);
        assert_eq!(editor.selection().len(), 2);
        key(&mut editor, &ctx, egui::Key::Delete, egui::Modifiers::NONE);
        assert_eq!(editor.project.nodes.len(), count - 2);
        editor.history.undo(&mut editor.project);
        assert_eq!(editor.project.nodes.len(), count);
    }

    #[test]
    fn multi_nudge_duplicate_and_copy_paste_keep_relative_layout() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        let ids: Vec<String> = unlocked_ids(&editor).into_iter().take(2).collect();
        editor.set_selection(ids.clone());
        let pos = |e: &Editor, id: &str| {
            let t = e
                .project
                .nodes
                .iter()
                .find(|n| n.id == id)
                .unwrap()
                .transform;
            (t.x, t.y)
        };
        let before: Vec<_> = ids.iter().map(|id| pos(&editor, id)).collect();
        key(
            &mut editor,
            &ctx,
            egui::Key::ArrowDown,
            egui::Modifiers::SHIFT,
        );
        for (id, (x, y)) in ids.iter().zip(&before) {
            assert_eq!(pos(&editor, id), (*x, y + 10.));
        }
        let count = editor.project.nodes.len();
        key(&mut editor, &ctx, egui::Key::D, egui::Modifiers::COMMAND);
        assert_eq!(editor.project.nodes.len(), count + 2);
        assert_eq!(editor.selection().len(), 2);
        assert!(editor.selection().iter().all(|id| !ids.contains(id)));
        send(
            &mut editor,
            &ctx,
            vec![egui::Event::Copy],
            egui::Modifiers::COMMAND,
        );
        let json = serde_json::to_string(&editor.clipboard).unwrap();
        send(
            &mut editor,
            &ctx,
            vec![egui::Event::Paste(json)],
            egui::Modifiers::COMMAND,
        );
        assert_eq!(editor.project.nodes.len(), count + 4);
        editor.project.validate().unwrap();
    }

    #[test]
    fn arrange_shortcuts_flip_and_restack() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        let ids = unlocked_ids(&editor);
        let first = ids[0].clone();
        editor.set_selection(vec![first.clone()]);
        key(&mut editor, &ctx, egui::Key::PageUp, egui::Modifiers::SHIFT);
        assert_eq!(editor.project.nodes.last().unwrap().id, first);
        key(
            &mut editor,
            &ctx,
            egui::Key::PageDown,
            egui::Modifiers::SHIFT,
        );
        assert_eq!(
            editor.project.nodes.iter().find(|n| !n.locked).unwrap().id,
            first
        );
        let scale = |e: &Editor| {
            e.project
                .nodes
                .iter()
                .find(|n| n.id == first)
                .unwrap()
                .transform
                .scale_x
        };
        key(&mut editor, &ctx, egui::Key::H, egui::Modifiers::SHIFT);
        assert_eq!(scale(&editor), -1.);
        // Shift+V flips instead of switching to the selection tool.
        key(&mut editor, &ctx, egui::Key::V, egui::Modifiers::SHIFT);
        assert_eq!(
            editor
                .project
                .nodes
                .iter()
                .find(|n| n.id == first)
                .unwrap()
                .transform
                .scale_y,
            -1.
        );
    }

    #[test]
    fn marquee_selects_touching_visible_shapes() {
        let mut editor = Editor::default();
        let scene = editor.evaluate_scene(&EvaluationContext::default());
        let marquee = selection::Marquee {
            start: Vec2::new(-10., -10.),
            end: Vec2::new(
                editor.project.width as f32 + 10.,
                editor.project.height as f32 + 10.,
            ),
            additive: false,
        };
        editor.finish_marquee(marquee, &scene);
        let visible = editor
            .project
            .nodes
            .iter()
            .filter(|n| n.visible && !n.locked)
            .count();
        assert_eq!(editor.selection().len(), visible);
        let empty = selection::Marquee {
            start: Vec2::new(-500., -500.),
            end: Vec2::new(-400., -400.),
            additive: true,
        };
        editor.finish_marquee(empty, &scene);
        assert_eq!(editor.selection().len(), visible);
    }

    #[test]
    fn boolean_union_replaces_the_selection_with_one_path() {
        let mut editor = Editor::default();
        editor.project.nodes = vec![
            Node::rect("a", 0., 0., 50., 50.),
            Node::rect("b", 25., 25., 50., 50.),
        ];
        editor.project.clips.clear();
        editor.project.state_machines.clear();
        editor.sync_buffers();
        editor.set_selection(vec!["a".into(), "b".into()]);
        editor.apply_arrange(ArrangeOp::Boolean(splinely_document::BooleanOp::Union));
        assert_eq!(editor.project.nodes.len(), 1, "{}", editor.status);
        assert_eq!(editor.selection(), vec![editor.project.nodes[0].id.clone()]);
        editor.history.undo(&mut editor.project);
        assert_eq!(editor.project.nodes.len(), 2);
    }

    #[test]
    fn generated_shapes_are_added_as_paths() {
        let mut editor = Editor::default();
        let count = editor.project.nodes.len();
        for shape in [
            NewShape::Polygon,
            NewShape::Triangle,
            NewShape::Star,
            NewShape::Line,
        ] {
            editor.add_shape(shape);
        }
        assert_eq!(editor.project.nodes.len(), count + 4);
        assert!(editor.project.nodes[count..]
            .iter()
            .all(|n| matches!(n.kind, NodeKind::Path { .. })));
        assert_eq!(editor.project.nodes.last().unwrap().name, "Linie");
    }

    #[test]
    fn shortcuts_are_ignored_while_a_dialog_is_open() {
        let ctx = egui::Context::default();
        let mut editor = Editor {
            close_prompt: true,
            ..Editor::default()
        };
        let count = editor.project.nodes.len();
        key(&mut editor, &ctx, egui::Key::R, egui::Modifiers::NONE);
        key(&mut editor, &ctx, egui::Key::A, egui::Modifiers::COMMAND);
        assert_eq!(editor.project.nodes.len(), count);
        assert!(editor.selection().is_empty());
        editor.close_prompt = false;
        key(&mut editor, &ctx, egui::Key::R, egui::Modifiers::NONE);
        assert_eq!(editor.project.nodes.len(), count + 1);
    }

    #[test]
    fn morph_creates_a_clip_and_previews_the_target_shape() {
        let mut editor = Editor::default();
        editor.project.nodes = vec![
            Node::rect("a", 0., 0., 50., 50.),
            Node::star("b", 200., 200., 40., 5, 0.5).unwrap(),
        ];
        editor.project.clips.clear();
        editor.project.state_machines.clear();
        editor.sync_buffers();
        editor.set_selection(vec!["a".into(), "b".into()]);
        editor.apply_arrange(ArrangeOp::Morph);
        assert_eq!(editor.project.clips.len(), 1, "{}", editor.status);
        assert!(editor.timeline_visible);
        assert!(!editor.project.nodes[1].visible);
        // The editor preview at the clip end shows the star outline at its place.
        editor.preview_animation = true;
        editor.time = 1.;
        let node = editor
            .evaluated()
            .into_iter()
            .find(|n| n.id == "a")
            .unwrap();
        let bounds = splinely_document::transformed_bounds(&node);
        assert!((bounds[0] + bounds[2] / 2. - 200.).abs() < 1., "{bounds:?}");
        editor.history.undo(&mut editor.project);
        assert!(editor.project.clips.is_empty());
    }

    #[test]
    fn repeated_duplicate_repeats_the_last_move() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        editor.project.nodes = vec![Node::rect("a", 0., 0., 10., 10.)];
        editor.project.clips.clear();
        editor.project.state_machines.clear();
        editor.sync_buffers();
        editor.set_selection(vec!["a".into()]);
        key(&mut editor, &ctx, egui::Key::D, egui::Modifiers::COMMAND);
        let copy = editor.selection();
        // Move the copy to x = 50 (40 further), then duplicate again.
        let mut next = editor.project.clone();
        next.translate_nodes(&copy, 40., -10.).unwrap();
        editor.commit(next);
        key(&mut editor, &ctx, egui::Key::D, egui::Modifiers::COMMAND);
        let third = editor.selection();
        let b = editor.project.bounds_of(&third).unwrap();
        assert_eq!((b[0], b[1]), (100., 0.), "same step as from a to the copy");
        // A fresh selection falls back to the small default offset.
        editor.set_selection(vec!["a".into()]);
        key(&mut editor, &ctx, egui::Key::D, egui::Modifiers::COMMAND);
        let b = editor.project.bounds_of(&editor.selection()).unwrap();
        assert_eq!((b[0], b[1]), (10., 10.));
    }

    #[test]
    fn zoom_at_keeps_document_point_under_cursor() {
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.));
        let document = Vec2::new(400., 300.);
        let cursor = Pos2::new(123., 456.);
        let pan = Vec2::new(17., -5.);
        let before = (cursor - canvas_origin(viewport, document, 1.5, pan)) / 1.5;
        let next = zoom_at(viewport, document, 1.5, 6., pan, cursor);
        let after = (cursor - canvas_origin(viewport, document, 6., next)) / 6.;
        assert!((before - after).length() < 1e-3);
    }
}
