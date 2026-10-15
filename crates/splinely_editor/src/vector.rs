//! Vector inspector and transactional authoring commands, without app state.
use eframe::egui;
use splinely_document::{Color, NodeKind, Project};
use splinely_editor::accessible::Named;
use splinely_editor::i18n::{tr, trf};

#[path = "vector_tools.rs"]
pub(crate) mod tools;

#[derive(Clone, Debug)]
pub(crate) enum VectorCommand {
    ConvertToPath,
    ConvertBack,
    SetPath(String),
}

/// Edit exact SVG path data; never infer a rect/circle from arbitrary paths.
/// Bounds are recomputed through the document's SVG geometry importer, while
/// the user's original command string (including arcs) is retained.
pub(crate) fn apply_vector_command(
    project: &mut Project,
    id: &str,
    command: VectorCommand,
) -> Result<(), String> {
    let mut next = project.clone();
    let node = next
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .ok_or_else(|| trf("Ebene {id} fehlt", &[id.to_string()]))?;
    if node.locked {
        return Err(tr("Ebene zuerst entsperren").into());
    }
    if matches!(command, VectorCommand::ConvertToPath) && matches!(node.kind, NodeKind::Text { .. })
    {
        next.convert_text_to_path(id, splinely_document::system_fonts())
            .map_err(|e| e.to_string())?;
        *project = next;
        return Ok(());
    }
    match command {
        VectorCommand::ConvertToPath => node.convert_to_path().map_err(|e| e.to_string())?,
        VectorCommand::ConvertBack => node.convert_back().map_err(|e| e.to_string())?,
        VectorCommand::SetPath(d) => {
            let NodeKind::Path { d: data, .. } = &mut node.kind else {
                return Err(tr("Zum Bearbeiten der Pfaddaten einen Pfad wählen").into());
            };
            if *data == d {
                // Do not quantize generated bounds or invalidate exact shape
                // provenance when the user applies an unchanged draft.
                return next.validate().map_err(|e| e.to_string());
            }
            let bounds = splinely_document::path_bounds(&d).map_err(|e| e.to_string())?;
            if let NodeKind::Path {
                d: data,
                bounds: current,
                ..
            } = &mut node.kind
            {
                *data = d;
                *current = bounds;
            }
            node.original_kind = None;
        }
    }
    next.validate()
        .map_err(|e| trf("Änderung abgelehnt: {e}", &[format!("{e}")]))?;
    *project = next;
    Ok(())
}

#[derive(Clone)]
struct PathDraft {
    source: String,
    text: String,
    point: [f64; 2],
}

fn color_picker(ui: &mut egui::Ui, label: &str, color: &mut Color) {
    ui.horizontal(|ui| {
        ui.label(label);
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
    });
}

/// Fill: plain colour, linear or radial gradient with editable stops.
/// Distinct colours used in the document (fills, strokes, gradient stops).
pub(crate) fn document_colors(project: &Project) -> Vec<Color> {
    let mut seen = std::collections::HashSet::new();
    let mut colors = vec![];
    let mut add = |c: Color| {
        let key = [c.r, c.g, c.b, c.a].map(|v| (v.clamp(0., 1.) * 255.).round() as u8);
        if c.a > 0.01 && seen.insert(key) {
            colors.push(c);
        }
    };
    for node in &project.nodes {
        match &node.gradient {
            Some(g) => g.stops.iter().for_each(|s| add(s.color)),
            None => add(node.style.fill),
        }
        if let Some(stroke) = node.style.stroke {
            add(stroke);
        }
    }
    colors.truncate(24);
    colors
}

/// Clickable swatches: left click sets the fill, right click the stroke.
fn swatches_ui(ui: &mut egui::Ui, node: &mut splinely_document::Node, colors: &[Color]) {
    if colors.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(3., 3.);
        for c in colors {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(16., 16.), egui::Sense::click());
            let color = egui::Color32::from_rgba_unmultiplied(
                (c.r * 255.) as u8,
                (c.g * 255.) as u8,
                (c.b * 255.) as u8,
                (c.a * 255.) as u8,
            );
            ui.painter().rect_filled(rect, 3., color);
            ui.painter().rect_stroke(
                rect,
                3.,
                egui::Stroke::new(1_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
                egui::StrokeKind::Inside,
            );
            let response = response.on_hover_text(tr("Klick: als Füllung · Rechtsklick: als Rand"));
            if response.clicked() {
                node.gradient = None;
                node.style.fill = *c;
            }
            if response.secondary_clicked() {
                node.style.stroke = Some(*c);
                if node.style.stroke_width == 0. {
                    node.style.stroke_width = 1.;
                }
            }
        }
    });
}

