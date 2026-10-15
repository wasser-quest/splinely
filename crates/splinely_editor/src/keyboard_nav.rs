//! Keyboard operation of the drawing area: Tab / Shift+Tab walk through the
//! shapes in draw order, Enter enters a group or edits a text, Escape clears,
//! arrows nudge (see `shortcuts.rs`). The selection model is a set of pure
//! functions over the project so it can be tested without a UI; the `Editor`
//! part below only applies the result, announces it in the status line and
//! paints the keyboard focus ring.
//!
//! Navigation levels: at the top level an item is an ungrouped shape or a
//! whole top-level group (selecting it selects the group like a click does).
//! Enter on a group enters it, Tab then walks its direct children;
//! Shift+Enter leaves the group again.
use super::*;
use splinely_document::{NodeKind, Project};

/// One stop of the Tab order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NavItem {
    Node(String),
    Group(String),
}

/// Only visible, unlocked shapes take part in keyboard navigation.
fn navigable(node: &Node) -> bool {
    node.visible && !node.locked
}

/// The child of `scope` (`None` = top level) that contains `node`.
fn item_of(project: &Project, scope: Option<&str>, node: &Node) -> Option<NavItem> {
    let path = project.group_path(&node.id);
    let below = match scope {
        None => path.first(),
        Some(scope) => path.get(path.iter().position(|g| g == scope)? + 1),
    };
    Some(match below {
        Some(group) => NavItem::Group(group.clone()),
        None => NavItem::Node(node.id.clone()),
    })
}

/// Tab order of a scope: draw order (back to front), a group at the position
/// of its first navigable shape; hidden, locked and empty items are skipped.
pub(super) fn items(project: &Project, scope: Option<&str>) -> Vec<NavItem> {
    let mut result: Vec<NavItem> = vec![];
    for node in project.nodes.iter().filter(|n| navigable(n)) {
        if let Some(item) = item_of(project, scope, node) {
            if !result.contains(&item) {
                result.push(item);
            }
        }
    }
    result
}

/// The item of `scope` that contains the shape `id`, navigable or not.
pub(super) fn locate(project: &Project, scope: Option<&str>, id: &str) -> Option<NavItem> {
    let node = project.nodes.iter().find(|n| n.id == id)?;
    item_of(project, scope, node)
}

/// Next or previous item with wrap-around. Without a (known) current item
/// the first (forward) or last (backward) one is chosen; no items, no result.
pub(super) fn step(
    items: &[NavItem],
    current: Option<&NavItem>,
    backwards: bool,
) -> Option<NavItem> {
    if items.is_empty() {
        return None;
    }
    let last = items.len() - 1;
    let index = match current.and_then(|c| items.iter().position(|i| i == c)) {
        Some(at) if backwards => (at + last) % items.len(),
        Some(at) => (at + 1) % items.len(),
        None if backwards => last,
        None => 0,
    };
    items.get(index).cloned()
}

/// Shape IDs a navigation stop selects, the first navigable one primary.
pub(super) fn item_ids(project: &Project, item: &NavItem) -> Vec<String> {
    match item {
        NavItem::Node(id) => vec![id.clone()],
        NavItem::Group(group) => {
            let mut ids = project.nodes_in_group(group);
            if let Some(first) = ids
                .iter()
                .position(|id| project.nodes.iter().any(|n| &n.id == id && navigable(n)))
            {
                let primary = ids.remove(first);
                ids.insert(0, primary);
            }
            ids
        }
    }
}

/// Parent of a group (`None` = top level).
pub(super) fn parent_group(project: &Project, group: &str) -> Option<String> {
    project
        .groups
        .iter()
        .find(|g| g.id == group)
        .and_then(|g| g.parent_id.clone())
}

/// Document-space step of an arrow key: `small` units, `large` with Shift.
pub(super) fn nudge_delta(key: egui::Key, shift: bool, small: f64, large: f64) -> Option<Vec2> {
    let step = if shift { large } else { small } as f32;
    match key {
        egui::Key::ArrowLeft => Some(Vec2::new(-step, 0.)),
        egui::Key::ArrowRight => Some(Vec2::new(step, 0.)),
        egui::Key::ArrowUp => Some(Vec2::new(0., -step)),
        egui::Key::ArrowDown => Some(Vec2::new(0., step)),
        _ => None,
    }
}

