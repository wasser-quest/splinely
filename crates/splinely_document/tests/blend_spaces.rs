use serde_json::json;
use splinely_document::*;

fn fixture() -> Project {
    Project::from_json(include_str!(
        "../../../runtime/tests/fixtures/blend-spaces.json"
    ))
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}

#[test]
fn shared_fixture_additive_rest_pose_sparse_spaces_and_immutable_context() {
    let p = fixture();
    let before = p.to_json().unwrap();
    let n = &p.evaluate(0.).unwrap()[0];
    near(n.transform.x, 42.);
    near(n.transform.y, 32.);
    near(n.transform.rotation, 15.);
    near(n.transform.scale_x, 1.25);
    near(n.style.opacity, 0.6);
    let color = n.style.fill;
    near(color.r, 0.4);
    near(color.g, 0.3);
    near(color.b, 0.8);
    near(color.a, 0.75);
    let mut ctx = EvaluationContext::default();
    ctx.variables.insert("speed".into(), json!(0));
    near(
        p.evaluate_with_context(0., &ctx).unwrap()[0].transform.x,
        32.,
    );
    ctx.variables.insert("speed".into(), json!(2));
    near(
        p.evaluate_with_context(0., &ctx).unwrap()[0].transform.x,
        52.,
    );
    ctx.variables.insert("speed".into(), json!(1));
    ctx.clip_weights.insert("walk".into(), 0.5);
    near(
        p.evaluate_with_context(0., &ctx).unwrap()[0].transform.x,
        39.5,
    );
    ctx.overrides.push(PropertyOverride {
        node_id: "n".into(),
        property: Property::X,
        value: AnimValue::Number(99.),
    });
    near(
        p.evaluate_with_context(0., &ctx).unwrap()[0].transform.x,
        99.,
    );
    ctx.clips = Some(vec![]);
    ctx.overrides.clear();
    near(
        p.evaluate_with_context(0., &ctx).unwrap()[0].transform.x,
        20.,
    );
    assert_eq!(before, p.to_json().unwrap());
}

#[test]
fn idw_is_one_hot_symmetric_normalized_and_defined_outside_hull() {
    let mut p = fixture();
    let s = &p.blend_spaces[0];
    assert_eq!(s.weights([0., 0.]).unwrap(), vec![1., 0.]);
    assert_eq!(s.weights([2., 0.]).unwrap(), vec![0., 1.]);
    for point in [[1., 0.], [1., 100.]] {
        assert_eq!(s.weights(point).unwrap(), vec![0.5, 0.5]);
    }
    let w = s.weights([4., 0.]).unwrap();
    near(w[0], 0.2);
    near(w[1], 0.8);
    for point in [[1e-200, 0.], [-1e7, 1e7]] {
        let w = s.weights(point).unwrap();
        assert!(w.iter().all(|w| w.is_finite() && *w >= 0.));
        near(w.iter().sum(), 1.);
    }
    p.blend_spaces[0].samples.truncate(1);
    assert_eq!(p.blend_spaces[0].weights([123., 45.]).unwrap(), vec![1.]);
}

#[test]
fn additive_order_is_a_weighted_boundary_and_bounded_channels_clamp() {
    let mut p = fixture();
    p.blend_spaces.clear();
    p.clips[0].weight = 0.5;
    p.clips[1].weight = 0.5;
    p.clips.swap(1, 2);
    // state 20 -> weighted walk 25 -> additive 27 -> weighted run 38.5.
    near(p.evaluate(0.).unwrap()[0].transform.x, 38.5);
    p.nodes[0].style.opacity = 0.1;
    let n = p.evaluate(0.).unwrap().remove(0);
    near(n.style.opacity, 0.15); // 0.1 + .5*(.2-.1), not first-key delta.
    p.nodes[0].style.opacity = 1.;
    p.state_machines[0].states[0]
        .overrides
        .push(PropertyOverride {
            node_id: "n".into(),
            property: Property::Opacity,
            value: AnimValue::Number(0.),
        });
    near(p.evaluate(0.).unwrap()[0].style.opacity, 0.);
    // Absent stroke reference is transparent black.
    p.clips[1].tracks.push(Track {
        node_id: "n".into(),
        property: Property::Stroke,
        keyframes: vec![Keyframe {
            time: 0.,
            value: AnimValue::Color(Color {
                r: 1.,
                g: 0.,
                b: 0.,
                a: 1.,
            }),
            easing: Easing::Linear,
        }],
    });
    near(p.evaluate(0.).unwrap()[0].style.stroke.unwrap().a, 0.5);
}

