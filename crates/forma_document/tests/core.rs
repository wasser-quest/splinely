use forma_document::*;
use serde_json::json;

fn project() -> Project {
    Project {
        nodes: vec![
            Node::rect("box", 10.0, 20.0, 80.0, 40.0),
            Node::circle("dot", 0.0, 0.0, 10.0),
            Node::text("label", 0.0, 0.0, "Forma"),
        ],
        ..Default::default()
    }
}
fn key(time: f64, value: f64) -> Keyframe {
    Keyframe {
        time,
        value: AnimValue::Number(value),
        easing: Easing::Linear,
    }
}
fn clip() -> Clip {
    Clip {
        id: "move".into(),
        name: "Move".into(),
        duration: 2.0,
        autoplay: true,
        looping: false,
        blend_mode: BlendMode::Override,
        weight: 1.0,
        tracks: vec![Track {
            node_id: "box".into(),
            property: Property::X,
            keyframes: vec![key(0.0, 0.0), key(2.0, 100.0)],
        }],
    }
}
fn machine() -> StateMachine {
    StateMachine {
        id: "main".into(),
        initial: "idle".into(),
        states: vec![
            State {
                id: "idle".into(),
                on_enter: vec![],
                overrides: vec![PropertyOverride {
                    node_id: "box".into(),
                    property: Property::X,
                    value: AnimValue::Number(7.0),
                }],
            },
            State {
                id: "open".into(),
                on_enter: vec![Action::SetVariable {
                    name: "open".into(),
                    value: json!(true),
                }],
                overrides: vec![],
            },
        ],
        transitions: vec![Transition {
            from: "idle".into(),
            to: "open".into(),
            event: Event {
                name: "click".into(),
                target: Some("box".into()),
            },
            guard: Some(Guard {
                variable: "open".into(),
                equals: json!(false),
            }),
            actions: vec![Action::PlayClip {
                clip_id: "move".into(),
            }],
        }],
    }
}
fn asset() -> AudioAsset {
    AudioAsset {
        id: "sound".into(),
        name: "test".into(),
        mime_type: "audio/wav".into(),
        data_base64: "AAECAw==".into(),
    }
}

