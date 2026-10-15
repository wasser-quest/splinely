use super::*;

fn machine() -> StateMachine {
    StateMachine {
        id: "machine".into(),
        initial: "idle".into(),
        states: ["idle", "active", "disconnected"]
            .into_iter()
            .map(|id| State {
                id: id.into(),
                on_enter: vec![],
                on_exit: vec![],
                announce: None,
                overrides: vec![],
            })
            .collect(),
        transitions: vec![],
    }
}

#[test]
fn connections_preserve_order_support_loops_and_validate_atomically() {
    let mut m = machine();
    assert_eq!(connect(&mut m, 0, 1), Some(0));
    assert_eq!(connect(&mut m, 0, 1), Some(1));
    assert_eq!(connect(&mut m, 1, 1), Some(2));
    let saved = m.clone();
    assert_eq!(connect(&mut m, 99, 0), None);
    assert_eq!(connect(&mut m, 0, 99), None);
    assert_eq!(m, saved);
    let mut project = Project::default();
    project.state_machines.push(m.clone());
    replacement(&project, 0, m).unwrap();
    assert_eq!(project.state_machines[0].transitions[2].from, "active");
}

#[test]
fn layout_is_deterministic_nonoverlapping_and_cycle_safe() {
    let mut m = machine();
    connect(&mut m, 0, 1);
    connect(&mut m, 1, 0);
    connect(&mut m, 1, 1);
    let before = m.clone();
    let positions = auto_layout(&m);
    assert_eq!(positions, auto_layout(&m));
    assert!(positions[1].x > positions[0].x);
    assert_eq!(positions[2].x, positions[0].x);
    for (i, p) in positions.iter().enumerate() {
        for q in positions.iter().skip(i + 1) {
            assert!(!Rect::from_min_size(p.to_pos2(), Vec2::new(145.0, 42.0))
                .intersects(Rect::from_min_size(q.to_pos2(), Vec2::new(145.0, 42.0))));
        }
    }
    assert_eq!(m, before);
    assert!(auto_layout(&StateMachine {
        states: vec![],
        ..m
    })
    .is_empty());
}

#[test]
fn layout_survives_apply_but_machine_switch_cancels_drag() {
    let m = machine();
    let mut graph = StateGraph::default();
    graph.reset(0, &m);
    graph.positions[0] = Vec2::new(321.0, 123.0);
    graph.connecting = Some(0);
    let mut edited = m.clone();
    connect(&mut edited, 0, 1);
    graph.draft = Some(edited.clone());
    graph.reset(0, &edited);
    assert_eq!(graph.positions[0], Vec2::new(321.0, 123.0));
    assert_eq!(graph.connecting, None);
    graph.reset(1, &m);
    assert_eq!(graph.positions, auto_layout(&m));
}

#[test]
fn structured_controls_do_not_mutate_untouched_advanced_values() {
    use splinely_document::{Action, Guard};
    let mut project = Project::default();
    project
        .variables
        .insert("count".into(), serde_json::json!(2));
    let mut guard = Some(Guard {
        variable: "count".into(),
        equals: serde_json::json!(2),
    });
    let mut actions = vec![
        Action::SetVariable {
            name: "count".into(),
            value: serde_json::json!(9007199254740993_u64),
        },
        Action::BlendClip {
            clip_id: "unresolved".into(),
            weight: 0.5,
            duration: 7200.0,
        },
        Action::SetProperty {
            node_id: "unresolved".into(),
            property: splinely_document::Property::Gradient,
            value: splinely_document::AnimValue::Gradient(splinely_document::Gradient::linear(
                27.0,
                splinely_document::Color::rgb(10, 20, 30),
                splinely_document::Color::rgb(40, 50, 60),
            )),
        },
    ];
    let original = actions.clone();
    let original_guard = guard.clone();
    let context = egui::Context::default();
    for _ in 0..2 {
        let _ = context.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                controls::guard_ui(ui, &mut guard, &project);
                controls::actions_ui(ui, &mut actions, &project);
            });
        });
    }
    assert_eq!(actions, original);
    assert_eq!(guard, original_guard);
}

