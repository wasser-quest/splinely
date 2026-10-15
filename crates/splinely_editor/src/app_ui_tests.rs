use super::*;

fn frame(editor: &mut Editor, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.style_mut(|style| style.animation_time = 0.);
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400., 1000.))),
            events,
            ..Default::default()
        },
        |ctx| {
            editor.toolbar(ctx);
            if editor.timeline_visible {
                editor.transport(ctx);
            }
            egui::CentralPanel::default().show(ctx, |ui| editor.canvas(ui));
        },
    )
}

fn texts(output: &egui::FullOutput) -> Vec<(String, Pos2)> {
    fn collect(shape: &egui::Shape, result: &mut Vec<(String, Pos2)>) {
        match shape {
            egui::Shape::Text(text) => result.push((
                text.galley.text().into(),
                text.pos + text.galley.size() * 0.5,
            )),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, result);
                }
            }
            _ => {}
        }
    }
    let mut result = vec![];
    for shape in &output.shapes {
        collect(&shape.shape, &mut result);
    }
    result
}

fn click(editor: &mut Editor, ctx: &egui::Context, label: &str) -> egui::FullOutput {
    // Warm up egui's panel sizing before locating controls by their painted text.
    frame(editor, ctx, vec![]);
    let output = frame(editor, ctx, vec![]);
    let pos = texts(&output)
        .into_iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("Missing UI control: {label}"))
        .1;
    for pressed in [true, false] {
        let output = frame(
            editor,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
        if !pressed {
            return output;
        }
    }
    unreachable!()
}

#[test]
fn every_tool_transition_is_exclusive_and_preserves_source_trust() {
    let tools = [
        CanvasTool::Selection,
        CanvasTool::Pen,
        CanvasTool::PathPoints,
        CanvasTool::Freehand,
    ];
    for from in tools {
        for to in tools {
            let mut editor = Editor::default();
            let original = editor.project.clone();
            editor.code_buffer = "staged JS".into();
            editor.svg_buffer = "staged SVG".into();
            editor.machine_buffer = "staged machine".into();
            editor.trusted_js_export = true;
            editor.choose_tool(from);
            editor.choose_tool(to);
            assert_eq!(editor.canvas_tool(), to);
            assert_eq!(editor.pen.is_some(), to == CanvasTool::Pen);
            assert_eq!(editor.edit_path_handles, to == CanvasTool::PathPoints);
            assert_eq!(editor.freehand_mode, to == CanvasTool::Freehand);
            assert_eq!(editor.project, original);
            assert!(!editor.history.can_undo());
            assert_eq!(editor.code_buffer, "staged JS");
            assert_eq!(editor.svg_buffer, "staged SVG");
            assert_eq!(editor.machine_buffer, "staged machine");
            assert!(editor.trusted_js_export);
        }
    }
}

#[test]
fn pen_reselection_keeps_draft_and_leaving_discards_only_draft() {
    let mut editor = Editor::default();
    let original = editor.project.clone();
    editor.choose_tool(CanvasTool::Pen);
    editor
        .pen
        .as_mut()
        .unwrap()
        .add_anchor([10., 20.], None)
        .unwrap();
    editor
        .pen
        .as_mut()
        .unwrap()
        .add_anchor([80., 30.], None)
        .unwrap();
    let segments = editor.pen.as_ref().unwrap().path().segments().len();
    editor.choose_tool(CanvasTool::Pen);
    assert_eq!(
        editor.pen.as_ref().unwrap().path().segments().len(),
        segments
    );
    editor.pen_press = Some([40., 50.]);
    editor.choose_tool(CanvasTool::Freehand);
    assert!(editor.pen.is_none());
    assert!(editor.pen_press.is_none());
    assert!(editor.path_drag.is_none());
    assert_eq!(editor.project, original);
    assert!(!editor.history.can_undo());
}

#[test]
fn tool_switch_freezes_real_move_once_without_release_snap() {
    let mut editor = Editor::default();
    let original = editor.project.clone();
    editor.selected = Some("circle".into());
    editor.snap_grid = true;
    editor.drag_start = Some(original.clone());
    editor.project.nodes[0].transform.x = 23.;
    editor.choose_tool(CanvasTool::Pen);
    assert_eq!(editor.project.nodes[0].transform.x, 23.);
    assert!(editor.drag_start.is_none());
    editor.finish_drag();
    assert!(editor.history.undo(&mut editor.project));
    assert_eq!(editor.project, original);
    assert!(!editor.history.can_undo());
}

#[test]
fn tool_selection_ends_independent_playback_and_fades_without_document_edits() {
    for tool in [
        CanvasTool::Selection,
        CanvasTool::Pen,
        CanvasTool::PathPoints,
        CanvasTool::Freehand,
    ] {
        let mut editor = Editor::default();
        editor.project.clips[0].blend_mode = BlendMode::Weighted;
        let original = editor.project.clone();
        editor.blend_preview.play(&editor.project, "intro").unwrap();
        editor
            .blend_preview
            .fade(&editor.project, "intro", 0.2, 3.)
            .unwrap();
        assert!(editor.blend_preview.ticking());
        editor.choose_tool(tool);
        assert!(!editor.blend_preview.ticking());
        assert!(!editor.preview_animation && !editor.playing && !editor.preview_mode);
        assert_eq!(editor.canvas_tool(), tool);
        assert_eq!(editor.project, original);
        assert!(!editor.history.can_undo());
    }
}

