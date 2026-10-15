//! `--runtime rust` preparation-level tests: no GUI, no display. They drive `RustSession`
//! exactly like the event loop does (IPC messages in, frame messages out) and, when `node`
//! is installed, check the page-side frame applier against the JS runtime.
use serde_json::{json, Value};
use splinely_document::Project;
use splinely_viewer::rust_mode::{self, IpcOutcome};
use splinely_viewer::{prepare, RuntimeMode, RustSession, ViewerOptions};
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn example(name: &str) -> PathBuf {
    root().join("examples").join(name)
}
fn project(name: &str) -> Project {
    Project::from_bytes(&std::fs::read(example(name)).unwrap()).unwrap()
}
fn rust_options() -> ViewerOptions {
    ViewerOptions {
        runtime: RuntimeMode::Rust,
        ..Default::default()
    }
}
fn input(session: &mut RustSession, body: Value) -> Result<IpcOutcome, String> {
    session.handle_ipc(&body.to_string())
}

// ---- parity with the JS runtime (needs node) -------------------------------------------

enum Op {
    Tick(u32),
    Event(&'static str, Option<&'static str>),
    Pointer(f64, f64),
    Leave,
}
fn run_script(
    name: &str,
    project_path: &Path,
    project: &Project,
    ops: &[Op],
    checkpoint_after: &[usize],
) -> Value {
    let mut session = RustSession::new(project).unwrap();
    let mut steps = vec![];
    let mut first = true;
    let mut checkpoint = |session: &mut RustSession, label: String, steps: &mut Vec<Value>| {
        let message = if first {
            Some(session.full_message())
        } else {
            session.frame_message()
        };
        first = false;
        steps.push(json!({
            "checkpoint": label,
            "message": message.map(|m| serde_json::from_str::<Value>(&m).unwrap()),
        }));
    };
    checkpoint(&mut session, "start".into(), &mut steps);
    for (index, op) in ops.iter().enumerate() {
        match op {
            Op::Tick(n) => {
                for _ in 0..*n {
                    session.tick();
                }
                steps.push(json!({ "tick": n }));
            }
            Op::Event(event, target) => {
                session.send_event(event, *target).unwrap();
                steps.push(json!({ "event": { "name": event, "target": target } }));
            }
            Op::Pointer(x, y) => {
                let outcome = input(
                    &mut session,
                    json!({ "t": "input", "pointer": { "x": x, "y": y } }),
                );
                assert_eq!(
                    outcome,
                    Ok(IpcOutcome::Input {
                        forwarded: 1,
                        dropped: 0
                    })
                );
                steps.push(json!({ "pointer": { "x": x, "y": y } }));
            }
            Op::Leave => {
                input(&mut session, json!({ "t": "input", "leave": true })).unwrap();
                steps.push(json!({ "leave": true }));
            }
        }
        if checkpoint_after.contains(&index) {
            checkpoint(&mut session, format!("{name}#{index}"), &mut steps);
        }
    }
    assert!(session.halted().is_none());
    json!({
        "name": name,
        "projectPath": project_path,
        "staticSvg": project.to_static_svg(0.0).unwrap(),
        "steps": steps,
    })
}
fn timeline(ops: &mut Vec<Op>, cps: &mut Vec<usize>, ticks: &[u32]) {
    for t in ticks {
        ops.push(Op::Tick(*t));
        cps.push(ops.len() - 1);
    }
}

