//! Standalone, staged state-machine editor. Wire `StateGraph::show` into states_ui
//! and commit its returned Project through the application's existing history path.
//! No runtime simulation: arrows describe declared transitions, not reachability.
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use splinely_document::{Event, Project, State, StateMachine, Transition, TIMER_EVENT};

#[path = "state_graph_controls.rs"]
mod controls;
use splinely_editor::i18n::{tr, trf};

const MAX_DRAWN_STATES: usize = 64;
const MAX_DRAWN_ARROWS: usize = 128;
const MAX_TIMER_SECONDS: f64 = 3600.0;
const MAX_TRANSITION_BLEND_SECONDS: f64 = 60.0;

#[derive(Default)]
pub struct StateGraph {
    source: Option<(usize, StateMachine)>,
    draft: Option<StateMachine>,
    selected_state: String,
    selected_transition: Option<usize>,
    new_state: String,
    new_state_error: String,
    error: String,
    positions: Vec<Vec2>,
    connecting: Option<usize>,
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
            ui.label(tr("Zustandsmaschine zum Bearbeiten wählen."));
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
            ui.label(tr("Zustand oder Übergang wählen. Änderungen werden erst mit „Übernehmen“ wirksam."));
            ui.label(tr("Startzustand: ★. Vom rechten Anschluss auf einen Zustand ziehen, um ihn zu verbinden."));
            ui.horizontal(|ui| {
                if ui.button(tr("Automatisch anordnen")).clicked() {
                    self.positions = auto_layout(draft);
                }
                if self.connecting.is_some() && ui.button(tr("Verbindung abbrechen")).clicked() {
                    self.connecting = None;
                }
            });
            ui.label(tr("Zeigt nur deklarierte Übergänge, keine Simulation."));
            draw_graph(
                ui,
                draft,
                &mut self.selected_state,
                &mut self.selected_transition,
                &mut self.positions,
                &mut self.connecting,
            );
            if let Some(state) = draft.states.iter_mut().find(|s| s.id == self.selected_state) {
                ui.collapsing(tr("Zustandsaktionen"), |ui| {
                    if ui.button(tr("Als Startzustand setzen")).clicked() {
                        draft.initial = state.id.clone();
                    }
                    ui.label(tr("Beim Betreten"));
                    ui.push_id(("enter", &state.id), |ui| controls::actions_ui(ui, &mut state.on_enter, project));
                    ui.label(tr("Beim Verlassen"));
                    ui.push_id(("exit", &state.id), |ui| controls::actions_ui(ui, &mut state.on_exit, project));
                });
            }
            ui.horizontal(|ui| {
                ui.label(tr("ID des neuen Zustands:"));
                ui.text_edit_singleline(&mut self.new_state);
                if ui.button(tr("+ Zustand")).clicked() {
                    match insert_state(draft, &self.new_state) {
                        Ok(()) => {
                            self.selected_state = self.new_state.clone();
                            self.new_state.clear();
                            self.new_state_error.clear();
                        }
                        Err(message) => self.new_state_error = message,
                    }
                }
            });
            if !self.new_state_error.is_empty() {
                ui.colored_label(Color32::LIGHT_RED, &self.new_state_error);
            }
            ui.horizontal(|ui| {
                state_picker(
                    ui,
                    "source",
                    tr("Von"),
                    &mut self.selected_state,
                    &draft.states,
                );
                if ui
                    .add_enabled(!draft.states.is_empty(), egui::Button::new(tr("+ Übergang")))
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
                        blend: 0.0,
                        after: None,
                    });
                    self.selected_transition = Some(draft.transitions.len() - 1);
                }
            });
            ui.label(tr("Übergänge werden von oben nach unten geprüft. Der erste passende Übergang gewinnt."));
            egui::ScrollArea::vertical()
                .id_salt("transition_list")
                .max_height(180.0)
                .show_rows(ui, 22.0, draft.transitions.len(), |ui, rows| {
                    for i in rows {
                        let t = &draft.transitions[i];
                        let label = trf("#{} {} › {} · {} [{}] · {} Aktionen{}", &[format!("{}", i + 1), t.from.to_string(), t.to.to_string(), match t.after { Some(after) => trf("nach {} s", &[format!("{after}")]), None => t.event.name.to_string() }, t.event.target.as_deref().unwrap_or(tr("beliebig")).to_string(), format!("{}", t.actions.len()), (if t.guard.is_some() { tr(" · mit Bedingung") } else { "" }).to_string()]);
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
                ui.horizontal(|ui| {
                    if ui.add_enabled(i > 0, egui::Button::new(tr("Übergang nach oben"))).clicked() {
                        move_transition(draft, &mut self.selected_transition, i, i - 1);
                    }
                    if ui.add_enabled(i + 1 < draft.transitions.len(), egui::Button::new(tr("Übergang nach unten"))).clicked() {
                        move_transition(draft, &mut self.selected_transition, i, i + 1);
                    }
                });
                let i = self.selected_transition.unwrap();
                let t = &mut draft.transitions[i];
                ui.horizontal(|ui| {
                    state_picker(ui, "from", tr("Von"), &mut t.from, &draft.states);
                    state_picker(ui, "to", tr("Nach"), &mut t.to, &draft.states);
                });
                let mut timer = t.after.is_some();
                ui.horizontal(|ui| {
                    if ui
                        .checkbox(&mut timer, tr("Nach Zeit statt Ereignis"))
                        .on_hover_text(tr("Der Übergang passiert automatisch, wenn der Zustand so lange aktiv war – z. B. zum Blinzeln oder zum Zurückkehren in Ruhe."))
                        .changed()
                    {
                        t.after = timer.then_some(1.0);
                        t.event = Event {
                            name: if timer { TIMER_EVENT } else { "click" }.into(),
                            target: None,
                        };
                    }
                    if let Some(after) = t.after.as_mut() {
                        ui.add(
                            egui::DragValue::new(after)
                                .range(0.0..=MAX_TIMER_SECONDS)
                                .speed(0.05)
                                .suffix(" s"),
                        );
                    }
                });
                if !timer {
                ui.horizontal(|ui| {
                    ui.label(tr("Ereignisname:"));
                    ui.text_edit_singleline(&mut t.event.name);
                    egui::ComboBox::from_id_salt("event_target")
                        .selected_text(t.event.target.as_deref().unwrap_or(tr("Beliebiges Ziel")))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut t.event.target, None, tr("Beliebiges Ziel"));
                            for node in &project.nodes {
                                ui.selectable_value(
                                    &mut t.event.target,
                                    Some(node.id.clone()),
                                    &node.id,
                                );
                            }
                        });
                });
                }
                ui.horizontal(|ui| {
                    ui.label(tr("Überblenden (s):"));
                    ui.add(
                        egui::DragValue::new(&mut t.blend)
                            .range(0.0..=MAX_TRANSITION_BLEND_SECONDS)
                            .speed(0.05)
                            .max_decimals(2),
                    )
                    .on_hover_text(tr("Der alte Zustand geht in dieser Zeit weich in den neuen über (0 = sofort). Mehrere Zustandsmaschinen überblenden unabhängig voneinander."));
                });
                ui.push_id(("transition_guard", i), |ui| controls::guard_ui(ui, &mut t.guard, project));
                ui.label(tr("Übergangsaktionen"));
                ui.push_id(("transition_actions", i), |ui| controls::actions_ui(ui, &mut t.actions, project));
                if ui.button(tr("Ausgewählten Übergang entfernen")).clicked() {
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
                .add_enabled(dirty, egui::Button::new(tr("Graph-Änderungen übernehmen")))
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
                .add_enabled(
                    dirty,
                    egui::Button::new(tr("Vorgemerkte Änderungen verwerfen")),
                )
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
        let keep_positions = self
            .source
            .as_ref()
            .is_some_and(|(old_index, _)| *old_index == index)
            && self.draft.as_ref().is_some_and(|draft| {
                draft
                    .states
                    .iter()
                    .map(|s| &s.id)
                    .eq(machine.states.iter().map(|s| &s.id))
            })
            && self.positions.len() == machine.states.len();
        self.source = Some((index, machine.clone()));
        self.draft = Some(machine.clone());
        self.selected_state = machine.initial.clone();
        self.selected_transition = None;
        self.new_state.clear();
        self.new_state_error.clear();
        self.error.clear();
        if !keep_positions {
            self.positions = auto_layout(machine);
        }
        self.connecting = None;
    }
}

