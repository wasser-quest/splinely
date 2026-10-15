use super::*;

impl Editor {
    pub(super) fn add_bone(&mut self, child: bool) {
        let parent = if child {
            self.selected_bone.clone()
        } else {
            None
        };
        if child
            && !self
                .project
                .bones
                .iter()
                .any(|b| Some(&b.id) == parent.as_ref())
        {
            self.status = tr("Zuerst einen Elternknochen wählen.").into();
            return;
        }
        let id = self.new_id("bone");
        let mut p = self.project.clone();
        p.bones.push(Bone {
            id: id.clone(),
            parent_id: parent,
            x: if child { 0. } else { p.width / 2. },
            y: if child { 0. } else { p.height / 2. },
            rotation: 0.,
            length: 100.,
        });
        self.commit(p);
        self.selected_bone = Some(id);
    }

    pub(super) fn bind_selected(&mut self, bind: bool) {
        let Some(index) = self
            .project
            .nodes
            .iter()
            .position(|n| Some(&n.id) == self.selected.as_ref())
        else {
            self.status = tr("Zuerst eine Form auswählen.").into();
            return;
        };
        let node = &self.project.nodes[index];
        if node.locked {
            self.status = tr("Form vor dem Binden entsperren.").into();
            return;
        }
        let changes_transform = |id: &str, property: Property| {
            id == node.id && matches!(property, Property::X | Property::Y | Property::Rotation)
        };
        let action_changes_transform = |a: &Action| matches!(a, Action::SetProperty { node_id, property, .. } if changes_transform(node_id, *property));
        if self.project.clips.iter().any(|c| {
            c.tracks
                .iter()
                .any(|t| changes_transform(&t.node_id, t.property))
        }) || self.project.state_machines.iter().any(|m| {
            m.states.iter().any(|s| {
                s.overrides
                    .iter()
                    .any(|o| changes_transform(&o.node_id, o.property))
                    || s.on_enter.iter().any(&action_changes_transform)
            }) || m
                .transitions
                .iter()
                .any(|t| t.actions.iter().any(&action_changes_transform))
        }) {
            self.status = tr("Binden ändert das Koordinatensystem. Zuerst X/Y/Drehungs-Spuren und Zustandsaktionen/-overrides der Form entfernen und stattdessen den Knochen animieren.").into();
            return;
        }
        let mut base = self.project.clone();
        base.clips.clear();
        base.state_machines.clear();
        let context = EvaluationContext {
            clips: Some(vec![]),
            ..Default::default()
        };
        let result = (|| -> Result<Project, DocumentError> {
            let nodes = base.evaluate_with_context(0., &context)?;
            let mut p = self.project.clone();
            let mut transform = nodes[index].transform;
            let bone_id = if bind {
                self.selected_bone.clone()
            } else {
                None
            };
            if bind {
                let poses = base.evaluate_bones(0., &context)?;
                let Some(pose) = poses.iter().find(|b| Some(&b.id) == bone_id.as_ref()) else {
                    return Ok(p);
                };
                let angle = pose.rotation.to_radians();
                let (x, y) = (transform.x - pose.x, transform.y - pose.y);
                transform.x = x * angle.cos() + y * angle.sin();
                transform.y = -x * angle.sin() + y * angle.cos();
                transform.rotation -= pose.rotation;
            }
            p.nodes[index].bone_id = bone_id;
            p.nodes[index].transform = transform;
            Ok(p)
        })();
        match result {
            Ok(p) => self.commit(p),
            Err(error) => self.status = error.to_string(),
        }
    }

    pub(super) fn key_bone(&mut self) {
        let Some(bone) = self
            .project
            .bones
            .iter()
            .find(|b| Some(&b.id) == self.selected_bone.as_ref())
            .cloned()
        else {
            return;
        };
        let mut p = self.project.clone();
        let Some(clip) = p.clips.get_mut(self.active_clip) else {
            self.status = tr("Zuerst in der Zeitleiste einen Clip anlegen.").into();
            return;
        };
        for (property, value) in [
            (Property::X, bone.x),
            (Property::Y, bone.y),
            (Property::Rotation, bone.rotation),
        ] {
            if !clip
                .tracks
                .iter()
                .any(|t| t.node_id == bone.id && t.property == property)
            {
                clip.tracks.push(Track {
                    node_id: bone.id.clone(),
                    property,
                    keyframes: vec![],
                });
            }
            let track = clip
                .tracks
                .iter_mut()
                .find(|t| t.node_id == bone.id && t.property == property)
                .unwrap();
            let key = Keyframe {
                time: self.time.clamp(0., clip.duration),
                value: AnimValue::Number(value),
                easing: Easing::Linear,
            };
            if let Some(old) = track
                .keyframes
                .iter_mut()
                .find(|k| (k.time - key.time).abs() < 1e-6)
            {
                *old = key;
            } else {
                track.keyframes.push(key);
            }
            track.keyframes.sort_by(|a, b| a.time.total_cmp(&b.time));
        }
        self.commit(p);
        self.status = tr("Knochen-X/Y/Drehung als Schlüsselbild gesetzt · Werte/Easing in der Zeitleiste bearbeiten.").into();
    }

