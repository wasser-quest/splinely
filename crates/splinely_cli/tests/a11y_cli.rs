//! `a11y` subcommand and the `interactive_without_label` check warning.
use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

const CLI: &str = env!("CARGO_BIN_EXE_splinely-cli");

fn invoke(dir: &Path, args: &[&str]) -> Output {
    Command::new(CLI)
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}
fn ok(dir: &Path, args: &[&str]) -> Value {
    let out = invoke(dir, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn err(dir: &Path, args: &[&str]) -> Value {
    let out = invoke(dir, args);
    assert!(!out.status.success(), "{args:?} unexpectedly succeeded");
    serde_json::from_slice(&out.stderr).unwrap()
}
fn codes(check: &Value) -> Vec<String> {
    check["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["code"].as_str().unwrap().to_owned())
        .collect()
}
fn json(dir: &Path, file: &str) -> Value {
    serde_json::from_slice(&invoke(dir, &["cat", file]).stdout).unwrap()
}

fn scene(d: &Path) {
    ok(
        d,
        &[
            "new", "--output", "s.sply", "--width", "200", "--height", "100", "--name", "Lamp",
        ],
    );
    ok(
        d,
        &[
            "node", "add", "s.sply", "--kind", "rect", "--id", "switch", "--x", "10", "--y", "10",
            "--width", "40", "--height", "20", "--fill", "#ffffff",
        ],
    );
    ok(
        d,
        &[
            "node", "add", "s.sply", "--kind", "circle", "--id", "deco", "--x", "100", "--y", "50",
            "--radius", "5",
        ],
    );
    ok(
        d,
        &[
            "on",
            "s.sply",
            "--event",
            "click",
            "--target",
            "switch",
            "--machine",
            "lamp",
            "--from",
            "off",
            "--to",
            "on",
        ],
    );
    ok(
        d,
        &[
            "on",
            "s.sply",
            "--event",
            "click",
            "--target",
            "switch",
            "--machine",
            "lamp",
            "--from",
            "on",
            "--to",
            "off",
        ],
    );
}

#[test]
fn a11y_edits_report_and_clear() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    scene(d);
    let report = ok(d, &["a11y", "s.sply"]);
    assert_eq!(report["enabled"], false);
    assert_eq!(
        report["interactiveWithoutLabel"],
        serde_json::json!(["switch"])
    );

    ok(
        d,
        &[
            "a11y",
            "s.sply",
            "--title",
            "Lamp scene",
            "--description",
            "A lamp with a switch.",
        ],
    );
    ok(
        d,
        &[
            "a11y",
            "s.sply",
            "--node",
            "switch",
            "--label",
            "Lamp switch",
            "--description",
            "Turns the lamp on or off",
            "--role",
            "button",
        ],
    );
    ok(d, &["a11y", "s.sply", "--node", "deco", "--hidden", "true"]);
    let done = ok(
        d,
        &[
            "a11y",
            "s.sply",
            "--state-announce",
            "lamp.on=Lamp is on",
            "--state-announce",
            "lamp.off=Lamp is off",
        ],
    );
    assert_eq!(done["a11y"]["announcements"].as_array().unwrap().len(), 2);

    let value = json(d, "s.sply");
    assert_eq!(value["name"], "Lamp scene");
    assert_eq!(value["description"], "A lamp with a switch.");
    assert_eq!(value["nodes"][0]["a11y"]["label"], "Lamp switch");
    assert_eq!(value["nodes"][0]["a11y"]["role"], "button");
    assert_eq!(value["nodes"][1]["a11y"]["hidden"], true);
    let states = &value["stateMachines"][0]["states"];
    assert!(states
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["announce"] == "Lamp is on"));

    let report = ok(d, &["a11y", "s.sply"]);
    assert_eq!(report["enabled"], true);
    assert_eq!(report["interactiveWithoutLabel"], serde_json::json!([]));
    ok(d, &["validate", "s.sply"]);

    // Clearing: empty text and --hidden false remove the data again, an empty a11y object is dropped.
    ok(
        d,
        &["a11y", "s.sply", "--node", "deco", "--hidden", "false"],
    );
    ok(
        d,
        &[
            "a11y",
            "s.sply",
            "--node",
            "switch",
            "--label",
            "",
            "--description",
            "",
            "--role",
            "",
        ],
    );
    ok(
        d,
        &[
            "a11y",
            "s.sply",
            "--description",
            "",
            "--state-announce",
            "lamp.on=",
            "--state-announce",
            "lamp.off=",
        ],
    );
    let text = String::from_utf8(invoke(d, &["cat", "s.sply"]).stdout).unwrap();
    for key in ["description", "a11y", "announce"] {
        assert!(!text.contains(key), "{key} should be gone: {text}");
    }
    assert_eq!(ok(d, &["a11y", "s.sply"])["enabled"], false);
}

#[test]
fn a11y_rejects_bad_input_with_hints() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    scene(d);
    let e = err(d, &["a11y", "s.sply", "--label", "x"]);
    assert_eq!(e["error"]["code"], "invalid_arguments");
    let e = err(
        d,
        &["a11y", "s.sply", "--node", "switch", "--role", "banner"],
    );
    assert!(
        e["error"]["hint"].as_str().unwrap().contains("button"),
        "{e}"
    );
    let e = err(d, &["a11y", "s.sply", "--node", "swich", "--label", "x"]);
    assert!(
        e["error"]["hint"].as_str().unwrap().contains("switch"),
        "{e}"
    );
    let e = err(
        d,
        &["a11y", "s.sply", "--node", "deco", "--label", " padded "],
    );
    assert!(
        e["error"]["message"].as_str().unwrap().contains("label"),
        "{e}"
    );
    // hidden + interactive is refused by validation and nothing is written
    let before = std::fs::read(d.join("s.sply")).unwrap();
    err(
        d,
        &["a11y", "s.sply", "--node", "switch", "--hidden", "true"],
    );
    assert_eq!(std::fs::read(d.join("s.sply")).unwrap(), before);
    err(d, &["a11y", "s.sply", "--state-announce", "lamp.on"]);
    err(d, &["a11y", "s.sply", "--state-announce", "nope.on=x"]);
    err(d, &["a11y", "s.sply", "--state-announce", "lamp.nope=x"]);
}

