//! `import-svg`: structured import report, `--strict`, `--report`, refused imports.
use serde_json::Value;
use splinely_document::{Project, SourceSync};
use std::io::Write;
use std::process::{Command, Output, Stdio};
use tempfile::tempdir;

fn run(args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_splinely-cli"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdin.take().unwrap().flush());
    child.wait_with_output().unwrap()
}
fn fixture(name: &str) -> String {
    format!(
        "{}/../splinely_document/tests/fixtures/svg/{name}.svg",
        env!("CARGO_MANIFEST_DIR")
    )
}
fn codes(report: &Value, status: &str) -> Vec<String> {
    report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["status"] == status)
        .map(|d| d["code"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn import_prints_report_and_saves_project_with_baseline() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("figma.sply");
    let o = run(&[
        "import-svg",
        &fixture("figma_gradient_text"),
        "--output",
        out.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["ok"], true);
    let report = &v["report"];
    assert_eq!(report["rejected"], 0);
    assert!(report["imported"].as_u64().unwrap() >= 4);
    assert!(codes(report, "approximated").contains(&"text-anchor".to_owned()));
    assert!(codes(report, "dropped").contains(&"text-font-family".to_owned()));
    let p = Project::load_file(&out).unwrap();
    assert_eq!(p.nodes.len(), 4);
    assert_eq!(p.source_sync(), SourceSync::InSync);
    // inspect shows the source/visual relation
    let o = run(&["inspect", out.to_str().unwrap()]);
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["svgSourceSync"], "inSync");
}

#[test]
fn strict_refuses_lossy_imports_and_writes_nothing() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("strict.sply");
    let o = run(&[
        "import-svg",
        &fixture("inkscape"),
        "--output",
        out.to_str().unwrap(),
        "--strict",
    ]);
    assert_eq!(o.status.code(), Some(1));
    let v: Value = serde_json::from_slice(&o.stderr).unwrap();
    assert_eq!(v["error"]["code"], "svg_import_lossy");
    assert!(!codes(&v["error"]["importReport"], "dropped").is_empty());
    assert!(!out.exists());
    // a lossless file passes --strict
    let plain = dir.path().join("plain.svg");
    std::fs::write(&plain, r#"<svg xmlns="http://www.w3.org/2000/svg" width="9" height="9"><rect width="9" height="9"/></svg>"#).unwrap();
    let o = run(&[
        "import-svg",
        plain.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--strict",
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(out.exists());
}

#[test]
fn refused_import_carries_rejecting_diagnostics() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("none.sply");
    let bad = dir.path().join("ext.svg");
    std::fs::write(&bad, r#"<svg xmlns="http://www.w3.org/2000/svg" width="9" height="9"><image href="https://example.test/a.png" width="9" height="9"/></svg>"#).unwrap();
    for (input, code) in [
        (bad.to_str().unwrap().to_owned(), "image-external"),
        (fixture("malformed"), "svg-malformed"),
    ] {
        let o = run(&["import-svg", &input, "--output", out.to_str().unwrap()]);
        assert_eq!(o.status.code(), Some(1));
        let v: Value = serde_json::from_slice(&o.stderr).unwrap();
        assert_eq!(v["error"]["code"], "unsupported_svg");
        assert!(
            codes(&v["error"]["importReport"], "rejected").contains(&code.to_owned()),
            "{v}"
        );
        assert!(!out.exists());
    }
}

#[test]
fn report_file_and_stdout_output_keep_stdout_a_project() {
    let dir = tempdir().unwrap();
    let report = dir.path().join("report.json");
    let o = run(&[
        "import-svg",
        &fixture("data_uri_image"),
        "--output",
        "-",
        "--report",
        report.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let project: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(project["nodes"].is_array() && project.get("report").is_none());
    assert!(project["svgImport"]["sourceHash"]
        .as_str()
        .unwrap()
        .starts_with("fnv1a64:"));
    let r: Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
    assert!(codes(&r, "dropped").contains(&"image-raster".to_owned()));
    // without --report the report goes to stderr instead of being lost
    let o = run(&["import-svg", &fixture("data_uri_image"), "--output", "-"]);
    let err: Value = serde_json::from_slice(&o.stderr).unwrap();
    assert!(err["importReport"]["dropped"].as_u64().unwrap() >= 1);
}
