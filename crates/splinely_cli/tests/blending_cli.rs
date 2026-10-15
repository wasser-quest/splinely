use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn invoke(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_splinely-cli"))
        .args(args)
        .output()
        .unwrap()
}
fn success(args: &[&str]) -> Value {
    let out = invoke(args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn write(path: &str, value: &Value) {
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}
fn read(path: &str) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn fixture() -> Value {
    json!({"nodes":[{"id":"ball","kind":{"type":"circle","cx":0,"cy":0,"radius":10},"transform":{"x":10},"locked":true}],
        "clips":[{"id":"walk","duration":2,"autoplay":true,"blendMode":"weighted","weight":0.5,"tracks":[{"nodeId":"ball","property":"x","keyframes":[{"time":0,"value":{"type":"number","value":20}},{"time":2,"value":{"type":"number","value":100}}]}]},
        {"id":"jump","duration":2,"blendMode":"weighted","weight":0.5,"tracks":[{"nodeId":"ball","property":"x","keyframes":[{"time":0,"value":{"type":"number","value":200}}]}]}],
        "stateMachines":[{"id":"machine","initial":"idle","states":[{"id":"idle"},{"id":"active","overrides":[{"nodeId":"ball","property":"x","value":{"type":"number","value":30}}]}]}],
        "customJs":"/* inert */","rawSvg":"<g/>","variables":{"enabled":true}})
}

#[test]
fn blend_edits_are_validated_atomic_and_preserve_unrelated_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.json");
    let path = path.to_str().unwrap();
    write(path, &fixture());
    // Canonicalize defaults first.
    let canonical = success(&[
        "blend-clip",
        path,
        "--clip",
        "walk",
        "--weight",
        "0.5",
        "--output",
        "-",
    ]);
    write(path, &canonical);
    success(&[
        "blend-clip",
        path,
        "--clip",
        "jump",
        "--weight",
        "0.25",
        "--output",
        path,
    ]);
    let mut expected = canonical;
    expected["clips"][1]["weight"] = json!(0.25);
    assert_eq!(read(path), expected);
    assert_eq!(expected["clips"][1]["autoplay"], false);
    for (id, weight) in [
        ("jump", "-1"),
        ("jump", "1.01"),
        ("jump", "NaN"),
        ("jump", "inf"),
        ("missing", "0.5"),
    ] {
        let before = std::fs::read(path).unwrap();
        let out = invoke(&[
            "blend-clip",
            path,
            "--clip",
            id,
            "--weight",
            weight,
            "--output",
            path,
        ]);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
    expected["stateMachines"][0]["states"][0]["onEnter"] =
        json!([{"type":"blendClip","clipId":"jump","weight":0,"duration":0.25}]);
    write(path, &expected);
    let before = std::fs::read(path).unwrap();
    let out = invoke(&[
        "blend-clip",
        path,
        "--clip",
        "jump",
        "--mode",
        "override",
        "--output",
        path,
    ]);
    assert!(!out.status.success());
    assert_eq!(std::fs::read(path).unwrap(), before);
    success(&[
        "blend-clip",
        path,
        "--clip",
        "walk",
        "--mode",
        "override",
        "--output",
        path,
    ]);
    assert_eq!(read(path)["clips"][0]["blendMode"], "override");
}

#[test]
fn context_samples_independent_clocks_weights_states_and_final_overrides() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.json");
    let path = path.to_str().unwrap();
    let ctx = dir.path().join("context.json");
    let ctx = ctx.to_str().unwrap();
    write(path, &fixture());
    let before = std::fs::read(path).unwrap();
    assert_eq!(
        success(&["evaluate", path, "--time", "1"])["nodes"][0]["transform"]["x"],
        35.0
    );
    write(
        ctx,
        &json!({"states":{"machine":"active"},"clips":[{"clipId":"walk","time":1},{"clipId":"jump","time":0}],"clipWeights":{"walk":0.25,"jump":0.25}}),
    );
    // Baseline 30*0.5 + walk(1)=60*0.25 + jump(0)=200*0.25.
    assert_eq!(
        success(&["evaluate", path, "--time", "99", "--context", ctx])["nodes"][0]["transform"]
            ["x"],
        80.0
    );
    write(
        ctx,
        &json!({"clips":[{"clipId":"walk","time":1},{"clipId":"jump","time":0}],"clipWeights":{"walk":1,"jump":1}}),
    );
    assert_eq!(
        success(&["evaluate", path, "--context", ctx])["nodes"][0]["transform"]["x"],
        130.0
    );
    write(ctx, &json!({"clips":[],"states":{"machine":"active"}}));
    assert_eq!(
        success(&["evaluate", path, "--context", ctx])["nodes"][0]["transform"]["x"],
        30.0
    );
    write(
        ctx,
        &json!({"clips":[],"overrides":[{"nodeId":"ball","property":"x","value":{"type":"number","value":7}}]}),
    );
    assert_eq!(
        success(&["evaluate", path, "--context", ctx])["nodes"][0]["transform"]["x"],
        7.0
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn context_rejects_invalid_and_lossy_data_including_inactive_weights() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.json");
    let path = path.to_str().unwrap();
    let ctx = dir.path().join("context.json");
    let ctx = ctx.to_str().unwrap();
    write(path, &fixture());
    for value in [
        json!({"clips":[],"clipWeights":{"missing":0}}),
        json!({"clips":[],"clipWeights":{"jump":2}}),
        json!({"states":{"machine":"missing"}}),
        json!({"clips":[{"clipId":"missing","time":0}]}),
        json!({"clips":[{"clipId":"jump","time":-1}]}),
        json!({"clips":[{"clipId":"jump","time":0,"extra":1}]}),
        json!({"clips":[{"clipId":"jump","time":9007199254740993_u64}]}),
        json!({"overrides":[{"nodeId":"ball","property":"x","value":{"type":"number","value":1},"extra":1}]}),
        json!({"extra":1}),
        json!([]),
    ] {
        write(ctx, &value);
        let out = invoke(&["evaluate", path, "--context", ctx]);
        assert!(!out.status.success(), "unexpected success: {value}");
        assert!(out.stdout.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stderr).unwrap()["ok"],
            false
        );
    }
    std::fs::write(ctx, br#"{"clipWeights":{"walk":0,"walk":1}}"#).unwrap();
    assert!(!invoke(&["evaluate", path, "--context", ctx])
        .status
        .success());
    assert!(!invoke(&["evaluate", "-", "--context", "-"])
        .status
        .success());
    for time in [
        "9007199254740993",
        "18446744073709551616",
        "NaN",
        "inf",
        "-1",
    ] {
        assert!(!invoke(&["evaluate", path, "--time", time]).status.success());
    }
    success(&["evaluate", path, "--time", "9007199254740992"]);
}

#[test]
fn context_supports_stdin_and_empty_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.json");
    let path = path.to_str().unwrap();
    write(path, &fixture());
    let mut child = Command::new(env!("CARGO_BIN_EXE_splinely-cli"))
        .args(["evaluate", path, "--context", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"{}").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        success(&["evaluate", path])
    );
}

#[test]
fn schema_templates_and_discovery_cover_supported_features() {
    let schema = success(&["schema"]);
    let capabilities = success(&["capabilities"]);
    assert_eq!(schema["model"], capabilities["model"]);
    assert_eq!(schema["model"]["actions"].as_array().unwrap().len(), 8);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.json");
    let path = path.to_str().unwrap();
    for example in schema["examples"].as_object().unwrap().values() {
        write(path, example);
        success(&["validate", path]);
    }
    for geometry in schema["model"]["geometry"].as_array().unwrap() {
        write(path, &json!({"nodes":[{"id":"shape","kind":geometry}]}));
        success(&["validate", path]);
    }
    write(path, &json!({"audioAssets":[schema["audioAssetExample"]]}));
    success(&["validate", path]);
    write(path, &schema["examples"]["complete"]);
    let ctx = dir.path().join("context.json");
    let ctx = ctx.to_str().unwrap();
    write(ctx, &schema["model"]["evaluationContext"]["example"]);
    success(&["evaluate", path, "--context", ctx]);
    let commands = capabilities["commands"].as_array().unwrap();
    let evaluate = commands.iter().find(|c| c["name"] == "evaluate").unwrap();
    assert!(evaluate["arguments"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["long"] == "context"));
    assert!(commands.iter().any(|c| c["name"] == "blend-clip"));
}