#[test]
fn document_replacement_revokes_trust_and_cancels_drawing() {
    let mut editor = Editor {
        trusted_js_export: true,
        ..Default::default()
    };
    editor.choose_tool(CanvasTool::Freehand);
    editor.sync_buffers();
    assert!(!editor.trusted_js_export);
    assert_eq!(editor.canvas_tool(), CanvasTool::Selection);
}

#[test]
fn real_tool_buttons_and_preview_enforce_exclusivity() {
    let ctx = egui::Context::default();
    let mut editor = Editor::default();
    for (label, tool) in [
        ("Pen", CanvasTool::Pen),
        ("Pfadpunkte", CanvasTool::PathPoints),
        ("Freihand", CanvasTool::Freehand),
        ("Auswahl", CanvasTool::Selection),
    ] {
        click(&mut editor, &ctx, label);
        assert_eq!(editor.canvas_tool(), tool);
        assert_eq!(
            usize::from(editor.pen.is_some())
                + usize::from(editor.edit_path_handles)
                + usize::from(editor.freehand_mode),
            usize::from(tool != CanvasTool::Selection)
        );
    }
    click(&mut editor, &ctx, "Freihand");
    click(&mut editor, &ctx, "Vorschau");
    assert!(editor.preview_mode);
    assert_eq!(editor.canvas_tool(), CanvasTool::Selection);
    click(&mut editor, &ctx, "Pen");
    assert!(!editor.preview_mode);
}

#[test]
fn primary_controls_visible_file_paths_disclosed_on_request() {
    let ctx = egui::Context::default();
    let mut editor = Editor::default();
    assert!(!editor.timeline_visible);
    frame(&mut editor, &ctx, vec![]);
    let initial = texts(&frame(&mut editor, &ctx, vec![]));
    for label in [
        "Gestalten",
        "Animieren",
        "Erweitert",
        "Speichern",
        "▶ Abspielen",
        "Vorschau",
        "Splinely Viewer öffnen",
    ] {
        assert!(initial.iter().any(|(text, _)| text == label), "{label}");
    }
    assert!(!initial
        .iter()
        .any(|(text, _)| text.contains("splinely-project.sply") || text.contains("splinely.svg")));
    click(&mut editor, &ctx, "Datei");
    let opened = texts(&frame(&mut editor, &ctx, vec![]));
    for label in [
        "Neues Projekt",
        "Öffnen …",
        "Speichern unter …",
        "SVG importieren …",
        "Interaktives SVG exportieren …",
        "Statisches SVG exportieren …",
        "Pfade manuell eingeben",
    ] {
        assert!(opened.iter().any(|(text, _)| text == label), "{label}");
    }
}

#[test]
fn animate_reveals_timeline_but_advanced_layer_controls_start_collapsed() {
    let ctx = egui::Context::default();
    let mut editor = Editor::default();
    click(&mut editor, &ctx, "Animieren");
    assert_eq!(editor.workspace, Workspace::Animate);
    assert!(editor.timeline_visible);
    let initial = texts(&frame(&mut editor, &ctx, vec![]));
    assert!(!initial
        .iter()
        .any(|(text, _)| text == "Ebenen abspielen" || text == "Gewicht"));
    click(&mut editor, &ctx, "Mehrere Clips & Überblendung");
    assert!(texts(&frame(&mut editor, &ctx, vec![]))
        .iter()
        .any(|(text, _)| text == "Ebenen abspielen"));
    click(&mut editor, &ctx, "Clip-Optionen & Mischgewicht");
    assert!(texts(&frame(&mut editor, &ctx, vec![]))
        .iter()
        .any(|(text, _)| text == "Gewicht"));
}