    pub(super) fn bones_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr("Skelett"));
        ui.label(tr("Wurzel anlegen, auswählen, dann Kinder hinzufügen. Gelenke auf der Zeichenfläche anklicken. Kinder beginnen an der Spitze des Elternknochens."));
        ui.horizontal(|ui| {
            if ui.button(tr("+ Wurzel")).clicked() {
                self.add_bone(false);
            }
            if ui
                .add_enabled(
                    self.selected_bone.is_some(),
                    egui::Button::new(tr("+ Kind")),
                )
                .clicked()
            {
                self.add_bone(true);
            }
        });
        for bone in &self.project.bones {
            if ui
                .selectable_label(self.selected_bone.as_deref() == Some(&bone.id), &bone.id)
                .clicked()
            {
                self.selected_bone = Some(bone.id.clone());
            }
        }
        let mut p = self.project.clone();
        if let Some(index) = p
            .bones
            .iter()
            .position(|b| Some(&b.id) == self.selected_bone.as_ref())
        {
            let choices: Vec<_> = p.bones.iter().map(|b| b.id.clone()).collect();
            let bone = &mut p.bones[index];
            ui.separator();
            egui::ComboBox::from_id_salt("bone_parent")
                .selected_text(bone.parent_id.as_deref().unwrap_or(tr("Szenenwurzel")))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut bone.parent_id, None, tr("Szenenwurzel"));
                    for id in choices.iter().filter(|id| **id != bone.id) {
                        ui.selectable_value(&mut bone.parent_id, Some(id.clone()), id);
                    }
                });
            ui.label(tr(
                "Grundpose lokal (Zyklen und ungültige Längen werden abgelehnt)",
            ));
            for (label, value) in [
                ("X", &mut bone.x),
                ("Y", &mut bone.y),
                (tr("Drehung °"), &mut bone.rotation),
                (tr("Länge"), &mut bone.length),
            ] {
                ui.horizontal(|ui| {
                    ui.label(label);
                    ui.add(egui::DragValue::new(value).speed(0.5));
                });
            }
            self.commit(p);
            if ui
                .button(tr("◼ Pose am Abspielkopf merken (X/Y/Drehung)"))
                .clicked()
            {
                self.key_bone();
            }
            ui.label(tr("Animieren: Pose bei 0 merken, Abspielkopf verschieben, Pose ändern, erneut merken, dann abspielen. „Clip auswerten“ ausschalten, um die Grundpose zu sehen."));
            ui.collapsing(tr("IK-Pose (zwei Knochen)"), |ui| {
                ui.label(tr("Unteren Knochen wählen; sein direkter Elternknochen ist der obere, der Gelenkversatz muss 0 sein. Ziel in Szenenkoordinaten. Ändert die Grunddrehungen; zum Animieren beide Knochen merken."));
                ui.horizontal(|ui| {
                    ui.label(tr("Ziel X/Y"));
                    ui.add(egui::DragValue::new(&mut self.ik_target[0]).speed(1.));
                    ui.add(egui::DragValue::new(&mut self.ik_target[1]).speed(1.));
                });
                ui.checkbox(&mut self.ik_bend_positive, tr("Positiv beugen"));
                if ui.button(tr("Pose berechnen")).clicked() {
                    let lower = &self.project.bones[index];
                    let result = lower.parent_id.as_deref()
                        .ok_or_else(|| DocumentError(tr("Unteren Knochen mit Elternknochen wählen").into()))
                        .and_then(|upper| self.project.pose_two_bone_ik(upper, &lower.id, self.ik_target, self.ik_bend_positive));
                    match result {
                        Ok(next) => { self.commit(next); self.status = tr("IK-Pose berechnet (unerreichbare Ziele werden begrenzt) · beide Knochen merken, um zu animieren.").into(); }
                        Err(error) => self.status = error.to_string(),
                    }
                }
            });
            ui.label(trf(
                "Form: {}",
                &[self
                    .selected
                    .as_deref()
                    .unwrap_or("keine ausgewählt")
                    .to_string()],
            ));
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.selected.is_some(),
                        egui::Button::new(tr("Form binden")),
                    )
                    .clicked()
                {
                    self.bind_selected(true);
                }
                if ui
                    .add_enabled(self.selected.is_some(), egui::Button::new(tr("Lösen")))
                    .clicked()
                {
                    self.bind_selected(false);
                }
            });
            ui.label(tr("Binden erhält die sichtbare Lage. Die Transformation der Form wird relativ zur Knochenwurzel, die Geometrie bleibt unverändert. Transformationsspuren/Zustandsaktionen der Form vorher entfernen."));
        }
    }

    pub(super) fn bone_guides(
        &mut self,
        painter: &egui::Painter,
        origin: Pos2,
        pick: Option<Vec2>,
    ) -> bool {
        let poses = match self
            .project
            .evaluate_bones(self.time, &self.preview_context())
        {
            Ok(poses) => poses,
            Err(error) => {
                self.status = error.to_string();
                return false;
            }
        };
        let mut picked = false;
        if let Some(point) = pick {
            if let Some(pose) = poses
                .iter()
                .rev()
                .find(|b| (Vec2::new(b.x as f32, b.y as f32) - point).length() * self.zoom <= 10.)
            {
                self.selected_bone = Some(pose.id.clone());
                picked = true;
            }
        }
        for pose in poses {
            let angle = pose.rotation.to_radians();
            let root = origin + Vec2::new(pose.x as f32, pose.y as f32) * self.zoom;
            let tip = root
                + Vec2::new(
                    (pose.length * angle.cos()) as f32,
                    (pose.length * angle.sin()) as f32,
                ) * self.zoom;
            let color = if self.selected_bone.as_deref() == Some(&pose.id) {
                Color32::from_rgb(255, 125, 0)
            } else {
                Color32::from_rgb(20, 150, 170)
            };
            painter.line_segment([root, tip], egui::Stroke::new(3_f32, color));
            painter.circle_filled(root, 6., color);
            painter.circle_stroke(tip, 4., egui::Stroke::new(2_f32, color));
            painter.text(
                root + Vec2::new(8., -8.),
                egui::Align2::LEFT_BOTTOM,
                pose.id,
                egui::FontId::proportional(12.),
                color,
            );
        }
        picked
    }
}
