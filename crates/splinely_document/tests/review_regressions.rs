use splinely_document::{path_bounds, Node, NodeKind, Project};

#[test]
fn core_rejects_ambiguous_and_lossy_json_before_model_conversion() {
    for json in [
        r#"{"bones":[{"id":"b","x":0,"x":100,"y":0,"rotation":0,"length":10}]}"#,
        r#"{"bones":[{"id":"b","x":9007199254740993,"y":0,"rotation":0,"length":1}]}"#,
        r#"{"variables":{"n":18446744073709551617}}"#,
    ] {
        assert!(Project::from_json(json).is_err(), "{json}");
    }
    assert!(Project::from_json(r#"{"width":9007199254740992}"#).is_ok());
}

#[test]
fn native_geometry_and_canvas_overflow_are_rejected() {
    let mut p = Project {
        width: 1e100,
        ..Project::default()
    };
    assert!(p.validate().is_err());
    p.width = 640.;
    p.nodes.push(Node::rect("r", 0., 0., 10., 10.));
    p.nodes[0].transform.x = 1e100;
    assert!(p.validate().is_err());
}

#[test]
fn tight_bounds_cannot_be_forged_and_horizontal_paths_are_valid() {
    let d = "M0 0 Q100 100 200 0";
    assert_eq!(path_bounds(d).unwrap(), [0., 0., 200., 50.]);
    let mut n = Node::rect("p", 0., 0., 10., 10.);
    n.kind = NodeKind::Path {
        d: d.into(),
        bounds: [0., 0., 200., 100.],
        evenodd: false,
    };
    let mut p = Project::default();
    p.nodes.push(n);
    assert!(p.validate().is_err());
    p.nodes[0].kind = NodeKind::Path {
        d: "M0 0 L10 0".into(),
        bounds: [0., 0., 10., 0.],
        evenodd: false,
    };
    assert!(p.validate().is_ok());
}

#[test]
fn tiny_nonuniform_stroke_transforms_are_not_silently_flattened() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="24"><path d="M0 0 L1000000 0" fill="none" stroke="black" stroke-width="100000" transform="scale(.00001 .00002)"/></svg>"#;
    assert!(Project::from_svg(svg).is_err());
}
