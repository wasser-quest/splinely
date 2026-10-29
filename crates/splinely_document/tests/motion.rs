use splinely_document::*;

fn scene(d: &str) -> Project {
    let mut target = Node::circle("target", 0., 0., 3.);
    target.motion_path = Some(MotionPath {
        path: "route".into(),
        progress: 0.5,
        orient: false,
    });
    Project {
        nodes: vec![
            target,
            Node::new(
                "route",
                NodeKind::Path {
                    d: d.into(),
                    bounds: path_bounds(d).unwrap_or([0.; 4]),
                    evenodd: false,
                },
            ),
        ],
        ..Default::default()
    }
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}
#[test]
fn arc_length_endpoints_transforms_and_orientation() {
    let mut p = scene("M0 0 L100 0 L100 300");
    p.nodes[0].transform.x = 4.;
    p.nodes[0].motion_path.as_mut().unwrap().orient = true;
    let n = p.evaluate(0.).unwrap().remove(0);
    near(n.transform.x, 104.);
    near(n.transform.y, 100.);
    near(n.transform.rotation, 90.);
    p.nodes[1].transform = Transform {
        x: 10.,
        y: 20.,
        rotation: 90.,
        scale_x: 2.,
        scale_y: 2.,
    };
    let n = p.evaluate(0.).unwrap().remove(0);
    near(n.transform.x, -186.);
    near(n.transform.y, 220.);
    near(n.transform.rotation, 180.);
    p.nodes[0].motion_path.as_mut().unwrap().progress = 1.;
    let n = p.evaluate(0.).unwrap().remove(0);
    near(n.transform.x, -586.);
    near(n.transform.y, 220.);
}
#[test]
fn curves_relative_closed_disconnected_and_zero_segments() {
    for (d, x, y) in [
        ("m0 0 q50 100 100 0", 50., 50.),
        ("M0 0 C0 100 100 100 100 0", 50., 75.),
        ("M0 0 L0 0 L10 0 M100 0 L130 0", 110., 0.),
        ("M0 0 L10 0 Z", 10., 0.),
    ] {
        let n = scene(d).evaluate(0.).unwrap().remove(0);
        near(n.transform.x, x);
        near(n.transform.y, y);
    }
}
#[test]
fn progress_tracks_states_explicit_overrides_and_source_animation() {
    let mut p = scene("M0 0 L100 0");
    p.clips.push(Clip {
        audio_cues: vec![],
        id: "travel".into(),
        name: String::new(),
        duration: 1.,
        looping: false,
        autoplay: true,
        blend_mode: BlendMode::Override,
        weight: 1.,
        tracks: vec![Track {
            node_id: "target".into(),
            property: Property::MotionProgress,
            keyframes: vec![
                Keyframe {
                    time: 0.,
                    value: AnimValue::Number(0.),
                    easing: Easing::Linear,
                },
                Keyframe {
                    time: 1.,
                    value: AnimValue::Number(1.),
                    easing: Easing::Linear,
                },
            ],
        }],
    });
    near(p.evaluate(0.25).unwrap()[0].transform.x, 25.);
    let context = EvaluationContext {
        overrides: vec![
            PropertyOverride {
                node_id: "target".into(),
                property: Property::MotionProgress,
                value: AnimValue::Number(0.8),
            },
            PropertyOverride {
                node_id: "route".into(),
                property: Property::X,
                value: AnimValue::Number(10.),
            },
        ],
        ..Default::default()
    };
    near(
        p.evaluate_with_context(0.25, &context).unwrap()[0]
            .transform
            .x,
        90.,
    );
    p.remove_node_with_references("route").unwrap();
    assert!(p.nodes[0].motion_path.is_none());
    assert!(p.clips[0].tracks.is_empty());
    p.validate().unwrap();
}
#[test]
fn invalid_bindings_and_progress_are_rejected() {
    let base = scene("M0 0 L100 0");
    for value in [-0.1, 1.1, f64::NAN] {
        let mut p = base.clone();
        p.nodes[0].motion_path.as_mut().unwrap().progress = value;
        assert!(p.validate().is_err());
    }
    for id in ["missing", "target"] {
        let mut p = base.clone();
        p.nodes[0].motion_path.as_mut().unwrap().path = id.into();
        assert!(p.validate().is_err());
    }
    let mut p = base.clone();
    p.nodes[1].motion_path = p.nodes[0].motion_path.clone();
    assert!(p.validate().is_err());
    for d in ["M0 0", "M0 0 L0 0", "M0 0 H100"] {
        assert!(scene(d).validate().is_err());
    }
    let mut p = base;
    p.nodes[0].motion_path = None;
    let context = EvaluationContext {
        overrides: vec![PropertyOverride {
            node_id: "target".into(),
            property: Property::MotionProgress,
            value: AnimValue::Number(0.5),
        }],
        ..Default::default()
    };
    assert!(p.evaluate_with_context(0., &context).is_err());
}
#[test]
fn source_collapse_and_bone_binding_fail_without_panicking() {
    let mut p = scene("M0 0 L100 0");
    p.bones.push(Bone {
        id: "root".into(),
        parent_id: None,
        x: 0.,
        y: 0.,
        rotation: 0.,
        length: 1.,
    });
    for index in [0, 1] {
        p.nodes[index].bone_id = Some("root".into());
        assert!(p.validate().is_err());
        p.nodes[index].bone_id = None;
    }
    let context = EvaluationContext {
        overrides: vec![PropertyOverride {
            node_id: "route".into(),
            property: Property::ScaleX,
            value: AnimValue::Number(0.),
        }],
        ..Default::default()
    };
    assert!(p
        .evaluate_with_context(0., &context)
        .unwrap_err()
        .0
        .contains("nonzero length"));
    let context = EvaluationContext {
        overrides: vec![PropertyOverride {
            node_id: "route".into(),
            property: Property::D,
            value: AnimValue::Path("M0 0 H100".into()),
        }],
        ..Default::default()
    };
    assert!(p
        .evaluate_with_context(0., &context)
        .unwrap_err()
        .0
        .contains("M/L/Q/C/Z"));
}

