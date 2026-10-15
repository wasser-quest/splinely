use serde_json::{json, Value};
use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_splinely-cli"))
        .args(args)
        .output()
        .unwrap()
}
/// Raw JSON value tree of a project file, binary `.sply` or JSON text.
fn project_value(bytes: &[u8]) -> Value {
    if splinely_document::Project::is_binary(bytes) {
        splinely_document::decode_to_value(bytes).unwrap()
    } else {
        serde_json::from_slice(bytes).unwrap()
    }
}
fn success(args: &[&str]) -> Value {
    let out = run(args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn bind_keyframe_evaluate_export_remove_and_invalid_edits_are_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("motion.sply");
    let path = path.to_str().unwrap();
    std::fs::write(path, serde_json::to_vec(&json!({
        "nodes":[{"id":"route","visible":false,"kind":{"type":"path","d":"M0 0 L100 0 L100 100","bounds":[0,0,100,100]}},
                 {"id":"ball","kind":{"type":"circle","cx":0,"cy":0,"radius":5}}],
        "variables":{"preserved":true},"customJs":"/* inert */"
    })).unwrap()).unwrap();
    success(&[
        "motion-path",
        path,
        "--target",
        "ball",
        "--path",
        "route",
        "--orient",
    ]);
    let before = std::fs::read(path).unwrap();
    for flags in [
        vec!["--path", "ball"],
        vec!["--path", "missing"],
        vec!["--path", "route", "--progress", "1.01"],
        vec!["--path", "route", "--progress", "NaN"],
    ] {
        let mut args = vec!["motion-path", path, "--target", "ball"];
        args.extend(flags);
        assert!(!run(&args).status.success());
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
    success(&[
        "keyframe",
        path,
        "--clip",
        "travel",
        "--target",
        "ball",
        "--property",
        "motionProgress",
        "--time",
        "0",
        "--value",
        "0",
    ]);
    success(&[
        "keyframe",
        path,
        "--clip",
        "travel",
        "--target",
        "ball",
        "--property",
        "motionProgress",
        "--time",
        "2",
        "--value",
        "1",
        "--autoplay",
    ]);
    let evaluated = success(&["evaluate", path, "--time", "1.5"]);
    let ball = evaluated["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "ball")
        .unwrap();
    for (property, expected) in [("x", 100.), ("y", 50.), ("rotation", 90.)] {
        let actual = ball["transform"][property].as_f64().unwrap();
        assert!((actual - expected).abs() < 1e-8, "{property}: {actual}");
    }
    let svg = dir.path().join("motion.svg");
    success(&[
        "export",
        path,
        "--output",
        svg.to_str().unwrap(),
        "--embed",
        "json",
    ]);
    let text = std::fs::read_to_string(svg).unwrap();
    assert!(text.contains("motionProgress") && text.contains("motionPath"));
    let before = std::fs::read(path).unwrap();
    assert!(!run(&["motion-path", path, "--target", "ball", "--remove"])
        .status
        .success());
    assert_eq!(std::fs::read(path).unwrap(), before);
    let mut data: Value = project_value(&before);
    data["clips"] = json!([]);
    std::fs::write(path, serde_json::to_vec(&data).unwrap()).unwrap();
    success(&["motion-path", path, "--target", "ball", "--remove"]);
    let data: Value = project_value(&std::fs::read(path).unwrap());
    assert!(data["nodes"][1].get("motionPath").is_none());
    assert_eq!(data["variables"]["preserved"], true);
    assert_eq!(data["customJs"], "/* inert */");
    let schema = success(&["schema"]);
    assert_eq!(schema["model"]["motionPath"]["nodeField"]["progress"], 0);
}
