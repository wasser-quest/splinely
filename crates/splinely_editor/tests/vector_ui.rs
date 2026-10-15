// Compile the owned modules directly: no library/app registration is needed.
#[path = "../src/layers.rs"]
mod layers;
#[allow(dead_code)] // Painting APIs are exercised by the application, not this headless fixture.
#[path = "../src/vector.rs"]
mod vector;

use layers::{apply_layer_command, LayerCommand};
use splinely_document::*;
use vector::{apply_vector_command, VectorCommand};

fn scene() -> Project {
    Project {
        nodes: vec![
            Node::rect("rect", 10.0, 20.0, 30.0, 40.0),
            Node::circle("circle", 80.0, 90.0, 10.0),
        ],
        ..Project::default()
    }
}

fn animate_rect(project: &mut Project, property: Property) {
    project.clips.push(Clip {
        audio_cues: vec![],
        blend_mode: Default::default(),
        weight: 1.0,
        id: "rect-copy-1".into(),
        name: String::new(),
        duration: 1.0,
        looping: false,
        autoplay: false,
        tracks: vec![Track {
            node_id: "rect".into(),
            property,
            keyframes: vec![Keyframe {
                time: 0.0,
                value: AnimValue::Number(1.0),
                easing: Easing::Linear,
            }],
        }],
    });
}

#[test]
fn duplication_avoids_shared_ids_and_copies_animation_tracks() {
    let mut project = scene();
    animate_rect(&mut project, Property::X);
    let source = project.nodes[0].clone();
    let mut selected = Some("rect".into());
    apply_layer_command(
        &mut project,
        &mut selected,
        LayerCommand::Duplicate("rect".into()),
    )
    .unwrap();
    assert_eq!(selected.as_deref(), Some("rect-copy-2"));
    assert_eq!(project.nodes[1].kind, source.kind);
    assert_eq!(project.nodes[1].style, source.style);
    assert_eq!(project.clips[0].tracks.len(), 2);
    assert_eq!(project.clips[0].tracks[0].node_id, "rect");
    assert_eq!(project.clips[0].tracks[1].node_id, "rect-copy-2");
    project.validate().unwrap();
}

#[test]
fn deletion_rejects_references_without_changing_selection_or_document() {
    let mut project = scene();
    animate_rect(&mut project, Property::X);
    let before = project.clone();
    let mut selected = Some("rect".into());
    assert!(apply_layer_command(
        &mut project,
        &mut selected,
        LayerCommand::Delete("rect".into())
    )
    .is_err());
    assert_eq!(project, before);
    assert_eq!(selected.as_deref(), Some("rect"));
    apply_layer_command(
        &mut project,
        &mut selected,
        LayerCommand::Delete("circle".into()),
    )
    .unwrap();
    assert_eq!(selected.as_deref(), Some("rect"));
    project.clips.clear();
    apply_layer_command(
        &mut project,
        &mut selected,
        LayerCommand::Delete("rect".into()),
    )
    .unwrap();
    assert_eq!(selected, None);
}

#[test]
fn deletion_rejects_event_targets() {
    let mut project = scene();
    project.state_machines.push(StateMachine {
        id: "machine".into(),
        initial: "idle".into(),
        states: vec![State {
            id: "idle".into(),
            on_enter: vec![],
            overrides: vec![],
            on_exit: vec![],
            announce: None,
        }],
        transitions: vec![Transition {
            from: "idle".into(),
            to: "idle".into(),
            event: Event {
                name: "click".into(),
                target: Some("rect".into()),
            },
            guard: None,
            actions: vec![],
            blend: 0.0,
            after: None,
        }],
    });
    let before = project.clone();
    assert!(
        apply_layer_command(&mut project, &mut None, LayerCommand::Delete("rect".into())).is_err()
    );
    assert_eq!(project, before);
}

