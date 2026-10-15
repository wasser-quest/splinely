use serde_json::{json, Value};
use splinely_document::{Project, MAX_PROJECT_FILE_BYTES};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::tempdir;

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
/// Raw JSON value tree of a project file, binary `.sply` or JSON text.
fn project_value(bytes: &[u8]) -> Value {
    if splinely_document::Project::is_binary(bytes) {
        splinely_document::decode_to_value(bytes).unwrap()
    } else {
        serde_json::from_slice(bytes).unwrap()
    }
}
fn saved(path: &Path) -> Value {
    project_value(&std::fs::read(path).unwrap())
}
fn error(args: &[&str], code: &str) {
    let output = invoke(args);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let value: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(value["error"]["code"], code);
}

#[test]
fn authored_payload_survives_json_frm_json_and_edit() {
    let dir = tempdir().unwrap();
    let legacy = dir.path().join("project.splinely.json");
    let sply = dir.path().join("project.sply");
    let json_path = dir.path().join("project.json");
    let svg = dir.path().join("project.svg");
    let legacy_s = legacy.to_str().unwrap();
    let frm_s = sply.to_str().unwrap();
    let json_s = json_path.to_str().unwrap();
    let mut source = success(&["schema"])["examples"]["complete"].clone();
    source["customJs"] = json!("throw new Error('do not execute'); // Grüße\n");
    source["rawSvg"] = json!("<g><script>untrusted()</script></g>");
    std::fs::write(&legacy, source.to_string()).unwrap();
    let canonical: Value =
        serde_json::from_str(&Project::load_file(&legacy).unwrap().to_json().unwrap()).unwrap();
    success(&["convert", legacy_s, "--output", frm_s]);
    assert_eq!(saved(&sply), canonical);
    success(&["convert", frm_s, "--output", json_s]);
    assert_eq!(saved(&json_path), canonical);
    assert_eq!(success(&["convert", frm_s, "--output", "-"]), canonical);
    assert_eq!(success(&["validate", frm_s])["ok"], true);
    assert_eq!(success(&["inspect", frm_s])["hasCustomJs"], true);
    success(&["evaluate", frm_s, "--time", "0.5"]);
    success(&["export", frm_s, "--output", svg.to_str().unwrap()]);
    let exported = std::fs::read_to_string(&svg).unwrap();
    assert!(!exported.contains("new Function('splinely'"));
    let patch = dir.path().join("patch.json");
    std::fs::write(&patch, r#"{"name":"Single file"}"#).unwrap();
    success(&[
        "apply",
        frm_s,
        "--patch",
        patch.to_str().unwrap(),
        "--output",
        frm_s,
    ]);
    let mut expected = canonical;
    expected["name"] = json!("Single file");
    assert_eq!(saved(&sply), expected);
    success(&["layer", "lock", frm_s, "--node", "ball", "--output", frm_s]);
    expected["nodes"][1]["locked"] = json!(true);
    assert_eq!(saved(&sply), expected);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 5);
}

#[test]
fn creation_example_import_and_authoring_use_frm() {
    let dir = tempdir().unwrap();
    let sply = dir.path().join("authored.sply");
    let path = sply.to_str().unwrap();
    success(&["new", "--output", path]);
    assert!(Project::load_file(&sply).unwrap().nodes.is_empty());
    let mut child = Command::new(env!("CARGO_BIN_EXE_splinely-cli"))
        .args(["convert", "-", "--output", path])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"{}").unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
    assert_eq!(Project::load_file(&sply).unwrap().name, "Untitled");
    let example = success(&["example"]);
    success(&["example", "--output", path]);
    assert_eq!(saved(&sply), example);
    success(&["new", "--demo", "--output", path]);
    success(&[
        "keyframe",
        path,
        "--output",
        path,
        "--clip",
        "motion",
        "--target",
        "ball",
        "--property",
        "x",
        "--time",
        "0.5",
        "--value",
        "120",
    ]);
    assert!(!Project::load_file(&sply).unwrap().clips.is_empty());
    success(&["rig-demo", "--output", path]);
    assert!(!Project::load_file(&sply).unwrap().bones.is_empty());
    success(&["water-drop", "--output", path]);
    success(&["random-figures", "--output", path]);
    let svg = dir.path().join("input.svg");
    std::fs::write(&svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><rect x="10" y="10" width="20" height="30"/></svg>"#).unwrap();
    success(&["import-svg", svg.to_str().unwrap(), "--output", path]);
    assert!(!Project::load_file(&sply).unwrap().nodes.is_empty());
}

#[test]
fn strict_conversion_failures_leave_destination_untouched() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("bad.sply");
    let output = dir.path().join("keep.sply");
    let input_s = input.to_str().unwrap();
    let output_s = output.to_str().unwrap();
    std::fs::write(&output, b"sentinel").unwrap();
    for (source, code) in [
        (r#"{"name":"a","name":"b"}"#, "invalid_json"),
        (r#"{"variables":{"a":1,"a":2}}"#, "invalid_json"),
        (r#"{"typo":true}"#, "unknown_field"),
        (r#"{"width":9007199254740993}"#, "data_loss"),
        (r#"{"width":18446744073709551616}"#, "data_loss"),
        (r#"{"schemaVersion":99}"#, "validation_error"),
        (r#"{"width":-1}"#, "validation_error"),
        ("{} trailing", "invalid_json"),
    ] {
        std::fs::write(&input, source).unwrap();
        error(&["convert", input_s, "--output", output_s], code);
        assert_eq!(std::fs::read(&output).unwrap(), b"sentinel");
        error(&["convert", input_s, "--output", input_s], code);
        assert_eq!(std::fs::read_to_string(&input).unwrap(), source);
    }
    std::fs::File::create(&input)
        .unwrap()
        .set_len(MAX_PROJECT_FILE_BYTES + 1)
        .unwrap();
    error(&["convert", input_s, "--output", output_s], "read_error");
    assert_eq!(std::fs::read(&output).unwrap(), b"sentinel");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}
