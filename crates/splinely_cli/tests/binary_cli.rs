//! Binary `.sply` defaults, sniffed loading, stdin, hostile files, convert/cat/inspect, SVG embedding.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;
use splinely_document::{decode_to_value, Project, SPLY_MAGIC};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::tempdir;

fn run(args: &[&str], stdin: Option<&[u8]>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_splinely-cli"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut pipe = child.stdin.take().unwrap();
    if let Some(bytes) = stdin {
        pipe.write_all(bytes).unwrap();
    }
    drop(pipe);
    child.wait_with_output().unwrap()
}
fn ok(args: &[&str], stdin: Option<&[u8]>) -> Value {
    let out = run(args, stdin);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty());
    serde_json::from_slice(&out.stdout).unwrap()
}
fn fails(args: &[&str], stdin: Option<&[u8]>, code: &str) -> Value {
    let out = run(args, stdin);
    assert_eq!(out.status.code(), Some(1), "{args:?}");
    assert!(out.stdout.is_empty());
    let value: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(value["error"]["code"], code, "{value}");
    value
}
fn is_binary(path: &Path) -> bool {
    std::fs::read(path).unwrap().starts_with(&SPLY_MAGIC)
}
fn canonical(path: &Path) -> Value {
    let bytes = std::fs::read(path).unwrap();
    if Project::is_binary(&bytes) {
        decode_to_value(&bytes).unwrap()
    } else {
        serde_json::from_slice(&bytes).unwrap()
    }
}
fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn sply_is_binary_by_default_and_json_extensions_stay_text() {
    let dir = tempdir().unwrap();
    for (name, binary) in [
        ("a.sply", true),
        ("b.SPLY", true),
        ("c.splinely.json", false),
        ("d.json", false),
        ("e", false),
    ] {
        let path = dir.path().join(name);
        let result = ok(&["new", "--demo", "--output", s(&path)], None);
        assert_eq!(result["encoding"], if binary { "binary" } else { "json" });
        assert_eq!(is_binary(&path), binary, "{name}");
        if !binary {
            assert!(std::fs::read(&path).unwrap().starts_with(b"{"));
        }
        assert_eq!(
            Project::load_file(&path).unwrap().name,
            "Splinely autonomous demo"
        );
    }
    // Other creators and `example --output` follow the same extension rule.
    let sply = dir.path().join("rig.sply");
    ok(&["rig-demo", "--output", s(&sply)], None);
    assert!(is_binary(&sply));
    let sply = dir.path().join("ex.sply");
    ok(&["example", "--output", s(&sply)], None);
    assert!(is_binary(&sply));
    // stdout stays JSON for LLM workflows.
    let stdout = ok(&["example"], None);
    assert_eq!(stdout["name"], "Splinely autonomous demo");
}

#[test]
fn binary_projects_edit_in_place_and_stay_binary() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("scene.sply");
    ok(
        &[
            "new",
            "--output",
            s(&path),
            "--width",
            "200",
            "--height",
            "100",
        ],
        None,
    );
    ok(
        &[
            "node",
            "add",
            s(&path),
            "--kind",
            "rect",
            "--id",
            "bg",
            "--width",
            "200",
            "--height",
            "100",
        ],
        None,
    );
    assert!(is_binary(&path));
    assert_eq!(ok(&["validate", s(&path)], None)["ok"], true);
    let nodes = ok(&["node", "list", s(&path)], None);
    assert!(nodes.to_string().contains("bg"));
    // A JSON file edited in place stays JSON; the same edit on .sply stays binary.
    let json = dir.path().join("scene.json");
    ok(&["convert", s(&path), "--output", s(&json)], None);
    ok(
        &[
            "node",
            "add",
            s(&json),
            "--kind",
            "circle",
            "--id",
            "dot",
            "--radius",
            "5",
        ],
        None,
    );
    assert!(!is_binary(&json));
    assert_eq!(canonical(&json)["nodes"].as_array().unwrap().len(), 2);
    ok(
        &["batch", s(&path), "--steps", "-", "--output", s(&path)],
        Some(b"node add --kind circle --id dot --radius 5\n"),
    );
    assert!(is_binary(&path));
    assert_eq!(canonical(&path), canonical(&json));
    // Patches are JSON text and apply to binary projects.
    let patch = dir.path().join("patch.json");
    std::fs::write(&patch, r#"{"name":"patched"}"#).unwrap();
    ok(
        &[
            "apply",
            s(&path),
            "--patch",
            s(&patch),
            "--output",
            s(&path),
        ],
        None,
    );
    assert!(is_binary(&path));
    assert_eq!(canonical(&path)["name"], "patched");
    // A binary file is not valid patch text.
    fails(
        &["apply", s(&json), "--patch", s(&path), "--output", s(&json)],
        None,
        "invalid_json",
    );
}

