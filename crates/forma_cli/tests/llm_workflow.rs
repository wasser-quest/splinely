//! The agent-facing workflow: the guide's recipes run verbatim, edits default
//! to in-place, errors carry hints, batch is all-or-nothing, and `check`
//! suggestions are runnable commands that fix what they report.
use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

const CLI: &str = env!("CARGO_BIN_EXE_forma-cli");

fn invoke(dir: &Path, args: &[&str]) -> Output {
    Command::new(CLI)
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn ok(dir: &Path, args: &[&str]) -> Value {
    let out = invoke(dir, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn err(dir: &Path, args: &[&str]) -> Value {
    let out = invoke(dir, args);
    assert!(!out.status.success(), "{args:?} unexpectedly succeeded");
    assert!(out.stdout.is_empty());
    serde_json::from_slice(&out.stderr).unwrap()
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn guide_recipes_run_verbatim_and_pass_check() {
    let dir = tempfile::tempdir().unwrap();
    let guide = invoke(dir.path(), &["guide"]);
    assert!(guide.status.success());
    let text = String::from_utf8(guide.stdout).unwrap();
    assert!(text.starts_with("# Forma CLI"));
    let mut script = String::from("set -e\n");
    for block in text.split("```sh\n").skip(1) {
        script.push_str(block.split("```").next().unwrap());
    }
    // Run the blocks with `forma-cli` resolving to this build.
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    std::os::unix::fs::symlink(CLI, bin.join("forma-cli")).unwrap();
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let run = Command::new("bash")
        .current_dir(dir.path())
        .env("PATH", path)
        .args(["-c", &script])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "guide script failed:\n{}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    for file in ["scene.frm", "scene.svg", "preview.png", "card.frm"] {
        assert!(dir.path().join(file).exists(), "{file}");
    }
    for file in ["scene.frm", "card.frm"] {
        let check = ok(dir.path(), &["check", file]);
        assert_eq!(check["warnings"], 0, "{file}: {check}");
    }
    // The sun rises: y moves, fill changes, clip autoplays and loops.
    let clips = ok(dir.path(), &["clip", "list", "scene.frm"]);
    let rise = &clips["clips"][0];
    assert_eq!(
        (rise["autoplay"].clone(), rise["loop"].clone()),
        (true.into(), true.into())
    );
    assert_eq!(rise["tracks"][0]["times"], serde_json::json!([0.0, 3.0]));
    assert!(clips["clips"][1]["startedBy"][0]
        .as_str()
        .unwrap()
        .contains("click on sun"));
    // No temporary files from batch are left behind.
    let leftovers = std::fs::read_dir(dir.path())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".forma-batch")
        })
        .count();
    assert_eq!(leftovers, 0);
}

#[test]
fn edits_default_to_in_place_and_errors_explain_the_fix() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    ok(
        d,
        &[
            "new",
            "--output",
            "a.frm",
            "--width",
            "300",
            "--height",
            "200",
            "--background",
            "navy",
        ],
    );
    let project = read(&d.join("a.frm"));
    assert_eq!(
        (project["width"].as_f64(), project["height"].as_f64()),
        (Some(300.), Some(200.))
    );
    assert_eq!(project["nodes"][0]["id"], "background");
    // No --output: the input is updated.
    ok(
        d,
        &[
            "node", "add", "a.frm", "--kind", "circle", "--id", "dot", "--cx", "50", "--cy", "60",
            "--r", "9",
        ],
    );
    ok(d, &["node", "update", "a.frm", "dot", "--fill", "gold"]);
    let project = read(&d.join("a.frm"));
    assert_eq!(project["nodes"][1]["kind"]["radius"], 9.0);
    // Typos name the closest ID; unknown flags list the valid ones.
    let e = err(d, &["node", "update", "a.frm", "Dot", "--fill", "red"]);
    assert_eq!(e["error"]["code"], "not_found");
    assert!(e["error"]["hint"]
        .as_str()
        .unwrap()
        .starts_with("Did you mean 'dot'?"));
    let e = err(
        d,
        &["node", "add", "a.frm", "--kind", "rect", "--centre", "4"],
    );
    assert!(
        e["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Valid flags: --kind"),
        "{e}"
    );
    assert!(e["error"]["hint"]
        .as_str()
        .unwrap()
        .contains("node add --help"));
    let e = err(
        d,
        &[
            "keyframe",
            "a.frm",
            "--clip",
            "c",
            "--target",
            "dot",
            "--property",
            "fill",
            "--time",
            "1",
            "--value",
            "1",
        ],
    );
    assert!(
        e["error"]["message"]
            .as_str()
            .unwrap()
            .contains("needs --color"),
        "{e}"
    );
    // Every error carries a hint.
    let e = err(d, &["validate", "missing.frm"]);
    assert!(e["error"]["hint"].is_string());
    // Explicit-output commands still require it.
    err(d, &["export", "a.frm"]);
}

