use forma_document::*;
use serde_json::json;

fn track(id: &str, property: Property, value: AnimValue) -> Track {
    Track {
        node_id: id.into(),
        property,
        keyframes: vec![Keyframe {
            time: 0.0,
            value,
            easing: Easing::Linear,
        }],
    }
}
fn number(property: Property, value: f64) -> Track {
    track("box", property, AnimValue::Number(value))
}
fn clip(id: &str, mode: BlendMode, weight: f64, tracks: Vec<Track>) -> Clip {
    Clip {
        id: id.into(),
        name: String::new(),
        duration: 1.0,
        looping: false,
        autoplay: true,
        blend_mode: mode,
        weight,
        tracks,
    }
}
fn project(clips: Vec<Clip>) -> Project {
    let mut node = Node::rect("box", 0.0, 0.0, 10.0, 10.0);
    node.transform.x = 10.0;
    Project {
        nodes: vec![node],
        clips,
        ..Default::default()
    }
}
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
}
fn patch(value: f64) -> PropertyOverride {
    PropertyOverride {
        node_id: "box".into(),
        property: Property::X,
        value: AnimValue::Number(value),
    }
}

#[test]
fn residual_midpoint_normalized_sparse_and_immutable() {
    let mut p = project(vec![
        clip(
            "walk",
            BlendMode::Weighted,
            0.25,
            vec![number(Property::X, 30.0)],
        ),
        clip(
            "jump",
            BlendMode::Weighted,
            0.25,
            vec![number(Property::X, 50.0), number(Property::Y, -80.0)],
        ),
    ]);
    near(p.evaluate(0.0).unwrap()[0].transform.x, 25.0);
    near(p.evaluate(0.0).unwrap()[0].transform.y, -20.0);
    for (weight, expected) in [(0.0, 10.0), (0.5, 40.0), (0.75, 40.0), (1.0, 40.0)] {
        p.clips[0].weight = weight;
        p.clips[1].weight = weight;
        near(p.evaluate(0.0).unwrap()[0].transform.x, expected);
    }
    let json = p.to_json().unwrap();
    let context = EvaluationContext {
        clip_weights: [("walk".into(), 0.25), ("jump".into(), 0.0)].into(),
        ..Default::default()
    };
    near(
        p.evaluate_with_context(0.0, &context).unwrap()[0]
            .transform
            .x,
        15.0,
    );
    near(
        p.evaluate_with_context(0.0, &context).unwrap()[0]
            .transform
            .y,
        0.0,
    );
    assert_eq!(p.to_json().unwrap(), json);
    near(p.evaluate_clip("walk", 0.0).unwrap()[0].transform.x, 30.0);
    let context = EvaluationContext {
        clips: Some(vec![]),
        ..context
    };
    near(
        p.evaluate_with_context(0.0, &context).unwrap()[0]
            .transform
            .x,
        10.0,
    );
    p.clips[1].tracks = vec![track("other", Property::X, AnimValue::Number(100.0))];
    p.nodes.push(Node::circle("other", 0.0, 0.0, 2.0));
    p.clips[0].weight = 0.25;
    near(p.evaluate(0.0).unwrap()[0].transform.x, 15.0);
}

#[test]
fn mixed_boundaries_playback_priority_and_overrides() {
    let mut p = project(vec![
        clip(
            "before",
            BlendMode::Override,
            0.0,
            vec![number(Property::X, 100.0)],
        ),
        clip(
            "a",
            BlendMode::Weighted,
            0.5,
            vec![number(Property::X, 20.0)],
        ),
        clip(
            "boundary",
            BlendMode::Override,
            0.0,
            vec![number(Property::X, 40.0)],
        ),
        clip(
            "b",
            BlendMode::Weighted,
            0.5,
            vec![number(Property::X, 80.0)],
        ),
    ]);
    near(p.evaluate(0.0).unwrap()[0].transform.x, 60.0);
    let context = EvaluationContext {
        clips: Some(
            ["b", "boundary", "a"]
                .into_iter()
                .map(|id| ClipPlayback {
                    clip_id: id.into(),
                    time: 0.0,
                })
                .collect(),
        ),
        ..Default::default()
    };
    near(
        p.evaluate_with_context(0.0, &context).unwrap()[0]
            .transform
            .x,
        30.0,
    );
    p.clips[0].autoplay = false;
    p.clips[2].tracks.clear();
    near(p.evaluate(0.0).unwrap()[0].transform.x, 47.5);
    p.state_machines.push(StateMachine {
        id: "movement".into(),
        initial: "walking".into(),
        states: vec![State {
            id: "walking".into(),
            on_enter: vec![],
            overrides: vec![patch(20.0)],
        }],
        transitions: vec![],
    });
    near(p.evaluate(0.0).unwrap()[0].transform.x, 50.0);
    let context = EvaluationContext {
        overrides: vec![patch(99.0)],
        ..Default::default()
    };
    near(
        p.evaluate_with_context(0.0, &context).unwrap()[0]
            .transform
            .x,
        99.0,
    );
}