#[test]
fn frame_applier_matches_js_runtime_for_examples() {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("skipped: node is not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut scenarios = vec![];
    // Autoplay/clip/state-machine projects at several times (0.33 s .. 11.7 s).
    for name in [
        "stickfight.splinely.json",
        "desert-stickfight.splinely.json",
        "rig.splinely.json",
        "brown-owl.splinely.json",
        "cactus-runner.splinely.json",
        "water-drop.splinely.json",
    ] {
        let (mut ops, mut cps) = (vec![], vec![]);
        timeline(&mut ops, &mut cps, &[20, 41, 139, 200, 300]);
        if name.starts_with("brown-owl") {
            ops.push(Op::Event("owl.click", None));
            cps.push(ops.len() - 1);
            timeline(&mut ops, &mut cps, &[45]);
        }
        if name.starts_with("cactus") {
            ops.push(Op::Event("JUMP", None));
            timeline(&mut ops, &mut cps, &[10, 50]);
        }
        let path = example(name);
        scenarios.push(run_script(name, &path, &project(name), &ops, &cps));
    }
    // demo: the click arrives through the validated IPC path like a real pointer click.
    let demo = project("demo.splinely.json");
    let mut session = RustSession::new(&demo).unwrap();
    let mut steps = vec![
        json!({"checkpoint":"start","message":serde_json::from_str::<Value>(&session.full_message()).unwrap()}),
    ];
    for _ in 0..10 {
        session.tick();
    }
    steps.push(json!({"tick":10}));
    let clicked = input(
        &mut session,
        json!({"t":"input","events":[{"name":"click","target":"ball"}]}),
    );
    assert_eq!(
        clicked,
        Ok(IpcOutcome::Input {
            forwarded: 1,
            dropped: 0
        })
    );
    steps.push(json!({"event":{"name":"click","target":"ball"}}));
    for (i, n) in [1, 30, 90].into_iter().enumerate() {
        for _ in 0..n {
            session.tick();
        }
        steps.push(json!({"tick":n}));
        let message = session
            .frame_message()
            .map(|m| serde_json::from_str::<Value>(&m).unwrap());
        assert!(message.is_some(), "frame {i} changed after the click");
        steps.push(json!({"checkpoint":format!("demo#{i}"),"message":message}));
    }
    scenarios.push(json!({
        "name":"demo","projectPath":example("demo.splinely.json"),
        "staticSvg":demo.to_static_svg(0.0).unwrap(),"steps":steps,
    }));
    // Followers + pointer: a synthetic follower on the rig example.
    let mut value: Value =
        serde_json::from_slice(&std::fs::read(example("rig.splinely.json")).unwrap()).unwrap();
    let target = value["nodes"][1]["id"].as_str().unwrap().to_owned();
    value["followers"] = json!([{ "id": "look", "target": target, "mode": "offset", "strength": 1.0, "max": 25.0, "smoothing": 0.25 }]);
    let followed = dir.path().join("follow.splinely.json");
    std::fs::write(&followed, value.to_string()).unwrap();
    let follow_project = Project::from_json(&value.to_string()).unwrap();
    let ops = vec![
        Op::Tick(5),
        Op::Pointer(500.0, 40.0),
        Op::Tick(3),
        Op::Tick(30),
        Op::Pointer(40.0, 400.0),
        Op::Tick(60),
        Op::Leave,
        Op::Tick(90),
    ];
    scenarios.push(run_script(
        "followers",
        &followed,
        &follow_project,
        &ops,
        &[1, 2, 3, 5, 6, 7],
    ));

    let bundle = dir.path().join("bundle.json");
    std::fs::write(&bundle, json!({ "scenarios": scenarios }).to_string()).unwrap();
    let out = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/frame_applier.test.cjs"))
        .arg(&bundle)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "node parity failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("parity ok"), "{stdout}");
    eprintln!("{stdout}");
}

#[test]
fn page_glue_and_applier_work_without_a_project() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }
    let out = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/frame_applier.test.cjs"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// ---- IPC validation ----------------------------------------------------------------------

fn demo_session() -> RustSession {
    RustSession::new(&project("demo.splinely.json")).unwrap()
}

