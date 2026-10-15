use super::*;

#[test]
fn stopping_audio_keeps_the_single_preparation_slot() {
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut editor = Editor {
        audio_loading: Some(receiver),
        audio_load_wanted: true,
        ..Default::default()
    };
    editor.disable_native_audio();
    assert!(editor.audio_loading.is_some());
    assert!(!editor.audio_load_wanted);
    sender
        .send(Ok(audio_preview::AudioPreview::new()))
        .ok()
        .unwrap();
    editor.poll_native_audio(&egui::Context::default());
    assert!(editor.audio_loading.is_none());
    assert!(!editor.audio_enabled);
}

#[test]
fn grid_snap_is_one_undo_transaction() {
    let mut editor = Editor {
        snap_grid: true,
        selected: Some("circle".into()),
        ..Default::default()
    };
    let before = editor.project.clone();
    editor.drag_start = Some(before.clone());
    editor.drag_by(Vec2::new(13., 27.));
    editor.finish_drag();
    assert_eq!(editor.project.nodes[0].transform.x, 10.);
    assert_eq!(editor.project.nodes[0].transform.y, 30.);
    assert!(editor.history.undo(&mut editor.project));
    assert_eq!(editor.project, before);
}

#[test]
fn incremental_drag_accumulates_and_undo_restores() {
    let mut e = Editor::default();
    e.selected = Some("circle".into());
    e.drag_start = Some(e.project.clone());
    for delta in [Vec2::new(10., 5.), Vec2::new(20., -5.), Vec2::ZERO] {
        e.drag_by(delta);
    }
    assert_eq!(e.project.nodes[0].transform.x, 30.);
    assert_eq!(e.project.nodes[0].transform.y, 0.);
    e.finish_drag();
    assert!(e.history.undo(&mut e.project));
    assert_eq!(e.project.nodes[0].transform.x, 0.);
}

#[test]
fn continuous_property_edits_are_one_transaction() {
    let mut e = Editor {
        editing: true,
        ..Editor::default()
    };
    for x in [10., 20., 30.] {
        let mut p = e.project.clone();
        p.nodes[0].transform.x = x;
        e.commit(p);
    }
    assert!(!e.history.can_undo());
    e.editing = false;
    e.finish_edit();
    assert!(e.history.undo(&mut e.project));
    assert_eq!(e.project.nodes[0].transform.x, 0.);
    assert!(!e.history.can_undo());
}

#[test]
fn invalid_changes_leave_document_and_history_untouched() {
    let mut e = Editor::default();
    let before = e.project.clone();
    let mut bad = before.clone();
    bad.width = -1.;
    e.commit(bad);
    assert_eq!(e.project, before);
    assert!(!e.history.can_undo());
}

#[test]
fn active_state_and_explicit_overrides_match_ecs_evaluation() {
    let mut e = Editor::default();
    e.project.state_machines[0].states[0]
        .overrides
        .push(PropertyOverride {
            node_id: "circle".into(),
            property: Property::Opacity,
            value: AnimValue::Number(0.2),
        });
    e.project.state_machines[0].states[1]
        .overrides
        .push(PropertyOverride {
            node_id: "circle".into(),
            property: Property::Opacity,
            value: AnimValue::Number(0.8),
        });
    e.send_event("click".into(), Some("circle".into()));
    assert_eq!(e.machines[0].current_state, "active");
    e.time = 0.75;
    let context = e.preview_context();
    let scene = e.projection.evaluate_project(
        &e.project,
        e.time,
        Some("intro".into()),
        true,
        context.states,
        context.overrides,
    );
    assert!((scene[0].opacity - 0.8).abs() < 1e-6);
    assert!((scene[0].x - 340.).abs() < 1e-6);
    assert_eq!(e.project.nodes[0].transform.x, 0.);
    e.runtime_overrides.push(PropertyOverride {
        node_id: "circle".into(),
        property: Property::X,
        value: AnimValue::Number(42.),
    });
    assert_eq!(e.evaluated()[0].transform.x, 42.);
}

