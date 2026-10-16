use serde_json::json;
use splinely_document::*;
use splinely_runtime::*;

fn fixture() -> Project {
    let mut p = Project::default();
    let mut n = Node::rect("node", 0.0, 0.0, 10.0, 10.0);
    n.bone_id = Some("root".into());
    p.nodes.push(n);
    p.bones.push(Bone {
        id: "root".into(),
        parent_id: None,
        x: 3.0,
        y: 0.0,
        rotation: 0.0,
        length: 10.0,
    });
    p.clips = vec![Clip {
        audio_cues: vec![],
        id: "walk".into(),
        name: String::new(),
        duration: 1.0,
        looping: false,
        autoplay: true,
        blend_mode: BlendMode::Weighted,
        weight: 1.0,
        tracks: vec![Track {
            node_id: "root".into(),
            property: Property::X,
            keyframes: vec![
                Keyframe {
                    time: 0.0,
                    value: AnimValue::Number(3.0),
                    easing: Easing::Linear,
                },
                Keyframe {
                    time: 1.0,
                    value: AnimValue::Number(13.0),
                    easing: Easing::Linear,
                },
            ],
        }],
    }];
    p.variables.insert("flag".into(), json!(false));
    p.custom_js = "throw new Error('must never run')".into();
    p
}
fn ev(name: &str) -> Event {
    Event {
        name: name.into(),
        target: None,
    }
}
fn act(r: &mut Runtime, action: Action) -> Response {
    r.execute(Request::Action { action }).unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

#[test]
fn clocks_bones_fades_pause_stop_and_immutability() {
    let p = fixture();
    let original = p.to_json().unwrap();
    let mut r = Runtime::new(p).unwrap();
    r.execute(Request::FadeClip {
        clip_id: "walk".into(),
        weight: 0.0,
        duration: 1.0,
    })
    .unwrap();
    let f = r.tick(0.5).unwrap().frame;
    near(f.bones[0].x, 5.5);
    near(f.nodes[0].transform.x, 5.5);
    r.execute(Request::PauseClip {
        clip_id: "walk".into(),
    })
    .unwrap();
    r.execute(Request::FadeClip {
        clip_id: "walk".into(),
        weight: 1.0,
        duration: 1.0,
    })
    .unwrap();
    r.execute(Request::Pause).unwrap();
    let f = r.tick(0.5).unwrap().frame;
    near(f.time, 0.5);
    near(f.clips[0].time, 0.5);
    near(f.clips[0].weight, 0.75);
    r.execute(Request::SeekClip {
        clip_id: "walk".into(),
        time: 1.0,
    })
    .unwrap();
    let f = r.tick(0.5).unwrap().frame;
    near(f.clips[0].weight, 1.0);
    near(f.bones[0].x, 13.0);
    r.execute(Request::FadeClip {
        clip_id: "walk".into(),
        weight: 0.0,
        duration: 1.0,
    })
    .unwrap();
    r.execute(Request::StopClip {
        clip_id: "walk".into(),
    })
    .unwrap();
    let f = r.tick(2.0).unwrap().frame;
    near(f.clips[0].weight, 1.0);
    assert!(!f.clips[0].active);
    near(f.bones[0].x, 3.0);
    assert_eq!(r.project().to_json().unwrap(), original);
}

#[test]
fn ordered_initial_entries_transitions_shared_guards_and_overrides() {
    let mut p = fixture();
    for id in ["initial", "transition", "entered"] {
        p.audio_assets.push(AudioAsset {
            id: id.into(),
            name: String::new(),
            mime_type: "audio/wav".into(),
            data_base64: "AA==".into(),
        });
    }
    p.state_machines = vec![
        StateMachine {
            id: "first".into(),
            initial: "a".into(),
            states: vec![
                State {
                    id: "a".into(),
                    on_enter: vec![Action::Emit { event: ev("go") }],
                    overrides: vec![],
                    on_exit: vec![],
                    announce: None,
                },
                State {
                    id: "b".into(),
                    on_enter: vec![Action::PlayAudio {
                        asset_id: "entered".into(),
                    }],
                    overrides: vec![],
                    on_exit: vec![],
                    announce: None,
                },
            ],
            transitions: vec![Transition {
                from: "a".into(),
                to: "b".into(),
                event: ev("go"),
                guard: None,
                actions: vec![
                    Action::SetVariable {
                        name: "flag".into(),
                        value: json!(true),
                    },
                    Action::PlayAudio {
                        asset_id: "transition".into(),
                    },
                ],
                blend: 0.0,
                after: None,
            }],
        },
        StateMachine {
            id: "second".into(),
            initial: "a".into(),
            states: vec![
                State {
                    id: "a".into(),
                    on_enter: vec![Action::PlayAudio {
                        asset_id: "initial".into(),
                    }],
                    overrides: vec![],
                    on_exit: vec![],
                    announce: None,
                },
                State {
                    id: "b".into(),
                    on_enter: vec![],
                    overrides: vec![PropertyOverride {
                        node_id: "root".into(),
                        property: Property::Y,
                        value: AnimValue::Number(7.0),
                    }],
                    on_exit: vec![],
                    announce: None,
                },
            ],
            transitions: vec![Transition {
                from: "a".into(),
                to: "b".into(),
                event: ev("go"),
                guard: Some(Guard {
                    variable: "flag".into(),
                    equals: json!(true),
                }),
                actions: vec![],
                blend: 0.0,
                after: None,
            }],
        },
    ];
    let mut r = Runtime::new(p).unwrap();
    let out = r.execute(Request::Evaluate).unwrap();
    assert_eq!(out.frame.states["second"], "b");
    near(out.frame.bones[0].y, 7.0);
    let words: Vec<_> = out
        .host_actions
        .iter()
        .filter_map(|a| match a {
            HostAction::PlayAudio { asset_id } => Some(asset_id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(words, ["initial", "transition", "entered"]);
    assert!(r
        .execute(Request::Evaluate)
        .unwrap()
        .host_actions
        .is_empty());
    act(
        &mut r,
        Action::SetProperty {
            node_id: "root".into(),
            property: Property::Y,
            value: AnimValue::Number(9.0),
        },
    );
    near(r.evaluate().unwrap().bones[0].y, 9.0);
    near(
        r.execute(Request::ClearOverrides).unwrap().frame.bones[0].y,
        7.0,
    );
}

#[test]
fn segmented_callbacks_start_clips_and_fades_mid_tick() {
    let mut p = fixture();
    p.clips[0].autoplay = false;
    p.state_machines.push(StateMachine {
        id: "m".into(),
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
            event: ev("start"),
            guard: None,
            actions: vec![
                Action::PlayClip {
                    clip_id: "walk".into(),
                },
                Action::BlendClip {
                    clip_id: "walk".into(),
                    weight: 0.0,
                    duration: 1.0,
                },
            ],
            blend: 0.0,
            after: None,
        }],
    });
    let mut r = Runtime::new(p).unwrap();
    r.execute(Request::Schedule {
        id: "half".into(),
        time: 0.5,
        data: json!({"custom":"payload"}),
        event: Some(ev("start")),
    })
    .unwrap();
    let mut split = r.clone();
    let out = r.tick(1.0).unwrap();
    split.tick(0.25).unwrap();
    split.tick(0.25).unwrap();
    split.tick(0.5).unwrap();
    assert_eq!(
        serde_json::to_value(out.frame.clone()).unwrap(),
        serde_json::to_value(split.evaluate().unwrap()).unwrap()
    );
    near(out.frame.clips[0].time, 0.5);
    near(out.frame.clips[0].weight, 0.5);
    assert!(out
        .host_actions
        .iter()
        .any(|a| matches!(a, HostAction::Callback { id, .. } if id == "half")));
}

#[test]
fn end_event_holds_last_pose_and_loop_order() {
    let mut r = Runtime::new(fixture()).unwrap();
    let out = r.tick(2.0).unwrap();
    near(out.frame.clips[0].time, 1.0);
    assert!(out.frame.clips[0].active);
    assert!(!out.frame.clips[0].playing);
    assert_eq!(
        out.host_actions
            .iter()
            .filter(|a| matches!(a, HostAction::Event { event } if event.name == "clip.end"))
            .count(),
        1
    );
    assert!(r.tick(1.0).unwrap().host_actions.is_empty());
    r.execute(Request::PlayClip {
        clip_id: "walk".into(),
    })
    .unwrap();
    near(r.evaluate().unwrap().clips[0].time, 0.0);
    let mut p = fixture();
    p.clips[0].looping = true;
    let mut r = Runtime::new(p).unwrap();
    let out = r.tick(2.5).unwrap();
    near(out.frame.clips[0].time, 0.5);
    assert_eq!(
        out.host_actions
            .iter()
            .filter(|a| matches!(a, HostAction::Event { event } if event.name == "clip.loop"))
            .count(),
        2
    );
}

#[test]
fn cues_zero_crossings_seek_and_host_only_audio() {
    let mut p = fixture();
    p.audio_assets.push(AudioAsset {
        id: "sound".into(),
        name: String::new(),
        mime_type: "audio/wav".into(),
        data_base64: "AA==".into(),
    });
    p.audio_cues.push(AudioCue {
        id: "cue".into(),
        asset_id: "sound".into(),
        time: 0.0,
        offset: 0.0,
        duration: None,
        volume: 1.0,
    });
    p.audio_cues.push(AudioCue {
        id: "later".into(),
        asset_id: "sound".into(),
        time: 0.5,
        offset: 0.0,
        duration: None,
        volume: 1.0,
    });
    let mut r = Runtime::new(p).unwrap();
    assert!(matches!(
        r.tick(0.0).unwrap().host_actions[0],
        HostAction::AudioCue { .. }
    ));
    assert!(matches!(
        &r.tick(0.5).unwrap().host_actions[0],
        HostAction::AudioCue { cue } if cue.id == "later"
    ));
    assert!(r
        .execute(Request::Seek { time: 0.0 })
        .unwrap()
        .host_actions
        .is_empty());
    assert!(matches!(
        &r.tick(0.5).unwrap().host_actions[0],
        HostAction::AudioCue { cue } if cue.id == "later"
    ));
    assert!(matches!(
        act(
            &mut r,
            Action::PlayAudio {
                asset_id: "sound".into()
            }
        )
        .host_actions[0],
        HostAction::PlayAudio { .. }
    ));
}

#[test]
fn invalid_requests_and_cycles_roll_back() {
    let mut p = fixture();
    p.state_machines.push(StateMachine {
        id: "m".into(),
        initial: "a".into(),
        states: vec![State {
            id: "a".into(),
            on_enter: vec![],
            overrides: vec![],
            on_exit: vec![],
            announce: None,
        }],
        transitions: vec![Transition {
            from: "a".into(),
            to: "a".into(),
            event: ev("cycle"),
            guard: None,
            actions: vec![Action::Emit { event: ev("cycle") }],
            blend: 0.0,
            after: None,
        }],
    });
    let mut r = Runtime::new(p).unwrap();
    let before = serde_json::to_value(r.evaluate().unwrap()).unwrap();
    assert_eq!(
        r.event(ev("cycle")).unwrap_err().0,
        "event drain limit exceeded"
    );
    assert!(r.tick(f64::NAN).is_err());
    assert!(r.tick(-1.0).is_err());
    assert!(r
        .execute(Request::SetWeight {
            clip_id: "walk".into(),
            weight: 2.0
        })
        .is_err());
    assert!(r
        .execute(Request::FadeClip {
            clip_id: "absent".into(),
            weight: 0.5,
            duration: 1.0
        })
        .is_err());
    assert!(r
        .execute(Request::Action {
            action: Action::SetProperty {
                node_id: "root".into(),
                property: Property::Opacity,
                value: AnimValue::Number(0.5)
            }
        })
        .is_err());
    assert!(r
        .execute(Request::Action {
            action: Action::SetVariable {
                name: "flag".into(),
                value: json!(4)
            }
        })
        .is_err());
    assert_eq!(serde_json::to_value(r.evaluate().unwrap()).unwrap(), before);
    let mut p = fixture();
    p.clips[0].looping = true;
    let mut r = Runtime::new(p).unwrap();
    assert!(r.tick(10_000.0).is_err());
    near(r.evaluate().unwrap().time, 0.0);
}

#[test]
fn override_clips_reject_fades_and_initial_cycles_are_bounded() {
    let mut p = fixture();
    p.clips[0].blend_mode = BlendMode::Override;
    let mut r = Runtime::new(p).unwrap();
    assert!(r
        .execute(Request::FadeClip {
            clip_id: "walk".into(),
            weight: 0.0,
            duration: 1.0
        })
        .is_err());
    let mut p = fixture();
    p.state_machines.push(StateMachine {
        id: "m".into(),
        initial: "a".into(),
        states: vec![State {
            id: "a".into(),
            on_enter: vec![Action::Emit { event: ev("cycle") }],
            overrides: vec![],
            on_exit: vec![],
            announce: None,
        }],
        transitions: vec![Transition {
            from: "a".into(),
            to: "a".into(),
            event: ev("cycle"),
            guard: None,
            actions: vec![],
            blend: 0.0,
            after: None,
        }],
    });
    assert_eq!(Runtime::new(p).unwrap_err().0, "event drain limit exceeded");
}

/// Two machines crossfade on the same shape while a pointer follower adds on top.
#[test]
fn two_machines_blend_and_followers_track_the_pointer_together() {
    let project = Project::from_json(
        &json!({
            "nodes": [
                {"id": "face", "kind": {"type": "circle", "cx": 100, "cy": 100, "radius": 50}},
                {"id": "pupil", "kind": {"type": "circle", "cx": 100, "cy": 100, "radius": 5}}
            ],
            "stateMachines": [
                {"id": "mood", "initial": "calm", "states": [
                    {"id": "calm", "overrides": [{"nodeId": "face", "property": "opacity", "value": {"type": "number", "value": 1}}]},
                    {"id": "happy", "overrides": [{"nodeId": "face", "property": "opacity", "value": {"type": "number", "value": 0.5}}]}
                ], "transitions": [{"from": "calm", "to": "happy", "event": {"name": "click", "target": "face"}, "blend": 1}]},
                {"id": "hover", "initial": "out", "states": [
                    {"id": "out"},
                    {"id": "in", "overrides": [{"nodeId": "face", "property": "scaleX", "value": {"type": "number", "value": 2}}]}
                ], "transitions": [{"from": "out", "to": "in", "event": {"name": "pointerenter", "target": "face"}, "blend": 0.5}]}
            ],
            "followers": [{"id": "look", "target": "pupil", "max": 10, "strength": 0.5, "smoothing": 0,
                "when": {"machine": "hover", "states": ["in"]}}]
        })
        .to_string(),
    )
    .unwrap();
    let mut r = Runtime::new(project).unwrap();
    let face = |f: &Frame| f.nodes.iter().find(|n| n.id == "face").unwrap().clone();
    let pupil = |f: &Frame| f.nodes.iter().find(|n| n.id == "pupil").unwrap().transform;
    let click = Event {
        name: "click".into(),
        target: Some("face".into()),
    };
    let enter = Event {
        name: "pointerenter".into(),
        target: Some("face".into()),
    };
    r.execute(Request::Event { event: click }).unwrap();
    r.execute(Request::Event { event: enter }).unwrap();
    r.execute(Request::Pointer { x: 108.0, y: 100.0 }).unwrap();
    let half = r.tick(0.25).unwrap().frame;
    // Both machines are mid-crossfade at the same time.
    let opacity = face(&half).style.opacity;
    assert!(opacity < 1.0 && opacity > 0.5, "{opacity}");
    let scale = face(&half).transform.scale_x;
    assert!(scale > 1.0 && scale < 2.0, "{scale}");
    assert_eq!(pupil(&half).x, 4.0);
    let done = r.tick(1.0).unwrap().frame;
    assert_eq!(face(&done).style.opacity, 0.5);
    assert_eq!(face(&done).transform.scale_x, 2.0);
    let left = r.execute(Request::PointerLeave).unwrap().frame;
    assert_eq!(pupil(&left).x, 0.0);
}

/// Timer transitions fire on time and exit actions run before the next state.
#[test]
fn timers_fire_on_time_and_exit_actions_run_first() {
    let project = Project::from_json(
        &json!({
            "nodes": [{"id": "eye", "kind": {"type": "circle", "cx": 10, "cy": 10, "radius": 5}}],
            "variables": {"blinks": 0},
            "stateMachines": [{"id": "blink", "initial": "open", "states": [
                {"id": "open", "onExit": [{"type": "emit", "event": {"name": "closing"}}]},
                {"id": "shut", "overrides": [{"nodeId": "eye", "property": "scaleY", "value": {"type": "number", "value": 0.1}}]}
            ], "transitions": [
                {"from": "open", "to": "shut", "after": 2},
                {"from": "shut", "to": "open", "after": 0.15}
            ]}]
        })
        .to_string(),
    )
    .unwrap();
    let mut r = Runtime::new(project).unwrap();
    assert_eq!(r.tick(1.9).unwrap().frame.states["blink"], "open");
    let out = r.tick(0.2).unwrap();
    assert_eq!(out.frame.states["blink"], "shut");
    assert!(out
        .host_actions
        .iter()
        .any(|a| matches!(a, HostAction::Event { event } if event.name == "closing")));
    let eye = |f: &Frame| f.nodes[0].transform.scale_y;
    assert_eq!(eye(&out.frame), 0.1);
    // 0.1 s already passed in "shut" within the same tick; 0.05 s more reopens.
    assert_eq!(r.tick(0.05).unwrap().frame.states["blink"], "open");
    // A timer cycle repeats.
    assert_eq!(r.tick(2.0).unwrap().frame.states["blink"], "shut");
    // Timer transitions ignore events with the reserved name.
    assert!(Project::from_json(
        &json!({"stateMachines": [{"id": "m", "initial": "a", "states": [{"id": "a"}],
        "transitions": [{"from": "a", "to": "a", "event": {"name": "after"}}]}]})
        .to_string()
    )
    .is_err());
}

#[test]
fn rigged_groups_are_set_through_their_rig_bones() {
    let mut p = Project::default();
    p.nodes.push(Node::rect("a", 0.0, 0.0, 10.0, 10.0));
    p.nodes.push(Node::rect("b", 20.0, 0.0, 10.0, 10.0));
    p.nodes.push(Node::rect("c", 50.0, 0.0, 10.0, 10.0));
    let group = p
        .group_nodes(&["a".to_string(), "b".to_string()], None)
        .unwrap();
    p.rig_group(&group).unwrap();
    // An outer group around the rigged one and c has no rig of its own.
    let plain = p
        .group_nodes(&["c".to_string(), "a".to_string()], None)
        .unwrap();
    let mut r = Runtime::new(p).unwrap();
    let set = |r: &mut Runtime, id: &str, property, value| {
        r.execute(Request::Action {
            action: Action::SetProperty {
                node_id: id.into(),
                property,
                value: AnimValue::Number(value),
            },
        })
    };
    // Centre (15, 5): a quarter turn moves a's origin to (20, -10), then 7 down.
    set(&mut r, &group, Property::Rotation, 90.0).unwrap();
    set(&mut r, &group, Property::Y, 7.0).unwrap();
    let a = r
        .evaluate()
        .unwrap()
        .nodes
        .into_iter()
        .find(|n| n.id == "a")
        .unwrap()
        .transform;
    near(a.x, 20.0);
    near(a.y, -3.0);
    near(a.rotation, 90.0);
    assert!(set(&mut r, &group, Property::Opacity, 0.5)
        .unwrap_err()
        .0
        .contains("groups animate x, y and rotation"));
    assert!(set(&mut r, &plain, Property::X, 1.0)
        .unwrap_err()
        .0
        .contains("has no rig"));
}
