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