#[test]
fn stdin_accepts_binary_and_json() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("p.sply");
    ok(&["new", "--demo", "--output", s(&path)], None);
    let binary = std::fs::read(&path).unwrap();
    let json = serde_json::to_vec(&canonical(&path)).unwrap();
    for input in [&binary, &json] {
        assert_eq!(ok(&["validate", "-"], Some(input))["ok"], true);
        assert_eq!(ok(&["inspect", "-"], Some(input))["nodes"][1]["id"], "ball");
        assert_eq!(ok(&["cat", "-"], Some(input)), canonical(&path));
        assert!(ok(&["evaluate", "-", "--time", "0.5"], Some(input))["ok"]
            .as_bool()
            .unwrap_or(true));
        let out = dir.path().join("from-stdin.sply");
        ok(&["convert", "-", "--output", s(&out)], Some(input));
        assert!(is_binary(&out));
        assert_eq!(canonical(&out), canonical(&path));
    }
    // stdin is bounded to 64 MiB.
    let huge = vec![b' '; 64 * 1024 * 1024 + 1];
    let out = run(&["validate", "-"], Some(&huge));
    assert_eq!(out.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(value["error"]["code"], "read_error");
}

#[test]
fn import_svg_input_is_bounded_to_64_mib() {
    let dir = tempdir().unwrap();
    let huge = dir.path().join("huge.svg");
    // Sparse file: one byte over the limit, never written to disk.
    std::fs::File::create(&huge)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    let out = dir.path().join("out.sply");
    let value = fails(
        &["import-svg", s(&huge), "--output", s(&out)],
        None,
        "read_error",
    );
    assert!(value["error"]["message"]
        .as_str()
        .unwrap()
        .contains("64 MiB"));
    assert!(!out.exists());
}