#[test]
fn rgba_and_absent_stroke() {
    let red = Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 0.4,
    };
    let mut p = project(vec![clip(
        "color",
        BlendMode::Weighted,
        0.25,
        vec![
            track("box", Property::Fill, AnimValue::Color(red)),
            track("box", Property::Stroke, AnimValue::Color(red)),
        ],
    )]);
    p.nodes[0].style.fill = Color {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 0.8,
    };
    let node = &p.evaluate(0.0).unwrap()[0];
    assert_eq!(
        node.style.fill,
        Color {
            r: 0.25,
            g: 0.0,
            b: 0.75,
            a: 0.8 * 0.75 + 0.4 * 0.25
        }
    );
    assert_eq!(
        node.style.stroke,
        Some(Color {
            r: 0.25,
            g: 0.0,
            b: 0.0,
            a: 0.1
        })
    );
    p.clips[0].weight = 0.0;
    assert_eq!(p.evaluate(0.0).unwrap()[0].style.stroke, None);
    p.nodes[0].style.stroke = Some(Color::rgb(0, 0, 255));
    assert_eq!(
        p.evaluate(0.0).unwrap()[0].style.stroke,
        p.nodes[0].style.stroke
    );
}

#[test]
fn numeric_properties_and_sampling_match_override_at_full_weight() {
    for (id, property, value) in [
        ("box", Property::X, 30.0),
        ("box", Property::Y, 20.0),
        ("box", Property::Rotation, 180.0),
        ("box", Property::ScaleX, 2.0),
        ("box", Property::ScaleY, 3.0),
        ("box", Property::Opacity, 0.4),
        ("box", Property::Width, 20.0),
        ("box", Property::Height, 30.0),
        ("circle", Property::Radius, 5.0),
        ("text", Property::FontSize, 24.0),
    ] {
        for easing in [
            Easing::Linear,
            Easing::Step,
            Easing::CubicBezier {
                x1: 0.2,
                y1: 0.3,
                x2: 0.8,
                y2: 0.9,
            },
        ] {
            let mut t = track(id, property, AnimValue::Number(value / 2.0));
            t.keyframes[0].easing = easing;
            t.keyframes.push(Keyframe {
                time: 1.0,
                value: AnimValue::Number(value),
                easing: Easing::Linear,
            });
            let mut p = project(vec![clip("sample", BlendMode::Override, 1.0, vec![t])]);
            p.nodes.push(Node::circle("circle", 0.0, 0.0, 2.0));
            p.nodes.push(Node::text("text", 0.0, 0.0, "text"));
            for looping in [false, true] {
                p.clips[0].looping = looping;
                for time in [0.0, 0.25, 0.5, 1.0, 1.5, 2.0] {
                    p.clips[0].blend_mode = BlendMode::Override;
                    let expected = p.evaluate(time).unwrap();
                    p.clips[0].blend_mode = BlendMode::Weighted;
                    assert_eq!(p.evaluate(time).unwrap(), expected);
                }
            }
        }
    }
    let p = project(vec![
        clip(
            "a",
            BlendMode::Weighted,
            0.5,
            vec![number(Property::Rotation, 350.0)],
        ),
        clip(
            "b",
            BlendMode::Weighted,
            0.5,
            vec![number(Property::Rotation, 10.0)],
        ),
    ]);
    near(p.evaluate(0.0).unwrap()[0].transform.rotation, 180.0);
}

#[test]
fn local_bones_blend_before_fk_and_attachment() {
    let mut p = project(vec![
        clip(
            "walk",
            BlendMode::Weighted,
            0.5,
            vec![track("root", Property::Rotation, AnimValue::Number(0.0))],
        ),
        clip(
            "jump",
            BlendMode::Weighted,
            0.5,
            vec![
                track("root", Property::Rotation, AnimValue::Number(180.0)),
                track("root", Property::Y, AnimValue::Number(-20.0)),
            ],
        ),
    ]);
    p.bones = vec![
        Bone {
            id: "root".into(),
            parent_id: None,
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            length: 10.0,
        },
        Bone {
            id: "child".into(),
            parent_id: Some("root".into()),
            x: 2.0,
            y: 0.0,
            rotation: 5.0,
            length: 4.0,
        },
    ];
    p.nodes[0].bone_id = Some("child".into());
    p.nodes[0].transform.x = 0.0;
    let poses = p
        .evaluate_bones(0.0, &EvaluationContext::default())
        .unwrap();
    near(poses[0].rotation, 90.0);
    near(poses[1].x, 0.0);
    near(poses[1].y, 2.0);
    let node = &p.evaluate(0.0).unwrap()[0];
    near(node.transform.x, 0.0);
    near(node.transform.y, 2.0);
    near(node.transform.rotation, 95.0);
    assert_eq!(p.evaluate(0.0).unwrap(), p.evaluate(0.0).unwrap());
}

