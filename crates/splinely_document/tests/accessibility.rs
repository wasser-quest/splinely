//! Web-player/SVG accessibility data: strict validation, serialisation round
//! trips (JSON and binary `.sply`), SVG export semantics and byte-identical
//! export for projects without accessibility data.
use serde_json::{json, Value};
use splinely_document::Project;

fn base() -> Value {
    json!({
        "schemaVersion": 1,
        "name": "Lamp & <scene>",
        "width": 200.0, "height": 100.0,
        "nodes": [
            {"id": "switch", "name": "Switch", "kind": {"type": "rect", "x": 0.0, "y": 0.0, "width": 40.0, "height": 20.0}},
            {"id": "bulb", "kind": {"type": "circle", "cx": 100.0, "cy": 50.0, "radius": 10.0}},
            {"id": "deco", "kind": {"type": "circle", "cx": 5.0, "cy": 5.0, "radius": 2.0}}
        ],
        "stateMachines": [{
            "id": "lamp", "initial": "off",
            "states": [
                {"id": "off", "onEnter": [], "overrides": []},
                {"id": "on", "onEnter": [], "overrides": []}
            ],
            "transitions": [
                {"from": "off", "to": "on", "event": {"name": "click", "target": "switch"}, "guard": null, "actions": [], "blend": 0.0},
                {"from": "on", "to": "off", "event": {"name": "click", "target": "switch"}, "guard": null, "actions": [], "blend": 0.0}
            ]
        }]
    })
}
fn load(value: Value) -> Result<Project, String> {
    Project::from_value_strict(value).map_err(|e| e.to_string())
}
fn with(mutate: impl FnOnce(&mut Value)) -> Value {
    let mut v = base();
    mutate(&mut v);
    v
}
fn accessible() -> Value {
    with(|v| {
        v["description"] = json!("A switch turns a lamp on and off.");
        v["nodes"][0]["a11y"] =
            json!({"label": "Lamp switch", "description": "Toggles the lamp", "role": "button"});
        v["nodes"][2]["a11y"] = json!({"hidden": true});
        v["stateMachines"][0]["states"][0]["announce"] = json!("Lamp off");
        v["stateMachines"][0]["states"][1]["announce"] = json!("Lamp on");
    })
}

#[test]
fn rejects_invalid_accessibility_data() {
    let cases: Vec<(Value, &str)> = vec![
        (with(|v| v["description"] = json!(" padded")), "description"),
        (
            with(|v| v["description"] = json!("x".repeat(1001))),
            "1..1000",
        ),
        (with(|v| v["description"] = json!("a\u{7}b")), "control"),
        (
            with(|v| v["nodes"][0]["a11y"] = json!({"label": ""})),
            "label",
        ),
        (
            with(|v| v["nodes"][0]["a11y"] = json!({"label": "x".repeat(201)})),
            "1..200",
        ),
        (
            with(|v| v["nodes"][0]["a11y"] = json!({"label": "tab\there"})),
            "control",
        ),
        (
            with(|v| v["nodes"][0]["a11y"] = json!({"role": "banner"})),
            "banner",
        ),
        (
            with(|v| v["nodes"][0]["a11y"] = json!({"colour": "red"})),
            "colour",
        ),
        (
            with(|v| v["nodes"][0]["a11y"] = json!({"hidden": true})),
            "interactive",
        ),
        (
            with(|v| v["nodes"][1]["a11y"] = json!({"hidden": true, "label": "x"})),
            "hidden nodes",
        ),
        (
            with(|v| v["stateMachines"][0]["states"][0]["announce"] = json!("")),
            "announce",
        ),
        (
            with(|v| v["stateMachines"][0]["states"][0]["announce"] = json!("x".repeat(201))),
            "announce",
        ),
        (
            with(|v| v["stateMachines"][0]["states"][0]["announce"] = json!(3)),
            "expected a string",
        ),
    ];
    for (value, needle) in cases {
        let error = load(value.clone()).expect_err(&value.to_string());
        assert!(error.contains(needle), "{needle}: {error}");
    }
}

#[test]
fn accepts_valid_data_and_empty_a11y_object() {
    let p = load(accessible()).unwrap();
    assert!(p.a11y_enabled());
    assert_eq!(
        p.interactive_node_ids().into_iter().collect::<Vec<_>>(),
        ["switch"]
    );
    assert_eq!(p.unlabeled_interactive_nodes().len(), 0);
    let plain = load(with(|v| v["nodes"][1]["a11y"] = json!({}))).unwrap();
    assert!(plain.a11y_enabled());
    assert_eq!(plain.unlabeled_interactive_nodes(), ["switch"]);
}