fn gradient_ui(ui: &mut egui::Ui, node: &mut splinely_document::Node, colors: &[Color]) {
    #[derive(PartialEq)]
    enum Mode {
        Color,
        Linear,
        Radial,
    }
    let mode = match &node.gradient {
        None => Mode::Color,
        Some(g) if g.radial => Mode::Radial,
        Some(_) => Mode::Linear,
    };
    let mut next = None;
    ui.horizontal(|ui| {
        ui.label(tr("Füllung"));
        for (m, label) in [
            (Mode::Color, tr("Farbe")),
            (Mode::Linear, tr("Farbverlauf")),
            (Mode::Radial, tr("Radial")),
        ] {
            if ui.selectable_label(mode == m, label).clicked() && mode != m {
                next = Some(m);
            }
        }
    });
    match next {
        Some(Mode::Color) => {
            // Keep the first stop's colour as the plain fill.
            if let Some(g) = node.gradient.take() {
                node.style.fill = g.stops[0].color;
            }
        }
        Some(m) => {
            let radial = m == Mode::Radial;
            match &mut node.gradient {
                Some(g) => g.radial = radial,
                None => {
                    let from = node.style.fill;
                    let to = Color {
                        r: from.r + (1. - from.r) * 0.7,
                        g: from.g + (1. - from.g) * 0.7,
                        b: from.b + (1. - from.b) * 0.7,
                        a: from.a,
                    };
                    let mut g = splinely_document::Gradient::linear(90., from, to);
                    g.radial = radial;
                    node.gradient = Some(g);
                }
            }
        }
        None => {}
    }
    let Some(gradient) = &mut node.gradient else {
        color_picker(ui, tr("Farbe"), &mut node.style.fill);
        if !colors.is_empty() {
            ui.weak(tr("Dokumentfarben"));
            swatches_ui(ui, node, colors);
        }
        return;
    };
    if !gradient.radial {
        ui.add(
            egui::Slider::new(&mut gradient.angle, -180.0..=180.0)
                .suffix("°")
                .text(tr("Richtung")),
        )
        .on_hover_text(tr("0° links › rechts, 90° oben › unten"));
    }
    let mut remove = None;
    let count = gradient.stops.len();
    for (i, stop) in gradient.stops.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let mut rgba = [
                stop.color.r as f32,
                stop.color.g as f32,
                stop.color.b as f32,
                stop.color.a as f32,
            ];
            if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
                stop.color = Color {
                    r: rgba[0] as f64,
                    g: rgba[1] as f64,
                    b: rgba[2] as f64,
                    a: rgba[3] as f64,
                };
            }
            let mut percent = stop.offset * 100.;
            if ui
                .add(egui::Slider::new(&mut percent, 0.0..=100.0).suffix(" %"))
                .changed()
            {
                stop.offset = percent / 100.;
            }
            if count > 2
                && ui
                    .small_button("🗑")
                    .on_hover_text(tr("Farbstopp entfernen"))
                    .named(tr("Farbstopp entfernen"))
                    .clicked()
            {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        gradient.stops.remove(i);
    }
    // Keep offsets ordered so validation never rejects a slider drag.
    gradient.stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    if gradient.stops.len() < 16 && ui.button(tr("+ Farbstopp")).clicked() {
        // Insert in the widest gap, with the colour halfway between.
        let (index, _) = gradient
            .stops
            .windows(2)
            .enumerate()
            .max_by(|a, b| {
                (a.1[1].offset - a.1[0].offset).total_cmp(&(b.1[1].offset - b.1[0].offset))
            })
            .map(|(i, w)| (i, w[1].offset - w[0].offset))
            .unwrap_or((0, 0.));
        let (a, b) = (gradient.stops[index], gradient.stops[index + 1]);
        let mix = |x: f64, y: f64| (x + y) / 2.;
        gradient.stops.insert(
            index + 1,
            splinely_document::GradientStop {
                offset: mix(a.offset, b.offset),
                color: Color {
                    r: mix(a.color.r, b.color.r),
                    g: mix(a.color.g, b.color.g),
                    b: mix(a.color.b, b.color.b),
                    a: mix(a.color.a, b.color.a),
                },
            },
        );
    }
}

