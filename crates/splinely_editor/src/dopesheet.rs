//! Visual timeline ("dope sheet") for the active clip: a time ruler with a
//! draggable playhead, one row per track with keyframe diamonds. Drag a
//! diamond to retime it (snaps to the time grid from the settings, Alt: free), double-click a row to
//! insert a key with the track's current value, Delete removes the selected key.
use super::*;

const LABEL_WIDTH: f32 = 170.;
const ROW: f32 = 22.;
const RULER: f32 = 22.;

/// A keyframe being dragged: (track index, key index).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct KeyRef {
    pub track: usize,
    pub key: usize,
}

/// Easing presets offered in the keyframe menu (CSS-style curves).
const EASINGS: &[(&str, Easing)] = &[
    ("Linear", Easing::Linear),
    (
        "Weich (ease)",
        Easing::CubicBezier {
            x1: 0.25,
            y1: 0.1,
            x2: 0.25,
            y2: 1.,
        },
    ),
    (
        "Beschleunigen (ease-in)",
        Easing::CubicBezier {
            x1: 0.42,
            y1: 0.,
            x2: 1.,
            y2: 1.,
        },
    ),
    (
        "Abbremsen (ease-out)",
        Easing::CubicBezier {
            x1: 0.,
            y1: 0.,
            x2: 0.58,
            y2: 1.,
        },
    ),
    (
        "Weich rein und raus (ease-in-out)",
        Easing::CubicBezier {
            x1: 0.42,
            y1: 0.,
            x2: 0.58,
            y2: 1.,
        },
    ),
    ("Sprung (Stufe)", Easing::Step),
];

fn nice_step(duration: f64, width: f32) -> f64 {
    let target = duration * 70. / width.max(1.) as f64;
    [0.05, 0.1, 0.25, 0.5, 1., 2., 5., 10., 30., 60.]
        .into_iter()
        .find(|s| *s >= target)
        .unwrap_or(120.)
}

impl Editor {
    fn track_label(&self, track: &Track) -> String {
        let name = self
            .project
            .nodes
            .iter()
            .find(|n| n.id == track.node_id)
            .map(|n| {
                if n.name.is_empty() {
                    n.id.clone()
                } else {
                    n.name.clone()
                }
            })
            .unwrap_or_else(|| trf("Knochen {}", std::slice::from_ref(&track.node_id)));
        let property = serde_json::to_value(track.property)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        trf(
            "{name} · {property}",
            &[name.to_string(), property.to_string()],
        )
    }

