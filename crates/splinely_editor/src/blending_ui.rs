//! Authored blending controls, isolated from transport, audio and state-graph UI.
use eframe::egui;
use splinely_document::*;
use splinely_editor::accessible::Named;
use splinely_editor::i18n::tr;

pub(super) fn allowed_modes(project: &Project, index: usize) -> [bool; 3] {
    [
        BlendMode::Override,
        BlendMode::Weighted,
        BlendMode::Additive,
    ]
    .map(|mode| {
        let mut candidate = project.clone();
        let Some(clip) = candidate.clips.get_mut(index) else {
            return false;
        };
        clip.blend_mode = mode;
        candidate.validate().is_ok()
    })
}

pub(super) fn clip_options(ui: &mut egui::Ui, clip: &mut Clip, allowed: [bool; 3]) {
    egui::ComboBox::from_id_salt("clip_blend_mode")
        .selected_text(match clip.blend_mode {
            BlendMode::Override => tr("Ersetzen"),
            BlendMode::Weighted => tr("Gewichtet"),
            BlendMode::Additive => tr("Additiv"),
        })
        .show_ui(ui, |ui| {
            for (i, mode, label) in [
                (0, BlendMode::Override, tr("Ersetzen")),
                (1, BlendMode::Weighted, tr("Gewichtet")),
                (2, BlendMode::Additive, tr("Additiv")),
            ] {
                ui.add_enabled_ui(allowed[i], |ui| {
                    ui.selectable_value(&mut clip.blend_mode, mode, label);
                });
            }
        });
    ui.add_enabled(
        clip.blend_mode != BlendMode::Override,
        egui::Slider::new(&mut clip.weight, 0.0..=1.0).text(tr("Gewicht")),
    );
    if clip.blend_mode == BlendMode::Additive {
        ui.small(tr("Additiv: Gewicht × (Clipwert − Dokument-Grundpose). Nur Zahlen und RGBA; begrenzte Werte werden geklemmt."));
    }
}

#[derive(Clone)]
struct Draft {
    original: Option<String>,
    space: BlendSpace,
    error: String,
}
fn apply(
    project: &mut Project,
    original: Option<&str>,
    space: BlendSpace,
) -> Result<(), DocumentError> {
    let mut candidate = project.clone();
    for name in &space.parameters {
        candidate
            .variables
            .entry(name.clone())
            .or_insert(serde_json::json!(0));
    }
    if let Some(original) = original {
        let existing = candidate
            .blend_spaces
            .iter_mut()
            .find(|s| s.id == original)
            .ok_or_else(|| DocumentError("unknown blend space".into()))?;
        *existing = space;
    } else {
        candidate.blend_spaces.push(space);
    }
    candidate.validate()?;
    *project = candidate;
    Ok(())
}