#[test]
fn json_and_binary_round_trip_preserve_every_field() {
    let p = load(accessible()).unwrap();
    let json = p.to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), p);
    assert!(
        json.contains("\"announce\":\"Lamp on\"") || json.contains("\"announce\": \"Lamp on\"")
    );
    let bytes = p.to_binary().unwrap();
    assert!(Project::is_binary(&bytes));
    let back = Project::from_bytes(&bytes).unwrap();
    assert_eq!(back, p);
    assert_eq!(back.description, "A switch turns a lamp on and off.");
    assert!(back.nodes[2].a11y.as_ref().unwrap().hidden);
    assert_eq!(
        back.state_machines[0].states[1].announce.as_deref(),
        Some("Lamp on")
    );
    // the SVG metadata embedding round-trips too
    let svg = p.to_svg("/*rt*/").unwrap();
    assert_eq!(Project::from_svg_metadata(&svg).unwrap(), p);
}

#[test]
fn data_is_omitted_when_empty() {
    let p = load(base()).unwrap();
    let text = p.to_json().unwrap();
    for key in ["description", "a11y", "announce"] {
        assert!(!text.contains(key), "{key} must not be serialised");
    }
    assert!(!p.a11y_enabled());
}

#[test]
fn svg_export_adds_semantics_only_when_opted_in() {
    let plain = load(base()).unwrap();
    let svg = plain.to_svg("/*rt*/").unwrap();
    assert!(!svg.contains("aria-") && !svg.contains("role="), "{svg}");
    assert!(svg.contains("<title>Lamp &amp; &lt;scene&gt;</title>"));

    let p = load(accessible()).unwrap();
    let svg = p.to_svg("/*rt*/").unwrap();
    let doc = roxmltree::Document::parse(&svg).unwrap();
    let root = doc.root_element();
    assert_eq!(
        root.attribute("role"),
        Some("group"),
        "interactive nodes keep the root a group"
    );
    let title = doc.descendants().find(|n| n.has_tag_name("title")).unwrap();
    let desc = doc.descendants().find(|n| n.has_tag_name("desc")).unwrap();
    assert_eq!(root.attribute("aria-labelledby"), title.attribute("id"));
    assert_eq!(root.attribute("aria-describedby"), desc.attribute("id"));
    assert_eq!(title.text(), Some("Lamp & <scene>"));
    assert_eq!(desc.text(), Some("A switch turns a lamp on and off."));
    let node = |id: &str| {
        doc.descendants()
            .find(|n| n.attribute("id") == Some(id))
            .unwrap()
    };
    assert_eq!(node("switch").attribute("aria-label"), Some("Lamp switch"));
    assert_eq!(
        node("switch").attribute("aria-description"),
        Some("Toggles the lamp")
    );
    assert_eq!(node("switch").attribute("role"), Some("button"));
    assert_eq!(node("deco").attribute("aria-hidden"), Some("true"));
    assert_eq!(
        node("bulb").attribute("aria-label"),
        None,
        "no invented labels in the export"
    );
}

#[test]
fn static_export_is_an_image_and_description_alone_opts_in() {
    let p = load(with(|v| v["description"] = json!("Just a picture"))).unwrap();
    let doc_text = p.to_static_svg(0.0).unwrap();
    let doc = roxmltree::Document::parse(&doc_text).unwrap();
    assert_eq!(doc.root_element().attribute("role"), Some("img"));
    assert!(doc
        .descendants()
        .any(|n| n.has_tag_name("desc") && n.text() == Some("Just a picture")));
    let runtime = p.to_svg("/*rt*/").unwrap();
    let rd = roxmltree::Document::parse(&runtime).unwrap();
    assert_eq!(
        rd.root_element().attribute("role"),
        Some("group"),
        "interactive switch"
    );
    let no_interaction = load(with(|v| {
        v["description"] = json!("d");
        v["stateMachines"] = json!([]);
    }))
    .unwrap();
    let text = no_interaction.to_svg("/*rt*/").unwrap();
    let rd = roxmltree::Document::parse(&text).unwrap();
    assert_eq!(rd.root_element().attribute("role"), Some("img"));
}

#[test]
fn export_ids_do_not_collide_with_project_ids() {
    let p = load(with(|v| {
        v["description"] = json!("d");
        v["nodes"][2]["id"] = json!("splinely-title");
    }))
    .unwrap();
    let svg = p.to_svg("/*rt*/").unwrap();
    let doc = roxmltree::Document::parse(&svg).unwrap();
    let ids: Vec<_> = doc
        .descendants()
        .filter_map(|n| n.attribute("id"))
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(ids.len(), sorted.len(), "duplicate ids: {ids:?}");
}
