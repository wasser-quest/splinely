//! `plugin list|validate|apply` and `export --plugin` (trust gate, hash pinning, round trips).
use serde_json::{json, Value};
use splinely_document::{Node, Project};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;

fn invoke(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_splinely-cli"))
        .args(args)
        .output()
        .unwrap()
}
fn ok(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn fails(output: Output, code: &str) -> Value {
    assert_eq!(output.status.code(), Some(1), "{:?}", output.status);
    assert!(output.stdout.is_empty());
    let value: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(value["error"]["code"], code, "{value}");
    value
}
fn example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/hover-wiggle")
}
fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}
fn project(dir: &Path) -> PathBuf {
    let mut p = Project::default();
    p.nodes.push(Node::rect("box", 0., 0., 40., 40.));
    let path = dir.join("demo.splinely.json");
    std::fs::write(&path, p.to_json().unwrap()).unwrap();
    path
}

#[test]
fn validate_and_list_report_both_parts_without_executing() {
    let v = ok(invoke(&["plugin", "validate", s(&example())]));
    assert_eq!(v["executed"], false);
    assert_eq!(v["plugin"]["id"], "hover-wiggle");
    assert_eq!(v["plugin"]["tools"][0]["id"], "wiggle");
    assert_eq!(v["plugin"]["tools"][0]["needsSelection"], true);
    let runtime = &v["plugin"]["runtime"];
    assert_eq!(runtime["entry"], "runtime.js");
    assert!(runtime["sha256"].as_str().unwrap().starts_with("sha256:"));
    assert!(runtime["exportedOnlyWith"]
        .as_str()
        .unwrap()
        .contains("--trusted-js"));
    let parent = example().parent().unwrap().to_owned();
    let listed = ok(invoke(&["plugin", "list", "--dir", s(&parent)]));
    let ids: Vec<_> = listed["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert!(
        ids.contains(&"hover-wiggle") && ids.contains(&"basic-shapes"),
        "{ids:?}"
    );
    assert!(listed["diagnostics"].as_array().unwrap().is_empty());
}

#[test]
fn validate_rejects_a_hostile_plugin() {
    let dir = tempdir().unwrap();
    let plugin = dir.path().join("evil");
    std::fs::create_dir(&plugin).unwrap();
    std::fs::write(
        plugin.join("splinely-plugin.json"),
        json!({"id":"evil","title":"E","runtime":{"apiVersion":1,"entry":"runtime.js"}})
            .to_string(),
    )
    .unwrap();
    std::fs::write(plugin.join("runtime.js"), "fetch('https://evil.example')").unwrap();
    let e = fails(
        invoke(&["plugin", "validate", s(&plugin)]),
        "invalid_plugin",
    );
    assert!(e["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not allowed"));
    fails(
        invoke(&["plugin", "validate", s(&dir.path().join("missing"))]),
        "invalid_plugin",
    );
}

