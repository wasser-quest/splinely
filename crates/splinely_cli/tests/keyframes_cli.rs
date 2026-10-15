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
    serde_json::from_slice(&output.stdout).unwrap()
}

fn read(path: &str) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn edit(
    input: &str,
    output: &str,
    clip: &str,
    target: &str,
    property: &str,
    time: &str,
    value: &str,
) -> Output {
    invoke(&[
        "keyframe",
        input,
        "--output",
        output,
        "--clip",
        clip,
        "--target",
        target,
        "--property",
        property,
        "--time",
        time,
        "--value",
        value,
    ])
}

fn fixture(path: &str) -> Value {
    let mut project = success(&["example"]);
    project["bones"] =
        json!([{"id":"joint","parentId":null,"x":0.0,"y":0.0,"rotation":0.0,"length":10.0}]);
    project["audioAssets"] =
        json!([{"id":"voice","name":"Voice","mimeType":"audio/wav","dataBase64":"AA=="}]);
    project["audioCues"] = json!([{"id":"cue","assetId":"voice","time":0.0,"offset":0.0,"duration":null,"volume":1.0}]);
    project["variables"] = json!({"enabled":true,"counter":42});
    project["customJs"] = json!("/* preserve */");
    project["rawSvg"] = json!("<g />");
    project["nodes"][1]["locked"] = json!(true);
    project["clips"][0]["tracks"][0]["keyframes"][1]["easing"] = json!({"type":"step"});
    std::fs::write(path, serde_json::to_vec_pretty(&project).unwrap()).unwrap();
    success(&["validate", path]);
    project
}

#[test]
fn inserts_replaces_and_preserves_existing_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.json");
    let path = path.to_str().unwrap();
    let before = fixture(path);
    for (target, property, time, value) in [
        ("joint", "rotation", "1.5", "-30"),
        ("joint", "rotation", "0.5", "10"),
        ("joint", "rotation", "1.5", "70"),
        ("ball", "x", "1", "123"),
        ("ball", "x", "0.25", "15"),
    ] {
        let result = edit(path, path, "travel", target, property, time, value);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let after = read(path);
    // A new track starts with a key at 0 s holding the bone's current rotation.
    let keys = &after["clips"][0]["tracks"][1]["keyframes"];
    assert_eq!(keys.as_array().unwrap().len(), 3);
    assert_eq!(keys[0]["time"], 0.0);
    assert_eq!(keys[0]["value"]["value"], 0.0);
    assert_eq!(keys[1]["time"], 0.5);
    assert_eq!(keys[2]["time"], 1.5);
    assert_eq!(keys[2]["value"]["value"], 70.0);
    assert_eq!(keys[1]["easing"], json!({"type":"linear"}));
    let node_keys = &after["clips"][0]["tracks"][0]["keyframes"];
    assert_eq!(node_keys.as_array().unwrap().len(), 4);
    assert_eq!(node_keys[2]["value"]["value"], 123.0);
    assert_eq!(node_keys[2]["easing"], json!({"type":"step"}));
    let mut expected = before.clone();
    expected["clips"] = after["clips"].clone();
    assert_eq!(after, expected);
    for field in ["id", "name", "duration", "looping", "autoplay"] {
        assert_eq!(after["clips"][0][field], before["clips"][0][field]);
    }
    success(&["validate", path]);
}

#[test]
fn creates_inactive_clips_extends_duration_and_treats_signed_zero_as_one_time() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.json");
    let output = dir.path().join("output.json");
    let input = input.to_str().unwrap();
    let output = output.to_str().unwrap();
    let before = fixture(input);
    assert!(edit(input, output, "pose", "joint", "x", "0", "-2")
        .status
        .success());
    let created = read(output);
    assert_eq!(read(input), before);
    assert_eq!(created["clips"][1]["duration"], 0.001);
    assert_eq!(created["clips"][1]["autoplay"], false);
    assert_eq!(created["clips"][1]["looping"], false);
    assert_eq!(created["clips"][0], before["clips"][0]);
    assert!(edit(output, output, "pose", "joint", "x", "-0", "3")
        .status
        .success());
    assert_eq!(
        read(output)["clips"][1]["tracks"][0]["keyframes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(edit(output, output, "pose", "joint", "y", "3", "4")
        .status
        .success());
    assert_eq!(read(output)["clips"][1]["duration"], 3.0);
    assert!(edit(output, output, "travel", "ball", "x", "4", "5")
        .status
        .success());
    assert_eq!(read(output)["clips"][0]["duration"], 4.0);
    success(&["validate", output]);
}

#[test]
fn invalid_arguments_and_input_never_overwrite_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.json");
    let output = dir.path().join("output.json");
    let path = path.to_str().unwrap();
    let output = output.to_str().unwrap();
    fixture(path);
    let original = std::fs::read(path).unwrap();
    std::fs::write(output, b"destination sentinel").unwrap();
    for (clip, target, property, time, value) in [
        ("travel", "missing", "x", "0", "1"),
        ("ball", "joint", "x", "0", "1"),
        ("bad id", "joint", "x", "0", "1"),
        ("travel", "joint", "scaleX", "0", "1"),
        ("travel", "ball", "width", "0", "1"),
        ("travel", "background", "radius", "0", "1"),
        ("travel", "ball", "fontSize", "0", "1"),
        ("travel", "ball", "opacity", "0", "2"),
        ("travel", "ball", "radius", "0", "-1"),
        ("travel", "ball", "x", "-1", "1"),
        ("travel", "ball", "fill", "0", "1"),
        ("travel", "ball", "nonsense", "0", "1"),
        ("travel", "ball", "x", "NaN", "1"),
        ("travel", "ball", "x", "0", "inf"),
        ("travel", "ball", "x", "0", "9007199254740993"),
    ] {
        for destination in [path, output] {
            let result = edit(path, destination, clip, target, property, time, value);
            assert!(
                !result.status.success(),
                "unexpected success: {clip}/{target}/{property}/{time}/{value}"
            );
            assert!(result.stdout.is_empty());
            assert_eq!(std::fs::read(path).unwrap(), original);
            assert_eq!(std::fs::read(output).unwrap(), b"destination sentinel");
        }
    }
    for invalid in [
        "{",
        "{\"nodes\":[],\"unknown\":1}",
        "{\"nodes\":[],\"nodes\":[]}",
    ] {
        std::fs::write(path, invalid).unwrap();
        for destination in [path, output] {
            assert!(!edit(path, destination, "pose", "joint", "x", "0", "1")
                .status
                .success());
            assert_eq!(std::fs::read(path).unwrap(), invalid.as_bytes());
            assert_eq!(std::fs::read(output).unwrap(), b"destination sentinel");
        }
    }
}

#[test]
fn help_and_schema_describe_numeric_editing() {
    let help = invoke(&["keyframe", "--help"]);
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout).unwrap();
    for flag in [
        "--target",
        "--clip",
        "--property",
        "--time",
        "--value",
        "--output",
    ] {
        assert!(text.contains(flag));
    }
    assert!(text.contains("fontSize"));
    assert!(!invoke(&["embed-speech", "--help"]).status.success());
    assert_eq!(
        success(&["schema"])["keyframeCommand"]["numericProperties"]
            .as_array()
            .unwrap()
            .len(),
        10
    );
}
