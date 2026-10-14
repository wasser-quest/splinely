use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

fn invoke(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_forma-cli"))
        .args(args)
        .output()
        .unwrap()
}
fn success(args: &[&str]) -> Value {
    let output = invoke(args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn error(args: &[&str], code: &str) {
    let output = invoke(args);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let response: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(response["error"]["code"], code, "{response}");
}
fn embed<'a>(input: &'a str, output: &'a str, text: &'a str) -> Vec<&'a str> {
    vec![
        "embed-speech",
        input,
        "--output",
        output,
        "--text",
        text,
        "--voice",
        "de",
        "--asset-id",
        "hello",
        "--time",
        "0",
    ]
}

#[test]
fn help_describes_local_speech() {
    let output = invoke(&["embed-speech", "--help"]);
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for option in [
        "--text",
        "--voice",
        "--asset-id",
        "--time",
        "--engine",
        "4096",
        "local",
    ] {
        assert!(help.contains(option), "{help}");
    }
}

#[test]
fn missing_engine_and_bad_arguments_preserve_files() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.json");
    let input = input.to_str().unwrap();
    success(&["new", "--output", input]);
    let original = std::fs::read(input).unwrap();
    let missing = dir.path().join("absent-engine");
    let mut args = embed(input, input, "Hallo");
    args.extend(["--engine", missing.to_str().unwrap()]);
    error(&args, "speech_engine_error");
    assert_eq!(std::fs::read(input).unwrap(), original);
    let output = dir.path().join("output.json");
    let output = output.to_str().unwrap();
    std::fs::write(output, b"sentinel").unwrap();
    args = embed(input, output, "Hallo");
    args.extend(["--engine", missing.to_str().unwrap()]);
    error(&args, "speech_engine_error");
    assert_eq!(std::fs::read(output).unwrap(), b"sentinel");
    for text in [" ", &"x".repeat(4097)] {
        error(&embed(input, output, text), "invalid_speech");
    }
    for time in ["NaN", "inf"] {
        args = embed(input, output, "Hallo");
        let index = args.iter().position(|a| *a == "--time").unwrap();
        args[index + 1] = time;
        error(&args, "invalid_speech");
    }
    std::fs::write(input, b"{\"width\":18446744073709551616}").unwrap();
    error(&embed(input, output, "Hallo"), "data_loss");
    assert_eq!(std::fs::read(output).unwrap(), b"sentinel");
}

#[test]
fn actual_espeak_embeds_wav_and_exports_without_browser_speech() {
    let engine = ["/usr/bin/espeak-ng", "/usr/bin/espeak"]
        .into_iter()
        .find(|p| Path::new(p).is_file());
    let Some(engine) = engine else {
        eprintln!("Skipping actual synthesis: no local eSpeak installed");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("speech.json");
    let input = input.to_str().unwrap();
    success(&["new", "--output", input]);
    let mut args = embed(input, input, "Hallo! Offline ohne Netzwerk.");
    args.extend(["--engine", engine]);
    success(&args);
    let bytes = std::fs::read(input).unwrap();
    let project: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(project["audioAssets"][0]["mimeType"], "audio/wav");
    let data = project["audioAssets"][0]["dataBase64"].as_str().unwrap();
    assert!(data.starts_with("UklGR")); // RIFF
    assert!(data.len() > 1000);
    assert_eq!(project["audioCues"][0]["assetId"], "hello");
    assert_eq!(project["audioCues"][0]["id"], "hello-cue");
    assert_eq!(project["audioCues"][0]["time"], 0.0);
    assert_eq!(project["speechCues"].as_array().unwrap().len(), 0);
    success(&["validate", input]);
    error(&args, "validation_error"); // Duplicate IDs rejected before synthesis.
    assert_eq!(std::fs::read(input).unwrap(), bytes);
    let svg = dir.path().join("speech.svg");
    success(&["export", input, "--output", svg.to_str().unwrap()]);
    let svg = std::fs::read_to_string(svg).unwrap();
    assert!(svg.contains(data));
}

#[cfg(unix)]
#[test]
fn mock_engines_reject_failure_invalid_wav_and_oversized_output() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.json");
    let input = input.to_str().unwrap();
    success(&["new", "--output", input]);
    let original = std::fs::read(input).unwrap();
    let engine = dir.path().join("engine with spaces");
    for (body, code) in [
        ("exit 2", "speech_engine_error"),
        ("printf not-wav > \"$out\"", "speech_error"),
        ("/usr/bin/mkfifo \"$out\"", "speech_error"),
        ("ln -s /dev/zero \"$out\"", "speech_error"),
        ("/usr/bin/truncate -s 8388609 \"$out\"", "speech_error"),
    ] {
        // A mock executable is deliberately injected; the CLI itself never uses a shell.
        // The WAV path follows -w (eSpeak's argument convention).
        let find_output = "while [ \"$1\" != -w ]; do shift; done; out=\"$2\"";
        std::fs::write(&engine, format!("#!/bin/sh\n{find_output}\n{body}\n")).unwrap();
        std::fs::set_permissions(&engine, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut args = embed(input, input, "--help; $(touch unsafe)");
        args.extend(["--engine", engine.to_str().unwrap()]);
        error(&args, code);
        assert_eq!(std::fs::read(input).unwrap(), original);
    }
}

#[test]
fn speak_writes_polished_wav_and_engines_are_listed() {
    let engines = success(&["speech-engines"]);
    assert!(engines["installed"].is_array());
    if !engines["installed"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["name"] == "espeak-ng" || e["name"] == "espeak")
    {
        eprintln!("Skipping: no local eSpeak installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("probe.wav");
    let wav = wav.to_str().unwrap();
    let result = success(&[
        "speak",
        "--text",
        "Hallo, Welt.",
        "--lang",
        "de-DE",
        "--rate",
        "0.9",
        "--output",
        wav,
    ]);
    assert!(result["engine"].as_str().unwrap().starts_with("espeak"));
    assert!(result["seconds"].as_f64().unwrap() > 0.3);
    assert!(std::fs::read(wav).unwrap().starts_with(b"RIFF"));
    let raw = dir.path().join("raw.wav");
    success(&[
        "speak",
        "--text",
        "Hallo, Welt.",
        "--lang",
        "de-DE",
        "--raw",
        "--output",
        raw.to_str().unwrap(),
    ]);
    // Polishing trims the engine's leading/trailing silence.
    assert!(
        std::fs::metadata(wav).unwrap().len() < std::fs::metadata(&raw).unwrap().len() * 11 / 10
    );
    // Embedding reports engine and length; out-of-range options are rejected.
    let project = dir.path().join("p.frm");
    let project = project.to_str().unwrap();
    success(&["new", "--output", project]);
    let embedded = success(&[
        "embed-speech",
        project,
        "--text",
        "Guten Tag",
        "--lang",
        "de-DE",
        "--asset-id",
        "gruss",
        "--pitch",
        "1.1",
    ]);
    assert!(embedded["engine"].is_string() && embedded["seconds"].as_f64().unwrap() > 0.2);
    error(
        &[
            "embed-speech",
            project,
            "--text",
            "x",
            "--asset-id",
            "y",
            "--rate",
            "9",
        ],
        "invalid_speech",
    );
    error(
        &["speak", "--text", "x", "--synth", "piper", "--output", wav],
        "speech_engine_error",
    );
}
