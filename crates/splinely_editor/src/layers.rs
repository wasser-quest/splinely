//! Layer controls independent of the application and its undo/history policy.
use eframe::egui;
use splinely_document::Project;
use splinely_editor::i18n::{tr, trf};

#[derive(Clone, Debug)]
pub(crate) enum LayerCommand {
    Visibility {
        id: String,
        visible: bool,
    },
    Lock {
        id: String,
        locked: bool,
    },
    Raise(String),
    Lower(String),
    Duplicate(String),
    Delete(String),
    /// Show/hide or lock/unlock every node inside a group (any depth).
    GroupFlags {
        group: String,
        visible: Option<bool>,
        locked: Option<bool>,
    },
    RenameGroup {
        group: String,
        name: String,
    },
}

/// Apply one command atomically. A duplicated node gets copies of its animation
/// tracks; state-machine references continue to refer to the original node.
/// Deletion never silently strips tracks, events or state-machine actions.
pub(crate) fn apply_layer_command(
    project: &mut Project,
    selection: &mut Option<String>,
    command: LayerCommand,
) -> Result<(), String> {
    let mut next = project.clone();
    match &command {
        LayerCommand::GroupFlags {
            group,
            visible,
            locked,
        } => {
            let members = next.nodes_in_group(group);
            for node in next.nodes.iter_mut().filter(|n| members.contains(&n.id)) {
                if let Some(v) = visible {
                    node.visible = *v;
                }
                if let Some(l) = locked {
                    node.locked = *l;
                }
            }
            next.validate()
                .map_err(|e| trf("Ebenenänderung abgelehnt: {e}", &[format!("{e}")]))?;
            *project = next;
            return Ok(());
        }
        LayerCommand::RenameGroup { group, name } => {
            next.rename_group(group, name).map_err(|e| e.to_string())?;
            *project = next;
            return Ok(());
        }
        _ => {}
    }
    let id = match &command {
        LayerCommand::Visibility { id, .. } | LayerCommand::Lock { id, .. } => id,
        LayerCommand::Raise(id)
        | LayerCommand::Lower(id)
        | LayerCommand::Duplicate(id)
        | LayerCommand::Delete(id) => id,
        LayerCommand::GroupFlags { .. } | LayerCommand::RenameGroup { .. } => unreachable!(),
    };
    let index = next
        .nodes
        .iter()
        .position(|n| &n.id == id)
        .ok_or_else(|| trf("Ebene {id} fehlt", std::slice::from_ref(id)))?;
    if next.nodes[index].locked && !matches!(command, LayerCommand::Lock { .. }) {
        return Err(tr("Ebene zuerst entsperren").into());
    }
    let mut selected = selection.clone();
    match command {
        LayerCommand::Visibility { visible, .. } => next.nodes[index].visible = visible,
        LayerCommand::Lock { locked, .. } => next.nodes[index].locked = locked,
        LayerCommand::Raise(_) => {
            if index + 1 < next.nodes.len() {
                next.nodes.swap(index, index + 1);
            }
        }
        LayerCommand::Lower(_) => {
            if index > 0 {
                next.nodes.swap(index, index - 1);
            }
        }
        LayerCommand::Duplicate(id) => {
            let mut copy = next.nodes[index].clone();
            // Project validation uses a shared ID namespace, not only node IDs.
            let used = |id: &str| {
                id == "splinely-project"
                    || next.nodes.iter().any(|n| n.id == id)
                    || next.bones.iter().any(|n| n.id == id)
                    || next.clips.iter().any(|n| n.id == id)
                    || next.state_machines.iter().any(|n| n.id == id)
                    || next.audio_assets.iter().any(|n| n.id == id)
                    || next.audio_cues.iter().any(|n| n.id == id)
            };
            let mut suffix = 1_u64;
            let new_id = loop {
                let candidate = format!("{id}-copy-{suffix}");
                if !used(&candidate) {
                    break candidate;
                }
                suffix = suffix
                    .checked_add(1)
                    .ok_or(tr("Keine freie ID für die Kopie"))?;
            };
            copy.id = new_id.clone();
            if !copy.name.is_empty() {
                copy.name.push_str(tr(" Kopie"));
            }
            next.nodes.insert(index + 1, copy);
            for clip in &mut next.clips {
                let copies: Vec<_> = clip
                    .tracks
                    .iter()
                    .filter(|t| t.node_id == id)
                    .cloned()
                    .map(|mut t| {
                        t.node_id = new_id.clone();
                        t
                    })
                    .collect();
                clip.tracks.extend(copies);
            }
            selected = Some(new_id);
        }
        LayerCommand::Delete(id) => {
            next.nodes.remove(index);
            next.prune_empty_groups();
            if selected.as_deref() == Some(&id) {
                selected = None;
            }
        }
        LayerCommand::GroupFlags { .. } | LayerCommand::RenameGroup { .. } => unreachable!(),
    }
    next.validate()
        .map_err(|e| trf("Ebenenänderung abgelehnt: {e}", &[format!("{e}")]))?;
    *project = next;
    *selection = selected;
    Ok(())
}