#[test]
fn pointer_drag_connects_and_escape_cancels_without_document_edit() {
    let mut m = machine();
    let mut positions = auto_layout(&m);
    let context = egui::Context::default();
    let mut selected = String::new();
    let mut edge = None;
    let mut connecting = None;
    let mut origin = Pos2::ZERO;
    let mut frame =
        |events: Vec<egui::Event>, m: &mut StateMachine, connecting: &mut Option<usize>| {
            let _ = context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 700.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        origin = ui.cursor().min;
                        draw_graph(ui, m, &mut selected, &mut edge, &mut positions, connecting);
                    });
                },
            );
            origin
        };
    let origin = frame(vec![], &mut m, &mut connecting);
    // ScrollArea content starts at the current cursor; initial layout puts root at (20,40).
    let source = origin + Vec2::new(165.0, 61.0);
    let target = origin + Vec2::new(90.0, 171.0);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(
        vec![egui::Event::PointerMoved(source), button(source, true)],
        &mut m,
        &mut connecting,
    );
    frame(
        vec![egui::Event::PointerMoved(source + Vec2::new(25.0, 0.0))],
        &mut m,
        &mut connecting,
    );
    frame(
        vec![egui::Event::PointerMoved(target)],
        &mut m,
        &mut connecting,
    );
    frame(vec![button(target, false)], &mut m, &mut connecting);
    assert_eq!(m.transitions.len(), 1);
    assert_eq!(m.transitions[0].from, "idle");
    assert_eq!(m.transitions[0].to, "active");
    connecting = Some(0);
    frame(
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
        &mut m,
        &mut connecting,
    );
    assert_eq!(connecting, None);
    assert_eq!(m.transitions.len(), 1);
    // Releasing a connector over empty canvas must not create an edge.
    connecting = Some(0);
    let outside = origin + Vec2::new(600.0, 50.0);
    frame(
        vec![egui::Event::PointerMoved(outside), button(outside, false)],
        &mut m,
        &mut connecting,
    );
    assert_eq!(connecting, None);
    assert_eq!(m.transitions.len(), 1);
    let saved = m.clone();
    let center = origin + Vec2::new(90.0, 61.0);
    let moved = center + Vec2::new(50.0, 20.0);
    frame(
        vec![egui::Event::PointerMoved(center), button(center, true)],
        &mut m,
        &mut connecting,
    );
    frame(
        vec![egui::Event::PointerMoved(moved)],
        &mut m,
        &mut connecting,
    );
    frame(vec![button(moved, false)], &mut m, &mut connecting);
    assert_eq!(positions[0], Vec2::new(70.0, 60.0));
    assert_eq!(m, saved);
}

#[test]
fn invalid_new_ids_are_rejected_without_losing_staged_edits() {
    let mut m = machine();
    connect(&mut m, 0, 1);
    let before = m.clone();
    for id in ["", "idle", "bad id", "9bad", "ä", "a/b", " a", "a\n"] {
        assert!(insert_state(&mut m, id).is_err(), "{id:?}");
        assert_eq!(m, before);
    }
    for id in ["_ready", "Ready-2.0"] {
        insert_state(&mut m, id).unwrap();
    }
    let mut project = Project::default();
    project.state_machines.push(m);
    project.validate().unwrap();
}

#[test]
fn priority_reorder_keeps_selected_transition_and_changes_first_match() {
    use splinely_document::MachineInstance;
    let mut m = machine();
    connect(&mut m, 0, 0); // Earlier self-loop shadows the new connection.
    connect(&mut m, 0, 1);
    let event = m.transitions[0].event.clone();
    let mut project = Project::default();
    project.state_machines.push(m.clone());
    let mut instance = MachineInstance::new(&project, "machine").unwrap();
    instance.send(&project, &event).unwrap();
    assert_eq!(instance.current_state, "idle");
    let mut selected = Some(1);
    let selected_transition = m.transitions[1].clone();
    move_transition(&mut m, &mut selected, 1, 0);
    assert_eq!(selected, Some(0));
    assert_eq!(m.transitions[selected.unwrap()], selected_transition);
    project.state_machines[0] = m.clone();
    instance.send(&project, &event).unwrap();
    assert_eq!(instance.current_state, "active");
    move_transition(&mut m, &mut selected, 0, 1);
    assert_eq!(selected, Some(1));
    assert_eq!(m.transitions[1], selected_transition);
    move_transition(&mut m, &mut selected, 0, 1);
    assert_eq!(selected, Some(0));
    assert_eq!(m.transitions[0], selected_transition);
}