#[test]
fn initial_actions_run_once_and_stop_clip_respects_id() {
    let mut e = Editor::default();
    e.project.state_machines[0].states[0]
        .on_enter
        .push(Action::SetProperty {
            node_id: "circle".into(),
            property: Property::Y,
            value: AnimValue::Number(12.),
        });
    e.project.state_machines[0].states[0]
        .on_enter
        .push(Action::PlayClip {
            clip_id: "intro".into(),
        });
    e.dispatch_events(None);
    assert!(e.blend_preview.layers["intro"].playing);
    assert_eq!(e.evaluated()[0].transform.y, 12.);
    let count = e.event_log.len();
    e.dispatch_events(None);
    assert_eq!(e.event_log.len(), count);
    let mut queue = std::collections::VecDeque::new();
    e.preview_actions(
        vec![Action::StopClip {
            clip_id: "other".into(),
        }],
        &mut queue,
    );
    assert!(e.blend_preview.layers["intro"].playing);
    e.preview_actions(
        vec![Action::StopClip {
            clip_id: "intro".into(),
        }],
        &mut queue,
    );
    assert!(!e.blend_preview.layers["intro"].active);
}

#[test]
fn event_recursion_is_bounded() {
    let mut e = Editor::default();
    e.project.state_machines[0].transitions = vec![Transition {
        from: "idle".into(),
        to: "idle".into(),
        event: Event {
            name: "loop".into(),
            target: None,
        },
        guard: None,
        actions: vec![Action::Emit {
            event: Event {
                name: "loop".into(),
                target: None,
            },
        }],
    }];
    e.send_event("loop".into(), None);
    assert!(e.status.contains("32 Ereignissen"));
    assert!(e.event_log.len() <= 200);
}

#[test]
fn hit_test_inverts_rotation_and_scale() {
    let mut node = Node::rect("rect", 10., 20., 100., 50.);
    node.transform = Transform {
        x: 100.,
        y: 50.,
        rotation: 37.,
        scale_x: 2.,
        scale_y: 0.5,
    };
    let scene = project_node(&node).unwrap();
    assert!(hit(&scene, local_to_scene(&scene, Vec2::new(50., 25.))));
    assert!(!hit(&scene, local_to_scene(&scene, Vec2::new(-1., 25.))));
}