#[test]
fn roundtrip_preserves_all_data_and_enum_shape() {
    let mut p = project();
    p.clips.push(clip());
    p.variables.insert("open".into(), json!(false));
    p.state_machines.push(machine());
    p.audio_assets.push(asset());
    p.audio_cues.push(AudioCue {
        id: "cue".into(),
        asset_id: "sound".into(),
        time: 1.5,
        offset: 0.1,
        duration: Some(0.5),
        volume: 0.8,
    });
    p.speech_cues.push(SpeechCue {
        id: "speech".into(),
        time: 0.0,
        text: "Hello".into(),
        lang: "en".into(),
        rate: 1.0,
        pitch: 1.0,
        volume: 1.0,
    });
    p.raw_svg = "<path unknown='value'/>".into();
    p.custom_js = "forma.set('box', {opacity: 0.5});".into();
    let encoded = p.to_json().unwrap();
    assert_eq!(Project::from_json(&encoded).unwrap(), p);
    let v: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(v["schemaVersion"], 1);
    assert_eq!(v["nodes"][2]["kind"]["fontSize"], 16.0);
    assert_eq!(
        v["stateMachines"][0]["transitions"][0]["actions"][0]["clipId"],
        "move"
    );
    assert_eq!(
        v["clips"][0]["tracks"][0]["keyframes"][0]["value"],
        json!({"type":"number","value":0.0})
    );
}
#[test]
fn version_and_defaults() {
    assert_eq!(Project::from_json("{}").unwrap(), Project::default());
    assert!(Project::from_json("{\"schemaVersion\":2}").is_err());
    assert!(Project::from_json("{\"schemaVersion\":0}").is_err());
    assert!(Project::from_json("not json").is_err());
}
#[test]
fn rejects_bad_geometry_ids_and_numbers() {
    let mut p = project();
    p.nodes.push(p.nodes[0].clone());
    assert!(p.validate().is_err());
    p.nodes.pop();
    p.nodes[0].id = "bad\" onclick=\"x".into();
    assert!(p.validate().is_err());
    p.nodes[0].id = "forma-project".into();
    assert!(p.validate().is_err());
    p.nodes[0].id = "box".into();
    p.nodes[0].transform.x = f64::NAN;
    assert!(p.validate().is_err());
    p.nodes[0].transform.x = 0.0;
    p.width = f64::INFINITY;
    assert!(p.validate().is_err());
    p.width = 800.0;
    p.nodes[0].kind = NodeKind::Rect {
        x: 0.0,
        y: 0.0,
        width: -1.0,
        height: 1.0,
        rx: 0.0,
    };
    assert!(p.validate().is_err());
}
#[test]
fn rejects_bad_tracks() {
    let mut p = project();
    p.clips.push(clip());
    p.clips[0].tracks[0].node_id = "missing".into();
    assert!(p.validate().is_err());
    p.clips[0].tracks[0].node_id = "box".into();
    p.clips[0].tracks[0].keyframes[1].time = 0.0;
    assert!(p.validate().is_err());
    p.clips[0].tracks[0].keyframes[1].time = 3.0;
    assert!(p.validate().is_err());
    p.clips[0].tracks[0].keyframes[1].time = 2.0;
    p.clips[0].tracks[0].property = Property::Radius;
    assert!(p.validate().is_err());
    p.clips[0].tracks[0].property = Property::Fill;
    assert!(p.validate().is_err());
    p.clips[0].tracks[0].property = Property::X;
    p.clips[0].tracks[0].keyframes[0].easing = Easing::CubicBezier {
        x1: -0.1,
        y1: 0.0,
        x2: 0.5,
        y2: 1.0,
    };
    assert!(p.validate().is_err());
    p.clips[0].tracks[0].keyframes[0].easing = Easing::Linear;
    let t = p.clips[0].tracks[0].clone();
    p.clips[0].tracks.push(t);
    assert!(p.validate().is_err());
}
#[test]
fn linear_deterministic_clamp_and_loop() {
    let mut p = project();
    p.clips.push(clip());
    let original = p.clone();
    assert_eq!(p.evaluate(1.0).unwrap()[0].transform.x, 50.0);
    assert_eq!(p.evaluate(9.0).unwrap()[0].transform.x, 100.0);
    assert_eq!(p.evaluate(1.0).unwrap(), p.evaluate(1.0).unwrap());
    assert_eq!(p, original);
    p.clips[0].looping = true;
    assert_eq!(p.evaluate(2.0).unwrap()[0].transform.x, 0.0);
    assert_eq!(p.evaluate(3.0).unwrap()[0].transform.x, 50.0);
    assert!(p.evaluate(-1.0).is_err());
    assert!(p.evaluate(f64::NAN).is_err());
}
#[test]
fn step_and_cubic_bezier() {
    let mut p = project();
    p.clips.push(clip());
    p.clips[0].tracks[0].keyframes[0].easing = Easing::Step;
    assert_eq!(p.evaluate(1.99).unwrap()[0].transform.x, 0.0);
    assert_eq!(p.evaluate(2.0).unwrap()[0].transform.x, 100.0);
    p.clips[0].tracks[0].keyframes[0].easing = Easing::CubicBezier {
        x1: 0.25,
        y1: 0.1,
        x2: 0.25,
        y2: 1.0,
    };
    assert!((p.evaluate(1.0).unwrap()[0].transform.x - 80.2403387584857).abs() < 1e-9);
}
#[test]
fn color_interpolation_and_geometry_property() {
    let mut p = project();
    let mut c = clip();
    c.tracks.push(Track {
        node_id: "dot".into(),
        property: Property::Fill,
        keyframes: vec![
            Keyframe {
                time: 0.0,
                value: AnimValue::Color(Color::rgb(255, 0, 0)),
                easing: Easing::Linear,
            },
            Keyframe {
                time: 2.0,
                value: AnimValue::Color(Color {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 0.5,
                }),
                easing: Easing::Linear,
            },
        ],
    });
    c.tracks.push(Track {
        node_id: "dot".into(),
        property: Property::Radius,
        keyframes: vec![key(0.0, 10.0), key(2.0, 20.0)],
    });
    p.clips.push(c);
    let nodes = p.evaluate(1.0).unwrap();
    assert_eq!(
        nodes[1].style.fill,
        Color {
            r: 0.5,
            g: 0.0,
            b: 0.5,
            a: 0.75
        }
    );
    assert!(matches!(
        nodes[1].kind,
        NodeKind::Circle { radius: 15.0, .. }
    ));
}
#[test]
fn precedence_context_and_clip_selection() {
    let mut p = project();
    p.clips.push(clip());
    p.variables.insert("open".into(), json!(false));
    p.state_machines.push(machine());
    let mut context = EvaluationContext {
        clips: Some(vec![]),
        ..Default::default()
    };
    assert_eq!(
        p.evaluate_with_context(1.0, &context).unwrap()[0]
            .transform
            .x,
        7.0
    );
    context.clips = None;
    assert_eq!(
        p.evaluate_with_context(1.0, &context).unwrap()[0]
            .transform
            .x,
        50.0
    );
    context.overrides.push(PropertyOverride {
        node_id: "box".into(),
        property: Property::X,
        value: AnimValue::Number(99.0),
    });
    assert_eq!(
        p.evaluate_with_context(1.0, &context).unwrap()[0]
            .transform
            .x,
        99.0
    );
    let mut c = clip();
    c.id = "second".into();
    c.tracks[0].keyframes = vec![key(0.0, 70.0)];
    p.clips.push(c);
    assert_eq!(p.evaluate(1.0).unwrap()[0].transform.x, 70.0);
    assert_eq!(p.evaluate_clip("move", 1.0).unwrap()[0].transform.x, 50.0);
    assert!(p.evaluate_clip("missing", 0.0).is_err());
    context.states.insert("main".into(), "missing".into());
    assert!(p.evaluate_with_context(0.0, &context).is_err());
}
#[test]
fn state_actions_guards_priority_and_bounded_emit() {
    let mut p = project();
    p.clips.push(clip());
    p.variables.insert("open".into(), json!(false));
    let mut m = machine();
    m.states[0].on_enter.push(Action::Emit {
        event: Event {
            name: "click".into(),
            target: Some("box".into()),
        },
    });
    let mut second = m.transitions[0].clone();
    second.to = "idle".into();
    m.transitions.push(second);
    p.state_machines.push(m);
    let mut instance = MachineInstance::new(&p, "main").unwrap();
    assert_eq!(instance.initial_actions.len(), 1);
    assert_eq!(instance.current_state, "idle");
    assert!(instance
        .send(
            &p,
            &Event {
                name: "click".into(),
                target: Some("dot".into())
            }
        )
        .unwrap()
        .is_empty());
    instance.variables.insert("open".into(), json!(true));
    assert!(instance
        .send(
            &p,
            &Event {
                name: "click".into(),
                target: Some("box".into())
            }
        )
        .unwrap()
        .is_empty());
    instance.variables.insert("open".into(), json!(false));
    let actions = instance
        .send(
            &p,
            &Event {
                name: "click".into(),
                target: Some("box".into()),
            },
        )
        .unwrap();
    assert!(matches!(&actions[0],Action::PlayClip {clip_id} if clip_id=="move"));
    assert!(matches!(&actions[1], Action::SetVariable { .. }));
    assert_eq!(instance.current_state, "open");
    assert_eq!(instance.variables["open"], json!(true));
}
#[test]
fn validates_state_references_and_typed_variables() {
    let mut p = project();
    p.clips.push(clip());
    p.variables.insert("open".into(), json!(false));
    p.state_machines.push(machine());
    assert!(p.validate().is_ok());
    p.state_machines[0].initial = "absent".into();
    assert!(p.validate().is_err());
    p.state_machines[0].initial = "idle".into();
    p.state_machines[0].transitions[0]
        .guard
        .as_mut()
        .unwrap()
        .equals = json!(1);
    assert!(p.validate().is_err());
    p.state_machines[0].transitions[0]
        .guard
        .as_mut()
        .unwrap()
        .equals = json!(false);
    p.state_machines[0].states[1].on_enter[0] = Action::PlayAudio {
        asset_id: "missing".into(),
    };
    assert!(p.validate().is_err());
}
#[test]
fn validates_audio_base64_and_speech() {
    let mut p = project();
    p.audio_assets.push(asset());
    assert!(p.validate().is_ok());
    for bad in ["A", "AA=", "AA==evil", "AB==", "AAB=", "====", "AA\n="] {
        p.audio_assets[0].data_base64 = bad.into();
        assert!(p.validate().is_err(), "{bad}");
    }
    p.audio_assets[0].data_base64 = "AAA=".into();
    assert!(p.validate().is_ok());
    for bad in ["audio/", "audio/a b", "text/plain", "audio/wav; rate=1"] {
        p.audio_assets[0].mime_type = bad.into();
        assert!(p.validate().is_err());
    }
    p.audio_assets[0].mime_type = "audio/wav".into();
    p.audio_cues.push(AudioCue {
        id: "cue".into(),
        asset_id: "sound".into(),
        time: 0.0,
        offset: 0.0,
        duration: None,
        volume: 2.0,
    });
    assert!(p.validate().is_err());
    p.audio_cues[0].volume = 1.0;
    p.speech_cues.push(SpeechCue {
        id: "speech".into(),
        time: 0.0,
        text: "Hello".into(),
        lang: "en".into(),
        rate: 0.0,
        pitch: 1.0,
        volume: 1.0,
    });
    assert!(p.validate().is_err());
}
#[test]
fn history_snapshots_branching_and_failed_commands() {
    let mut p = project();
    let original = p.clone();
    let mut h = History::default();
    h.execute(&mut p, Command::AddNode(Node::circle("new", 1.0, 2.0, 3.0)))
        .unwrap();
    let added = p.clone();
    assert!(h.undo(&mut p));
    assert_eq!(p, original);
    assert!(h.redo(&mut p));
    assert_eq!(p, added);
    assert!(h
        .execute(&mut p, Command::AddNode(Node::circle("new", 0.0, 0.0, 1.0)))
        .is_err());
    assert_eq!(p, added);
    assert!(h.undo(&mut p));
    let mut replacement = p.clone();
    replacement.custom_js = "hello()".into();
    replacement.raw_svg = "<defs/>".into();
    replacement.audio_assets.push(asset());
    h.execute(&mut p, Command::ReplaceProject(replacement.clone()))
        .unwrap();
    assert!(!h.can_redo());
    assert!(h.undo(&mut p));
    assert_eq!(p, original);
    assert!(h.redo(&mut p));
    assert_eq!(p, replacement);
    assert!(!h.redo(&mut p));
}
#[test]
fn history_batches_are_atomic_and_references_not_silently_deleted() {
    let mut p = project();
    p.clips.push(clip());
    let before = p.clone();
    let mut h = History::default();
    assert!(h
        .execute(&mut p, Command::RemoveNode { id: "box".into() })
        .is_err());
    assert_eq!(p, before);
    assert!(!h.can_undo());
    let mut edited = p.nodes[0].clone();
    edited.transform.x = 12.0;
    h.execute_batch(
        &mut p,
        [
            Command::ReplaceNode(edited),
            Command::AddNode(Node::circle("new", 0.0, 0.0, 1.0)),
        ],
    )
    .unwrap();
    assert!(h.undo(&mut p));
    assert_eq!(p, before);
    assert!(!h.can_undo());
    h.execute_batch(&mut p, []).unwrap();
    assert!(h.can_redo());
}
#[test]
fn svg_is_well_formed_and_preserves_script_and_json() {
    let mut p = project();
    p.name = "<title & \"quote\">".into();
    p.nodes[2].kind = NodeKind::Text {
        x: 0.0,
        y: 0.0,
        text: "<script>&\"\r\n".into(),
        font_size: 16.0,
    };
    p.custom_js = "</metadata><script>alert('x')</script> ]]> \u{ffff}".into();
    p.raw_svg = "<script>doNotInject()</script>".into();
    p.audio_assets.push(asset());
    let runtime = "const a = ']]>'; const b = '</script>';\n// & < >";
    let exported = p.to_svg(runtime).unwrap();
    let doc = roxmltree::Document::parse(&exported).unwrap();
    assert_eq!(doc.root_element().tag_name().name(), "svg");
    let metadata = doc
        .descendants()
        .find(|n| n.has_tag_name("metadata") && n.attribute("id") == Some("forma-project"))
        .unwrap();
    let decoded: Project = serde_json::from_str(metadata.text().unwrap()).unwrap();
    assert_eq!(decoded, p);
    let script = doc
        .descendants()
        .find(|n| n.has_tag_name("script"))
        .unwrap();
    let code = script
        .children()
        .filter_map(|n| n.text())
        .collect::<String>();
    assert_eq!(code, runtime);
    assert_eq!(
        doc.descendants()
            .filter(|n| n.has_tag_name("script"))
            .count(),
        1
    );
    let text = doc.descendants().find(|n| n.has_tag_name("text")).unwrap();
    assert_eq!(text.text().unwrap(), "<script>&\"\r\n");
}
#[test]
fn static_export_evaluates_and_excludes_behavior() {
    let mut p = project();
    p.clips.push(clip());
    p.custom_js = "bad()".into();
    let svg = p.to_static_svg(1.0).unwrap();
    let doc = roxmltree::Document::parse(&svg).unwrap();
    assert_eq!(
        doc.descendants()
            .find(|n| n.has_tag_name("rect"))
            .unwrap()
            .attribute("transform"),
        Some("translate(50 0) rotate(0) scale(1 1)")
    );
    assert!(!svg.contains("<script"));
    assert!(!doc
        .descendants()
        .any(|n| n.attribute("id") == Some("forma-project")));
    assert!(doc
        .descendants()
        .any(|n| n.has_tag_name(("urn:forma:metadata", "generator"))));
    p.name = "\0".into();
    assert!(p.to_svg("").is_err());
    p.name = "OK".into();
    assert!(p.to_svg("\0").is_err());
}

