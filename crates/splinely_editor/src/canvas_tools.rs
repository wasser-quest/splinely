use super::vector::tools::HandleKind;
use super::*;
use eframe::egui::Stroke;
use splinely_document::path_edit;

impl Editor {
    pub(super) fn vector_toolbar(&mut self, ui: &mut egui::Ui) {
        if ui
            .button(tr("Pen"))
            .on_hover_text(tr(
                "Anker klicken, für Tangenten ziehen · Enter: fertig · Escape: verwerfen.",
            ))
            .clicked()
        {
            self.pen = Some(PenPath::default());
            self.pen_press = None;
            self.path_drag = None;
            self.drag_start = None;
            self.playing = false;
            self.preview_animation = false;
            self.preview_mode = false;
        }
        ui.checkbox(&mut self.edit_path_handles, tr("Pfadpunkte"))
            .on_hover_text(tr("Doppelklick nahe einer Kante fügt einen Anker ein, ohne die Form zu ändern. Runde Tangenten ziehen; Shift koppelt symmetrisch, Shift+Alt erhält die Gegenlänge. Escape bricht ab."));
        if self.pen.is_some() {
            if ui.button(tr("Pfad schließen")).clicked() {
                if let Err(error) = self.pen.as_mut().unwrap().close() {
                    self.status = error;
                }
            }
            if ui.button(tr("Pfad fertigstellen")).clicked() {
                self.finish_pen();
            }
            if ui.button(tr("Pfad verwerfen")).clicked() {
                self.pen = None;
                self.pen_press = None;
            }
        }
    }
    fn finish_pen(&mut self) {
        let id = self.new_id("path");
        let result = self
            .pen
            .as_ref()
            .map(|pen| pen.candidate(&self.project, &id));
        if let Some(result) = result {
            match result {
                Ok(next) => {
                    self.commit(next);
                    self.selected = Some(id);
                    self.pen = None;
                    self.pen_press = None;
                    self.edit_path_handles = true;
                    self.status = tr("Pfad erstellt · eckige Anker oder runde Griffe ziehen; Rückgängig entfernt den ganzen Pfad.").into();
                }
                Err(error) => self.status = error,
            }
        }
    }
    pub(super) fn vector_canvas(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        painter: &egui::Painter,
        origin: Pos2,
        scene: &[SceneNode],
    ) -> bool {
        let enabled = !self.playing
            && !self.preview_animation
            && !self.preview_mode
            && self.tab == Tab::Canvas;
        if !enabled {
            self.path_drag = None;
            self.pen_press = None;
            return false;
        }
        if !self.typing(ui.ctx()) && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            let was_active =
                self.pen.is_some() || self.path_drag.is_some() || self.point_marquee.is_some();
            self.pen = None;
            self.pen_press = None;
            self.path_drag = None;
            self.point_marquee = None;
            if was_active {
                return true;
            }
            if self.edit_path_handles && !self.point_selection.is_empty() {
                self.point_selection.clear();
                self.status = tr("Punktauswahl aufgehoben.").into();
                return true;
            }
        }
        if self.pen.is_some() {
            let map = CanvasMap::from_transform(origin, self.zoom as f64, Transform::default());
            if !self.typing(ui.ctx()) && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.finish_pen();
                return true;
            }
            if !self.typing(ui.ctx())
                && ui.input(|i| i.key_pressed(egui::Key::Backspace))
                && self.pen.as_mut().unwrap().remove_last_anchor()
            {
                self.pen_press = None;
            }
            // Clicking the first anchor closes and finishes the path.
            let closes = response.clicked()
                && response
                    .interact_pointer_pos()
                    .zip(
                        self.pen
                            .as_ref()
                            .and_then(|pen| pen.first_anchor().filter(|_| pen.anchor_count() >= 2)),
                    )
                    .and_then(|(pointer, first)| map.to_screen(first).ok().map(|p| (p, pointer)))
                    .is_some_and(|(p, pointer)| p.distance(pointer) <= 8.);
            if closes {
                match self.pen.as_mut().unwrap().close() {
                    Ok(()) => self.finish_pen(),
                    Err(error) => self.status = error,
                }
                return true;
            }
            if response.drag_started() {
                self.pen_press = ui
                    .input(|i| i.pointer.press_origin())
                    .and_then(|p| map.to_local(p).ok());
            }
            let result = if response.drag_stopped() {
                self.pen_press
                    .take()
                    .zip(response.interact_pointer_pos())
                    .map(|(anchor, end)| {
                        map.to_local(end).and_then(|outgoing| {
                            self.pen
                                .as_mut()
                                .unwrap()
                                .add_anchor(anchor, Some(outgoing))
                        })
                    })
            } else if response.clicked() {
                response.interact_pointer_pos().map(|p| {
                    map.to_local(p)
                        .and_then(|anchor| self.pen.as_mut().unwrap().add_anchor(anchor, None))
                })
            } else {
                None
            };
            if let Some(Err(error)) = result {
                self.status = error;
            }
            if let Some(pen) = &self.pen {
                let result = pen
                    .path()
                    .paint_outline(
                        painter,
                        &map,
                        Stroke::new(2_f32, Color32::from_rgb(30, 170, 210)),
                    )
                    .and_then(|()| pen.path().paint_handles(painter, &map, None));
                if let Err(error) = result {
                    self.status = error;
                }
                painter.text(
                    origin + Vec2::new(12., 12.),
                    egui::Align2::LEFT_TOP,
                    trf(
                        "Pen: {} Segmente · Enter: fertig · Escape: verwerfen",
                        &[format!("{}", pen.path().segments().len())],
                    ),
                    egui::FontId::proportional(12.),
                    Color32::from_rgb(20, 100, 190),
                );
            }
            return true;
        }
        if !self.edit_path_handles {
            self.path_drag = None;
            return false;
        }
        if self
            .path_drag
            .as_ref()
            .is_some_and(|(_, _, id)| self.selected.as_ref() != Some(id))
        {
            self.path_drag = None;
        }
        if self.point_owner != self.selected {
            self.point_selection.clear();
            self.point_marquee = None;
            self.point_owner = self.selected.clone();
        }
        let insert = response.double_clicked_by(egui::PointerButton::Primary);
        let (shift, alt) = ui.input(|i| (i.modifiers.shift, i.modifiers.alt));
        if insert {
            self.path_drag = None;
            self.point_marquee = None;
            self.drag_start = None;
        }
        if self.path_drag.is_none() {
            let target = self
                .project
                .nodes
                .iter()
                .find(|n| Some(&n.id) == self.selected.as_ref() && n.visible && !n.locked)
                .and_then(|node| match &node.kind {
                    NodeKind::Path { d, .. } => Some((node.id.clone(), d.clone())),
                    _ => None,
                })
                .and_then(|(id, d)| {
                    let evaluated = scene.iter().find(|n| n.id == id)?;
                    let transform = Transform {
                        x: evaluated.x as f64,
                        y: evaluated.y as f64,
                        rotation: evaluated.rotation as f64,
                        scale_x: evaluated.scale_x as f64,
                        scale_y: evaluated.scale_y as f64,
                    };
                    Some((id, d, transform))
                });
            if let Some((id, d, transform)) = target {
                let map = CanvasMap::from_transform(origin, self.zoom as f64, transform);
                self.point_map = Some((id.clone(), map));
                let path = match EditablePath::parse(&d) {
                    Ok(path) => path,
                    Err(error) => {
                        self.status = error;
                        return false;
                    }
                };
                let segments = path.segments();
                self.point_selection
                    .retain(|h| path_edit::canonical_anchor(segments, *h) == Some(*h));
                let hovered = response
                    .hover_pos()
                    .and_then(|p| path.hit_test_points(p, &map, 8., &self.point_selection));
                if let Err(error) = path.paint_points(painter, &map, &self.point_selection, hovered)
                {
                    self.status = error;
                }
                if insert {
                    if let Some(pointer) = response.interact_pointer_pos() {
                        match PathGesture::insert_candidate(&self.project, &id, pointer, &map, 8.) {
                            Ok(Some(next)) => {
                                self.commit(next);
                                self.status = tr("Anker eingefügt, Kurvenform erhalten · Shift+Ziehen koppelt die Gegentangente.").into();
                            }
                            Ok(None) => self.status = tr("Doppelklick innerhalb von 8 Pixeln einer Kante, abseits vorhandener Endpunkte.").into(),
                            Err(error) => self.status = error,
                        }
                    }
                    return true;
                }
                if let Some((start, end, additive)) = self.point_marquee {
                    let end = response.interact_pointer_pos().unwrap_or(end);
                    self.point_marquee = Some((start, end, additive));
                    let rect = egui::Rect::from_two_pos(start, end);
                    let accent = Color32::from_rgb(80, 130, 210);
                    painter.rect_filled(rect, 0., accent.gamma_multiply(0.15));
                    painter.rect_stroke(
                        rect,
                        0.,
                        Stroke::new(1_f32, accent),
                        egui::StrokeKind::Inside,
                    );
                    if response.drag_stopped() {
                        self.point_marquee = None;
                        let found = path_edit::anchors_in_rect(
                            segments,
                            [start.x as f64, start.y as f64],
                            [end.x as f64, end.y as f64],
                            |p| map.to_screen(p).ok().map(|s| [s.x as f64, s.y as f64]),
                        );
                        if !additive {
                            self.point_selection.clear();
                        }
                        self.point_selection.extend(found);
                        let count = self.point_selection.len();
                        self.status = trf("{count} Punkte ausgewählt.", &[format!("{count}")]);
                    }
                    return true;
                }
                if response.clicked() {
                    let hit = response
                        .interact_pointer_pos()
                        .and_then(|p| path.hit_test_points(p, &map, 8., &self.point_selection));
                    match hit {
                        Some(h) if h.kind == HandleKind::Anchor => {
                            if !shift {
                                self.point_selection.clear();
                                self.point_selection.insert(h);
                            } else if !self.point_selection.remove(&h) {
                                self.point_selection.insert(h);
                            }
                            return true;
                        }
                        Some(_) => return true,
                        None if !shift && !self.point_selection.is_empty() => {
                            self.point_selection.clear();
                            return true;
                        }
                        None => {}
                    }
                }
                if response.drag_started() {
                    if let Some(press) = ui.input(|i| i.pointer.press_origin()) {
                        match path.hit_test_points(press, &map, 8., &self.point_selection) {
                            Some(h) => {
                                if h.kind == HandleKind::Anchor
                                    && !self.point_selection.contains(&h)
                                {
                                    if !shift {
                                        self.point_selection.clear();
                                    }
                                    self.point_selection.insert(h);
                                }
                                match PathGesture::begin_with(
                                    &self.project,
                                    &id,
                                    press,
                                    &map,
                                    Some(&self.point_selection),
                                ) {
                                    Ok(Some(gesture)) => {
                                        self.path_drag = Some((gesture, map, id));
                                        self.drag_start = None;
                                    }
                                    Ok(None) => {}
                                    Err(error) => self.status = error,
                                }
                            }
                            None => {
                                // Drag on empty canvas: rubber-band point selection.
                                self.point_marquee = Some((press, press, shift));
                                self.drag_start = None;
                                return true;
                            }
                        }
                    }
                }
            }
        }
        if insert {
            return true; // A double-click must never start an ordinary object move.
        }
        if let Some((gesture, map, _)) = self.path_drag.as_mut() {
            if let Some(pointer) = response.interact_pointer_pos() {
                let result = if shift {
                    let link = if alt {
                        super::vector::tools::TangentLink::Smooth
                    } else {
                        super::vector::tools::TangentLink::Symmetric
                    };
                    gesture.update_linked(pointer, map, link)
                } else {
                    gesture.update(pointer, map)
                };
                if let Err(error) = result {
                    self.status = error;
                }
            }
            let result = gesture
                .path
                .paint_outline(
                    painter,
                    map,
                    Stroke::new(2_f32, Color32::from_rgb(30, 170, 210)),
                )
                .and_then(|()| {
                    gesture.path.paint_points(
                        painter,
                        map,
                        &self.point_selection,
                        Some(gesture.handle),
                    )
                });
            if let Err(error) = result {
                self.status = error;
            }
            if response.drag_stopped() {
                let (gesture, _, _) = self.path_drag.take().unwrap();
                match gesture.candidate(&self.project) {
                    Ok(next) => self.commit(next),
                    Err(error) => self.status = error,
                }
            }
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::super::path_points::PointOp;
    use super::*;

    fn editor() -> Editor {
        let mut editor = Editor {
            project: Project::default(),
            ..Default::default()
        };
        let d = "M20 20 C40 20 60 20 80 20 C100 20 120 20 140 20";
        editor.project.nodes.push(Node::new(
            "path",
            NodeKind::Path {
                d: d.into(),
                bounds: splinely_document::path_bounds(d).unwrap(),
                evenodd: false,
            },
        ));
        editor.selected = Some("path".into());
        editor.edit_path_handles = true;
        editor
    }
    fn frame(
        editor: &mut Editor,
        ctx: &egui::Context,
        time: f64,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> bool {
        let scene =
            editor
                .projection
                .evaluate_with_context(&editor.project, 0., &Default::default());
        let mut consumed = false;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, Vec2::splat(300.))),
                time: Some(time),
                events,
                modifiers,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = ui.allocate_rect(
                        egui::Rect::from_min_size(Pos2::ZERO, Vec2::splat(300.)),
                        egui::Sense::click_and_drag(),
                    );
                    consumed =
                        editor.vector_canvas(ui, &response, ui.painter(), Pos2::ZERO, &scene);
                });
            },
        );
        consumed
    }
    fn click(p: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }
    #[test]
    fn double_click_inserts_once_and_miss_still_consumes() {
        let ctx = egui::Context::default();
        let mut editor = editor();
        let p = egui::pos2(50., 20.);
        frame(&mut editor, &ctx, 0., vec![], Default::default());
        for (time, pressed) in [(0.1, true), (0.15, false), (0.2, true)] {
            frame(
                &mut editor,
                &ctx,
                time,
                vec![egui::Event::PointerMoved(p), click(p, pressed)],
                Default::default(),
            );
        }
        assert!(frame(
            &mut editor,
            &ctx,
            0.25,
            vec![click(p, false)],
            Default::default()
        ));
        assert_eq!(editor.history_steps, 1);
        assert!(editor.path_drag.is_none() && editor.drag_start.is_none());
        let before = editor.project.clone();
        let miss = egui::pos2(250., 250.);
        for (time, pressed) in [(1., true), (1.05, false), (1.1, true)] {
            frame(
                &mut editor,
                &ctx,
                time,
                vec![egui::Event::PointerMoved(miss), click(miss, pressed)],
                Default::default(),
            );
        }
        assert!(frame(
            &mut editor,
            &ctx,
            1.15,
            vec![click(miss, false)],
            Default::default()
        ));
        assert_eq!(editor.project, before);
        assert_eq!(editor.history_steps, 1);
    }
    #[test]
    fn shift_preview_is_staged_and_escape_cancels() {
        let ctx = egui::Context::default();
        let mut editor = editor();
        let before = editor.project.clone();
        frame(&mut editor, &ctx, 0., vec![], Default::default());
        let map = CanvasMap::from_transform(Pos2::ZERO, 1., Transform::default());
        let gesture = PathGesture::begin(&editor.project, "path", egui::pos2(100., 20.), &map)
            .unwrap()
            .unwrap();
        editor.path_drag = Some((gesture, map, "path".into()));
        let shift = egui::Modifiers {
            shift: true,
            ..Default::default()
        };
        let pointer = egui::pos2(80., 40.);
        assert!(frame(
            &mut editor,
            &ctx,
            0.1,
            vec![
                egui::Event::PointerMoved(pointer),
                egui::Event::PointerButton {
                    pos: pointer,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: shift,
                }
            ],
            shift
        ));
        assert_eq!(editor.project, before);
        let path = &editor.path_drag.as_ref().unwrap().0.path;
        assert!(
            matches!(
                path.segments()[1],
                super::super::vector::tools::Segment::Cubic {
                    second: [80., 0.],
                    ..
                }
            ),
            "{}; {}",
            path.path_data(),
            editor.status
        );
        assert!(frame(
            &mut editor,
            &ctx,
            0.2,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            Default::default()
        ));
        assert!(editor.path_drag.is_none());
        assert_eq!(editor.project, before);
        assert_eq!(editor.history_steps, 0);
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }
    }
    /// Press at `from`, move to `to` over a few frames, release at `to`.
    fn drag(
        editor: &mut Editor,
        ctx: &egui::Context,
        t: f64,
        from: Pos2,
        to: Pos2,
        m: egui::Modifiers,
    ) {
        frame(editor, ctx, t, vec![egui::Event::PointerMoved(from)], m);
        frame(editor, ctx, t + 0.01, vec![click(from, true)], m);
        let mid = from + (to - from) * 0.5;
        frame(
            editor,
            ctx,
            t + 0.05,
            vec![egui::Event::PointerMoved(mid)],
            m,
        );
        frame(editor, ctx, t + 0.1, vec![egui::Event::PointerMoved(to)], m);
        frame(editor, ctx, t + 0.15, vec![click(to, false)], m);
        frame(editor, ctx, t + 0.2, vec![], m);
    }
    fn tap(editor: &mut Editor, ctx: &egui::Context, t: f64, p: Pos2, m: egui::Modifiers) {
        frame(editor, ctx, t, vec![egui::Event::PointerMoved(p)], m);
        frame(editor, ctx, t + 0.01, vec![click(p, true)], m);
        frame(editor, ctx, t + 0.02, vec![click(p, false)], m);
    }
    fn path_d(editor: &Editor) -> String {
        match &editor.project.nodes[0].kind {
            NodeKind::Path { d, .. } => d.clone(),
            _ => unreachable!(),
        }
    }
    fn anchor(i: usize) -> super::super::vector::tools::Handle {
        super::super::vector::tools::Handle::anchor(i)
    }
    const SHIFT: egui::Modifiers = egui::Modifiers {
        alt: false,
        ctrl: false,
        shift: true,
        mac_cmd: false,
        command: false,
    };

    #[test]
    fn click_shift_click_and_empty_click_select_points() {
        let ctx = egui::Context::default();
        let mut editor = editor();
        frame(&mut editor, &ctx, 0., vec![], Default::default());
        tap(
            &mut editor,
            &ctx,
            0.1,
            egui::pos2(20., 20.),
            Default::default(),
        );
        assert_eq!(editor.point_selection, [anchor(0)].into());
        tap(&mut editor, &ctx, 1., egui::pos2(140., 20.), SHIFT);
        assert_eq!(editor.point_selection, [anchor(0), anchor(2)].into());
        tap(&mut editor, &ctx, 2., egui::pos2(20., 20.), SHIFT);
        assert_eq!(editor.point_selection, [anchor(2)].into());
        tap(
            &mut editor,
            &ctx,
            3.,
            egui::pos2(200., 200.),
            Default::default(),
        );
        assert!(editor.point_selection.is_empty());
        assert_eq!(editor.history_steps, 0);
    }

    #[test]
    fn marquee_selects_points_and_drag_moves_them_with_handles() {
        let ctx = egui::Context::default();
        let mut editor = editor();
        frame(&mut editor, &ctx, 0., vec![], Default::default());
        drag(
            &mut editor,
            &ctx,
            0.1,
            egui::pos2(70., 0.),
            egui::pos2(150., 40.),
            Default::default(),
        );
        assert_eq!(editor.point_selection, [anchor(1), anchor(2)].into());
        assert_eq!(editor.history_steps, 0);
        // Dragging one selected anchor moves both, handles included.
        drag(
            &mut editor,
            &ctx,
            1.,
            egui::pos2(80., 20.),
            egui::pos2(80., 50.),
            Default::default(),
        );
        assert_eq!(editor.history_steps, 1, "{}", editor.status);
        assert_eq!(
            path_d(&editor),
            "M20 20 C40 20 60 50 80 50 C100 50 120 50 140 50"
        );
        assert_eq!(editor.point_selection, [anchor(1), anchor(2)].into());
    }

    #[test]
    fn dragging_an_unselected_anchor_selects_it_and_carries_its_handles() {
        let ctx = egui::Context::default();
        let mut editor = editor();
        frame(&mut editor, &ctx, 0., vec![], Default::default());
        drag(
            &mut editor,
            &ctx,
            0.1,
            egui::pos2(80., 20.),
            egui::pos2(80., 30.),
            Default::default(),
        );
        assert_eq!(editor.point_selection, [anchor(1)].into());
        assert_eq!(
            path_d(&editor),
            "M20 20 C40 20 60 30 80 30 C100 30 120 20 140 20"
        );
    }

    #[test]
    fn point_keys_delete_nudge_and_change_node_types() {
        let mut editor = editor();
        editor.tab = Tab::Canvas;
        editor.point_owner = Some("path".into());
        editor.point_selection = [anchor(1)].into();
        assert!(editor.nudge_points(Vec2::new(0., 5.)));
        assert_eq!(
            path_d(&editor),
            "M20 20 C40 20 60 25 80 25 C100 25 120 20 140 20"
        );
        assert!(editor.point_op_allowed(PointOp::Cusp));
        assert!(!editor.point_op_allowed(PointOp::Join));
        assert!(editor.apply_point_op(PointOp::Retract));
        assert_eq!(
            path_d(&editor),
            "M20 20 C40 20 80 25 80 25 C80 25 120 20 140 20"
        );
        assert!(editor.apply_point_op(PointOp::Extend));
        assert_ne!(
            path_d(&editor),
            "M20 20 C40 20 80 25 80 25 C80 25 120 20 140 20"
        );
        assert!(editor.delete_points());
        assert_eq!(path_d(&editor).matches(['C', 'L']).count(), 1);
        assert!(editor.point_selection.is_empty());
        // Each point edit is one undo step.
        assert!(editor.history.undo(&mut editor.project));
        assert_eq!(path_d(&editor).matches('C').count(), 2);
    }

    #[test]
    fn copy_and_paste_points_adds_an_offset_subpath() {
        let ctx = egui::Context::default();
        let mut editor = editor();
        editor.tab = Tab::Canvas;
        editor.point_owner = Some("path".into());
        editor.point_selection = [anchor(0), anchor(1)].into();
        assert!(editor.copy_points(&ctx));
        assert!(editor.paste_points(None));
        let d = path_d(&editor);
        assert_eq!(d.matches('M').count(), 2, "{d}");
        assert_eq!(editor.point_selection.len(), 2);
        // Deleting every point removes the shape.
        editor.set_selection(vec!["path".into()]);
        editor.machine_buffer = "[]".into();
        assert!(editor.select_all_points());
        assert!(editor.delete_points());
        assert!(editor.project.nodes.is_empty(), "{}", editor.status);
    }

    #[test]
    fn pen_backspace_removes_last_anchor_and_first_anchor_click_closes() {
        let ctx = egui::Context::default();
        let mut editor = editor();
        editor.edit_path_handles = false;
        editor.pen = Some(PenPath::default());
        frame(&mut editor, &ctx, 0., vec![], Default::default());
        for (i, p) in [(20., 200.), (100., 200.), (100., 260.)]
            .into_iter()
            .enumerate()
        {
            tap(
                &mut editor,
                &ctx,
                1. + i as f64,
                egui::pos2(p.0, p.1),
                Default::default(),
            );
        }
        assert_eq!(editor.pen.as_ref().unwrap().anchor_count(), 3);
        frame(
            &mut editor,
            &ctx,
            5.,
            vec![key(egui::Key::Backspace)],
            Default::default(),
        );
        assert_eq!(editor.pen.as_ref().unwrap().anchor_count(), 2);
        tap(
            &mut editor,
            &ctx,
            6.,
            egui::pos2(100., 260.),
            Default::default(),
        );
        tap(
            &mut editor,
            &ctx,
            7.,
            egui::pos2(21., 201.),
            Default::default(),
        );
        assert!(editor.pen.is_none(), "{}", editor.status);
        let created = editor.project.nodes.last().unwrap();
        let NodeKind::Path { d, .. } = &created.kind else {
            panic!("pen creates a path");
        };
        assert!(d.trim_end().ends_with(['Z', 'z']), "{d}");
    }

    #[test]
    fn history_limit_drops_oldest_steps_instead_of_clearing() {
        let mut editor = editor();
        editor.settings.undo_steps = 10;
        for i in 0..15 {
            let mut next = editor.project.clone();
            next.nodes[0].name = format!("step {i}");
            editor.commit(next);
        }
        assert_eq!(editor.history_steps, 10);
        for _ in 0..10 {
            assert!(editor.history.undo(&mut editor.project));
        }
        assert!(!editor.history.undo(&mut editor.project));
        assert_eq!(editor.project.nodes[0].name, "step 4");
    }
}