/// Where a shape sits in the document (its anchor plus transform offset).
pub(super) fn node_position(node: &Node) -> (f64, f64) {
    let (x, y) = match &node.kind {
        NodeKind::Rect { x, y, .. } | NodeKind::Text { x, y, .. } => (*x, *y),
        NodeKind::Circle { cx, cy, .. } => (*cx, *cy),
        NodeKind::Path { bounds, .. } => (bounds[0], bounds[1]),
    };
    (x + node.transform.x, y + node.transform.y)
}

fn kind_label(node: &Node) -> &'static str {
    match node.kind {
        NodeKind::Rect { .. } => tr("Rechteck"),
        NodeKind::Circle { .. } => tr("Kreis"),
        NodeKind::Text { .. } => tr("Text"),
        NodeKind::Path { .. } => tr("Pfad"),
    }
}

/// Keyboard state that belongs to the drawing area.
#[derive(Default)]
pub(super) struct KeyboardState {
    /// Widget ID of the canvas, known after its first frame.
    pub canvas_id: Option<egui::Id>,
    /// Group whose children Tab walks through (`None` = top level).
    pub scope: Option<String>,
    /// Draw the focus ring (keyboard use; hidden again by a mouse press).
    pub ring: bool,
    /// The canvas had keyboard focus in the previous frame.
    pub had_focus: bool,
}

