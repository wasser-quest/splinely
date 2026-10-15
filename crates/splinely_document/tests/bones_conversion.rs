use splinely_document::*;

fn bone(id: &str, parent: Option<&str>) -> Bone {
    Bone {
        id: id.into(),
        parent_id: parent.map(str::to_owned),
        x: 0.0,
        y: 0.0,
        rotation: 0.0,
        length: 10.0,
    }
}
fn rig() -> Project {
    let mut n = Node::rect("shape", 1.0, 2.0, 4.0, 6.0);
    n.bone_id = Some("child".into());
    n.transform.x = 3.0;
    n.transform.rotation = 5.0;
    Project {
        bones: vec![bone("root", None), bone("child", Some("root"))],
        nodes: vec![n],
        ..Default::default()
    }
}
fn patch(id: &str, property: Property, number: f64) -> PropertyOverride {
    PropertyOverride {
        node_id: id.into(),
        property,
        value: AnimValue::Number(number),
    }
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}

#[test]
fn old_documents_and_schema_roundtrip() {
    let p = Project::from_json(
        r#"{"nodes":[{"id":"old","kind":{"type":"circle","cx":0,"cy":0,"radius":2}}]}"#,
    )
    .unwrap();
    assert!(p.bones.is_empty());
    assert!(p.nodes[0].visible);
    assert!(!p.nodes[0].locked);
    assert!(p.nodes[0].original_kind.is_none());
    let mut p = rig();
    p.nodes[0].locked = true;
    p.nodes[0].convert_to_path().unwrap();
    assert_eq!(Project::from_json(&p.to_json().unwrap()).unwrap(), p);
}

#[test]
fn rejects_invalid_rigs_and_properties() {
    let p = rig();
    for variant in 0..8 {
        let mut p = p.clone();
        match variant {
            0 => p.bones[0].parent_id = Some("child".into()),
            1 => p.bones[1].parent_id = Some("missing".into()),
            2 => p.bones[1].id = "root".into(),
            3 => p.bones[0].id = "shape".into(),
            4 => p.bones[0].length = 0.0,
            5 => p.bones[0].rotation = f64::NAN,
            6 => p.nodes[0].bone_id = Some("missing".into()),
            _ => {
                p.bones[0].x = f64::MAX;
                p.bones[0].length = f64::MAX;
            }
        }
        assert!(p.validate().is_err(), "variant {variant}");
    }
    let mut deep = Project::default();
    for i in 0..128 {
        deep.bones.push(bone(
            &format!("b{i}"),
            (i > 0).then(|| format!("b{}", i - 1)).as_deref(),
        ));
    }
    deep.validate().unwrap();
    deep.bones.push(bone("b128", Some("b127")));
    assert!(deep.validate().is_err());
    for property in [
        Property::Width,
        Property::ScaleX,
        Property::Opacity,
        Property::Fill,
    ] {
        assert!(p
            .evaluate_with_context(
                0.0,
                &EvaluationContext {
                    overrides: vec![patch("root", property, 1.0)],
                    ..Default::default()
                }
            )
            .is_err());
    }
}

#[test]
fn clip_state_explicit_precedence_and_single_attachment() {
    let mut p = rig();
    p.bones[0].x = 20.0;
    p.bones[1].x = 2.0;
    p.bones[1].y = 1.0;
    p.state_machines.push(StateMachine {
        id: "machine".into(),
        initial: "posed".into(),
        states: vec![State {
            id: "posed".into(),
            on_enter: vec![Action::SetProperty {
                node_id: "root".into(),
                property: Property::X,
                value: AnimValue::Number(30.0),
            }],
            overrides: vec![patch("root", Property::Rotation, 45.0)],
        }],
        transitions: vec![],
    });
    p.clips.push(Clip {
        id: "pose".into(),
        name: "".into(),
        duration: 1.0,
        looping: false,
        autoplay: true,
        blend_mode: BlendMode::Override,
        weight: 1.0,
        tracks: vec![Track {
            node_id: "root".into(),
            property: Property::Rotation,
            keyframes: vec![
                Keyframe {
                    time: 0.0,
                    value: AnimValue::Number(0.0),
                    easing: Easing::Linear,
                },
                Keyframe {
                    time: 1.0,
                    value: AnimValue::Number(180.0),
                    easing: Easing::Linear,
                },
            ],
        }],
    });
    let context = EvaluationContext::default();
    let poses = p.evaluate_bones(0.5, &context).unwrap();
    near(poses[1].x, 19.0);
    near(poses[1].y, 12.0);
    let nodes = p.evaluate(0.5).unwrap();
    near(nodes[0].transform.x, 19.0);
    near(nodes[0].transform.y, 15.0);
    near(nodes[0].transform.rotation, 95.0);
    assert_eq!(nodes[0].kind, p.nodes[0].kind);
    assert_eq!(p.evaluate(0.5).unwrap(), nodes);
    let context = EvaluationContext {
        overrides: vec![patch("root", Property::Rotation, 180.0)],
        ..Default::default()
    };
    near(p.evaluate_bones(0.5, &context).unwrap()[0].rotation, 180.0);
    let context = EvaluationContext {
        clips: Some(vec![]),
        ..Default::default()
    };
    near(p.evaluate_bones(0.5, &context).unwrap()[0].rotation, 45.0);
    let context = EvaluationContext {
        overrides: vec![
            patch("root", Property::X, f64::MAX),
            patch("child", Property::X, f64::MAX),
        ],
        ..Default::default()
    };
    assert!(p.evaluate_with_context(0.0, &context).is_err());
    assert!(p.evaluate_bones(0.0, &context).is_err());
}

