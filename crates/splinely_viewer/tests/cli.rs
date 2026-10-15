use std::process::Command;

#[test]
fn output_is_atomic_and_rejects_hard_link_input_aliases() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.json");
    let output = dir.path().join("output.html");
    std::fs::write(&input, "{}").unwrap();
    std::fs::hard_link(&input, &output).unwrap();
    let failed = cli()
        .arg(&input)
        .args(["--check", "--output"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("hard-link aliases"));
    assert_eq!(std::fs::read_to_string(&input).unwrap(), "{}");
    std::fs::remove_file(&output).unwrap();
    std::fs::write(&output, "old output").unwrap();
    let other_link = dir.path().join("old-output-alias");
    std::fs::hard_link(&output, &other_link).unwrap();
    assert!(cli()
        .arg(&input)
        .args(["--check", "--output"])
        .arg(&output)
        .output()
        .unwrap()
        .status
        .success());
    assert!(std::fs::read_to_string(&output)
        .unwrap()
        .starts_with("<!doctype html>"));
    assert_eq!(std::fs::read_to_string(&other_link).unwrap(), "old output");
    assert_eq!(std::fs::read_to_string(&input).unwrap(), "{}");
}

#[cfg(target_os = "linux")]
#[test]
fn gui_without_a_display_is_an_actionable_error() {
    let input = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    std::fs::write(input.path(), "{}").unwrap();
    let result = cli()
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .arg(input.path())
        .output()
        .unwrap();
    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("DISPLAY") || stderr.contains("native viewer unavailable"),
        "{stderr}"
    );
}

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_splinely-viewer"))
}

#[test]
fn help_and_version_do_not_need_a_display() {
    for flag in ["--help", "--version"] {
        assert!(cli()
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .arg(flag)
            .output()
            .unwrap()
            .status
            .success());
    }
}

#[test]
fn frm_check_is_headless_and_trust_is_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("authored.sply");
    std::fs::write(&input, r#"{"customJs":"window.FRM_CLI_MARKER=1;"}"#).unwrap();
    for trusted in [false, true] {
        let mut command = cli();
        command
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .arg(&input)
            .arg("--check");
        if trusted {
            command.arg("--trusted-js");
        }
        let result = command.output().unwrap();
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(result.status.success(), "{stderr}");
        assert_eq!(stderr.contains("customJs disabled"), !trusted);
        assert!(!stderr.contains("SVG is active content"));
        assert!(String::from_utf8_lossy(&result.stdout).contains("no scripts executed"));
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn check_needs_no_display_and_reports_errors() {
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/native-smoke.splinely.json");
    let result = cli()
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .arg(&project)
        .arg("--check")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("no scripts executed"));
    assert!(!cli()
        .args(["/nonexistent/splinely-viewer.svg", "--check"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(!cli()
        .arg(&project)
        .arg("--check")
        .arg("--output")
        .arg(&project)
        .output()
        .unwrap()
        .status
        .success());
}
