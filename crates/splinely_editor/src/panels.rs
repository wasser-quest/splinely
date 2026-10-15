use super::*;
use base64::Engine;

#[path = "animation_assistant.rs"]
mod animation_assistant;
use animation_assistant::{EntryClip, MachineDraft, Preset};

fn number(ui: &mut egui::Ui, label: &str, value: &mut f64) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).speed(0.5));
    });
}
fn color_ui(ui: &mut egui::Ui, label: &str, c: &mut Color) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut rgba = [c.r as f32, c.g as f32, c.b as f32, c.a as f32];
        if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
            *c = Color {
                r: rgba[0] as f64,
                g: rgba[1] as f64,
                b: rgba[2] as f64,
                a: rgba[3] as f64,
            };
        }
    });
}
fn entry_clip_ui(ui: &mut egui::Ui, project: &Project, label: &str, entry: &mut EntryClip) {
    ui.push_id(label, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(label);
            egui::ComboBox::from_id_salt("entry_clip")
                .selected_text(if entry.clip_id.is_empty() { tr("Kein Clip") } else { &entry.clip_id })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut entry.clip_id, String::new(), tr("Kein Clip"));
                    for clip in &project.clips {
                        ui.selectable_value(&mut entry.clip_id, clip.id.clone(), &clip.name);
                    }
                });
            let weighted = project.clips.iter().any(|c| c.id == entry.clip_id && c.blend_mode == BlendMode::Weighted);
            if !weighted { entry.fade = false; }
            ui.add_enabled(weighted, egui::Checkbox::new(&mut entry.fade, tr("Gewicht einblenden")))
                .on_disabled_hover_text(tr("Zuerst im Clip unter Clip-Optionen den Mischmodus Gewichtet wählen."));
            if entry.fade {
                ui.add(egui::Slider::new(&mut entry.weight, 0.0..=1.0).text(tr("Zielgewicht")));
                ui.add(egui::DragValue::new(&mut entry.duration).range(0.0..=60.).speed(0.05).suffix(" s"));
            }
        });
        if entry.fade { ui.small(tr("Start ist das aktuelle Laufzeitgewicht (anfangs Clipgewicht). Erstes Einblenden: Clipgewicht 0 › Ziel 1. Erneutes PlayClip setzt das Gewicht nicht zurück.")); }
    });
}

fn property_label(property: Property) -> &'static str {
    match property {
        Property::X => "Position X",
        Property::Y => "Position Y",
        Property::Rotation => "Drehung °",
        Property::ScaleX => "Skalierung X",
        Property::ScaleY => "Skalierung Y",
        Property::Opacity => "Deckkraft",
        Property::Fill => tr("Füllfarbe"),
        Property::Stroke => tr("Konturfarbe"),
        Property::Width => tr("Breite"),
        Property::Height => tr("Höhe"),
        Property::Radius => tr("Radius"),
        Property::FontSize => tr("Schriftgröße"),
        Property::Gradient => tr("Farbverlauf"),
        Property::D => tr("Pfadform (Morph)"),
    }
}