#[test]
fn keyframes_start_from_the_current_look_and_set_clip_options() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    ok(d, &["new", "--output", "k.frm"]);
    ok(
        d,
        &[
            "node", "add", "k.frm", "--kind", "rect", "--id", "box", "--x", "100", "--y", "100",
            "--width", "50", "--height", "50", "--fill", "red", "--pivot", "center",
        ],
    );
    let r = ok(
        d,
        &[
            "keyframe",
            "k.frm",
            "--clip",
            "go",
            "--target",
            "box",
            "--property",
            "fill",
            "--time",
            "2",
            "--color",
            "blue",
            "--easing",
            "ease-in-out",
            "--autoplay",
            "--loop",
        ],
    );
    assert!(r["hints"][0]
        .as_str()
        .unwrap()
        .contains("starting keyframe"));
    ok(
        d,
        &[
            "keyframe",
            "k.frm",
            "--clip",
            "go",
            "--target",
            "box",
            "--property",
            "y",
            "--time",
            "2",
            "--by",
            "-40",
        ],
    );
    let project = read(&d.join("k.frm"));
    let clip = &project["clips"][0];
    assert_eq!(
        (clip["autoplay"].as_bool(), clip["looping"].as_bool()),
        (Some(true), Some(true))
    );
    let fill = &clip["tracks"][0]["keyframes"];
    assert_eq!(fill[0]["time"], 0.0);
    assert_eq!(fill[0]["value"]["value"]["r"], 1.0);
    assert_eq!(fill[0]["easing"]["type"], "cubicBezier");
    let y = &clip["tracks"][1]["keyframes"];
    let base = project["nodes"][0]["transform"]["y"].as_f64().unwrap();
    assert_eq!(base, 125.0, "pivot moved the origin to the centre");
    assert_eq!(y[1]["value"]["value"].as_f64(), Some(base - 40.));
    // Pivoting keeps the look: bounds are unchanged.
    let list = ok(d, &["node", "list", "k.frm"]);
    assert_eq!(
        list["nodes"][0]["bounds"],
        serde_json::json!([100.0, 100.0, 50.0, 50.0])
    );
    let e = err(
        d,
        &["clip", "update", "k.frm", "--id", "go", "--duration", "1"],
    );
    assert!(e["error"]["message"]
        .as_str()
        .unwrap()
        .contains("shorter than the last keyframe"));
}

#[test]
fn batch_is_all_or_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    ok(d, &["new", "--output", "b.frm"]);
    let before = std::fs::read(d.join("b.frm")).unwrap();
    std::fs::write(
        d.join("bad.txt"),
        "node add --kind rect --id a\nnode update zzz --fill red\n",
    )
    .unwrap();
    let e = err(d, &["batch", "b.frm", "--steps", "bad.txt"]);
    assert!(
        e["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("Step 2"),
        "{e}"
    );
    assert!(e["error"]["hint"]
        .as_str()
        .unwrap()
        .contains("Nothing was saved"));
    assert_eq!(std::fs::read(d.join("b.frm")).unwrap(), before);
    // JSON steps; repeating the project path in a step is forgiven.
    std::fs::write(
        d.join("good.json"),
        r#"[["node","add","b.frm","--kind","rect","--id","a"], "node update a --fill green", "on --event click --target a --set a.opacity=0.5"]"#,
    )
    .unwrap();
    let r = ok(d, &["batch", "b.frm", "--steps", "good.json"]);
    assert_eq!(r["steps"], 3);
    assert_eq!(r["results"][0]["result"]["id"], "a");
    let project = read(&d.join("b.frm"));
    assert_eq!(project["stateMachines"][0]["id"], "interactions");
    for step in ["new --output x.frm", "guide"] {
        std::fs::write(d.join("s.txt"), step).unwrap();
        err(d, &["batch", "b.frm", "--steps", "s.txt"]);
    }
}

#[test]
fn check_fixes_are_runnable_commands() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    ok(
        d,
        &[
            "new", "--output", "c.frm", "--width", "200", "--height", "100",
        ],
    );
    ok(
        d,
        &[
            "node", "add", "c.frm", "--kind", "text", "--id", "label", "--x", "10", "--y", "50",
            "--text", "Hi", "--fill", "#111111",
        ],
    );
    ok(
        d,
        &[
            "node", "add", "c.frm", "--kind", "rect", "--id", "bg", "--width", "200", "--height",
            "100", "--fill", "#101010",
        ],
    );
    ok(d, &["clip", "add", "c.frm", "--id", "idle"]);
    let check = ok(d, &["check", "c.frm"]);
    let codes: Vec<&str> = check["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["code"].as_str().unwrap())
        .collect();
    assert!(
        codes.contains(&"hidden_behind") && codes.contains(&"clip_never_plays"),
        "{codes:?}"
    );
    err(d, &["check", "c.frm", "--strict"]);
    // Run the suggested fix for the covered text verbatim.
    let fix = check["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["code"] == "hidden_behind")
        .unwrap()["fix"]
        .as_str()
        .unwrap()
        .to_owned();
    let args: Vec<&str> = fix.split_whitespace().skip(1).collect();
    ok(d, &args);
    let check = ok(d, &["check", "c.frm"]);
    let codes: Vec<&str> = check["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["code"].as_str().unwrap())
        .collect();
    assert!(!codes.contains(&"hidden_behind"), "{codes:?}");
    assert!(
        codes.contains(&"low_contrast"),
        "now visible but unreadable: {codes:?}"
    );
    // A shape far off the canvas: the computed move brings it to the centre.
    ok(
        d,
        &[
            "node", "add", "c.frm", "--kind", "circle", "--id", "lost", "--x", "900", "--y",
            "-400", "--radius", "10",
        ],
    );
    let check = ok(d, &["check", "c.frm"]);
    let fix = check["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["code"] == "outside_canvas")
        .unwrap()["fix"]
        .as_str()
        .unwrap()
        .to_owned();
    let args: Vec<&str> = fix.split_whitespace().skip(1).collect();
    ok(d, &args);
    let list = ok(d, &["node", "list", "c.frm"]);
    let lost = list["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "lost")
        .unwrap();
    assert_eq!(lost["bounds"], serde_json::json!([90.0, 40.0, 20.0, 20.0]));
}