#[test]
fn atomic_project_save_and_interactive_export_preserve_source_and_assets() {
    let mut p = demo();
    p.custom_js = "splinely.set('circle', {opacity: 0.5});".into();
    p.raw_svg = "<svg xmlns='http://www.w3.org/2000/svg'><path d='M0 0'/></svg>".into();
    p.audio_assets.push(AudioAsset {
        id: "sound".into(),
        name: "sound.wav".into(),
        mime_type: "audio/wav".into(),
        data_base64: "UklGRg==".into(),
    });
    p.audio_cues.push(AudioCue {
        id: "cue".into(),
        asset_id: "sound".into(),
        time: 1.5,
        offset: 0.,
        duration: None,
        volume: 1.,
    });
    let path = std::env::temp_dir().join(format!("splinely-editor-{}.json", std::process::id()));
    let svg = path.with_extension("svg");
    atomic_save(path.to_str().unwrap(), &p.to_json().unwrap()).unwrap();
    let loaded = Project::from_json(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(loaded.custom_js, p.custom_js);
    assert_eq!(loaded.raw_svg, p.raw_svg);
    assert_eq!(loaded.audio_assets, p.audio_assets);
    export_file(path.to_str().unwrap(), svg.to_str().unwrap()).unwrap();
    let source = fs::read_to_string(&svg).unwrap();
    let xml = roxmltree::Document::parse(&source).unwrap();
    let data = xml
        .descendants()
        .find(|n| n.attribute("id") == Some("splinely-project"))
        .unwrap()
        .text()
        .unwrap();
    let exported = Project::from_json(data).unwrap();
    assert_eq!(exported.audio_cues, p.audio_cues);
    assert!(source.contains("data-splinely-node"));
    assert!(source.contains("<script"));
    fs::remove_file(path).unwrap();
    fs::remove_file(svg).unwrap();
}

#[test]
fn machines_observe_previous_machine_actions_in_document_order() {
    let mut e = Editor::default();
    e.project
        .variables
        .insert("ready".into(), serde_json::json!(false));
    let mut a = e.project.state_machines[0].clone();
    a.transitions[0].actions = vec![Action::SetVariable {
        name: "ready".into(),
        value: serde_json::json!(true),
    }];
    let mut b = a.clone();
    b.id = "second".into();
    b.transitions[0].actions.clear();
    b.transitions[0].guard = Some(Guard {
        variable: "ready".into(),
        equals: serde_json::json!(true),
    });
    e.project.state_machines = vec![a, b];
    e.send_event("click".into(), Some("circle".into()));
    assert_eq!(e.machines[0].current_state, "active");
    assert_eq!(e.machines[1].current_state, "active");
}

#[test]
fn event_play_uses_independent_clock_and_stop_removes_clip_contribution() {
    let mut e = Editor {
        time: 0.75,
        ..Editor::default()
    };
    let mut pending = std::collections::VecDeque::new();
    e.preview_actions(
        vec![Action::PlayClip {
            clip_id: "intro".into(),
        }],
        &mut pending,
    );
    assert_eq!(e.time, 0.75);
    assert_eq!(e.blend_preview.layers["intro"].time, 0.);
    e.advance_layers(0.5);
    assert_eq!(e.blend_preview.layers["intro"].time, 0.5);
    e.preview_actions(
        vec![Action::StopClip {
            clip_id: "intro".into(),
        }],
        &mut pending,
    );
    assert_eq!(e.time, 0.75);
    assert!(!e.preview_animation);
    assert_eq!(e.evaluated()[0].transform.x, 0.);
}

#[test]
fn stroked_horizontal_paths_have_a_pick_tolerance() {
    let mut node = Node::rect("line", 0., 0., 10., 10.);
    node.kind = NodeKind::Path {
        d: "M0 0 L100 0".into(),
        bounds: [0., 0., 100., 0.],
        evenodd: false,
    };
    node.style.stroke = Some(Color::rgb(0, 0, 0));
    node.style.stroke_width = 4.;
    let scene = project_node(&node).unwrap();
    assert!(hit(&scene, Vec2::new(50., 1.5)));
    assert!(!hit(&scene, Vec2::new(50., 20.)));
}

#[test]
fn path_picking_follows_the_shape_not_its_bounding_box() {
    // Two bars like a letterbox: the empty middle must not pick the shape.
    let mut node = Node::rect("bars", 0., 0., 10., 10.);
    let d = "M0 0 H100 V10 H0 Z M0 90 H100 V100 H0 Z";
    node.kind = NodeKind::Path {
        d: d.into(),
        bounds: splinely_document::path_bounds(d).unwrap(),
        evenodd: false,
    };
    let scene = project_node(&node).unwrap();
    assert!(hit(&scene, Vec2::new(50., 5.)));
    assert!(hit(&scene, Vec2::new(50., 95.)));
    assert!(!hit(&scene, Vec2::new(50., 50.)));
    // Even-odd ring: the hole is empty, the band is solid.
    let d = "M0 0 H100 V100 H0 Z M25 25 H75 V75 H25 Z";
    node.kind = NodeKind::Path {
        d: d.into(),
        bounds: splinely_document::path_bounds(d).unwrap(),
        evenodd: true,
    };
    let scene = project_node(&node).unwrap();
    assert!(hit(&scene, Vec2::new(10., 50.)));
    assert!(!hit(&scene, Vec2::new(50., 50.)));
    // Invisible (opacity 0) shapes are never picked.
    node.style.opacity = 0.;
    let scene = project_node(&node).unwrap();
    assert!(!hit(&scene, Vec2::new(10., 50.)));
}

fn blend_editor() -> Editor {
    let mut e = Editor::default();
    e.project.state_machines.clear();
    e.project.clips[0].blend_mode = BlendMode::Weighted;
    e.project.clips[0].weight = 1.;
    e.project.clips[0].looping = false;
    let mut second = e.project.clips[0].clone();
    second.id = "second".into();
    second.weight = 0.;
    second.duration = 1.;
    second.tracks[0].keyframes = vec![Keyframe {
        time: 0.,
        value: AnimValue::Number(200.),
        easing: Easing::Linear,
    }];
    e.project.clips.push(second);
    e
}

#[test]
fn native_blend_actions_interrupt_pause_stop_and_preserve_document() {
    let mut e = blend_editor();
    let json = e.project.to_json().unwrap();
    let mut pending = std::collections::VecDeque::new();
    e.preview_actions(
        vec![Action::BlendClip {
            clip_id: "second".into(),
            weight: 1.,
            duration: 2.,
        }],
        &mut pending,
    );
    assert!(!e.blend_preview.enabled);
    assert!(!e.blend_preview.layers["second"].active);
    e.advance_layers(1.);
    assert_eq!(e.blend_preview.layers["second"].weight, 0.5);
    e.preview_actions(
        vec![Action::PlayClip {
            clip_id: "second".into(),
        }],
        &mut pending,
    );
    e.blend_preview.pause("second");
    e.preview_actions(
        vec![Action::BlendClip {
            clip_id: "second".into(),
            weight: 0.,
            duration: 1.,
        }],
        &mut pending,
    );
    e.advance_layers(0.5);
    assert_eq!(e.blend_preview.layers["second"].weight, 0.25);
    assert_eq!(e.blend_preview.layers["second"].time, 0.);
    e.preview_actions(
        vec![Action::StopClip {
            clip_id: "second".into(),
        }],
        &mut pending,
    );
    e.advance_layers(5.);
    assert_eq!(e.blend_preview.layers["second"].weight, 0.25);
    assert!(!e.blend_preview.layers["second"].active);
    e.preview_actions(
        vec![Action::BlendClip {
            clip_id: "second".into(),
            weight: 0.,
            duration: 0.,
        }],
        &mut pending,
    );
    assert_eq!(e.blend_preview.layers["second"].weight, 0.);
    assert!(e.status.starts_with("Mischung"));
    assert_eq!(e.project.to_json().unwrap(), json);
}

#[test]
fn native_layer_weights_reach_projection_without_changing_manual_selection() {
    let mut e = blend_editor();
    e.time = 0.75;
    e.preview_animation = true;
    assert_eq!(e.evaluated()[0].transform.x, 160.);
    e.blend_preview.play(&e.project, "intro").unwrap();
    e.blend_preview.play(&e.project, "second").unwrap();
    e.blend_preview.fade(&e.project, "intro", 0.5, 0.).unwrap();
    e.blend_preview.fade(&e.project, "second", 0.5, 0.).unwrap();
    e.advance_layers(0.75);
    assert_eq!(e.time, 0.75);
    assert_eq!(e.active_clip, 0);
    let context = e.preview_context();
    assert_eq!(context.clips.as_ref().unwrap().len(), 2);
    assert_eq!(e.evaluated()[0].transform.x, 180.);
    let scene = e
        .projection
        .evaluate_with_context(&e.project, e.time, &context);
    assert_eq!(scene[0].x, 360.); // circle center includes its 180-unit geometry origin
    e.blend_preview = Default::default();
    assert_eq!(e.evaluated()[0].transform.x, 160.);
}

#[test]
fn native_completion_actions_are_segmented_and_parallel_machines_keep_scrolling() {
    fn setup() -> Editor {
        let mut e = blend_editor();
        e.project.clips[0].looping = true;
        e.project.clips[0].blend_mode = BlendMode::Override;
        e.project.clips[0].tracks[0].property = Property::Y;
        let transition = Transition {
            from: "idle".into(),
            to: "done".into(),
            event: Event {
                name: "clip.end".into(),
                target: Some("second".into()),
            },
            guard: None,
            actions: vec![Action::BlendClip {
                clip_id: "second".into(),
                weight: 0.,
                duration: 1.,
            }],
        };
        for id in ["scroll_machine", "jump_machine"] {
            e.project.state_machines.push(StateMachine {
                id: id.into(),
                initial: "idle".into(),
                states: vec![
                    State {
                        id: "idle".into(),
                        on_enter: vec![],
                        overrides: vec![],
                    },
                    State {
                        id: "done".into(),
                        on_enter: vec![],
                        overrides: vec![],
                    },
                ],
                transitions: if id == "jump_machine" {
                    vec![transition.clone()]
                } else {
                    vec![]
                },
            });
        }
        e.dispatch_events(None);
        e.blend_preview.play(&e.project, "intro").unwrap();
        e.blend_preview.play(&e.project, "second").unwrap();
        e.blend_preview.fade(&e.project, "second", 1., 0.).unwrap();
        e
    }
    let mut whole = setup();
    let mut split = setup();
    whole.advance_layers(1.5);
    for dt in [0.25, 0.5, 0.25, 0.5] {
        split.advance_layers(dt);
    }
    assert_eq!(whole.machines[0].current_state, "idle");
    assert_eq!(whole.machines[1].current_state, "done");
    assert_eq!(whole.blend_preview.layers["intro"].time, 1.5);
    assert_eq!(whole.blend_preview.layers["second"].time, 1.);
    assert_eq!(whole.blend_preview.layers["second"].weight, 0.5);
    assert!(!whole.blend_preview.layers["second"].playing);
    assert!(whole.blend_preview.layers["second"].active);
    assert_eq!(whole.evaluated(), split.evaluated());
    assert_eq!(whole.evaluated()[0].transform.y, 320.);
}

#[test]
fn weighted_key_capture_records_the_source_not_composite() {
    let mut e = blend_editor();
    e.active_clip = 1;
    e.preview_animation = true;
    e.time = 0.5;
    e.project.clips[1].weight = 0.5;
    assert_eq!(e.evaluated()[0].transform.x, 100.);
    assert_eq!(e.keyframe_samples()[0].transform.x, 200.);
}

#[test]
fn manual_completion_dispatches_timeline_before_remaining_fade() {
    let mut e = blend_editor();
    e.project.state_machines.push(StateMachine {
        id: "timeline-test".into(),
        initial: "idle".into(),
        states: vec![
            State {
                id: "idle".into(),
                on_enter: vec![],
                overrides: vec![],
            },
            State {
                id: "done".into(),
                on_enter: vec![],
                overrides: vec![],
            },
        ],
        transitions: vec![Transition {
            from: "idle".into(),
            to: "done".into(),
            event: Event {
                name: "clip.end".into(),
                target: Some("@timeline".into()),
            },
            guard: None,
            actions: vec![Action::BlendClip {
                clip_id: "second".into(),
                weight: 0.,
                duration: 1.,
            }],
        }],
    });
    e.project.validate().unwrap();
    e.dispatch_events(None);
    e.blend_preview.play(&e.project, "second").unwrap();
    e.blend_preview.fade(&e.project, "second", 1., 0.).unwrap();
    e.time = 2.75;
    e.playing = true;
    e.looping = false;
    e.advance_playback(0.75);
    assert_eq!(e.time, 3.);
    assert!(!e.playing);
    assert_eq!(e.machines[0].current_state, "done");
    assert_eq!(e.blend_preview.layers["second"].weight, 0.5);
}

#[test]
fn sub_millisecond_native_loop_uses_document_period() {
    let mut e = blend_editor();
    e.project.clips[0].duration = 0.0006;
    e.project.clips[0].looping = true;
    e.project.clips[0].tracks[0].keyframes.truncate(1);
    e.project.validate().unwrap();
    e.blend_preview.play(&e.project, "intro").unwrap();
    e.advance_layers(0.0012);
    assert!(e.blend_preview.layers["intro"].time.abs() < 1e-12);
}

#[test]
fn native_blend_rejects_invalid_actions_without_silent_fallback() {
    let mut e = blend_editor();
    let mut pending = std::collections::VecDeque::new();
    for (id, weight, duration) in [
        ("missing", 0.5, 1.),
        ("second", f64::NAN, 0.),
        ("second", 1.1, 0.),
        ("second", 0.5, -1.),
        ("second", 0.5, f64::INFINITY),
    ] {
        e.preview_actions(
            vec![Action::BlendClip {
                clip_id: id.into(),
                weight,
                duration,
            }],
            &mut pending,
        );
        assert!(
            e.status.contains("Unbekannter Vorschau-Clip")
                || e.status.contains("Mischvorschau braucht")
        );
        assert!(e.blend_preview.layers.is_empty());
    }
    e.project.clips[0].blend_mode = BlendMode::Override;
    e.preview_actions(
        vec![Action::BlendClip {
            clip_id: "intro".into(),
            weight: 0.5,
            duration: 1.,
        }],
        &mut pending,
    );
    assert!(e.status.contains("braucht einen gewichteten Clip"));
}

#[test]
fn native_initial_actions_do_not_consume_idle_time_before_activation() {
    let mut e = Editor::default();
    e.project.state_machines[0].states[0]
        .on_enter
        .push(Action::PlayClip {
            clip_id: "intro".into(),
        });
    e.advance_layers(20.);
    e.dispatch_events(None);
    assert_eq!(e.blend_preview.layers["intro"].time, 0.);
}