fn replacement(project: &Project, index: usize, machine: StateMachine) -> Result<Project, String> {
    let mut next = project.clone();
    let slot = next
        .state_machines
        .get_mut(index)
        .ok_or(tr("Zustandsmaschine fehlt"))?;
    *slot = machine;
    // Followers that only act in removed states keep the states that remain.
    let states: Vec<String> = slot.states.iter().map(|s| s.id.clone()).collect();
    let machine_id = slot.id.clone();
    for follower in &mut next.followers {
        if let Some(when) = follower.when.as_mut().filter(|w| w.machine == machine_id) {
            when.states.retain(|s| states.contains(s));
            if when.states.is_empty() {
                follower.when = None;
            }
        }
    }
    next.validate().map_err(|e| e.to_string())?;
    Ok(next)
}

fn insert_state(machine: &mut StateMachine, id: &str) -> Result<(), String> {
    let mut chars = id.chars();
    if !matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(tr("Ungültige Zustands-ID: Mit Buchstabe oder _ beginnen; nur ASCII-Buchstaben, Ziffern, _, - und . verwenden.").into());
    }
    if machine.states.iter().any(|state| state.id == id) {
        return Err(tr("Diese Zustands-ID ist bereits vorhanden.").into());
    }
    machine.states.push(State {
        id: id.into(),
        on_enter: vec![],
        overrides: vec![],
        on_exit: vec![],
        announce: None,
    });
    Ok(())
}

