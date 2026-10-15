use std::process::Command;
use tempfile::tempdir;

#[test]
fn isolated_preview_exports_html_with_browser_consent_not_svg_trust() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("project.sply");
    splinely_document::Project {
        custom_js: "parent.pwned=true".into(),
        ..Default::default()
    }
    .save_file(&input)
    .unwrap();
    let output = dir.path().join("preview.html");
    let result = Command::new(env!("CARGO_BIN_EXE_splinely-cli"))
        .args([
            "isolated-preview",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["format"], "html");
    assert_eq!(value["customJs"], "browser-consent-required");
    let html = std::fs::read_to_string(output).unwrap();
    assert!(html.contains("Content-Security-Policy"));
    assert!(html.contains("id=\"consent\""));
    assert!(!html.contains("parent.pwned"));
    let stdout = Command::new(env!("CARGO_BIN_EXE_splinely-cli"))
        .args(["isolated-preview", input.to_str().unwrap(), "--output", "-"])
        .output()
        .unwrap();
    assert!(stdout.status.success());
    let value: serde_json::Value = serde_json::from_slice(&stdout.stdout).unwrap();
    assert_eq!(value["html"], html);
}
