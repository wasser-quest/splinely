use super::*;

fn rig_editor() -> Editor {
    let mut e = Editor {
        project: Project::default(),
        ..Editor::default()
    };
    e.project
        .nodes
        .push(Node::rect("limb", 0., -10., 100., 20.));
    e.selected = Some("limb".into());
    e
}

#[test]
fn root_child_and_geometry_changes_validate_and_undo() {
    let mut e = rig_editor();
    let before = e.project.clone();
    e.add_bone(false);
    let root = e.selected_bone.clone().unwrap();
    e.add_bone(true);
    assert_eq!(e.project.bones[1].parent_id.as_ref(), Some(&root));
    let poses = e
        .project
        .evaluate_bones(0., &EvaluationContext::default())
        .unwrap();
    assert_eq!(poses[1].x, poses[0].x + 100.);
    assert!(e.history.undo(&mut e.project));
    assert_eq!(e.project.bones.len(), 1);
    assert!(e.history.undo(&mut e.project));
    assert_eq!(e.project, before);
}

#[test]
fn binding_preserves_base_world_pose_and_rotated_drag_is_local() {
    let mut e = rig_editor();
    e.project.nodes[0].transform.x = 42.;
    e.project.nodes[0].transform.y = 17.;
    e.add_bone(false);
    e.project.bones[0].rotation = 90.;
    let before = e.evaluated()[0].transform;
    e.bind_selected(true);
    assert!(e.project.nodes[0].bone_id.is_some());
    let bound = e.evaluated()[0].transform;
    assert!((bound.x - before.x).abs() < 1e-8);
    assert!((bound.y - before.y).abs() < 1e-8);
    assert!((bound.rotation - before.rotation).abs() < 1e-8);
    e.drag_start = Some(e.project.clone());
    e.drag_by(Vec2::new(20., 0.));
    e.finish_drag();
    assert!((e.evaluated()[0].transform.x - 62.).abs() < 1e-5);
    assert!(e.history.undo(&mut e.project));
    e.bind_selected(false);
    assert!(e.project.nodes[0].bone_id.is_none());
    assert!((e.project.nodes[0].transform.x - 42.).abs() < 1e-8);
}

#[test]
fn bone_pose_keys_are_sorted_replaceable_and_preview_matches_projection() {
    let mut e = rig_editor();
    e.add_bone(false);
    e.bind_selected(true);
    e.project.clips.push(Clip {
        id: "pose".into(),
        name: "Pose".into(),
        duration: 3.,
        looping: false,
        autoplay: false,
        blend_mode: BlendMode::Override,
        weight: 1.,
        tracks: vec![],
    });
    e.key_bone();
    e.time = 2.;
    let mut next = e.project.clone();
    next.bones[0].rotation = 90.;
    e.commit(next);
    e.key_bone();
    e.key_bone();
    assert_eq!(e.project.clips[0].tracks.len(), 3);
    assert!(e.project.clips[0]
        .tracks
        .iter()
        .all(|t| t.keyframes.len() == 2 && t.keyframes[0].time == 0.));
    e.time = 1.;
    e.preview_animation = true;
    let context = e.preview_context();
    let poses = e.project.evaluate_bones(e.time, &context).unwrap();
    assert!((poses[0].rotation - 45.).abs() < 1e-8);
    let nodes = e.evaluated();
    let scene = e.projection.evaluate_project(
        &e.project,
        e.time,
        Some("pose".into()),
        true,
        context.states,
        context.overrides,
    );
    let projected = project_node(&nodes[0]).unwrap();
    assert!((scene[0].x - projected.x).abs() < 1e-5);
    assert!((scene[0].rotation - projected.rotation).abs() < 1e-5);
    assert_eq!(e.project.bones[0].rotation, 90.);
}

#[test]
fn cycles_invalid_lengths_and_animated_binding_do_not_change_history() {
    let mut e = rig_editor();
    e.add_bone(false);
    e.add_bone(true);
    let before = e.project.clone();
    let steps = e.history_steps;
    let mut bad = before.clone();
    bad.bones[0].parent_id = Some(bad.bones[1].id.clone());
    e.commit(bad);
    let mut bad = before.clone();
    bad.bones[0].length = 0.;
    e.commit(bad);
    assert_eq!(e.project, before);
    assert_eq!(e.history_steps, steps);
    e.project.nodes[0].locked = true;
    e.bind_selected(true);
    assert!(e.project.nodes[0].bone_id.is_none());
    e.project.nodes[0].locked = false;
    e.project.clips = demo().clips;
    e.project.clips[0].tracks[0].node_id = "limb".into();
    let before = e.project.clone();
    e.bind_selected(true);
    assert_eq!(e.project, before);
    assert_eq!(e.history_steps, steps);
}

#[test]
fn static_export_does_not_apply_binding_twice() {
    let mut e = rig_editor();
    e.add_bone(false);
    e.bind_selected(true);
    e.project.bones[0].rotation = 30.;
    let expected = e.evaluated()[0].transform;
    let path = std::env::temp_dir().join(format!("forma-bone-snapshot-{}.svg", std::process::id()));
    e.export_path = path.to_string_lossy().into();
    e.export(false);
    let svg = fs::read_to_string(&path).unwrap();
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let shape = xml
        .descendants()
        .find(|n| n.attribute("id") == Some("limb"))
        .unwrap();
    assert_eq!(
        shape.attribute("transform").unwrap(),
        format!(
            "translate({} {}) rotate({}) scale({} {})",
            expected.x, expected.y, expected.rotation, expected.scale_x, expected.scale_y
        )
    );
    assert!(!svg.contains("<script"));
    fs::remove_file(path).unwrap();
}

#[test]
fn fit_and_visibility_keep_native_preview_consistent() {
    assert!((fit_zoom(Vec2::new(816., 616.), 800., 600.) - 1.).abs() < 1e-6);
    assert_eq!(fit_zoom(Vec2::new(1., 1.), 800., 600.), MIN_ZOOM);
    let mut node = Node::circle("hidden", 10., 10., 5.);
    node.visible = false;
    assert!(project_node(&node).is_none());
}

#[test]
fn bones_workspace_draws_and_joint_pick_selects_without_document_mutation() {
    let mut e = rig_editor();
    e.add_bone(false);
    e.tab = Tab::Bones;
    e.zoom = 1.;
    let bone = e.project.bones[0].clone();
    e.selected_bone = None;
    let before = e.project.clone();
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            assert!(e.bone_guides(
                ui.painter(),
                Pos2::ZERO,
                Some(Vec2::new(bone.x as f32, bone.y as f32))
            ));
            e.bones_ui(ui);
            e.canvas(ui);
        });
    });
    assert_eq!(e.selected_bone.as_deref(), Some(bone.id.as_str()));
    assert_eq!(e.project, before);
}
