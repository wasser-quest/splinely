//! SVG import report: text, gradients, data-URI images, hostile input limits, source sync.
use splinely_document::*;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/svg/{name}.svg",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
fn svg(body: &str) -> String {
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">{body}</svg>"##)
}
fn ok(source: &str) -> ImportOutcome {
    Project::from_svg_report(source)
        .unwrap_or_else(|e| panic!("{}: {:?}", e.error, e.report.diagnostics))
}
fn fail(source: &str) -> ImportFailure {
    match Project::from_svg_report(source) {
        Ok(_) => panic!("import should fail"),
        Err(f) => f,
    }
}
fn has(report: &ImportReport, status: ImportStatus, code: &str) -> bool {
    report
        .diagnostics
        .iter()
        .any(|d| d.status == status && d.code == code)
}
fn text_nodes(p: &Project) -> Vec<(String, f64, f64, f64)> {
    p.nodes
        .iter()
        .filter_map(|n| match &n.kind {
            NodeKind::Text {
                x,
                y,
                text,
                font_size,
            } => Some((text.clone(), *x, *y, *font_size)),
            _ => None,
        })
        .collect()
}
fn b64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in bytes.chunks(3) {
        let v = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(v >> 18) as usize & 63] as char);
        s.push(T[(v >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 {
            T[(v >> 6) as usize & 63] as char
        } else {
            '='
        });
        s.push(if c.len() > 2 {
            T[v as usize & 63] as char
        } else {
            '='
        });
    }
    s
}
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}
/// Structurally valid PNG (IHDR + IDAT + IEND with correct CRCs); pixels need not decode.
fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(crc32(&body).to_be_bytes());
    };
    let mut ihdr = vec![];
    ihdr.extend(w.to_be_bytes());
    ihdr.extend(h.to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &[0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01]);
    chunk(b"IEND", &[]);
    out
}
fn jpeg(w: u16, h: u16) -> Vec<u8> {
    let mut v = vec![0xff, 0xd8, 0xff, 0xc0, 0x00, 0x0b, 8];
    v.extend(h.to_be_bytes());
    v.extend(w.to_be_bytes());
    v.extend([1, 1, 0x11, 0]);
    v.extend([
        0xff, 0xda, 0x00, 0x08, 1, 1, 0, 0, 0x3f, 0, 0x12, 0x34, 0xff, 0xd9,
    ]);
    v
}
fn image(href: &str) -> String {
    svg(&format!(
        r##"<rect width="5" height="5"/><image id="im" x="1" y="2" width="30" height="20" href="{href}"/>"##
    ))
}

#[test]
fn inkscape_like_file_reports_dropped_editor_metadata() {
    let out = ok(&fixture("inkscape"));
    let p = &out.project;
    assert_eq!(p.name, "Inkscape poster");
    assert_eq!(p.nodes.len(), 4); // rect, path, two tspan lines
    let t = text_nodes(p);
    assert_eq!(t.len(), 2);
    assert_eq!(t[0].0, "Hello Inkscape");
    assert_eq!((t[0].1, t[0].2), (20., 150.));
    assert_eq!(t[1].0, "second line");
    assert!((t[1].2 - 163.229).abs() < 1e-9);
    // viewBox in mm: text keeps local coordinates, the transform carries the scale
    assert!((p.nodes[2].transform.scale_x - 96. / 25.4).abs() < 1e-4);
    let g = p.nodes[0].gradient.as_ref().unwrap();
    assert_eq!((g.radial, g.angle, g.stops.len()), (false, 0., 2));
    let r = &out.report;
    assert!(has(r, ImportStatus::Dropped, "foreign-element")); // sodipodi:namedview, inkscape:perspective
    assert!(has(r, ImportStatus::Dropped, "foreign-attribute"));
    assert!(has(r, ImportStatus::Dropped, "text-font-family"));
    assert!(!has(r, ImportStatus::Rejected, "x"));
    assert_eq!(
        r.imported,
        r.diagnostics
            .iter()
            .filter(|d| d.status == ImportStatus::Imported)
            .map(|d| d.count)
            .sum::<usize>()
    );
    assert!(r.dropped > 0 && r.rejected == 0);
    // Stored source text and visual document start in sync.
    assert_eq!(p.source_sync(), SourceSync::InSync);
}

