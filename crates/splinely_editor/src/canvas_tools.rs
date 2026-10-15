use super::*;
use eframe::egui::Stroke;

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
        if !ui.ctx().wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            let was_active = self.pen.is_some() || self.path_drag.is_some();
            self.pen = None;
            self.pen_press = None;
            self.path_drag = None;
            if was_active {
                return true;
            }
        }
        if self.pen.is_some() {
            let map = CanvasMap::from_transform(origin, self.zoom as f64, Transform::default());
            if !ui.ctx().wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.finish_pen();
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
        let insert = response.double_clicked_by(egui::PointerButton::Primary);
        let (shift, alt) = ui.input(|i| (i.modifiers.shift, i.modifiers.alt));
        if insert {
            self.path_drag = None;
            self.drag_start = None;
        }
        if self.path_drag.is_none() {
            if let Some(node) = self
                .project
                .nodes
                .iter()
                .find(|n| Some(&n.id) == self.selected.as_ref() && n.visible && !n.locked)
            {
                if let NodeKind::Path { d, .. } = &node.kind {
                    if let Some(evaluated) = scene.iter().find(|n| n.id == node.id) {
                        let map = CanvasMap::from_transform(
                            origin,
                            self.zoom as f64,
                            Transform {
                                x: evaluated.x as f64,
                                y: evaluated.y as f64,
                                rotation: evaluated.rotation as f64,
                                scale_x: evaluated.scale_x as f64,
                                scale_y: evaluated.scale_y as f64,
                            },
                        );
                        match EditablePath::parse(d)
                            .and_then(|path| path.paint_handles(painter, &map, None))
                        {
                            Ok(()) => {}
                            Err(error) => self.status = error,
                        }
                        if insert {
                            if let Some(pointer) = response.interact_pointer_pos() {
                                match PathGesture::insert_candidate(&self.project, &node.id, pointer, &map, 8.) {
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
                        if response.drag_started() {
                            if let Some(press) = ui.input(|i| i.pointer.press_origin()) {
                                match PathGesture::begin(&self.project, &node.id, press, &map) {
                                    Ok(Some(gesture)) => {
                                        self.path_drag = Some((gesture, map, node.id.clone()));
                                        self.drag_start = None;
                                    }
                                    Ok(None) => {}
                                    Err(error) => self.status = error,
                                }
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
                    gesture
                        .path
                        .paint_handles(painter, map, Some(gesture.handle))
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
}