#[test]
fn reorder_and_flags_are_transactional_and_lock_is_enforced() {
    let mut project = scene();
    let mut selected = Some("rect".into());
    apply_layer_command(
        &mut project,
        &mut selected,
        LayerCommand::Raise("rect".into()),
    )
    .unwrap();
    assert_eq!(project.nodes[1].id, "rect");
    apply_layer_command(
        &mut project,
        &mut selected,
        LayerCommand::Lower("rect".into()),
    )
    .unwrap();
    assert_eq!(project.nodes[0].id, "rect");
    apply_layer_command(
        &mut project,
        &mut selected,
        LayerCommand::Visibility {
            id: "rect".into(),
            visible: false,
        },
    )
    .unwrap();
    assert!(!project.nodes[0].visible);
    apply_layer_command(
        &mut project,
        &mut selected,
        LayerCommand::Lock {
            id: "rect".into(),
            locked: true,
        },
    )
    .unwrap();
    let before = project.clone();
    assert!(apply_layer_command(
        &mut project,
        &mut selected,
        LayerCommand::Delete("rect".into())
    )
    .is_err());
    assert!(apply_vector_command(&mut project, "rect", VectorCommand::ConvertToPath).is_err());
    assert_eq!(project, before);
    apply_layer_command(
        &mut project,
        &mut selected,
        LayerCommand::Lock {
            id: "rect".into(),
            locked: false,
        },
    )
    .unwrap();
    assert!(!project.nodes[0].locked);
}

#[test]
fn exact_shape_conversion_preserves_style_transform_and_roundtrips() {
    let mut project = scene();
    project.nodes[0].style.opacity = 0.4;
    project.nodes[0].transform.rotation = 23.0;
    if let NodeKind::Rect { rx, .. } = &mut project.nodes[0].kind {
        *rx = 5.0;
    }
    let before = project.clone();
    for id in ["rect", "circle"] {
        apply_vector_command(&mut project, id, VectorCommand::ConvertToPath).unwrap();
    }
    for node in &project.nodes {
        let NodeKind::Path { d, .. } = &node.kind else {
            panic!("expected path");
        };
        assert!(
            d.contains('A'),
            "rounded rects and circles must use real arcs"
        );
    }
    assert_eq!(project.nodes[0].style, before.nodes[0].style);
    assert_eq!(project.nodes[0].transform, before.nodes[0].transform);
    for id in ["rect", "circle"] {
        apply_vector_command(&mut project, id, VectorCommand::ConvertBack).unwrap();
    }
    assert_eq!(project, before);
}

#[test]
fn conversion_rejects_incompatible_animation_without_losing_tracks() {
    let mut project = scene();
    animate_rect(&mut project, Property::Width);
    let before = project.clone();
    assert!(apply_vector_command(&mut project, "rect", VectorCommand::ConvertToPath).is_err());
    assert_eq!(project, before);
}

#[test]
fn path_edits_recompute_bounds_and_invalid_drafts_do_not_commit() {
    let mut project = scene();
    apply_vector_command(&mut project, "rect", VectorCommand::ConvertToPath).unwrap();
    apply_vector_command(
        &mut project,
        "rect",
        VectorCommand::SetPath("M100 200 L140 200 L140 230 Z".into()),
    )
    .unwrap();
    let NodeKind::Path { d, bounds, .. } = &project.nodes[0].kind else {
        panic!("expected path");
    };
    assert_eq!(d, "M100 200 L140 200 L140 230 Z");
    assert_eq!(*bounds, [100.0, 200.0, 40.0, 30.0]);
    let before = project.clone();
    for invalid in ["M0", "M0 0 LNaN 2", "", "M0 0 L10000001 2"] {
        assert!(
            apply_vector_command(&mut project, "rect", VectorCommand::SetPath(invalid.into()))
                .is_err()
        );
        assert_eq!(project, before);
    }
    assert!(apply_vector_command(&mut project, "rect", VectorCommand::ConvertBack).is_err());
    assert_eq!(project, before);
}

#[test]
fn applying_unchanged_path_preserves_exact_conversion_provenance() {
    let mut project = scene();
    if let NodeKind::Rect { x, width, .. } = &mut project.nodes[0].kind {
        *x = 0.123456789;
        *width = 31.123456789;
    }
    let before = project.clone();
    apply_vector_command(&mut project, "rect", VectorCommand::ConvertToPath).unwrap();
    let NodeKind::Path { d, .. } = &project.nodes[0].kind else {
        panic!("expected path");
    };
    let d = d.clone();
    apply_vector_command(&mut project, "rect", VectorCommand::SetPath(d)).unwrap();
    apply_vector_command(&mut project, "rect", VectorCommand::ConvertBack).unwrap();
    assert_eq!(project, before);
}

