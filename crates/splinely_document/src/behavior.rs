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
    /// Set by `send` when a transition fires: the state it left and its blend
    /// duration, so hosts can crossfade.
    pub last_transition: Option<(String, f64)>,
}
impl MachineInstance {
    pub fn new(project: &Project, machine_id: &str) -> Result<Self, DocumentError> {
        project.validate()?;
        Self::build(project, machine_id)
    }
    /// Like [`MachineInstance::new`] for a project that already has an
    /// [`EvalIndex`]: holding the index proves `validate` succeeded, so the
    /// whole-project validation is not repeated (the index only rejects a
    /// project whose shape changed since it was built).
    pub fn new_indexed(
        project: &Project,
        index: &EvalIndex,
        machine_id: &str,
    ) -> Result<Self, DocumentError> {
        index.check(project)?;
        Self::build(project, machine_id)
    }
    fn build(project: &Project, machine_id: &str) -> Result<Self, DocumentError> {
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
            last_transition: None,
        };
        let actions = instance.initial_actions.clone();
        instance.apply_variables(&actions);
        Ok(instance)
    }
    /// Document-order first match wins. Untargeted transition events match any
    /// target. Timer transitions never match events. The left state's exit
    /// actions run first, then transition actions, then destination on-enter.
    pub fn send(&mut self, project: &Project, event: &Event) -> Result<Vec<Action>, DocumentError> {
        self.fire(project, None, |t, _| Self::event_matches(t, event))
    }
    /// `send` for a validated project with an [`EvalIndex`] (no whole-project
    /// validation per fired transition).
    pub fn send_indexed(
        &mut self,
        project: &Project,
        index: &EvalIndex,
        event: &Event,
    ) -> Result<Vec<Action>, DocumentError> {
        self.fire(project, Some(index), |t, _| Self::event_matches(t, event))
    }
    fn event_matches(t: &Transition, event: &Event) -> bool {
        t.after.is_none()
            && t.event.name == event.name
            && (t.event.target.is_none() || t.event.target == event.target)
    }
    /// Fire the first timer transition of the current state that is due after
    /// `elapsed` seconds in that state (guards are checked now).
    pub fn send_timeout(
        &mut self,
        project: &Project,
        elapsed: f64,
    ) -> Result<Vec<Action>, DocumentError> {
        self.fire(project, None, |t, _| Self::timer_due(t, elapsed))
    }
    /// `send_timeout` for a validated project with an [`EvalIndex`].
    pub fn send_timeout_indexed(
        &mut self,
        project: &Project,
        index: &EvalIndex,
        elapsed: f64,
    ) -> Result<Vec<Action>, DocumentError> {
        self.fire(project, Some(index), |t, _| Self::timer_due(t, elapsed))
    }
    fn timer_due(t: &Transition, elapsed: f64) -> bool {
        // A nanosecond of slack absorbs rounding from split ticks.
        t.after.is_some_and(|after| after <= elapsed + 1e-9)
    }
    /// Seconds until the earliest timer transition of the current state, if any.
    pub fn next_timer(&self, project: &Project) -> Option<f64> {
        let m = project
            .state_machines
            .iter()
            .find(|m| m.id == self.machine_id)?;
        m.transitions
            .iter()
            .filter(|t| t.from == self.current_state)
            .filter_map(|t| t.after)
            .min_by(f64::total_cmp)
    }
    fn fire(
        &mut self,
        project: &Project,
        index: Option<&EvalIndex>,
        matches: impl Fn(&Transition, &Self) -> bool,
    ) -> Result<Vec<Action>, DocumentError> {
        match index {
            Some(index) => index.check(project)?,
            None => project.validate()?,
        }
        let m = project
            .state_machines
            .iter()
            .find(|m| m.id == self.machine_id)
            .ok_or_else(|| DocumentError(format!("unknown machine {}", self.machine_id)))?;
        let Some(current) = m.states.iter().find(|s| s.id == self.current_state) else {
            return Err(DocumentError(format!(
                "unknown state {}",
                self.current_state
            )));
        };
        let transition = m.transitions.iter().find(|t| {
            t.from == self.current_state
                && matches(t, self)
                && t.guard
                    .as_ref()
                    .is_none_or(|g| self.variables.get(&g.variable) == Some(&g.equals))
        });
        self.last_transition = None;
        let Some(t) = transition else {
            return Ok(vec![]);
        };
        self.last_transition = Some((self.current_state.clone(), t.blend));
        let mut actions = current.on_exit.clone();
        actions.extend(t.actions.iter().cloned());
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