    pub(super) fn dopesheet_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            self.autokey_toggle_ui(ui);
            if self.auto_key {
                ui.weak(tr("Formen verschieben, drehen oder umfärben setzt Schlüsselbilder am Abspielkopf."));
            }
        });
        let Some(clip) = self.project.clips.get(self.active_clip).cloned() else {
            ui.weak(tr("Noch kein Clip · „+ Clip“ legt einen an, dann Formen mit „◼ Aktuellen Wert merken“ animieren."));
            return;
        };
        let duration = clip.duration.max(0.001);
        let rows = clip.tracks.len();
        let width = ui.available_width();
        let height = RULER + ROW * rows.max(1) as f32 + 4.;
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, height), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals().clone();
        painter.rect_filled(rect, 4., visuals.extreme_bg_color);
        let lane = Rect::from_min_max(
            Pos2::new(rect.min.x + LABEL_WIDTH, rect.min.y),
            Pos2::new(rect.max.x - 10., rect.max.y),
        );
        let x_of = |t: f64| lane.min.x + (t / duration) as f32 * lane.width();
        let t_of =
            |x: f32| (((x - lane.min.x) / lane.width()) as f64 * duration).clamp(0., duration);
        let row_rect = |i: usize| {
            Rect::from_min_size(
                Pos2::new(rect.min.x, rect.min.y + RULER + i as f32 * ROW),
                egui::vec2(rect.width(), ROW),
            )
        };
        // Ruler with ticks and labels.
        let step = nice_step(duration, lane.width());
        let mut t = 0.;
        while t <= duration + 1e-9 {
            let x = x_of(t);
            painter.line_segment(
                [
                    Pos2::new(x, rect.min.y + RULER - 6.),
                    Pos2::new(x, rect.max.y),
                ],
                egui::Stroke::new(1_f32, visuals.weak_text_color().gamma_multiply(0.25)),
            );
            painter.text(
                Pos2::new(x + 2., rect.min.y + 3.),
                egui::Align2::LEFT_TOP,
                format!("{}", (t * 100.).round() / 100.),
                egui::FontId::monospace(10.),
                visuals.weak_text_color(),
            );
            t += step;
        }
        // Rows: labels and diamonds.
        let selected_ids = self.selection();
        for (i, track) in clip.tracks.iter().enumerate() {
            let r = row_rect(i);
            if i % 2 == 1 {
                painter.rect_filled(r, 0., visuals.faint_bg_color);
            }
            let highlighted = selected_ids.contains(&track.node_id);
            painter.text(
                r.left_center() + egui::vec2(6., 0.),
                egui::Align2::LEFT_CENTER,
                self.track_label(track),
                egui::FontId::proportional(12.),
                if highlighted {
                    visuals.strong_text_color()
                } else {
                    visuals.text_color()
                },
            );
            for (k, key) in track.keyframes.iter().enumerate() {
                let c = Pos2::new(x_of(key.time), r.center().y);
                let selected = self.selected_key == Some(KeyRef { track: i, key: k });
                let s = if selected { 6. } else { 5. };
                // Shape shows the easing out of this key: diamond = linear,
                // square = step, rounded = eased curve.
                let shape: Vec<Pos2> = match key.easing {
                    Easing::Linear => vec![
                        c + egui::vec2(0., -s),
                        c + egui::vec2(s, 0.),
                        c + egui::vec2(0., s),
                        c + egui::vec2(-s, 0.),
                    ],
                    Easing::Step => vec![
                        c + egui::vec2(-s * 0.8, -s * 0.8),
                        c + egui::vec2(s * 0.8, -s * 0.8),
                        c + egui::vec2(s * 0.8, s * 0.8),
                        c + egui::vec2(-s * 0.8, s * 0.8),
                    ],
                    Easing::CubicBezier { .. } => (0..12)
                        .map(|i| {
                            let a = i as f32 * std::f32::consts::TAU / 12.;
                            c + egui::vec2(a.cos() * s, a.sin() * s)
                        })
                        .collect(),
                };
                let fill = if selected {
                    visuals.selection.bg_fill
                } else if matches!(key.easing, Easing::Step) {
                    visuals.warn_fg_color
                } else {
                    Color32::from_rgb(230, 190, 80)
                };
                painter.add(egui::Shape::convex_polygon(
                    shape,
                    fill,
                    egui::Stroke::new(1_f32, Color32::BLACK),
                ));
            }
        }
        if rows == 0 {
            painter.text(
                row_rect(0).center(),
                egui::Align2::CENTER_CENTER,
                tr("Keine Spuren · Form wählen und im Inspektor „◼ Aktuellen Wert merken“"),
                egui::FontId::proportional(12.),
                visuals.weak_text_color(),
            );
        }
        // Playhead.
        let x = x_of(self.time.min(duration));
        painter.line_segment(
            [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
            egui::Stroke::new(2_f32, Color32::from_rgb(230, 70, 70)),
        );
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(x - 5., rect.min.y),
                Pos2::new(x + 5., rect.min.y),
                Pos2::new(x, rect.min.y + 7.),
            ],
            Color32::from_rgb(230, 70, 70),
            egui::Stroke::NONE,
        ));

        // Interaction.
        let hit_key = |p: Pos2| -> Option<KeyRef> {
            let row = ((p.y - rect.min.y - RULER) / ROW).floor();
            if row < 0. || row as usize >= rows || p.x < lane.min.x - 6. {
                return None;
            }
            let track = row as usize;
            clip.tracks[track]
                .keyframes
                .iter()
                .enumerate()
                .filter(|(_, k)| (x_of(k.time) - p.x).abs() <= 7.)
                .min_by(|a, b| {
                    (x_of(a.1.time) - p.x)
                        .abs()
                        .total_cmp(&(x_of(b.1.time) - p.x).abs())
                })
                .map(|(key, _)| KeyRef { track, key })
        };
        let scrub = |editor: &mut Editor, x: f32| {
            editor.time = (t_of(x) * 100.).round() / 100.;
            editor.playing = false;
            editor.preview_animation = true;
            editor.audio_preview.seek(editor.time, false);
        };
        if response.drag_started() {
            if let Some(press) = ui.input(|i| i.pointer.press_origin()) {
                self.key_drag = hit_key(press);
                if let Some(key) = self.key_drag {
                    self.selected_key = Some(key);
                }
            }
        }
        if response.dragged() {
            if let Some(p) = response.interact_pointer_pos() {
                match self.key_drag {
                    Some(key) => {
                        let free = ui.input(|i| i.modifiers.alt);
                        self.move_key(key, t_of(p.x), free);
                    }
                    None if p.x >= lane.min.x - 6. => scrub(self, p.x),
                    None => {}
                }
            }
        }
        if response.drag_stopped() {
            self.key_drag = None;
        }
        if response.clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                match hit_key(p) {
                    Some(key) => {
                        self.selected_key = Some(key);
                        self.time = clip.tracks[key.track].keyframes[key.key].time;
                        self.preview_animation = true;
                        self.playing = false;
                    }
                    None if p.x >= lane.min.x => {
                        self.selected_key = None;
                        scrub(self, p.x);
                    }
                    None => {
                        // Label click selects the animated shape.
                        let row = ((p.y - rect.min.y - RULER) / ROW).floor();
                        if row >= 0. && (row as usize) < rows {
                            let id = clip.tracks[row as usize].node_id.clone();
                            if self.project.nodes.iter().any(|n| n.id == id) {
                                self.set_selection(vec![id]);
                            }
                        }
                    }
                }
            }
        }
        if response.double_clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                let row = ((p.y - rect.min.y - RULER) / ROW).floor();
                if row >= 0. && (row as usize) < rows && p.x >= lane.min.x && hit_key(p).is_none() {
                    let time = snap_time(t_of(p.x), self.settings.key_snap);
                    self.insert_key(row as usize, time);
                }
            }
        }
        if self.selected_key.is_some() {
            let delete = ui
                .input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
            if delete && !self.typing(ui.ctx()) {
                if let Some(key) = self.selected_key.take() {
                    self.delete_key(key);
                }
            }
        }
        // Right-click on a key: easing presets and delete.
        if response.secondary_clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                self.selected_key = hit_key(p);
            }
        }
        let menu_key = self.selected_key;
        response
            .on_hover_text(
                tr("Lineal/Leerraum: Abspielkopf ziehen · Raute ziehen: Zeit ändern (Alt: ohne Raster) · Doppelklick: Schlüsselbild einfügen · Rechtsklick: Übergang wählen · Entf: löschen · Name: Form wählen"),
            )
            .context_menu(|ui| {
                let Some(key) = menu_key else {
                    ui.weak(tr("Rechtsklick auf eine Raute"));
                    return;
                };
                ui.weak(tr("Übergang zum nächsten Schlüsselbild"));
                for (label, easing) in EASINGS {
                    if ui.button(tr(label)).clicked() {
                        self.set_easing(key, *easing, false);
                        ui.close_menu();
                    }
                }
                ui.separator();
                ui.menu_button(tr("Für die ganze Spur"), |ui| {
                    for (label, easing) in EASINGS {
                        if ui.button(tr(label)).clicked() {
                            self.set_easing(key, *easing, true);
                            ui.close_menu();
                        }
                    }
                });
                ui.separator();
                if ui.button(tr("Schlüsselbild kopieren")).clicked() {
                    self.copy_key(key);
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        self.key_clipboard.is_some(),
                        egui::Button::new(tr("Am Abspielkopf einfügen")),
                    )
                    .clicked()
                {
                    self.paste_key();
                    ui.close_menu();
                }
                if ui.button(tr("Schlüsselbild löschen")).clicked() {
                    self.selected_key = None;
                    self.delete_key(key);
                    ui.close_menu();
                }
            });
    }

    /// Set the easing of one key, or of every key in its track.
    pub(super) fn set_easing(&mut self, key: KeyRef, easing: Easing, whole_track: bool) {
        let mut next = self.project.clone();
        let Some(track) = next
            .clips
            .get_mut(self.active_clip)
            .and_then(|c| c.tracks.get_mut(key.track))
        else {
            return;
        };
        if whole_track {
            for k in &mut track.keyframes {
                k.easing = easing;
            }
        } else if let Some(k) = track.keyframes.get_mut(key.key) {
            k.easing = easing;
        }
        self.commit(next);
        self.status = tr("Übergang geändert.").into();
    }

    /// Retime one keyframe, kept strictly between its neighbours.
    pub(super) fn move_key(&mut self, key: KeyRef, time: f64, free: bool) {
        let snap = self.settings.key_snap;
        let mut next = self.project.clone();
        let Some(clip) = next.clips.get_mut(self.active_clip) else {
            return;
        };
        let duration = clip.duration;
        let Some(track) = clip.tracks.get_mut(key.track) else {
            return;
        };
        let time = if free { time } else { snap_time(time, snap) };
        let lo = if key.key == 0 {
            0.
        } else {
            track.keyframes[key.key - 1].time + 0.001
        };
        let hi = track
            .keyframes
            .get(key.key + 1)
            .map_or(duration, |k| k.time - 0.001);
        if lo > hi {
            return;
        }
        track.keyframes[key.key].time = time.clamp(lo, hi);
        self.time = track.keyframes[key.key].time;
        self.preview_animation = true;
        self.commit(next);
    }

    /// Insert a key at `time` carrying the track's current value there.
    pub(super) fn insert_key(&mut self, track: usize, time: f64) {
        let mut next = self.project.clone();
        let Some(clip) = next.clips.get_mut(self.active_clip) else {
            return;
        };
        let time = time.clamp(0., clip.duration);
        let Some(t) = clip.tracks.get_mut(track) else {
            return;
        };
        if t.keyframes.iter().any(|k| (k.time - time).abs() < 1e-6) {
            return;
        }
        let value = t.sample(time);
        let index = t.keyframes.partition_point(|k| k.time < time);
        t.keyframes.insert(
            index,
            Keyframe {
                time,
                value,
                easing: Easing::Linear,
            },
        );
        self.commit(next);
        self.selected_key = Some(KeyRef { track, key: index });
        self.time = time;
        self.preview_animation = true;
        self.status =
            tr("Schlüsselbild eingefügt · Wert im Inspektor ändern und erneut merken.").into();
    }

    /// Copy a keyframe (value and easing) for pasting at another time or track.
    pub(super) fn copy_key(&mut self, key: KeyRef) {
        let Some(track) = self
            .project
            .clips
            .get(self.active_clip)
            .and_then(|c| c.tracks.get(key.track))
        else {
            return;
        };
        let Some(keyframe) = track.keyframes.get(key.key) else {
            return;
        };
        self.key_clipboard = Some((track.node_id.clone(), track.property, keyframe.clone()));
        self.paste_key_next = true;
        self.status =
            tr("Schlüsselbild kopiert · Abspielkopf setzen und Strg+V fügt es ein.").into();
    }

    /// Paste the copied keyframe at the playhead: into the selected key's track
    /// when the value fits, otherwise into the track it was copied from (created
    /// in the active clip if needed). A key at the same time is replaced.
    pub(super) fn paste_key(&mut self) {
        let Some((node, property, keyframe)) = self.key_clipboard.clone() else {
            return;
        };
        let mut next = self.project.clone();
        let Some(clip) = next.clips.get_mut(self.active_clip) else {
            self.status = tr("Zuerst in der Zeitleiste einen Clip anlegen.").into();
            return;
        };
        let time = self.time.clamp(0., clip.duration);
        let selected = self.selected_key.map(|k| k.track).filter(|&t| {
            clip.tracks.get(t).is_some_and(|track| {
                track
                    .keyframes
                    .first()
                    .is_some_and(|k| k.value.compatible(&keyframe.value))
            })
        });
        let index = selected
            .or_else(|| {
                clip.tracks
                    .iter()
                    .position(|t| t.node_id == node && t.property == property)
            })
            .unwrap_or_else(|| {
                clip.tracks.push(Track {
                    node_id: node.clone(),
                    property,
                    keyframes: vec![],
                });
                clip.tracks.len() - 1
            });
        let track = &mut clip.tracks[index];
        track.keyframes.retain(|k| (k.time - time).abs() >= 1e-6);
        let at = track.keyframes.partition_point(|k| k.time < time);
        track.keyframes.insert(at, Keyframe { time, ..keyframe });
        if let Err(error) = next.validate() {
            self.status = error.to_string();
            return;
        }
        self.commit(next);
        self.selected_key = Some(KeyRef {
            track: index,
            key: at,
        });
        self.preview_animation = true;
        self.status = tr("Schlüsselbild eingefügt · Strg+Z macht es rückgängig.").into();
    }

    pub(super) fn delete_key(&mut self, key: KeyRef) {
        let mut next = self.project.clone();
        let Some(clip) = next.clips.get_mut(self.active_clip) else {
            return;
        };
        let Some(track) = clip.tracks.get_mut(key.track) else {
            return;
        };
        if key.key >= track.keyframes.len() {
            return;
        }
        track.keyframes.remove(key.key);
        if track.keyframes.is_empty() {
            clip.tracks.remove(key.track);
        }
        self.commit(next);
        self.status = tr("Schlüsselbild gelöscht · Strg+Z stellt es wieder her.").into();
    }
}

