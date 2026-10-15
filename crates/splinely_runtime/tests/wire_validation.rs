use splinely_document::Project;
use splinely_runtime::{ffi, Runtime};
use serde_json::Value;
use std::ffi::CStr;
use std::io::Write;
use std::process::{Command, Stdio};

const BAD: &[&str] = &[
    r#"{"type":"event","event":{"name":"go","targte":"box"}}"#,
    r#"{"type":"action","action":{"type":"emit","event":{"name":"go"},"extra":1}}"#,
    r#"{"type":"action","action":{"type":"setVariable","name":"n","value":18446744073709551617}}"#,
    r#"{"type":"schedule","id":"x","time":1,"data":{"a":1,"a":2}}"#,
    r#"{"type":"tick","delta":9007199254740993}"#,
];

#[test]
fn rust_requests_reject_loss_and_unknown_nested_fields_atomically() {
    let mut runtime = Runtime::new(Project::default()).unwrap();
    for request in BAD {
        assert!(runtime.request_json(request).is_err(), "{request}");
        assert_eq!(runtime.evaluate().unwrap().time, 0.);
    }
    assert!(runtime
        .request_json(r#"{"type":"schedule","id":"x","time":1,"data":{"arbitrary":{"keys":true}}}"#)
        .is_ok());
}

unsafe fn take(ptr: *mut std::ffi::c_char) -> Value {
    let result = serde_json::from_str(CStr::from_ptr(ptr).to_str().unwrap()).unwrap();
    ffi::splinely_runtime_result_free(ptr);
    result
}

#[test]
fn c_abi_uses_the_same_strict_request_loader() {
    let project = Project::default().to_json().unwrap();
    unsafe {
        let mut handle = std::ptr::null_mut();
        assert_eq!(
            take(ffi::splinely_runtime_create(
                project.as_ptr(),
                project.len(),
                &mut handle
            ))["ok"],
            true
        );
        for request in BAD {
            assert_eq!(
                take(ffi::splinely_runtime_request(
                    handle,
                    request.as_ptr(),
                    request.len()
                ))["ok"],
                false,
                "{request}"
            );
        }
        ffi::splinely_runtime_destroy(handle);
    }
}

#[test]
fn json_lines_uses_the_same_strict_request_loader() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_splinely-runtime"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        "{}",
        serde_json::json!({"type":"load","project":Project::default()})
    )
    .unwrap();
    for request in BAD {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), BAD.len() + 1);
    assert_eq!(responses[0]["ok"], true);
    for response in &responses[1..] {
        assert_eq!(response["ok"], false);
    }
}
