//! `inspect` as a textual picture: scene, layout sentences, ASCII preview, animation, focus.
use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_splinely-cli"))
        .args(args)
        .output()
        .unwrap()
}
fn ok(args: &[&str]) -> Value {
    let out = run(args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// Background, peach head, two white eyes with brown pupils, a hidden shape
/// and a shape covered by the head; a clip on the head.
fn face(dir: &Path) -> std::path::PathBuf {
    let f = dir.join("face.sply");
    let p = s(&f);
    ok(&["new", "--output", p, "--width", "400", "--height", "300"]);
    let add = |args: &[&str]| {
        let mut all = vec!["node", "add", p];
        all.extend_from_slice(args);
        ok(&all);
    };
    add(&[
        "--kind", "rect", "--id", "bg", "--width", "400", "--height", "300", "--fill", "#0B1D3A",
    ]);
    add(&[
        "--kind", "circle", "--id", "behind", "--x", "200", "--y", "150", "--radius", "10",
        "--fill", "#00FF00",
    ]);
    add(&[
        "--kind", "circle", "--id", "head", "--x", "200", "--y", "150", "--radius", "100",
        "--fill", "#E09F70",
    ]);
    add(&[
        "--kind", "circle", "--id", "eye-l", "--x", "160", "--y", "120", "--radius", "20",
        "--fill", "#FFFFFF",
    ]);
    add(&[
        "--kind", "circle", "--id", "eye-r", "--x", "240", "--y", "120", "--radius", "20",
        "--fill", "#FFFFFF",
    ]);
    add(&[
        "--kind", "circle", "--id", "pupil-l", "--x", "160", "--y", "124", "--radius", "8",
        "--fill", "#6B3A1E",
    ]);
    add(&[
        "--kind", "circle", "--id", "pupil-r", "--x", "240", "--y", "124", "--radius", "8",
        "--fill", "#6B3A1E",
    ]);
    add(&[
        "--kind", "text", "--id", "ghost", "--x", "10", "--y", "20", "--text", "hi", "--fill",
        "#FFFFFF",
    ]);
    ok(&["node", "update", p, "ghost", "--opacity", "0"]);
    ok(&[
        "keyframe",
        p,
        "--clip",
        "nod",
        "--target",
        "head",
        "--property",
        "y",
        "--time",
        "0",
        "--value",
        "0",
        "--autoplay",
        "--loop",
    ]);
    ok(&[
        "keyframe",
        p,
        "--clip",
        "nod",
        "--target",
        "head",
        "--property",
        "y",
        "--time",
        "1",
        "--value",
        "10",
    ]);
    f
}

fn node<'a>(v: &'a Value, id: &str) -> &'a Value {
    v["scene"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == id)
        .unwrap_or_else(|| panic!("no node {id}"))
}