#[test]
fn corner_roundoff_keeps_incoming_tangent() {
    for (d, rotation, expected) in [("M0 0 L1 0 L1 1", 30., 30.), ("M0 0 L1 1 L0 2", 0., 45.)] {
        let mut p = scene(d);
        p.nodes[0].motion_path.as_mut().unwrap().orient = true;
        p.nodes[1].transform.rotation = rotation;
        near(p.evaluate(0.).unwrap()[0].transform.rotation, expected);
    }
}
#[test]
fn optional_fields_round_trip_and_remain_strict() {
    let p = scene("M0 0 L100 0");
    assert_eq!(Project::from_json(&p.to_json().unwrap()).unwrap(), p);
    let plain = Project {
        nodes: vec![Node::circle("n", 0., 0., 1.)],
        ..Default::default()
    };
    assert!(!plain.to_json().unwrap().contains("motionPath"));
    let mut value = serde_json::to_value(&plain).unwrap();
    value["nodes"][0]["motionPath"] = serde_json::Value::Null;
    assert_eq!(Project::from_json(&value.to_string()).unwrap(), plain);
    let mut value = serde_json::to_value(p).unwrap();
    value["nodes"][0]["motionPath"]["typo"] = true.into();
    assert!(Project::from_json(&value.to_string()).is_err());
}