pub(super) fn spaces(ui: &mut egui::Ui, project: &mut Project) {
    ui.collapsing(tr("2D-Mischräume"), |ui| {
        ui.small(tr("Zwei numerische Variablen steuern inverse quadratische Distanzgewichte. Nur aktive gewichtete Clips werden gemischt; Ebenenvorschau startet sie gemeinsam."));
        let draft_id = ui.make_persistent_id("blend_space_draft");
        let mut draft = ui.ctx().data_mut(|d| d.get_temp::<Draft>(draft_id));
        let weights = match project.blend_space_weights(&Default::default()) {
            Ok(weights) => weights,
            Err(error) => {
                ui.colored_label(ui.visuals().error_fg_color, format!("{}: {error}", tr("Mischraumgewichte ungültig")));
                Default::default()
            }
        };
        let mut remove = None;
        for space in &project.blend_spaces {
            ui.horizontal_wrapped(|ui| {
                ui.label(&space.id);
                if ui.small_button(tr("Bearbeiten")).clicked() {
                    draft = Some(Draft { original: Some(space.id.clone()), space: space.clone(), error: String::new() });
                }
                if ui.small_button(tr("Löschen")).clicked() { remove = Some(space.id.clone()); }
                for sample in &space.samples {
                    ui.label(format!("{}: {:.3}", sample.clip_id, weights.get(&sample.clip_id).unwrap_or(&0.)));
                }
            });
        }
        if let Some(id) = remove { project.blend_spaces.retain(|s| s.id != id); }
        let names: std::collections::BTreeSet<_> = project.blend_spaces.iter().flat_map(|s| s.parameters.clone()).collect();
        for name in names {
            if let Some(mut value) = project.variables.get(&name).and_then(|v| v.as_f64()) {
                ui.horizontal(|ui| {
                    ui.label(&name);
                    if ui.add(egui::DragValue::new(&mut value).speed(0.01).range(-1e7..=1e7)).changed() {
                        project.variables.insert(name.clone(), serde_json::json!(value));
                    }
                });
            }
        }
        if ui.button(tr("Mischraum hinzufügen")).clicked() {
            let id = project.unused_id("blend-space");
            draft = Some(Draft { original: None, space: BlendSpace {
                id, parameters: ["blendX".into(), "blendY".into()],
                samples: Vec::new(),
            }, error: String::new() });
        }
        let mut close = false;
        if let Some(d) = &mut draft {
            ui.group(|ui| {
                ui.horizontal(|ui| { ui.label(tr("ID")); ui.text_edit_singleline(&mut d.space.id); });
                for (i, name) in d.space.parameters.iter_mut().enumerate() {
                    ui.horizontal(|ui| { ui.label(if i == 0 { tr("X-Variable") } else { tr("Y-Variable") }); ui.text_edit_singleline(name); });
                }
                let mut delete = None;
                for (index, sample) in d.space.samples.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt(("blend_sample", index)).selected_text(&sample.clip_id).show_ui(ui, |ui| {
                            for clip in project.clips.iter().filter(|c| c.blend_mode == BlendMode::Weighted) {
                                ui.selectable_value(&mut sample.clip_id, clip.id.clone(), &clip.id);
                            }
                        });
                        for value in &mut sample.position { ui.add(egui::DragValue::new(value).speed(0.01).range(-1e7..=1e7)); }
                        if ui
                            .small_button("−")
                            .on_hover_text(tr("Löschen"))
                            .named(tr("Löschen"))
                            .clicked()
                        {
                            delete = Some(index);
                        }
                    });
                }
                if let Some(index) = delete { d.space.samples.remove(index); }
                if ui.button(tr("Stützpunkt hinzufügen")).clicked() {
                    let used: Vec<_> = project.blend_spaces.iter().filter(|s| Some(s.id.as_str()) != d.original.as_deref())
                        .flat_map(|s| s.samples.iter().map(|v| &v.clip_id)).collect();
                    if let Some(clip) = project.clips.iter().find(|c| c.blend_mode == BlendMode::Weighted && !used.contains(&&c.id) && !d.space.samples.iter().any(|s| s.clip_id == c.id)) {
                        d.space.samples.push(BlendSpaceSample { clip_id: clip.id.clone(), position: [d.space.samples.len() as f64, 0.] });
                    } else { d.error = tr("Zuerst einen freien gewichteten Clip erstellen.").into(); }
                }
                ui.horizontal(|ui| {
                    if ui.button(tr("Übernehmen")).clicked() {
                        match apply(project, d.original.as_deref(), d.space.clone()) {
                            Ok(()) => close = true,
                            Err(e) => d.error = e.to_string(),
                        }
                    }
                    if ui.button(tr("Abbrechen")).clicked() { close = true; }
                });
                if !d.error.is_empty() { ui.colored_label(egui::Color32::RED, &d.error); }
            });
        }
        ui.ctx().data_mut(|data| {
            if close { data.remove::<Draft>(draft_id); }
            else if let Some(draft) = draft { data.insert_temp(draft_id, draft); }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authoring_is_atomic_and_defaults_parameters() {
        let mut project = Project::default();
        let space = BlendSpace {
            id: "space".into(),
            parameters: ["speed".into(), "turn".into()],
            samples: vec![],
        };
        let before = project.clone();
        assert!(apply(&mut project, None, space).is_err());
        assert_eq!(project, before);
        let mut project = Project::from_json(include_str!(
            "../../../runtime/tests/fixtures/blend-spaces.json"
        ))
        .unwrap();
        assert_eq!(allowed_modes(&project, 0), [false, true, false]);
        assert_eq!(allowed_modes(&project, 2), [true, true, true]);
        let mut space = project.blend_spaces.remove(0);
        space.id = "authored".into();
        space.parameters = ["newX".into(), "newY".into()];
        apply(&mut project, None, space.clone()).unwrap();
        assert_eq!(project.variables["newX"], serde_json::json!(0));
        let before = project.clone();
        space.samples[1].position = space.samples[0].position;
        assert!(apply(&mut project, Some("authored"), space).is_err());
        assert_eq!(project, before);
    }
}