fn move_transition(
    machine: &mut StateMachine,
    selected: &mut Option<usize>,
    from: usize,
    to: usize,
) {
    if from >= machine.transitions.len() || to >= machine.transitions.len() {
        return;
    }
    machine.transitions.swap(from, to);
    *selected = selected.map(|index| {
        if index == from {
            to
        } else if index == to {
            from
        } else {
            index
        }
    });
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

/// Deterministic breadth-first layers, including cycles and disconnected components.
/// Positions are editor-only: arranging nodes never dirties the document.
fn auto_layout(machine: &StateMachine) -> Vec<Vec2> {
    use std::collections::{HashMap, VecDeque};
    let indices: HashMap<&str, usize> = machine
        .states
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.as_str(), i))
        .collect();
    let mut adjacent = vec![vec![]; machine.states.len()];
    for t in &machine.transitions {
        if let (Some(&a), Some(&b)) = (indices.get(t.from.as_str()), indices.get(t.to.as_str())) {
            adjacent[a].push(b);
        }
    }
    let mut layers = vec![None; machine.states.len()];
    let mut roots: Vec<usize> = indices
        .get(machine.initial.as_str())
        .copied()
        .into_iter()
        .collect();
    roots.extend(0..machine.states.len());
    for root in roots {
        if layers[root].is_some() {
            continue;
        }
        layers[root] = Some(0);
        let mut queue = VecDeque::from([root]);
        while let Some(a) = queue.pop_front() {
            for &b in &adjacent[a] {
                if layers[b].is_none() {
                    layers[b] = Some(layers[a].unwrap() + 1);
                    queue.push_back(b);
                }
            }
        }
    }
    let mut rows = vec![0; machine.states.len().max(1)];
    layers
        .into_iter()
        .map(|layer| {
            let layer = layer.unwrap_or(0);
            let position = Vec2::new(
                20.0 + layer as f32 * 215.0,
                40.0 + rows[layer] as f32 * 110.0,
            );
            rows[layer] += 1;
            position
        })
        .collect()
}

