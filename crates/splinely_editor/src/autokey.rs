//! Auto-key ("Aufnahme"): while on, editing a shape's position, rotation,
//! scale, opacity, colours or size writes keyframes at the playhead instead of
//! changing its base pose. A property's first key also records the old value
//! at 0 s, so the change becomes a movement. Dragging animated shapes shows
//! them under the pointer (live overrides) until the edit is committed.
use super::*;

/// Properties recorded by auto-key (path data is excluded: point edits can
/// change the path structure, which keyframes cannot interpolate).
const RECORDED: [Property; 12] = [
    Property::X,
    Property::Y,
    Property::Rotation,
    Property::ScaleX,
    Property::ScaleY,
    Property::Opacity,
    Property::Fill,
    Property::Stroke,
    Property::Width,
    Property::Height,
    Property::Radius,
    Property::FontSize,
];

impl Editor {
    /// Turn base-pose edits in `next` into keyframes of the active clip.
    pub(super) fn autokey(&mut self, next: &mut Project) {
        if !self.auto_key {
            return;
        }
        let mut changes = vec![];
        for node in &next.nodes {
            let Some(old) = self.project.nodes.iter().find(|n| n.id == node.id) else {
                continue;
            };
            for p in RECORDED
                .iter()
                .copied()
                .chain([Property::Gradient, Property::MotionProgress])
            {
                if let (Some(a), Some(b)) = (old.property_value(p), node.property_value(p)) {
                    if a != b {
                        changes.push((node.id.clone(), p, a, b));
                    }
                }
            }
        }
        if changes.is_empty() {
            return;
        }
        // Record into the active clip, creating one if the project has none.
        if next.clips.get(self.active_clip).is_none() {
            let id = next.unused_id("animation");
            next.clips.push(Clip {
                audio_cues: vec![],
                id,
                name: tr("Animation").into(),
                duration: self.time.max(2.),
                looping: true,
                autoplay: true,
                blend_mode: BlendMode::Override,
                weight: 1.,
                tracks: vec![],
            });
            self.active_clip = next.clips.len() - 1;
        }
        let time = self.time;
        for (id, p, old, new) in changes {
            if let Some(node) = next.nodes.iter_mut().find(|n| n.id == id) {
                node.set_property_value(p, old.clone());
            }
            let clip = &mut next.clips[self.active_clip];
            clip.duration = clip.duration.max(time);
            let key = |time, value| Keyframe {
                time,
                value,
                easing: Easing::Linear,
            };
            match clip
                .tracks
                .iter_mut()
                .find(|t| t.node_id == id && t.property == p)
            {
                Some(track) => match track
                    .keyframes
                    .iter_mut()
                    .find(|k| (k.time - time).abs() < 1e-6)
                {
                    Some(existing) => existing.value = new,
                    None => {
                        let index = track.keyframes.partition_point(|k| k.time < time);
                        track.keyframes.insert(index, key(time, new));
                    }
                },
                None => {
                    let mut keyframes = vec![];
                    if time > 1e-6 {
                        keyframes.push(key(0., old));
                    }
                    keyframes.push(key(time, new));
                    clip.tracks.push(Track {
                        node_id: id,
                        property: p,
                        keyframes,
                    });
                }
            }
        }
        self.preview_animation = true;
        self.status = trf(
            "Aufnahme: Schlüsselbild bei {:.2} s gesetzt.",
            &[format!("{:.2}", time)],
        );
    }

    /// While dragging/transforming, show the edited base pose of the selection
    /// instead of the clip's values, so animated shapes follow the pointer.
    pub(super) fn live_overrides(&self) -> Vec<PropertyOverride> {
        if self.drag_start.is_none() && self.transform_gesture.is_none() {
            return vec![];
        }
        let ids = self.editable_selection();
        let mut overrides = vec![];
        for node in self.project.nodes.iter().filter(|n| ids.contains(&n.id)) {
            for p in [
                Property::X,
                Property::Y,
                Property::Rotation,
                Property::ScaleX,
                Property::ScaleY,
            ] {
                if let Some(value) = node.property_value(p) {
                    overrides.push(PropertyOverride {
                        node_id: node.id.clone(),
                        property: p,
                        value,
                    });
                }
            }
        }
        overrides
    }

