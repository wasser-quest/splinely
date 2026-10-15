use super::*;

#[test]
fn pan_uses_screen_points_at_every_zoom() {
    for zoom in [0.1, 1., 3.] {
        let viewport = Rect::from_min_size(Pos2::new(20., 30.), Vec2::new(800., 600.));
        let document = Vec2::new(500., 400.);
        let mut gesture = PanGesture::default();
        let mut pan = Vec2::ZERO;
        assert!(gesture.update(
            PanInput {
                in_canvas: true,
                middle_pressed: true,
                middle_down: true,
                ..Default::default()
            },
            &mut pan
        ));
        let before = canvas_origin(viewport, document, zoom, pan);
        assert!(gesture.update(
            PanInput {
                middle_down: true,
                delta: Vec2::new(73., -41.),
                ..Default::default()
            },
            &mut pan
        ));
        assert_eq!(
            canvas_origin(viewport, document, zoom, pan) - before,
            Vec2::new(73., -41.)
        );
    }
}

#[test]
fn zoom_preserves_center_anchor_and_fit_recenters() {
    let viewport = Rect::from_min_size(Pos2::new(100., 50.), Vec2::new(900., 700.));
    let document = Vec2::new(640., 480.);
    let pan = Vec2::new(140., -80.);
    let old_zoom = 0.8;
    let new_zoom = 2.4;
    let before = (viewport.center() - canvas_origin(viewport, document, old_zoom, pan)) / old_zoom;
    let after = (viewport.center()
        - canvas_origin(
            viewport,
            document,
            new_zoom,
            zoom_pan(pan, old_zoom, new_zoom),
        ))
        / new_zoom;
    assert!((before - after).length() < 0.001);
    let zoom = fit_zoom(viewport.size(), document.x as f64, document.y as f64);
    let rect = Rect::from_min_size(
        canvas_origin(viewport, document, zoom, Vec2::ZERO),
        document * zoom,
    );
    assert_eq!(rect.center(), viewport.center());
    assert!(viewport.contains_rect(rect));
}

#[test]
fn pan_owns_release_and_survives_modifier_release_and_leaving_canvas() {
    let mut gesture = PanGesture::default();
    let mut pan = Vec2::ZERO;
    assert!(gesture.update(
        PanInput {
            in_canvas: true,
            space: true,
            primary_pressed: true,
            primary_down: true,
            ..Default::default()
        },
        &mut pan
    ));
    assert!(gesture.update(
        PanInput {
            primary_down: true,
            delta: Vec2::new(20., 30.),
            ..Default::default()
        },
        &mut pan
    ));
    assert_eq!(pan, Vec2::new(20., 30.));
    assert!(gesture.update(PanInput::default(), &mut pan));
    assert!(!gesture.update(PanInput::default(), &mut pan));
}

#[test]
fn middle_takes_precedence_and_never_hands_held_primary_to_editing() {
    let mut gesture = PanGesture::default();
    let mut pan = Vec2::ZERO;
    assert!(gesture.update(
        PanInput {
            in_canvas: true,
            space: true,
            middle_pressed: true,
            middle_down: true,
            primary_pressed: true,
            primary_down: true,
            ..Default::default()
        },
        &mut pan
    ));
    assert_eq!(gesture.button, Some(egui::PointerButton::Middle));
    assert!(gesture.update(
        PanInput {
            primary_down: true,
            delta: Vec2::splat(10.),
            ..Default::default()
        },
        &mut pan
    ));
    assert_eq!(pan, Vec2::ZERO);
    assert!(gesture.update(PanInput::default(), &mut pan));
    assert!(!gesture.update(PanInput::default(), &mut pan));
}

#[test]
fn ordinary_edit_drag_and_external_press_do_not_start_pan() {
    let mut gesture = PanGesture::default();
    let mut pan = Vec2::ZERO;
    for input in [
        PanInput {
            in_canvas: true,
            primary_pressed: true,
            primary_down: true,
            ..Default::default()
        },
        PanInput {
            middle_pressed: true,
            middle_down: true,
            ..Default::default()
        },
        PanInput {
            in_canvas: true,
            middle_down: true,
            ..Default::default()
        },
        PanInput {
            in_canvas: true,
            space: true,
            primary_down: true,
            ..Default::default()
        },
    ] {
        assert!(!gesture.update(input, &mut pan));
    }
    // Space may take over an existing canvas edit drag, but not another widget's drag.
    assert!(gesture.update(
        PanInput {
            space: true,
            primary_down: true,
            primary_dragging: true,
            ..Default::default()
        },
        &mut pan
    ));
}