fn connect(machine: &mut StateMachine, from: usize, to: usize) -> Option<usize> {
    let from = machine.states.get(from)?.id.clone();
    let to = machine.states.get(to)?.id.clone();
    machine.transitions.push(Transition {
        from,
        to,
        event: Event {
            name: "click".into(),
            target: None,
        },
        guard: None,
        actions: vec![],
        blend: 0.0,
        after: None,
    });
    Some(machine.transitions.len() - 1)
}

fn draw_graph(
    ui: &mut egui::Ui,
    machine: &mut StateMachine,
    selected: &mut String,
    edge: &mut Option<usize>,
    positions: &mut Vec<Vec2>,
    connecting: &mut Option<usize>,
) {
    if positions.len() != machine.states.len() {
        *positions = auto_layout(machine);
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        *connecting = None;
    }
    let count = machine.states.len().min(MAX_DRAWN_STATES);
    if machine.states.len() > MAX_DRAWN_STATES || machine.transitions.len() > MAX_DRAWN_ARROWS {
        ui.label(tr("Graphvorschau zeigt höchstens 64 Zustände / 128 Pfeile. Alle Zustände und Übergänge bleiben unten bearbeitbar."));
    }
    egui::ScrollArea::both()
        .id_salt("graph_canvas")
        .max_height(300.0)
        .show(ui, |ui| {
            let size = positions
                .iter()
                .take(count)
                .fold(Vec2::new(800.0, 180.0), |size, p| {
                    size.max(*p + Vec2::new(195.0, 100.0))
                });
            let (canvas, _) = ui.allocate_exact_size(size, Sense::hover());
            let boxes: Vec<Rect> = (0..count)
                .map(|i| Rect::from_min_size(canvas.min + positions[i], Vec2::new(145.0, 42.0)))
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
                let response = ui
                    .interact(*rect, ui.id().with(("state", i)), Sense::click_and_drag())
                    .on_hover_text(&state.id);
                if response.clicked() {
                    *selected = state.id.clone();
                    *edge = None;
                }
                if response.dragged() && connecting.is_none() {
                    positions[i] = (positions[i] + ui.input(|input| input.pointer.delta()))
                        .max(Vec2::new(20.0, 30.0));
                }
                let port = rect.right_center();
                painter.circle_filled(port, 6.0, Color32::LIGHT_BLUE);
                if ui
                    .interact(
                        Rect::from_center_size(port, Vec2::splat(20.0)),
                        ui.id().with(("port", i)),
                        Sense::drag(),
                    )
                    .drag_started()
                {
                    *connecting = Some(i);
                }
            }
            if let Some(from) = *connecting {
                if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                    if let Some(rect) = boxes.get(from) {
                        painter.arrow(
                            rect.right_center(),
                            pointer - rect.right_center(),
                            Stroke::new(2.0_f32, Color32::YELLOW),
                        );
                    }
                    if ui.input(|i| i.pointer.any_released()) {
                        if let Some(to) = boxes.iter().position(|rect| {
                            canvas.intersect(ui.clip_rect()).contains(pointer)
                                && rect.contains(pointer)
                        }) {
                            *edge = connect(machine, from, to);
                        }
                        *connecting = None;
                    }
                } else if !ui.input(|i| i.pointer.any_down()) {
                    *connecting = None;
                }
            }
        });
}

#[cfg(test)]
#[path = "state_graph_expansion_tests.rs"]
mod expansion_tests;

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
                on_exit: vec![],
                announce: None,
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
                on_exit: vec![],
                announce: None,
            });
            assert!(replacement(&project, 0, machine.clone()).is_err());
            machine.states.pop();
        }
        machine.states.push(State {
            id: "active".into(),
            on_enter: vec![],
            overrides: vec![],
            on_exit: vec![],
            announce: None,
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
            blend: 0.0,
            after: None,
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
        use splinely_document::{Action, Guard};
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
            blend: 0.0,
            after: None,
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
                on_exit: vec![],
                announce: None,
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
                blend: 0.0,
                after: None,
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