#[test]
fn invalid_spaces_values_actions_and_strict_wire_are_rejected() {
    let base = serde_json::to_value(fixture()).unwrap();
    let cases = [
        ("/blendSpaces/0/parameters", json!(["speed", "speed"])),
        ("/blendSpaces/0/samples/1/position", json!([0.0000001, 0])),
        ("/blendSpaces/0/samples/1/clipId", json!("offset")),
        ("/variables/speed", json!("fast")),
        ("/variables/speed", json!(10000001)),
        ("/blendSpaces/0/samples", json!([])),
        ("/blendSpaces/0/samples/1/clipId", json!("walk")),
        ("/clips/2/tracks/0/property", json!("d")),
        (
            "/clips/2/tracks/0/keyframes/0/value",
            json!({"type":"color","value":{"r":0,"g":0,"b":0,"a":1}}),
        ),
    ];
    for (path, value) in cases {
        let mut p = base.clone();
        *p.pointer_mut(path).unwrap() = value;
        assert!(Project::from_json(&p.to_string()).is_err(), "{path}");
    }
    let mut p = base.clone();
    p["blendSpaces"][0]["unknown"] = json!(1);
    assert!(Project::from_json(&p.to_string()).is_err());
    let mut p = fixture();
    p.state_machines[0].states[0]
        .on_enter
        .push(Action::SetVariable {
            name: "speed".into(),
            value: json!("bad"),
        });
    assert!(p.validate().is_err());
    let p = fixture();
    let mut ctx = EvaluationContext::default();
    ctx.variables.insert("speed".into(), json!(null));
    assert!(p.evaluate_with_context(0., &ctx).is_err());
    // Optional collection is omitted for old projects.
    let old = Project::from_json("{}").unwrap();
    assert!(serde_json::to_value(old)
        .unwrap()
        .get("blendSpaces")
        .is_none());
}

#[test]
fn review_regressions_unknown_fields_duplicate_tracks_and_additive_range() {
    let base: serde_json::Value = serde_json::from_str(include_str!(
        "../../../runtime/tests/fixtures/blend-spaces.json"
    ))
    .unwrap();
    let load = |v: &serde_json::Value| Project::from_json(&v.to_string());
    let mut v = base.clone();
    v["blendSpaces"][0]["extra"] = json!(1);
    assert!(load(&v).is_err());
    let mut v = base.clone();
    v["blendSpaces"][0]["samples"][0]["extra"] = json!(1);
    assert!(load(&v).is_err());
    let clip = |v: &serde_json::Value, mode: &str| {
        v["clips"]
            .as_array()
            .unwrap()
            .iter()
            .position(|c| c["blendMode"] == mode)
            .unwrap()
    };
    for mode in ["additive", "weighted"] {
        let mut v = base.clone();
        let i = clip(&v, mode);
        let dup = v["clips"][i]["tracks"][0].clone();
        v["clips"][i]["tracks"].as_array_mut().unwrap().push(dup);
        assert!(load(&v)
            .unwrap_err()
            .to_string()
            .contains("duplicate property track"));
    }
    // Additive numeric keyframes share JS's +/-1e7 bound.
    let mut v = base.clone();
    let i = clip(&v, "additive");
    v["clips"][i]["tracks"][0]["keyframes"][0]["value"] = json!({"type":"number","value":1e7});
    load(&v).unwrap();
    v["clips"][i]["tracks"][0]["keyframes"][0]["value"] = json!({"type":"number","value":1e7 + 1.});
    assert!(load(&v).is_err());
}