/// Topmost layer first. Returns true only for a committed document change;
/// selection-only changes do not create undo entries.
#[allow(dead_code)]
pub(crate) fn layers_ui(
    project: &mut Project,
    ui: &mut egui::Ui,
    selection: &mut Option<String>,
) -> bool {
    layers_ui_multi(project, ui, selection, &mut vec![])
}

/// Layer list with multi-selection: Shift/Ctrl+click toggles a layer in `multi`.
pub(crate) fn layers_ui_multi(
    project: &mut Project,
    ui: &mut egui::Ui,
    selection: &mut Option<String>,
    multi: &mut Vec<String>,
) -> bool {
    let error_id = ui.make_persistent_id("layers_error");
    let filter_id = ui.make_persistent_id("layers_filter");
    let mut command = None;
    ui.horizontal(|ui| {
        ui.heading(tr("Ebenen"));
        ui.weak(project.nodes.len().to_string());
    });
    let mut filter = ui
        .ctx()
        .data(|d| d.get_temp::<String>(filter_id))
        .unwrap_or_default();
    if project.nodes.len() > 8 {
        ui.add(egui::TextEdit::singleline(&mut filter).hint_text(tr("🔍 Ebenen filtern")));
        ui.ctx()
            .data_mut(|d| d.insert_temp(filter_id, filter.clone()));
    }
    let needle = filter.to_lowercase();
    let last_id = ui.make_persistent_id("layers_last_selection");
    let last = ui
        .ctx()
        .data(|d| d.get_temp::<Option<String>>(last_id))
        .flatten();
    let scroll_to_selection = last != *selection;
    ui.ctx()
        .data_mut(|d| d.insert_temp(last_id, selection.clone()));
    let row_height = ui.spacing().interact_size.y;
    // Topmost first; only visible rows are laid out (large rigs have 100+ layers).
    // Without a filter the list is a tree: group headers with collapsible members.
    let collapsed_id = ui.make_persistent_id("layers_collapsed");
    let mut collapsed: Vec<String> = ui
        .ctx()
        .data(|d| d.get_temp::<Vec<String>>(collapsed_id))
        .unwrap_or_default();
    let rows = layer_rows(project, &needle, &collapsed);
    let spacing = ui.spacing().item_spacing.y;
    let mut area = egui::ScrollArea::vertical()
        .id_salt("layers_list")
        .max_height((row_height + spacing) * 9.)
        .auto_shrink([true, true]);
    if scroll_to_selection {
        if let Some(index) = rows.iter().position(
            |r| matches!(r, Row::Node { node, .. } if selection.as_deref() == Some(&node.id)),
        ) {
            area =
                area.vertical_scroll_offset(((index as f32) - 3.).max(0.) * (row_height + spacing));
        }
    }
    let rename_id = ui.make_persistent_id("layers_renaming");
    // (group, draft name, focus already requested)
    let mut renaming: Option<(String, String, bool)> = ui.ctx().data(|d| d.get_temp(rename_id));
    area.show_rows(ui, row_height, rows.len(), |ui, range| {
        for row in &rows[range] {
            match row {
                Row::Group {
                    id,
                    name,
                    depth,
                    folded,
                } => {
                    let members = project.nodes_in_group(id);
                    let all_visible = members
                        .iter()
                        .all(|m| project.nodes.iter().any(|n| &n.id == m && n.visible));
                    let any_locked = members
                        .iter()
                        .any(|m| project.nodes.iter().any(|n| &n.id == m && n.locked));
                    let is_selected = !members.is_empty()
                        && members
                            .iter()
                            .all(|m| selection.as_deref() == Some(m.as_str()) || multi.contains(m));
                    ui.push_id(("group-row", id), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.;
                            ui.add_space(*depth as f32 * 12.);
                            let eye = if all_visible { "👁" } else { "–" };
                            if ui
                                .add(
                                    egui::Button::new(eye)
                                        .frame(false)
                                        .min_size(egui::vec2(22., 0.)),
                                )
                                .on_hover_text(tr("Ganze Gruppe ein-/ausblenden"))
                                .clicked()
                            {
                                command = Some(LayerCommand::GroupFlags {
                                    group: id.clone(),
                                    visible: Some(!all_visible),
                                    locked: None,
                                });
                            }
                            let lock =
                                egui::RichText::new(if any_locked { "🔒" } else { "🔓" }).color(
                                    if any_locked {
                                        ui.visuals().warn_fg_color
                                    } else {
                                        ui.visuals().weak_text_color()
                                    },
                                );
                            if ui
                                .add(
                                    egui::Button::new(lock)
                                        .frame(false)
                                        .min_size(egui::vec2(22., 0.)),
                                )
                                .on_hover_text(tr("Ganze Gruppe sperren/entsperren"))
                                .clicked()
                            {
                                command = Some(LayerCommand::GroupFlags {
                                    group: id.clone(),
                                    visible: None,
                                    locked: Some(!any_locked),
                                });
                            }
                            if ui
                                .add(
                                    egui::Button::new(if *folded { "›" } else { "⏷" }).frame(false),
                                )
                                .on_hover_text(if *folded {
                                    tr("Aufklappen")
                                } else {
                                    tr("Zuklappen")
                                })
                                .clicked()
                            {
                                if *folded {
                                    collapsed.retain(|c| c != id);
                                } else {
                                    collapsed.push(id.clone());
                                }
                            }
                            if let Some((_, text, focused)) =
                                renaming.as_mut().filter(|(g, _, _)| g == id)
                            {
                                let response = ui.add(
                                    egui::TextEdit::singleline(text)
                                        .desired_width(ui.available_width() - 16.),
                                );
                                // Request focus once; afterwards losing it (Enter,
                                // Escape or a click elsewhere) ends the edit.
                                if !*focused {
                                    response.request_focus();
                                    *focused = true;
                                } else if response.lost_focus() {
                                    if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                        command = Some(LayerCommand::RenameGroup {
                                            group: id.clone(),
                                            name: text.trim().to_owned(),
                                        });
                                    }
                                    renaming = None;
                                }
                                return;
                            }
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(
                                    (ui.available_width()
                                        - ui.spacing().scroll.allocated_width()
                                        - 8.)
                                        .max(40.),
                                    row_height,
                                ),
                                egui::Sense::click(),
                            );
                            let visuals = ui.visuals();
                            if is_selected {
                                ui.painter().rect_filled(
                                    rect,
                                    3.,
                                    visuals.selection.bg_fill.gamma_multiply(0.6),
                                );
                            } else if response.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    3.,
                                    visuals.widgets.hovered.weak_bg_fill,
                                );
                            }
                            let label = if name.is_empty() {
                                id.as_str()
                            } else {
                                name.as_str()
                            };
                            ui.painter().with_clip_rect(rect).text(
                                rect.left_center() + egui::vec2(6., 0.),
                                egui::Align2::LEFT_CENTER,
                                trf(
                                    "▣  {label}  ({})",
                                    &[label.to_string(), format!("{}", members.len())],
                                ),
                                egui::TextStyle::Body.resolve(ui.style()),
                                if is_selected {
                                    visuals.selection.stroke.color
                                } else {
                                    visuals.strong_text_color()
                                },
                            );
                            let response = response.on_hover_text(tr(
                                "Klick: ganze Gruppe wählen · Doppelklick: umbenennen",
                            ));
                            if response.double_clicked() {
                                renaming = Some((id.clone(), name.clone(), false));
                            } else if response.clicked() && !members.is_empty() {
                                let toggle = ui.input(|i| i.modifiers.shift || i.modifiers.command);
                                let mut next: Vec<String> = if toggle {
                                    selection
                                        .iter()
                                        .cloned()
                                        .chain(multi.iter().cloned())
                                        .collect()
                                } else {
                                    vec![]
                                };
                                for m in &members {
                                    if !next.contains(m) {
                                        next.push(m.clone());
                                    }
                                }
                                let mut next = next.into_iter();
                                *selection = next.next();
                                *multi = next.collect();
                            }
                        });
                    });
                }
                Row::Node { node, depth } => {
                    let node = *node;
                    let depth = *depth;
                    let is_selected = selection.as_deref() == Some(&node.id)
                        || (selection.is_some() && multi.contains(&node.id));
                    ui.push_id(&node.id, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.;
                            ui.add_space(depth as f32 * 12.);
                            let eye = if node.visible { "👁" } else { "–" };
                            if ui
                                .add_enabled(
                                    !node.locked,
                                    egui::Button::new(eye)
                                        .frame(false)
                                        .min_size(egui::vec2(22., 0.)),
                                )
                                .on_hover_text(if node.visible {
                                    tr("Ausblenden")
                                } else {
                                    tr("Einblenden")
                                })
                                .clicked()
                            {
                                command = Some(LayerCommand::Visibility {
                                    id: node.id.clone(),
                                    visible: !node.visible,
                                });
                            }
                            let lock = if node.locked { "🔒" } else { "🔓" };
                            let lock_text = egui::RichText::new(lock).color(if node.locked {
                                ui.visuals().warn_fg_color
                            } else {
                                ui.visuals().weak_text_color()
                            });
                            if ui
                                .add(
                                    egui::Button::new(lock_text)
                                        .frame(false)
                                        .min_size(egui::vec2(22., 0.)),
                                )
                                .on_hover_text(if node.locked {
                                    tr("Entsperren")
                                } else {
                                    tr("Sperren (vor versehentlichem Bearbeiten schützen)")
                                })
                                .clicked()
                            {
                                command = Some(LayerCommand::Lock {
                                    id: node.id.clone(),
                                    locked: !node.locked,
                                });
                            }
                            let icon = match node.kind {
                                splinely_document::NodeKind::Rect { .. } => "⬜",
                                splinely_document::NodeKind::Circle { .. } => "○",
                                splinely_document::NodeKind::Text { .. } => "T",
                                splinely_document::NodeKind::Path { .. } => "〰",
                            };
                            let name = if node.name.is_empty() {
                                &node.id
                            } else {
                                &node.name
                            };
                            let (rect, response) = ui.allocate_exact_size(
                                // Leave room for the scrollbar: filling it would make the
                                // resizable side panel grow a little every frame.
                                egui::vec2(
                                    (ui.available_width()
                                        - ui.spacing().scroll.allocated_width()
                                        - 8.)
                                        .max(40.),
                                    row_height,
                                ),
                                egui::Sense::click(),
                            );
                            let visuals = ui.visuals();
                            if is_selected {
                                ui.painter()
                                    .rect_filled(rect, 3., visuals.selection.bg_fill);
                            } else if response.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    3.,
                                    visuals.widgets.hovered.weak_bg_fill,
                                );
                            }
                            let color = if is_selected {
                                visuals.selection.stroke.color
                            } else if node.visible {
                                visuals.text_color()
                            } else {
                                visuals.weak_text_color()
                            };
                            ui.painter().with_clip_rect(rect).text(
                                rect.left_center() + egui::vec2(6., 0.),
                                egui::Align2::LEFT_CENTER,
                                format!("{icon}  {name}"),
                                egui::TextStyle::Body.resolve(ui.style()),
                                color,
                            );
                            let response = response
                                .on_hover_text(trf("ID: {}", std::slice::from_ref(&node.id)));
                            if response.clicked() {
                                let toggle = ui.input(|i| i.modifiers.shift || i.modifiers.command);
                                let id = node.id.clone();
                                if !toggle || selection.is_none() {
                                    *selection = Some(id);
                                    multi.clear();
                                } else if selection.as_ref() == Some(&id) {
                                    *selection = if multi.is_empty() {
                                        None
                                    } else {
                                        Some(multi.remove(0))
                                    };
                                } else if let Some(index) = multi.iter().position(|m| m == &id) {
                                    multi.remove(index);
                                } else {
                                    multi.push(id);
                                }
                            }
                        });
                    });
                }
            }
        }
    });
    ui.ctx().data_mut(|d| {
        d.insert_temp(collapsed_id, collapsed);
        match renaming {
            Some(r) => d.insert_temp(rename_id, r),
            None => d.remove::<(String, String, bool)>(rename_id),
        }
    });
    if let Some(id) = selection.clone() {
        if let Some(index) = project.nodes.iter().position(|n| n.id == id) {
            let locked = project.nodes[index].locked;
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!locked && index + 1 < project.nodes.len(), egui::Button::new("⬆"))
                    .on_hover_text(tr("Eine Ebene nach vorne"))
                    .clicked()
                {
                    command = Some(LayerCommand::Raise(id.clone()));
                }
                if ui
                    .add_enabled(!locked && index > 0, egui::Button::new("⬇"))
                    .on_hover_text(tr("Eine Ebene nach hinten"))
                    .clicked()
                {
                    command = Some(LayerCommand::Lower(id.clone()));
                }
                if ui
                    .add_enabled(!locked, egui::Button::new(tr("Duplizieren")))
                    .on_hover_text("Strg+D")
                    .clicked()
                {
                    command = Some(LayerCommand::Duplicate(id.clone()));
                }
                if ui
                    .add_enabled(!locked, egui::Button::new("🗑"))
                    .on_hover_text(tr("Ebene löschen · Entf. Ebenen mit Animations- oder Zustandsbezügen über „Form löschen“ entfernen."))
                    .clicked()
                {
                    command = Some(LayerCommand::Delete(id.clone()));
                }
            });
        }
    }
    let mut changed = false;
    if let Some(command) = command {
        let before = project.clone();
        match apply_layer_command(project, selection, command) {
            Ok(()) => {
                changed = *project != before;
                ui.ctx().data_mut(|d| d.remove::<String>(error_id));
            }
            Err(error) => ui.ctx().data_mut(|d| d.insert_temp(error_id, error)),
        }
    }
    if let Some(error) = ui.ctx().data(|d| d.get_temp::<String>(error_id)) {
        ui.colored_label(egui::Color32::RED, error);
    }
    changed
}