    pub(super) fn autokey_toggle_ui(&mut self, ui: &mut egui::Ui) {
        let label = if self.auto_key {
            tr("🔴 Aufnahme an")
        } else {
            tr("🔴 Aufnahme")
        };
        let text = egui::RichText::new(label).color(if self.auto_key {
            Color32::from_rgb(240, 80, 80)
        } else {
            ui.visuals().weak_text_color()
        });
        if ui
            .add(egui::Button::new(text).selected(self.auto_key))
            .on_hover_text(tr("Auto-Key: Änderungen an Formen werden zu Schlüsselbildern am Abspielkopf statt die Grundpose zu ändern."))
            .clicked()
        {
            self.auto_key = !self.auto_key;
            if self.auto_key {
                self.preview_animation = true;
                self.timeline_visible = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Editor {
        let mut editor = Editor::default();
        editor.project.nodes = vec![Node::rect("a", 0., 0., 10., 10.)];
        editor.project.clips.clear();
        editor.project.state_machines.clear();
        editor.sync_buffers();
        editor
    }

    #[test]
    fn edits_become_keyframes_and_the_base_pose_stays() {
        let mut editor = editor();
        editor.auto_key = true;
        editor.time = 1.;
        let mut next = editor.project.clone();
        next.nodes[0].transform.x = 100.;
        editor.commit(next);
        // Base unchanged, a new clip holds 0 s → old and 1 s → new.
        assert_eq!(editor.project.nodes[0].transform.x, 0.);
        let track = &editor.project.clips[0].tracks[0];
        assert_eq!(track.property, Property::X);
        assert_eq!(track.keyframes.len(), 2);
        assert_eq!(track.keyframes[1].value, AnimValue::Number(100.));
        // A second edit at another time adds a key to the same track.
        editor.time = 2.;
        let mut next = editor.project.clone();
        next.nodes[0].style.fill = Color::rgb(255, 0, 0);
        next.nodes[0].transform.x = 50.;
        editor.commit(next);
        let x = editor.project.clips[0]
            .tracks
            .iter()
            .find(|t| t.property == Property::X)
            .unwrap();
        assert_eq!(x.keyframes.len(), 3);
        assert!(editor.project.clips[0]
            .tracks
            .iter()
            .any(|t| t.property == Property::Fill));
        assert!(editor.project.clips[0].duration >= 2.);
        editor.project.validate().unwrap();
        // The preview at 1 s shows the recorded pose.
        editor.preview_animation = true;
        editor.time = 1.;
        assert_eq!(editor.evaluated()[0].transform.x, 100.);
    }

    #[test]
    fn motion_progress_autokey_preserves_binding_and_base() {
        let mut e = editor();
        let target = e.project.nodes[0].id.clone();
        e.project.nodes[0].motion_path = Some(MotionPath {
            path: "route".into(),
            progress: 0.,
            orient: true,
        });
        e.project.nodes.push(Node::new(
            "route",
            NodeKind::Path {
                d: "M0 0 L100 0".into(),
                bounds: [0., 0., 100., 0.],
                evenodd: false,
            },
        ));
        e.time = 1.;
        e.auto_key = true;
        let mut next = e.project.clone();
        next.nodes[0].motion_path.as_mut().unwrap().progress = 1.;
        e.commit(next);
        assert_eq!(
            e.project.nodes[0].motion_path.as_ref().unwrap().progress,
            0.
        );
        let track = e.project.clips[0]
            .tracks
            .iter()
            .find(|t| t.property == Property::MotionProgress)
            .unwrap();
        assert_eq!(track.node_id, target);
        assert_eq!(track.keyframes.len(), 2);
        assert!((e.evaluated()[0].transform.x - 100.).abs() < 1e-8);
        assert!(e.history.undo(&mut e.project));
        assert!(e.project.clips.is_empty());
    }

    #[test]
    fn without_auto_key_edits_change_the_base_pose() {
        let mut editor = editor();
        let mut next = editor.project.clone();
        next.nodes[0].transform.x = 100.;
        editor.commit(next);
        assert_eq!(editor.project.nodes[0].transform.x, 100.);
        assert!(editor.project.clips.is_empty());
    }

    #[test]
    fn dragging_overrides_the_clip_for_the_selection() {
        let mut editor = editor();
        editor.set_selection(vec!["a".into()]);
        assert!(editor.live_overrides().is_empty());
        editor.drag_start = Some(editor.project.clone());
        let overrides = editor.live_overrides();
        assert!(overrides.iter().any(|o| o.property == Property::X));
    }
}
