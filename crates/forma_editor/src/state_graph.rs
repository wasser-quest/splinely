//! Standalone, staged state-machine editor. Wire `StateGraph::show` into states_ui
//! and commit its returned Project through the application's existing history path.
//! No runtime simulation: arrows describe declared transitions, not reachability.
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use forma_document::{Event, Project, State, StateMachine, Transition};

const MAX_DRAWN_STATES: usize = 64;
const MAX_DRAWN_ARROWS: usize = 128;

#[derive(Default)]
pub struct StateGraph {
    source: Option<(usize, StateMachine)>,
    draft: Option<StateMachine>,
    selected_state: String,
    selected_transition: Option<usize>,
    new_state: String,
    error: String,
}

impl StateGraph {
    /// Returns a validated replacement only on Apply. Input is never mutated.
    /// Keep one instance in the caller. Switching machines or externally changing
    /// the machine (including undo/JSON apply) resets this editor's staged edits.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        machine_index: usize,
    ) -> Option<Project> {
        let Some(machine) = project.state_machines.get(machine_index) else {
            *self = Self::default();
            ui.label("Zustandsmaschine zum Bearbeiten wählen.");
            return None;
        };
        if !self
            .source
            .as_ref()
            .is_some_and(|(index, source)| *index == machine_index && source == machine)
        {
            self.reset(machine_index, machine);
        }
        let draft = self.draft.as_mut().unwrap();
        ui.push_id(("state_graph", machine_index), |ui| {
            ui.label("Zustand oder Übergang wählen. Änderungen werden erst mit „Übernehmen“ wirksam.");
            ui.label("Startzustand ist mit ★ markiert. Bedingungen, Aktionen und Overrides: über das Maschinen-JSON.");
            ui.label("Zeigt nur deklarierte Übergänge, keine Simulation.");
            draw_graph(
                ui,
                draft,
                &mut self.selected_state,
                &mut self.selected_transition,
            );
            ui.horizontal(|ui| {
                ui.label("ID des neuen Zustands:");
                ui.text_edit_singleline(&mut self.new_state);
                if ui.button("+ Zustand").clicked() {
                    draft.states.push(State {
                        id: self.new_state.clone(),
                        on_enter: vec![],
                        overrides: vec![],
                    });
                    self.selected_state = self.new_state.clone();
                    self.new_state.clear();
                }
            });
            ui.horizontal(|ui| {
                state_picker(
                    ui,
                    "source",
                    "Von",
                    &mut self.selected_state,
                    &draft.states,
                );
                if ui
                    .add_enabled(!draft.states.is_empty(), egui::Button::new("+ Übergang"))
                    .clicked()
                {
                    draft.transitions.push(Transition {
                        from: self.selected_state.clone(),
                        to: self.selected_state.clone(),
                        event: Event {
                            name: "click".into(),
                            target: None,
                        },
                        guard: None,
                        actions: vec![],
                    });
                    self.selected_transition = Some(draft.transitions.len() - 1);
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("transition_list")
                .max_height(180.0)
                .show_rows(ui, 22.0, draft.transitions.len(), |ui, rows| {
                    for i in rows {
                        let t = &draft.transitions[i];
                        let label = format!(
                            "#{} {} › {} · {} [{}] · {} Aktionen{}",
                            i + 1,
                            t.from,
                            t.to,
                            t.event.name,
                            t.event.target.as_deref().unwrap_or("beliebig"),
                            t.actions.len(),
                            if t.guard.is_some() { " · mit Bedingung" } else { "" }
                        );
                        if ui
                            .selectable_label(self.selected_transition == Some(i), label)
                            .clicked()
                        {
                            self.selected_transition = Some(i);
                        }
                    }
                });
            if let Some(i) = self
                .selected_transition
                .filter(|&i| i < draft.transitions.len())
            {
                let t = &mut draft.transitions[i];
                ui.horizontal(|ui| {
                    state_picker(ui, "from", "Von", &mut t.from, &draft.states);
                    state_picker(ui, "to", "Nach", &mut t.to, &draft.states);
                });
                ui.horizontal(|ui| {
                    ui.label("Ereignisname:");
                    ui.text_edit_singleline(&mut t.event.name);
                    egui::ComboBox::from_id_salt("event_target")
                        .selected_text(t.event.target.as_deref().unwrap_or("Beliebiges Ziel"))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut t.event.target, None, "Beliebiges Ziel");
                            for node in &project.nodes {
                                ui.selectable_value(
                                    &mut t.event.target,
                                    Some(node.id.clone()),
                                    &node.id,
                                );
                            }
                        });
                });
                ui.label(format!(
                    "Bedingung: {} · {} Aktionen (bleiben erhalten; per JSON bearbeiten)",
                    if t.guard.is_some() { "vorhanden" } else { "none" },
                    t.actions.len()
                ));
                if ui.button("Ausgewählten Übergang entfernen").clicked() {
                    draft.transitions.remove(i);
                    self.selected_transition = None;
                }
            }
        });
        let dirty = draft != machine;
        let mut result = None;
        let mut discard = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(dirty, egui::Button::new("Graph-Änderungen übernehmen"))
                .clicked()
            {
                match replacement(project, machine_index, draft.clone()) {
                    Ok(next) => {
                        self.error.clear();
                        result = Some(next);
                    }
                    Err(message) => self.error = message,
                }
            }
            discard = ui
                .add_enabled(dirty, egui::Button::new("Vorgemerkte Änderungen verwerfen"))
                .clicked();
        });
        if discard {
            self.reset(machine_index, machine);
        }
        if !self.error.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, &self.error);
        }
        result
    }

    fn reset(&mut self, index: usize, machine: &StateMachine) {
        self.source = Some((index, machine.clone()));
        self.draft = Some(machine.clone());
        self.selected_state = machine.initial.clone();
        self.selected_transition = None;
        self.new_state.clear();
        self.error.clear();
    }
}