/// One row of the layer list.
enum Row<'a> {
    Group {
        id: String,
        name: String,
        depth: usize,
        folded: bool,
    },
    Node {
        node: &'a splinely_document::Node,
        depth: usize,
    },
}

/// Layer rows topmost first. With a filter: matching nodes only, flat.
/// Otherwise group headers open whenever a node's group path changes, and
/// members of collapsed groups are hidden.
fn layer_rows<'a>(project: &'a Project, needle: &str, collapsed: &[String]) -> Vec<Row<'a>> {
    let mut rows = vec![];
    if !needle.is_empty() {
        for node in project.nodes.iter().rev() {
            if node.name.to_lowercase().contains(needle) || node.id.to_lowercase().contains(needle)
            {
                rows.push(Row::Node { node, depth: 0 });
            }
        }
        return rows;
    }
    let mut open: Vec<String> = vec![];
    for node in project.nodes.iter().rev() {
        let path = project.group_path(&node.id);
        let shared = open.iter().zip(&path).take_while(|(a, b)| a == b).count();
        open.truncate(shared);
        for (depth, id) in path.iter().enumerate().skip(shared) {
            let hidden = path[..depth].iter().any(|g| collapsed.contains(g));
            if !hidden {
                let name = project
                    .groups
                    .iter()
                    .find(|g| &g.id == id)
                    .map(|g| g.name.clone())
                    .unwrap_or_default();
                rows.push(Row::Group {
                    id: id.clone(),
                    name,
                    depth,
                    folded: collapsed.contains(id),
                });
            }
            open.push(id.clone());
        }
        if !path.iter().any(|g| collapsed.contains(g)) {
            rows.push(Row::Node {
                node,
                depth: path.len(),
            });
        }
    }
    rows
}

