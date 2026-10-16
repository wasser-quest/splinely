//! Browser-independent, host-driven Splinely model playback. No rendering or code execution.
pub mod ffi;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use splinely_document::*;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

pub const MAX_EVENTS: usize = 256;
pub const MAX_OUTPUTS: usize = 4096;
pub const MAX_SEGMENTS: usize = 4096;
pub const MAX_CALLBACKS: usize = 4096;
pub type Result<T> = std::result::Result<T, DocumentError>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipStatus {
    pub id: String,
    pub time: f64,
    pub active: bool,
    pub playing: bool,
    pub weight: f64,
    #[serde(skip)]
    fade: Option<Fade>,
}
#[derive(Clone, Debug)]
struct Fade {
    from: f64,
    to: f64,
    duration: f64,
    elapsed: f64,
}

/// Host requests are data only. The host decides whether/how to play sound or invoke callbacks.
#[derive(Clone, Debug, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum HostAction {
    Event {
        event: Event,
    },
    PlayAudio {
        asset_id: String,
    },
    AudioCue {
        cue: AudioCue,
    },
    /// Clip-local start/resume. Offset and duration describe the remaining
    /// portion, truncated at the clip end. Hosts own voice cancellation on
    /// pause/stop/seek, just as for global cue voices.
    ClipAudioCue {
        clip_id: String,
        cue: AudioCue,
    },
    Callback {
        id: String,
        data: Value,
    },
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub time: f64,
    pub running: bool,
    pub states: BTreeMap<String, String>,
    pub variables: BTreeMap<String, Value>,
    pub clips: Vec<ClipStatus>,
    pub nodes: Vec<Node>,
    pub bones: Vec<BonePose>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub frame: Frame,
    pub host_actions: Vec<HostAction>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Request {
    Evaluate,
    Tick {
        delta: f64,
    },
    Event {
        event: Event,
    },
    Action {
        action: Action,
    },
    PlayClip {
        clip_id: String,
    },
    PauseClip {
        clip_id: String,
    },
    StopClip {
        clip_id: String,
    },
    SeekClip {
        clip_id: String,
        time: f64,
    },
    SetWeight {
        clip_id: String,
        weight: f64,
    },
    FadeClip {
        clip_id: String,
        weight: f64,
        duration: f64,
    },
    Play,
    Pause,
    Seek {
        time: f64,
    },
    ClearOverrides,
    Schedule {
        id: String,
        time: f64,
        #[serde(default)]
        data: Value,
        #[serde(default)]
        event: Option<Event>,
    },
    CancelCallback {
        id: String,
    },
    /// Pointer position in scene coordinates (drives followers).
    Pointer {
        x: f64,
        y: f64,
    },
    /// The pointer left the scene; followers return to rest.
    PointerLeave,
}

#[derive(Clone, Debug)]
struct RunningFade {
    from: String,
    elapsed: f64,
    duration: f64,
}
#[derive(Clone, Debug)]
struct Callback {
    id: String,
    time: f64,
    data: Value,
    event: Option<Event>,
    fired: bool,
}

#[derive(Clone, Debug)]
pub struct Runtime {
    /// Immutable after construction and shared by `Arc`, so the per-request
    /// rollback snapshot in `execute` copies only the mutable playback state.
    project: Arc<Project>,
    /// Validated once in `new`; resolves IDs without re-validating per evaluation.
    index: Arc<EvalIndex>,
    time: f64,
    running: bool,
    machines: Vec<MachineInstance>,
    variables: BTreeMap<String, Value>,
    clips: Vec<ClipStatus>,
    overrides: Vec<PropertyOverride>,
    events: VecDeque<Event>,
    outputs: Vec<HostAction>,
    callbacks: Vec<Callback>,
    audio_fired: Vec<bool>,
    clip_audio_fired: Vec<Vec<bool>>,
    events_processed: usize,
    state_fades: BTreeMap<String, RunningFade>,
    pointer: Option<[f64; 2]>,
    follow: BTreeMap<String, [f64; 2]>,
    /// Seconds each machine has spent in its current state (timer transitions).
    state_elapsed: Vec<f64>,
}
fn error(message: impl Into<String>) -> DocumentError {
    DocumentError(message.into())
}
fn time_valid(value: f64) -> Result<()> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(error("time must be finite and nonnegative"))
    }
}
fn weight_valid(value: f64) -> Result<()> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(error("weight must be finite in [0,1]"))
    }
}