#[test]
fn check_warns_about_unlabeled_interactive_nodes_and_the_fix_works() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    scene(d);
    let check = ok(d, &["check", "s.sply"]);
    assert!(
        codes(&check).contains(&"interactive_without_label".to_owned()),
        "{check}"
    );
    assert!(!check["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["code"] == "interactive_without_label"
            && f["message"].as_str().unwrap().contains("deco")));
    err(d, &["check", "s.sply", "--strict"]);
    let fix = check["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["code"] == "interactive_without_label")
        .unwrap()["fix"]
        .as_str()
        .unwrap()
        .replace("'LABEL'", "Switch");
    let args: Vec<&str> = fix.split_whitespace().skip(1).collect();
    ok(d, &args);
    assert!(!codes(&ok(d, &["check", "s.sply"])).contains(&"interactive_without_label".to_owned()));
}

#[test]
fn a11y_works_in_batch_and_exports_semantics() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    scene(d);
    std::fs::write(
        d.join("steps.txt"),
        "a11y --description 'Batch description'\na11y --node switch --label Switch\n",
    )
    .unwrap();
    ok(d, &["batch", "s.sply", "--steps", "steps.txt"]);
    ok(d, &["export", "s.sply", "--output", "s.svg"]);
    let svg = std::fs::read_to_string(d.join("s.svg")).unwrap();
    assert!(svg.contains("aria-label=\"Switch\""), "{svg}");
    assert!(svg.contains("<desc"), "{svg}");
    assert!(svg.contains("role=\"group\""));
}