/// Round a time to the grid; a grid of 0 leaves it free.
fn snap_time(time: f64, grid: f64) -> f64 {
    if grid > 1e-9 {
        ((time / grid).round() * grid * 1e6).round() / 1e6
    } else {
        time
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_move_insert_and_delete_with_undo() {
        let mut editor = Editor::default();
        let track = editor.project.clips[0].tracks[0].clone();
        assert!(track.keyframes.len() >= 2);
        let before = editor.project.clone();
        // Retime the second key; it never passes its neighbours.
        editor.move_key(KeyRef { track: 0, key: 1 }, 1e9, false);
        let keys = &editor.project.clips[0].tracks[0].keyframes;
        assert!(keys[1].time < keys.get(2).map_or(f64::INFINITY, |k| k.time));
        editor.move_key(KeyRef { track: 0, key: 1 }, 0.333, false);
        assert!((editor.project.clips[0].tracks[0].keyframes[1].time - 0.35).abs() < 1e-9);
        // Insert keeps the animation: the new key carries the sampled value.
        let sampled = editor.project.clips[0].tracks[0].sample(0.2);
        editor.insert_key(0, 0.2);
        let keys = &editor.project.clips[0].tracks[0].keyframes;
        assert!(keys
            .iter()
            .any(|k| (k.time - 0.2).abs() < 1e-9 && k.value == sampled));
        let count = keys.len();
        editor.delete_key(KeyRef { track: 0, key: 0 });
        assert_eq!(editor.project.clips[0].tracks[0].keyframes.len(), count - 1);
        editor.project.validate().unwrap();
        while editor.history.undo(&mut editor.project) {}
        assert_eq!(editor.project, before);
    }

    #[test]
    fn easing_presets_apply_to_one_key_or_the_whole_track() {
        let mut editor = Editor::default();
        let ease = EASINGS[4].1;
        editor.set_easing(KeyRef { track: 0, key: 0 }, ease, false);
        let keys = &editor.project.clips[0].tracks[0].keyframes;
        assert_eq!(keys[0].easing, ease);
        editor.set_easing(KeyRef { track: 0, key: 0 }, Easing::Step, true);
        assert!(editor.project.clips[0].tracks[0]
            .keyframes
            .iter()
            .all(|k| k.easing == Easing::Step));
        editor.project.validate().unwrap();
    }

    #[test]
    fn keyframes_copy_to_the_playhead_and_replace_same_time_keys() {
        let mut editor = Editor::default();
        let before = editor.project.clips[0].tracks[0].keyframes.len();
        editor.copy_key(KeyRef { track: 0, key: 0 });
        editor.time = 0.75;
        editor.paste_key();
        let keys = &editor.project.clips[0].tracks[0].keyframes;
        assert_eq!(keys.len(), before + 1);
        let pasted = keys.iter().find(|k| (k.time - 0.75).abs() < 1e-9).unwrap();
        assert_eq!(
            pasted.value,
            editor.project.clips[0].tracks[0].keyframes[0].value
        );
        editor.paste_key();
        assert_eq!(
            editor.project.clips[0].tracks[0].keyframes.len(),
            before + 1
        );
        // Pasting the same key again changes nothing, so one undo restores it.
        assert!(editor.history.undo(&mut editor.project));
        assert_eq!(editor.project.clips[0].tracks[0].keyframes.len(), before);
    }

    #[test]
    fn ruler_steps_are_readable() {
        assert_eq!(nice_step(3., 700.), 0.5);
        assert_eq!(nice_step(20., 700.), 2.);
    }
}
