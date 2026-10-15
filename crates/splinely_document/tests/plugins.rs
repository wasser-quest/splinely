//! Plugin manifests (editor part), runtime-part files, apply/validate/round-trip and trusted
//! plugin export. The runtime code itself is exercised in runtime/tests/plugins.test.cjs and the
//! browser matrix; nothing here executes JavaScript.
use serde_json::{json, Value};
use splinely_document::plugin::{
    load_runtime_bundle, sha256_hex, CandidateError, ExtensionRegistry, MAX_EXPORT_PLUGINS,
    MAX_RUNTIME_ENTRY,
};
use splinely_document::{BrowserExportOptions, EmbedFormat, Node, Project};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "splinely-plugin-tests-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn plugin(&self, manifest: &Value, runtime: Option<&[u8]>) -> PathBuf {
        let dir = self.0.join("plugin");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir).unwrap();
        fs::write(
            dir.join("splinely-plugin.json"),
            serde_json::to_vec(manifest).unwrap(),
        )
        .unwrap();
        if let Some(bytes) = runtime {
            fs::write(dir.join("runtime.js"), bytes).unwrap();
        }
        dir
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn example_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/hover-wiggle")
}
fn behavior() -> Value {
    json!({
        "trigger": {"event": "click"},
        "clip": {"duration": 1.0, "tracks": [
            {"property": "rotation", "keyframes": [{"time": 0, "value": 0}, {"time": 1, "value": 10, "easing": "ease"}]}
        ]},
        "variables": {"hits": 0},
        "a11yLabel": "Clickable"
    })
}
fn manifest(behavior: Value) -> Value {
    json!({"id": "t", "title": "T", "tools": [{"id": "b", "title": "B", "action": {"type": "addBehavior", "behavior": behavior}}]})
}
fn load(manifest: &Value) -> Result<ExtensionRegistry, String> {
    let dir = Temp::new();
    let path = dir.plugin(manifest, None);
    ExtensionRegistry::load_plugin(&path).map(ExtensionRegistry::single)
}
fn demo() -> Project {
    let mut project = Project::default();
    project.nodes.push(Node::rect("box", 0., 0., 40., 40.));
    project.nodes.push(Node::rect("other", 50., 0., 10., 10.));
    project.validate().unwrap();
    project
}