#[test]
fn frm_project_opens_without_svg_and_does_not_grant_code_trust() {
    let directory = std::env::temp_dir().join(format!(
        "splinely-editor-sply-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("single.sply");
    let mut project = demo();
    project.custom_js = "throw new Error('must not execute natively')".into();
    std::fs::write(&path, project.to_json().unwrap()).unwrap();
    let editor = Editor::open_project_file(path.to_str().unwrap()).unwrap();
    assert_eq!(editor.project, project);
    assert!(!editor.trusted_js_export);
    assert!(!editor.dirty);
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn shape_creation_leaves_preview_and_is_one_undoable_edit() {
    let ctx = egui::Context::default();
    let mut editor = Editor::default();
    click(&mut editor, &ctx, "Vorschau");
    assert!(editor.preview_mode);
    let before = editor.project.clone();
    click(&mut editor, &ctx, "+ Rechteck");
    assert!(!editor.preview_mode);
    assert_eq!(editor.canvas_tool(), CanvasTool::Selection);
    assert_eq!(editor.project.nodes.len(), before.nodes.len() + 1);
    assert!(editor.history.undo(&mut editor.project));
    assert_eq!(editor.project, before);
}

#[test]
fn retained_path_finish_and_cancel_leave_coherent_tool_state() {
    let ctx = egui::Context::default();
    let mut editor = Editor::default();
    click(&mut editor, &ctx, "Pen");
    editor
        .pen
        .as_mut()
        .unwrap()
        .add_anchor([10., 20.], None)
        .unwrap();
    editor
        .pen
        .as_mut()
        .unwrap()
        .add_anchor([80., 30.], None)
        .unwrap();
    let count = editor.project.nodes.len();
    click(&mut editor, &ctx, "Pfad fertigstellen");
    assert_eq!(editor.project.nodes.len(), count + 1);
    assert_eq!(editor.canvas_tool(), CanvasTool::PathPoints);
    assert!(!editor.freehand_mode);
    assert!(editor.history.undo(&mut editor.project));
    assert_eq!(editor.project.nodes.len(), count);
    click(&mut editor, &ctx, "Pen");
    click(&mut editor, &ctx, "Pfad verwerfen");
    assert_eq!(editor.canvas_tool(), CanvasTool::Selection);
    assert_eq!(editor.project.nodes.len(), count);
}

/// egui's bundled fonts lack many symbols; missing glyphs render as boxes.
/// Every non-ASCII character in a UI string literal must exist in the font.
#[test]
fn ui_symbols_exist_in_the_bundled_fonts() {
    let ctx = egui::Context::default();
    let _ = ctx.run(Default::default(), |_| {});
    let font = egui::FontId::proportional(14.);
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let mut missing = vec![];
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let source = std::fs::read_to_string(&path).unwrap();
        for (line_no, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            // Only string literals: text between quotes.
            for (i, part) in code.split('"').enumerate() {
                if i % 2 == 0 {
                    continue;
                }
                for c in part.chars().filter(|c| !c.is_ascii() && !c.is_alphabetic()) {
                    if !ctx.fonts(|f| f.has_glyph(&font, c)) {
                        missing.push(format!("{}:{} {c:?}", path.display(), line_no + 1));
                    }
                }
            }
        }
    }
    assert!(missing.is_empty(), "missing glyphs: {missing:#?}");
}

#[test]
fn double_click_on_canvas_text_opens_the_inline_editor() {
    let ctx = egui::Context::default();
    let mut editor = Editor::default();
    frame(&mut editor, &ctx, vec![]);
    frame(&mut editor, &ctx, vec![]);
    // Screen position of the demo's text from the canvas mapping.
    let text = editor
        .project
        .nodes
        .iter()
        .find(|n| matches!(n.kind, NodeKind::Text { .. }))
        .unwrap()
        .id
        .clone();
    let scene = editor.evaluate_scene(&EvaluationContext::default());
    let node = scene.iter().find(|n| n.id == text).unwrap();
    let rect = selection::scene_rect(node);
    // The canvas records where the document origin was painted.
    let origin = editor.debug_origin.expect("canvas painted");
    let pos = origin + rect.center().to_vec2() * editor.zoom;
    let click = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    for _ in 0..2 {
        frame(
            &mut editor,
            &ctx,
            vec![egui::Event::PointerMoved(pos), click(true)],
        );
        frame(&mut editor, &ctx, vec![click(false)]);
    }
    assert!(
        editor.text_draft.is_some(),
        "selected: {:?}",
        editor.selection()
    );
}

/// Every language renders the main workspaces without panics, and Latin-script
/// translations only use glyphs that egui bundles (CJK loads a system font).
#[test]
fn every_language_renders_and_has_glyphs() {
    use splinely_editor::i18n::{set_thread_language, tr, Lang};
    for lang in Lang::ALL {
        set_thread_language(Some(lang));
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        for workspace in [Workspace::Design, Workspace::Animate, Workspace::Advanced] {
            editor.workspace = workspace;
            let output = frame(&mut editor, &ctx, vec![]);
            assert!(!texts(&output).is_empty());
        }
        if lang != Lang::De {
            assert_ne!(tr("Speichern"), "Speichern", "{lang:?}");
        }
        if !lang.needs_cjk_font() {
            let font = egui::FontId::proportional(14.);
            let table = std::fs::read_to_string(format!(
                "{}/i18n/{}.json",
                env!("CARGO_MANIFEST_DIR"),
                lang.code()
            ))
            .unwrap_or_default();
            let missing: Vec<char> = table
                .chars()
                .filter(|c| !c.is_ascii() && !ctx.fonts(|f| f.has_glyph(&font, *c)))
                .collect();
            assert!(missing.is_empty(), "{lang:?} misses glyphs {missing:?}");
        }
    }
    set_thread_language(None);
}
