use forma_document::*;
fn svg(body: &str) -> String {
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="24">{body}</svg>"#)
}
#[test]
fn vectors_roundtrip_and_animate() {
    let mut project=Project::from_svg(&svg(r##"<g fill="#ff0000" transform="translate(2 3)"><rect id="shape" width="10" height="8" rx="2"/><path d="M0 0L3 0L3 3Z"/></g>"##)).unwrap();
    assert_eq!(project.nodes.len(), 2);
    assert!(matches!(project.nodes[0].kind, NodeKind::Path { .. }));
    assert_eq!(project.nodes[0].style.fill, Color::rgb(255, 0, 0));
    project.clips.push(Clip {
        id: "move".into(),
        name: "Move".into(),
        duration: 1.,
        looping: false,
        autoplay: true,
        blend_mode: BlendMode::Override,
        weight: 1.0,
        tracks: vec![Track {
            node_id: "shape".into(),
            property: Property::X,
            keyframes: vec![
                Keyframe {
                    time: 0.,
                    value: AnimValue::Number(0.),
                    easing: Easing::Linear,
                },
                Keyframe {
                    time: 1.,
                    value: AnimValue::Number(10.),
                    easing: Easing::Linear,
                },
            ],
        }],
    });
    assert_eq!(project.evaluate(0.5).unwrap()[0].transform.x, 5.);
    let json = project.to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), project);
    let out = project.to_static_svg(0.).unwrap();
    let second = Project::from_svg(&out).unwrap();
    assert_eq!(second.nodes.len(), 2);
}
#[test]
fn reject_unsupported_without_partial_success() {
    for body in [
        r#"<rect width="10" height="10"/><image href="https://example.test/x"/>"#,
        r#"<script>alert(1)</script>"#,
        r#"<text>unsupported rich text</text>"#,
        r#"<g opacity=".5"><rect width="10" height="10"/><rect width="10" height="10"/></g>"#,
        r#"<path fill="url(#gradient)" d="M0 0L10 0L10 10Z"/>"#,
        r#"<path onclick="evil()" d="M0 0L10 0Z"/>"#,
        r#"<foreignObject/>"#,
        r#"<rect width="10" height="10" style="clip-path:inset(2px)"/>"#,
        r#"<rect width="10" height="10" style="vector-effect:non-scaling-stroke" transform="scale(2)" stroke="black"/>"#,
    ] {
        assert!(Project::from_svg(&svg(body)).is_err(), "{body}");
    }
    assert!(Project::from_svg("<svg xmlns='urn:wrong'/>").is_err());
    assert!(Project::from_svg(
        "<!DOCTYPE svg [<!ENTITY a 'x'>]><svg xmlns='http://www.w3.org/2000/svg'/>"
    )
    .is_err());
}
#[test]
fn path_validation_is_strict_and_ids_are_mapped() {
    assert!(Project::from_svg(&svg(r#"<path d="M NaN 0Z"/>"#)).is_err());
    let p = Project::from_svg(&svg(r#"<rect id="bad:id" width="4" height="4"/>"#)).unwrap();
    assert_eq!(p.nodes[0].id, "svg-path-1");
    let mut p = p;
    p.nodes[0].kind = NodeKind::Path {
        d: "M inf 0".into(),
        bounds: [0., 0., 1., 1.],
        evenodd: false,
    };
    assert!(p.validate().is_err());
}

#[test]
fn groups_and_gradients_survive_import() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
      <defs><linearGradient id="sky" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff" stop-opacity="0.5"/>
      </linearGradient></defs>
      <g id="figure">
        <g id="head"><circle id="face" cx="50" cy="50" r="20" fill="url(#sky)"/></g>
        <rect id="body" x="40" y="70" width="20" height="20" fill="#00ff00"/>
      </g>
      <rect id="loose" x="150" y="10" width="10" height="10"/>
    </svg>"##;
    let project = Project::from_svg(svg).unwrap();
    assert_eq!(
        project.group_path("face"),
        vec!["figure".to_string(), "head".to_string()]
    );
    assert_eq!(project.group_path("body"), vec!["figure".to_string()]);
    assert!(project.group_path("loose").is_empty());
    let face = project.nodes.iter().find(|n| n.id == "face").unwrap();
    let gradient = face.gradient.as_ref().expect("gradient imported");
    assert!(!gradient.radial);
    assert!(
        (gradient.angle - 90.).abs() < 1e-6,
        "top to bottom: {}",
        gradient.angle
    );
    assert_eq!(gradient.stops.len(), 2);
    assert!((gradient.stops[1].color.a - 0.5).abs() < 1e-6);
    // Export keeps the hierarchy.
    let out = project.to_static_svg(0.).unwrap();
    assert!(out.contains("<g id=\"figure\""));
}
