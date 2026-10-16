//! The agent-facing workflow: the guide's recipes run verbatim, edits default
//! to in-place, errors carry hints, batch is all-or-nothing, and `check`
//! suggestions are runnable commands that fix what they report.
use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

const CLI: &str = env!("CARGO_BIN_EXE_splinely-cli");

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

/// Raw JSON value tree of a project file, binary `.sply` or JSON text.
fn project_value(bytes: &[u8]) -> Value {
    if splinely_document::Project::is_binary(bytes) {
        splinely_document::decode_to_value(bytes).unwrap()
    } else {
        serde_json::from_slice(bytes).unwrap()
    }
}

fn read(path: &Path) -> Value {
    project_value(&std::fs::read(path).unwrap())
}

// Runs the guide's shell recipes through bash with a symlinked CLI: Unix only.
#[cfg(unix)]
#[test]
fn guide_recipes_run_verbatim_and_pass_check() {
    let dir = tempfile::tempdir().unwrap();
    let guide = invoke(dir.path(), &["guide"]);
    assert!(guide.status.success());
    let text = String::from_utf8(guide.stdout).unwrap();
    assert!(text.starts_with("# Splinely CLI"));
    let mut script = String::from("set -e\n");
    for block in text.split("```sh\n").skip(1) {
        script.push_str(block.split("```").next().unwrap());
    }
    // Run the blocks with `splinely-cli` resolving to this build.
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    std::os::unix::fs::symlink(CLI, bin.join("splinely-cli")).unwrap();
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
    for file in ["scene.sply", "scene.svg", "preview.png", "card.sply"] {
        assert!(dir.path().join(file).exists(), "{file}");
    }
    for file in ["scene.sply", "card.sply"] {
        let check = ok(dir.path(), &["check", file]);
        assert_eq!(check["warnings"], 0, "{file}: {check}");
    }
    // The sun rises: y moves, fill changes, clip autoplays and loops.
    let clips = ok(dir.path(), &["clip", "list", "scene.sply"]);
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
                .starts_with(".splinely-batch")
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
            "a.sply",
            "--width",
            "300",
            "--height",
            "200",
            "--background",
            "navy",
        ],
    );
    let project = read(&d.join("a.sply"));
    assert_eq!(
        (project["width"].as_f64(), project["height"].as_f64()),
        (Some(300.), Some(200.))
    );
    assert_eq!(project["nodes"][0]["id"], "background");
    // No --output: the input is updated.
    ok(
        d,
        &[
            "node", "add", "a.sply", "--kind", "circle", "--id", "dot", "--cx", "50", "--cy", "60",
            "--r", "9",
        ],
    );
    ok(d, &["node", "update", "a.sply", "dot", "--fill", "gold"]);
    let project = read(&d.join("a.sply"));
    assert_eq!(project["nodes"][1]["kind"]["radius"], 9.0);
    // Typos name the closest ID; unknown flags list the valid ones.
    let e = err(d, &["node", "update", "a.sply", "Dot", "--fill", "red"]);
    assert_eq!(e["error"]["code"], "not_found");
    assert!(e["error"]["hint"]
        .as_str()
        .unwrap()
        .starts_with("Did you mean 'dot'?"));
    let e = err(
        d,
        &["node", "add", "a.sply", "--kind", "rect", "--centre", "4"],
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
            "a.sply",
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
    let e = err(d, &["validate", "missing.sply"]);
    assert!(e["error"]["hint"].is_string());
    // Explicit-output commands still require it.
    err(d, &["export", "a.sply"]);
}