fn layout(v: &Value) -> Vec<String> {
    v["layout"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn inspect_describes_scene_layout_and_animation_in_words() {
    let dir = tempdir().unwrap();
    let f = face(dir.path());
    let v = ok(&["inspect", s(&f)]);
    // Older fields stay.
    assert_eq!(v["stats"]["nodes"], 8);
    assert_eq!(v["nodes"][0]["id"], "bg");
    assert!(v["file"]["bytes"].as_u64().unwrap() > 0);
    // Scene: draw order, colour words, world bounds.
    let scene = &v["scene"];
    assert_eq!(scene["canvas"]["width"], 400.0);
    assert!(scene["summary"].as_str().unwrap().contains("background"));
    assert_eq!(scene["nodes"][0]["id"], "bg");
    let head = node(&v, "head");
    assert_eq!(head["looksLike"], "peach circle");
    assert_eq!(
        head["bounds"],
        serde_json::json!([100.0, 50.0, 200.0, 200.0])
    );
    assert_eq!(head["center"], serde_json::json!([200.0, 150.0]));
    assert_eq!(head["geometry"]["radiusX"], 100.0);
    assert_eq!(node(&v, "eye-l")["inside"], "head");
    assert_eq!(node(&v, "pupil-l")["inside"], "eye-l");
    assert_eq!(node(&v, "pupil-l")["looksLike"], "brown circle");
    assert_eq!(node(&v, "behind")["visiblePercent"], 0.0);
    assert!(node(&v, "ghost")["notDrawn"]
        .as_str()
        .unwrap()
        .contains("opacity 0"));
    // Layout sentences.
    let text = layout(&v).join("\n");
    assert!(
        text.contains("eye-l (white circle) is inside head"),
        "{text}"
    );
    assert!(text.contains("upper-left"), "{text}");
    assert!(text.contains("completely hidden behind"), "{text}");
    assert!(
        text.contains("eye-l (left) and eye-r (right) are a matching pair"),
        "{text}"
    );
    assert!(
        text.contains("pupil-l (left) and pupil-r (right)"),
        "{text}"
    );
    assert!(
        text.contains("ghost") && text.contains("not drawn"),
        "{text}"
    );
    assert!(text.contains("background"), "{text}");
    // Animation summary.
    let clip = &v["animation"]["clips"][0];
    assert_eq!(clip["id"], "nod");
    assert_eq!(clip["autoplay"], true);
    assert_eq!(clip["loop"], true);
    assert_eq!(clip["tracks"][0]["target"], "head");
    assert_eq!(clip["tracks"][0]["property"], "y");
    assert_eq!(clip["tracks"][0]["keys"], "0s 0 → 1s 10");
    // No ASCII unless asked for.
    assert!(v.get("ascii").is_none());
}

#[test]
fn inspect_ascii_draws_one_letter_per_shape_with_legend() {
    let dir = tempdir().unwrap();
    let f = face(dir.path());
    let v = ok(&["inspect", s(&f), "--ascii", "--cols", "40"]);
    let ascii = &v["ascii"];
    let lines: Vec<&str> = ascii["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect();
    assert_eq!(ascii["cols"], 40);
    assert!(lines.iter().all(|l| l.chars().count() == 40));
    assert_eq!(lines.len(), 15); // 40 × 300/400 / 2
    let legend = ascii["legend"].as_object().unwrap();
    let letter = |id: &str| {
        legend
            .iter()
            .find(|(_, v)| v.as_str().unwrap().starts_with(&format!("{id} ")))
            .map(|(k, _)| k.chars().next().unwrap())
            .unwrap_or_else(|| panic!("{id} missing in legend {legend:?}"))
    };
    let picture = lines.join("\n");
    for id in ["bg", "head", "eye-l", "eye-r", "pupil-l", "pupil-r"] {
        assert!(picture.contains(letter(id)), "{id}\n{picture}");
    }
    // Hidden and fully covered shapes get no letter.
    assert!(!legend
        .values()
        .any(|v| v.as_str().unwrap().starts_with("behind ")));
    // The left eye is left of the right eye in the picture.
    let column = |c: char| lines.iter().find_map(|l| l.find(c)).unwrap();
    assert!(column(letter("eye-l")) < column(letter("eye-r")));
    // The background fills the corners.
    assert!(lines[0].starts_with(letter("bg")));
    // Cropping zooms onto the drawn shapes.
    let cropped = ok(&["inspect", s(&f), "--ascii", "--crop", "--cols", "40"]);
    let area = cropped["ascii"]["area"].as_array().unwrap();
    assert!(area[2].as_f64().unwrap() < 400.0, "{area:?}");
    assert_eq!(ascii["area"], serde_json::json!([0.0, 0.0, 400.0, 300.0]));
}

#[test]
fn inspect_focuses_on_one_node_and_full_detail_adds_transforms() {
    let dir = tempdir().unwrap();
    let f = face(dir.path());
    let v = ok(&["inspect", s(&f), "--node", "pupil-r"]);
    let nodes = v["scene"]["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["id"], "pupil-r");
    assert!(nodes[0].get("transform").is_some());
    assert!(layout(&v).iter().all(|s| s.contains("pupil-r")));
    assert!(v["animation"]["clips"].as_array().unwrap().is_empty());
    let head = ok(&["inspect", s(&f), "--node", "head"]);
    assert_eq!(head["animation"]["clips"][0]["id"], "nod");

    let full = ok(&["inspect", s(&f), "--detail", "full"]);
    assert!(node(&full, "head").get("transform").is_some());
    assert!(node(&full, "head").get("localGeometry").is_some());
    assert!(layout(&full).len() >= layout(&ok(&["inspect", s(&f)])).len());

    // Unknown node and bad --cols are typed errors.
    let out = run(&["inspect", s(&f), "--node", "nope"]);
    assert_eq!(out.status.code(), Some(1));
    let err: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(err["error"]["code"], "not_found");
    let out = run(&["inspect", s(&f), "--ascii", "--cols", "3"]);
    assert_eq!(out.status.code(), Some(1));
}