impl Editor {
    /// A text field (or another widget that needs keys) has keyboard focus.
    /// The canvas itself having focus does not count: it is operated by keys.
    pub(super) fn typing(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|m| m.focused())
            .is_some_and(|id| Some(id) != self.kbd.canvas_id)
    }

    /// Keyboard navigation applies to editing with the selection tool.
    pub(super) fn canvas_keys_active(&self) -> bool {
        self.tab == Tab::Canvas
            && self.canvas_tool() == CanvasTool::Selection
            && !self.preview_mode
            && self.text_draft.is_none()
            && !self.modal_open()
    }

    fn canvas_focused(&self, ctx: &egui::Context) -> bool {
        self.kbd
            .canvas_id
            .is_some_and(|id| ctx.memory(|m| m.has_focus(id)))
    }

    /// The scope Tab works in; leaving it when the selection moved elsewhere.
    fn nav_scope(&mut self) -> Option<String> {
        let valid = match (&self.kbd.scope, &self.selected) {
            (Some(scope), Some(primary)) => {
                self.project.group_path(primary).iter().any(|g| g == scope)
            }
            _ => false,
        };
        if !valid {
            self.kbd.scope = None;
        }
        self.kbd.scope.clone()
    }

    /// F6 puts keyboard focus on the drawing area (also out of text fields).
    pub(super) fn focus_canvas_key(&mut self, ctx: &egui::Context) {
        if self.modal_open() || self.editing_keymap() || self.palette.is_some() {
            return;
        }
        let f6 = ctx.input(|i| i.key_pressed(egui::Key::F6) && !i.modifiers.any());
        if let (true, Some(id)) = (f6, self.kbd.canvas_id) {
            ctx.memory_mut(|m| m.request_focus(id));
            self.kbd.ring = true;
            self.status = match self.describe_selection() {
                Some(text) => trf("Zeichenfläche fokussiert · {}", &[text]),
                None => tr("Zeichenfläche fokussiert · Tab wählt die nächste Form, F1 zeigt alle Tastenkürzel.").into(),
            };
        }
    }

    /// Tab, Shift+Tab and Enter on the focused drawing area. Escape and the
    /// arrows are handled in `shortcuts()`.
    pub(super) fn keyboard_navigation(&mut self, ctx: &egui::Context) {
        let presses: Vec<(egui::Key, egui::Modifiers)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } if matches!(key, egui::Key::Tab | egui::Key::Enter) => {
                        Some((*key, *modifiers))
                    }
                    _ => None,
                })
                .collect()
        });
        let focused = self.canvas_focused(ctx);
        for (key, modifiers) in presses {
            if modifiers.command || modifiers.alt {
                continue;
            }
            match key {
                // Without canvas focus Tab keeps its normal job: moving focus.
                egui::Key::Tab if focused => self.tab_select(modifiers.shift),
                egui::Key::Enter => self.enter_key(modifiers.shift),
                _ => {}
            }
        }
    }

    fn tab_select(&mut self, backwards: bool) {
        let scope = self.nav_scope();
        let items = items(&self.project, scope.as_deref());
        let current = self
            .selected
            .as_deref()
            .and_then(|id| locate(&self.project, scope.as_deref(), id));
        match step(&items, current.as_ref(), backwards) {
            Some(next) => self.select_item(&next),
            None => self.status = tr("Keine sichtbare, entsperrte Form zum Auswählen.").into(),
        }
    }

    fn select_item(&mut self, item: &NavItem) {
        let ids = item_ids(&self.project, item);
        self.set_selection(ids);
        self.kbd.ring = true;
        if let Some(text) = self.describe_selection() {
            self.status = text;
        }
    }

    fn enter_key(&mut self, leave: bool) {
        let Some(primary) = self.selected.clone() else {
            return;
        };
        let scope = self.nav_scope();
        if leave {
            // Shift+Enter: back out of the group, the group itself stays selected.
            if let Some(group) = scope {
                self.kbd.scope = parent_group(&self.project, &group);
                self.select_item(&NavItem::Group(group));
            }
            return;
        }
        match locate(&self.project, scope.as_deref(), &primary) {
            Some(NavItem::Group(group)) => {
                let inner = items(&self.project, Some(&group));
                match inner.first() {
                    Some(first) => {
                        self.kbd.scope = Some(group);
                        self.select_item(&first.clone());
                    }
                    None => {
                        self.status =
                            tr("Die Gruppe enthält keine sichtbare, entsperrte Form.").into()
                    }
                }
            }
            Some(NavItem::Node(id)) => {
                let is_text = self
                    .project
                    .nodes
                    .iter()
                    .any(|n| n.id == id && matches!(n.kind, NodeKind::Text { .. }));
                if is_text {
                    self.begin_text_draft(&id);
                } else if let Some(text) = self.describe_selection() {
                    self.status = text;
                }
            }
            None => {}
        }
    }

    /// Announcement for the status line (and screen readers): name, type,
    /// position and place in the Tab order of the primary selected shape.
    pub(super) fn describe_selection(&self) -> Option<String> {
        let primary = self.selected.as_deref()?;
        let node = self.project.nodes.iter().find(|n| n.id == primary)?;
        let scope = self
            .kbd
            .scope
            .as_deref()
            .filter(|s| self.project.group_path(primary).iter().any(|g| g == s));
        let all = items(&self.project, scope);
        let place = locate(&self.project, scope, primary)
            .and_then(|item| all.iter().position(|i| *i == item))
            .map(|at| trf("{} von {}", &[(at + 1).to_string(), all.len().to_string()]));
        let place = place.map(|p| format!(" · {p}")).unwrap_or_default();
        if let Some(NavItem::Group(group)) = locate(&self.project, scope, primary) {
            let name = self
                .project
                .groups
                .iter()
                .find(|g| g.id == group)
                .map(|g| if g.name.is_empty() { &g.id } else { &g.name })
                .unwrap_or(&group)
                .clone();
            let count = self.project.nodes_in_group(&group).len();
            return Some(trf(
                "Gruppe {name} · {count} Formen · Enter öffnet die Gruppe{place}",
                &[name, count.to_string(), place],
            ));
        }
        let (x, y) = node_position(node);
        Some(trf(
            "{name} ({kind}) · Position x {x}, y {y}{place}",
            &[
                shortcuts::display_name(node).to_owned(),
                kind_label(node).to_owned(),
                format!("{x:.1}"),
                format!("{y:.1}"),
                place,
            ],
        ))
    }

    /// Canvas side: keep Tab/arrows on the canvas while it is keyboard
    /// operated, give it focus on a mouse press and describe it for AT.
    pub(super) fn canvas_keyboard_setup(&mut self, ui: &egui::Ui, response: &egui::Response) {
        self.kbd.canvas_id = Some(response.id);
        let pressed = response.contains_pointer() && ui.input(|i| i.pointer.any_pressed());
        if pressed {
            response.request_focus();
            self.kbd.ring = false;
        }
        let focused = response.has_focus();
        if focused && !self.kbd.had_focus && !pressed {
            self.kbd.ring = true; // reached by Tab from another control
        }
        if !focused {
            self.kbd.ring = false;
        }
        self.kbd.had_focus = focused;
        if focused && self.canvas_keys_active() {
            let filter = egui::EventFilter {
                tab: true,
                horizontal_arrows: true,
                vertical_arrows: true,
                // The first Escape clears the selection, the next one leaves.
                escape: self.selected.is_some(),
            };
            ui.memory_mut(|m| m.set_focus_lock_filter(response.id, filter));
        }
        let label = match (focused, self.describe_selection()) {
            (true, Some(text)) => format!("{} – {text}", tr("Zeichenfläche")),
            _ => tr("Zeichenfläche").to_owned(),
        };
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, true, label.clone())
        });
    }

    /// High-contrast ring (black under yellow, thicker than the thin teal
    /// selection bounds and handles) around the keyboard-selected shape or
    /// group; an inner frame marks the canvas itself when nothing is selected.
    pub(super) fn paint_keyboard_focus(
        &self,
        painter: &egui::Painter,
        response: &egui::Response,
        origin: Pos2,
        content: Rect,
        scene: &[SceneNode],
    ) {
        if !response.has_focus() || !self.kbd.ring {
            return;
        }
        let selection = self.selection();
        let rects: Vec<Rect> = scene
            .iter()
            .filter(|n| selection.contains(&n.id))
            .map(selection::scene_rect)
            .collect();
        let target = match rects.into_iter().reduce(|a, b| a.union(b)) {
            Some(rect) => Rect::from_min_max(
                origin + rect.min.to_vec2() * self.zoom,
                origin + rect.max.to_vec2() * self.zoom,
            )
            .expand(6.),
            None => content.shrink(3.),
        };
        painter.rect_stroke(
            target,
            2.,
            egui::Stroke::new(5_f32, Color32::BLACK),
            egui::StrokeKind::Middle,
        );
        painter.rect_stroke(
            target,
            2.,
            egui::Stroke::new(2.5_f32, Color32::from_rgb(255, 214, 10)),
            egui::StrokeKind::Middle,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        let mut p = Project::default();
        for id in ["a", "b", "c", "d"] {
            p.nodes.push(Node::circle(id, 10., 20., 5.));
        }
        p
    }

    fn ids(items: &[NavItem]) -> Vec<String> {
        items
            .iter()
            .map(|i| match i {
                NavItem::Node(id) | NavItem::Group(id) => id.clone(),
            })
            .collect()
    }

    #[test]
    fn tab_order_is_draw_order_and_wraps_both_ways() {
        let p = project();
        let all = items(&p, None);
        assert_eq!(ids(&all), ["a", "b", "c", "d"]);
        let b = NavItem::Node("b".into());
        assert_eq!(step(&all, Some(&b), false), Some(NavItem::Node("c".into())));
        assert_eq!(step(&all, Some(&b), true), Some(NavItem::Node("a".into())));
        let d = NavItem::Node("d".into());
        let a = NavItem::Node("a".into());
        assert_eq!(
            step(&all, Some(&d), false),
            Some(a.clone()),
            "wraps forward"
        );
        assert_eq!(step(&all, Some(&a), true), Some(d), "wraps backward");
        assert_eq!(step(&all, None, false), Some(a), "no selection: first");
        assert_eq!(
            step(&all, None, true),
            Some(NavItem::Node("d".into())),
            "no selection backwards: last"
        );
        let unknown = NavItem::Node("gone".into());
        assert_eq!(
            step(&all, Some(&unknown), false),
            Some(NavItem::Node("a".into()))
        );
    }

    #[test]
    fn empty_and_single_documents_are_safe() {
        let mut p = Project::default();
        assert!(items(&p, None).is_empty());
        assert_eq!(step(&[], None, false), None);
        assert_eq!(step(&[], Some(&NavItem::Node("x".into())), true), None);
        p.nodes.push(Node::circle("only", 0., 0., 1.));
        let all = items(&p, None);
        let only = NavItem::Node("only".into());
        assert_eq!(step(&all, Some(&only), false), Some(only.clone()));
        assert_eq!(step(&all, Some(&only), true), Some(only));
    }

    #[test]
    fn hidden_and_locked_shapes_are_skipped() {
        let mut p = project();
        p.nodes[1].visible = false;
        p.nodes[2].locked = true;
        let all = items(&p, None);
        assert_eq!(ids(&all), ["a", "d"]);
        // A hidden/locked current shape (selected with the mouse elsewhere) is not a stop.
        let c = NavItem::Node("c".into());
        assert_eq!(step(&all, Some(&c), false), Some(NavItem::Node("a".into())));
        for node in &mut p.nodes {
            node.locked = true;
        }
        assert!(items(&p, None).is_empty());
    }

    fn grouped() -> Project {
        let mut p = project();
        p.nodes.push(Node::circle("e", 0., 0., 1.));
        p.group_nodes(&["b".into(), "c".into()], Some("Paar"))
            .unwrap();
        p
    }

    #[test]
    fn a_group_is_one_stop_at_its_first_member_and_enter_goes_inside() {
        let p = grouped();
        let top = items(&p, None);
        assert_eq!(top.len(), 4, "a, group(b,c), d, e: {top:?}");
        assert_eq!(top[0], NavItem::Node("a".into()));
        let NavItem::Group(group) = top[1].clone() else {
            panic!("second stop is the group: {top:?}")
        };
        assert_eq!(top[2], NavItem::Node("d".into()));
        assert_eq!(item_ids(&p, &top[1]), ["b", "c"]);
        assert_eq!(ids(&items(&p, Some(&group))), ["b", "c"]);
        assert_eq!(locate(&p, None, "c"), Some(top[1].clone()));
        assert_eq!(
            locate(&p, Some(&group), "c"),
            Some(NavItem::Node("c".into()))
        );
        assert_eq!(locate(&p, Some(&group), "a"), None, "outside the scope");
        assert_eq!(parent_group(&p, &group), None);
    }

    #[test]
    fn nested_groups_are_walked_level_by_level() {
        let mut p = grouped();
        let inner = p.group_path("b")[0].clone();
        let outer = p
            .group_nodes(&["b".into(), "d".into()], Some("Aussen"))
            .map(|_| p.group_path("b")[0].clone())
            .unwrap();
        assert_ne!(inner, outer);
        let top = items(&p, None);
        assert!(top.contains(&NavItem::Group(outer.clone())));
        let inside = items(&p, Some(&outer));
        assert!(
            inside.contains(&NavItem::Group(inner.clone())),
            "{inside:?}"
        );
        assert_eq!(parent_group(&p, &inner).as_deref(), Some(outer.as_str()));
    }

    #[test]
    fn a_group_with_only_locked_or_hidden_members_is_skipped() {
        let mut p = grouped();
        p.nodes[1].locked = true;
        p.nodes[2].visible = false;
        let all = items(&p, None);
        assert!(
            all.iter().all(|i| !matches!(i, NavItem::Group(_))),
            "{all:?}"
        );
        // The primary of a group with one usable member is that member.
        let mut p = grouped();
        p.nodes[1].locked = true;
        let group = p.group_path("c")[0].clone();
        assert_eq!(item_ids(&p, &NavItem::Group(group))[0], "c");
    }

    #[test]
    fn nudge_is_one_unit_or_ten_with_shift() {
        use egui::Key::*;
        assert_eq!(
            nudge_delta(ArrowLeft, false, 1., 10.),
            Some(Vec2::new(-1., 0.))
        );
        assert_eq!(
            nudge_delta(ArrowRight, false, 1., 10.),
            Some(Vec2::new(1., 0.))
        );
        assert_eq!(
            nudge_delta(ArrowUp, false, 1., 10.),
            Some(Vec2::new(0., -1.))
        );
        assert_eq!(
            nudge_delta(ArrowDown, false, 1., 10.),
            Some(Vec2::new(0., 1.))
        );
        assert_eq!(
            nudge_delta(ArrowLeft, true, 1., 10.),
            Some(Vec2::new(-10., 0.))
        );
        assert_eq!(
            nudge_delta(ArrowDown, true, 0.5, 20.),
            Some(Vec2::new(0., 20.))
        );
        assert_eq!(nudge_delta(Tab, false, 1., 10.), None);
    }

    #[test]
    fn position_includes_the_transform_offset() {
        let mut node = Node::circle("c", 10., 20., 5.);
        node.transform.x = 3.;
        node.transform.y = -4.;
        assert_eq!(node_position(&node), (13., 16.));
    }

    // ---- headless egui tests (same style as src/palette.rs ui_tests) ----

    fn frame(editor: &mut Editor, ctx: &egui::Context, events: Vec<egui::Event>, field: bool) {
        ctx.style_mut(|style| style.animation_time = 0.);
        let modifiers = events
            .iter()
            .find_map(|e| match e {
                egui::Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 700.))),
                modifiers,
                events,
                ..Default::default()
            },
            |ctx| {
                editor.shortcuts(ctx);
                if field {
                    egui::TopBottomPanel::top("side").show(ctx, |ui| {
                        let mut text = String::new();
                        ui.add(egui::TextEdit::singleline(&mut text).id(egui::Id::new("other")));
                    });
                }
                egui::CentralPanel::default().show(ctx, |ui| editor.canvas(ui));
                editor.palette_ui(ctx);
            },
        );
    }

    fn press(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
        [true, false]
            .map(|pressed| egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            })
            .to_vec()
    }

    const NONE: egui::Modifiers = egui::Modifiers::NONE;
    const SHIFT: egui::Modifiers = egui::Modifiers {
        shift: true,
        ..egui::Modifiers::NONE
    };

    /// Editor with the canvas focused through F6, as a keyboard user would.
    fn focused(project: Project) -> (Editor, egui::Context) {
        let ctx = egui::Context::default();
        let mut editor = Editor {
            project,
            ..Editor::default()
        };
        editor.set_selection(vec![]);
        frame(&mut editor, &ctx, vec![], false);
        frame(&mut editor, &ctx, press(egui::Key::F6, NONE), false);
        frame(&mut editor, &ctx, vec![], false);
        assert!(editor.canvas_focused(&ctx), "F6 focuses the canvas");
        (editor, ctx)
    }

    #[test]
    fn tab_and_shift_tab_walk_the_shapes_and_announce_them() {
        let (mut editor, ctx) = focused(project());
        for expected in ["a", "b", "c", "d", "a"] {
            frame(&mut editor, &ctx, press(egui::Key::Tab, NONE), false);
            assert_eq!(editor.selected.as_deref(), Some(expected));
        }
        assert!(
            editor.status.contains('a') && editor.status.contains("Position"),
            "status announces the shape: {}",
            editor.status
        );
        frame(&mut editor, &ctx, press(egui::Key::Tab, SHIFT), false);
        assert_eq!(editor.selected.as_deref(), Some("d"), "wraps backwards");
        assert!(editor.canvas_focused(&ctx), "Tab must not leave the canvas");
        frame(&mut editor, &ctx, press(egui::Key::Escape, NONE), false);
        assert_eq!(editor.selected, None, "Escape clears the selection");
        assert!(
            editor.canvas_focused(&ctx),
            "the first Escape stays on the canvas"
        );
    }

    #[test]
    fn tab_on_an_empty_document_does_nothing_harmful() {
        let (mut editor, ctx) = focused(Project::default());
        frame(&mut editor, &ctx, press(egui::Key::Tab, NONE), false);
        assert_eq!(editor.selected, None);
        assert!(editor.status.contains("Form"), "{}", editor.status);
    }

    #[test]
    fn arrows_nudge_one_unit_and_shift_ten() {
        let (mut editor, ctx) = focused(project());
        frame(&mut editor, &ctx, press(egui::Key::Tab, NONE), false);
        let before = node_position(&editor.project.nodes[0]);
        frame(&mut editor, &ctx, press(egui::Key::ArrowRight, NONE), false);
        let one = node_position(&editor.project.nodes[0]);
        assert_eq!(one.0 - before.0, editor.settings.nudge_small);
        frame(&mut editor, &ctx, press(egui::Key::ArrowDown, SHIFT), false);
        let ten = node_position(&editor.project.nodes[0]);
        assert_eq!(ten.1 - one.1, editor.settings.nudge_large);
        assert!(
            editor.status.contains(&format!("{:.1}", ten.0)),
            "{}",
            editor.status
        );
    }

    #[test]
    fn enter_enters_a_group_and_shift_enter_leaves_it() {
        let (mut editor, ctx) = focused(grouped());
        frame(&mut editor, &ctx, press(egui::Key::Tab, NONE), false); // a
        frame(&mut editor, &ctx, press(egui::Key::Tab, NONE), false); // group(b, c)
        assert_eq!(editor.selection().len(), 2);
        frame(&mut editor, &ctx, press(egui::Key::Enter, NONE), false);
        assert_eq!(editor.selected.as_deref(), Some("b"));
        assert_eq!(editor.selection().len(), 1);
        frame(&mut editor, &ctx, press(egui::Key::Tab, NONE), false);
        assert_eq!(editor.selected.as_deref(), Some("c"));
        frame(&mut editor, &ctx, press(egui::Key::Tab, NONE), false);
        assert_eq!(
            editor.selected.as_deref(),
            Some("b"),
            "Tab stays inside the group"
        );
        frame(&mut editor, &ctx, press(egui::Key::Enter, SHIFT), false);
        assert_eq!(editor.selection().len(), 2, "the group is selected again");
        frame(&mut editor, &ctx, press(egui::Key::Tab, NONE), false);
        assert_eq!(editor.selected.as_deref(), Some("d"));
    }

    #[test]
    fn enter_on_a_text_starts_editing_like_a_double_click() {
        let mut p = Project::default();
        p.nodes.push(Node::text("t", 5., 5., "Hallo"));
        let (mut editor, ctx) = focused(p);
        frame(&mut editor, &ctx, press(egui::Key::Tab, NONE), false);
        assert!(editor.text_draft.is_none());
        frame(&mut editor, &ctx, press(egui::Key::Enter, NONE), false);
        let draft = editor.text_draft.as_ref().expect("text editor opened");
        assert_eq!((draft.id.as_str(), draft.text.as_str()), ("t", "Hallo"));
    }

    #[test]
    fn keys_are_ignored_while_a_text_field_has_focus() {
        let (mut editor, ctx) = focused(project());
        frame(&mut editor, &ctx, press(egui::Key::Tab, NONE), false);
        frame(&mut editor, &ctx, vec![], true);
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("other")));
        frame(&mut editor, &ctx, vec![], true);
        assert!(editor.typing(&ctx));
        let before = node_position(&editor.project.nodes[0]);
        frame(&mut editor, &ctx, press(egui::Key::ArrowRight, NONE), true);
        assert_eq!(
            node_position(&editor.project.nodes[0]),
            before,
            "no nudge while typing"
        );
        let before = editor.selected.clone();
        frame(&mut editor, &ctx, vec![egui::Event::Text("?".into())], true);
        assert!(!editor.shortcuts_open, "'?' is text in a text field");
        assert_eq!(editor.selected, before);
    }

    #[test]
    fn question_mark_and_the_help_button_open_the_shortcut_window() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        frame(
            &mut editor,
            &ctx,
            vec![egui::Event::Text("?".into())],
            false,
        );
        assert!(editor.shortcuts_open);
        let rows = editor.shortcut_rows();
        let has = |keys: &str| rows.iter().any(|(k, _)| k == keys);
        assert!(has("Tab / Umschalt+Tab") || rows.iter().any(|(k, _)| k.contains("Tab")));
        assert!(rows.iter().any(|(k, _)| k == "F1" || k.contains("F1")));
        // Generated from the live key bindings: every bound command is listed.
        for (cmd, keys) in editor.bindings() {
            if let Some(first) = keys.first() {
                assert!(
                    rows.iter().any(|(k, what)| k.contains(&first.label())
                        && *what == palette::command_label(&cmd)),
                    "{cmd:?} missing in the overview"
                );
            }
        }
    }
}