#[cfg(test)]
mod tree_tests {
    use super::*;
    use splinely_document::Node;

    #[test]
    fn rows_nest_groups_and_hide_collapsed_members() {
        let mut p = Project {
            nodes: ["a", "b", "c", "d"]
                .iter()
                .map(|id| Node::rect(*id, 0., 0., 1., 1.))
                .collect(),
            ..Project::default()
        };
        let ids = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let inner = p.group_nodes(&ids(&["a", "b"]), Some("Arm")).unwrap();
        let outer = p
            .group_nodes(&ids(&["a", "b", "c"]), Some("Figur"))
            .unwrap();
        let describe = |rows: &[Row]| {
            rows.iter()
                .map(|r| match r {
                    Row::Group { name, depth, .. } => format!("{depth}:[{name}]"),
                    Row::Node { node, depth } => format!("{depth}:{}", node.id),
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            describe(&layer_rows(&p, "", &[])),
            ["0:d", "0:[Figur]", "1:c", "1:[Arm]", "2:b", "2:a"]
        );
        assert_eq!(
            describe(&layer_rows(&p, "", &[inner])),
            ["0:d", "0:[Figur]", "1:c", "1:[Arm]"]
        );
        assert_eq!(
            describe(&layer_rows(&p, "", &[outer])),
            ["0:d", "0:[Figur]"]
        );
        assert_eq!(describe(&layer_rows(&p, "b", &[])), ["0:b"]);
    }
}

#[cfg(test)]
mod rename_tests {
    use super::*;
    use splinely_document::Node;

    fn frame(
        ctx: &egui::Context,
        project: &mut Project,
        selection: &mut Option<String>,
        multi: &mut Vec<String>,
        events: Vec<egui::Event>,
    ) -> (bool, Vec<(String, egui::Pos2)>) {
        let mut changed = false;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400., 800.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    changed = layers_ui_multi(project, ui, selection, multi);
                });
            },
        );
        let mut texts = vec![];
        for shape in output.shapes {
            if let egui::Shape::Text(t) = shape.shape {
                texts.push((t.galley.text().to_owned(), t.pos + t.galley.size() / 2.));
            }
        }
        (changed, texts)
    }

    #[test]
    fn double_click_renames_a_group() {
        let ctx = egui::Context::default();
        let mut p = Project {
            nodes: vec![
                Node::rect("a", 0., 0., 1., 1.),
                Node::rect("b", 0., 0., 1., 1.),
            ],
            ..Project::default()
        };
        let group = p
            .group_nodes(&["a".into(), "b".into()], Some("Alt"))
            .unwrap();
        let (mut selection, mut multi) = (None, vec![]);
        let (_, texts) = frame(&ctx, &mut p, &mut selection, &mut multi, vec![]);
        let pos = texts
            .iter()
            .find(|(t, _)| t.contains("Alt"))
            .expect("group row")
            .1;
        let click = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        for _ in 0..2 {
            frame(
                &ctx,
                &mut p,
                &mut selection,
                &mut multi,
                vec![egui::Event::PointerMoved(pos), click(true)],
            );
            frame(&ctx, &mut p, &mut selection, &mut multi, vec![click(false)]);
        }
        assert_eq!(
            selection.as_deref(),
            Some("a"),
            "first click selects the group"
        );
        frame(&ctx, &mut p, &mut selection, &mut multi, vec![]);
        frame(
            &ctx,
            &mut p,
            &mut selection,
            &mut multi,
            vec![egui::Event::Text("Neu".into())],
        );
        let (changed, _) = frame(
            &ctx,
            &mut p,
            &mut selection,
            &mut multi,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        let _ = changed;
        let name = &p.groups.iter().find(|g| g.id == group).unwrap().name;
        assert_eq!(name, "AltNeu");
    }
}