#[test]
fn gradients_validate_round_trip_and_export_with_unique_ids() {
    let mut project = Project::default();
    let mut node = Node::rect("box", 0., 0., 100., 50.);
    node.gradient = Some(Gradient::linear(
        90.,
        Color::rgb(255, 0, 0),
        Color::rgb(0, 0, 255),
    ));
    project.nodes.push(node);
    // A node already uses the natural gradient ID; export must avoid it.
    project.nodes.push(Node::circle("box-gradient", 0., 0., 5.));
    project.validate().unwrap();
    let json = project.to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), project);
    let svg = project.to_static_svg(0.).unwrap();
    assert!(
        svg.contains("<linearGradient id=\"box-gradient_\""),
        "{svg}"
    );
    assert!(svg.contains("fill=\"url(#box-gradient_)\""));
    assert!(svg.contains("x1=\"0.5\" y1=\"0\""));
    let parsed = roxmltree::Document::parse(&svg).unwrap();
    assert!(parsed.descendants().any(|n| n.has_tag_name("stop")));

    let mut radial = project.clone();
    radial.nodes[0].gradient.as_mut().unwrap().radial = true;
    assert!(radial
        .to_static_svg(0.)
        .unwrap()
        .contains("<radialGradient"));

    let invalid = |change: fn(&mut Gradient)| {
        let mut broken = project.clone();
        change(broken.nodes[0].gradient.as_mut().unwrap());
        broken.validate().is_err()
    };
    assert!(invalid(|g| g.stops.truncate(1)), "too few stops");
    assert!(invalid(|g| g.stops[1].offset = 1.5), "offset above 1");
    assert!(
        invalid(|g| {
            g.stops[0].offset = 1.;
            g.stops[1].offset = 0.2;
        }),
        "decreasing offsets"
    );
    assert!(invalid(|g| g.angle = f64::NAN), "non-finite angle");
    // Plain projects do not gain a gradient field.
    assert!(!Project::default().to_json().unwrap().contains("gradient"));
}

#[test]
fn nested_groups_export_as_nested_svg_groups() {
    let mut project = Project::default();
    for (i, id) in ["a", "b", "c", "d"].iter().enumerate() {
        project
            .nodes
            .push(Node::rect(*id, i as f64 * 20., 0., 10., 10.));
    }
    let ids = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let inner = project.group_nodes(&ids(&["a", "b"]), Some("Arm")).unwrap();
    let outer = project
        .group_nodes(&ids(&["a", "b", "c"]), Some("Figur & Co"))
        .unwrap();
    let svg = project.to_static_svg(0.).unwrap();
    let doc = roxmltree::Document::parse(&svg).unwrap();
    let parent = |id: &str| {
        doc.descendants()
            .find(|n| n.attribute("id") == Some(id))
            .and_then(|n| n.parent_element())
            .and_then(|p| p.attribute("id").map(str::to_owned))
    };
    assert_eq!(parent("a").as_deref(), Some(inner.as_str()));
    assert_eq!(parent(&inner).as_deref(), Some(outer.as_str()));
    assert_eq!(parent("c").as_deref(), Some(outer.as_str()));
    assert_eq!(parent("d"), None);
    assert!(svg.contains("aria-label=\"Figur &amp; Co\""));
    let json = project.to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), project);
}
