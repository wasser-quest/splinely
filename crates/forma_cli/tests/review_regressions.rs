use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn rejects_oversized_integer_before_serde_rounds_it() {
    for source in [
        r#"{"width":18446744073709551617}"#,
        r#"{"variables":{"large":18446744073709551617}}"#,
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_forma-cli"))
            .args(["validate", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(!result.status.success());
        let error: serde_json::Value = serde_json::from_slice(&result.stderr).unwrap();
        assert_eq!(error["error"]["code"], "data_loss");
    }
}

#[test]
fn trusted_export_is_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("test.json");
    std::fs::write(
        &project,
        r#"{"customJs":"forma.variable('test').set(true);"}"#,
    )
    .unwrap();
    for trusted in [false, true] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_forma-cli"));
        cmd.args(["export", project.to_str().unwrap(), "--output", "-"]);
        if trusted {
            cmd.arg("--trusted-js");
        }
        let result = cmd.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(
            value["svg"]
                .as_str()
                .unwrap()
                .contains("new Function('forma'"),
            trusted
        );
    }
}