#[test]
fn open_line_path_has_recomputed_selection_bounds() {
    let mut project = scene();
    apply_vector_command(&mut project, "rect", VectorCommand::ConvertToPath).unwrap();
    apply_vector_command(
        &mut project,
        "rect",
        VectorCommand::SetPath("M10 20 L40 50".into()),
    )
    .unwrap();
    let NodeKind::Path { bounds, .. } = project.nodes[0].kind else {
        panic!("expected path");
    };
    assert_eq!(bounds, [10.0, 20.0, 30.0, 30.0]);
}

// Render actual inspector widgets and click their painted labels, without app state.
fn inspector_frame(
    context: &eframe::egui::Context,
    project: &mut Project,
    selected: &mut Option<String>,
    events: Vec<eframe::egui::Event>,
) -> (bool, Vec<(String, eframe::egui::Pos2)>) {
    use eframe::egui::{epaint::Shape, CentralPanel, RawInput, Rect, Vec2};
    let mut changed = false;
    let output = context.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Default::default(), Vec2::splat(900.0))),
            events,
            ..Default::default()
        },
        |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                changed = vector::vector_inspector_ui(project, ui, selected);
            });
        },
    );
    let labels = output
        .shapes
        .into_iter()
        .filter_map(|shape| {
            if let Shape::Text(text) = shape.shape {
                Some((
                    text.galley.text().to_owned(),
                    text.pos + text.galley.size() * 0.5,
                ))
            } else {
                None
            }
        })
        .collect();
    (changed, labels)
}

fn click_inspector(
    context: &eframe::egui::Context,
    project: &mut Project,
    selected: &mut Option<String>,
    label: &str,
) -> bool {
    use eframe::egui::{Event, Modifiers, PointerButton};
    let (_, labels) = inspector_frame(context, project, selected, vec![]);
    let pos = labels
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("missing inspector label {label}: {labels:?}"))
        .1;
    let mut changed = false;
    for pressed in [true, false] {
        changed |= inspector_frame(
            context,
            project,
            selected,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::default(),
                },
            ],
        )
        .0;
    }
    changed
}

#[test]
fn inspector_guides_selection_and_exposes_thickness_without_a_border() {
    let context = eframe::egui::Context::default();
    let mut project = scene();
    let before = project.clone();
    let mut selected = None;
    let (changed, labels) = inspector_frame(&context, &mut project, &mut selected, vec![]);
    assert!(!changed);
    assert!(labels.iter().any(|(text, _)| text.contains("Form wählen")));
    selected = Some("missing".into());
    let (changed, labels) = inspector_frame(&context, &mut project, &mut selected, vec![]);
    assert!(!changed);
    assert!(labels
        .iter()
        .any(|(text, _)| text.contains("Auswahl nicht gefunden")));
    selected = Some("rect".into());
    let (changed, labels) = inspector_frame(&context, &mut project, &mut selected, vec![]);
    assert!(!changed);
    for label in [
        "Füllung",
        "Rand (Kontur)",
        "Randstärke",
        "Deckkraft",
        "1 px",
        "2 px",
        "4 px",
        "8 px",
        "−",
        "+",
    ] {
        assert!(
            labels.iter().any(|(text, _)| text == label),
            "missing {label}"
        );
    }
    assert_eq!(project, before);
}

#[test]
fn inspector_border_defaults_presets_and_steps_commit_valid_edits() {
    let context = eframe::egui::Context::default();
    let mut project = scene();
    let mut selected = Some("rect".into());
    project.nodes[0].style.stroke_width = 0.0;
    assert!(click_inspector(
        &context,
        &mut project,
        &mut selected,
        "Rand (Kontur)"
    ));
    assert_eq!(project.nodes[0].style.stroke, Some(Color::rgb(0, 0, 0)));
    assert_eq!(project.nodes[0].style.stroke_width, 1.0);
    let (_, labels) = inspector_frame(&context, &mut project, &mut selected, vec![]);
    assert!(labels.iter().any(|(text, _)| text == "Randfarbe"));
    assert!(click_inspector(
        &context,
        &mut project,
        &mut selected,
        "Rand (Kontur)"
    ));
    assert_eq!(project.nodes[0].style.stroke, None);
    assert!(click_inspector(
        &context,
        &mut project,
        &mut selected,
        "4 px"
    ));
    assert_eq!(project.nodes[0].style.stroke_width, 4.0);
    assert_eq!(project.nodes[0].style.stroke, None);
    assert!(click_inspector(&context, &mut project, &mut selected, "+"));
    assert_eq!(project.nodes[0].style.stroke_width, 5.0);
    assert!(click_inspector(&context, &mut project, &mut selected, "−"));
    assert_eq!(project.nodes[0].style.stroke_width, 4.0);
    assert!(click_inspector(
        &context,
        &mut project,
        &mut selected,
        "Rand (Kontur)"
    ));
    assert_eq!(project.nodes[0].style.stroke_width, 4.0);
    project.validate().unwrap();
    project.nodes[0].locked = true;
    let before = project.clone();
    assert!(!click_inspector(
        &context,
        &mut project,
        &mut selected,
        "8 px"
    ));
    assert_eq!(project, before);
}