#[test]
fn pointer_connections_accept_self_but_reject_clipped_targets() {
    for (position, offset, accepted) in [
        (Vec2::new(20.0, 40.0), Vec2::new(70.0, 21.0), true),
        (Vec2::new(20.0, 350.0), Vec2::new(70.0, 21.0), false),
        (Vec2::new(950.0, 40.0), Vec2::new(70.0, 21.0), false),
        // The node is partly visible, but this drop is below the viewport.
        (Vec2::new(20.0, 280.0), Vec2::new(70.0, 35.0), false),
    ] {
        let mut m = machine();
        let mut positions = vec![
            Vec2::new(20.0, 40.0),
            if accepted {
                Vec2::new(250.0, 40.0)
            } else {
                position
            },
            Vec2::new(20.0, 500.0),
        ];
        let context = egui::Context::default();
        let mut selected = String::new();
        let mut edge = None;
        let mut connecting = None;
        let mut origin = Pos2::ZERO;
        let mut frame = |events| {
            let _ = context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        origin = ui.cursor().min;
                        draw_graph(
                            ui,
                            &mut m,
                            &mut selected,
                            &mut edge,
                            &mut positions,
                            &mut connecting,
                        );
                    });
                },
            );
            origin
        };
        let origin = frame(vec![]);
        let source = origin + Vec2::new(165.0, 61.0);
        let target = origin + position + offset;
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(vec![
            egui::Event::PointerMoved(source),
            button(source, true),
        ]);
        frame(vec![egui::Event::PointerMoved(
            source + Vec2::new(25.0, 0.0),
        )]);
        frame(vec![egui::Event::PointerMoved(target)]);
        frame(vec![button(target, false)]);
        assert_eq!(connecting, None);
        assert_eq!(m.transitions.len(), usize::from(accepted), "{position:?}");
        if accepted {
            assert_eq!(edge, Some(0));
            assert_eq!(m.transitions[0].from, "idle");
            assert_eq!(m.transitions[0].to, "idle");
        }
    }
}

#[test]
fn numeric_text_edits_allow_fractions_and_preserve_json_representations() {
    let context = egui::Context::default();
    let mut number = serde_json::Number::from(1);
    let frame = |number: &mut serde_json::Number, text: Option<&str>| {
        let mut events = vec![];
        if let Some(text) = text {
            let modifiers = egui::Modifiers {
                ctrl: true,
                command: true,
                ..Default::default()
            };
            events.push(egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            });
            events.push(egui::Event::Text(text.into()));
            events.push(egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers,
            });
        }
        let _ = context.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    controls::number_ui(ui, number).request_focus();
                });
            },
        );
    };
    frame(&mut number, None);
    frame(&mut number, Some("1.5"));
    assert_eq!(number.to_string(), "1.5");
    frame(&mut number, Some("1.0"));
    assert!(number.is_f64());
    frame(&mut number, None);
    assert_eq!(number.to_string(), "1.0");
    frame(&mut number, Some("1"));
    assert!(number.is_i64());
    frame(&mut number, Some("-"));
    assert_eq!(number.to_string(), "1"); // Incomplete edits do not corrupt the draft.
    frame(&mut number, Some("2.5"));
    assert_eq!(number.to_string(), "2.5");
    for text in ["9007199254740993", "18446744073709551615", "1.0", "1"] {
        number = serde_json::from_str(text).unwrap();
        frame(&mut number, None);
        frame(&mut number, None);
        assert_eq!(number.to_string(), text);
    }
}

#[test]
fn numeric_pending_input_does_not_leak_between_equal_valued_variables() {
    use splinely_document::Guard;
    let context = egui::Context::default();
    let mut project = Project::default();
    project.variables.insert("a".into(), serde_json::json!(1));
    project.variables.insert("b".into(), serde_json::json!(1));
    let mut guard = Some(Guard {
        variable: "a".into(),
        equals: serde_json::json!(1),
    });
    let frame = |guard: &mut Option<Guard>, events| {
        context.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    controls::guard_ui(ui, guard, &project);
                });
            },
        )
    };
    let number_rect = |output: &egui::FullOutput| {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "1" => {
                    Some(Rect::from_min_size(text.pos, text.galley.size()))
                }
                _ => None,
            })
            .expect("numeric field must show the selected variable's value")
    };
    let output = frame(&mut guard, vec![]);
    let pos = number_rect(&output).center();
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(
        &mut guard,
        vec![egui::Event::PointerMoved(pos), button(true), button(false)],
    );
    frame(
        &mut guard,
        vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                },
            },
            egui::Event::Text("-".into()),
        ],
    );
    assert_eq!(guard.as_ref().unwrap().equals, serde_json::json!(1));
    guard.as_mut().unwrap().variable = "b".into();
    let output = frame(&mut guard, vec![]);
    number_rect(&output);
    assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Text(text) if text.galley.text() == "-")));
}

#[test]
fn timing_widget_limits_match_canonical_validation() {
    let mut m = machine();
    connect(&mut m, 0, 1);
    let t = &mut m.transitions[0];
    t.after = Some(MAX_TIMER_SECONDS);
    t.blend = MAX_TRANSITION_BLEND_SECONDS;
    t.event.name = TIMER_EVENT.into();
    let mut project = Project::default();
    project.state_machines.push(m.clone());
    replacement(&project, 0, m.clone()).unwrap();
    m.transitions[0].after = Some(MAX_TIMER_SECONDS + 0.01);
    assert!(replacement(&project, 0, m.clone()).is_err());
    m.transitions[0].after = Some(MAX_TIMER_SECONDS);
    m.transitions[0].blend = MAX_TRANSITION_BLEND_SECONDS + 0.01;
    assert!(replacement(&project, 0, m).is_err());
}
