use splinely_document::*;
use splinely_runtime::*;
use serde_json::json;

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
    p.state_machines = vec![
        StateMachine {
            id: "first".into(),
            initial: "a".into(),
            states: vec![
                State {
                    id: "a".into(),
                    on_enter: vec![Action::Emit { event: ev("go") }],
                    overrides: vec![],
                },
                State {
                    id: "b".into(),
                    on_enter: vec![Action::Speak {
                        text: "entered".into(),
                        lang: "en".into(),
                    }],
                    overrides: vec![],
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
                    Action::Speak {
                        text: "transition".into(),
                        lang: "en".into(),
                    },
                ],
            }],
        },
        StateMachine {
            id: "second".into(),
            initial: "a".into(),
            states: vec![
                State {
                    id: "a".into(),
                    on_enter: vec![Action::Speak {
                        text: "initial".into(),
                        lang: "en".into(),
                    }],
                    overrides: vec![],
                },
                State {
                    id: "b".into(),
                    on_enter: vec![],
                    overrides: vec![PropertyOverride {
                        node_id: "root".into(),
                        property: Property::Y,
                        value: AnimValue::Number(7.0),
                    }],
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
            HostAction::Speak { text, .. } => Some(text.as_str()),
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
    p.speech_cues.push(SpeechCue {
        id: "say".into(),
        time: 0.5,
        text: "hi".into(),
        lang: "en".into(),
        rate: 1.0,
        pitch: 1.0,
        volume: 1.0,
    });
    let mut r = Runtime::new(p).unwrap();
    assert!(matches!(
        r.tick(0.0).unwrap().host_actions[0],
        HostAction::AudioCue { .. }
    ));
    assert!(matches!(
        r.tick(0.5).unwrap().host_actions[0],
        HostAction::SpeechCue { .. }
    ));
    assert!(r
        .execute(Request::Seek { time: 0.0 })
        .unwrap()
        .host_actions
        .is_empty());
    assert!(matches!(
        r.tick(0.5).unwrap().host_actions[0],
        HostAction::SpeechCue { .. }
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
        }],
        transitions: vec![Transition {
            from: "a".into(),
            to: "a".into(),
            event: ev("cycle"),
            guard: None,
            actions: vec![Action::Emit { event: ev("cycle") }],
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
        }],
        transitions: vec![Transition {
            from: "a".into(),
            to: "a".into(),
            event: ev("cycle"),
            guard: None,
            actions: vec![],
        }],
    });
    assert_eq!(Runtime::new(p).unwrap_err().0, "event drain limit exceeded");
}
