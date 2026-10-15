use serde_json::{json, Value};
use std::process::{Command, Output};
fn invoke(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_splinely-cli"))
        .args(args)
        .output()
        .unwrap()
}
fn success(args: &[&str]) -> Value {
    let out = invoke(args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn author_inspect_parameter_sample_remove_and_atomic_errors() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("project.json");
    let path = file.to_str().unwrap();
    std::fs::write(
        path,
        include_str!("../../../runtime/tests/fixtures/blend-spaces.json"),
    )
    .unwrap();
    let weights = success(&[
        "blend-space",
        "weights",
        path,
        "--id",
        "locomotion",
        "--x",
        "4",
        "--y",
        "0",
    ]);
    assert_eq!(weights["weights"], json!({"walk":0.2,"run":0.8}));
    assert_eq!(
        success(&["evaluate", path])["nodes"][0]["transform"]["x"],
        42.0
    );
    // Persisted parameter edits preserve every unrelated field after canonicalization.
    let canonical = success(&[
        "blend-space",
        "parameter",
        path,
        "--name",
        "speed",
        "--value",
        "1",
        "--output",
        "-",
    ]);
    std::fs::write(path, serde_json::to_vec(&canonical).unwrap()).unwrap();
    success(&[
        "blend-space",
        "parameter",
        path,
        "--name",
        "speed",
        "--value",
        "0",
        "--output",
        path,
    ]);
    let mut expected = canonical;
    expected["variables"]["speed"] = json!(0.0);
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(path).unwrap()).unwrap(),
        expected
    );
    assert_eq!(
        success(&["evaluate", path])["nodes"][0]["transform"]["x"],
        32.0
    );
    let context = dir.path().join("context.json");
    std::fs::write(&context, r#"{"variables":{"speed":2}}"#).unwrap();
    assert_eq!(
        success(&["evaluate", path, "--context", context.to_str().unwrap()])["nodes"][0]
            ["transform"]["x"],
        52.0
    );
    for args in [
        vec![
            "blend-space",
            "set",
            path,
            "--id",
            "locomotion",
            "--x",
            "speed",
            "--y",
            "turn",
            "--sample",
            "walk:0:0",
            "--sample",
            "run:0:0",
            "--output",
            path,
        ],
        vec![
            "blend-space",
            "parameter",
            path,
            "--name",
            "speed",
            "--value",
            "10000001",
            "--output",
            path,
        ],
        vec![
            "blend-clip",
            path,
            "--clip",
            "walk",
            "--mode",
            "additive",
            "--output",
            path,
        ],
    ] {
        let before = std::fs::read(path).unwrap();
        let out = invoke(&args);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert_eq!(before, std::fs::read(path).unwrap());
    }
    success(&[
        "blend-space",
        "remove",
        path,
        "--id",
        "locomotion",
        "--output",
        path,
    ]);
    assert_eq!(
        success(&["blend-space", "list", path])["blendSpaces"],
        json!([])
    );
    success(&[
        "blend-space",
        "set",
        path,
        "--id",
        "movement",
        "--x",
        "horizontal",
        "--y",
        "vertical",
        "--sample",
        "walk:-1:0",
        "--sample",
        "run:1:0",
        "--output",
        path,
    ]);
    let list = success(&["blend-space", "list", path]);
    assert_eq!(list["weights"], json!({"walk":0.5,"run":0.5}));
    success(&[
        "blend-clip",
        path,
        "--clip",
        "offset",
        "--mode",
        "additive",
        "--weight",
        "0.25",
        "--output",
        path,
    ]);
    assert_eq!(
        success(&["evaluate", path])["nodes"][0]["transform"]["x"],
        41.0
    );
}
