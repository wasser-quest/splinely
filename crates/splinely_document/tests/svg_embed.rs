//! SVG metadata embedding: binary (`data-encoding="sply-base64"`) and legacy JSON.
use splinely_document::{BrowserExportOptions, EmbedFormat, Node, Project};
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn metadata_attr(svg: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(svg).unwrap();
    let m = doc
        .descendants()
        .find(|n| n.has_tag_name("metadata") && n.attribute("id") == Some("splinely-project"))
        .unwrap();
    m.attribute("data-encoding").map(str::to_owned)
}
fn sample() -> Project {
    let mut p = Project {
        name: "embed <&> \"q\" 😀".into(),
        ..Project::default()
    };
    p.nodes.push(Node::rect("box", 1.0, 2.0, 30.0, 40.0));
    p.custom_js = "</metadata><script>]]> & \r\n \u{ffff}".into();
    p
}

#[test]
fn default_export_embeds_base64_binary_and_round_trips() {
    let p = sample();
    let svg = p.to_svg("/*rt*/").unwrap();
    assert_eq!(metadata_attr(&svg).as_deref(), Some("sply-base64"));
    assert!(
        !svg.contains("\"schemaVersion\""),
        "no JSON text in binary embed"
    );
    assert_eq!(Project::from_svg_metadata(&svg).unwrap(), p);
    assert_eq!(svg, p.to_svg_embed("/*rt*/", EmbedFormat::Binary).unwrap());
    assert_eq!(EmbedFormat::default(), EmbedFormat::Binary);
}

#[test]
fn json_embed_keeps_legacy_form_and_round_trips() {
    let p = sample();
    let svg = p.to_svg_embed("", EmbedFormat::Json).unwrap();
    assert_eq!(metadata_attr(&svg), None);
    assert!(svg.contains("schemaVersion"));
    assert_eq!(Project::from_svg_metadata(&svg).unwrap(), p);
}

#[test]
fn browser_options_export_honours_embed_and_uses_source_project() {
    let p = sample();
    let o = BrowserExportOptions {
        trusted_js: true,
        audio_controls: true,
    };
    let bin = p.to_svg_with_options("", o).unwrap();
    let json = p
        .to_svg_with_options_embed("", o, EmbedFormat::Json)
        .unwrap();
    assert_eq!(metadata_attr(&bin).as_deref(), Some("sply-base64"));
    assert_eq!(metadata_attr(&json), None);
    for svg in [&bin, &json] {
        assert!(svg.contains("splinely.sourceProject||"));
        assert!(!svg.contains("JSON.parse(svg.querySelector"));
        assert_eq!(Project::from_svg_metadata(svg).unwrap(), p);
    }
}

#[test]
fn metadata_reader_rejects_bad_encodings() {
    let p = sample();
    let svg = p.to_svg("").unwrap();
    let unknown = svg.replace("sply-base64", "zip");
    assert!(Project::from_svg_metadata(&unknown)
        .unwrap_err()
        .to_string()
        .contains("data-encoding"));
    // Corrupt one base64 character of the payload: CRC/structure check must reject it.
    let start = svg.find("sply-base64\">").unwrap() + "sply-base64\">".len();
    let mut bytes = svg.clone().into_bytes();
    bytes[start + 60] = if bytes[start + 60] == b'A' {
        b'B'
    } else {
        b'A'
    };
    assert!(Project::from_svg_metadata(&String::from_utf8(bytes).unwrap()).is_err());
    assert!(
        Project::from_svg_metadata(&svg.replace("sply-base64\">", "sply-base64\">!!")).is_err()
    );
    assert!(Project::from_svg_metadata("<svg xmlns='http://www.w3.org/2000/svg'/>").is_err());
}

#[test]
fn desert_stickfight_binary_embed_is_smaller_than_json_embed() {
    let text =
        std::fs::read_to_string(root().join("examples/desert-stickfight.splinely.json")).unwrap();
    let p = Project::from_json(&text).unwrap();
    let json = p.to_svg_embed("", EmbedFormat::Json).unwrap();
    let bin = p.to_svg_embed("", EmbedFormat::Binary).unwrap();
    assert_eq!(Project::from_svg_metadata(&bin).unwrap(), p);
    assert_eq!(Project::from_svg_metadata(&json).unwrap(), p);
    eprintln!(
        "desert-stickfight SVG (no runtime): json={} binary={}",
        json.len(),
        bin.len()
    );
    assert!(
        bin.len() < json.len() / 2,
        "binary {} vs json {}",
        bin.len(),
        json.len()
    );
}

