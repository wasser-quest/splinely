//! Structured controls shared by the state graph's entry, exit and transition editors.
use eframe::egui;
use splinely_document::{Action, AnimValue, Color, Event, Guard, Project, Property};
use splinely_editor::i18n::tr;

fn picker(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut String,
    choices: impl Iterator<Item = String>,
) -> bool {
    let old = value.clone();
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.as_str())
        .show_ui(ui, |ui| {
            for choice in choices {
                ui.selectable_value(value, choice.clone(), choice);
            }
        });
    old != *value
}

fn variable_ui(
    ui: &mut egui::Ui,
    name: &mut String,
    value: &mut serde_json::Value,
    project: &Project,
) {
    if picker(ui, "variable", name, project.variables.keys().cloned()) {
        *value = project.variables[name].clone();
    }
    ui.push_id(name.as_str(), |ui| match value {
        serde_json::Value::Bool(v) => {
            ui.checkbox(v, tr("Wert"));
        }
        serde_json::Value::Number(n) => {
            number_ui(ui, n);
        }
        serde_json::Value::String(v) => {
            ui.text_edit_singleline(v);
        }
        serde_json::Value::Null => {
            ui.label("null");
        }
        _ => {
            ui.label(tr(
                "Komplexer Wert bleibt erhalten; im Maschinen-JSON bearbeiten.",
            ));
        }
    });
}

// Retain incomplete input between frames, without normalizing untouched JSON numbers.
// In particular, 1, 1.0 and integers above f64's exact range must stay distinct.
pub(super) fn number_ui(ui: &mut egui::Ui, number: &mut serde_json::Number) -> egui::Response {
    let id = ui.id().with("numeric_value");
    let (previous, mut text) = ui.data_mut(|data| {
        data.get_temp::<(serde_json::Number, String)>(id)
            .unwrap_or_else(|| (number.clone(), number.to_string()))
    });
    if previous != *number {
        text = number.to_string();
    }
    let response = ui
        .text_edit_singleline(&mut text)
        .on_hover_text(tr("JSON-Zahl: 1 für Ganzzahl, 1.0 für Dezimalzahl."));
    if response.changed() {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Number>(&text) {
            *number = parsed;
        }
    }
    if serde_json::from_str::<serde_json::Number>(&text).is_err() {
        ui.colored_label(egui::Color32::LIGHT_RED, tr("Ungültige JSON-Zahl."));
    }
    if !response.has_focus() {
        text = number.to_string();
    }
    ui.data_mut(|data| data.insert_temp(id, (number.clone(), text)));
    response
}

pub(super) fn guard_ui(ui: &mut egui::Ui, guard: &mut Option<Guard>, project: &Project) {
    let mut enabled = guard.is_some();
    if ui
        .add_enabled(
            !project.variables.is_empty() || enabled,
            egui::Checkbox::new(&mut enabled, tr("Bedingung")),
        )
        .changed()
    {
        *guard = if enabled {
            project.variables.iter().next().map(|(name, value)| Guard {
                variable: name.clone(),
                equals: value.clone(),
            })
        } else {
            None
        };
    }
    if let Some(guard) = guard {
        ui.push_id("guard", |ui| {
            ui.horizontal(|ui| {
                variable_ui(ui, &mut guard.variable, &mut guard.equals, project);
            })
        });
    }
}

fn event_ui(ui: &mut egui::Ui, event: &mut Event, project: &Project) {
    ui.label(tr("Ereignisname:"));
    ui.text_edit_singleline(&mut event.name);
    egui::ComboBox::from_id_salt("target")
        .selected_text(event.target.as_deref().unwrap_or(tr("Beliebiges Ziel")))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut event.target, None, tr("Beliebiges Ziel"));
            for node in &project.nodes {
                ui.selectable_value(&mut event.target, Some(node.id.clone()), &node.id);
            }
        });
}

