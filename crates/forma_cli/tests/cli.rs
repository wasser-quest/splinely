use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Output, Stdio};
use tempfile::tempdir;

fn invoke(args: &[&str], stdin: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_forma-cli"))
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = stdin {
        if let Err(error) = child.stdin.take().unwrap().write_all(input.as_bytes()) {
            // Argument validation can reject a command before it reads stdin.
            assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
        }
    }
    child.wait_with_output().unwrap()
}
fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert!(output.stdout.ends_with(b"\n"));
    serde_json::from_slice(&output.stdout).unwrap()
}
fn error(output: Output, code: &str) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.ends_with(b"\n"));
    let value: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], code);
    assert!(!value["error"]["message"].as_str().unwrap().is_empty());
}

#[test]
fn schema_examples_validate_and_evaluate() {
    let schema = success(invoke(&["schema"], None));
    for example in ["empty", "demo"] {
        let source = schema["examples"][example].to_string();
        assert_eq!(
            success(invoke(&["validate", "-"], Some(&source)))["ok"],
            true
        );
    }
    let demo = success(invoke(&["example"], None)).to_string();
    let evaluation = success(invoke(&["evaluate", "-", "--time", "0.5"], Some(&demo)));
    assert_eq!(evaluation["nodeCount"], 3);
    assert_eq!(evaluation["nodes"][1]["transform"]["x"], 240.0);
    let summary = success(invoke(&["inspect", "-"], Some(&demo)));
    assert_eq!(summary["nodes"][1]["id"], "ball");
}

#[test]
fn create_patch_export_and_in_place_update() {
    let directory = tempdir().unwrap();
    let project = directory.path().join("project.json");
    let patch = directory.path().join("patch.json");
    let svg = directory.path().join("export.svg");
    let project = project.to_str().unwrap();
    let patch = patch.to_str().unwrap();
    let svg = svg.to_str().unwrap();
    success(invoke(&["new", "--demo", "--output", project], None));
    std::fs::write(patch, r#"{"name":"AI edited","variables":{"purpose":"offline"},"rawSvg":"<g />","customJs":"/* preserved */"}"#).unwrap();
    success(invoke(
        &["apply", project, "--patch", patch, "--output", project],
        None,
    ));
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(project).unwrap()).unwrap();
    assert_eq!(saved["name"], "AI edited");
    assert_eq!(saved["rawSvg"], "<g />");
    assert_eq!(saved["customJs"], "/* preserved */");
    success(invoke(&["export", project, "--output", svg], None));
    let exported = std::fs::read_to_string(svg).unwrap();
    assert!(exported.contains("forma-project"));
    assert!(exported.contains("<script"));
    assert!(exported.contains("AI edited"));
    assert!(exported.contains("requestAnimationFrame"));
    let streamed = success(invoke(&["export", project, "--output", "-"], None));
    assert_eq!(streamed["svg"], exported);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 3);
}