#[test]
fn inspector_rejects_style_edits_atomically_when_validation_fails() {
    let context = eframe::egui::Context::default();
    let mut project = scene();
    let mut selected = Some("rect".into());
    // An invalid external document must not let a style edit bypass validation.
    project.nodes[1].id = "rect".into();
    let before = project.clone();
    assert!(!click_inspector(
        &context,
        &mut project,
        &mut selected,
        "8 px"
    ));
    assert_eq!(project, before);
}

#[test]
fn inspector_does_not_quantize_opacity_or_clamp_imported_width_on_display() {
    let context = eframe::egui::Context::default();
    let mut project = scene();
    let mut selected = Some("rect".into());
    project.nodes[0].style.opacity = 0.123456789;
    project.nodes[0].style.stroke_width = 2000.0;
    project.validate().unwrap();
    let before = project.clone();
    assert!(!inspector_frame(&context, &mut project, &mut selected, vec![]).0);
    assert_eq!(project, before);
}

#[test]
fn inspector_thickness_steps_stop_at_range_limits() {
    let context = eframe::egui::Context::default();
    let mut project = scene();
    let mut selected = Some("rect".into());
    project.nodes[0].style.stroke_width = 0.0;
    assert!(!click_inspector(&context, &mut project, &mut selected, "−"));
    assert_eq!(project.nodes[0].style.stroke_width, 0.0);
    project.nodes[0].style.stroke_width = 1000.0;
    assert!(!click_inspector(&context, &mut project, &mut selected, "+"));
    assert_eq!(project.nodes[0].style.stroke_width, 1000.0);
}

#[test]
fn headless_ui_does_not_report_selection_as_a_document_edit() {
    let context = eframe::egui::Context::default();
    let mut project = scene();
    let before = project.clone();
    let mut selected = Some("rect".into());
    let _ = context.run(Default::default(), |ctx| {
        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            assert!(!layers::layers_ui(&mut project, ui, &mut selected));
            assert!(!vector::vector_inspector_ui(
                &mut project,
                ui,
                &mut selected
            ));
        });
    });
    assert_eq!(project, before);
}

#[test]
fn inspector_switches_between_colour_linear_and_radial_gradients() {
    let context = eframe::egui::Context::default();
    let mut project = scene();
    let mut selected = Some("rect".into());
    let fill = project.nodes[0].style.fill;
    assert!(click_inspector(
        &context,
        &mut project,
        &mut selected,
        "Farbverlauf"
    ));
    let gradient = project.nodes[0].gradient.clone().expect("gradient created");
    assert!(!gradient.radial);
    assert_eq!(
        gradient.stops[0].color, fill,
        "starts from the current fill"
    );
    let (_, labels) = inspector_frame(&context, &mut project, &mut selected, vec![]);
    for label in ["Richtung", "+ Farbstopp"] {
        assert!(labels.iter().any(|(t, _)| t == label), "missing {label}");
    }
    assert!(click_inspector(
        &context,
        &mut project,
        &mut selected,
        "+ Farbstopp"
    ));
    assert_eq!(project.nodes[0].gradient.as_ref().unwrap().stops.len(), 3);
    assert!(click_inspector(
        &context,
        &mut project,
        &mut selected,
        "Radial"
    ));
    assert!(project.nodes[0].gradient.as_ref().unwrap().radial);
    project.validate().unwrap();
    assert!(click_inspector(
        &context,
        &mut project,
        &mut selected,
        "Farbe"
    ));
    assert!(project.nodes[0].gradient.is_none());
    assert_eq!(project.nodes[0].style.fill, fill);
}