#[test]
fn freeze_cancels_pending_pen_press_without_deleting_draft_or_mutating_document() {
    let mut editor = Editor::default();
    let before = editor.project.clone();
    let mut pen = PenPath::default();
    pen.add_anchor([10., 20.], None).unwrap();
    editor.pen = Some(pen);
    editor.pen_press = Some([80., 90.]);
    let draft = editor.pen.as_ref().unwrap().path().path_data();
    editor.freeze_canvas_gesture();
    assert_eq!(editor.project, before);
    assert!(editor.pen_press.is_none());
    assert_eq!(editor.pen.as_ref().unwrap().path().path_data(), draft);
}

#[test]
fn freeze_node_drag_does_not_apply_release_snap() {
    let mut editor = Editor::default();
    editor.selected = Some("circle".into());
    editor.snap_grid = true;
    editor.drag_start = Some(editor.project.clone());
    editor.drag_by(Vec2::new(3., 7.));
    let frozen = editor.project.clone();
    editor.freeze_canvas_gesture();
    assert_eq!(editor.project, frozen);
    assert!(editor.drag_start.is_none());
    assert!(editor.history.undo(&mut editor.project));
    assert_eq!(editor.project, demo());
}

#[test]
fn freeze_discards_speculative_path_handle_changes() {
    let mut editor = Editor::default();
    let mut node = Node::rect("path", 0., 0., 100., 100.);
    node.kind = NodeKind::Path {
        d: "M0 0 L100 100".into(),
        bounds: [0., 0., 100., 100.],
        evenodd: false,
    };
    editor.project.nodes.push(node);
    let before = editor.project.clone();
    let map = CanvasMap::from_transform(Pos2::ZERO, 1., Transform::default());
    let mut gesture = PathGesture::begin(&editor.project, "path", Pos2::ZERO, &map)
        .unwrap()
        .unwrap();
    gesture.update(Pos2::new(40., 50.), &map).unwrap();
    editor.path_drag = Some((gesture, map, "path".into()));
    editor.freeze_canvas_gesture();
    assert!(editor.path_drag.is_none());
    assert_eq!(editor.project, before);
}

fn canvas_frame(editor: &mut Editor, ctx: &egui::Context, events: Vec<egui::Event>) {
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 800.))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| editor.canvas(ui));
        },
    );
}

#[test]
fn canvas_pan_never_edits_pen_nodes_or_bones_even_on_release() {
    for button in [egui::PointerButton::Middle, egui::PointerButton::Primary] {
        for bones in [false, true] {
            let ctx = egui::Context::default();
            let mut editor = Editor {
                selected: Some("circle".into()),
                ..Default::default()
            };
            if bones {
                editor.add_bone(false);
                editor.tab = Tab::Bones;
            } else {
                let mut pen = PenPath::default();
                pen.add_anchor([10., 20.], None).unwrap();
                editor.pen = Some(pen);
            }
            let before = editor.project.clone();
            let selected = editor.selected.clone();
            let selected_bone = editor.selected_bone.clone();
            canvas_frame(&mut editor, &ctx, vec![]);
            let start = Pos2::new(400., 400.);
            let end = Pos2::new(470., 440.);
            let mut events = vec![egui::Event::PointerMoved(start)];
            if button == egui::PointerButton::Primary {
                events.push(egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            events.push(egui::Event::PointerButton {
                pos: start,
                button,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            });
            canvas_frame(&mut editor, &ctx, events);
            canvas_frame(&mut editor, &ctx, vec![egui::Event::PointerMoved(end)]);
            canvas_frame(
                &mut editor,
                &ctx,
                vec![egui::Event::PointerButton {
                    pos: end,
                    button,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            assert_eq!(editor.pan_offset, Vec2::new(70., 40.));
            assert_eq!(editor.project, before);
            assert_eq!(editor.selected, selected);
            assert_eq!(editor.selected_bone, selected_bone);
            assert!(editor.drag_start.is_none());
            assert!(editor.pen_press.is_none());
            if !bones {
                assert_eq!(editor.pen.as_ref().unwrap().path().segments().len(), 1);
            }
        }
    }
}

#[test]
fn document_sync_resets_view_and_pan_capture() {
    let mut editor = Editor {
        pan_offset: Vec2::new(100., -500.),
        ..Default::default()
    };
    editor.pan_gesture.button = Some(egui::PointerButton::Middle);
    editor.fit_requested = false;
    editor.sync_buffers();
    assert_eq!(editor.pan_offset, Vec2::ZERO);
    assert!(editor.pan_gesture.button.is_none());
    assert!(editor.fit_requested);
}