#[test]
fn figma_like_gradients_and_text_map_onto_the_model() {
    let out = ok(&fixture("figma_gradient_text"));
    let p = &out.project;
    let card = p.nodes.iter().find(|n| n.id == "Card").unwrap();
    let g = card.gradient.as_ref().unwrap();
    assert!(!g.radial);
    assert_eq!(g.angle, 45.);
    assert_eq!(g.stops.len(), 3);
    assert_eq!(g.stops[0].offset, 0.);
    assert_eq!(g.stops[1].offset, 0.5);
    // The model's gradient vector has length 1 around the box centre, so a corner-to-corner
    // diagonal is clipped: the end stops are re-interpolated, the middle stop is exact.
    assert!((g.stops[1].color.r - 0x2c as f64 / 255.).abs() < 1e-9);
    assert!(has(
        &out.report,
        ImportStatus::Approximated,
        "gradient-range"
    ));
    // userSpaceOnUse corner-to-corner vector on a non-square box cannot be exact
    assert!(has(
        &out.report,
        ImportStatus::Approximated,
        "gradient-skew"
    ));
    let glow = p.nodes.iter().find(|n| n.id == "Glow").unwrap();
    let rg = glow.gradient.as_ref().unwrap();
    assert!(rg.radial);
    assert_eq!((rg.stops[0].color.a, rg.stops[1].color.a), (1., 0.));
    assert_eq!((rg.stops[0].offset, rg.stops[1].offset), (0., 1.));
    let t = text_nodes(p);
    assert_eq!(t[0], ("Gradient card".into(), 30., 50.5, 24.));
    // text-anchor: middle -> x shifted by half the estimated width (14 chars * 0.6 * 16)
    assert_eq!(t[1].0, "Centered label");
    assert!((t[1].1 - (200. - 14. * 0.6 * 16. / 2.)).abs() < 1e-9);
    assert!(has(&out.report, ImportStatus::Approximated, "text-anchor"));
    assert!(has(&out.report, ImportStatus::Dropped, "text-font-style")); // font-weight: bold
    assert!(out
        .report
        .diagnostics
        .iter()
        .all(|d| d.status != ImportStatus::Rejected));
}

