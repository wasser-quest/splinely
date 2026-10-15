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
