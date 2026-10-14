//! End-to-end tests for `node`, `arrange` and `render` through the real binary.
use serde_json::Value;
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
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let response: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(response["error"]["code"], code, "{response}");
}
fn bounds(path: &str, id: &str) -> Vec<f64> {
    let list = success(&["node", "list", path]);
    let node = list["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == id)
        .unwrap_or_else(|| panic!("missing {id}"))
        .clone();
    node["bounds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}

#[test]
fn shapes_can_be_authored_arranged_and_rendered() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shapes.frm");
    let path = path.to_str().unwrap();
    success(&["new", "--output", path]);
    let added = success(&[
        "node", "add", path, "--kind", "rect", "--x", "10", "--y", "10", "--width", "40",
        "--height", "20", "--fill", "tomato", "--name", "Box", "--output", path,
    ]);
    assert_eq!(added["id"], "node-1");
    success(&[
        "node", "add", path, "--kind", "circle", "--id", "dot", "--x", "200", "--y", "100",
        "--radius", "10", "--stroke", "black", "--output", path,
    ]);
    success(&[
        "node",
        "add",
        path,
        "--kind",
        "path",
        "--id",
        "tri",
        "--d",
        "M0 0 L30 0 L15 20 Z",
        "--tx",
        "400",
        "--ty",
        "300",
        "--output",
        path,
    ]);
    success(&[
        "arrange",
        "align",
        path,
        "--nodes",
        "node-1,dot,tri",
        "--edge",
        "top",
        "--output",
        path,
    ]);
    for id in ["node-1", "dot", "tri"] {
        assert_eq!(bounds(path, id)[1], 10.0, "{id}");
    }
    success(&[
        "arrange",
        "distribute",
        path,
        "--nodes",
        "tri,node-1,dot",
        "--axis",
        "horizontal",
        "--output",
        path,
    ]);
    let (a, b, c) = (
        bounds(path, "node-1"),
        bounds(path, "dot"),
        bounds(path, "tri"),
    );
    let gap1 = b[0] - (a[0] + a[2]);
    let gap2 = c[0] - (b[0] + b[2]);
    assert!((gap1 - gap2).abs() < 1e-9);
    let group = success(&[
        "arrange",
        "group",
        path,
        "--nodes",
        "node-1,dot",
        "--name",
        "Kopf",
        "--output",
        path,
    ]);
    let list = success(&["node", "list", path]);
    assert_eq!(list["nodes"][0]["group"], group["id"]);
    assert_eq!(list["groups"][0]["name"], "Kopf");
    success(&[
        "arrange", "order", path, "--nodes", "node-1", "--to", "front", "--output", path,
    ]);
    let list = success(&["node", "list", path]);
    assert_eq!(list["nodes"][2]["id"], "node-1");
    let before = bounds(path, "tri");
    success(&[
        "arrange", "flip", path, "--nodes", "tri", "--axis", "vertical", "--output", path,
    ]);
    assert_eq!(bounds(path, "tri"), before);
    success(&[
        "arrange", "move", path, "--nodes", "tri", "--dx", "-5", "--output", path,
    ]);
    assert_eq!(bounds(path, "tri")[0], before[0] - 5.0);
    success(&[
        "node", "update", path, "--node", "dot", "--radius", "15", "--stroke", "none", "--output",
        path,
    ]);
    assert_eq!(bounds(path, "dot")[2], 30.0);
    let copy = success(&[
        "node",
        "duplicate",
        path,
        "--nodes",
        "dot",
        "--output",
        path,
    ]);
    assert_eq!(copy["created"][0], "dot-copy-1");
    success(&[
        "node",
        "delete",
        path,
        "--nodes",
        "dot-copy-1,tri",
        "--output",
        path,
    ]);
    assert_eq!(
        success(&["node", "list", path])["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    success(&["validate", path]);

    let png = dir.path().join("shot.png");
    let png = png.to_str().unwrap();
    let rendered = success(&["render", path, "--output", png, "--width", "200"]);
    assert_eq!(rendered["width"], 200);
    assert_eq!(rendered["height"], 150);
    assert!(std::fs::read(png).unwrap().starts_with(b"\x89PNG"));
}

#[test]
fn invalid_shape_arguments_fail_with_json_errors_and_leave_files_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shapes.frm");
    let path = path.to_str().unwrap();
    success(&["new", "--output", path]);
    success(&[
        "node", "add", path, "--kind", "text", "--text", "Hi", "--output", path,
    ]);
    let original = std::fs::read(path).unwrap();
    error(
        &[
            "node", "update", path, "--node", "node-1", "--width", "3", "--output", path,
        ],
        "invalid_arguments",
    );
    error(
        &[
            "node", "add", path, "--kind", "rect", "--fill", "nope", "--output", path,
        ],
        "invalid_arguments",
    );
    error(
        &["node", "add", path, "--kind", "path", "--output", path],
        "invalid_arguments",
    );
    error(
        &[
            "node", "add", path, "--kind", "rect", "--id", "node-1", "--output", path,
        ],
        "validation_error",
    );
    error(
        &[
            "arrange", "align", path, "--nodes", "ghost", "--edge", "left", "--output", path,
        ],
        "not_found",
    );
    error(
        &[
            "arrange",
            "distribute",
            path,
            "--nodes",
            "node-1",
            "--axis",
            "vertical",
            "--output",
            path,
        ],
        "validation_error",
    );
    error(
        &[
            "arrange", "group", path, "--nodes", "node-1", "--output", path,
        ],
        "validation_error",
    );
    error(
        &[
            "node", "update", path, "--node", "node-1", "--group", "ghost", "--output", path,
        ],
        "validation_error",
    );
    error(
        &["render", path, "--output", "/nonexistent-dir/x.png"],
        "write_error",
    );
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn scale_rotate_and_boolean_through_the_cli() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bool.frm");
    let path = path.to_str().unwrap();
    success(&["new", "--output", path]);
    success(&[
        "node", "add", path, "--kind", "rect", "--id", "a", "--x", "0", "--y", "0", "--width",
        "100", "--height", "100", "--fill", "red", "--output", path,
    ]);
    success(&[
        "node", "add", path, "--kind", "rect", "--id", "b", "--x", "50", "--y", "50", "--width",
        "100", "--height", "100", "--output", path,
    ]);
    success(&[
        "arrange",
        "scale",
        path,
        "--nodes",
        "a",
        "--sx",
        "2",
        "--anchor-x",
        "0",
        "--anchor-y",
        "0",
        "--output",
        path,
    ]);
    assert_eq!(bounds(path, "a"), vec![0.0, 0.0, 200.0, 200.0]);
    success(&[
        "arrange", "scale", path, "--nodes", "a", "--sx", "0.5", "--output", path,
    ]);
    assert_eq!(bounds(path, "a"), vec![50.0, 50.0, 100.0, 100.0]);
    success(&[
        "arrange",
        "rotate",
        path,
        "--nodes",
        "b",
        "--degrees",
        "90",
        "--output",
        path,
    ]);
    let b = bounds(path, "b");
    assert!(
        (b[0] - 50.0).abs() < 1e-9 && (b[2] - 100.0).abs() < 1e-9,
        "{b:?}"
    );
    success(&[
        "arrange", "move", path, "--nodes", "b", "--dx", "50", "--output", path,
    ]);
    let result = success(&[
        "arrange", "boolean", path, "--nodes", "a,b", "--op", "union", "--output", path,
    ]);
    let id = result["id"].as_str().unwrap().to_owned();
    let list = success(&["node", "list", path]);
    let nodes = list["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["fill"], "#ff0000");
    assert_eq!(bounds(path, &id), vec![50.0, 50.0, 150.0, 100.0]);
    success(&["validate", path]);
    error(
        &[
            "arrange", "boolean", path, "--nodes", &id, "--op", "union", "--output", path,
        ],
        "validation_error",
    );
    error(
        &[
            "arrange", "scale", path, "--nodes", &id, "--sx", "0", "--output", path,
        ],
        "validation_error",
    );
}

#[test]
fn generated_polygon_star_and_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prim.frm");
    let path = path.to_str().unwrap();
    success(&["new", "--output", path]);
    success(&[
        "node", "add", path, "--kind", "polygon", "--id", "hex", "--x", "100", "--y", "100",
        "--radius", "50", "--output", path,
    ]);
    success(&[
        "node", "add", path, "--kind", "star", "--id", "star", "--x", "300", "--y", "100",
        "--tips", "7", "--inner", "0.4", "--fill", "gold", "--output", path,
    ]);
    success(&[
        "node", "add", path, "--kind", "line", "--id", "line", "--x", "10", "--y", "10", "--x2",
        "110", "--y2", "10", "--output", path,
    ]);
    assert_eq!(bounds(path, "hex")[1], 50.0);
    assert_eq!(bounds(path, "line"), vec![10.0, 10.0, 100.0, 0.0]);
    let list = success(&["node", "list", path]);
    assert!(list["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|n| n["kind"] == "path"));
    error(
        &[
            "node", "add", path, "--kind", "polygon", "--sides", "2", "--output", path,
        ],
        "validation_error",
    );
    error(
        &[
            "node", "update", path, "--node", "hex", "--sides", "5", "--output", path,
        ],
        "invalid_arguments",
    );
}

#[test]
fn gradient_fill_via_cli_exports_and_renders() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("grad.frm");
    let path = path.to_str().unwrap();
    success(&["new", "--output", path]);
    success(&[
        "node",
        "add",
        path,
        "--kind",
        "rect",
        "--id",
        "sky",
        "--width",
        "800",
        "--height",
        "600",
        "--gradient",
        "linear 90 #1e3c72 #2a5298@0.6 #f8b500",
        "--output",
        path,
    ]);
    let list = success(&["node", "list", path]);
    assert_eq!(list["nodes"][0]["gradient"], "linear");
    let svg = dir.path().join("grad.svg");
    let svg = svg.to_str().unwrap();
    success(&["export", path, "--output", svg]);
    let text = std::fs::read_to_string(svg).unwrap();
    assert!(text.contains("<linearGradient id=\"sky-gradient\""));
    let png = dir.path().join("grad.png");
    let png = png.to_str().unwrap();
    success(&["render", path, "--output", png, "--width", "80"]);
    success(&[
        "node",
        "update",
        path,
        "--node",
        "sky",
        "--gradient",
        "none",
        "--output",
        path,
    ]);
    assert!(success(&["node", "list", path])["nodes"][0]["gradient"].is_null());
    error(
        &[
            "node",
            "update",
            path,
            "--node",
            "sky",
            "--gradient",
            "linear red",
            "--output",
            path,
        ],
        "invalid_arguments",
    );
}

#[test]
fn text_converts_to_an_outline_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("text.frm");
    let path = path.to_str().unwrap();
    success(&["new", "--output", path]);
    success(&[
        "node",
        "add",
        path,
        "--kind",
        "text",
        "--id",
        "t",
        "--text",
        "Hi",
        "--font-size",
        "50",
        "--x",
        "10",
        "--y",
        "100",
        "--output",
        path,
    ]);
    let output = invoke(&["convert-to-path", path, "--node", "t", "--output", path]);
    if !output.status.success() {
        // Machines without any system font cannot outline text; the error is explicit.
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("font"), "{error}");
        return;
    }
    let list = success(&["node", "list", path]);
    assert_eq!(list["nodes"][0]["kind"], "path");
    let b = bounds(path, "t");
    assert!(
        b[1] < 100.0 && b[1] + b[3] <= 101.0,
        "glyphs sit on the baseline: {b:?}"
    );
    error(
        &["convert-to-path", path, "--node", "t", "--output", path],
        "conversion_error",
    );
}

#[test]
fn morph_and_gradient_keyframes_through_the_cli() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("anim.frm");
    let path = path.to_str().unwrap();
    success(&["new", "--output", path]);
    success(&[
        "node",
        "add",
        path,
        "--kind",
        "rect",
        "--id",
        "box",
        "--x",
        "0",
        "--y",
        "0",
        "--width",
        "100",
        "--height",
        "100",
        "--gradient",
        "linear 0 red blue",
        "--output",
        path,
    ]);
    success(&[
        "node", "add", path, "--kind", "star", "--id", "star", "--x", "400", "--y", "300",
        "--radius", "60", "--output", path,
    ]);
    let morph = success(&[
        "arrange",
        "morph",
        path,
        "--source",
        "box",
        "--target",
        "star",
        "--duration",
        "2",
        "--output",
        path,
    ]);
    let clip = morph["clip"].as_str().unwrap().to_owned();
    // Halfway the box has moved toward the star.
    let mid = success(&["evaluate", path, "--time", "1"]);
    let nodes = mid["nodes"].as_array().unwrap();
    let box_node = nodes.iter().find(|n| n["id"] == "box").unwrap();
    assert_eq!(box_node["kind"]["type"], "path");
    let star = nodes.iter().find(|n| n["id"] == "star").unwrap();
    assert_eq!(star["visible"], false);
    // Gradient keyframes in the morph clip.
    for (time, spec) in [("0", "linear 0 red blue"), ("2", "linear 90 gold navy")] {
        success(&[
            "keyframe",
            path,
            "--clip",
            &clip,
            "--target",
            "box",
            "--property",
            "gradient",
            "--time",
            time,
            "--gradient",
            spec,
            "--output",
            path,
        ]);
    }
    let mid = success(&["evaluate", path, "--time", "1"]);
    let box_node = mid["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "box")
        .unwrap()
        .clone();
    assert_eq!(box_node["gradient"]["angle"], 45.0);
    let svg = dir.path().join("anim.svg");
    success(&["export", path, "--output", svg.to_str().unwrap()]);
    // Mismatched structures are rejected without touching the file.
    let before = std::fs::read(path).unwrap();
    error(
        &[
            "keyframe",
            path,
            "--clip",
            &clip,
            "--target",
            "box",
            "--property",
            "gradient",
            "--time",
            "1",
            "--gradient",
            "radial red blue",
            "--output",
            path,
        ],
        "validation_error",
    );
    error(
        &[
            "keyframe",
            path,
            "--clip",
            &clip,
            "--target",
            "box",
            "--property",
            "d",
            "--time",
            "1",
            "--path",
            "M0 0 L1 1",
            "--output",
            path,
        ],
        "validation_error",
    );
    error(
        &[
            "keyframe",
            path,
            "--clip",
            &clip,
            "--target",
            "box",
            "--property",
            "x",
            "--time",
            "1",
            "--gradient",
            "linear red blue",
            "--output",
            path,
        ],
        "invalid_arguments",
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn render_writes_animated_gifs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gif.frm");
    let path = path.to_str().unwrap();
    success(&["new", "--demo", "--output", path]);
    let gif = dir.path().join("demo.gif");
    let gif = gif.to_str().unwrap();
    let out = success(&[
        "render",
        path,
        "--output",
        gif,
        "--width",
        "64",
        "--fps",
        "5",
        "--duration",
        "1",
    ]);
    assert_eq!(out["format"], "gif");
    assert!(std::fs::read(gif).unwrap().starts_with(b"GIF89a"));
    error(
        &[
            "render",
            path,
            "--output",
            gif,
            "--fps",
            "500",
            "--duration",
            "1",
        ],
        "export_error",
    );
}