#[test]
fn invalid_patch_is_transactional_and_original_input_is_validated_first() {
    let directory = tempdir().unwrap();
    let project = directory.path().join("project.json");
    let project = project.to_str().unwrap();
    success(invoke(&["new", "--demo", "--output", project], None));
    let original = std::fs::read(project).unwrap();
    error(
        invoke(
            &["apply", project, "--patch", "-", "--output", project],
            Some(r#"{"width":-1}"#),
        ),
        "validation_error",
    );
    assert_eq!(std::fs::read(project).unwrap(), original);
    error(
        invoke(
            &["apply", project, "--patch", "-", "--output", project],
            Some("{"),
        ),
        "invalid_json",
    );
    assert_eq!(std::fs::read(project).unwrap(), original);
    std::fs::write(project, r#"{"schemaVersion":99}"#).unwrap();
    error(
        invoke(
            &["apply", project, "--patch", "-", "--output", project],
            Some(r#"{"schemaVersion":1}"#),
        ),
        "validation_error",
    );
    assert_eq!(
        std::fs::read_to_string(project).unwrap(),
        r#"{"schemaVersion":99}"#
    );
}

#[test]
fn merge_patch_null_deletes_and_arrays_replace() {
    let mut source = success(invoke(&["example"], None));
    source["variables"] = json!({"remove":"existing"});
    let directory = tempdir().unwrap();
    let input = directory.path().join("input.json");
    std::fs::write(&input, source.to_string()).unwrap();
    let patched = success(invoke(
        &[
            "apply",
            input.to_str().unwrap(),
            "--patch",
            "-",
            "--output",
            "-",
        ],
        Some(r#"{"clips":[],"stateMachines":[],"nodes":[],"variables":{"keep":1,"remove":null}}"#),
    ));
    assert_eq!(patched["nodes"], json!([]));
    assert_eq!(patched["clips"], json!([]));
    assert_eq!(patched["variables"], json!({"keep":1}));
}

#[test]
fn invalid_inputs_report_structured_errors() {
    error(invoke(&["validate", "-"], Some("not JSON")), "invalid_json");
    error(
        invoke(&["validate", "-"], Some(r#"{"schemaVersion":99}"#)),
        "validation_error",
    );
    error(
        invoke(&["validate", "-"], Some(r#"{"width":0}"#)),
        "validation_error",
    );
    error(
        invoke(&["validate", "-"], Some(r#"{"newFeature":true}"#)),
        "unknown_field",
    );
    error(
        invoke(&["apply", "-", "--patch", "-", "--output", "-"], Some("{}")),
        "invalid_arguments",
    );
    error(invoke(&["unknown-command"], None), "invalid_arguments");
    error(invoke(&["new"], None), "invalid_arguments");
    let directory = tempdir().unwrap();
    error(
        invoke(
            &[
                "validate",
                directory.path().join("missing").to_str().unwrap(),
            ],
            None,
        ),
        "read_error",
    );
    error(
        invoke(
            &[
                "new",
                "--output",
                directory
                    .path()
                    .join("missing/child.json")
                    .to_str()
                    .unwrap(),
            ],
            None,
        ),
        "write_error",
    );
    error(
        invoke(
            &["new", "--output", directory.path().to_str().unwrap()],
            None,
        ),
        "write_error",
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn rejects_unknown_nested_fields_and_dangling_references() {
    let mut source = success(invoke(&["example"], None));
    source["nodes"][1]["kind"]["unexpected"] = json!(123);
    error(
        invoke(&["validate", "-"], Some(&source.to_string())),
        "unknown_field",
    );
    source["nodes"][1]["kind"]
        .as_object_mut()
        .unwrap()
        .remove("unexpected");
    source["clips"][0]["tracks"][0]["nodeId"] = json!("missing");
    error(
        invoke(&["validate", "-"], Some(&source.to_string())),
        "validation_error",
    );
    error(
        invoke(&["evaluate", "-", "--time", "NaN"], Some("{}")),
        "invalid_time",
    );
}

#[test]
fn rejects_duplicate_members_and_lossy_model_numbers() {
    for source in [
        r#"{"name":"first","name":"second"}"#,
        r#"{"variables":{"key":1,"key":2}}"#,
    ] {
        error(invoke(&["validate", "-"], Some(source)), "invalid_json");
    }
    error(
        invoke(&["validate", "-"], Some(r#"{"width":9007199254740993}"#)),
        "data_loss",
    );
    success(invoke(
        &["validate", "-"],
        Some(r#"{"width":9007199254740992}"#),
    ));
    success(invoke(
        &["validate", "-"],
        Some(r#"{"width":640,"height":360}"#),
    ));
}

#[test]
fn all_invalid_patch_forms_leave_existing_output_untouched() {
    let directory = tempdir().unwrap();
    let project = directory.path().join("project.json");
    let project = project.to_str().unwrap();
    success(invoke(&["new", "--demo", "--output", project], None));
    let original = std::fs::read(project).unwrap();
    for (patch, code) in [
        (r#"{"name":"first","name":"second"}"#, "invalid_json"),
        (r#"{"width":9007199254740993}"#, "data_loss"),
        (r#"{"unknown":true}"#, "unknown_field"),
        (r#"{"nodes":[]}"#, "validation_error"),
        ("null", "invalid_project"),
        ("[]", "invalid_project"),
        ("5", "invalid_project"),
    ] {
        error(
            invoke(
                &["apply", project, "--patch", "-", "--output", project],
                Some(patch),
            ),
            code,
        );
        assert_eq!(std::fs::read(project).unwrap(), original);
    }
    let reset = success(invoke(
        &["apply", project, "--patch", "-", "--output", "-"],
        Some(r#"{"name":null}"#),
    ));
    assert_eq!(reset["name"], "Untitled");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
