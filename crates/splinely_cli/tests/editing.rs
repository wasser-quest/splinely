use serde_json::{json, Value};
use std::process::{Command, Output};

fn invoke(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_splinely-cli"))
        .args(args)
        .output()
        .unwrap()
}
fn success(args: &[&str]) -> Value {
    let output = invoke(args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert!(output.stdout.ends_with(b"\n"));
    serde_json::from_slice(&output.stdout).unwrap()
}
fn error(args: &[&str], code: &str) {
    let output = invoke(args);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let response: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], code, "{response}");
}
fn read(path: &str) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn rig_bones_binding_and_keyframes_use_real_cli() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rig.json");
    let path = path.to_str().unwrap();
    success(&["rig-demo", "--output", path]);
    assert_eq!(read(path)["bones"].as_array().unwrap().len(), 10);
    success(&["validate", path]);
    let before = success(&["evaluate", path, "--time", "0"]);
    let after = success(&["evaluate", path, "--time", "1"]);
    assert_ne!(
        before["nodes"][6]["transform"]["rotation"],
        after["nodes"][6]["transform"]["rotation"]
    );
    success(&[
        "bone", "add", path, "--id", "extra", "--parent", "neck", "--length", "12", "--output",
        path,
    ]);
    success(&[
        "bone",
        "update",
        path,
        "--id",
        "extra",
        "--root",
        "--x",
        "10",
        "--rotation",
        "-30",
        "--output",
        path,
    ]);
    let saved = read(path);
    let extra = saved["bones"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == "extra")
        .unwrap();
    assert!(extra["parentId"].is_null());
    assert_eq!(extra["rotation"], -30.0);
    success(&[
        "bone", "bind", path, "--node", "head", "--bone", "extra", "--output", path,
    ]);
    assert_eq!(read(path)["nodes"][11]["boneId"], "extra");
    for (time, value) in [("1.5", "50"), ("0.5", "10"), ("1.5", "70")] {
        success(&[
            "bone",
            "keyframe",
            path,
            "--bone",
            "extra",
            "--clip",
            "wave",
            "--property",
            "x",
            "--time",
            time,
            "--value",
            value,
            "--output",
            path,
        ]);
    }
    let saved = read(path);
    let track = &saved["clips"][0]["tracks"][1];
    assert_eq!(track["nodeId"], "extra");
    assert_eq!(track["property"], "x");
    assert_eq!(track["keyframes"].as_array().unwrap().len(), 2);
    assert_eq!(track["keyframes"][0]["time"], 0.5);
    assert_eq!(track["keyframes"][1]["value"]["value"], 70.0);
    let bytes = std::fs::read(path).unwrap();
    for (args, code) in [
        (
            vec![
                "bone", "update", path, "--id", "torso", "--parent", "neck", "--output", path,
            ],
            "validation_error",
        ),
        (
            vec![
                "bone", "add", path, "--id", "extra", "--length", "12", "--output", path,
            ],
            "validation_error",
        ),
        (
            vec![
                "bone", "add", path, "--id", "bad", "--length", "0", "--output", path,
            ],
            "validation_error",
        ),
        (
            vec![
                "bone", "bind", path, "--node", "head", "--bone", "missing", "--output", path,
            ],
            "not_found",
        ),
        (
            vec![
                "bone", "keyframe", path, "--bone", "extra", "--clip", "wave", "--time", "3",
                "--value", "0", "--output", path,
            ],
            "invalid_time",
        ),
        (
            vec![
                "bone",
                "update",
                path,
                "--id",
                "extra",
                "--x",
                "9007199254740993",
                "--output",
                path,
            ],
            "invalid_arguments",
        ),
        (
            vec![
                "bone", "update", path, "--id", "extra", "--x", "NaN", "--output", path,
            ],
            "invalid_arguments",
        ),
    ] {
        error(&args, code);
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn conversions_layers_and_defaults_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vectors.json");
    let path = path.to_str().unwrap();
    // Missing serde fields must default without losing existing authored data.
    std::fs::write(
        path,
        json!({"nodes":[
            {"id":"rect","kind":{"type":"rect","x":1,"y":2,"width":50,"height":20,"rx":4}},
            {"id":"circle","kind":{"type":"circle","cx":20,"cy":20,"radius":10}},
            {"id":"text","kind":{"type":"text","x":0,"y":0,"text":"hello","fontSize":16}}
        ]})
        .to_string(),
    )
    .unwrap();
    let inspect = success(&["inspect", path]);
    assert_eq!(inspect["nodes"][0]["locked"], false);
    assert!(inspect["nodes"][0]["originalKind"].is_null());
    for id in ["rect", "circle"] {
        let original = inspect["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .unwrap()["kind"]
            .clone();
        let converted = success(&["convert-to-path", path, "--node", id, "--output", "-"]);
        let node = converted["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .unwrap();
        assert_eq!(node["kind"]["type"], "path");
        assert_eq!(node["originalKind"], original);
        success(&["convert-to-path", path, "--node", id, "--output", path]);
        success(&["convert-back", path, "--node", id, "--output", path]);
        let saved = read(path);
        let node = saved["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .unwrap();
        assert_eq!(node["kind"], original);
        assert!(node["originalKind"].is_null());
    }
    success(&[
        "layer", "reorder", path, "--node", "rect", "--index", "2", "--output", path,
    ]);
    assert_eq!(read(path)["nodes"][2]["id"], "rect");
    for (command, flag, field) in [
        ("hide", "--hidden", "visible"),
        ("lock", "--locked", "locked"),
    ] {
        success(&["layer", command, path, "--node", "rect", "--output", path]);
        assert_eq!(read(path)["nodes"][2][field], command == "lock");
        success(&[
            "layer", command, path, "--node", "rect", flag, "false", "--output", path,
        ]);
        assert_eq!(read(path)["nodes"][2][field], command == "hide");
    }
    let bytes = std::fs::read(path).unwrap();
    // Text converts with system fonts now (see shapes_cli); a never-converted
    // text cannot be converted back.
    error(
        &["convert-back", path, "--node", "text", "--output", path],
        "conversion_error",
    );
    error(
        &["convert-back", path, "--node", "rect", "--output", path],
        "conversion_error",
    );
    error(
        &[
            "layer", "reorder", path, "--node", "rect", "--index", "3", "--output", path,
        ],
        "invalid_arguments",
    );
    error(
        &["layer", "hide", path, "--node", "missing", "--output", path],
        "not_found",
    );
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    success(&["convert-to-path", path, "--node", "rect", "--output", path]);
    let mut edited = read(path);
    edited["nodes"][2]["kind"]["d"] = json!("M0 0 L1 0 L1 1 Z");
    edited["nodes"][2]["kind"]["bounds"] = json!([0, 0, 1, 1]);
    std::fs::write(path, edited.to_string()).unwrap();
    let bytes = std::fs::read(path).unwrap();
    error(
        &["convert-back", path, "--node", "rect", "--output", path],
        "conversion_error",
    );
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn incompatible_animation_conversion_does_not_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("animation.json");
    let path = path.to_str().unwrap();
    let mut project = success(&["example"]);
    project["clips"][0]["tracks"][0]["property"] = json!("radius");
    std::fs::write(path, project.to_string()).unwrap();
    success(&["validate", path]);
    let bytes = std::fs::read(path).unwrap();
    error(
        &["convert-to-path", path, "--node", "ball", "--output", path],
        "validation_error",
    );
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn scaffold_is_source_only_and_never_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plugin");
    let output = path.to_str().unwrap();
    let result = success(&["plugin", "new", "--name", "test-plugin", "--output", output]);
    assert_eq!(result["executed"], false);
    assert_eq!(result["editorOnly"], true);
    let manifest = std::fs::read_to_string(path.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("name = \"test-plugin\""));
    assert!(manifest.contains("[workspace]"));
    assert!(!manifest.contains("{{"));
    let source = std::fs::read_to_string(path.join("src/lib.rs")).unwrap();
    assert!(source.contains("impl EditorPlugin for GeneratedPlugin"));
    assert!(source.contains("projection.add_plugin(GeneratedPlugin)"));
    assert!(source.contains("pub fn register"));
    assert!(!path.join("target").exists());
    assert!(!path.join("Cargo.lock").exists());
    let bytes = std::fs::read(path.join("src/lib.rs")).unwrap();
    error(
        &["plugin", "new", "--name", "test-plugin", "--output", output],
        "write_error",
    );
    assert_eq!(std::fs::read(path.join("src/lib.rs")).unwrap(), bytes);
    for name in ["../escape", "Bad", "a\"\n[dependencies]", ""] {
        error(
            &["plugin", "new", "--name", name, "--output", output],
            "invalid_arguments",
        );
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    let schema = success(&["schema"]);
    assert_eq!(schema["nodeMetadata"]["locked"]["default"], false);
    let rig = dir.path().join("rig.json");
    std::fs::write(&rig, schema["examples"]["rig"].to_string()).unwrap();
    success(&["validate", rig.to_str().unwrap()]);
}