#[test]
fn gradient_href_inheritance_units_and_unsupported_parts() {
    let body = r##"<defs>
      <linearGradient id="base"><stop offset="0" stop-color="#f00"/><stop offset=".25" stop-color="#0f0"/><stop offset="1" stop-color="#00f"/></linearGradient>
      <linearGradient id="vertical" href="#base" x1="0" y1="0" x2="0" y2="1"/>
      <radialGradient id="rad" href="#base" cx=".5" cy=".5" r=".5" spreadMethod="reflect" fx=".3"/>
      <linearGradient id="skewed" href="#base" gradientTransform="rotate(30) skewX(20)"/>
    </defs>
    <rect id="a" width="40" height="20" fill="url(#vertical)"/>
    <circle id="b" cx="70" cy="70" r="20" fill="url(#rad)" stroke="url(#base)" stroke-width="2"/>
    <rect id="c" y="30" width="40" height="20" fill="url(#skewed)"/>"##;
    let out = ok(&svg(body));
    let a = out.project.nodes[0].gradient.as_ref().unwrap();
    assert_eq!(a.angle, 90.);
    assert_eq!(a.stops.len(), 3, "stops are inherited through href");
    assert_eq!(a.stops[1].offset, 0.25);
    let b = &out.project.nodes[1];
    assert!(b.gradient.as_ref().unwrap().radial);
    assert!(b.style.stroke.is_some());
    for code in [
        "gradient-spread",
        "gradient-focal",
        "gradient-stroke",
        "gradient-skew",
    ] {
        assert!(has(&out.report, ImportStatus::Approximated, code), "{code}");
    }
    // unknown reference is rejected, never silently solid
    assert!(
        Project::from_svg_report(&svg(r##"<rect width="9" height="9" fill="url(#none)"/>"##))
            .is_err()
    );
    // patterns are not gradients
    let f = fail(&svg(
        r##"<pattern id="p"/><rect width="9" height="9" fill="url(#p)"/>"##,
    ));
    assert!(has(
        &f.report,
        ImportStatus::Rejected,
        "element-unsupported"
    ));
    let f = fail(&svg(r##"<rect width="9" height="9" fill="url(#p)"/>"##));
    assert!(has(&f.report, ImportStatus::Rejected, "url-unsupported"));
}

#[test]
fn text_tspans_whitespace_and_positions() {
    let body = r##"<text x="10 20 30" y="40" font-size="2em" xml:space="default" fill="#336699">
        Hello   <tspan>wor</tspan>ld
        <tspan x="5" dy="10">next</tspan><tspan font-size="30">big</tspan></text>
      <text id="empty">   </text><text id="rot" x="1" y="9" rotate="10 20" text-decoration="underline">r</text>"##;
    let out = ok(&svg(body));
    let t = text_nodes(&out.project);
    assert_eq!(t[0], ("Hello world".into(), 10., 40., 24.));
    assert_eq!((t[1].0.as_str(), t[1].1, t[1].2), ("nextbig", 5., 50.));
    assert_eq!(t.len(), 3);
    assert_eq!(
        out.project.nodes[0].style.fill,
        Color {
            r: 0.2,
            g: 0.4,
            b: 0.6,
            a: 1.
        }
    );
    for (s, c) in [
        (ImportStatus::Approximated, "text-glyph-positions"),
        (ImportStatus::Approximated, "text-tspan-style"),
        (ImportStatus::Dropped, "text-empty"),
        (ImportStatus::Dropped, "text-rotate"),
        (ImportStatus::Dropped, "text-decoration"),
    ] {
        assert!(has(&out.report, s, c), "{c}: {:?}", out.report.diagnostics);
    }
    // limits
    let long = "x".repeat(70_000);
    let f = fail(&svg(&format!("<text>{long}</text>")));
    assert!(has(&f.report, ImportStatus::Rejected, "text-limit"));
}

#[test]
fn raster_data_uris_are_validated_then_reported_as_dropped() {
    let p = ok(&image(&format!(
        "data:image/png;base64,{}",
        b64(&png(40, 30))
    )));
    assert_eq!(
        p.project.nodes.len(),
        1,
        "raster pixels are not faked as nodes"
    );
    let d = p
        .report
        .diagnostics
        .iter()
        .find(|d| d.code == "image-raster")
        .unwrap();
    assert_eq!(
        (d.status, d.id.as_deref()),
        (ImportStatus::Dropped, Some("im"))
    );
    assert!(d.message.contains("40x30") && d.message.contains("no image node"));
    let j = ok(&image(&format!(
        "data:image/jpeg;base64,{}",
        b64(&jpeg(7, 5))
    )));
    assert!(j
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == "image-raster" && d.message.contains("7x5")));
}

#[test]
fn bad_or_external_images_cancel_the_import_with_a_diagnostic() {
    let mut bad_crc = png(4, 4);
    bad_crc[20] ^= 0xff;
    let mut huge_dims = png(20_000, 20_000);
    let cases: Vec<(String, &str)> = vec![
        ("https://example.test/a.png".into(), "image-external"),
        ("file:///etc/passwd".into(), "image-external"),
        ("a.png".into(), "image-external"),
        ("//example.test/a.png".into(), "image-external"),
        (
            format!("data:image/png;base64,{}", b64(&bad_crc)),
            "image-malformed",
        ),
        (
            format!("data:image/png;base64,{}", b64(&huge_dims)),
            "image-too-large",
        ),
        (
            format!("data:image/png;base64,{}", b64(&jpeg(2, 2))),
            "image-malformed",
        ),
        (
            "data:image/gif;base64,R0lGODlhAQABAAAAACw=".into(),
            "image-unsupported-type",
        ),
        ("data:image/png;base64,!!!".into(), "image-malformed"),
        ("data:image/png,rawbytes".into(), "image-malformed"),
        ("data:image/png;base64".into(), "image-malformed"),
    ];
    huge_dims.clear();
    for (href, code) in cases {
        let f = fail(&image(&href));
        assert!(
            has(&f.report, ImportStatus::Rejected, code),
            "{href}: {:?}",
            f.report.diagnostics
        );
    }
    // size limits: > 3 MiB of image data, > 32 images, > 8 MiB total
    let mut big = png(10, 10);
    big.extend(vec![0u8; 3 * 1024 * 1024]);
    // (base64 of > 3 MiB is also over the 4 MiB source limit; either limit stops it)
    let f = fail(&image(&format!("data:image/png;base64,{}", b64(&big))));
    assert!(
        has(&f.report, ImportStatus::Rejected, "limit-source")
            || has(&f.report, ImportStatus::Rejected, "image-too-large")
    );
    let tiny = format!("data:image/png;base64,{}", b64(&png(2, 2)));
    let many: String = (0..33)
        .map(|i| format!(r##"<image id="i{i}" width="1" height="1" href="{tiny}"/>"##))
        .collect();
    let f = fail(&svg(&many));
    assert!(has(&f.report, ImportStatus::Rejected, "limit-images"));
    let mut two_mib = png(10, 10);
    two_mib.extend(vec![0u8; 2_500_000]);
    let one = format!("data:image/png;base64,{}", b64(&two_mib));
    let total: String = (0..4)
        .map(|i| format!(r##"<image id="t{i}" width="1" height="1" href="{one}"/>"##))
        .collect();
    // 4 * 2.5 MB > 8 MiB source limit as well; either limit must stop it
    assert!(Project::from_svg_report(&svg(&total)).is_err());
}

#[test]
fn svg_data_uri_images_become_vector_groups_and_nest_only_once() {
    let out = ok(&fixture("data_uri_image"));
    let p = &out.project;
    assert!(p.groups.iter().any(|g| g.id == "badge"));
    let dot = p.nodes.iter().find(|n| n.id == "badge-dot").unwrap();
    assert_eq!(dot.group.as_deref(), Some("badge"));
    let NodeKind::Path { bounds, .. } = &dot.kind else {
        panic!()
    };
    assert_eq!(*bounds, [65., 15., 40., 40.]);
    assert!(has(&out.report, ImportStatus::Dropped, "image-raster"));
    assert!(has(&out.report, ImportStatus::Approximated, "image-clip"));
    // An image inside a nested SVG image is dropped, not followed.
    let inner = "<svg xmlns='http://www.w3.org/2000/svg' width='4' height='4'><rect width='4' height='4'/><image width='2' height='2' href='http://x.test/a.png'/></svg>";
    let outer = format!("data:image/svg+xml;base64,{}", b64(inner.as_bytes()));
    let o = ok(&svg(&format!(
        r##"<image width="20" height="20" href="{outer}"/>"##
    )));
    assert!(has(
        &o.report,
        ImportStatus::Dropped,
        "image-nested-dropped"
    ));
    // Nested SVG with unsupported appearance fails the whole import.
    let bad =
        "<svg xmlns='http://www.w3.org/2000/svg'><rect width='4' height='4' filter='blur'/></svg>";
    let f = fail(&svg(&format!(
        r##"<image width="20" height="20" href="data:image/svg+xml;base64,{}"/>"##,
        b64(bad.as_bytes())
    )));
    assert!(f.report.rejected > 0);
    // Nested SVG too large (> 512 KiB).
    let pad = format!(
        "<svg xmlns='http://www.w3.org/2000/svg'><!--{}--><rect width='1' height='1'/></svg>",
        "x".repeat(600_000)
    );
    let f = fail(&svg(&format!(
        r##"<image width="20" height="20" href="data:image/svg+xml;base64,{}"/>"##,
        b64(pad.as_bytes())
    )));
    assert!(has(&f.report, ImportStatus::Rejected, "image-too-large"));
}

#[test]
fn hostile_inputs_hit_explicit_limits() {
    // billion laughs through <use>
    let mut s = String::from(r##"<defs><rect id="l0" width="1" height="1"/>"##);
    for i in 1..=12 {
        s.push_str(&format!(r##"<g id="l{i}">"##));
        for _ in 0..10 {
            s.push_str(&format!(r##"<use href="#l{}"/>"##, i - 1));
        }
        s.push_str("</g>");
    }
    s.push_str(r##"</defs><use href="#l12"/>"##);
    let f = fail(&svg(&s));
    assert!(
        has(&f.report, ImportStatus::Rejected, "limit-expansion"),
        "{:?}",
        f.report.diagnostics
    );
    // cyclic use
    let f = fail(&svg(r##"<g id="a"><use href="#a"/></g><use href="#a"/>"##));
    assert!(has(&f.report, ImportStatus::Rejected, "use-cycle"));
    // external use
    let f = fail(&svg(r##"<use href="other.svg#x"/>"##));
    assert!(has(&f.report, ImportStatus::Rejected, "reference-external"));
    // bounded use still works
    let o = ok(&svg(
        r##"<defs><rect id="r" width="5" height="5"/></defs><use href="#r"/><use href="#r" x="10"/>"##,
    ));
    assert_eq!(o.project.nodes.len(), 2);
    // stops
    let stops: String = (0..300)
        .map(|i| {
            format!(
                r##"<stop offset="{}" stop-color="#000"/>"##,
                i as f64 / 300.
            )
        })
        .collect();
    let f = fail(&svg(&format!(
        r##"<defs><linearGradient id="g">{stops}</linearGradient></defs><rect width="9" height="9" fill="url(#g)"/>"##
    )));
    assert!(has(&f.report, ImportStatus::Rejected, "limit-stops"));
    // gradient count
    let many: String = (0..513)
        .map(|i| format!(r##"<linearGradient id="g{i}"/>"##))
        .collect();
    let f = fail(&svg(&format!("<defs>{many}</defs>")));
    assert!(has(&f.report, ImportStatus::Rejected, "limit-gradients"));
    // href chain cycle and chain length
    let f = fail(&svg(
        r##"<defs><linearGradient id="a" href="#b"/><linearGradient id="b" href="#a"/></defs><rect width="9" height="9"/>"##,
    ));
    assert!(has(&f.report, ImportStatus::Rejected, "gradient-cycle"));
    let chain: String = (0..70)
        .map(|i| format!(r##"<linearGradient id="c{i}" href="#c{}"/>"##, i + 1))
        .collect();
    let f = fail(&svg(&format!(
        "<defs>{chain}<linearGradient id=\"c70\"/></defs><rect width=\"9\" height=\"9\"/>"
    )));
    assert!(has(
        &f.report,
        ImportStatus::Rejected,
        "limit-gradient-chain"
    ));
    // nesting depth
    let deep = format!(
        "{}<rect width='9' height='9'/>{}",
        "<g>".repeat(200),
        "</g>".repeat(200)
    );
    let f = fail(&svg(&deep));
    assert!(has(&f.report, ImportStatus::Rejected, "limit-depth"));
    // element count and source size
    let flat = "<rect width='1' height='1'/>".repeat(10_001);
    assert!(has(
        &fail(&svg(&flat)).report,
        ImportStatus::Rejected,
        "limit-elements"
    ));
    let big = svg(&format!(
        "<!--{}--><rect width='1' height='1'/>",
        "x".repeat(4 * 1024 * 1024)
    ));
    assert!(has(
        &fail(&big).report,
        ImportStatus::Rejected,
        "limit-source"
    ));
    // DTD entities
    assert!(Project::from_svg_report(
        "<!DOCTYPE svg [<!ENTITY a 'x'>]><svg xmlns='http://www.w3.org/2000/svg'/>"
    )
    .is_err());
}

#[test]
fn malformed_input_fails_without_a_partial_document() {
    let f = fail(&fixture("malformed"));
    assert!(has(&f.report, ImportStatus::Rejected, "svg-malformed"));
    assert_eq!(f.report.imported, 0);
    assert!(Project::from_svg(&fixture("malformed")).is_err());
    for body in [
        r##"<rect width="9" height="9" style="fill:red;behavior:url(x)"/>"##,
        r##"<rect width="9" height="9" onload="x()"/>"##,
        r##"<a href="https://x.test"><rect width="9" height="9"/></a>"##,
        r##"<rect width="9" height="9" stroke-dasharray="3 1" stroke="#000"/>"##,
        r##"<image href="data:text/html;base64,PGI+"/>"##,
    ] {
        let f = fail(&svg(body));
        assert!(f.report.rejected >= 1, "{body}");
    }
}

#[test]
fn imported_gradients_and_text_survive_export_and_reimport() {
    let first = ok(&fixture("figma_gradient_text"));
    let exported = first.project.to_static_svg(0.).unwrap();
    let second = ok(&exported);
    let key = |p: &Project| {
        p.nodes
            .iter()
            .filter_map(|n| n.gradient.as_ref())
            .map(|g| {
                (
                    g.radial,
                    (g.angle * 10.).round(),
                    g.stops
                        .iter()
                        .map(|s| {
                            (
                                (s.offset * 100.).round(),
                                (s.color.r * 255.).round(),
                                (s.color.a * 100.).round(),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(key(&first.project), key(&second.project));
    let (a, b) = (text_nodes(&first.project), text_nodes(&second.project));
    assert_eq!(a.len(), 2);
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.0, y.0);
        assert!(
            (x.1 - y.1).abs() < 1e-3 && (x.2 - y.2).abs() < 1e-3 && x.3 == y.3,
            "{x:?} {y:?}"
        );
    }
}

#[test]
fn source_sync_detects_stale_text_and_visual_edits() {
    let mut p = ok(&fixture("figma_gradient_text")).project;
    assert_eq!(p.source_sync(), SourceSync::InSync);
    assert!(!p.source_sync().is_conflict());
    p.nodes[0].transform.x += 5.;
    assert_eq!(p.source_sync(), SourceSync::VisualEdited);
    p.raw_svg.push_str("<!-- edited -->");
    assert_eq!(p.source_sync(), SourceSync::Both);
    p.accept_svg_source_state();
    assert_eq!(p.source_sync(), SourceSync::InSync);
    let mut q = ok(&fixture("figma_gradient_text")).project;
    q.raw_svg.push(' ');
    assert_eq!(q.source_sync(), SourceSync::SourceEdited);
    // baseline survives save/load; projects without one report NoBaseline
    let r = Project::from_json(&q.to_json().unwrap()).unwrap();
    assert_eq!(r.source_sync(), SourceSync::SourceEdited);
    assert_eq!(Project::default().source_sync(), SourceSync::NoBaseline);
}

#[test]
fn report_serialises_with_stable_fields() {
    let out = ok(&fixture("inkscape"));
    let j = out.report.to_json();
    assert!(j["imported"].as_u64().unwrap() > 0);
    let d = &j["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["status"] == "dropped")
        .unwrap();
    for k in ["status", "code", "element", "message", "count"] {
        assert!(d.get(k).is_some(), "{k}");
    }
}
