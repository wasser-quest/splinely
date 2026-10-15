//! Undo history window: every step with a generated name; click a step to
//! jump there (undoing or redoing as many steps as needed).
use super::*;

fn node_name(node: &Node) -> String {
    if node.name.is_empty() {
        node.id.clone()
    } else {
        node.name.clone()
    }
}

/// Short German description of what changed between two documents.
pub(super) fn describe_change(before: &Project, after: &Project) -> String {
    let added: Vec<&Node> = after
        .nodes
        .iter()
        .filter(|n| !before.nodes.iter().any(|b| b.id == n.id))
        .collect();
    let removed: Vec<&Node> = before
        .nodes
        .iter()
        .filter(|n| !after.nodes.iter().any(|a| a.id == n.id))
        .collect();
    match (added.len(), removed.len()) {
        (1, 0) => return trf("„{}“ hinzugefügt", &[node_name(added[0]).to_string()]),
        (n, 0) if n > 1 => return trf("{n} Formen hinzugefügt", &[format!("{n}")]),
        (0, 1) => return trf("„{}“ gelöscht", &[node_name(removed[0]).to_string()]),
        (0, n) if n > 1 => return trf("{n} Formen gelöscht", &[format!("{n}")]),
        (a, r) if a > 0 && r > 0 => {
            return trf(
                "{r} Formen zu {a} kombiniert",
                &[format!("{r}"), format!("{a}")],
            )
        }
        _ => {}
    }
    if before.clips != after.clips {
        return tr("Animation geändert").into();
    }
    if before.groups != after.groups {
        return tr("Gruppen geändert").into();
    }
    if before.guides != after.guides {
        return tr("Hilfslinien geändert").into();
    }
    let order_changed = before
        .nodes
        .iter()
        .map(|n| &n.id)
        .ne(after.nodes.iter().map(|n| &n.id));
    let changed: Vec<(&Node, &Node)> = after
        .nodes
        .iter()
        .filter_map(|a| before.nodes.iter().find(|b| b.id == a.id).map(|b| (b, a)))
        .filter(|(b, a)| b != a)
        .collect();
    if changed.is_empty() {
        return if order_changed {
            tr("Reihenfolge geändert").into()
        } else if before.state_machines != after.state_machines {
            tr("Zustände geändert").into()
        } else if before.bones != after.bones {
            tr("Skelett geändert").into()
        } else if before.name != after.name
            || before.width != after.width
            || before.height != after.height
        {
            tr("Projekt geändert").into()
        } else {
            tr("Änderung").into()
        };
    }
    let what = {
        let (b, a) = changed[0];
        if b.transform != a.transform {
            "verschoben/transformiert"
        } else if b.style != a.style || b.gradient != a.gradient {
            tr("Darstellung geändert")
        } else if b.kind != a.kind {
            tr("Form geändert")
        } else if b.visible != a.visible || b.locked != a.locked {
            tr("Sichtbarkeit/Sperre geändert")
        } else if b.name != a.name {
            "umbenannt"
        } else {
            tr("geändert")
        }
    };
    if changed.len() == 1 {
        trf(
            "„{}“ {what}",
            &[node_name(changed[0].1).to_string(), what.to_string()],
        )
    } else {
        trf(
            "{} Formen {what}",
            &[format!("{}", changed.len()), what.to_string()],
        )
    }
}

impl Editor {
    /// Undo or redo until `done` steps are applied.
    pub(super) fn jump_history(&mut self, done: usize) {
        self.finish_drag();
        self.finish_edit();
        let mut changed = false;
        loop {
            let current = self.history.labels().0.len();
            let moved = if current > done {
                self.history.undo(&mut self.project)
            } else if current < done {
                self.history.redo(&mut self.project)
            } else {
                break;
            };
            if !moved {
                break;
            }
            changed = true;
        }
        if changed {
            self.dirty = true;
            self.sync_buffers();
        }
    }

    pub(super) fn history_window(&mut self, ctx: &egui::Context) {
        let mut open = self.history_open;
        let mut jump = None;
        egui::Window::new(tr("Verlauf"))
            .open(&mut open)
            .default_width(260.)
            .resizable(true)
            .show(ctx, |ui| {
                let (done, undone) = self.history.labels();
                egui::ScrollArea::vertical()
                    .max_height(360.)
                    .show(ui, |ui| {
                        if ui
                            .selectable_label(done.is_empty(), tr("Ausgangszustand"))
                            .clicked()
                        {
                            jump = Some(0);
                        }
                        for (i, label) in done.iter().enumerate() {
                            let text = if label.is_empty() {
                                tr("Änderung")
                            } else {
                                label
                            };
                            let current = i + 1 == done.len();
                            if ui
                                .selectable_label(current, format!("{}. {text}", i + 1))
                                .clicked()
                            {
                                jump = Some(i + 1);
                            }
                        }
                        for (i, label) in undone.iter().enumerate() {
                            let text = if label.is_empty() {
                                tr("Änderung")
                            } else {
                                label
                            };
                            let number = done.len() + i + 1;
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(format!("{number}. {text}")).weak(),
                                    )
                                    .frame(false),
                                )
                                .on_hover_text(tr(
                                    "Rückgängig gemacht · Klick stellt bis hierhin wieder her",
                                ))
                                .clicked()
                            {
                                jump = Some(number);
                            }
                        }
                    });
                ui.weak(tr("Klick auf einen Schritt springt dorthin."));
            });
        self.history_open = open;
        if let Some(done) = jump {
            self.jump_history(done);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_get_names_and_jumping_moves_through_them() {
        let mut editor = Editor::default();
        editor.project.nodes = vec![Node::rect("a", 0., 0., 10., 10.)];
        editor.project.clips.clear();
        editor.project.state_machines.clear();
        editor.history.clear();
        editor.add_shape(shortcuts::NewShape::Star);
        let mut next = editor.project.clone();
        next.nodes[0].transform.x = 5.;
        editor.commit(next);
        let mut next = editor.project.clone();
        next.nodes[0].style.fill = Color::rgb(255, 0, 0);
        editor.commit(next);
        let (done, undone) = editor.history.labels();
        assert_eq!(
            done,
            vec![
                "„Stern“ hinzugefügt",
                "„a“ verschoben/transformiert",
                "„a“ Darstellung geändert"
            ]
        );
        assert!(undone.is_empty());
        editor.jump_history(1);
        assert_eq!(editor.project.nodes.len(), 2);
        assert_eq!(editor.project.nodes[0].transform.x, 0.);
        let (done, undone) = editor.history.labels();
        assert_eq!(done.len(), 1);
        assert_eq!(
            undone,
            vec!["„a“ verschoben/transformiert", "„a“ Darstellung geändert"]
        );
        editor.jump_history(3);
        assert_eq!(editor.project.nodes[0].style.fill, Color::rgb(255, 0, 0));
        editor.jump_history(0);
        assert_eq!(editor.project.nodes.len(), 1);
    }
}