#[test]
fn keyframes_start_from_the_current_look_and_set_clip_options() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    ok(d, &["new", "--output", "k.sply"]);
    ok(
        d,
        &[
            "node", "add", "k.sply", "--kind", "rect", "--id", "box", "--x", "100", "--y", "100",
            "--width", "50", "--height", "50", "--fill", "red", "--pivot", "center",
        ],
    );
    let r = ok(
        d,
        &[
            "keyframe",
            "k.sply",
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
            "k.sply",
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
    let project = read(&d.join("k.sply"));
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
    let list = ok(d, &["node", "list", "k.sply"]);
    assert_eq!(
        list["nodes"][0]["bounds"],
        serde_json::json!([100.0, 100.0, 50.0, 50.0])
    );
    let e = err(
        d,
        &["clip", "update", "k.sply", "--id", "go", "--duration", "1"],
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
    ok(d, &["new", "--output", "b.sply"]);
    let before = std::fs::read(d.join("b.sply")).unwrap();
    std::fs::write(
        d.join("bad.txt"),
        "node add --kind rect --id a\nnode update zzz --fill red\n",
    )
    .unwrap();
    let e = err(d, &["batch", "b.sply", "--steps", "bad.txt"]);
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
    assert_eq!(std::fs::read(d.join("b.sply")).unwrap(), before);
    // JSON steps; repeating the project path in a step is forgiven.
    std::fs::write(
        d.join("good.json"),
        r#"[["node","add","b.sply","--kind","rect","--id","a"], "node update a --fill green", "on --event click --target a --set a.opacity=0.5"]"#,
    )
    .unwrap();
    let r = ok(d, &["batch", "b.sply", "--steps", "good.json"]);
    assert_eq!(r["steps"], 3);
    assert_eq!(r["results"][0]["result"]["id"], "a");
    let project = read(&d.join("b.sply"));
    assert_eq!(project["stateMachines"][0]["id"], "interactions");
    for step in ["new --output x.sply", "guide"] {
        std::fs::write(d.join("s.txt"), step).unwrap();
        err(d, &["batch", "b.sply", "--steps", "s.txt"]);
    }
}

#[test]
fn check_fixes_are_runnable_commands() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    ok(
        d,
        &[
            "new", "--output", "c.sply", "--width", "200", "--height", "100",
        ],
    );
    ok(
        d,
        &[
            "node", "add", "c.sply", "--kind", "text", "--id", "label", "--x", "10", "--y", "50",
            "--text", "Hi", "--fill", "#111111",
        ],
    );
    ok(
        d,
        &[
            "node", "add", "c.sply", "--kind", "rect", "--id", "bg", "--width", "200", "--height",
            "100", "--fill", "#101010",
        ],
    );
    ok(d, &["clip", "add", "c.sply", "--id", "idle"]);
    let check = ok(d, &["check", "c.sply"]);
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
    err(d, &["check", "c.sply", "--strict"]);
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
    let check = ok(d, &["check", "c.sply"]);
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
            "node", "add", "c.sply", "--kind", "circle", "--id", "lost", "--x", "900", "--y",
            "-400", "--radius", "10",
        ],
    );
    let check = ok(d, &["check", "c.sply"]);
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
    let list = ok(d, &["node", "list", "c.sply"]);
    let lost = list["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "lost")
        .unwrap();
    assert_eq!(lost["bounds"], serde_json::json!([90.0, 40.0, 20.0, 20.0]));
}

#[test]
fn check_counts_states_and_actions_as_animation() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    ok(d, &["new", "--output", "b.sply"]);
    for id in ["shut", "flash", "hidden"] {
        ok(
            d,
            &[
                "node",
                "add",
                "b.sply",
                "--kind",
                "circle",
                "--id",
                id,
                "--x",
                "100",
                "--y",
                "100",
                "--radius",
                "10",
                "--opacity",
                "0",
            ],
        );
    }
    // A blink state shows `shut`; a click action shows `flash`; `hidden` never shows.
    ok(
        d,
        &[
            "on",
            "b.sply",
            "--machine",
            "blink",
            "--from",
            "open",
            "--to",
            "closed",
            "--after",
            "3",
            "--state",
            "shut.opacity=1",
        ],
    );
    ok(
        d,
        &[
            "on",
            "b.sply",
            "--event",
            "click",
            "--target",
            "hidden",
            "--set",
            "flash.opacity=1",
        ],
    );
    let check = ok(d, &["check", "b.sply"]);
    let invisible: Vec<&str> = check["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "invisible")
        .map(|f| f["message"].as_str().unwrap())
        .collect();
    assert_eq!(invisible.len(), 1, "{invisible:?}");
    assert!(invisible[0].starts_with("'hidden'"), "{invisible:?}");
}