/// Insert `extra` right after the root `<svg ...>` start tag (before the real block).
fn with_prefix(svg: &str, extra: &str) -> String {
    let at = svg.find(">\n").unwrap() + 2;
    format!("{}{}{}", &svg[..at], extra, &svg[at..])
}

#[test]
fn metadata_lookup_ignores_decoys_like_the_js_runtime() {
    let p = sample();
    let svg = p.to_svg("").unwrap();
    for (name, decoy) in [
        (
            "other namespace",
            r#"<x:metadata xmlns:x="urn:x" id="splinely-project" data-encoding="sply-base64">garbage</x:metadata>"#,
        ),
        (
            "nested svg",
            r#"<svg><metadata id="splinely-project">{garbage</metadata></svg>"#,
        ),
        (
            "nested in g",
            r#"<g><metadata id="splinely-project">{garbage</metadata></g>"#,
        ),
        (
            "foreignObject",
            r#"<foreignObject><div xmlns="http://www.w3.org/1999/xhtml" id="splinely-project">{garbage</div></foreignObject>"#,
        ),
        (
            "no namespace",
            r#"<metadata xmlns="" id="splinely-project">{garbage</metadata>"#,
        ),
    ] {
        let tampered = with_prefix(&svg, decoy);
        assert_eq!(
            Project::from_svg_metadata(&tampered).unwrap(),
            p,
            "decoy: {name}"
        );
    }
}

#[test]
fn metadata_lookup_rejects_multiple_candidates() {
    let p = sample();
    let svg = p.to_svg("").unwrap();
    for extra in [
        r#"<metadata id="splinely-project">{}</metadata>"#,
        r#"<desc data-splinely-project="">{}</desc>"#,
    ] {
        let err = Project::from_svg_metadata(&with_prefix(&svg, extra)).unwrap_err();
        assert!(
            err.to_string().contains("Multiple splinely-project"),
            "{err}"
        );
    }
    // The `data-splinely-project` attribute alone is a valid block (JS parity).
    let json = p.to_svg_embed("", EmbedFormat::Json).unwrap();
    let attr = json.replace(
        "<metadata id=\"splinely-project\">",
        "<metadata data-splinely-project=\"\">",
    );
    assert_eq!(Project::from_svg_metadata(&attr).unwrap(), p);
    // A root without the SVG namespace has no candidates.
    let plain = svg.replace(" xmlns=\"http://www.w3.org/2000/svg\"", "");
    assert!(Project::from_svg_metadata(&plain)
        .unwrap_err()
        .to_string()
        .contains("Missing splinely-project"));
}

#[test]
fn metadata_text_is_bounded_before_cleaning_and_decoding() {
    let p = sample();
    let svg = p.to_svg("").unwrap();
    let start = svg.find("sply-base64\">").unwrap() + "sply-base64\">".len();
    let end = svg[start..].find("</metadata>").unwrap() + start;
    // base64 of the 64 MiB limit is 89_478_488 chars; one more group must be refused up front.
    let huge = "A".repeat(89_478_488 + 4);
    let tampered = format!("{}{}{}", &svg[..start], huge, &svg[end..]);
    let err = Project::from_svg_metadata(&tampered).unwrap_err();
    assert!(err.to_string().contains("exceeds 64 MiB"), "{err}");
}

#[test]
fn metadata_rejects_non_canonical_base64() {
    // Grow the name until the payload ends in padding (file length % 3 != 0).
    let (svg, start, end) = (0..3)
        .find_map(|pad| {
            let mut p = sample();
            p.name.push_str(&"x".repeat(pad));
            let svg = p.to_svg("").unwrap();
            let start = svg.find("sply-base64\">").unwrap() + "sply-base64\">".len();
            let end = svg[start..].find("</metadata>").unwrap() + start;
            svg[start..end].ends_with('=').then_some((svg, start, end))
        })
        .expect("some name length yields padded base64");
    let text = &svg[start..end];
    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let two = text.ends_with("==");
    let pos = text.len() - if two { 3 } else { 2 };
    let digit = alphabet.find(&text[pos..pos + 1]).unwrap();
    // Same decoded bytes, but non-zero unused trailing bits.
    let forged = format!(
        "{}{}{}",
        &text[..pos],
        &alphabet[digit | 1..(digit | 1) + 1],
        &text[pos + 1..]
    );
    let tampered = format!("{}{}{}", &svg[..start], forged, &svg[end..]);
    assert!(Project::from_svg_metadata(&svg).is_ok());
    assert!(Project::from_svg_metadata(&tampered)
        .unwrap_err()
        .to_string()
        .contains("not canonical base64"));
}

