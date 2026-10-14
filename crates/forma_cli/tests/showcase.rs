use serde_json::Value;
use std::process::Command;

fn cli(args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_forma-cli"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn showcases_are_canonical_and_deterministic() {
    for args in [
        vec!["water-drop", "--output", "-"],
        vec![
            "random-figures",
            "--seed",
            "18446744073709551615",
            "--count",
            "12",
            "--output",
            "-",
        ],
    ] {
        let value = cli(&args);
        assert_eq!(value, cli(&args));
        let project: forma_document::Project = serde_json::from_value(value.clone()).unwrap();
        project.validate().unwrap();
        assert_eq!(serde_json::to_value(&project).unwrap(), value);
        assert!(project.custom_js.contains("forma.on('state.change'"));
        assert!(project.custom_js.contains("forma.onDestroy"));
        assert!(project.raw_svg.is_empty());
    }
    assert_ne!(
        cli(&["random-figures", "--seed", "42", "--output", "-"])["nodes"],
        cli(&["random-figures", "--seed", "43", "--output", "-"])["nodes"]
    );
}

#[test]
fn invalid_count_does_not_replace_output() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("keep.json");
    std::fs::write(&file, "keep me").unwrap();
    for count in ["0", "2", "13", "999999"] {
        let output = Command::new(env!("CARGO_BIN_EXE_forma-cli"))
            .args([
                "random-figures",
                "--count",
                count,
                "--output",
                file.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["error"]["code"], "invalid_arguments");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "keep me");
    }
}

#[test]
fn water_has_loop_continuity_and_supported_deformation() {
    let project: forma_document::Project =
        serde_json::from_value(cli(&["water-drop", "--output", "-"])).unwrap();
    for (id, x, y) in [("floor-shadow", 480., 507.), ("contact-shadow", 480., 289.)] {
        let node = project.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!((node.transform.x, node.transform.y), (x, y));
        assert!(matches!(
            node.kind,
            forma_document::NodeKind::Circle { cx: 0., cy: 0., .. }
        ));
    }
    let clip = &project.clips[0];
    assert!(clip.autoplay && clip.looping);
    for track in &clip.tracks {
        // Ripple scale resets while invisible; visible drop parts match at loop seam.
        if track.node_id.starts_with("drop") {
            assert_eq!(
                track.keyframes.first().unwrap().value,
                track.keyframes.last().unwrap().value
            );
        }
    }
    let first = project.evaluate(0.).unwrap();
    let impact = project.evaluate(1.02).unwrap();
    let drop_index = project.nodes.iter().position(|n| n.id == "drop").unwrap();
    assert_ne!(first[drop_index].transform, impact[drop_index].transform);
}

#[test]
fn exports_require_independent_opt_ins() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("gallery.json");
    cli(&["random-figures", "--output", file.to_str().unwrap()]);
    for flags in [
        vec![],
        vec!["--trusted-js"],
        vec!["--speech"],
        vec!["--trusted-js", "--speech"],
    ] {
        let mut args = vec!["export", file.to_str().unwrap(), "--output", "-"];
        args.extend(&flags);
        let result = cli(&args);
        let svg = result["svg"].as_str().unwrap();
        assert_eq!(
            svg.contains("new Function('forma'"),
            flags.contains(&"--trusted-js")
        );
        assert_eq!(
            svg.contains("const speech=true"),
            flags.contains(&"--speech")
        );
    }
}