/// `n` L segments along x (64 chords each).
fn zigzag(n: usize) -> String {
    let mut d = String::from("M0 0");
    for i in 1..=n {
        d.push_str(&format!(" L{i} {}", i % 2));
    }
    d
}
fn bind(p: &mut Project, count: usize, progress: f64) {
    for i in 0..count {
        let mut n = Node::circle(&format!("extra{i}"), 0., 0., 1.);
        n.motion_path = Some(MotionPath {
            path: "route".into(),
            progress,
            orient: true,
        });
        p.nodes.push(n);
    }
}
#[test]
fn chord_limits_match_the_js_runtime() {
    assert_eq!((MOTION_MAX_CHORDS, MOTION_TOTAL_CHORDS), (100_000, 100_000));
    // 1562 segments = 99968 chords: fine; 1563 = 100032: one path too complex.
    scene(&zigzag(1562)).validate().unwrap();
    let err = scene(&zigzag(1563)).validate().unwrap_err().0;
    assert!(err.contains("motion path too complex"), "{err}");
    // The total counts distinct sources once, however many bindings use them.
    let mut p = scene(&zigzag(1562));
    bind(&mut p, 2000, 0.3);
    p.validate().unwrap();
    // ...but adds `d` keyframes and state overrides targeting a source
    // (700 segments = 44800 chords each).
    let mut p = scene(&zigzag(700));
    let keyframe = |time| Keyframe {
        time,
        value: AnimValue::Path(zigzag(700)),
        easing: Easing::Linear,
    };
    p.clips.push(Clip {
        audio_cues: vec![],
        id: "morph".into(),
        name: String::new(),
        duration: 1.,
        looping: false,
        autoplay: false,
        blend_mode: BlendMode::Override,
        weight: 1.,
        tracks: vec![Track {
            node_id: "route".into(),
            property: Property::D,
            keyframes: vec![keyframe(0.)],
        }],
    });
    p.validate().unwrap();
    let mut q = p.clone();
    q.clips[0].tracks[0].keyframes.push(keyframe(1.));
    let err = q.validate().unwrap_err().0;
    assert!(err.contains("motion paths too complex"), "{err}");
    let mut value = serde_json::to_value(&p).unwrap();
    value["stateMachines"] = serde_json::json!([{"id": "m", "initial": "a", "transitions": [], "states": [{"id": "a", "onEnter": [],
        "overrides": [{"nodeId": "route", "property": "d", "value": {"type": "path", "value": zigzag(700)}}]}]}]);
    let err = Project::from_json(&value.to_string()).unwrap_err().0;
    assert!(err.contains("motion paths too complex"), "{err}");
}
#[test]
fn chord_budget_counts_segments_not_visible_length() {
    // Zero-length segments still cost 64 samples each: 1563 of them are too complex.
    let d = format!("M0 0 L1 0{}", " L1 0".repeat(1562));
    let err = scene(&d).validate().unwrap_err().0;
    assert!(err.contains("motion path too complex"), "{err}");
    // A tiny base scale made animated `d` values flatten to nothing and count 0;
    // they now count per segment, independent of the transform.
    let mut p = scene("M0 0 L1e7 0");
    p.nodes[1].transform.scale_x = 1e-17;
    p.nodes[1].transform.scale_y = 1e-17;
    p.clips.push(Clip {
        audio_cues: vec![],
        id: "grow".into(),
        name: String::new(),
        duration: 1.,
        looping: false,
        autoplay: true,
        blend_mode: BlendMode::Override,
        weight: 1.,
        tracks: vec![Track {
            node_id: "route".into(),
            property: Property::D,
            keyframes: vec![Keyframe {
                time: 0.,
                value: AnimValue::Path(zigzag(800)),
                easing: Easing::Linear,
            }],
        }],
    });
    p.validate().unwrap();
    let mut q = p.clone();
    q.clips[0].tracks[0].keyframes.push(Keyframe {
        time: 1.,
        value: AnimValue::Path(zigzag(800)),
        easing: Easing::Linear,
    });
    let err = q.validate().unwrap_err().0;
    assert!(err.contains("motion paths too complex"), "{err}");
}
#[test]
fn many_bindings_sample_each_source_once() {
    // Before: every binding re-flattened the path (bindings x chords, minutes).
    let mut p = scene(&zigzag(1500));
    bind(&mut p, 20_000, 0.37);
    let start = std::time::Instant::now();
    let nodes = p.evaluate(0.).unwrap();
    assert!(start.elapsed().as_secs_f64() < 5.0, "{:?}", start.elapsed());
    let (first, last) = (&nodes[2], &nodes[nodes.len() - 1]);
    assert_eq!(first.transform, last.transform);
    near(first.transform.x, 0.37 * 1500.);
}
#[test]
fn long_paths_binary_search_the_walked_chord() {
    // Over 1024 chords the lookup bisects prefix sums (as in JS); it must pick the
    // chord the linear walk picks, including exact corners.
    let d = zigzag(40); // 2560 chords, corners every 64
    for k in [0usize, 1, 37, 64, 640, 1279, 1280, 2559, 2560] {
        let progress = k as f64 / 2560.;
        let mut p = scene(&d);
        p.nodes[0].motion_path.as_mut().unwrap().progress = progress;
        p.nodes[0].motion_path.as_mut().unwrap().orient = true;
        let n = p.evaluate(0.).unwrap().remove(0);
        let total = 40. * 2f64.sqrt();
        let along = progress * total;
        let seg = ((along / 2f64.sqrt()).ceil() as usize).clamp(1, 40);
        let up = seg % 2 == 1;
        near(n.transform.rotation, if up { 45. } else { -45. });
        near(n.transform.x, along / 2f64.sqrt());
    }
}