#[test]
fn inline_export_without_scripts_keeps_metadata_and_host_attributes() {
    use splinely_document::SvgOptions;
    let p = sample();
    let svg = p
        .to_svg_with(&SvgOptions {
            max_fps: Some(30),
            native_animations: Some(false),
            ..Default::default()
        })
        .unwrap();
    assert!(!svg.contains("<script"), "{svg}");
    let doc = roxmltree::Document::parse(&svg).unwrap();
    let root = doc.root_element();
    assert_eq!(root.attribute("data-splinely-max-fps"), Some("30"));
    assert_eq!(
        root.attribute("data-splinely-native-animations"),
        Some("false")
    );
    assert_eq!(metadata_attr(&svg).as_deref(), Some("sply-base64"));
    assert_eq!(Project::from_svg_metadata(&svg).unwrap(), p);
    // Without host options the document is the scripted export minus its script.
    let plain = p.to_svg_with(&SvgOptions::default()).unwrap();
    let scripted = p.to_svg("/*rt*/").unwrap();
    let script = "<script type=\"application/ecmascript\"><![CDATA[/*rt*/]]></script>\n";
    assert_eq!(scripted.replace(script, ""), plain);
    assert!(p
        .to_svg_with(&SvgOptions {
            max_fps: Some(0),
            ..Default::default()
        })
        .is_err());
}

#[test]
fn svg_from_bytes_matches_load_then_export() {
    use splinely_document::SvgOptions;
    let mut files = vec![];
    for dir in [
        "examples",
        "crates/splinely_document/tests/fixtures/sply/valid",
    ] {
        for entry in std::fs::read_dir(root().join(dir)).unwrap() {
            let path = entry.unwrap().path();
            let name = path.to_string_lossy().into_owned();
            if name.ends_with(".sply") || name.ends_with(".splinely.json") {
                files.push(path);
            }
        }
    }
    assert!(files.len() > 5);
    // Re-encoded vs. embedded as-is: identical whenever the bytes are canonical,
    // otherwise (defaults omitted, legacy data) the export re-encodes.
    let mut broken = Project::default().to_binary().unwrap();
    let mut legacy = serde_json::to_value(sample()).unwrap();
    legacy["speechCues"] = serde_json::json!([]);
    files.push(PathBuf::new());
    for path in &files {
        let bytes = if path.as_os_str().is_empty() {
            splinely_document::encode_value(&legacy).unwrap()
        } else {
            std::fs::read(path).unwrap()
        };
        let options = SvgOptions {
            runtime: Some("/*rt*/"),
            ..Default::default()
        };
        let expected = Project::from_bytes(&bytes).and_then(|p| p.to_svg("/*rt*/"));
        let actual = Project::svg_from_bytes(&bytes, &options);
        match (expected, actual) {
            (Ok(a), Ok(b)) => assert_eq!(a, b, "{path:?}"),
            (Err(a), Err(b)) => assert_eq!(a.0, b.0, "{path:?}"),
            (a, b) => panic!("{path:?}: {a:?} vs {b:?}"),
        }
    }
    broken.push(0);
    assert!(Project::svg_from_bytes(&broken, &SvgOptions::default()).is_err());
}

#[test]
fn oversized_input_is_rejected_before_parsing() {
    let limit = splinely_document::MAX_PROJECT_FILE_BYTES as usize;
    let mut json = String::with_capacity(limit + 1);
    json.push_str("{\"schemaVersion\":1,\"junk\":\"");
    json.extend(std::iter::repeat_n('x', limit - json.len()));
    json.push_str("\"}");
    let start = std::time::Instant::now();
    assert!(Project::from_json(&json).unwrap_err().0.contains("64 MiB"));
    assert!(Project::from_bytes(json.as_bytes())
        .unwrap_err()
        .0
        .contains("64 MiB"));
    assert!(start.elapsed().as_secs_f64() < 1.0);
}