/// Returns true only when a validated document edit was committed. The caller
/// owns history/dirty tracking. Invalid path drafts and errors persist in egui
/// memory, so typing an incomplete command never destroys existing geometry.
pub(crate) fn vector_inspector_ui(
    project: &mut Project,
    ui: &mut egui::Ui,
    selection: &mut Option<String>,
) -> bool {
    ui.heading(tr("Darstellung"));
    let Some(id) = selection.as_deref() else {
        ui.label(tr("Form wählen, um Füllung und Rand zu bearbeiten."));
        return false;
    };
    let Some(index) = project.nodes.iter().position(|n| n.id == id) else {
        ui.label(tr(
            "Auswahl nicht gefunden · Form auf der Zeichenfläche oder in den Ebenen wählen.",
        ));
        return false;
    };
    let error_id = ui.make_persistent_id(("vector_error", id));
    let draft_id = ui.make_persistent_id(("path_draft", id));
    let mut next = project.clone();
    let colors = document_colors(project);
    let node = &mut next.nodes[index];
    let mut command = None;
    if node.locked {
        ui.label(tr(
            "Ebene ist gesperrt · zum Bearbeiten in den Ebenen entsperren.",
        ));
    }
    ui.add_enabled_ui(!node.locked, |ui| {
        gradient_ui(ui, node, &colors);
        let mut opacity = node.style.opacity * 100.0;
        if ui
            .add(egui::Slider::new(&mut opacity, 0.0..=100.0).suffix(" %").text(tr("Deckkraft")))
            .on_hover_text(tr("0 % unsichtbar, 100 % deckend. Der Alphawert der Farbe wirkt zusätzlich."))
            .changed()
        {
            node.style.opacity = opacity / 100.0;
        }
        let mut stroke = node.style.stroke.is_some();
        if ui.checkbox(&mut stroke, tr("Rand (Kontur)"))
            .on_hover_text(tr("Rand um die Form ein-/ausschalten. Die Füllung färbt das Innere."))
            .changed()
        {
            node.style.stroke = stroke.then(|| Color::rgb(0, 0, 0));
            if stroke && node.style.stroke_width == 0.0 {
                node.style.stroke_width = 1.0;
            }
        }
        if let Some(color) = &mut node.style.stroke {
            color_picker(ui, tr("Randfarbe"), color);
        }
        ui.horizontal(|ui| {
            ui.label(tr("Randstärke"));
            ui.add(
                egui::DragValue::new(&mut node.style.stroke_width)
                    .speed(0.1)
                    .range(0.0..=1000.0)
                    .clamp_existing_to_range(false)
                    .suffix(" px"),
            ).on_hover_text(tr("Randstärke in lokalen SVG-Einheiten vor Skalierung · 0–1000 px · 0 blendet den Rand aus."));
        });
        ui.horizontal(|ui| {
            if ui.button("−").on_hover_text(tr("Rand 1 px dünner")).named(tr("Rand 1 px dünner")).clicked() {
                node.style.stroke_width = (node.style.stroke_width - 1.0).clamp(0.0, 1000.0);
            }
            for width in [1.0, 2.0, 4.0, 8.0] {
                if ui.button(format!("{width} px")).on_hover_text(tr("Randstärke setzen")).clicked() {
                    node.style.stroke_width = width;
                }
            }
            if ui.button("+").on_hover_text(tr("Rand 1 px dicker")).named(tr("Rand 1 px dicker")).clicked() {
                node.style.stroke_width = (node.style.stroke_width + 1.0).clamp(0.0, 1000.0);
            }
        });
        if !stroke {
            ui.small(tr("Rand ist aus · „Rand (Kontur)“ einschalten, um ihn zu zeigen."));
        }
        match &mut node.kind {
            NodeKind::Rect { .. } | NodeKind::Circle { .. } => {
                if ui.button(tr("In Pfad umwandeln")).clicked() {
                    command = Some(VectorCommand::ConvertToPath);
                }
            }
            NodeKind::Path { d, bounds, evenodd } => {
                ui.checkbox(evenodd, tr("Gerade-ungerade-Füllung (Löcher)"));
                ui.weak(trf("Begrenzung: x {:.1} · y {:.1} · {:.1} × {:.1}", &[format!("{:.1}", bounds[0]), format!("{:.1}", bounds[1]), format!("{:.1}", bounds[2]), format!("{:.1}", bounds[3])]));
                ui.small(tr("Werkzeug Pfadpunkte (A): eckige Anker, runde Bézier-Griffe ziehen · Doppelklick auf Kante fügt Anker ein · Shift koppelt Tangenten, Shift+Alt erhält die Länge."));
                let mut draft = ui
                    .ctx()
                    .data(|data| data.get_temp::<PathDraft>(draft_id))
                    .unwrap_or_else(|| PathDraft {
                        source: d.clone(),
                        text: d.clone(),
                        point: [0.0, 0.0],
                    });
                // Resync after undo, external edits or conversion, not while
                // the user is still constructing an incomplete SVG command.
                if draft.source != *d {
                    draft.source = d.clone();
                    draft.text = d.clone();
                }
                egui::CollapsingHeader::new(tr("SVG-Pfaddaten d (lokal)"))
                    .id_salt(("path_data", draft_id))
                    .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut draft.text)
                            .desired_rows(5)
                            .desired_width(ui.available_width())
                            .code_editor(),
                    );
                    ui.horizontal(|ui| {
                        ui.label(tr("Punkt X / Y"));
                        ui.add(egui::DragValue::new(&mut draft.point[0]).speed(0.5));
                        ui.add(egui::DragValue::new(&mut draft.point[1]).speed(0.5));
                    });
                    ui.horizontal(|ui| {
                        for (label, prefix) in [(tr("+ Sprung (M)"), "M"), (tr("+ Linie (L)"), "L")] {
                            if ui.button(label).clicked() {
                                draft.text.push_str(&format!(
                                    " {prefix}{} {}",
                                    draft.point[0], draft.point[1]
                                ));
                                command = Some(VectorCommand::SetPath(draft.text.clone()));
                            }
                        }
                        if ui.button(tr("Schließen (Z)")).clicked() {
                            draft.text.push_str(" Z");
                            command = Some(VectorCommand::SetPath(draft.text.clone()));
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button(tr("Pfad übernehmen")).clicked() {
                            command = Some(VectorCommand::SetPath(draft.text.clone()));
                        }
                        if ui.button(tr("Entwurf zurücksetzen")).clicked() {
                            draft.text = d.clone();
                            ui.ctx().data_mut(|data| data.remove::<String>(error_id));
                        }
                        if ui
                            .add_enabled(
                                node.original_kind.is_some(),
                                egui::Button::new(tr("Ursprungsform wiederherstellen")),
                            )
                            .on_hover_text(tr("Nur unveränderte, umgewandelte Pfade lassen sich zurückverwandeln"))
                            .clicked()
                        {
                            command = Some(VectorCommand::ConvertBack);
                        }
                    });
                    });
                ui.ctx().data_mut(|data| data.insert_temp(draft_id, draft));
            }
            NodeKind::Text { .. } => {
                if ui
                    .button(tr("In Pfad umwandeln"))
                    .on_hover_text(tr("Buchstaben werden zu bearbeitbaren Umrissen (lokale Systemschrift). Danach ist der Text nicht mehr editierbar."))
                    .clicked()
                {
                    command = Some(VectorCommand::ConvertToPath);
                }
            }
        }
    });
    let mut changed = false;
    if command.is_some() || next != *project {
        let result = if let Some(command) = command {
            apply_vector_command(&mut next, id, command)
        } else {
            next.validate().map_err(|e| e.to_string())
        };
        match result {
            Ok(()) => {
                changed = next != *project;
                *project = next;
                ui.ctx().data_mut(|data| data.remove::<String>(error_id));
            }
            Err(error) => ui.ctx().data_mut(|data| data.insert_temp(error_id, error)),
        }
    }
    if let Some(error) = ui.ctx().data(|data| data.get_temp::<String>(error_id)) {
        ui.colored_label(egui::Color32::RED, error);
    }
    changed
}