#[test]
fn sha256_matches_published_vectors() {
    assert_eq!(
        sha256_hex(b""),
        "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "sha256:248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    assert_eq!(
        sha256_hex(&vec![b'a'; 1_000_000]),
        "sha256:cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

#[test]
fn accepts_valid_behavior_and_rejects_the_hostile_matrix() {
    assert!(load(&manifest(behavior())).is_ok());
    let mutate = |f: &dyn Fn(&mut Value)| {
        let mut b = behavior();
        f(&mut b);
        load(&manifest(b))
    };
    type Mutation = Box<dyn Fn(&mut Value)>;
    let cases: Vec<(&str, Mutation)> = vec![
        (
            "unknown behavior field",
            Box::new(|b| b["script"] = json!("x")),
        ),
        (
            "unknown trigger field",
            Box::new(|b| b["trigger"]["target"] = json!("n")),
        ),
        (
            "unknown clip field",
            Box::new(|b| b["clip"]["looping"] = json!(true)),
        ),
        (
            "unknown keyframe field",
            Box::new(|b| b["clip"]["tracks"][0]["keyframes"][0]["x"] = json!(1)),
        ),
        (
            "non-pointer trigger",
            Box::new(|b| b["trigger"]["event"] = json!("keydown")),
        ),
        (
            "custom trigger",
            Box::new(|b| b["trigger"]["event"] = json!("clip.end")),
        ),
        (
            "zero duration",
            Box::new(|b| b["clip"]["duration"] = json!(0)),
        ),
        (
            "huge duration",
            Box::new(|b| b["clip"]["duration"] = json!(31)),
        ),
        ("no tracks", Box::new(|b| b["clip"]["tracks"] = json!([]))),
        (
            "too many tracks",
            Box::new(|b| {
                let t = b["clip"]["tracks"][0].clone();
                b["clip"]["tracks"] = Value::Array(vec![t; 9]);
            }),
        ),
        (
            "duplicate track",
            Box::new(|b| {
                let t = b["clip"]["tracks"][0].clone();
                b["clip"]["tracks"] = json!([t.clone(), t]);
            }),
        ),
        (
            "one keyframe",
            Box::new(|b| b["clip"]["tracks"][0]["keyframes"] = json!([{"time": 0, "value": 1}])),
        ),
        (
            "too many keyframes",
            Box::new(|b| {
                b["clip"]["tracks"][0]["keyframes"] = Value::Array(
                    (0..33)
                        .map(|i| json!({"time": i as f64 / 40.0, "value": 1}))
                        .collect(),
                );
            }),
        ),
        (
            "non-increasing keyframes",
            Box::new(|b| b["clip"]["tracks"][0]["keyframes"][1]["time"] = json!(0)),
        ),
        (
            "keyframe after duration",
            Box::new(|b| b["clip"]["tracks"][0]["keyframes"][1]["time"] = json!(1.5)),
        ),
        (
            "unknown property",
            Box::new(|b| b["clip"]["tracks"][0]["property"] = json!("d")),
        ),
        (
            "string value for number",
            Box::new(|b| b["clip"]["tracks"][0]["keyframes"][0]["value"] = json!("1")),
        ),
        (
            "NaN-like huge value",
            Box::new(|b| b["clip"]["tracks"][0]["keyframes"][0]["value"] = json!(1e300)),
        ),
        (
            "bad easing",
            Box::new(|b| b["clip"]["tracks"][0]["keyframes"][1]["easing"] = json!("url(#x)")),
        ),
        (
            "color for number track",
            Box::new(|b| b["clip"]["tracks"][0]["keyframes"][0]["value"] = json!("#fff")),
        ),
        (
            "url color",
            Box::new(|b| {
                b["clip"]["tracks"][0]["property"] = json!("fill");
                b["clip"]["tracks"][0]["keyframes"][0]["value"] = json!("url(#evil)");
                b["clip"]["tracks"][0]["keyframes"][1]["value"] = json!("#112233");
            }),
        ),
        (
            "too many variables",
            Box::new(|b| {
                b["variables"] =
                    Value::Object((0..9).map(|i| (format!("v{i}"), json!(0))).collect());
            }),
        ),
        (
            "array variable",
            Box::new(|b| b["variables"]["hits"] = json!([1])),
        ),
        (
            "object variable",
            Box::new(|b| b["variables"]["hits"] = json!({"a": 1})),
        ),
        (
            "null variable",
            Box::new(|b| b["variables"]["hits"] = Value::Null),
        ),
        (
            "bad variable name",
            Box::new(|b| {
                b["variables"] = json!({"1bad": 0});
            }),
        ),
        (
            "path variable name",
            Box::new(|b| {
                b["variables"] = json!({"__proto__/x": 0});
            }),
        ),
        (
            "label whitespace",
            Box::new(|b| b["a11yLabel"] = json!(" padded ")),
        ),
        (
            "label too long",
            Box::new(|b| b["a11yLabel"] = json!("x".repeat(201))),
        ),
        ("label wrong type", Box::new(|b| b["a11yLabel"] = json!(5))),
        (
            "missing trigger",
            Box::new(|b| {
                b.as_object_mut().unwrap().remove("trigger");
            }),
        ),
        (
            "missing clip",
            Box::new(|b| {
                b.as_object_mut().unwrap().remove("clip");
            }),
        ),
    ];
    for (name, f) in &cases {
        assert!(mutate(f.as_ref()).is_err(), "must reject: {name}");
    }
    // Strict top-level / action fields and duplicate keys.
    let mut m = manifest(behavior());
    m["tools"][0]["action"]["code"] = json!("alert(1)");
    assert!(load(&m).is_err());
    let dir = Temp::new();
    let path = dir.plugin(&manifest(behavior()), None);
    let dup = fs::read_to_string(path.join("splinely-plugin.json"))
        .unwrap()
        .replacen("{\"id\"", "{\"id\":\"x\",\"id\"", 1);
    fs::write(path.join("splinely-plugin.json"), dup).unwrap();
    assert!(ExtensionRegistry::load_plugin(&path).is_err());
}

#[test]
fn runtime_part_matrix() {
    let dir = Temp::new();
    let good = b"return {id:'t', apiVersion:1};";
    let with_runtime = |runtime: Value, bytes: Option<&[u8]>| {
        let m = json!({"id": "t", "title": "T", "runtime": runtime});
        ExtensionRegistry::load_plugin(dir.plugin(&m, bytes))
    };
    // A runtime-only plugin is valid and reports its pinned hash and size.
    let ok = with_runtime(json!({"apiVersion": 1, "entry": "runtime.js"}), Some(good)).unwrap();
    let part = ok.runtime.as_ref().unwrap();
    assert_eq!(part.sha256, sha256_hex(good));
    assert_eq!(part.bytes, good.len());
    assert_eq!(part.source().as_bytes(), good);
    // Bad manifests.
    for (name, runtime) in [
        (
            "future apiVersion",
            json!({"apiVersion": 2, "entry": "runtime.js"}),
        ),
        (
            "string apiVersion",
            json!({"apiVersion": "1", "entry": "runtime.js"}),
        ),
        ("missing entry", json!({"apiVersion": 1})),
        (
            "subdirectory",
            json!({"apiVersion": 1, "entry": "sub/runtime.js"}),
        ),
        ("parent", json!({"apiVersion": 1, "entry": "../runtime.js"})),
        ("dotdot name", json!({"apiVersion": 1, "entry": "a..b.js"})),
        (
            "absolute",
            json!({"apiVersion": 1, "entry": "/etc/passwd.js"}),
        ),
        ("not js", json!({"apiVersion": 1, "entry": "runtime.html"})),
        ("hidden", json!({"apiVersion": 1, "entry": ".runtime.js"})),
        ("url", json!({"apiVersion": 1, "entry": "https://x/y.js"})),
        (
            "unknown field",
            json!({"apiVersion": 1, "entry": "runtime.js", "url": "x"}),
        ),
        (
            "long description",
            json!({"apiVersion": 1, "entry": "runtime.js", "description": "x".repeat(241)}),
        ),
    ] {
        assert!(
            with_runtime(runtime, Some(good)).is_err(),
            "must reject: {name}"
        );
    }
    let entry = json!({"apiVersion": 1, "entry": "runtime.js"});
    assert!(with_runtime(entry.clone(), None).is_err(), "missing file");
    assert!(
        with_runtime(entry.clone(), Some(b"")).is_err(),
        "empty file"
    );
    assert!(
        with_runtime(entry.clone(), Some(&[0xff, 0xfe, 0x00])).is_err(),
        "not UTF-8"
    );
    assert!(
        with_runtime(entry.clone(), Some(b"return 1;\0")).is_err(),
        "NUL"
    );
    assert!(
        with_runtime(entry.clone(), Some(&vec![b' '; MAX_RUNTIME_ENTRY + 1])).is_err(),
        "size cap"
    );
    let mut at_cap = b"return 1;".to_vec();
    at_cap.resize(MAX_RUNTIME_ENTRY, b' ');
    assert!(
        with_runtime(entry.clone(), Some(&at_cap)).is_ok(),
        "exactly at the cap"
    );
    for token in [
        "fetch('x')",
        "new XMLHttpRequest()",
        "new WebSocket('x')",
        "navigator.sendBeacon(1)",
        "var u='https://evil.example/'",
        "importScripts('a')",
        "import('x')",
        "eval('1')",
        "new Function('1')",
        "'</script>'",
        "<!--",
    ] {
        let src = format!("return {{id:'t'}}; // {token}");
        assert!(
            with_runtime(entry.clone(), Some(src.as_bytes())).is_err(),
            "lint must reject {token}"
        );
    }
    // Harmless text that merely resembles a token is fine (`rows:` is not `ws://`).
    assert!(with_runtime(entry.clone(), Some(b"const rows: 5; return {id:'t'};")).is_ok());
    // A plugin with neither tools, palette nor runtime is rejected.
    let nothing = json!({"id": "t", "title": "T"});
    assert!(ExtensionRegistry::load_plugin(dir.plugin(&nothing, None)).is_err());
}

#[cfg(unix)]
#[test]
fn runtime_entry_symlink_is_rejected() {
    let dir = Temp::new();
    let target = dir.0.join("outside.js");
    fs::write(&target, "return {id:'t'};").unwrap();
    let m = json!({"id": "t", "title": "T", "runtime": {"apiVersion": 1, "entry": "runtime.js"}});
    let path = dir.plugin(&m, None);
    std::os::unix::fs::symlink(&target, path.join("runtime.js")).unwrap();
    assert!(ExtensionRegistry::load_plugin(&path).is_err());
}

#[test]
fn behavior_merges_declaratively_with_unique_ids_and_no_side_effects() {
    let registry = load(&manifest(behavior())).unwrap();
    let project = demo();
    let before = project.clone();
    let one = registry.candidate("t", "b", &project, Some("box")).unwrap();
    assert_eq!(project, before, "the input project is never mutated");
    one.validate().unwrap();
    assert_eq!(one.clips.len(), 1);
    assert_eq!(one.state_machines.len(), 1);
    let clip = &one.clips[0];
    assert!(!clip.looping && !clip.autoplay);
    assert!(clip.tracks.iter().all(|t| t.node_id == "box"));
    assert_eq!(one.variables["hits"], json!(0));
    assert_eq!(
        one.nodes[0].a11y.as_ref().unwrap().label.as_deref(),
        Some("Clickable")
    );
    // Applying twice on the same and on another node allocates fresh ids.
    let two = registry.candidate("t", "b", &one, Some("box")).unwrap();
    let three = registry.candidate("t", "b", &two, Some("other")).unwrap();
    three.validate().unwrap();
    let ids: std::collections::BTreeSet<_> = three
        .clips
        .iter()
        .map(|c| c.id.clone())
        .chain(three.state_machines.iter().map(|m| m.id.clone()))
        .collect();
    assert_eq!(ids.len(), 6);
    // An existing accessible label is never overwritten.
    assert_eq!(
        three.nodes[0].a11y.as_ref().unwrap().label.as_deref(),
        Some("Clickable")
    );
    // Round trips: JSON, binary and SVG metadata.
    assert_eq!(
        Project::from_json(&three.to_json().unwrap()).unwrap(),
        three
    );
    assert_eq!(
        Project::from_binary(&three.to_binary().unwrap()).unwrap(),
        three
    );
    let svg = three.to_svg("/*rt*/").unwrap();
    assert_eq!(Project::from_svg_metadata(&svg).unwrap(), three);
}

#[test]
fn behavior_apply_errors_leave_nothing_behind() {
    let registry = load(&manifest(behavior())).unwrap();
    let mut project = demo();
    assert_eq!(
        registry.candidate("t", "b", &project, None),
        Err(CandidateError::NeedSelection)
    );
    assert_eq!(
        registry.candidate("t", "b", &project, Some("nope")),
        Err(CandidateError::SelectionMissing)
    );
    assert_eq!(
        registry.candidate("x", "b", &project, Some("box")),
        Err(CandidateError::UnknownPlugin)
    );
    assert_eq!(
        registry.candidate("t", "x", &project, Some("box")),
        Err(CandidateError::UnknownTool)
    );
    // A same-named variable of another type is refused rather than coerced.
    project.variables.insert("hits".into(), json!("text"));
    assert!(registry.candidate("t", "b", &project, Some("box")).is_err());
    project.variables.insert("hits".into(), json!(7));
    let kept = registry.candidate("t", "b", &project, Some("box")).unwrap();
    assert_eq!(kept.variables["hits"], json!(7), "existing values are kept");
    project.nodes[0].locked = true;
    assert_eq!(
        registry.candidate("t", "b", &project, Some("box")),
        Err(CandidateError::SelectionLocked)
    );
    // A pre-existing id in any namespace is avoided, not overwritten.
    let mut project = demo();
    project
        .nodes
        .push(Node::rect("plugin-b-clip-1", 0., 0., 1., 1.));
    let next = registry.candidate("t", "b", &project, Some("box")).unwrap();
    assert!(next.clips.iter().all(|c| c.id != "plugin-b-clip-1"));
    next.validate().unwrap();
    // The per-project clip bound stops runaway repeated application.
    let mut project = demo();
    for _ in 0..256 {
        project = registry.candidate("t", "b", &project, Some("box")).unwrap();
    }
    assert!(registry.candidate("t", "b", &project, Some("box")).is_err());
}

#[test]
fn shipped_hover_wiggle_example_applies_validates_and_pins_its_runtime() {
    let plugin = ExtensionRegistry::load_plugin(example_dir()).unwrap();
    assert_eq!(plugin.id, "hover-wiggle");
    let part = plugin.runtime.clone().unwrap();
    let bytes = fs::read(example_dir().join("runtime.js")).unwrap();
    assert_eq!(part.sha256, sha256_hex(&bytes));
    assert_eq!(part.entry, "runtime.js");
    let tools: Vec<_> = plugin.tools.iter().map(|t| t.id.clone()).collect();
    assert_eq!(tools, ["wiggle", "bounce"]);
    let registry = ExtensionRegistry::single(plugin);
    let mut project = demo();
    for tool in &tools {
        project = registry
            .candidate("hover-wiggle", tool, &project, Some("box"))
            .unwrap();
        project.validate().unwrap();
    }
    assert!(project.variables.contains_key("wiggleCount"));
    assert!(project.variables.contains_key("wiggleLast"));
    let events: Vec<_> = project
        .state_machines
        .iter()
        .flat_map(|m| &m.transitions)
        .map(|t| t.event.name.clone())
        .collect();
    assert_eq!(events, ["pointerenter", "click"]);
}

#[test]
fn plugin_export_requires_trust_pins_hashes_and_stays_valid_xml() {
    let bundle = load_runtime_bundle(example_dir()).unwrap();
    let project = demo();
    let off = BrowserExportOptions {
        trusted_js: false,
        audio_controls: false,
    };
    let on = BrowserExportOptions {
        trusted_js: true,
        audio_controls: false,
    };
    let err = project
        .to_svg_with_plugins(
            "/*rt*/",
            off,
            EmbedFormat::Binary,
            std::slice::from_ref(&bundle),
        )
        .unwrap_err();
    assert!(err.to_string().contains("trusted_js"), "{err}");
    // No plugins: byte-identical to the plain export path.
    assert_eq!(
        project
            .to_svg_with_plugins("/*rt*/", off, EmbedFormat::Binary, &[])
            .unwrap(),
        project
            .to_svg_with_options_embed("/*rt*/", off, EmbedFormat::Binary)
            .unwrap()
    );
    for embed in [EmbedFormat::Binary, EmbedFormat::Json] {
        let svg = project
            .to_svg_with_plugins("/*rt*/", on, embed, std::slice::from_ref(&bundle))
            .unwrap();
        roxmltree::Document::parse(&svg).expect("exported SVG is well-formed XML");
        assert!(svg.contains("<metadata id=\"splinely-plugins\""));
        assert!(svg.contains(&bundle.sha256));
        assert!(svg.contains("splinely.use(plugin"));
        assert_eq!(Project::from_svg_metadata(&svg).unwrap(), project);
    }
    // The plain trusted export contains no plugin machinery.
    let plain = project
        .to_svg_with_options_embed("/*rt*/", on, EmbedFormat::Binary)
        .unwrap();
    assert!(!plain.contains("splinely-plugins") && !plain.contains("splinely.use"));
    // Duplicate ids and the per-export cap.
    assert!(project
        .to_svg_with_plugins(
            "/*rt*/",
            on,
            EmbedFormat::Binary,
            &[bundle.clone(), bundle.clone()]
        )
        .is_err());
    let dir = Temp::new();
    let many: Vec<_> = (0..=MAX_EXPORT_PLUGINS)
        .map(|i| {
            let p = dir.0.join(format!("p{i}.js"));
            fs::write(&p, "return {id:'x'};").unwrap();
            load_runtime_bundle(&p).unwrap()
        })
        .collect();
    assert!(project
        .to_svg_with_plugins("/*rt*/", on, EmbedFormat::Binary, &many)
        .is_err());
}

#[test]
fn hostile_runtime_source_cannot_break_out_of_the_svg() {
    let dir = Temp::new();
    let path = dir.0.join("evil.js");
    // CDATA terminator, XML-special characters and JS line separators inside a string literal.
    fs::write(
        &path,
        "return {id:'evil', apiVersion:1, note:']]><x/>&amp;\u{2028}\u{2029}\u{ffff}'};",
    )
    .unwrap();
    let bundle = load_runtime_bundle(&path).unwrap();
    assert_eq!(bundle.id, "evil");
    let on = BrowserExportOptions {
        trusted_js: true,
        audio_controls: false,
    };
    let svg = demo()
        .to_svg_with_plugins("", on, EmbedFormat::Binary, &[bundle])
        .unwrap();
    roxmltree::Document::parse(&svg).expect("still well-formed XML");
    assert_eq!(svg.matches("<script").count(), 1);
    // Raw file names must be plain.
    let weird = dir.0.join("a b.js");
    fs::write(&weird, "return 1;").unwrap();
    assert!(load_runtime_bundle(&weird).is_err());
    assert!(load_runtime_bundle(dir.0.join("evil.txt")).is_err());
    assert!(
        load_runtime_bundle(&dir.0).is_err(),
        "folder without a manifest"
    );
}

#[test]
fn isolated_preview_lists_embedded_plugins_but_keeps_one_wrapper_script() {
    let bundle = load_runtime_bundle(example_dir()).unwrap();
    let html = demo()
        .to_isolated_preview_html_with_plugins("/*rt*/", std::slice::from_ref(&bundle))
        .unwrap();
    assert!(html.contains(&bundle.sha256));
    assert!(html.contains("hover-wiggle"));
    assert_eq!(html.matches("<script>").count(), 1);
    assert!(html.contains("setAttribute('sandbox','allow-scripts')"));
    assert!(!html.contains("__PLUGIN_NOTICE__"));
    // Plugin source only travels percent-encoded inside the sandboxed data URL.
    assert!(!html.contains("setVariable"));
    let plain = demo().to_isolated_preview_html("/*rt*/").unwrap();
    assert!(!plain.contains("Runtime plugins embedded"));
}