fn properties() -> &'static [Property] {
    &[
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
        Property::Gradient,
        Property::D,
    ]
}
fn base_value(node: &Node, property: Property) -> AnimValue {
    AnimValue::Number(match property {
        Property::X => node.transform.x,
        Property::Y => node.transform.y,
        Property::Rotation => node.transform.rotation,
        Property::ScaleX => node.transform.scale_x,
        Property::ScaleY => node.transform.scale_y,
        Property::Opacity => node.style.opacity,
        Property::Fill => return AnimValue::Color(node.style.fill),
        Property::Stroke => {
            return AnimValue::Color(node.style.stroke.unwrap_or(Color::rgb(0, 0, 0)))
        }
        Property::Width => {
            if let NodeKind::Rect { width, .. } = node.kind {
                width
            } else {
                0.
            }
        }
        Property::Height => {
            if let NodeKind::Rect { height, .. } = node.kind {
                height
            } else {
                0.
            }
        }
        Property::Radius => {
            if let NodeKind::Circle { radius, .. } = node.kind {
                radius
            } else {
                0.
            }
        }
        Property::FontSize => {
            if let NodeKind::Text { font_size, .. } = node.kind {
                font_size
            } else {
                16.
            }
        }
        Property::Gradient => {
            return match &node.gradient {
                Some(g) => AnimValue::Gradient(g.clone()),
                None => AnimValue::Color(node.style.fill),
            }
        }
        Property::D => {
            return match &node.kind {
                NodeKind::Path { d, .. } => AnimValue::Path(d.clone()),
                _ => AnimValue::Path(String::new()),
            }
        }
    })
}
fn property_supported(node: &Node, property: Property) -> bool {
    match property {
        Property::Width | Property::Height => matches!(node.kind, NodeKind::Rect { .. }),
        Property::Radius => matches!(node.kind, NodeKind::Circle { .. }),
        Property::FontSize => matches!(node.kind, NodeKind::Text { .. }),
        Property::Gradient => node.gradient.is_some(),
        Property::D => matches!(node.kind, NodeKind::Path { .. }),
        _ => true,
    }
}
impl Editor {
    pub(super) fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr("Dein Projekt"));
        let mut p = self.project.clone();
        ui.add(egui::TextEdit::singleline(&mut p.name).hint_text(tr("Projektname")));
        ui.collapsing(tr("Dokumentgröße"), |ui| {
            number(ui, tr("Breite"), &mut p.width);
            number(ui, tr("Höhe"), &mut p.height);
        });
        ui.separator();
        layers::layers_ui_multi(&mut p, ui, &mut self.selected, &mut self.multi);
        self.last_primary = self.selected.clone();
        if let Some(op) = self.arrange_ui(ui) {
            self.pending_arrange = Some(op);
        }
        if let Some(index) = p
            .nodes
            .iter()
            .position(|n| Some(&n.id) == self.selected.as_ref())
        {
            ui.separator();
            if p.nodes[index].locked {
                ui.label(tr(
                    "Diese Ebene ist geschützt. Zum Bearbeiten oben entsperren.",
                ));
                self.commit(p);
                return;
            }
            let node = &mut p.nodes[index];
            ui.heading(tr("Deine Auswahl"));
            ui.small(trf("Feste ID: {}", std::slice::from_ref(&node.id)));
            if let Some(bone) = &node.bone_id {
                ui.small(trf(
                    "An {bone} gebunden · Position relativ zum Knochen.",
                    std::slice::from_ref(bone),
                ));
            }
            ui.add(
                egui::TextEdit::singleline(&mut node.name)
                    .hint_text(tr("Name (optional, z. B. „Kopf“)")),
            );
            egui::CollapsingHeader::new(tr("Position & Drehung"))
                .default_open(true)
                .show(ui, |ui| {
                    number(ui, "X", &mut node.transform.x);
                    number(ui, "Y", &mut node.transform.y);
                    number(ui, tr("Drehung °"), &mut node.transform.rotation);
                    number(ui, tr("Skalierung X"), &mut node.transform.scale_x);
                    number(ui, tr("Skalierung Y"), &mut node.transform.scale_y);
                });
            ui.collapsing(tr("Form & Text"), |ui| match &mut node.kind {
                NodeKind::Rect {
                    x,
                    y,
                    width,
                    height,
                    rx,
                } => {
                    number(ui, "X", x);
                    number(ui, "Y", y);
                    number(ui, tr("Breite"), width);
                    number(ui, tr("Höhe"), height);
                    number(ui, tr("Eckenradius"), rx);
                }
                NodeKind::Path { d, bounds, .. } => {
                    ui.weak(trf(
                        "Pfad · {} Zeichen · {:.0} × {:.0}",
                        &[
                            format!("{}", d.len()),
                            format!("{:.0}", bounds[2]),
                            format!("{:.0}", bounds[3]),
                        ],
                    ));
                    ui.weak(tr("Punkte bearbeiten: Werkzeug Pfadpunkte (A)."));
                }
                NodeKind::Circle { cx, cy, radius } => {
                    number(ui, tr("Mittelpunkt X"), cx);
                    number(ui, tr("Mittelpunkt Y"), cy);
                    number(ui, tr("Radius"), radius);
                }
                NodeKind::Text {
                    x,
                    y,
                    text,
                    font_size,
                } => {
                    number(ui, "X", x);
                    number(ui, "Y", y);
                    ui.text_edit_multiline(text);
                    number(ui, tr("Schriftgröße"), font_size);
                }
            });
            if ui
                .add_enabled(
                    self.machine_source_clean(),
                    egui::Button::new(tr("Form löschen")),
                )
                .on_hover_text(tr(
                    "Entfernt auch zugehörige Spuren und Zustandsreferenzen · rückgängig möglich.",
                ))
                .on_disabled_hover_text(tr(
                    "Zuerst vorgemerktes Zustands-JSON übernehmen oder neu laden.",
                ))
                .clicked()
            {
                let id = node.id.clone();
                remove_node(&mut p, &id);
                self.commit(p);
                self.selected = None;
                self.machine_buffer =
                    serde_json::to_string_pretty(&self.project.state_machines).unwrap();
                return;
            }
        } else {
            ui.label(tr("Was möchtest du gestalten?"));
            ui.small(tr("Form anklicken oder neu anlegen: R Rechteck · E Kreis · T Text · P Pen · B Freihand."));
            if self.preview_mode {
                ui.small(tr("Zum Verschieben die Vorschau ausschalten."));
            }
        }
        if self.selected.is_some() {
            ui.separator();
            vector::vector_inspector_ui(&mut p, ui, &mut self.selected);
        }
        ui.separator();
        ui.collapsing(tr("Erweiterungen"), |ui| {
            ui.add_enabled_ui(
                !self.playing && !self.preview_mode && !self.blend_preview.ticking(),
                |ui| match self.extensions.ui(ui, &p, self.selected.as_deref()) {
                    Ok(Some(next)) => p = next,
                    Ok(None) => {}
                    Err(error) => self.status = error,
                },
            );
        });
        self.commit(p);
        ui.separator();
        if self.workspace == Workspace::Animate {
            let mut p = self.project.clone();
            self.animation_assistant_ui(ui, &mut p);
            self.commit(p);
        } else {
            self.inspector_animation_ui(ui);
        }
        ui.collapsing(tr("Gut zu wissen"), |ui| {
            ui.small(trf("Alle Tastenkürzel: {} · Befehle suchen: {} · Tasten und Optionen ändern: {}.", &[self.binding_label(&palette::Cmd::Shortcuts).to_string(), self.binding_label(&palette::Cmd::Palette).to_string(), self.binding_label(&palette::Cmd::Settings).to_string()]));
            ui.small(tr("Erweitert › SVG-Quelltext bewahrt Quelltext, importiert keine Formen. JavaScript läuft im Viewer/Browser; native Audiovorschau unter Animieren › Audio."));
        });
    }
    pub(super) fn keyframe_samples(&self) -> Vec<Node> {
        // Author the selected source clip, not its already weighted composite.
        // Otherwise recording weight=.5 would halve the pose for a second time.
        let mut sample_project = self.project.clone();
        let mut sample_context = self.preview_context();
        if let Some(clip) = sample_project.clips.get_mut(self.active_clip) {
            if clip.blend_mode == BlendMode::Weighted {
                clip.blend_mode = BlendMode::Override;
                sample_context.clips = Some(vec![ClipPlayback {
                    clip_id: clip.id.clone(),
                    time: self.time,
                }]);
                sample_context.clip_weights.clear();
            }
        }
        let mut evaluated = sample_project
            .evaluate_with_context(self.time, &sample_context)
            .unwrap_or_else(|_| self.project.nodes.clone());
        if let Ok(poses) = sample_project.evaluate_bones(self.time, &sample_context) {
            for node in &mut evaluated {
                if let Some(pose) = poses.iter().find(|b| Some(&b.id) == node.bone_id.as_ref()) {
                    let a = pose.rotation.to_radians();
                    let (x, y) = (node.transform.x - pose.x, node.transform.y - pose.y);
                    node.transform.x = x * a.cos() + y * a.sin();
                    node.transform.y = -x * a.sin() + y * a.cos();
                    node.transform.rotation -= pose.rotation;
                }
            }
        }
        evaluated
    }
    fn inspector_animation_ui(&mut self, ui: &mut egui::Ui) {
        let Some(node) = self
            .project
            .nodes
            .iter()
            .find(|n| Some(&n.id) == self.selected.as_ref())
            .cloned()
        else {
            return;
        };
        ui.collapsing(tr("Auswahl animieren"), |ui| {
            let Some(clip) = self.project.clips.get(self.active_clip) else {
                ui.small(tr("Unter Animieren zuerst + Clip wählen."));
                return;
            };
            let clip_id = clip.id.clone();
            ui.small(format!("{} · {:.2}s", clip.name, self.time));
            let id = egui::Id::new(("assistant_property", &node.id));
            let mut property = ui
                .ctx()
                .data_mut(|d| d.get_temp::<Property>(id))
                .unwrap_or(Property::X);
            if !property_supported(&node, property) {
                property = Property::X;
            }
            egui::ComboBox::from_id_salt("inspector_key_property")
                .selected_text(property_label(property))
                .show_ui(ui, |ui| {
                    for &p in properties()
                        .iter()
                        .filter(|&&p| property_supported(&node, p))
                    {
                        ui.selectable_value(&mut property, p, property_label(p));
                    }
                });
            ui.ctx().data_mut(|d| d.insert_temp(id, property));
            ui.small(tr(
                "Grundwert oben ändern, Zeit in der Zeitleiste wählen, dann merken.",
            ));
            if ui
                .add_enabled(
                    !node.locked,
                    egui::Button::new(tr("◼ Aktuellen Wert merken")),
                )
                .clicked()
            {
                match animation_assistant::keyframe_candidate(
                    &self.project,
                    &clip_id,
                    &node.id,
                    property,
                    self.time,
                    base_value(&node, property),
                ) {
                    Ok(next) => self.commit(next),
                    Err(error) => self.status = error,
                }
            }
        });
    }

    fn animation_assistant_ui(&mut self, ui: &mut egui::Ui, project: &mut Project) {
        egui::CollapsingHeader::new(tr("Einfach animieren · Schritt für Schritt"))
            .default_open(true)
            .show(ui, |ui| {
                ui.small(tr("1 Auswahl · 2 Clip oben wählen oder + Clip · 3 Zeit schieben · 4 Eigenschaft merken"));
                let mut target = self.selected.clone().or_else(|| self.selected_bone.clone()).unwrap_or_default();
                let previous_target = target.clone();
                egui::ComboBox::from_id_salt("assistant_target")
                    .selected_text(if target.is_empty() { tr("Form / Knochen wählen") } else { &target })
                    .show_ui(ui, |ui| {
                        for node in &project.nodes {
                            ui.selectable_value(&mut target, node.id.clone(), trf("Form: {}", std::slice::from_ref(&node.name)));
                        }
                        for bone in &project.bones {
                            ui.selectable_value(&mut target, bone.id.clone(), trf("Knochen: {}", std::slice::from_ref(&bone.id)));
                        }
                    });
                let bone = project.bones.iter().find(|b| b.id == target);
                let node = project.nodes.iter().find(|n| n.id == target);
                if target != previous_target {
                    if bone.is_some() {
                        self.selected_bone = Some(target.clone());
                        self.selected = None;
                    } else if node.is_some() {
                        self.selected = Some(target.clone());
                        self.selected_bone = None;
                    }
                }
                let Some(clip_id) = project.clips.get(self.active_clip).map(|c| c.id.clone()) else {
                    ui.label(tr("Oben mit + Clip starten. Danach kannst du Schlüssel oder ein Muster hinzufügen."));
                    return;
                };
                if node.is_none() && bone.is_none() {
                    ui.label(tr("Eine Form oder einen Knochen auswählen."));
                    return;
                }
                let id = egui::Id::new(("assistant_property", &target));
                let mut property = ui.ctx().data_mut(|d| d.get_temp::<Property>(id)).unwrap_or(Property::X);
                let supported: Vec<_> = properties().iter().copied().filter(|&p| {
                    if let Some(node) = node { property_supported(node, p) }
                    else { matches!(p, Property::X | Property::Y | Property::Rotation) }
                }).collect();
                if !supported.contains(&property) { property = Property::X; }
                egui::ComboBox::from_id_salt("assistant_property")
                    .selected_text(property_label(property))
                    .show_ui(ui, |ui| {
                        for p in supported { ui.selectable_value(&mut property, p, property_label(p)); }
                    });
                ui.ctx().data_mut(|d| d.insert_temp(id, property));
                let value_for = |p| {
                    if let Some(node) = node { base_value(node, p) }
                    else {
                        let bone = bone.unwrap();
                        AnimValue::Number(match p { Property::X => bone.x, Property::Y => bone.y, _ => bone.rotation })
                    }
                };
                let value = value_for(property);
                ui.small(trf("Aktueller Grundwert: {value:?} · Zeit {:.2}s", &[format!("{value:?}"), format!("{:.2}", self.time)]));
                ui.small(tr("Grundwert im Inspektor / Knochenbereich ändern, Zeit wählen und merken. Kein Ablesen der gemischten Vorschau; gleiche Zeit ersetzt den Schlüssel."));
                let editable = !node.is_some_and(|n| n.locked);
                let mut candidate = None;
                ui.add_enabled_ui(editable, |ui| {
                    if ui.button(tr("◼ Schlüssel aus aktuellem Wert hinzufügen")).clicked() {
                        candidate = Some(animation_assistant::keyframe_candidate(project, &clip_id, &target, property, self.time, value));
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label(tr("Oder ein einfaches Muster:"));
                        for (preset, label, p) in [
                            (Preset::Bounce, tr("Hüpfen"), Property::Y),
                            (Preset::Wave, tr("Winken"), Property::Rotation),
                            (Preset::Opacity, tr("Aus- / Einblenden"), Property::Opacity),
                        ] {
                            if preset == Preset::Opacity && bone.is_some() { continue; }
                            if ui.button(label).on_hover_text(tr("Drei Schlüssel über die Clipdauer; bestehende Spur wird nicht überschrieben.")).clicked() {
                                if let AnimValue::Number(base) = value_for(p) {
                                    candidate = Some(animation_assistant::preset_candidate(project, &clip_id, &target, preset, base));
                                }
                            }
                        }
                    });
                });
                if !editable { ui.small(tr("Geschützte Form: zuerst in der Ebenenliste entsperren.")); }
                if !project.raw_svg.is_empty() {
                    ui.small(tr("SVG-Quelltext bleibt unverändert. Diese Werkzeuge animieren nur die modellierten Formen / Knochen, nicht beliebige Elemente im Quelltext."));
                }
                if let Some(candidate) = candidate {
                    match candidate {
                        Ok(next) => { *project = next; self.preview_animation = true; }
                        Err(error) => self.status = error,
                    }
                }
            });
    }

    fn machine_assistant_ui(&mut self, ui: &mut egui::Ui) {
        let id = egui::Id::new("new_machine_assistant");
        let mut draft = ui
            .ctx()
            .data_mut(|d| d.get_temp::<MachineDraft>(id))
            .unwrap_or_else(|| MachineDraft {
                target: self.selected.clone().unwrap_or_default(),
                ..Default::default()
            });
        egui::CollapsingHeader::new(tr("Neue Zustandsmaschine · geführt"))
            .default_open(true)
            .show(ui, |ui| {
                let clean = self.machine_source_clean();
                if !clean { ui.small(tr("Zuerst Zustands-JSON übernehmen oder Entwurf neu laden. Dein JSON-Entwurf bleibt erhalten.")); }
                ui.add_enabled_ui(clean, |ui| {
                    ui.small(tr("1 Zwei Zustands-IDs und Anfang wählen"));
                    ui.small(tr("IDs: A–Z / a–z oder _ am Anfang; danach auch Ziffern, _ - .; keine Leerzeichen / Umlaute."));
                    ui.horizontal(|ui| {
                        ui.label(tr("Ruhezustand")); ui.text_edit_singleline(&mut draft.idle);
                        ui.label(tr("Aktivzustand")); ui.text_edit_singleline(&mut draft.active);
                    });
                    ui.horizontal(|ui| {
                        ui.label(tr("Start:"));
                        ui.radio_value(&mut draft.initial_active, false, &draft.idle);
                        ui.radio_value(&mut draft.initial_active, true, &draft.active);
                    });
                    ui.small(tr("2 Ereignis: Ruhe › Aktiv (ohne Bedingung)"));
                    egui::ComboBox::from_id_salt("machine_assistant_target")
                        .selected_text(if draft.target.is_empty() { tr("Zielform wählen") } else { &draft.target })
                        .show_ui(ui, |ui| {
                            for node in &self.project.nodes {
                                ui.selectable_value(&mut draft.target, node.id.clone(), format!("{} ({})", node.name, node.id));
                            }
                        });
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut draft.hover, false, tr("Klick"));
                        ui.radio_value(&mut draft.hover, true, tr("Zeiger betritt Form (Hover)"));
                    });
                    ui.small(tr("Hover ist pointerenter im Viewer / Browser; nativ über Ereignis Senden testen. Kein Rückweg automatisch: weitere Übergänge im Graphen ergänzen."));
                    ui.small(tr("3 Optional: Clip beim Eintritt starten (onEnter)"));
                    entry_clip_ui(ui, &self.project, tr("Ruhe"), &mut draft.idle_clip);
                    entry_clip_ui(ui, &self.project, tr("Aktiv"), &mut draft.active_clip);
                    ui.small(tr("Gewicht einblenden ist kein Zustands-Crossfade. PlayClip startet zuerst, BlendClip ändert danach nur dessen Gewicht. Andere Clips laufen weiter; StopClip / Gegenblenden im Graphen ergänzen."));
                    if ui.button(tr("+ Geführte Zustandsmaschine erstellen")).clicked() {
                        let machine_id = self.new_id("machine");
                        match animation_assistant::machine_candidate(&self.project, &machine_id, &draft) {
                            Ok(next) => {
                                self.commit(next);
                                self.machine_index = self.project.state_machines.len().saturating_sub(1);
                                self.machine_buffer = serde_json::to_string_pretty(&self.project.state_machines).unwrap();
                            }
                            Err(error) => self.status = error,
                        }
                    }
                });
            });
        ui.ctx().data_mut(|d| d.insert_temp(id, draft));
    }

    pub(super) fn timeline_ui(&mut self, ui: &mut egui::Ui) {
        let mut p = self.project.clone();
        let evaluated = self.keyframe_samples();
        ui.horizontal(|ui| {
            if ui
                .button(if self.playing {
                    tr("⏸ Pause")
                } else {
                    tr("▶ Abspielen")
                })
                .clicked()
            {
                self.playing = !self.playing;
                self.preview_animation = true;
            }
            if ui.button(tr("Zum Anfang")).clicked() {
                self.time = 0.;
                self.playing = false;
                self.audio_preview.seek(0., false);
            }
            ui.checkbox(&mut self.looping, tr("Wiederholen"));
            ui.checkbox(&mut self.preview_animation, tr("Clip-Vorschau"));
            let duration = p
                .clips
                .get(self.active_clip)
                .map(|c| c.duration)
                .unwrap_or(10.);
            if ui
                .add(egui::Slider::new(&mut self.time, 0.0..=duration).text(tr("Sekunden")))
                .changed()
            {
                self.playing = false;
                self.audio_preview.seek(self.time, false);
                self.preview_animation = true;
            }
            egui::ComboBox::from_id_salt("active_clip")
                .selected_text(
                    p.clips
                        .get(self.active_clip)
                        .map(|c| c.name.as_str())
                        .unwrap_or(tr("Noch kein Clip")),
                )
                .show_ui(ui, |ui| {
                    for (i, clip) in p.clips.iter().enumerate() {
                        if ui
                            .selectable_value(&mut self.active_clip, i, &clip.name)
                            .changed()
                        {
                            self.time = 0.;
                        }
                    }
                });
            if ui.button(tr("+ Clip")).clicked() {
                p.clips.push(Clip {
                    id: self.new_id("clip"),
                    name: tr("Neuer Clip").into(),
                    duration: 5.,
                    looping: true,
                    autoplay: false,
                    blend_mode: BlendMode::Override,
                    weight: 1.,
                    tracks: vec![],
                });
                self.active_clip = p.clips.len() - 1;
            }
        });
        ui.small(tr("Einfach animieren: Auswahl und Schlüssel im rechten Inspektor. Hier Clip und Zeit wählen."));
        ui.collapsing(tr("Mehrere Clips & Überblendung"), |ui| {
        ui.small(tr("Gewichtet mischt gleichzeitig aktive Clips mit der Grundpose. Ersetzen nutzt die Clip-Reihenfolge. Ein BlendClip ändert nur das Gewicht eines schon aktiven gewichteten Clips; kein automatischer Zustands-Crossfade."));
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr("Ebenen abspielen")).on_hover_text(tr("Automatisch startende Clips parallel abspielen; sonst den gewählten Clip. Jeder Clip hat eine eigene Uhr.")).clicked() {
                let ids: Vec<_> = p.clips.iter().filter(|c| c.autoplay).map(|c| c.id.clone()).collect();
                let ids = if ids.is_empty() { p.clips.get(self.active_clip).map(|c| vec![c.id.clone()]).unwrap_or_default() } else { ids };
                self.status = tr("Ebenenvorschau: eigene Uhren; Gewichte wirken auf Formen, nicht auf Audio.").into();
                for id in ids {
                    if let Err(error) = self.blend_preview.play(&p, &id) { self.status = error; break; }
                }
            }
            if ui.button(tr("Ebenen zurücksetzen")).clicked() {
                self.blend_preview = Default::default();
                self.status = tr("Ebenen zurückgesetzt · Vorschau des gewählten Clips wieder aktiv.").into();
            }
            if let Some(clip) = p.clips.get(self.active_clip) {
                if ui.button(tr("Gewählten Clip abspielen")).clicked() {
                    if let Err(error) = self.blend_preview.play(&p, &clip.id) { self.status = error; }
                }
                if ui.button(tr("Clip pausieren")).clicked() { self.blend_preview.pause(&clip.id); }
                if ui.button(tr("Clip stoppen")).clicked() {
                    if let Err(error) = self.blend_preview.stop(&p, &clip.id) { self.status = error; }
                }
            }
        });
        });
        if self.blend_preview.enabled {
            for clip in &p.clips {
                if let Some(layer) = self.blend_preview.layers.get(&clip.id).filter(|l| l.active) {
                    ui.label(trf(
                        "{}: {:.2} s · Gewicht {:.3} · {}",
                        &[
                            clip.id.to_string(),
                            format!("{:.2}", layer.time),
                            format!("{:.3}", layer.weight),
                            (if layer.playing {
                                "playing"
                            } else {
                                "paused / ended"
                            })
                            .to_string(),
                        ],
                    ));
                }
            }
        }
        if p.clips.is_empty() {
            ui.label(tr(
                "Bring deine Idee in Bewegung! + Clip › Form auswählen › Spur hinzufügen.",
            ));
        }
        ui.label(tr("Spuren & Schlüssel (Experten)"));
        egui::ScrollArea::vertical()
            .id_salt("tracks_scroll")
            .show(ui, |ui| {
                if let Some(clip) = p.clips.get_mut(self.active_clip) {
                    let mut delete_clip = false;
                    ui.horizontal(|ui| {
                        ui.label(&clip.id);
                        ui.text_edit_singleline(&mut clip.name);
                        number(ui, tr("Dauer"), &mut clip.duration);

                        let locked_tracks = clip.tracks.iter().any(|t| {
                            self.project
                                .nodes
                                .iter()
                                .any(|n| n.id == t.node_id && n.locked)
                        });
                        delete_clip = ui
                            .add_enabled(!locked_tracks, egui::Button::new(tr("Clip löschen")))
                            .clicked();
                    });
                    ui.collapsing(tr("Clip-Optionen & Mischgewicht"), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(&mut clip.looping, tr("Im Export wiederholen"));
                            ui.checkbox(&mut clip.autoplay, tr("Automatisch starten"));
                            egui::ComboBox::from_id_salt("clip_blend_mode")
                                .selected_text(match clip.blend_mode { BlendMode::Override => tr("Ersetzen"), BlendMode::Weighted => tr("Gewichtet") })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut clip.blend_mode, BlendMode::Override, tr("Ersetzen"));
                                    ui.selectable_value(&mut clip.blend_mode, BlendMode::Weighted, tr("Gewichtet"));
                                });
                            ui.add_enabled(clip.blend_mode == BlendMode::Weighted,
                                egui::Slider::new(&mut clip.weight, 0.0..=1.0).text(tr("Gewicht")));
                        });
                    });
                    if let Some(node) = self
                        .project
                        .nodes
                        .iter()
                        .find(|n| !n.locked && Some(&n.id) == self.selected.as_ref())
                    {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(trf("Spur für {} hinzufügen:", std::slice::from_ref(&node.id)));
                            for &property in properties()
                                .iter()
                                .filter(|&&property| property_supported(node, property))
                            {
                                if !clip
                                    .tracks
                                    .iter()
                                    .any(|t| t.node_id == node.id && t.property == property)
                                    && ui.small_button(format!("{property:?}")).clicked()
                                {
                                    clip.tracks.push(Track {
                                        node_id: node.id.clone(),
                                        property,
                                        keyframes: vec![Keyframe {
                                            time: self.time,
                                            value: base_value(node, property),
                                            easing: Easing::Linear,
                                        }],
                                    });
                                }
                            }
                        });
                    } else {
                        ui.label(tr("Eine Form auf der Zeichenfläche oder in der Ebenenliste auswählen, um eine Animationsspur hinzuzufügen."));
                    }
                    let mut remove_track = None;
                    for (ti, track) in clip.tracks.iter_mut().enumerate() {
                        let locked = self
                            .project
                            .nodes
                            .iter()
                            .any(|n| n.id == track.node_id && n.locked);
                        ui.push_id(ti, |ui| {
                            if locked {
                                ui.disable();
                            }
                            ui.collapsing(
                                trf("{} · {:?} ({} keys)", &[track.node_id.to_string(), format!("{:?}", track.property), format!("{}", track.keyframes.len())]),
                                |ui| {
                                    ui.horizontal(|ui| {
                                        if ui.button(tr("Schlüsselbild hier setzen (Skelett: Grundpose)")).clicked()
                                        {
                                            let value = evaluated
                                                .iter()
                                                .find(|n| n.id == track.node_id)
                                                .map(|node| base_value(node, track.property))
                                                .or_else(|| {
                                                    p.bones
                                                        .iter()
                                                        .find(|b| b.id == track.node_id)
                                                        .map(|bone| {
                                                            AnimValue::Number(
                                                                match track.property {
                                                                    Property::X => bone.x,
                                                                    Property::Y => bone.y,
                                                                    _ => bone.rotation,
                                                                },
                                                            )
                                                        })
                                                });
                                            if let Some(value) = value {
                                                if let Some(k) = track
                                                    .keyframes
                                                    .iter_mut()
                                                    .find(|k| (k.time - self.time).abs() < 1e-6)
                                                {
                                                    k.value = value;
                                                } else {
                                                    track.keyframes.push(Keyframe {
                                                        time: self.time,
                                                        value,
                                                        easing: Easing::Linear,
                                                    });
                                                }
                                            }
                                        }
                                        if ui.button(tr("Spur löschen")).clicked() {
                                            remove_track = Some(ti);
                                        }
                                    });
                                    let mut remove_key = None;
                                    let mut copy_key = None;
                                    for (ki, key) in track.keyframes.iter_mut().enumerate() {
                                        ui.push_id(ki, |ui| {
                                            ui.horizontal_wrapped(|ui| {
                                                ui.label(trf("Schlüsselbild {}", &[format!("{}", ki + 1)]));
                                                ui.add(
                                                    egui::DragValue::new(&mut key.time)
                                                        .speed(0.01)
                                                        .range(0.0..=clip.duration)
                                                        .prefix("t="),
                                                );
                                                match &mut key.value {
                                                    AnimValue::Number(n) => {
                                                        ui.add(egui::DragValue::new(n).speed(0.5));
                                                    }
                                                    AnimValue::Color(c) => color_ui(ui, "", c),
                                                    AnimValue::Gradient(g) => {
                                                        ui.add(
                                                            egui::DragValue::new(&mut g.angle)
                                                                .speed(1.)
                                                                .suffix("°"),
                                                        );
                                                        for stop in &mut g.stops {
                                                            color_ui(ui, "", &mut stop.color);
                                                        }
                                                    }
                                                    AnimValue::Path(d) => {
                                                        ui.weak(trf(
                                                            "Pfad · {} Zeichen",
                                                            &[d.len().to_string()],
                                                        ));
                                                    }
                                                };
                                                egui::ComboBox::from_id_salt("easing")
                                                    .selected_text(match key.easing {
                                                        Easing::Linear => tr("Linear"),
                                                        Easing::Step => tr("Stufe"),
                                                        _ => tr("Bézier-Kurve"),
                                                    })
                                                    .show_ui(ui, |ui| {
                                                        ui.selectable_value(
                                                            &mut key.easing,
                                                            Easing::Linear,
                                                            tr("Linear"),
                                                        );
                                                        ui.selectable_value(
                                                            &mut key.easing,
                                                            Easing::Step,
                                                            tr("Stufe"),
                                                        );
                                                        if ui
                                                            .selectable_label(
                                                                matches!(
                                                                    key.easing,
                                                                    Easing::CubicBezier { .. }
                                                                ),
                                                                tr("Bézier-Kurve"),
                                                            )
                                                            .clicked()
                                                        {
                                                            key.easing = Easing::CubicBezier {
                                                                x1: 0.42,
                                                                y1: 0.,
                                                                x2: 0.58,
                                                                y2: 1.,
                                                            };
                                                        }
                                                    });
                                                if let Easing::CubicBezier { x1, y1, x2, y2 } =
                                                    &mut key.easing
                                                {
                                                    for (label, v) in [
                                                        ("x1", x1),
                                                        ("y1", y1),
                                                        ("x2", x2),
                                                        ("y2", y2),
                                                    ] {
                                                        ui.add(
                                                            egui::DragValue::new(v)
                                                                .speed(0.01)
                                                                .prefix(format!("{label}=")),
                                                        );
                                                    }
                                                }
                                                if ui.small_button(tr("Kopieren")).clicked() {
                                                    copy_key = Some(key.clone());
                                                }
                                                if ui.small_button("×").clicked() {
                                                    remove_key = Some(ki);
                                                }
                                            });
                                        });
                                    }
                                    if let Some(i) = remove_key {
                                        track.keyframes.remove(i);
                                        if track.keyframes.is_empty() {
                                            remove_track = Some(ti);
                                        }
                                    }
                                    if let Some(mut key) = copy_key {
                                        key.time = (key.time + 0.1).min(clip.duration);
                                        if !track
                                            .keyframes
                                            .iter()
                                            .any(|k| (k.time - key.time).abs() < 1e-6)
                                        {
                                            track.keyframes.push(key);
                                        }
                                    }
                                    track.keyframes.sort_by(|a, b| a.time.total_cmp(&b.time));
                                },
                            );
                        });
                    }
                    if let Some(i) = remove_track {
                        clip.tracks.remove(i);
                    }
                    if delete_clip {
                        // Referenced clips must be removed from state actions first; validator reports this.
                        p.clips.remove(self.active_clip);
                        self.active_clip = self.active_clip.saturating_sub(1);
                    }
                }
            });
        self.commit(p);
    }
    pub(super) fn states_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr("Zustände"));
        ui.small(tr("Reaktionen gestalten: Vorschau aktivieren und eine Form anklicken – oder unten ein Ereignis senden."))
            .on_hover_text(tr("JSON-Änderungen werden erst beim Übernehmen validiert. Native Vorschau: Eigenschaften und Clips. Zustands-Audio im Viewer/Browser; native Audiovorschau nur für globale Audioeinsätze."));
        self.machine_assistant_ui(ui);
        ui.horizontal(|ui| {
            if ui.button(tr("Zustands-JSON übernehmen")).clicked() {
                match serde_json::from_str::<Vec<StateMachine>>(&self.machine_buffer) {
                    Ok(machines) => {
                        let mut p = self.project.clone();
                        p.state_machines = machines;
                        self.commit(p);
                    }
                    Err(e) => self.status = trf("Maschinen-JSON: {e}", &[format!("{e}")]),
                }
            }
            if ui.button(tr("Entwurf neu laden")).clicked() {
                self.machine_buffer =
                    serde_json::to_string_pretty(&self.project.state_machines).unwrap();
            }
        });
        ui.collapsing(tr("Variablen-JSON (Werte für Bedingungen)"), |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.variables_buffer)
                    .code_editor()
                    .desired_width(f32::INFINITY),
            );
            if ui.button(tr("Variablen übernehmen")).clicked() {
                match serde_json::from_str(&self.variables_buffer) {
                    Ok(variables) => {
                        let mut p = self.project.clone();
                        p.variables = variables;
                        self.commit(p);
                    }
                    Err(e) => self.status = trf("Variablen-JSON: {e}", &[format!("{e}")]),
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label(tr("Ereignis:"));
            ui.text_edit_singleline(&mut self.event_name);
            ui.label(tr("Ziel-ID (optional)"));
            ui.text_edit_singleline(&mut self.event_target);
            if ui.button(tr("Senden")).clicked() {
                self.send_event(
                    self.event_name.clone(),
                    if self.event_target.is_empty() {
                        None
                    } else {
                        Some(self.event_target.clone())
                    },
                );
            }
            if ui.button(tr("Vorschau zurücksetzen")).clicked() {
                self.machines.clear();
                self.blend_preview = Default::default();
                self.runtime_overrides.clear();
                self.event_log.clear();
                self.time = 0.;
                self.playing = false;
            }
        });
        ui.horizontal(|ui| {
            for (i, machine) in self.project.state_machines.iter().enumerate() {
                ui.selectable_value(&mut self.machine_index, i, &machine.id);
            }
        });
        let buffer_clean = serde_json::from_str::<Vec<StateMachine>>(&self.machine_buffer)
            .is_ok_and(|machines| machines == self.project.state_machines);
        if !buffer_clean {
            ui.label(tr("Vor der Graphbearbeitung Zustands-JSON übernehmen oder neu laden. Dein Entwurf bleibt erhalten."));
        }
        let next = ui
            .add_enabled_ui(buffer_clean, |ui| {
                self.state_graph.show(ui, &self.project, self.machine_index)
            })
            .inner;
        if let Some(next) = next {
            self.commit(next);
            self.machine_buffer =
                serde_json::to_string_pretty(&self.project.state_machines).unwrap();
        }
        if let Some(machine) = self.project.state_machines.get(self.machine_index) {
            let active = self
                .machines
                .iter()
                .find(|m| m.machine_id == machine.id)
                .map(|m| m.current_state.as_str())
                .unwrap_or(&machine.initial);
            ui.label(trf(
                "Native Vorschau: {} › {}",
                &[machine.id.to_string(), active.to_string()],
            ));
        }
        egui::CollapsingHeader::new(tr("Ereignisprotokoll")).show(ui, |ui| {
            for line in self.event_log.iter().rev().take(10) {
                ui.label(line);
            }
        });
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.machine_buffer)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(22),
            );
        });
    }
    pub(super) fn machine_source_clean(&self) -> bool {
        serde_json::from_str::<Vec<StateMachine>>(&self.machine_buffer)
            .is_ok_and(|machines| machines == self.project.state_machines)
    }
    pub(super) fn send_event(&mut self, name: String, target: Option<String>) {
        self.dispatch_events(Some(Event { name, target }));
    }
    pub(super) fn dispatch_events(&mut self, event: Option<Event>) {
        let mut pending = std::collections::VecDeque::new();
        if self.machines.is_empty() && !self.project.state_machines.is_empty() {
            self.machines = self
                .project
                .state_machines
                .iter()
                .filter_map(|m| MachineInstance::new(&self.project, &m.id).ok())
                .collect();
            let actions = self
                .machines
                .iter()
                .flat_map(|m| m.initial_actions.clone())
                .collect();
            self.preview_actions(actions, &mut pending);
        }
        if let Some(event) = event {
            pending.push_back(event);
        }
        let mut handled = 0;
        while let Some(event) = pending.pop_front() {
            handled += 1;
            if handled > 32 {
                self.status = tr("Vorschau-Ereignisschleife nach 32 Ereignissen angehalten").into();
                break;
            }
            self.event_log
                .push(format!("{} › {:?}", event.name, event.target));
            for index in 0..self.machines.len() {
                match self.machines[index].send(&self.project, &event) {
                    Ok(actions) => self.preview_actions(actions, &mut pending),
                    Err(e) => self.status = e.to_string(),
                }
            }
        }
        if self.event_log.len() > 200 {
            self.event_log.drain(..self.event_log.len() - 200);
        }
    }
    pub(super) fn preview_actions(
        &mut self,
        actions: Vec<Action>,
        pending: &mut std::collections::VecDeque<Event>,
    ) {
        for action in actions {
            self.event_log.push(format!("  {action:?}"));
            match action {
                Action::SetProperty {
                    node_id,
                    property,
                    value,
                } => {
                    self.runtime_overrides
                        .retain(|o| o.node_id != node_id || o.property != property);
                    self.runtime_overrides.push(PropertyOverride {
                        node_id,
                        property,
                        value,
                    });
                }
                Action::PlayClip { clip_id } => {
                    if let Err(error) = self.blend_preview.play(&self.project, &clip_id) {
                        self.status = error;
                    }
                }
                Action::StopClip { clip_id } => {
                    if let Err(error) = self.blend_preview.stop(&self.project, &clip_id) {
                        self.status = error;
                    }
                }
                Action::BlendClip {
                    clip_id,
                    weight,
                    duration,
                } => {
                    match self.blend_preview.fade(&self.project, &clip_id, weight, duration) {
                        Ok(()) => self.status = trf("Mischung {clip_id} › {weight:.3} in {duration:.3} s (startet den Clip nicht)", &[clip_id.to_string(), format!("{weight:.3}"), format!("{duration:.3}")]),
                        Err(error) => self.status = error,
                    }
                }
                Action::Emit { event } => pending.push_back(event),
                Action::SetVariable { name, value } => {
                    for machine in &mut self.machines {
                        machine.variables.insert(name.clone(), value.clone());
                    }
                }
                Action::PlayAudio { .. } => {
                    self.status =
                        tr("Audioaktion protokolliert · im Browser-Export anhören.").into()
                }
            }
        }
    }
    pub(super) fn audio_ui(&mut self, ui: &mut egui::Ui) {
        self.native_audio_controls(ui);
        ui.heading(tr("Audio"));
        if self.project.audio_assets.is_empty() {
            ui.label(tr(
                "Noch kein Audio: lokalen Dateipfad unten eintragen und die Datei einbetten.",
            ));
        }
        ui.small(tr("Audiodateien werden eingebettet."))
            .on_hover_text(tr(
                "Native Audiovorschau oben einschalten. Browser-Audio muss gestartet werden.",
            ));
        ui.horizontal(|ui| {
            ui.label(tr("Dateipfad:"));
            ui.text_edit_singleline(&mut self.audio_path);
            if ui.button(tr("Audio einbetten")).clicked() {
                let id = self.new_id("audio");
                match import_audio(&self.audio_path, id) {
                    Ok(asset) => {
                        let mut p = self.project.clone();
                        p.audio_assets.push(asset);
                        self.audio_asset = p.audio_assets.len() - 1;
                        self.commit(p);
                    }
                    Err(e) => self.status = e,
                }
            }
        });
        let mut p = self.project.clone();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, asset) in p.audio_assets.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.audio_asset, i, &asset.id);
                    ui.text_edit_singleline(&mut asset.name);
                    ui.label(trf(
                        "{} · ~{} bytes",
                        &[
                            asset.mime_type.to_string(),
                            format!("{}", asset.data_base64.len() * 3 / 4),
                        ],
                    ));
                });
            }
            if ui.button(tr("+ Audioeinsatz hier")).clicked() {
                if let Some(asset) = p.audio_assets.get(self.audio_asset) {
                    p.audio_cues.push(AudioCue {
                        id: self.new_id("cue"),
                        asset_id: asset.id.clone(),
                        time: self.time,
                        offset: 0.,
                        duration: None,
                        volume: 1.,
                    });
                }
            }
            let mut remove = None;
            for (i, cue) in p.audio_cues.iter_mut().enumerate() {
                ui.push_id(("cue", i), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(&cue.id);
                        egui::ComboBox::from_id_salt("asset")
                            .selected_text(&cue.asset_id)
                            .show_ui(ui, |ui| {
                                for asset in &p.audio_assets {
                                    ui.selectable_value(
                                        &mut cue.asset_id,
                                        asset.id.clone(),
                                        &asset.name,
                                    );
                                }
                            });
                        number(ui, tr("Zeit"), &mut cue.time);
                        number(ui, tr("Offset"), &mut cue.offset);
                        number(ui, tr("Lautstärke"), &mut cue.volume);
                        let mut end = cue.duration.is_some();
                        if ui.checkbox(&mut end, tr("Dauer begrenzen")).changed() {
                            cue.duration = if end { Some(1.) } else { None };
                        }
                        if let Some(duration) = &mut cue.duration {
                            number(ui, tr("Dauer"), duration);
                        }
                        if ui.button("×").clicked() {
                            remove = Some(i);
                        }
                    });
                });
            }
            if let Some(i) = remove {
                p.audio_cues.remove(i);
            }
        });
        self.commit(p);
    }
    pub(super) fn code_ui(&mut self, ui: &mut egui::Ui) {
        let is_js = self.tab == Tab::JavaScript;
        ui.heading(if is_js {
            tr("JavaScript")
        } else {
            tr("Bewahrter SVG-Quelltext")
        });
        ui.label(if is_js {tr("Dein Code bleibt erhalten. Ausführung nur nach ausdrücklicher Freigabe im Viewer/Browser, nie nativ. Syntax dort in der Konsole prüfen; CSP kann die Ausführung blockieren.")}else{tr("SVG wird als XML geprüft und in Projekt/Export-Metadaten bewahrt – nicht als Formen importiert oder gerendert. Visuelle Änderungen überschreiben diesen Entwurf nicht.")});
        if is_js {
            ui.checkbox(&mut self.trusted_js_export,tr("Vertrauenswürdiges eigenes JavaScript im interaktiven Export / Viewer ausführen (Browser-Zugriff auf Netzwerk/Dateien möglich)"));
        }
        ui.horizontal(|ui| {
            if ui.button(tr("Übernehmen")).clicked() {
                let mut p = self.project.clone();
                if is_js {
                    p.custom_js = self.code_buffer.clone();
                    self.commit(p);
                    self.status =
                        tr("JavaScript übernommen · Syntax und Ausführung prüft der Browser, nicht die native Vorschau.")
                            .into();
                } else {
                    match validate_xml(&self.svg_buffer) {
                        Ok(()) => {
                            p.raw_svg = self.svg_buffer.clone();
                            self.commit(p);
                            self.status = tr("XML gültig · Quelltext nur bewahrt.").into();
                        }
                        Err(e) => self.status = e,
                    }
                }
            }
            if ui.button(tr("Entwurf zurücksetzen")).clicked() {
                if is_js {
                    self.code_buffer = self.project.custom_js.clone();
                } else {
                    self.svg_buffer = self.project.raw_svg.clone();
                }
            }
        });
        if is_js {
            ui.collapsing(tr("API-Beispiel"),|ui|{ui.code("splinely.on(\"circle\", \"click\", () => {\n  splinely.clip(\"intro\").play();\n});");});
        }
        let buffer = if is_js {
            &mut self.code_buffer
        } else {
            &mut self.svg_buffer
        };
        egui::ScrollArea::both().show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(buffer)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(30),
            );
        });
        ui.separator();
        match self.project.validate() {
            Ok(()) => {
                ui.label(tr("Projektprüfung: Schema und Referenzen gültig."));
            }
            Err(e) => {
                ui.colored_label(Color32::RED, e.to_string());
            }
        }
        if !is_js && !self.svg_buffer.is_empty() {
            if let Err(e) = validate_xml(&self.svg_buffer) {
                ui.colored_label(Color32::RED, e);
            }
        }
    }
}
pub(super) fn remove_node(project: &mut Project, id: &str) {
    // Shared with the CLI; an unknown ID leaves the project unchanged.
    let _ = project.remove_node_with_references(id);
}
fn validate_xml(source: &str) -> Result<(), String> {
    if source.is_empty() {
        return Ok(());
    }
    let doc = roxmltree::Document::parse(source).map_err(|e| trf("XML: {e}", &[format!("{e}")]))?;
    if doc.root_element().tag_name().name() != "svg" {
        return Err("XML root must be <svg>".into());
    }
    let mut ids = std::collections::HashSet::new();
    for node in doc.descendants().filter(|n| n.is_element()) {
        if let Some(id) = node.attribute("id") {
            if !ids.insert(id) {
                return Err(trf("Doppelte SVG-ID: {id}", &[id.to_string()]));
            }
        }
    }
    Ok(())
}
fn import_audio(path: &str, id: String) -> Result<AudioAsset, String> {
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    if metadata.len() > 32 * 1024 * 1024 {
        return Err(tr("Audio-Import ist auf 32 MiB begrenzt").into());
    }
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mime = match extension.as_str() {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" | "oga" => "audio/ogg",
        "m4a" | "mp4" => "audio/mp4",
        "flac" => "audio/flac",
        "webm" => "audio/webm",
        _ => return Err(tr("Unterstützte Audioformate: wav, mp3, ogg, m4a, flac, webm").into()),
    };
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    Ok(AudioAsset {
        id,
        name: std::path::Path::new(path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        mime_type: mime.into(),
        data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delete_node_cleans_tracks_and_behavior_references() {
        let mut project = demo();
        remove_node(&mut project, "circle");
        project.validate().unwrap();
        assert!(project.clips[0].tracks.is_empty());
        assert!(project.state_machines[0].transitions.is_empty());
    }
    #[test]
    fn xml_edits_reject_malformed_and_duplicate_ids() {
        assert!(validate_xml("<svg><g></svg>").is_err());
        assert!(validate_xml("<svg><g id='a'/><g id='a'/></svg>").is_err());
        assert!(
            validate_xml("<svg xmlns='http://www.w3.org/2000/svg'><path d='M0 0'/></svg>").is_ok()
        );
    }
    #[test]
    fn demo_history_roundtrip_and_evaluation() {
        let mut e = Editor::default();
        e.project.validate().unwrap();
        let original = e.project.clone();
        let mut next = original.clone();
        next.nodes[0].transform.x = 42.;
        e.commit(next.clone());
        assert_eq!(e.project, next);
        assert!(e.history.undo(&mut e.project));
        assert_eq!(e.project, original);
        assert!(e.history.redo(&mut e.project));
        assert_eq!(e.project, next);
        let loaded = Project::from_json(&next.to_json().unwrap()).unwrap();
        assert_eq!(loaded.nodes[0].transform, next.nodes[0].transform);
        assert_eq!(loaded.clips, next.clips);
        assert_eq!(loaded.state_machines, next.state_machines);
        assert!((loaded.nodes[0].style.fill.b - next.nodes[0].style.fill.b).abs() < 1e-14);
        let nodes = original.evaluate_clip("intro", 0.75).unwrap();
        assert!((nodes[0].transform.x - 160.).abs() < 1e-8);
    }
    #[test]
    fn drag_is_one_history_transaction() {
        let mut e = Editor::default();
        let before = e.project.clone();
        e.drag_start = Some(before.clone());
        e.project.nodes[0].transform.x = 80.;
        e.finish_drag();
        assert_eq!(e.project.nodes[0].transform.x, 80.);
        assert!(e.history.undo(&mut e.project));
        assert_eq!(e.project, before);
        assert!(!e.history.can_undo());
    }
    #[test]
    fn assistant_preserves_staged_json_source_and_trust_and_uses_undo() {
        let mut e = Editor {
            machine_buffer: "[{ unfinished draft".into(),
            svg_buffer: "<svg>staged".into(),
            code_buffer: "// staged JavaScript".into(),
            trusted_js_export: true,
            ..Default::default()
        };
        let before = e.project.clone();
        let next = animation_assistant::keyframe_candidate(
            &before,
            &before.clips[0].id,
            &before.nodes[0].id,
            Property::Rotation,
            0.,
            AnimValue::Number(30.),
        )
        .unwrap();
        e.commit(next.clone());
        assert_eq!(e.project, next);
        assert_eq!(e.machine_buffer, "[{ unfinished draft");
        assert_eq!(e.svg_buffer, "<svg>staged");
        assert_eq!(e.code_buffer, "// staged JavaScript");
        assert!(e.trusted_js_export);
        assert!(e.history.undo(&mut e.project));
        assert_eq!(e.project, before);
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| e.machine_assistant_ui(ui));
        });
        assert_eq!(e.project, before);
        assert_eq!(e.machine_buffer, "[{ unfinished draft");
    }
    #[test]
    fn rendering_assistant_does_not_replace_a_shape_selection_with_stale_bone() {
        let mut e = Editor::default();
        e.project.bones.push(Bone {
            id: "arm".into(),
            parent_id: None,
            x: 0.,
            y: 0.,
            rotation: 0.,
            length: 10.,
        });
        e.selected = Some(e.project.nodes[0].id.clone());
        e.selected_bone = Some("arm".into());
        let selected = e.selected.clone();
        let mut p = e.project.clone();
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| e.animation_assistant_ui(ui, &mut p));
        });
        assert_eq!(e.selected, selected);
        assert_eq!(e.selected_bone.as_deref(), Some("arm"));
        assert_eq!(p, e.project);
    }
    #[test]
    fn preview_does_not_mutate_document() {
        let mut e = Editor::default();
        let before = e.project.clone();
        e.time = 1.;
        e.preview_animation = true;
        let nodes = e.evaluated();
        assert!(nodes[0].transform.x > 0.);
        assert_eq!(e.project, before);
    }
}