#[test]
fn apply_edits_validates_and_round_trips() {
    let dir = tempdir().unwrap();
    let file = project(dir.path());
    let before = std::fs::read(&file).unwrap();
    // A tool that needs a node refuses without one and leaves the file untouched.
    fails(
        invoke(&[
            "plugin",
            "apply",
            s(&example()),
            "wiggle",
            "--project",
            s(&file),
        ]),
        "invalid_arguments",
    );
    fails(
        invoke(&[
            "plugin",
            "apply",
            s(&example()),
            "nope",
            "--project",
            s(&file),
            "--node",
            "box",
        ]),
        "unknown_tool",
    );
    fails(
        invoke(&[
            "plugin",
            "apply",
            s(&example()),
            "wiggle",
            "--project",
            s(&file),
            "--node",
            "ghost",
        ]),
        "unknown_node",
    );
    assert_eq!(std::fs::read(&file).unwrap(), before);
    // Success: output to a separate file, original untouched.
    let out = dir.path().join("out.splinely.json");
    let r = ok(invoke(&[
        "plugin",
        "apply",
        s(&example()),
        "wiggle",
        "--project",
        s(&file),
        "--node",
        "box",
        "--output",
        s(&out),
    ]));
    assert_eq!(r["added"]["clips"].as_array().unwrap().len(), 1);
    assert_eq!(r["added"]["stateMachines"].as_array().unwrap().len(), 1);
    assert_eq!(
        r["added"]["variables"],
        json!(["wiggleCount", "wiggleLast"])
    );
    assert_eq!(r["executed"], false);
    assert_eq!(std::fs::read(&file).unwrap(), before);
    // In place (default --output), twice: ids stay unique, the project still validates.
    for tool in ["wiggle", "bounce"] {
        ok(invoke(&[
            "plugin",
            "apply",
            s(&example()),
            tool,
            "--project",
            s(&out),
            "--node",
            "box",
        ]));
    }
    ok(invoke(&["validate", s(&out)]));
    let result = Project::from_json(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(result.clips.len(), 3);
    assert_eq!(result.state_machines.len(), 3);
    result.validate().unwrap();
}

#[test]
fn export_embeds_plugins_only_with_the_trust_flag() {
    let dir = tempdir().unwrap();
    let file = project(dir.path());
    ok(invoke(&[
        "plugin",
        "apply",
        s(&example()),
        "bounce",
        "--project",
        s(&file),
        "--node",
        "box",
    ]));
    let svg = dir.path().join("trusted.svg");
    // No trust flag: explicit refusal, nothing written.
    let e = fails(
        invoke(&[
            "export",
            s(&file),
            "--output",
            s(&svg),
            "--plugin",
            s(&example()),
        ]),
        "trust_required",
    );
    assert!(e["error"]["hint"].as_str().unwrap().len() > 10);
    assert!(!svg.exists());
    // Trusted: embedded, hash-pinned, still loadable.
    let r = ok(invoke(&[
        "export",
        s(&file),
        "--output",
        s(&svg),
        "--plugin",
        s(&example()),
        "--trusted-js",
    ]));
    assert_eq!(r["ok"], true);
    let text = std::fs::read_to_string(&svg).unwrap();
    let sha = ok(invoke(&["plugin", "validate", s(&example())]))["plugin"]["runtime"]["sha256"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(text.contains("<metadata id=\"splinely-plugins\""));
    assert!(text.contains(&sha));
    assert!(text.contains("splinely.use(plugin"));
    assert_eq!(
        Project::from_svg_metadata(&text).unwrap(),
        Project::from_json(&std::fs::read_to_string(&file).unwrap()).unwrap()
    );
    // Without --plugin the same trusted export carries no plugin code.
    let plain = dir.path().join("plain.svg");
    ok(invoke(&[
        "export",
        s(&file),
        "--output",
        s(&plain),
        "--trusted-js",
    ]));
    let plain = std::fs::read_to_string(&plain).unwrap();
    assert!(!plain.contains("splinely-plugins") && !plain.contains("splinely.use(plugin"));
    // A runtime-less plugin cannot be embedded; a raw .js file can.
    let editor_only = example().parent().unwrap().join("colors");
    fails(
        invoke(&[
            "export",
            s(&file),
            "--output",
            s(&svg),
            "--plugin",
            s(&editor_only),
            "--trusted-js",
        ]),
        "invalid_plugin",
    );
    let js = dir.path().join("mine.js");
    std::fs::write(&js, "return {id:'mine', apiVersion:1};").unwrap();
    ok(invoke(&[
        "export",
        s(&file),
        "--output",
        s(&svg),
        "--plugin",
        s(&js),
        "--trusted-js",
    ]));
    assert!(std::fs::read_to_string(&svg)
        .unwrap()
        .contains("\"id\":\"mine\""));
}

#[test]
fn isolated_preview_lists_plugins_and_needs_no_extra_flag() {
    let dir = tempdir().unwrap();
    let file = project(dir.path());
    let html = dir.path().join("p.html");
    let r = ok(invoke(&[
        "isolated-preview",
        s(&file),
        "--output",
        s(&html),
        "--plugin",
        s(&example()),
    ]));
    assert_eq!(r["plugins"][0]["id"], "hover-wiggle");
    let text = std::fs::read_to_string(&html).unwrap();
    assert!(text.contains("Runtime plugins embedded"));
    assert!(text.contains("allow-scripts"));
}

#[test]
fn capabilities_and_guide_mention_plugins() {
    let caps = ok(invoke(&["capabilities"])).to_string();
    assert!(
        caps.contains("runtime-plugins"),
        "capabilities lacks runtime-plugins"
    );
    let guide = String::from_utf8(invoke(&["guide"]).stdout).unwrap();
    assert!(guide.contains("plugin apply"), "guide lacks plugin apply");
}