fn color_ui(ui: &mut egui::Ui, color: &mut Color) {
    let mut rgba = [
        color.r as f32,
        color.g as f32,
        color.b as f32,
        color.a as f32,
    ];
    if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
        *color = Color {
            r: rgba[0] as f64,
            g: rgba[1] as f64,
            b: rgba[2] as f64,
            a: rgba[3] as f64,
        };
    }
}

fn property_label(property: Property) -> &'static str {
    tr(match property {
        Property::MotionProgress => "Pfadfortschritt",
        Property::X => "Position X",
        Property::Y => "Position Y",
        Property::Rotation => "Drehung °",
        Property::ScaleX => "Skalierung X",
        Property::ScaleY => "Skalierung Y",
        Property::Opacity => "Deckkraft",
        Property::Fill => "Füllfarbe",
        Property::Stroke => "Konturfarbe",
        Property::Width => "Breite",
        Property::Height => "Höhe",
        Property::Radius => "Radius",
        Property::FontSize => "Schriftgröße",
        Property::Gradient => "Farbverlauf",
        Property::D => "Pfadform (Morph)",
    })
}

fn property_ui(
    ui: &mut egui::Ui,
    node_id: &mut String,
    property: &mut Property,
    value: &mut AnimValue,
    project: &Project,
) {
    picker(
        ui,
        "node",
        node_id,
        project.nodes.iter().map(|n| n.id.clone()),
    );
    let old = *property;
    egui::ComboBox::from_id_salt("property")
        .selected_text(property_label(*property))
        .show_ui(ui, |ui| {
            for p in [
                Property::X,
                Property::Y,
                Property::Rotation,
                Property::ScaleX,
                Property::ScaleY,
                Property::Opacity,
                Property::Fill,
                Property::Stroke,
                Property::Width,
                Property::Height,
                Property::Radius,
                Property::FontSize,
                Property::MotionProgress,
                Property::D,
                Property::Gradient,
            ] {
                ui.selectable_value(property, p, property_label(p));
            }
        });
    if old != *property {
        *value = match property {
            Property::Fill | Property::Stroke => AnimValue::Color(Color::rgb(0, 0, 0)),
            Property::D => AnimValue::Path(String::new()),
            Property::Gradient => AnimValue::Gradient(splinely_document::Gradient::linear(
                0.0,
                Color::rgb(0, 0, 0),
                Color::rgb(255, 255, 255),
            )),
            _ => AnimValue::Number(
                if matches!(
                    property,
                    Property::ScaleX | Property::ScaleY | Property::Opacity
                ) {
                    1.0
                } else {
                    0.0
                },
            ),
        };
    }
    match value {
        AnimValue::Number(v) => {
            ui.add(egui::DragValue::new(v).speed(0.1));
        }
        AnimValue::Color(c) => color_ui(ui, c),
        AnimValue::Path(d) => {
            ui.text_edit_multiline(d);
        }
        AnimValue::Gradient(g) => {
            ui.checkbox(&mut g.radial, tr("Radial"));
            ui.add(egui::DragValue::new(&mut g.angle).suffix("°"));
            for (i, stop) in g.stops.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut stop.offset)
                                .range(0.0..=1.0)
                                .speed(0.01),
                        );
                        color_ui(ui, &mut stop.color);
                    })
                });
            }
        }
    }
}

/// New actions always use a declared reference. Absent assets disable the corresponding type.
fn action_choices(project: &Project) -> Vec<(&'static str, Action)> {
    let mut choices = vec![(
        "Ereignis senden",
        Action::Emit {
            event: Event {
                name: "click".into(),
                target: None,
            },
        },
    )];
    if let Some((name, value)) = project.variables.iter().next() {
        choices.push((
            "Variable setzen",
            Action::SetVariable {
                name: name.clone(),
                value: value.clone(),
            },
        ));
    }
    if let Some(n) = project.nodes.first() {
        choices.push((
            "Eigenschaft setzen",
            Action::SetProperty {
                node_id: n.id.clone(),
                property: Property::X,
                value: AnimValue::Number(n.transform.x),
            },
        ));
    }
    if let Some(c) = project.clips.first() {
        choices.push((
            "Clip starten",
            Action::PlayClip {
                clip_id: c.id.clone(),
            },
        ));
        choices.push((
            "Clip stoppen",
            Action::StopClip {
                clip_id: c.id.clone(),
            },
        ));
    }
    if let Some(c) = project
        .clips
        .iter()
        .find(|c| c.blend_mode == splinely_document::BlendMode::Weighted)
    {
        choices.push((
            "Clip gewichten",
            Action::BlendClip {
                clip_id: c.id.clone(),
                weight: 1.0,
                duration: 0.0,
            },
        ));
    }
    if let Some(a) = project.audio_assets.first() {
        choices.push((
            "Audio abspielen",
            Action::PlayAudio {
                asset_id: a.id.clone(),
            },
        ));
    }
    choices
}