#[test]
fn hostile_binary_files_fail_with_typed_errors_and_change_nothing() {
    let dir = tempdir().unwrap();
    let good = dir.path().join("good.sply");
    ok(&["new", "--demo", "--output", s(&good)], None);
    let bytes = std::fs::read(&good).unwrap();
    let mut cases: Vec<(&str, Vec<u8>)> = vec![];
    cases.push(("truncated header", bytes[..20].to_vec()));
    cases.push(("magic only", SPLY_MAGIC.to_vec()));
    cases.push(("truncated body", bytes[..bytes.len() - 5].to_vec()));
    let mut trailing = bytes.clone();
    trailing.push(0);
    cases.push(("trailing byte", trailing));
    let mut crc = bytes.clone();
    *crc.last_mut().unwrap() ^= 0xff;
    cases.push(("crc mismatch", crc));
    let mut major = bytes.clone();
    major[8] = 9;
    cases.push(("unsupported major", major));
    let mut flags = bytes.clone();
    flags[12] = 1;
    cases.push(("flags", flags));
    let mut count = bytes.clone();
    count[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    cases.push(("string count", count));
    for (label, data) in cases {
        let bad = dir.path().join("bad.sply");
        std::fs::write(&bad, &data).unwrap();
        let keep = dir.path().join("keep.sply");
        std::fs::write(&keep, b"sentinel").unwrap();
        for args in [
            vec!["validate", s(&bad)],
            vec!["inspect", s(&bad)],
            vec!["cat", s(&bad)],
            vec!["convert", s(&bad), "--output", s(&keep)],
            vec!["node", "list", s(&bad)],
        ] {
            let value = fails(&args, None, "invalid_binary");
            assert!(
                value["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("bad.sply"),
                "{label}"
            );
            assert!(value["error"]["hint"].is_string());
        }
        assert_eq!(std::fs::read(&keep).unwrap(), b"sentinel", "{label}");
        assert_eq!(std::fs::read(&bad).unwrap(), data, "{label}");
        fails(&["validate", "-"], Some(&data), "invalid_binary");
    }
    // Structurally valid binary that breaks project rules reports the project error.
    let bad = dir.path().join("rules.sply");
    let tree = splinely_document::encode_value(&serde_json::json!({"typo": true})).unwrap();
    std::fs::write(&bad, tree).unwrap();
    fails(&["validate", s(&bad)], None, "unknown_field");
}

#[test]
fn inspect_reports_file_encoding_sizes_and_stats() {
    let dir = tempdir().unwrap();
    let sply = dir.path().join("p.sply");
    let json = dir.path().join("p.json");
    ok(&["new", "--demo", "--output", s(&sply)], None);
    ok(&["convert", s(&sply), "--output", s(&json)], None);
    let binary = ok(&["inspect", s(&sply)], None);
    let file = &binary["file"];
    let size = std::fs::metadata(&sply).unwrap().len();
    assert_eq!(file["format"], "binary");
    assert_eq!(file["formatVersion"], "1.0");
    assert_eq!(file["bytes"], size);
    assert_eq!(file["crcOk"], true);
    assert_eq!(file["schemaVersion"], 1);
    let strings = file["stringCount"].as_u64().unwrap();
    assert!(strings > 10);
    let sections = &file["sections"];
    assert_eq!(sections["header"], 32);
    assert_eq!(
        sections["stringOffsets"].as_u64().unwrap(),
        4 * (strings + 1)
    );
    assert_eq!(
        32 + sections["stringOffsets"].as_u64().unwrap()
            + sections["strings"].as_u64().unwrap()
            + sections["body"].as_u64().unwrap(),
        size
    );
    let json_size = std::fs::metadata(&json).unwrap().len();
    assert_eq!(file["equivalent"]["jsonBytes"], json_size);
    assert_eq!(file["equivalent"]["binaryBytes"], size);
    assert!(size < json_size);
    assert_eq!(binary["stats"]["nodes"], 3);
    assert_eq!(binary["stats"]["keyframes"], 3);
    assert_eq!(binary["stats"]["clips"], 1);
    let text = ok(&["inspect", s(&json)], None);
    assert_eq!(text["file"]["format"], "json");
    assert_eq!(text["file"]["bytes"], json_size);
    assert!(text["file"].get("crcOk").is_none());
    assert_eq!(text["file"]["equivalent"], file["equivalent"]);
    assert_eq!(text["stats"], binary["stats"]);
    // The previous summary fields are unchanged.
    assert_eq!(text["nodes"][1]["id"], "ball");
    assert_eq!(text["schemaVersion"], 1);
}

#[test]
fn convert_round_trips_and_format_overrides_the_extension() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("s.json");
    let mut value = ok(&["schema"], None)["examples"]["complete"].clone();
    value["customJs"] = serde_json::json!("// Grüße\n");
    std::fs::write(&source, value.to_string()).unwrap();
    let expected = ok(&["cat", s(&source)], None);
    let a = dir.path().join("a.sply");
    let b = dir.path().join("b.json");
    let c = dir.path().join("c.sply");
    ok(&["convert", s(&source), "--output", s(&a)], None);
    ok(&["convert", s(&a), "--output", s(&b)], None);
    ok(&["convert", s(&b), "--output", s(&c)], None);
    assert!(is_binary(&a) && is_binary(&c) && !is_binary(&b));
    assert_eq!(canonical(&a), expected);
    assert_eq!(canonical(&b), expected);
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&c).unwrap());
    // --format wins over the extension in both directions.
    let forced_binary = dir.path().join("forced.dat");
    let forced_json = dir.path().join("forced.sply");
    let r = ok(
        &[
            "convert",
            s(&source),
            "--output",
            s(&forced_binary),
            "--format",
            "binary",
        ],
        None,
    );
    assert_eq!(r["encoding"], "binary");
    ok(
        &[
            "convert",
            s(&source),
            "--output",
            s(&forced_json),
            "--format",
            "json",
        ],
        None,
    );
    assert!(is_binary(&forced_binary) && !is_binary(&forced_json));
    assert_eq!(canonical(&forced_json), expected);
    // Loading sniffs content, not extensions.
    assert_eq!(ok(&["cat", s(&forced_binary)], None), expected);
    assert_eq!(ok(&["cat", s(&forced_json)], None), expected);
    // Binary cannot go to stdout; JSON can.
    fails(
        &["convert", s(&a), "--output", "-", "--format", "binary"],
        None,
        "invalid_arguments",
    );
    assert_eq!(ok(&["convert", s(&a), "--output", "-"], None), expected);
    assert_eq!(
        ok(
            &["convert", s(&a), "--output", "-", "--format", "json"],
            None
        ),
        expected
    );
}

