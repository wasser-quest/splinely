use base64::{engine::general_purpose::STANDARD, Engine};
use forma_viewer::{prepare, wrap_svg, ViewerOptions, MAX_INPUT_BYTES, MAX_XML_NODES};

#[test]
fn xml_node_limit_is_enforced_before_rendering() {
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\">{}</svg>",
        "<g/>".repeat(MAX_XML_NODES as usize)
    );
    assert!(wrap_svg(&svg).is_err());
}
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Input(PathBuf);
impl Input {
    fn new(data: &str, extension: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "forma-viewer-test-{}-{}.{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            extension
        ));
        std::fs::write(&path, data).unwrap();
        Self(path)
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn svg_in(html: &str) -> String {
    let encoded = html
        .split("src=\"data:image/svg+xml;base64,")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    String::from_utf8(STANDARD.decode(encoded).unwrap()).unwrap()
}

#[test]
fn svg_is_a_document_not_html_parsed_markup() {
    let svg = r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><script><![CDATA[if(1<2)window.ok=true;]]></script></svg>"#;
    let html = wrap_svg(svg).unwrap();
    let decoded = svg_in(&html);
    assert!(decoded.contains("<![CDATA[if(1<2)window.ok=true;]]>"));
    assert!(decoded.contains(":root{width:100% !important;height:100% !important}"));
    roxmltree::Document::parse(&decoded).unwrap();
    assert!(html.contains("connect-src 'none'"));
}

#[test]
fn project_custom_js_requires_explicit_trust() {
    let input = Input::new(r#"{"customJs":"window.TEST_MARKER=123;"}"#, "json");
    let safe = prepare(&input.0, ViewerOptions::default()).unwrap();
    let trusted = prepare(
        &input.0,
        ViewerOptions {
            trusted_js: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(safe.custom_js_disabled);
    assert!(!trusted.custom_js_disabled);
    assert!(!svg_in(&safe.html).contains("new Function('forma'"));
    assert!(svg_in(&trusted.html).contains("new Function('forma'"));
    assert!(svg_in(&safe.html).contains("forma.audio.unlock()"));
    assert!(svg_in(&safe.html).contains("const speech=false, controls=true;"));
}

#[test]
fn frm_is_self_contained_and_matches_forma_json() {
    let mut project =
        forma_document::Project::from_json(include_str!("fixtures/native-smoke.forma.json"))
            .unwrap();
    project.clips = forma_document::Project::from_json(
        r#"{"nodes":[{"id":"button","kind":{"type":"rect","x":100,"y":100,"width":200,"height":100,"rx":0}}],"clips":[{"id":"move","duration":1,"autoplay":true,"tracks":[{"nodeId":"button","property":"x","keyframes":[{"time":0,"value":{"type":"number","value":100}},{"time":1,"value":{"type":"number","value":120}}]}]}]}"#,
    )
    .unwrap()
    .clips;
    let json = project.to_json().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("authored.frm");
    std::fs::write(&path, &json).unwrap();
    let prepared = prepare(&path, ViewerOptions::default()).unwrap();
    assert!(!prepared.is_svg_input);
    assert_eq!(prepared.title, project.name);
    assert!(prepared.custom_js_disabled);
    let svg = svg_in(&prepared.html);
    let doc = roxmltree::Document::parse(&svg).unwrap();
    let embedded = doc
        .descendants()
        .find(|node| node.attribute("id") == Some("forma-project"))
        .unwrap()
        .text()
        .unwrap();
    assert_eq!(
        forma_document::Project::from_json(embedded).unwrap(),
        project
    );
    assert!(svg.contains("const speech=false, controls=true;"));
    assert!(svg.contains("forma.audio.unlock()"));
    assert!(!svg.contains("new Function('forma'"));
    // Preparation neither requires nor writes a sidecar SVG or any other file.
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    let json_path = dir.path().join("authored.forma.json");
    std::fs::write(&json_path, json).unwrap();
    assert_eq!(
        prepared.html,
        prepare(&json_path, ViewerOptions::default()).unwrap().html
    );
}

#[test]
fn frm_custom_js_requires_explicit_trust_even_with_speech() {
    let input = Input::new(r#"{"customJs":"window.FRM_MARKER=123;"}"#, "FRM");
    for trusted_js in [false, true] {
        let prepared = prepare(
            &input.0,
            ViewerOptions {
                trusted_js,
                speech: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!prepared.is_svg_input);
        assert_eq!(prepared.custom_js_disabled, !trusted_js);
        let svg = svg_in(&prepared.html);
        assert!(svg.contains("window.FRM_MARKER=123;"));
        assert_eq!(svg.contains("new Function('forma'"), trusted_js);
        assert!(svg.contains("const speech=true, controls=true;"));
    }
}

#[test]
fn frm_rejects_noncanonical_or_invalid_project_schema() {
    for bad in [
        "{bad}",
        "[]",
        r#"{"unknown":true}"#,
        r#"{"name":"first","name":"second"}"#,
        r#"{"nodes":[{"id":"bad","kind":{"type":"rect","unknown":1}}]}"#,
        r#"{"width":-1}"#,
        r#"{"customJs":42}"#,
        r#"<svg xmlns="http://www.w3.org/2000/svg"/>"#,
    ] {
        let input = Input::new(bad, "frm");
        assert!(
            prepare(&input.0, ViewerOptions::default()).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn force_project_and_speech_options() {
    let input = Input::new("{}", "txt");
    let prepared = prepare(
        &input.0,
        ViewerOptions {
            project: true,
            speech: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!prepared.is_svg_input);
    assert!(svg_in(&prepared.html).contains("const speech=true, controls=true;"));
}

#[test]
fn rejects_invalid_xml_json_non_svg_and_missing_files() {
    for bad in ["<svg>", "<html/>", "<svg/>", "not SVG"] {
        assert!(wrap_svg(bad).is_err(), "{bad}");
    }
    let input = Input::new("{bad}", "json");
    assert!(prepare(&input.0, ViewerOptions::default()).is_err());
    assert!(prepare(&input.0.with_extension("missing"), ViewerOptions::default()).is_err());
}

#[test]
fn bounded_read_and_utf8() {
    let input = Input::new("", "svg");
    std::fs::OpenOptions::new()
        .write(true)
        .open(&input.0)
        .unwrap()
        .set_len(MAX_INPUT_BYTES + 1)
        .unwrap();
    let result = prepare(&input.0, ViewerOptions::default());
    assert!(match result {
        Err(e) => e.to_string().contains("32 MiB"),
        Ok(_) => false,
    });
    std::fs::write(&input.0, [0xff]).unwrap();
    assert!(prepare(&input.0, ViewerOptions::default()).is_err());
}

#[test]
fn empty_and_prefixed_svg_work() {
    wrap_svg(r#"<svg xmlns="http://www.w3.org/2000/svg"/>"#).unwrap();
    let prefixed =
        wrap_svg(r#"<s:svg xmlns:s="http://www.w3.org/2000/svg"><s:rect/></s:svg>"#).unwrap();
    roxmltree::Document::parse(&svg_in(&prefixed)).unwrap();
}