fn action_label(action: &Action) -> &'static str {
    match action {
        Action::Emit { .. } => "Ereignis senden",
        Action::SetVariable { .. } => "Variable setzen",
        Action::SetProperty { .. } => "Eigenschaft setzen",
        Action::PlayClip { .. } => "Clip starten",
        Action::StopClip { .. } => "Clip stoppen",
        Action::BlendClip { .. } => "Clip gewichten",
        Action::PlayAudio { .. } => "Audio abspielen",
    }
}

pub(super) fn actions_ui(ui: &mut egui::Ui, actions: &mut Vec<Action>, project: &Project) {
    let mut remove = None;
    let mut movement = None;
    let len = actions.len();
    for (i, action) in actions.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("{} · {}", i + 1, tr(action_label(action))));
                    if ui
                        .add_enabled(i > 0, egui::Button::new(tr("Aktion nach oben")))
                        .on_hover_text(tr("Aktion nach oben"))
                        .clicked()
                    {
                        movement = Some((i, i - 1));
                    }
                    if ui
                        .add_enabled(i + 1 < len, egui::Button::new(tr("Aktion nach unten")))
                        .on_hover_text(tr("Aktion nach unten"))
                        .clicked()
                    {
                        movement = Some((i, i + 1));
                    }
                    if ui.button(tr("Entfernen")).clicked() {
                        remove = Some(i);
                    }
                });
                // Editing never normalizes or replaces untouched advanced values.
                match action {
                    Action::Emit { event } => event_ui(ui, event, project),
                    Action::SetVariable { name, value } => variable_ui(ui, name, value, project),
                    Action::SetProperty {
                        node_id,
                        property,
                        value,
                    } => property_ui(ui, node_id, property, value, project),
                    Action::PlayClip { clip_id } | Action::StopClip { clip_id } => {
                        picker(
                            ui,
                            "clip",
                            clip_id,
                            project.clips.iter().map(|c| c.id.clone()),
                        );
                    }
                    Action::BlendClip {
                        clip_id,
                        weight,
                        duration,
                    } => {
                        picker(
                            ui,
                            "clip",
                            clip_id,
                            project
                                .clips
                                .iter()
                                .filter(|c| c.blend_mode == splinely_document::BlendMode::Weighted)
                                .map(|c| c.id.clone()),
                        );
                        ui.label(tr("Gewicht"));
                        ui.add(egui::DragValue::new(weight).range(0.0..=1.0).speed(0.01));
                        ui.label(tr("Dauer"));
                        ui.add(
                            egui::DragValue::new(duration)
                                .range(0.0..=f64::MAX)
                                .speed(0.05)
                                .suffix(" s"),
                        );
                    }
                    Action::PlayAudio { asset_id } => {
                        picker(
                            ui,
                            "audio",
                            asset_id,
                            project.audio_assets.iter().map(|a| a.id.clone()),
                        );
                    }
                }
            })
        });
    }
    if let Some(i) = remove {
        actions.remove(i);
    } else if let Some((a, b)) = movement {
        actions.swap(a, b);
    }
    ui.menu_button(tr("+ Aktion"), |ui| {
        for (label, action) in action_choices(project) {
            if ui.button(tr(label)).clicked() {
                actions.push(action);
                ui.close_menu();
            }
        }
    });
}