fn action_project(action: Action) -> Project {
    let mut p = project(vec![
        clip("weighted", BlendMode::Weighted, 1.0, vec![]),
        clip("legacy", BlendMode::Override, 1.0, vec![]),
    ]);
    p.state_machines.push(StateMachine {
        id: "machine".into(),
        initial: "idle".into(),
        states: vec![State {
            id: "idle".into(),
            on_enter: vec![action],
            overrides: vec![],
        }],
        transitions: vec![],
    });
    p
}

#[test]
fn validate_weights_context_and_actions_including_inactive_clips() {
    for weight in [-0.1, 1.1, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let p = project(vec![clip("weighted", BlendMode::Weighted, weight, vec![])]);
        assert!(p.validate().is_err());
        let mut p = p;
        p.clips[0].weight = 1.0;
        p.clips[0].autoplay = false;
        let context = EvaluationContext {
            clips: Some(vec![]),
            clip_weights: [("weighted".into(), weight)].into(),
            ..Default::default()
        };
        assert!(p.evaluate_with_context(0.0, &context).is_err());
        assert!(p.evaluate_bones(0.0, &context).is_err());
        assert!(action_project(Action::BlendClip {
            clip_id: "weighted".into(),
            weight,
            duration: 0.0
        })
        .validate()
        .is_err());
    }
    let context = EvaluationContext {
        clips: Some(vec![]),
        clip_weights: [("missing".into(), 0.5)].into(),
        ..Default::default()
    };
    assert!(project(vec![])
        .evaluate_with_context(0.0, &context)
        .is_err());
    for id in ["missing", "legacy"] {
        assert!(action_project(Action::BlendClip {
            clip_id: id.into(),
            weight: 0.5,
            duration: 0.0
        })
        .validate()
        .is_err());
    }
    for duration in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(action_project(Action::BlendClip {
            clip_id: "weighted".into(),
            weight: 0.5,
            duration
        })
        .validate()
        .is_err());
    }
    // No legacy time ceiling is imposed on the new fade duration.
    action_project(Action::BlendClip {
        clip_id: "weighted".into(),
        weight: 0.5,
        duration: 1e100,
    })
    .validate()
    .unwrap();
}