#[test]
fn svg_export_embeds_binary_by_default_and_json_on_request() {
    let dir = tempdir().unwrap();
    let project = dir.path().join("p.sply");
    ok(
        &[
            "new",
            "--demo",
            "--name",
            "Embedded Ünïcode",
            "--output",
            s(&project),
        ],
        None,
    );
    let expected = ok(&["cat", s(&project)], None);
    let default = dir.path().join("default.svg");
    let binary = dir.path().join("binary.svg");
    let legacy = dir.path().join("legacy.svg");
    ok(&["export", s(&project), "--output", s(&default)], None);
    ok(
        &[
            "export",
            s(&project),
            "--output",
            s(&binary),
            "--embed",
            "binary",
        ],
        None,
    );
    ok(
        &[
            "export",
            s(&project),
            "--output",
            s(&legacy),
            "--embed",
            "json",
        ],
        None,
    );
    assert_eq!(
        std::fs::read(&default).unwrap(),
        std::fs::read(&binary).unwrap()
    );
    let embedded = |path: &Path| metadata(&std::fs::read_to_string(path).unwrap());
    let (attr, payload) = embedded(&binary);
    assert!(attr, "binary embedding is marked data-encoding=sply-base64");
    let decoded = STANDARD.decode(payload.trim()).unwrap();
    assert!(Project::is_binary(&decoded));
    assert!(!payload.contains("Embedded"));
    assert_eq!(decode_to_value(&decoded).unwrap(), expected);
    let (attr, payload) = embedded(&legacy);
    assert!(!attr);
    assert_eq!(serde_json::from_str::<Value>(&payload).unwrap(), expected);
    assert!(std::fs::read_to_string(&legacy)
        .unwrap()
        .contains("Embedded Ünïcode"));
    // Streamed export and invalid values.
    let streamed = ok(
        &["export", s(&project), "--output", "-", "--embed", "json"],
        None,
    );
    assert_eq!(streamed["svg"], std::fs::read_to_string(&legacy).unwrap());
    let out = run(
        &[
            "export",
            s(&project),
            "--output",
            s(&binary),
            "--embed",
            "xml",
        ],
        None,
    );
    assert_eq!(out.status.code(), Some(1));
}

/// (has data-encoding="sply-base64", text of `<metadata id="splinely-project">`), unescaping XML entities.
fn metadata(svg: &str) -> (bool, String) {
    let start = svg.find("<metadata id=\"splinely-project\"").unwrap();
    let open_end = start + svg[start..].find('>').unwrap();
    let attr = svg[start..open_end].contains("data-encoding=\"sply-base64\"");
    let end = open_end + svg[open_end..].find("</metadata>").unwrap();
    let text = svg[open_end + 1..end]
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    (attr, text)
}
