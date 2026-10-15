use crate::*;
use std::collections::BTreeMap;

/// Host-managed state machine. Emits actions rather than running JS/audio or
/// recursively dispatching emitted events (so one send is always bounded).
#[derive(Clone, Debug)]
pub struct MachineInstance {
    pub machine_id: String,
    pub current_state: String,
    pub variables: BTreeMap<String, serde_json::Value>,
    /// Actions the host should perform once when constructing this instance.
    pub initial_actions: Vec<Action>,
}
impl MachineInstance {
    pub fn new(project: &Project, machine_id: &str) -> Result<Self, DocumentError> {
        project.validate()?;
        let m = project
            .state_machines
            .iter()
            .find(|m| m.id == machine_id)
            .ok_or_else(|| DocumentError(format!("unknown machine {machine_id}")))?;
        let initial_actions = m
            .states
            .iter()
            .find(|s| s.id == m.initial)
            .unwrap()
            .on_enter
            .clone();
        let mut instance = Self {
            machine_id: machine_id.into(),
            current_state: m.initial.clone(),
            variables: project.variables.clone(),
            initial_actions,
        };
        let actions = instance.initial_actions.clone();
        instance.apply_variables(&actions);
        Ok(instance)
    }
    /// Document-order first match wins. Untargeted transition events match any
    /// target. Transition actions precede destination on-enter actions.
    pub fn send(&mut self, project: &Project, event: &Event) -> Result<Vec<Action>, DocumentError> {
        project.validate()?;
        let m = project
            .state_machines
            .iter()
            .find(|m| m.id == self.machine_id)
            .ok_or_else(|| DocumentError(format!("unknown machine {}", self.machine_id)))?;
        if !m.states.iter().any(|s| s.id == self.current_state) {
            return Err(DocumentError(format!(
                "unknown state {}",
                self.current_state
            )));
        }
        let transition = m.transitions.iter().find(|t| {
            t.from == self.current_state
                && t.event.name == event.name
                && (t.event.target.is_none() || t.event.target == event.target)
                && t.guard
                    .as_ref()
                    .is_none_or(|g| self.variables.get(&g.variable) == Some(&g.equals))
        });
        let Some(t) = transition else {
            return Ok(vec![]);
        };
        let mut actions = t.actions.clone();
        actions.extend(
            m.states
                .iter()
                .find(|s| s.id == t.to)
                .unwrap()
                .on_enter
                .clone(),
        );
        self.current_state = t.to.clone();
        self.apply_variables(&actions);
        Ok(actions)
    }
    fn apply_variables(&mut self, actions: &[Action]) {
        for action in actions {
            if let Action::SetVariable { name, value } = action {
                self.variables.insert(name.clone(), value.clone());
            }
        }
    }
}