#[cfg(test)]
mod swatch_tests {
    use super::*;
    use splinely_document::Node;

    #[test]
    fn document_colors_are_distinct_and_skip_transparent() {
        let mut p = Project::default();
        let mut a = Node::rect("a", 0., 0., 1., 1.);
        a.style.fill = Color::rgb(255, 0, 0);
        a.style.stroke = Some(Color::rgb(0, 0, 255));
        let mut b = Node::rect("b", 0., 0., 1., 1.);
        b.style.fill = Color::rgb(255, 0, 0);
        let mut c = Node::line("c", 0., 0., 1., 1.).unwrap();
        c.style.stroke = Some(Color::rgb(0, 0, 255));
        let mut d = Node::rect("d", 0., 0., 1., 1.);
        d.gradient = Some(splinely_document::Gradient::linear(
            0.,
            Color::rgb(0, 255, 0),
            Color::rgb(255, 0, 0),
        ));
        p.nodes = vec![a, b, c, d];
        let colors = document_colors(&p);
        assert_eq!(
            colors,
            vec![
                Color::rgb(255, 0, 0),
                Color::rgb(0, 0, 255),
                Color::rgb(0, 255, 0)
            ]
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use splinely_document::Node;

    #[test]
    fn conversion_round_trip_and_unchanged_draft_preserve_provenance() {
        let mut project = Project::default();
        project.nodes.push(Node::rect("rect", 10., 20., 30., 40.));
        let original = project.clone();
        apply_vector_command(&mut project, "rect", VectorCommand::ConvertToPath).unwrap();
        let converted = project.clone();
        let NodeKind::Path { d, .. } = &project.nodes[0].kind else {
            panic!("expected path")
        };
        let d = d.clone();
        apply_vector_command(&mut project, "rect", VectorCommand::SetPath(d)).unwrap();
        assert_eq!(project, converted);
        apply_vector_command(&mut project, "rect", VectorCommand::ConvertBack).unwrap();
        assert_eq!(project, original);
    }

    #[test]
    fn changed_path_removes_original_kind_but_invalid_project_is_atomic() {
        let mut project = Project::default();
        project.nodes.push(Node::rect("rect", 0., 0., 10., 10.));
        apply_vector_command(&mut project, "rect", VectorCommand::ConvertToPath).unwrap();
        let converted = project.clone();
        project.nodes[0].style.opacity = 2.;
        let invalid = project.clone();
        assert!(apply_vector_command(
            &mut project,
            "rect",
            VectorCommand::SetPath("M0 0 L20 20".into())
        )
        .is_err());
        assert_eq!(project, invalid);
        project = converted;
        apply_vector_command(
            &mut project,
            "rect",
            VectorCommand::SetPath("M0 0 L20 20".into()),
        )
        .unwrap();
        assert!(project.nodes[0].original_kind.is_none());
        assert!(matches!(
            project.nodes[0].kind,
            NodeKind::Path {
                bounds: [0., 0., 20., 20.],
                ..
            }
        ));
    }

    #[test]
    fn rejected_path_changes_are_atomic_and_lock_aware() {
        let mut project = Project::default();
        project.nodes.push(Node::circle("circle", 10., 20., 5.));
        apply_vector_command(&mut project, "circle", VectorCommand::ConvertToPath).unwrap();
        let original = project.clone();
        for d in ["M0", "M0 0", "M0 0 L1e100 0"] {
            assert!(
                apply_vector_command(&mut project, "circle", VectorCommand::SetPath(d.into()))
                    .is_err()
            );
            assert_eq!(project, original);
        }
        project.nodes[0].locked = true;
        let locked = project.clone();
        assert!(apply_vector_command(
            &mut project,
            "circle",
            VectorCommand::SetPath("M0 0 L10 10".into())
        )
        .is_err());
        assert_eq!(project, locked);
    }
}