#[test]
fn ipc_rejects_malformed_oversized_and_unknown_messages() {
    let mut s = demo_session();
    let bad = [
        "".to_owned(),
        "not json".to_owned(),
        r#"{"t":"nope"}"#.to_owned(),
        r#"{"t":"ready","extra":1}"#.to_owned(),
        r#"{"t":"input","events":[{"name":"click","extra":1}]}"#.to_owned(),
        r#"{"t":"input","pointer":{"x":"1","y":2}}"#.to_owned(),
        r#"{"t":"input","pointer":{"x":1e9,"y":2}}"#.to_owned(),
        r#"{"t":"input","pointer":{"x":1,"y":2},"leave":true}"#.to_owned(),
        r#"{"t":"input","events":[{"name":1}]}"#.to_owned(),
        format!(r#"{{"t":"error","message":"{}"}}"#, "x".repeat(600)),
        format!(
            r#"{{"t":"input","events":[{}]}}"#,
            vec![r#"{"name":"click"}"#; 65].join(",")
        ),
        format!(
            r#"{{"t":"error","message":"{}"}}"#,
            "y".repeat(rust_mode::MAX_IPC_BYTES)
        ),
    ];
    for body in &bad {
        assert!(
            s.handle_ipc(body).is_err(),
            "accepted: {}",
            &body[..body.len().min(80)]
        );
    }
    assert_eq!(s.handle_ipc(r#"{"t":"ready"}"#), Ok(IpcOutcome::Ready));
    assert_eq!(s.handle_ipc(r#"{"t":"ack"}"#), Ok(IpcOutcome::Ack));
    // Unknown event names and unknown targets are dropped, valid ones forwarded.
    let outcome = input(
        &mut s,
        json!({"t":"input","events":[
            {"name":"click","target":"ball"},
            {"name":"owl.click"},
            {"name":"click","target":"no-such-node"},
            {"name":"click","target":"x".repeat(300)},
            {"name":"keydown"}
        ]}),
    );
    assert_eq!(
        outcome,
        Ok(IpcOutcome::Input {
            forwarded: 2,
            dropped: 3
        })
    );
    // Page error text is stripped of control characters.
    let reported = s.handle_ipc(r#"{"t":"error","message":"a\u001b[31mb\nc"}"#);
    assert_eq!(reported, Ok(IpcOutcome::PageError("a[31mbc".into())));
}

#[test]
fn clicks_reach_the_rust_runtime_and_change_the_next_frame() {
    let mut s = demo_session();
    let first = s.full_message();
    assert!(first.contains("\"id\":\"ball\""));
    assert_eq!(s.frame_message(), None, "nothing changed yet");
    s.tick();
    s.tick();
    input(
        &mut s,
        json!({"t":"input","events":[{"name":"click","target":"ball"}]}),
    )
    .unwrap();
    for _ in 0..30 {
        s.tick();
    }
    let delta = s.frame_message().expect("click started the clip");
    let delta: Value = serde_json::from_str(&delta).unwrap();
    let nodes = delta["nodes"].as_array().unwrap();
    assert!(nodes.iter().any(|n| n["id"] == "ball"));
    assert!(nodes.len() <= 3, "only changed nodes are sent");
    assert_eq!(s.ticks(), 32);
}

#[test]
fn audio_host_actions_are_reported_not_played() {
    let mut s = demo_session();
    input(
        &mut s,
        json!({"t":"input","events":[{"name":"click","target":"ball"}]}),
    )
    .unwrap();
    for _ in 0..3 {
        s.tick();
    }
    let notes = s.take_notes().join("\n");
    // The demo's audio is a cue on the timeline; whichever fires first is reported once.
    assert!(
        notes.is_empty() || notes.contains("Audio request ignored"),
        "{notes}"
    );
    assert!(s.take_notes().is_empty(), "notes are delivered once");
}

// ---- preparation / page ------------------------------------------------------------------

#[test]
fn rust_page_is_self_contained_and_states_what_is_missing() {
    let prepared = prepare(&example("demo.splinely.json"), rust_options()).unwrap();
    assert_eq!(prepared.runtime, RuntimeMode::Rust);
    assert!(prepared.rust_session.is_some());
    assert!(prepared.custom_js_disabled);
    let joined = prepared.diagnostics.join("\n");
    assert!(
        joined.contains("customJs") && joined.contains("NOT executed"),
        "{joined}"
    );
    assert!(joined.contains("Audio is not supported"), "{joined}");
    let html = &prepared.html;
    // Visible diagnostic in the page itself.
    assert!(
        html.contains("id=\"splinely-diagnostics\" role=\"status\"><div>customJs"),
        "{}",
        &html[..600.min(html.len())]
    );
    // No JS runtime, no customJs, no iframe/data document, no eval, no network.
    assert!(
        !html.contains("splinely-project")
            && !html.contains("<iframe")
            && !html.contains("splinely.on(")
    );
    assert!(
        html.contains("script-src 'unsafe-inline'; style-src") && !html.contains("unsafe-eval")
    );
    assert!(html.contains("connect-src 'none'") && html.contains("frame-src 'none'"));
    assert!(html.contains("<svg ") && html.contains("data-splinely-node=\"ball\""));
    assert_eq!(
        html.matches("</script>").count(),
        3,
        "inline scripts only: applier, host, bootstrap"
    );
    assert!(!html.contains("<script src") && !html.contains("<script type"));
}

#[test]
fn rust_mode_without_unsupported_features_has_no_diagnostics() {
    for name in ["stickfight.splinely.json", "rig.splinely.json"] {
        let prepared = prepare(&example(name), rust_options()).unwrap();
        assert_eq!(prepared.runtime, RuntimeMode::Rust);
        assert!(
            prepared.diagnostics.is_empty(),
            "{name}: {:?}",
            prepared.diagnostics
        );
        assert!(prepared
            .html
            .contains("id=\"splinely-diagnostics\" role=\"status\" hidden>"));
        assert!(!prepared.custom_js_disabled);
    }
}

#[test]
fn binary_sply_is_accepted_in_rust_mode() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.sply");
    std::fs::write(&path, project("rig.splinely.json").to_binary().unwrap()).unwrap();
    let prepared = prepare(&path, rust_options()).unwrap();
    assert_eq!(prepared.runtime, RuntimeMode::Rust);
}

#[test]
fn rust_mode_refuses_svg_trusted_js_and_leaves_js_default_unchanged() {
    let svg = prepare(&example("demo.svg"), rust_options())
        .err()
        .unwrap()
        .to_string();
    assert!(
        svg.contains("--runtime rust needs a Splinely project"),
        "{svg}"
    );
    let trusted = prepare(
        &example("demo.splinely.json"),
        ViewerOptions {
            trusted_js: true,
            ..rust_options()
        },
    )
    .err()
    .unwrap()
    .to_string();
    assert!(
        trusted.contains("--trusted-js cannot be combined with --runtime rust"),
        "{trusted}"
    );
    let js = prepare(&example("demo.splinely.json"), ViewerOptions::default()).unwrap();
    assert_eq!(js.runtime, RuntimeMode::Js);
    assert!(js.rust_session.is_none() && js.diagnostics.is_empty());
    assert!(js.html.contains("<iframe") && js.html.contains("unsafe-eval"));
}

#[test]
fn project_the_rust_runtime_cannot_start_falls_back_to_js_with_a_reason() {
    // An entry action that re-emits the event it reacts to never settles: the Rust runtime
    // refuses to start, the document model and the JS export accept it.
    let json = r#"{"schemaVersion":1,"name":"loop","width":10,"height":10,"nodes":[],
      "stateMachines":[{"id":"m","initial":"s","states":[{"id":"s","onEnter":[{"type":"emit","event":{"name":"again"}}],"overrides":[]}],
        "transitions":[{"from":"s","to":"s","event":{"name":"again"},"actions":[]}]}]}"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("loop.json");
    std::fs::write(&path, json).unwrap();
    let prepared = prepare(&path, rust_options()).unwrap();
    assert_eq!(
        prepared.runtime,
        RuntimeMode::Js,
        "{:?}",
        prepared.diagnostics
    );
    assert!(prepared.diagnostics[0].contains("falling back to the JS runtime"));
    assert!(prepared.html.contains("<iframe"));
}

#[test]
fn page_data_cannot_break_out_of_the_inline_script() {
    let mut value: Value =
        serde_json::from_slice(&std::fs::read(example("rig.splinely.json")).unwrap()).unwrap();
    value["nodes"][0]["id"] = json!("</script><b>x");
    value["name"] = json!("</script><i>&\u{2028}");
    // The id also appears as an SVG attribute; either it is rejected or it is escaped.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("evil.json");
    std::fs::write(&path, value.to_string()).unwrap();
    if let Ok(prepared) = prepare(&path, rust_options()) {
        assert_eq!(prepared.html.matches("</script>").count(), 3);
        assert!(!prepared.html.contains("</script><b>"));
    }
}