#[test]
fn svg_poses_metadata_visibility_and_z_order() {
    let mut p = rig();
    p.bones[0].x = 20.0;
    p.nodes.push(Node::circle("top", 0.0, 0.0, 2.0));
    let svg = p.to_svg("").unwrap();
    assert!(svg.contains("translate(33 0) rotate(5)"));
    assert!(svg.find("<rect ").unwrap() < svg.find("<circle ").unwrap());
    let doc = roxmltree::Document::parse(&svg).unwrap();
    let metadata = doc
        .descendants()
        .find(|n| n.has_tag_name("metadata") && n.attribute("id") == Some("splinely-project"))
        .unwrap()
        .text()
        .unwrap();
    assert_eq!(Project::from_json(metadata).unwrap(), p);
    assert!(!doc.descendants().any(|n| n.attribute("id") == Some("root")));
    p.nodes[0].visible = false;
    let svg = p.to_static_svg(0.0).unwrap();
    assert!(!svg.contains("<rect "));
    assert!(svg.contains("<circle "));
}

#[test]
fn conversion_is_exact_reversible_and_atomic() {
    for mut n in [
        Node::rect("rect", 1.0, 2.0, 10.0, 6.0),
        Node::new(
            "round",
            NodeKind::Rect {
                x: 1.0,
                y: 2.0,
                width: 10.0,
                height: 6.0,
                rx: 20.0,
            },
        ),
        Node::circle("circle", 5.0, 7.0, 3.0),
    ] {
        n.locked = true;
        n.bone_id = Some("root".into());
        let original = n.clone();
        n.convert_to_path().unwrap();
        let mut p = Project {
            nodes: vec![n.clone()],
            bones: vec![bone("root", None)],
            ..Default::default()
        };
        assert!(p.to_static_svg(0.0).unwrap().contains("<path "));
        p = Project::from_json(&p.to_json().unwrap()).unwrap();
        p.nodes[0].convert_back().unwrap();
        assert_eq!(p.nodes[0], original);
        for variant in 0..3 {
            let mut changed = n.clone();
            if let NodeKind::Path { d, bounds, evenodd } = &mut changed.kind {
                match variant {
                    0 => d.push_str(" M0 0"),
                    1 => bounds[0] += 1.0,
                    _ => *evenodd = true,
                }
            }
            let before = changed.clone();
            assert!(changed.convert_back().is_err());
            assert_eq!(changed, before);
        }
        n.style.opacity = 0.5;
        n.transform.x = 12.0;
        n.convert_back().unwrap();
        assert_eq!(n.style.opacity, 0.5);
        assert_eq!(n.transform.x, 12.0);
    }
    for mut n in [
        Node::text("text", 0.0, 0.0, "not outlined"),
        Node::circle("zero", 0.0, 0.0, 0.0),
        Node::rect("overflow", f64::MAX, 0.0, f64::MAX, 1.0),
    ] {
        let before = n.clone();
        assert!(n.convert_to_path().is_err());
        assert!(n.convert_back().is_err());
        assert_eq!(n, before);
    }
    let mut p = Project::default();
    let mut n = Node::circle("bad", 0.0, 0.0, 1.0);
    n.original_kind = Some(Box::new(NodeKind::Text {
        x: 0.0,
        y: 0.0,
        text: "bad".into(),
        font_size: 16.0,
    }));
    p.nodes.push(n);
    assert!(p.validate().is_err());
}