impl Runtime {
    pub fn from_json(json: &str) -> Result<Self> {
        Self::new(Project::from_json(json)?)
    }
    pub fn new(project: Project) -> Result<Self> {
        let index = project.eval_index()?;
        let machines = project
            .state_machines
            .iter()
            .map(|m| MachineInstance::new_indexed(&project, &index, &m.id))
            .collect::<Result<Vec<_>>>()?;
        let mut runtime = Self {
            time: 0.0,
            running: true,
            variables: project.variables.clone(),
            clips: project
                .clips
                .iter()
                .map(|c| ClipStatus {
                    id: c.id.clone(),
                    time: 0.0,
                    active: c.autoplay,
                    playing: c.autoplay,
                    weight: c.weight,
                    fade: None,
                })
                .collect(),
            audio_fired: vec![false; project.audio_cues.len()],
            clip_audio_fired: project
                .clips
                .iter()
                .map(|c| vec![false; c.audio_cues.len()])
                .collect(),
            project: Arc::new(project),
            index: Arc::new(index),
            machines,
            overrides: vec![],
            events: VecDeque::new(),
            outputs: vec![],
            callbacks: vec![],
            events_processed: 0,
            state_fades: BTreeMap::new(),
            pointer: None,
            follow: BTreeMap::new(),
            state_elapsed: vec![],
        };
        runtime.state_elapsed = vec![0.0; runtime.machines.len()];
        // All entries run in machine order before emitted events are dispatched.
        for index in 0..runtime.machines.len() {
            let actions = std::mem::take(&mut runtime.machines[index].initial_actions);
            for action in actions {
                runtime.apply_action(action)?;
            }
        }
        runtime.drain_events()?;
        runtime.evaluate()?;
        Ok(runtime)
    }
    pub fn project(&self) -> &Project {
        &self.project
    }
    pub fn evaluate(&self) -> Result<Frame> {
        let context = EvaluationContext {
            states: self
                .machines
                .iter()
                .map(|m| (m.machine_id.clone(), m.current_state.clone()))
                .collect(),
            clips: Some(
                self.clips
                    .iter()
                    .filter(|c| c.active)
                    .map(|c| ClipPlayback {
                        clip_id: c.id.clone(),
                        time: c.time,
                    })
                    .collect(),
            ),
            clip_weights: self
                .clips
                .iter()
                .map(|c| (c.id.clone(), c.weight))
                .collect(),
            overrides: self.overrides.clone(),
            state_fades: self
                .state_fades
                .iter()
                .map(|(machine, fade)| {
                    (
                        machine.clone(),
                        StateFade {
                            from: fade.from.clone(),
                            progress: fade.elapsed / fade.duration,
                        },
                    )
                })
                .collect(),
            follow: self.follow.clone(),
            variables: self.variables.clone(),
        };
        let (nodes, bones) = self
            .project
            .evaluate_frame(&self.index, self.time, &context)?;
        Ok(Frame {
            time: self.time,
            running: self.running,
            states: context.states,
            variables: self.variables.clone(),
            clips: self.clips.clone(),
            nodes,
            bones,
        })
    }
    /// Atomic request: validation, queue/segment exhaustion or evaluation errors roll back.
    /// Successful requests drain host output, including initial entry actions, exactly once.
    pub fn execute(&mut self, request: Request) -> Result<Response> {
        let mut next = self.clone();
        next.events_processed = 0;
        next.apply_request(request)?;
        next.drain_events()?;
        next.fire_timers()?;
        next.update_followers(0.0);
        let frame = next.evaluate()?;
        let host_actions = std::mem::take(&mut next.outputs);
        *self = next;
        Ok(Response {
            frame,
            host_actions,
        })
    }
    pub fn event(&mut self, event: Event) -> Result<Response> {
        self.execute(Request::Event { event })
    }
    pub fn tick(&mut self, delta: f64) -> Result<Response> {
        self.execute(Request::Tick { delta })
    }
    pub fn request_json(&mut self, json: &str) -> Result<String> {
        let request: Request = splinely_document::from_json_strict(json)?;
        Ok(serde_json::to_string(&self.execute(request)?)?)
    }
    fn output(&mut self, action: HostAction) -> Result<()> {
        if self.outputs.len() >= MAX_OUTPUTS {
            return Err(error("host output limit exceeded"));
        }
        self.outputs.push(action);
        Ok(())
    }
    fn enqueue(&mut self, event: Event) -> Result<()> {
        if event.name.is_empty() {
            return Err(error("event name must not be empty"));
        }
        if self.events.len() >= MAX_EVENTS {
            return Err(error("event queue limit exceeded"));
        }
        self.events.push_back(event);
        Ok(())
    }
    fn drain_events(&mut self) -> Result<()> {
        while let Some(event) = self.events.pop_front() {
            if self.events_processed >= MAX_EVENTS {
                return Err(error("event drain limit exceeded"));
            }
            self.events_processed += 1;
            self.output(HostAction::Event {
                event: event.clone(),
            })?;
            for index in 0..self.machines.len() {
                // A machine with no matching transition would only return
                // nothing, so skip it (and the variable-snapshot clone).
                if !self.event_may_fire(index, &event) {
                    continue;
                }
                self.machines[index].variables = self.variables.clone();
                let actions =
                    self.machines[index].send_indexed(&self.project, &self.index, &event)?;
                self.note_transition(index);
                for action in actions {
                    self.apply_action(action)?;
                }
            }
        }
        Ok(())
    }
    /// After a machine changed state: start its crossfade and timer clock.
    fn note_transition(&mut self, index: usize) {
        let Some((from, blend)) = self.machines[index].last_transition.take() else {
            return;
        };
        self.state_elapsed[index] = 0.0;
        let machine = self.machines[index].machine_id.clone();
        if blend > 0.0 {
            self.state_fades.insert(
                machine,
                RunningFade {
                    from,
                    elapsed: 0.0,
                    duration: blend,
                },
            );
        } else {
            self.state_fades.remove(&machine);
        }
    }
    /// Fire due timer transitions (each machine at most once per call, so a
    /// zero-second timer loop cannot spin).
    fn fire_timers(&mut self) -> Result<()> {
        for index in 0..self.machines.len() {
            if !self.timer_may_fire(index) {
                continue;
            }
            self.machines[index].variables = self.variables.clone();
            let actions = self.machines[index].send_timeout_indexed(
                &self.project,
                &self.index,
                self.state_elapsed[index],
            )?;
            self.note_transition(index);
            for action in actions {
                self.apply_action(action)?;
            }
        }
        self.drain_events()
    }
    /// Whether `send` could select a transition for this event (guards aside).
    /// `MachineInstance::send_indexed` returns nothing otherwise, so skipping
    /// the call avoids a variable-snapshot clone per machine per event.
    fn event_may_fire(&self, index: usize, event: &Event) -> bool {
        let machine = &self.machines[index];
        self.project
            .state_machines
            .get(index)
            .filter(|m| m.id == machine.machine_id)
            .is_none_or(|m| {
                m.transitions.iter().any(|t| {
                    t.from == machine.current_state
                        && t.after.is_none()
                        && t.event.name == event.name
                        && (t.event.target.is_none() || t.event.target == event.target)
                })
            })
    }
    /// Counterpart of `event_may_fire` for `send_timeout`.
    fn timer_may_fire(&self, index: usize) -> bool {
        let machine = &self.machines[index];
        let elapsed = self.state_elapsed[index];
        self.project
            .state_machines
            .get(index)
            .filter(|m| m.id == machine.machine_id)
            .is_none_or(|m| {
                m.transitions.iter().any(|t| {
                    t.from == machine.current_state
                        && t.after.is_some_and(|after| after <= elapsed + 1e-9)
                })
            })
    }
    /// Seconds until the next timer transition of any machine.
    fn next_timer(&self) -> Option<f64> {
        self.machines
            .iter()
            .zip(&self.state_elapsed)
            .filter_map(|(m, elapsed)| {
                m.next_timer(&self.project)
                    .map(|after| (after - elapsed).max(0.0))
            })
            .min_by(f64::total_cmp)
    }
    /// Crossfades and followers advance on every tick, like clip fades.
    fn advance_blends(&mut self, delta: f64) {
        for elapsed in &mut self.state_elapsed {
            *elapsed += delta;
        }
        for fade in self.state_fades.values_mut() {
            fade.elapsed = (fade.elapsed + delta).min(fade.duration);
        }
        self.state_fades.retain(|_, f| f.elapsed < f.duration);
        self.update_followers(delta);
    }
    fn update_followers(&mut self, delta: f64) {
        if self.project.followers.is_empty() {
            return;
        }
        let states: BTreeMap<String, String> = self
            .machines
            .iter()
            .map(|m| (m.machine_id.clone(), m.current_state.clone()))
            .collect();
        for follower in &self.project.followers {
            let goal = self.project.follower_goal(follower, self.pointer, &states);
            let current = self.follow.get(&follower.id).copied().unwrap_or([0.0, 0.0]);
            let next = smooth_toward(current, goal, follower.smoothing, delta);
            self.follow.insert(follower.id.clone(), next);
        }
    }
    fn clip_index(&self, id: &str) -> Result<usize> {
        self.clips
            .iter()
            .position(|c| c.id == id)
            .ok_or_else(|| error(format!("unknown clip {id}")))
    }
    fn fade(&mut self, id: &str, weight: f64, duration: f64) -> Result<()> {
        weight_valid(weight)?;
        time_valid(duration)?;
        let index = self.clip_index(id)?;
        if self.project.clips[index].blend_mode == BlendMode::Override {
            return Err(error("fades require a weighted or additive clip"));
        }
        let clip = &mut self.clips[index];
        clip.fade = if duration == 0.0 {
            clip.weight = weight;
            None
        } else {
            Some(Fade {
                from: clip.weight,
                to: weight,
                duration,
                elapsed: 0.0,
            })
        };
        Ok(())
    }
    fn apply_action(&mut self, action: Action) -> Result<()> {
        match action {
            Action::SetProperty {
                node_id,
                property,
                value,
            } => {
                // A rigged group is set through its rig bones (x/y move, rotation turns).
                let node_id = match self.project.groups.iter().find(|g| g.id == node_id) {
                    Some(group) => {
                        let rig = group.rig.as_ref().ok_or_else(|| {
                            DocumentError(format!(
                                "group {node_id} has no rig; animate the group once (e.g. splinely-cli keyframe --target {node_id}) to create one"
                            ))
                        })?;
                        group_rig_bone(rig, property)
                            .ok_or_else(|| {
                                DocumentError(format!(
                                    "group {node_id}: groups animate x, y and rotation, not {property:?}"
                                ))
                            })?
                            .to_owned()
                    }
                    None => node_id,
                };
                let patch = PropertyOverride {
                    node_id,
                    property,
                    value,
                };
                // Reuse core validation, including bone property and geometry constraints.
                self.project.evaluate_frame(
                    &self.index,
                    0.0,
                    &EvaluationContext {
                        clips: Some(vec![]),
                        overrides: vec![patch.clone()],
                        ..Default::default()
                    },
                )?;
                if let Some(old) = self
                    .overrides
                    .iter_mut()
                    .find(|o| o.node_id == patch.node_id && o.property == patch.property)
                {
                    *old = patch;
                } else {
                    self.overrides.push(patch);
                }
            }
            Action::SetVariable { name, value } => {
                let old = self
                    .variables
                    .get(&name)
                    .ok_or_else(|| error(format!("unknown variable {name}")))?;
                let kind = |v: &Value| match v {
                    Value::Null => 0,
                    Value::Bool(_) => 1,
                    Value::Number(_) => 2,
                    Value::String(_) => 3,
                    _ => 4,
                };
                if kind(old) != kind(&value) || kind(&value) == 4 {
                    return Err(error("variable type mismatch"));
                }
                self.project
                    .blend_space_weights(&[(name.clone(), value.clone())].into())?;
                self.variables.insert(name, value);
            }
            Action::PlayClip { clip_id } => {
                let i = self.clip_index(&clip_id)?;
                let c = &mut self.clips[i];
                if !c.playing
                    && !self.project.clips[i].looping
                    && c.time >= self.project.clips[i].duration
                {
                    c.time = 0.0;
                    self.clip_audio_fired[i].fill(false);
                }
                c.active = true;
                c.playing = true;
            }
            Action::StopClip { clip_id } => {
                let i = self.clip_index(&clip_id)?;
                let c = &mut self.clips[i];
                c.active = false;
                c.playing = false;
                c.time = 0.0;
                c.fade = None;
                self.clip_audio_fired[i].fill(false);
            }
            Action::BlendClip {
                clip_id,
                weight,
                duration,
            } => self.fade(&clip_id, weight, duration)?,
            Action::Emit { event } => self.enqueue(event)?,
            Action::PlayAudio { asset_id } => {
                if !self.project.audio_assets.iter().any(|a| a.id == asset_id) {
                    return Err(error(format!("unknown audio asset {asset_id}")));
                }
                self.output(HostAction::PlayAudio { asset_id })?;
            }
        }
        Ok(())
    }
    fn apply_request(&mut self, request: Request) -> Result<()> {
        match request {
            Request::Evaluate => {}
            Request::Event { event } => self.enqueue(event)?,
            Request::Tick { delta } => self.advance(delta)?,
            Request::Action { action } => self.apply_action(action)?,
            Request::PlayClip { clip_id } => self.apply_action(Action::PlayClip { clip_id })?,
            Request::StopClip { clip_id } => self.apply_action(Action::StopClip { clip_id })?,
            Request::PauseClip { clip_id } => {
                let i = self.clip_index(&clip_id)?;
                self.clips[i].playing = false;
                self.clip_audio_fired[i].fill(false);
            }
            Request::SeekClip { clip_id, time } => {
                time_valid(time)?;
                let i = self.clip_index(&clip_id)?;
                let c = &self.project.clips[i];
                self.clips[i].time = if c.looping {
                    time % c.duration
                } else {
                    time.min(c.duration)
                };
                self.clips[i].active = true;
                self.clip_audio_fired[i].fill(false);
                if self.running {
                    self.clip_audio_boundary_for(Some(i))?;
                }
            }
            Request::SetWeight { clip_id, weight } => {
                weight_valid(weight)?;
                let i = self.clip_index(&clip_id)?;
                self.clips[i].weight = weight;
                self.clips[i].fade = None;
            }
            Request::FadeClip {
                clip_id,
                weight,
                duration,
            } => self.fade(&clip_id, weight, duration)?,
            Request::Play => self.running = true,
            Request::Pause => {
                self.running = false;
                for fired in &mut self.clip_audio_fired {
                    fired.fill(false);
                }
            }
            Request::Seek { time } => {
                time_valid(time)?;
                self.time = time;
                for (clip, model) in self.clips.iter_mut().zip(&self.project.clips) {
                    if clip.active {
                        clip.time = if model.looping {
                            time % model.duration
                        } else {
                            time.min(model.duration)
                        };
                    }
                }
                for fired in &mut self.clip_audio_fired {
                    fired.fill(false);
                }
                if self.running {
                    self.clip_audio_boundary()?;
                }
                for (fired, cue) in self.audio_fired.iter_mut().zip(&self.project.audio_cues) {
                    *fired = cue.time <= time;
                }
                for cb in &mut self.callbacks {
                    cb.fired = cb.time <= time;
                }
            }
            Request::ClearOverrides => self.overrides.clear(),
            Request::Schedule {
                id,
                time,
                data,
                event,
            } => {
                time_valid(time)?;
                if id.is_empty() || self.callbacks.iter().any(|c| c.id == id) {
                    return Err(error("callback id must be nonempty and unique"));
                }
                if event.as_ref().is_some_and(|e| e.name.is_empty()) {
                    return Err(error("event name must not be empty"));
                }
                if self.callbacks.len() >= MAX_CALLBACKS {
                    return Err(error("callback limit exceeded"));
                }
                self.callbacks.push(Callback {
                    id,
                    time,
                    data,
                    event,
                    fired: time < self.time,
                });
            }
            Request::CancelCallback { id } => self.callbacks.retain(|c| c.id != id),
            Request::Pointer { x, y } => {
                if !(x.is_finite() && y.is_finite() && x.abs() <= 1e7 && y.abs() <= 1e7) {
                    return Err(error(
                        "pointer coordinates must be finite and within +/- 10000000",
                    ));
                }
                self.pointer = Some([x, y]);
            }
            Request::PointerLeave => self.pointer = None,
        }
        Ok(())
    }
    fn clip_audio_boundary(&mut self) -> Result<()> {
        self.clip_audio_boundary_for(None)
    }
    fn clip_audio_boundary_for(&mut self, only: Option<usize>) -> Result<()> {
        for i in 0..self.clips.len() {
            if only.is_some_and(|index| index != i) {
                continue;
            }
            let status = &self.clips[i];
            if !status.active || !status.playing {
                continue;
            }
            for j in 0..self.clip_audio_fired[i].len() {
                let source = &self.project.clips[i].audio_cues[j];
                if self.clip_audio_fired[i][j] || source.time > self.clips[i].time {
                    continue;
                }
                self.clip_audio_fired[i][j] = true;
                let elapsed = self.clips[i].time - source.time;
                let length = source
                    .duration
                    .unwrap_or(f64::INFINITY)
                    .min(self.project.clips[i].duration - source.time);
                if elapsed >= length {
                    continue;
                }
                let mut cue = source.clone();
                cue.offset += elapsed;
                cue.duration = Some(length - elapsed);
                self.output(HostAction::ClipAudioCue {
                    clip_id: self.clips[i].id.clone(),
                    cue,
                })?;
            }
        }
        Ok(())
    }
    fn global_boundary(&mut self) -> Result<()> {
        // Equal-time order: audio (document order), then registered callbacks.
        for i in 0..self.audio_fired.len() {
            if !self.audio_fired[i] && self.project.audio_cues[i].time <= self.time {
                self.audio_fired[i] = true;
                self.output(HostAction::AudioCue {
                    cue: self.project.audio_cues[i].clone(),
                })?;
            }
        }
        for i in 0..self.callbacks.len() {
            if !self.callbacks[i].fired && self.callbacks[i].time <= self.time {
                self.callbacks[i].fired = true;
                let cb = self.callbacks[i].clone();
                self.output(HostAction::Callback {
                    id: cb.id,
                    data: cb.data,
                })?;
                if let Some(event) = cb.event {
                    self.enqueue(event)?;
                }
            }
        }
        Ok(())
    }
    fn advance(&mut self, delta: f64) -> Result<()> {
        time_valid(delta)?;
        let endpoint = if self.running {
            self.time + delta
        } else {
            self.time
        };
        time_valid(endpoint)?;
        if self.running && delta > 0.0 && endpoint == self.time {
            return Err(error("tick exceeds clock precision"));
        }
        if self.running {
            self.global_boundary()?;
            self.clip_audio_boundary()?;
            self.drain_events()?;
        }
        let mut remaining = if self.running {
            endpoint - self.time
        } else {
            delta
        };
        let mut segments = 0;
        while remaining > 0.0 {
            if segments >= MAX_SEGMENTS {
                return Err(error("tick segment limit exceeded"));
            }
            segments += 1;
            let mut step = remaining;
            // Split at the next timer so it fires exactly on time.
            if let Some(timer) = self.next_timer().filter(|t| *t > 1e-9) {
                step = step.min(timer);
            }
            if self.running {
                for (i, cue) in self.project.audio_cues.iter().enumerate() {
                    if !self.audio_fired[i] {
                        step = step.min((cue.time - self.time).max(0.0));
                    }
                }
                for cb in &self.callbacks {
                    if !cb.fired {
                        step = step.min((cb.time - self.time).max(0.0));
                    }
                }
                for (i, (c, model)) in self.clips.iter().zip(&self.project.clips).enumerate() {
                    if c.playing {
                        step = step.min((model.duration - c.time).max(0.0));
                        for (j, cue) in model.audio_cues.iter().enumerate() {
                            if !self.clip_audio_fired[i][j] {
                                step = step.min((cue.time - c.time).max(0.0));
                            }
                        }
                    }
                }
            }
            self.advance_blends(step);
            self.fire_timers()?;
            // Fades advance even on paused or ended clips and while timeline is paused.
            for c in &mut self.clips {
                if let Some(f) = &mut c.fade {
                    f.elapsed += step.min(f.duration - f.elapsed);
                    c.weight =
                        f.from * (1.0 - f.elapsed / f.duration) + f.to * (f.elapsed / f.duration);
                    if f.elapsed >= f.duration {
                        c.weight = f.to;
                        c.fade = None;
                    }
                }
            }
            if self.running {
                let next = self.time + step;
                time_valid(next)?;
                if step > 0.0 && next == self.time {
                    return Err(error("tick exceeds clock precision"));
                }
                self.time = next;
                let mut boundaries = vec![];
                for (i, (c, model)) in self.clips.iter_mut().zip(&self.project.clips).enumerate() {
                    if c.playing {
                        c.time = (c.time + step).min(model.duration);
                        if model.duration - c.time <= 8.0 * f64::EPSILON * model.duration {
                            if model.looping {
                                c.time = 0.0;
                                self.clip_audio_fired[i].fill(false);
                            } else {
                                c.time = model.duration;
                                c.playing = false;
                            }
                            boundaries.push(Event {
                                name: if model.looping {
                                    "clip.loop"
                                } else {
                                    "clip.end"
                                }
                                .into(),
                                target: Some(c.id.clone()),
                            });
                        }
                    }
                }
                self.global_boundary()?;
                self.clip_audio_boundary()?;
                for event in boundaries {
                    self.enqueue(event)?;
                }
                self.drain_events()?;
            }
            // An absolute endpoint avoids subtraction residue after decimal loop boundaries.
            remaining = if self.running {
                (endpoint - self.time).max(0.0)
            } else {
                0.0
            };
        }
        Ok(())
    }
}