fn replacement(project: &Project, index: usize, machine: StateMachine) -> Result<Project, String> {
    let mut next = project.clone();
    let slot = next
        .state_machines
        .get_mut(index)
        .ok_or("Zustandsmaschine fehlt")?;
    *slot = machine;
    next.validate().map_err(|e| e.to_string())?;
    Ok(next)
}

fn state_picker(ui: &mut egui::Ui, id: &str, label: &str, value: &mut String, states: &[State]) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(format!("{label}: {value}"))
        .show_ui(ui, |ui| {
            for state in states {
                ui.selectable_value(value, state.id.clone(), &state.id);
            }
        });
}

fn draw_graph(
    ui: &mut egui::Ui,
    machine: &StateMachine,
    selected: &mut String,
    edge: &mut Option<usize>,
) {
    let count = machine.states.len().min(MAX_DRAWN_STATES);
    if machine.states.len() > MAX_DRAWN_STATES || machine.transitions.len() > MAX_DRAWN_ARROWS {
        ui.label("Graphvorschau zeigt höchstens 64 Zustände / 128 Pfeile. Alle Zustände und Übergänge bleiben unten bearbeitbar.");
    }
    egui::ScrollArea::both()
        .id_salt("graph_canvas")
        .max_height(300.0)
        .show(ui, |ui| {
            let rows = count.div_ceil(4).max(1);
            let (canvas, _) = ui
                .allocate_exact_size(Vec2::new(800.0, rows as f32 * 100.0 + 40.0), Sense::hover());
            let boxes: Vec<Rect> = (0..count)
                .map(|i| {
                    Rect::from_min_size(
                        canvas.min
                            + Vec2::new(
                                20.0 + (i % 4) as f32 * 195.0,
                                30.0 + (i / 4) as f32 * 100.0,
                            ),
                        Vec2::new(145.0, 42.0),
                    )
                })
                .collect();
            let painter = ui.painter_at(canvas);
            for (i, t) in machine
                .transitions
                .iter()
                .take(MAX_DRAWN_ARROWS)
                .enumerate()
            {
                let from = machine
                    .states
                    .iter()
                    .take(count)
                    .position(|s| s.id == t.from);
                let to = machine.states.iter().take(count).position(|s| s.id == t.to);
                let (Some(from), Some(to)) = (from, to) else {
                    continue;
                };
                let stroke = Stroke::new(
                    1.5_f32,
                    if *edge == Some(i) {
                        Color32::YELLOW
                    } else {
                        Color32::LIGHT_BLUE
                    },
                );
                let a = boxes[from].center_top();
                let b = boxes[to].center_top();
                // Routed above nodes; numbered labels link edges to the full list.
                // Parallel edges may overlap: this is a bounded preview, not a layout solver.
                let lift = 14.0 + (i % 3) as f32 * 5.0;
                let label_pos = if from == to {
                    let p = a + Vec2::new(0.0, -lift);
                    let q = p + Vec2::new(35.0, 0.0);
                    painter.line_segment([a, p], stroke);
                    painter.line_segment([p, q], stroke);
                    painter.arrow(q, Vec2::new(0.0, lift), stroke);
                    q
                } else {
                    let p = Pos2::new(a.x, a.y.min(b.y) - lift);
                    let q = Pos2::new(b.x, p.y);
                    painter.line_segment([a, p], stroke);
                    painter.line_segment([p, q], stroke);
                    painter.arrow(q, b - q, stroke);
                    p.lerp(q, 0.5)
                };
                painter.text(
                    label_pos,
                    egui::Align2::CENTER_BOTTOM,
                    format!("#{}", i + 1),
                    egui::FontId::proportional(11.0),
                    stroke.color,
                );
                if ui
                    .interact(
                        Rect::from_center_size(label_pos, Vec2::new(28.0, 18.0)),
                        ui.id().with(("edge", i)),
                        Sense::click(),
                    )
                    .clicked()
                {
                    *edge = Some(i);
                }
            }
            for (i, rect) in boxes.iter().enumerate() {
                let state = &machine.states[i];
                painter.rect_filled(
                    *rect,
                    5.0,
                    if *selected == state.id {
                        Color32::from_rgb(30, 110, 160)
                    } else {
                        Color32::DARK_GRAY
                    },
                );
                let text = format!(
                    "{}{}",
                    if state.id == machine.initial {
                        "★ "
                    } else {
                        ""
                    },
                    state.id
                );
                painter.with_clip_rect(*rect).text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::proportional(14.0),
                    Color32::WHITE,
                );
                if ui
                    .interact(*rect, ui.id().with(("state", i)), Sense::click())
                    .on_hover_text(&state.id)
                    .clicked()
                {
                    *selected = state.id.clone();
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Project {
        let mut project = Project::default();
        project.state_machines.push(StateMachine {
            id: "machine".into(),
            initial: "idle".into(),
            states: vec![State {
                id: "idle".into(),
                on_enter: vec![],
                overrides: vec![],
            }],
            transitions: vec![],
        });
        project
    }

    #[test]
    fn canonical_validation_and_atomic_rejection() {
        let project = fixture();
        let mut machine = project.state_machines[0].clone();
        for id in ["idle", "bad id", "9bad"] {
            machine.states.push(State {
                id: id.into(),
                on_enter: vec![],
                overrides: vec![],
            });
            assert!(replacement(&project, 0, machine.clone()).is_err());
            machine.states.pop();
        }
        machine.states.push(State {
            id: "active".into(),
            on_enter: vec![],
            overrides: vec![],
        });
        assert!(replacement(&project, 0, machine.clone()).is_ok());
        machine.transitions.push(Transition {
            from: "idle".into(),
            to: "active".into(),
            event: Event {
                name: "".into(),
                target: None,
            },
            guard: None,
            actions: vec![],
        });
        assert!(replacement(&project, 0, machine.clone()).is_err());
        machine.transitions[0].event.name = "click".into();
        machine.transitions[0].event.target = Some("missing-node".into());
        assert!(replacement(&project, 0, machine.clone()).is_err());
        machine.transitions[0].event.target = None;
        machine.transitions[0].to = "missing-state".into();
        assert!(replacement(&project, 0, machine.clone()).is_err());
        assert!(replacement(&project, 99, machine).is_err());
        assert_eq!(project, fixture());
    }

    #[test]
    fn edits_preserve_advanced_fields_and_order() {
        use forma_document::{Action, Guard};
        let mut project = fixture();
        project
            .variables
            .insert("ready".into(), serde_json::json!(true));
        let action = Action::SetVariable {
            name: "ready".into(),
            value: serde_json::json!(false),
        };
        project.state_machines[0].states[0]
            .on_enter
            .push(action.clone());
        let t = Transition {
            from: "idle".into(),
            to: "idle".into(),
            event: Event {
                name: "click".into(),
                target: None,
            },
            guard: Some(Guard {
                variable: "ready".into(),
                equals: serde_json::json!(true),
            }),
            actions: vec![action],
        };
        project.state_machines[0].transitions = vec![t.clone(), t.clone()];
        let mut draft = project.state_machines[0].clone();
        draft.transitions[0].event.name = "hover".into();
        let next = replacement(&project, 0, draft.clone()).unwrap();
        assert_eq!(
            next.state_machines[0].states,
            project.state_machines[0].states
        );
        assert_eq!(next.state_machines[0].transitions[0].guard, t.guard);
        assert_eq!(next.state_machines[0].transitions[0].actions, t.actions);
        assert_eq!(next.state_machines[0].transitions[1], t);
        draft.transitions.remove(0);
        assert_eq!(
            replacement(&next, 0, draft).unwrap().state_machines[0].transitions,
            vec![t]
        );
    }

    #[test]
    fn bounded_graph_handles_loops_parallel_edges_and_large_machines() {
        let mut project = fixture();
        let machine = &mut project.state_machines[0];
        for i in 0..80 {
            machine.states.push(State {
                id: format!("state-{i}"),
                on_enter: vec![],
                overrides: vec![],
            });
        }
        for i in 0..150 {
            machine.transitions.push(Transition {
                from: "idle".into(),
                to: if i % 2 == 0 {
                    "idle".into()
                } else {
                    "state-79".into()
                },
                event: Event {
                    name: "click".into(),
                    target: None,
                },
                guard: None,
                actions: vec![],
            });
        }
        project.validate().unwrap();
        let original = project.clone();
        let mut graph = StateGraph::default();
        let context = egui::Context::default();
        let _ = context.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(graph.show(ui, &project, 0).is_none());
            });
        });
        assert_eq!(graph.draft.as_ref().unwrap().states.len(), 81);
        assert_eq!(graph.draft.as_ref().unwrap().transitions.len(), 150);
        assert_eq!(project, original);
    }

    #[test]
    fn headless_ui_and_external_refresh() {
        let project = fixture();
        let mut graph = StateGraph::default();
        let context = egui::Context::default();
        let _ = context.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(graph.show(ui, &project, 0).is_none());
            });
        });
        graph.draft.as_mut().unwrap().states.clear();
        let mut next = project.clone();
        next.state_machines[0].id = "other".into();
        let _ = context.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(graph.show(ui, &next, 0).is_none());
                assert_eq!(graph.draft.as_ref(), Some(&next.state_machines[0]));
                assert!(graph.show(ui, &next, 99).is_none());
                assert!(graph.draft.is_none());
            });
        });
    }
}