#[test]
fn additive_json_defaults_strict_preservation_and_host_actions() {
    let old = r#"{"clips":[{"id":"old","duration":1,"autoplay":true,"tracks":[]}]}"#;
    let p = Project::from_json(old).unwrap();
    assert_eq!(p.schema_version, 1);
    assert_eq!(p.clips[0].blend_mode, BlendMode::Override);
    assert_eq!(p.clips[0].weight, 1.0);
    let action = Action::BlendClip {
        clip_id: "weighted".into(),
        weight: 0.5,
        duration: 0.0,
    };
    let p = action_project(action.clone());
    let mut value = serde_json::to_value(&p).unwrap();
    let actions = &mut value["stateMachines"][0]["states"][0]["onEnter"];
    assert_eq!(
        actions[0],
        json!({"type":"blendClip", "clipId":"weighted", "weight":0.5, "duration":0.0})
    );
    actions[0].as_object_mut().unwrap().remove("duration");
    let loaded = Project::from_json(&value.to_string()).unwrap();
    assert_eq!(loaded, p);
    assert_eq!(
        MachineInstance::new(&loaded, "machine")
            .unwrap()
            .initial_actions,
        vec![action]
    );
    assert_eq!(Project::from_json(&p.to_json().unwrap()).unwrap(), p);
    value["clips"][0]["unknown"] = json!(1);
    assert!(Project::from_json(&value.to_string()).is_err());
    assert!(
        Project::from_json(r#"{"clips":[{"id":"a","duration":1,"weight":0.5,"weight":0.7}]}"#)
            .is_err()
    );
    assert!(Project::from_json(r#"{"clips":[{"id":"a","duration":9007199254740993}]}"#).is_err());
    assert!(
        Project::from_json(r#"{"clips":[{"id":"a","duration":1,"blendMode":"additive"}]}"#)
            .is_err()
    );
}

#[test]
fn parallel_walking_jumping_states_keep_scroll_and_sparse_jump_baseline() {
    let fade = |id: &str, weight| Action::BlendClip {
        clip_id: id.into(),
        weight,
        duration: 0.2,
    };
    let mut scroll = number(Property::X, 0.0);
    scroll.keyframes.push(Keyframe {
        time: 1.0,
        value: AnimValue::Number(-100.0),
        easing: Easing::Linear,
    });
    let mut p = project(vec![
        clip("scroll", BlendMode::Override, 1.0, vec![scroll]),
        clip(
            "walk",
            BlendMode::Weighted,
            1.0,
            vec![track("leg", Property::Rotation, AnimValue::Number(30.0))],
        ),
        clip(
            "jump",
            BlendMode::Weighted,
            0.0,
            vec![
                track("leg", Property::Rotation, AnimValue::Number(90.0)),
                track("leg", Property::Y, AnimValue::Number(-40.0)),
            ],
        ),
    ]);
    p.bones.push(Bone {
        id: "leg".into(),
        parent_id: None,
        x: 0.0,
        y: 10.0,
        rotation: 0.0,
        length: 10.0,
    });
    p.state_machines = vec![
        StateMachine {
            id: "scene".into(),
            initial: "scrolling".into(),
            states: vec![State {
                id: "scrolling".into(),
                on_enter: vec![],
                overrides: vec![],
            }],
            transitions: vec![],
        },
        StateMachine {
            id: "runner".into(),
            initial: "walking".into(),
            states: vec![
                State {
                    id: "walking".into(),
                    on_enter: vec![fade("walk", 1.0), fade("jump", 0.0)],
                    overrides: vec![],
                },
                State {
                    id: "jumping".into(),
                    on_enter: vec![fade("walk", 0.0), fade("jump", 1.0)],
                    overrides: vec![],
                },
            ],
            transitions: vec![
                Transition {
                    from: "walking".into(),
                    to: "jumping".into(),
                    event: Event {
                        name: "click".into(),
                        target: None,
                    },
                    guard: None,
                    actions: vec![],
                },
                Transition {
                    from: "jumping".into(),
                    to: "walking".into(),
                    event: Event {
                        name: "clipEnd:jump".into(),
                        target: None,
                    },
                    guard: None,
                    actions: vec![],
                },
            ],
        },
    ];
    let original = p.to_json().unwrap();
    let scene = MachineInstance::new(&p, "scene").unwrap();
    let mut runner = MachineInstance::new(&p, "runner").unwrap();
    assert_eq!(
        runner
            .send(
                &p,
                &Event {
                    name: "click".into(),
                    target: None
                }
            )
            .unwrap(),
        vec![fade("walk", 0.0), fade("jump", 1.0)]
    );
    assert_eq!(scene.current_state, "scrolling");
    // The host owns the ramp clock; these are synchronized crossfade snapshots.
    for (time, walk, jump, rotation, y) in [
        (0.0, 1.0, 0.0, 30.0, 10.0),
        (0.5, 0.5, 0.5, 60.0, -15.0),
        (1.0, 0.0, 1.0, 90.0, -40.0),
    ] {
        let context = EvaluationContext {
            states: [
                ("scene".into(), scene.current_state.clone()),
                ("runner".into(), runner.current_state.clone()),
            ]
            .into(),
            clip_weights: [("walk".into(), walk), ("jump".into(), jump)].into(),
            ..Default::default()
        };
        near(
            p.evaluate_with_context(time, &context).unwrap()[0]
                .transform
                .x,
            -100.0 * time,
        );
        let pose = p.evaluate_bones(time, &context).unwrap();
        near(pose[0].rotation, rotation);
        near(pose[0].y, y);
    }
    assert_eq!(
        runner
            .send(
                &p,
                &Event {
                    name: "clipEnd:jump".into(),
                    target: None
                }
            )
            .unwrap(),
        vec![fade("walk", 1.0), fade("jump", 0.0)]
    );
    near(
        p.evaluate_bones(1.0, &EvaluationContext::default())
            .unwrap()[0]
            .y,
        10.0,
    );
    assert_eq!(p.to_json().unwrap(), original);
}

#[test]
fn core_export_smoke_uses_blended_pose_and_preserves_metadata() {
    let p = project(vec![clip(
        "weighted",
        BlendMode::Weighted,
        0.5,
        vec![number(Property::X, 30.0)],
    )]);
    let svg = p.to_static_svg(0.0).unwrap();
    assert!(svg.contains("translate(20 0)"));
    let svg = p.to_svg("").unwrap();
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let metadata = xml
        .descendants()
        .find(|n| n.has_tag_name("metadata") && n.attribute("id") == Some("forma-project"))
        .unwrap();
    assert_eq!(Project::from_json(metadata.text().unwrap()).unwrap(), p);
}
