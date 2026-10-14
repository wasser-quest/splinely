use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};
#[test]
fn real_binary_framing_reload_errors_utf8_and_strict_loading() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_forma-runtime"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = b"{\"type\":\"evaluate\"}\n{\"type\":\"load\",\"project\":{}}\n{\"type\":\"tick\",\"delta\":0.5}\n{\"type\":\"load\",\"project\":{\"name\":\"a\",\"name\":\"b\"}}\n{\"type\":\"load\",\"project\":{\"unknown\":true}}\n\xff\n{\"type\":\"evaluate\"}\n{\"type\":\"tick\",\"delta\":1,\"delta\":2}\n{\"type\":\"load\",\"project\":{}}";
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let results: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(results.len(), 9);
    assert_eq!(results[0]["ok"], false);
    assert_eq!(results[1]["ok"], true);
    assert_eq!(results[2]["result"]["frame"]["time"], 0.5);
    for i in [3, 4, 5, 7] {
        assert_eq!(results[i]["ok"], false);
    }
    assert_eq!(results[6]["result"]["frame"]["time"], 0.5);
    assert_eq!(results[8]["result"]["frame"]["time"], 0.0);
}
