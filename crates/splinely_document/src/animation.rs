use crate::*;
use std::collections::{BTreeMap, HashMap};

/// Stateless input to evaluation. No document data is mutated during playback.
#[derive(Clone, Debug, Default)]
pub struct EvaluationContext {
    /// Missing machines use their initial state.
    pub states: BTreeMap<String, String>,
    /// None selects autoplay clips; Some (including empty) replaces that selection.
    /// Playback list order is priority order; overrides replace, adjacent weighted clips blend.
    pub clips: Option<Vec<ClipPlayback>>,
    /// Effective clip weights, including inactive clips; absent entries use Clip.weight.
    pub clip_weights: BTreeMap<String, f64>,
    /// Live variable values for two-parameter blend spaces (missing = authored).
    pub variables: BTreeMap<String, serde_json::Value>,
    /// Applied after all clip tracks.
    pub overrides: Vec<PropertyOverride>,
    /// Machines crossfading from a previous state into their current one.
    pub state_fades: BTreeMap<String, StateFade>,
    /// Current (smoothed) follower values by follower ID, added last.
    pub follow: BTreeMap<String, [f64; 2]>,
}
#[derive(Clone, Debug)]
pub struct ClipPlayback {
    pub clip_id: String,
    pub time: f64,
}

/// A resolved animation target: a bone (preferred, as in the document ID rules)
/// or a node, by position in the project lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TargetRef {
    Bone(usize),
    Node(usize),
}

/// Pre-resolved ID lookups for one validated project. Holding an `EvalIndex`
/// is the proof that `Project::validate` already succeeded, so hosts that
/// evaluate repeatedly (the native runtime) validate once at construction and
/// then call [`Project::evaluate_frame`] without per-evaluation validation or
/// linear ID scans. An index only fits the project it was built from; a
/// project edited afterwards must get a fresh index.
#[derive(Clone, Debug)]
pub struct EvalIndex {
    nodes: HashMap<String, usize>,
    bones: HashMap<String, usize>,
    clips: HashMap<String, usize>,
    /// Per clip, per track: the resolved target.
    clip_tracks: Vec<Vec<Option<TargetRef>>>,
    /// Node, bone, clip and track counts; a cheap mismatch guard.
    shape: [usize; 4],
}
impl EvalIndex {
    fn build(project: &Project) -> Self {
        fn first_by_id<'a>(ids: impl Iterator<Item = &'a str>) -> HashMap<String, usize> {
            let mut map = HashMap::new();
            for (i, id) in ids.enumerate() {
                map.entry(id.to_string()).or_insert(i);
            }
            map
        }
        let mut index = Self {
            nodes: first_by_id(project.nodes.iter().map(|n| n.id.as_str())),
            bones: first_by_id(project.bones.iter().map(|b| b.id.as_str())),
            clips: first_by_id(project.clips.iter().map(|c| c.id.as_str())),
            clip_tracks: vec![],
            shape: [0; 4],
        };
        index.clip_tracks = project
            .clips
            .iter()
            .map(|c| c.tracks.iter().map(|t| index.target(&t.node_id)).collect())
            .collect();
        index.shape = [
            project.nodes.len(),
            project.bones.len(),
            project.clips.len(),
            project.clips.iter().map(|c| c.tracks.len()).sum(),
        ];
        index
    }
    /// Bones win over nodes, matching the animation target rules.
    pub(crate) fn target(&self, id: &str) -> Option<TargetRef> {
        self.bones
            .get(id)
            .map(|i| TargetRef::Bone(*i))
            .or_else(|| self.nodes.get(id).map(|i| TargetRef::Node(*i)))
    }
    fn expect_target(&self, id: &str) -> TargetRef {
        self.target(id).expect("validated animation target")
    }
    pub(crate) fn node(&self, id: &str) -> Option<usize> {
        self.nodes.get(id).copied()
    }
    pub(crate) fn bone(&self, id: &str) -> Option<usize> {
        self.bones.get(id).copied()
    }
    pub(crate) fn check(&self, project: &Project) -> Result<(), DocumentError> {
        let shape = [
            project.nodes.len(),
            project.bones.len(),
            project.clips.len(),
            project.clips.iter().map(|c| c.tracks.len()).sum(),
        ];
        if shape == self.shape {
            Ok(())
        } else {
            Err(DocumentError(
                "evaluation index does not belong to this project".into(),
            ))
        }
    }
}

impl Project {
    pub fn evaluate(&self, time: f64) -> Result<Vec<Node>, DocumentError> {
        self.evaluate_with_context(time, &EvaluationContext::default())
    }
    pub fn evaluate_clip(&self, clip_id: &str, time: f64) -> Result<Vec<Node>, DocumentError> {
        self.evaluate_with_context(
            time,
            &EvaluationContext {
                clips: Some(vec![ClipPlayback {
                    clip_id: clip_id.into(),
                    time,
                }]),
                ..Default::default()
            },
        )
    }
    pub fn evaluate_with_context(
        &self,
        time: f64,
        context: &EvaluationContext,
    ) -> Result<Vec<Node>, DocumentError> {
        let index = self.eval_index()?;
        Ok(self.evaluate_frame(&index, time, context)?.0)
    }
    pub fn evaluate_bones(
        &self,
        time: f64,
        context: &EvaluationContext,
    ) -> Result<Vec<BonePose>, DocumentError> {
        let index = self.eval_index()?;
        let (_, bones) = self.evaluate_local(&index, time, context)?;
        crate::bones::solve_bones(&bones)
    }
    /// Validate once and build the lookup tables for repeated evaluation.
    pub fn eval_index(&self) -> Result<EvalIndex, DocumentError> {
        self.validate()?;
        Ok(EvalIndex::build(self))
    }
    /// Evaluated nodes (bone-attached, motion applied) and solved bone poses in
    /// one pass; identical to `evaluate_with_context` plus `evaluate_bones`
    /// without validating the project or scanning IDs again. `index` must come
    /// from `eval_index` of this unmodified project.
    pub fn evaluate_frame(
        &self,
        index: &EvalIndex,
        time: f64,
        context: &EvaluationContext,
    ) -> Result<(Vec<Node>, Vec<BonePose>), DocumentError> {
        index.check(self)?;
        let (mut nodes, bones) = self.evaluate_local(index, time, context)?;
        let poses = crate::bones::solve_bones(&bones)?;
        crate::bones::attach_nodes(&mut nodes, &poses)?;
        crate::motion::apply(&mut nodes)?;
        Ok((nodes, poses))
    }
    fn evaluate_local(
        &self,
        index: &EvalIndex,
        time: f64,
        context: &EvaluationContext,
    ) -> Result<(Vec<Node>, Vec<Bone>), DocumentError> {
        valid_time(time)?;
        for (machine, state) in &context.states {
            if !self
                .state_machines
                .iter()
                .any(|m| &m.id == machine && m.states.iter().any(|s| &s.id == state))
            {
                return Err(DocumentError(format!(
                    "unknown machine/state {machine}/{state}"
                )));
            }
        }
        for (id, weight) in &context.clip_weights {
            if !index.clips.contains_key(id) {
                return Err(DocumentError(format!("unknown clip {id}")));
            }
            crate::validation::validate_clip_weight(*weight)?;
        }
        let mut nodes = self.nodes.clone();
        let mut bones = self.bones.clone();
        for m in &self.state_machines {
            let state = context.states.get(&m.id).unwrap_or(&m.initial);
            let current = &m.states.iter().find(|s| &s.id == state).unwrap().overrides;
            let fade = context.state_fades.get(&m.id).and_then(|fade| {
                let from = m.states.iter().find(|s| s.id == fade.from)?;
                (fade.progress < 1.0).then_some((from, fade.progress))
            });
            let Some((from, progress)) = fade else {
                for o in current {
                    apply_override(&mut nodes, &mut bones, index, o);
                }
                continue;
            };
            // Crossfade every property either state sets; a property only one
            // state sets fades between that value and the value underneath.
            let t = crate::follow::ease_fade(progress);
            let mut touched: Vec<(&str, Property)> = Vec::new();
            for o in from.overrides.iter().chain(current) {
                if !touched.contains(&(o.node_id.as_str(), o.property)) {
                    touched.push((&o.node_id, o.property));
                }
            }
            for (id, property) in touched {
                let target = index.expect_target(id);
                let under = get_target(&nodes, &bones, target, property);
                let pick = |list: &[PropertyOverride]| {
                    list.iter()
                        .rev()
                        .find(|o| o.node_id == id && o.property == property)
                        .map_or_else(|| under.clone(), |o| o.value.clone())
                };
                let value = crate::follow::blend_values(&pick(&from.overrides), &pick(current), t);
                set_target(&mut nodes, &mut bones, target, property, value);
            }
        }
        let mut selected: Vec<(usize, f64)> = Vec::new();
        if let Some(clips) = &context.clips {
            for playback in clips {
                valid_time(playback.time)?;
                let i = *index
                    .clips
                    .get(&playback.clip_id)
                    .ok_or_else(|| DocumentError(format!("unknown clip {}", playback.clip_id)))?;
                selected.push((i, playback.time));
            }
        } else {
            selected.extend(
                self.clips
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.autoplay)
                    .map(|(i, _)| (i, time)),
            );
        }
        let space_weights = self.blend_space_weights(&context.variables)?;
        let mut group: Vec<WeightedClip<'_>> = Vec::new();
        for (i, sample_time) in selected {
            let clip = &self.clips[i];
            let targets = index.clip_tracks[i].as_slice();
            if clip.blend_mode == BlendMode::Weighted {
                let weight = context
                    .clip_weights
                    .get(&clip.id)
                    .copied()
                    .unwrap_or(clip.weight);
                group.push((
                    clip,
                    targets,
                    sample_time,
                    weight * space_weights.get(&clip.id).copied().unwrap_or(1.0),
                ));
            } else {
                apply_weighted_group(&mut nodes, &mut bones, &group);
                group.clear();
                if clip.blend_mode == BlendMode::Additive {
                    let weight = context
                        .clip_weights
                        .get(&clip.id)
                        .copied()
                        .unwrap_or(clip.weight);
                    apply_additive(
                        &mut nodes,
                        &mut bones,
                        &self.nodes,
                        &self.bones,
                        clip,
                        targets,
                        sample_time,
                        weight,
                    );
                } else {
                    apply_clip(&mut nodes, &mut bones, clip, targets, sample_time);
                }
            }
        }
        apply_weighted_group(&mut nodes, &mut bones, &group);
        for o in &context.overrides {
            self.validate_property(&o.node_id, o.property, &o.value)?;
            apply_override(&mut nodes, &mut bones, index, o);
        }
        for (machine, fade) in &context.state_fades {
            let known = self
                .state_machines
                .iter()
                .any(|m| &m.id == machine && m.states.iter().any(|s| s.id == fade.from));
            if !known || !fade.progress.is_finite() {
                return Err(DocumentError(format!("invalid state fade for {machine}")));
            }
        }
        for (id, value) in &context.follow {
            if !self.followers.iter().any(|f| &f.id == id) || !value.iter().all(|v| v.is_finite()) {
                return Err(DocumentError(format!("invalid follower value for {id}")));
            }
        }
        self.apply_followers(&mut nodes, &mut bones, index, &context.follow);
        Ok((nodes, bones))
    }
}
/// A clip with its resolved track targets, sample time and effective weight.
type WeightedClip<'a> = (&'a Clip, &'a [Option<TargetRef>], f64, f64);
fn valid_time(time: f64) -> Result<(), DocumentError> {
    if time.is_finite() && time >= 0.0 {
        Ok(())
    } else {
        Err(DocumentError(
            "evaluation time must be nonnegative and finite".into(),
        ))
    }
}
fn clip_time(clip: &Clip, time: f64) -> f64 {
    if clip.looping {
        time % clip.duration
    } else {
        time.min(clip.duration)
    }
}
impl Node {
    /// Current base value of an animatable property (None if the node kind
    /// does not have it, e.g. radius on a rectangle).
    pub fn property_value(&self, p: Property) -> Option<AnimValue> {
        Some(match p {
            Property::MotionProgress => AnimValue::Number(self.motion_path.as_ref()?.progress),
            Property::X => AnimValue::Number(self.transform.x),
            Property::Y => AnimValue::Number(self.transform.y),
            Property::Rotation => AnimValue::Number(self.transform.rotation),
            Property::ScaleX => AnimValue::Number(self.transform.scale_x),
            Property::ScaleY => AnimValue::Number(self.transform.scale_y),
            Property::Opacity => AnimValue::Number(self.style.opacity),
            Property::Fill => AnimValue::Color(self.style.fill),
            Property::Stroke => AnimValue::Color(self.style.stroke?),
            Property::Width => match self.kind {
                NodeKind::Rect { width, .. } => AnimValue::Number(width),
                _ => return None,
            },
            Property::Height => match self.kind {
                NodeKind::Rect { height, .. } => AnimValue::Number(height),
                _ => return None,
            },
            Property::Radius => match self.kind {
                NodeKind::Circle { radius, .. } => AnimValue::Number(radius),
                _ => return None,
            },
            Property::FontSize => match self.kind {
                NodeKind::Text { font_size, .. } => AnimValue::Number(font_size),
                _ => return None,
            },
            Property::Gradient => AnimValue::Gradient(self.gradient.clone()?),
            Property::D => match &self.kind {
                NodeKind::Path { d, .. } => AnimValue::Path(d.clone()),
                _ => return None,
            },
        })
    }

    /// Set a base property from a value of the matching type (as returned by
    /// `property_value`); mismatched types are ignored.
    pub fn set_property_value(&mut self, p: Property, v: AnimValue) {
        let matching = matches!(
            (p, &v),
            (Property::Fill | Property::Stroke, AnimValue::Color(_))
                | (Property::Gradient, AnimValue::Gradient(_))
                | (Property::D, AnimValue::Path(_))
        ) || (!matches!(
            p,
            Property::Fill | Property::Stroke | Property::Gradient | Property::D
        ) && matches!(v, AnimValue::Number(_)));
        if matching {
            set_property(self, p, v);
        }
    }
}

impl Track {
    /// The track's value at a clip-local time (eased interpolation).
    pub fn sample(&self, time: f64) -> AnimValue {
        sample_track(self, time)
    }
}
fn sample_track(track: &Track, time: f64) -> AnimValue {
    let keys = &track.keyframes;
    if time <= keys[0].time {
        keys[0].value.clone()
    } else if time >= keys.last().unwrap().time {
        keys.last().unwrap().value.clone()
    } else {
        let index = keys.partition_point(|k| k.time <= time) - 1;
        let (a, b) = (&keys[index], &keys[index + 1]);
        a.value.interpolate(
            &b.value,
            ease(a.easing, (time - a.time) / (b.time - a.time)),
        )
    }
}
fn apply_clip(
    nodes: &mut [Node],
    bones: &mut [Bone],
    clip: &Clip,
    targets: &[Option<TargetRef>],
    time: f64,
) {
    let time = clip_time(clip, time);
    for (track, target) in clip.tracks.iter().zip(targets) {
        set_target(
            nodes,
            bones,
            target.expect("validated track target"),
            track.property,
            sample_track(track, time),
        );
    }
}

// Reference is the immutable document pose, not a state pose or first keyframe.
#[allow(clippy::too_many_arguments)]
fn apply_additive(
    nodes: &mut [Node],
    bones: &mut [Bone],
    rest_nodes: &[Node],
    rest_bones: &[Bone],
    clip: &Clip,
    targets: &[Option<TargetRef>],
    time: f64,
    weight: f64,
) {
    if weight == 0.0 {
        return;
    }
    for (track, target) in clip.tracks.iter().zip(targets) {
        let target = target.expect("validated track target");
        let baseline = get_target(nodes, bones, target, track.property);
        let reference = get_target(rest_nodes, rest_bones, target, track.property);
        let sample = sample_track(track, clip_time(clip, time));
        let mixed: Vec<f64> = baseline
            .numbers()
            .iter()
            .zip(sample.numbers())
            .zip(reference.numbers())
            .map(|((b, s), r)| b + weight * (s - r))
            .collect();
        let mut value = baseline.with_numbers(&mixed);
        match &mut value {
            AnimValue::Color(c) => {
                c.r = c.r.clamp(0., 1.);
                c.g = c.g.clamp(0., 1.);
                c.b = c.b.clamp(0., 1.);
                c.a = c.a.clamp(0., 1.);
            }
            AnimValue::Number(v) => match track.property {
                Property::Opacity | Property::MotionProgress => *v = v.clamp(0., 1.),
                Property::Width | Property::Height | Property::Radius | Property::FontSize => {
                    *v = v.max(0.)
                }
                _ => {}
            },
            _ => unreachable!("validated additive value"),
        }
        set_target(nodes, bones, target, track.property, value);
    }
}

// Collect only contributing tracks: sparse channels have their own weight sum.
fn apply_weighted_group(nodes: &mut [Node], bones: &mut [Bone], group: &[WeightedClip<'_>]) {
    type WeightedChannel = (TargetRef, Property, Vec<(AnimValue, f64)>);
    let mut channels: Vec<WeightedChannel> = Vec::new();
    // First-seen order is the application order; the map only finds a channel.
    let mut slots: HashMap<(TargetRef, Property), usize> = HashMap::new();
    for &(clip, targets, time, weight) in group {
        if weight == 0.0 {
            continue;
        }
        let time = clip_time(clip, time);
        for (track, target) in clip.tracks.iter().zip(targets) {
            let target = target.expect("validated track target");
            let sample = (sample_track(track, time), weight);
            if let Some(slot) = slots.get(&(target, track.property)) {
                channels[*slot].2.push(sample);
            } else {
                slots.insert((target, track.property), channels.len());
                channels.push((target, track.property, vec![sample]));
            }
        }
    }
    for (target, property, samples) in channels {
        let baseline = get_target(nodes, bones, target, property);
        let sum: f64 = samples.iter().map(|(_, weight)| weight).sum();
        let divisor = sum.max(1.0);
        let residual = (1.0 - sum).max(0.0);
        // Number-by-number mix when every sample matches the baseline's shape;
        // otherwise the heaviest contribution wins (structured values can't blend).
        let value = if samples.iter().all(|(v, _)| v.compatible(&baseline)) {
            let base = baseline.numbers();
            let mut mixed: Vec<f64> = base.iter().map(|b| b * residual).collect();
            for (sample, weight) in &samples {
                for (m, v) in mixed.iter_mut().zip(sample.numbers()) {
                    *m += v * (weight / divisor);
                }
            }
            baseline.with_numbers(&mixed)
        } else {
            let (heaviest, weight) = samples
                .iter()
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .cloned()
                .unwrap();
            if weight >= residual {
                heaviest
            } else {
                baseline
            }
        };
        set_target(nodes, bones, target, property, value);
    }
}
fn get_target(nodes: &[Node], bones: &[Bone], target: TargetRef, property: Property) -> AnimValue {
    let node = match target {
        TargetRef::Bone(i) => {
            let bone = &bones[i];
            return AnimValue::Number(match property {
                Property::X => bone.x,
                Property::Y => bone.y,
                Property::Rotation => bone.rotation,
                _ => unreachable!("validated bone property"),
            });
        }
        TargetRef::Node(i) => &nodes[i],
    };
    AnimValue::Number(match property {
        Property::MotionProgress => {
            node.motion_path
                .as_ref()
                .expect("validated motion binding")
                .progress
        }
        Property::X => node.transform.x,
        Property::Y => node.transform.y,
        Property::Rotation => node.transform.rotation,
        Property::ScaleX => node.transform.scale_x,
        Property::ScaleY => node.transform.scale_y,
        Property::Opacity => node.style.opacity,
        Property::Fill => return AnimValue::Color(node.style.fill),
        Property::Stroke => {
            return AnimValue::Color(node.style.stroke.unwrap_or(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }))
        }
        Property::Width => match node.kind {
            NodeKind::Rect { width, .. } => width,
            _ => unreachable!(),
        },
        Property::Height => match node.kind {
            NodeKind::Rect { height, .. } => height,
            _ => unreachable!(),
        },
        Property::Radius => match node.kind {
            NodeKind::Circle { radius, .. } => radius,
            _ => unreachable!(),
        },
        Property::FontSize => match node.kind {
            NodeKind::Text { font_size, .. } => font_size,
            _ => unreachable!(),
        },
        Property::Gradient => {
            return AnimValue::Gradient(node.gradient.clone().expect("validated gradient node"))
        }
        Property::D => match &node.kind {
            NodeKind::Path { d, .. } => return AnimValue::Path(d.clone()),
            _ => unreachable!(),
        },
    })
}
fn ease(easing: Easing, t: f64) -> f64 {
    match easing {
        Easing::Linear => t,
        Easing::Step => 0.0,
        Easing::CubicBezier { x1, y1, x2, y2 } => {
            let bezier = |s: f64, a: f64, b: f64| {
                3.0 * (1.0 - s) * (1.0 - s) * s * a + 3.0 * (1.0 - s) * s * s * b + s * s * s
            };
            let (mut low, mut high) = (0.0, 1.0);
            for _ in 0..60 {
                let mid = (low + high) * 0.5;
                if bezier(mid, x1, x2) < t {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            bezier((low + high) * 0.5, y1, y2)
        }
    }
}
fn apply_override(nodes: &mut [Node], bones: &mut [Bone], index: &EvalIndex, o: &PropertyOverride) {
    set_target(
        nodes,
        bones,
        index.expect_target(&o.node_id),
        o.property,
        o.value.clone(),
    );
}
fn set_target(
    nodes: &mut [Node],
    bones: &mut [Bone],
    target: TargetRef,
    p: Property,
    v: AnimValue,
) {
    match target {
        TargetRef::Bone(i) => {
            let bone = &mut bones[i];
            match (p, v) {
                (Property::X, AnimValue::Number(v)) => bone.x = v,
                (Property::Y, AnimValue::Number(v)) => bone.y = v,
                (Property::Rotation, AnimValue::Number(v)) => bone.rotation = v,
                _ => unreachable!("validated bone property"),
            }
        }
        TargetRef::Node(i) => set_property(&mut nodes[i], p, v),
    }
}
pub(crate) fn set_property(n: &mut Node, p: Property, v: AnimValue) {
    match (p, v) {
        (Property::MotionProgress, AnimValue::Number(v)) => {
            if let Some(binding) = &mut n.motion_path {
                binding.progress = v;
            }
        }
        (Property::X, AnimValue::Number(v)) => n.transform.x = v,
        (Property::Y, AnimValue::Number(v)) => n.transform.y = v,
        (Property::Rotation, AnimValue::Number(v)) => n.transform.rotation = v,
        (Property::ScaleX, AnimValue::Number(v)) => n.transform.scale_x = v,
        (Property::ScaleY, AnimValue::Number(v)) => n.transform.scale_y = v,
        (Property::Opacity, AnimValue::Number(v)) => n.style.opacity = v,
        (Property::Fill, AnimValue::Color(v)) => n.style.fill = v,
        (Property::Stroke, AnimValue::Color(v)) => n.style.stroke = Some(v),
        (Property::Width, AnimValue::Number(v)) => {
            if let NodeKind::Rect { width, .. } = &mut n.kind {
                *width = v;
            }
        }
        (Property::Height, AnimValue::Number(v)) => {
            if let NodeKind::Rect { height, .. } = &mut n.kind {
                *height = v;
            }
        }
        (Property::Radius, AnimValue::Number(v)) => {
            if let NodeKind::Circle { radius, .. } = &mut n.kind {
                *radius = v;
            }
        }
        (Property::FontSize, AnimValue::Number(v)) => {
            if let NodeKind::Text { font_size, .. } = &mut n.kind {
                *font_size = v;
            }
        }
        (Property::Gradient, AnimValue::Gradient(g)) => n.gradient = Some(g),
        (Property::D, AnimValue::Path(path)) => {
            if let NodeKind::Path { d, bounds, .. } = &mut n.kind {
                // Keep selection/hit bounds in step with the morphing outline.
                if let Ok(b) = crate::path_bounds(&path) {
                    *bounds = b;
                }
                *d = path;
            }
        }
        _ => unreachable!("validated property type"),
    }
}

/// The pre-optimization evaluation path, kept verbatim (validate on every
/// evaluation, cloned lists, linear ID scans, separate node and bone passes)
/// as the oracle for the parity tests.
#[cfg(test)]
mod reference {
    use super::{clip_time, sample_track, set_property, valid_time};
    use crate::*;
    use std::collections::BTreeMap;

    impl Project {
        pub(super) fn evaluate_reference(
            &self,
            time: f64,
            context: &EvaluationContext,
        ) -> Result<(Vec<Node>, Vec<BonePose>), DocumentError> {
            let (mut nodes, bones) = self.evaluate_local_reference(time, context)?;
            let poses = crate::bones::solve_bones(&bones)?;
            crate::bones::attach_nodes(&mut nodes, &poses)?;
            crate::motion::apply(&mut nodes)?;
            let (_, bones) = self.evaluate_local_reference(time, context)?;
            Ok((nodes, crate::bones::solve_bones(&bones)?))
        }
        fn evaluate_local_reference(
            &self,
            time: f64,
            context: &EvaluationContext,
        ) -> Result<(Vec<Node>, Vec<Bone>), DocumentError> {
            self.validate()?;
            valid_time(time)?;
            for (machine, state) in &context.states {
                if !self
                    .state_machines
                    .iter()
                    .any(|m| &m.id == machine && m.states.iter().any(|s| &s.id == state))
                {
                    return Err(DocumentError(format!(
                        "unknown machine/state {machine}/{state}"
                    )));
                }
            }
            for (id, weight) in &context.clip_weights {
                if !self.clips.iter().any(|c| &c.id == id) {
                    return Err(DocumentError(format!("unknown clip {id}")));
                }
                crate::validation::validate_clip_weight(*weight)?;
            }
            let mut nodes = self.nodes.clone();
            let mut bones = self.bones.clone();
            for m in &self.state_machines {
                let state = context.states.get(&m.id).unwrap_or(&m.initial);
                let current = &m.states.iter().find(|s| &s.id == state).unwrap().overrides;
                let fade = context.state_fades.get(&m.id).and_then(|fade| {
                    let from = m.states.iter().find(|s| s.id == fade.from)?;
                    (fade.progress < 1.0).then_some((from, fade.progress))
                });
                let Some((from, progress)) = fade else {
                    for o in current {
                        apply_override(&mut nodes, &mut bones, o);
                    }
                    continue;
                };
                // Crossfade every property either state sets; a property only one
                // state sets fades between that value and the value underneath.
                let t = crate::follow::ease_fade(progress);
                let mut touched: Vec<(&str, Property)> = Vec::new();
                for o in from.overrides.iter().chain(current) {
                    if !touched.contains(&(o.node_id.as_str(), o.property)) {
                        touched.push((&o.node_id, o.property));
                    }
                }
                for (id, property) in touched {
                    let under = get_target(&nodes, &bones, id, property);
                    let pick = |list: &[PropertyOverride]| {
                        list.iter()
                            .rev()
                            .find(|o| o.node_id == id && o.property == property)
                            .map_or_else(|| under.clone(), |o| o.value.clone())
                    };
                    let value =
                        crate::follow::blend_values(&pick(&from.overrides), &pick(current), t);
                    set_target(&mut nodes, &mut bones, id, property, value);
                }
            }
            let mut selected = Vec::new();
            if let Some(clips) = &context.clips {
                for playback in clips {
                    valid_time(playback.time)?;
                    let clip = self
                        .clips
                        .iter()
                        .find(|c| c.id == playback.clip_id)
                        .ok_or_else(|| {
                            DocumentError(format!("unknown clip {}", playback.clip_id))
                        })?;
                    selected.push((clip, playback.time));
                }
            } else {
                selected.extend(self.clips.iter().filter(|c| c.autoplay).map(|c| (c, time)));
            }
            let space_weights = self.blend_space_weights(&context.variables)?;
            let mut group = Vec::new();
            for (clip, sample_time) in selected {
                if clip.blend_mode == BlendMode::Weighted {
                    let weight = context
                        .clip_weights
                        .get(&clip.id)
                        .copied()
                        .unwrap_or(clip.weight);
                    group.push((
                        clip,
                        sample_time,
                        weight * space_weights.get(&clip.id).copied().unwrap_or(1.0),
                    ));
                } else {
                    apply_weighted_group(&mut nodes, &mut bones, &group);
                    group.clear();
                    if clip.blend_mode == BlendMode::Additive {
                        let weight = context
                            .clip_weights
                            .get(&clip.id)
                            .copied()
                            .unwrap_or(clip.weight);
                        apply_additive(
                            &mut nodes,
                            &mut bones,
                            &self.nodes,
                            &self.bones,
                            clip,
                            sample_time,
                            weight,
                        );
                    } else {
                        apply_clip(&mut nodes, &mut bones, clip, sample_time);
                    }
                }
            }
            apply_weighted_group(&mut nodes, &mut bones, &group);
            for o in &context.overrides {
                self.validate_property(&o.node_id, o.property, &o.value)?;
                apply_override(&mut nodes, &mut bones, o);
            }
            for (machine, fade) in &context.state_fades {
                let known = self
                    .state_machines
                    .iter()
                    .any(|m| &m.id == machine && m.states.iter().any(|s| s.id == fade.from));
                if !known || !fade.progress.is_finite() {
                    return Err(DocumentError(format!("invalid state fade for {machine}")));
                }
            }
            for (id, value) in &context.follow {
                if !self.followers.iter().any(|f| &f.id == id)
                    || !value.iter().all(|v| v.is_finite())
                {
                    return Err(DocumentError(format!("invalid follower value for {id}")));
                }
            }
            self.apply_followers_reference(&mut nodes, &mut bones, &context.follow);
            Ok((nodes, bones))
        }

        fn apply_followers_reference(
            &self,
            nodes: &mut [Node],
            bones: &mut [Bone],
            values: &BTreeMap<String, [f64; 2]>,
        ) {
            for follower in &self.followers {
                let Some([a, b]) = values.get(&follower.id).copied() else {
                    continue;
                };
                if let Some(node) = nodes.iter_mut().find(|n| n.id == follower.target) {
                    match follower.mode {
                        FollowMode::Offset => {
                            node.transform.x += a;
                            node.transform.y += b;
                        }
                        FollowMode::Rotate => node.transform.rotation += a,
                    }
                } else if let Some(bone) = bones.iter_mut().find(|n| n.id == follower.target) {
                    match follower.mode {
                        FollowMode::Offset => {
                            bone.x += a;
                            bone.y += b;
                        }
                        FollowMode::Rotate => bone.rotation += a,
                    }
                }
            }
        }
    }

    fn apply_clip(nodes: &mut [Node], bones: &mut [Bone], clip: &Clip, time: f64) {
        let time = clip_time(clip, time);
        for track in &clip.tracks {
            set_target(
                nodes,
                bones,
                &track.node_id,
                track.property,
                sample_track(track, time),
            );
        }
    }

    // Reference is the immutable document pose, not a state pose or first keyframe.
    fn apply_additive(
        nodes: &mut [Node],
        bones: &mut [Bone],
        rest_nodes: &[Node],
        rest_bones: &[Bone],
        clip: &Clip,
        time: f64,
        weight: f64,
    ) {
        if weight == 0.0 {
            return;
        }
        for track in &clip.tracks {
            let baseline = get_target(nodes, bones, &track.node_id, track.property);
            let reference = get_target(rest_nodes, rest_bones, &track.node_id, track.property);
            let sample = sample_track(track, clip_time(clip, time));
            let mixed: Vec<f64> = baseline
                .numbers()
                .iter()
                .zip(sample.numbers())
                .zip(reference.numbers())
                .map(|((b, s), r)| b + weight * (s - r))
                .collect();
            let mut value = baseline.with_numbers(&mixed);
            match &mut value {
                AnimValue::Color(c) => {
                    c.r = c.r.clamp(0., 1.);
                    c.g = c.g.clamp(0., 1.);
                    c.b = c.b.clamp(0., 1.);
                    c.a = c.a.clamp(0., 1.);
                }
                AnimValue::Number(v) => match track.property {
                    Property::Opacity | Property::MotionProgress => *v = v.clamp(0., 1.),
                    Property::Width | Property::Height | Property::Radius | Property::FontSize => {
                        *v = v.max(0.)
                    }
                    _ => {}
                },
                _ => unreachable!("validated additive value"),
            }
            set_target(nodes, bones, &track.node_id, track.property, value);
        }
    }

    // Collect only contributing tracks: sparse channels have their own weight sum.
    fn apply_weighted_group(nodes: &mut [Node], bones: &mut [Bone], group: &[(&Clip, f64, f64)]) {
        type WeightedChannel<'a> = (&'a str, Property, Vec<(AnimValue, f64)>);
        let mut channels: Vec<WeightedChannel<'_>> = Vec::new();
        for &(clip, time, weight) in group {
            if weight == 0.0 {
                continue;
            }
            let time = clip_time(clip, time);
            for track in &clip.tracks {
                let sample = (sample_track(track, time), weight);
                if let Some((_, _, samples)) = channels
                    .iter_mut()
                    .find(|(id, property, _)| *id == track.node_id && *property == track.property)
                {
                    samples.push(sample);
                } else {
                    channels.push((&track.node_id, track.property, vec![sample]));
                }
            }
        }
        for (id, property, samples) in channels {
            let baseline = get_target(nodes, bones, id, property);
            let sum: f64 = samples.iter().map(|(_, weight)| weight).sum();
            let divisor = sum.max(1.0);
            let residual = (1.0 - sum).max(0.0);
            // Number-by-number mix when every sample matches the baseline's shape;
            // otherwise the heaviest contribution wins (structured values can't blend).
            let value = if samples.iter().all(|(v, _)| v.compatible(&baseline)) {
                let base = baseline.numbers();
                let mut mixed: Vec<f64> = base.iter().map(|b| b * residual).collect();
                for (sample, weight) in &samples {
                    for (m, v) in mixed.iter_mut().zip(sample.numbers()) {
                        *m += v * (weight / divisor);
                    }
                }
                baseline.with_numbers(&mixed)
            } else {
                let (heaviest, weight) = samples
                    .iter()
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .cloned()
                    .unwrap();
                if weight >= residual {
                    heaviest
                } else {
                    baseline
                }
            };
            set_target(nodes, bones, id, property, value);
        }
    }
    fn get_target(nodes: &[Node], bones: &[Bone], id: &str, property: Property) -> AnimValue {
        if let Some(bone) = bones.iter().find(|b| b.id == id) {
            return AnimValue::Number(match property {
                Property::X => bone.x,
                Property::Y => bone.y,
                Property::Rotation => bone.rotation,
                _ => unreachable!("validated bone property"),
            });
        }
        let node = nodes.iter().find(|n| n.id == id).unwrap();
        AnimValue::Number(match property {
            Property::MotionProgress => {
                node.motion_path
                    .as_ref()
                    .expect("validated motion binding")
                    .progress
            }
            Property::X => node.transform.x,
            Property::Y => node.transform.y,
            Property::Rotation => node.transform.rotation,
            Property::ScaleX => node.transform.scale_x,
            Property::ScaleY => node.transform.scale_y,
            Property::Opacity => node.style.opacity,
            Property::Fill => return AnimValue::Color(node.style.fill),
            Property::Stroke => {
                return AnimValue::Color(node.style.stroke.unwrap_or(Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                }))
            }
            Property::Width => match node.kind {
                NodeKind::Rect { width, .. } => width,
                _ => unreachable!(),
            },
            Property::Height => match node.kind {
                NodeKind::Rect { height, .. } => height,
                _ => unreachable!(),
            },
            Property::Radius => match node.kind {
                NodeKind::Circle { radius, .. } => radius,
                _ => unreachable!(),
            },
            Property::FontSize => match node.kind {
                NodeKind::Text { font_size, .. } => font_size,
                _ => unreachable!(),
            },
            Property::Gradient => {
                return AnimValue::Gradient(node.gradient.clone().expect("validated gradient node"))
            }
            Property::D => match &node.kind {
                NodeKind::Path { d, .. } => return AnimValue::Path(d.clone()),
                _ => unreachable!(),
            },
        })
    }
    fn apply_override(nodes: &mut [Node], bones: &mut [Bone], o: &PropertyOverride) {
        set_target(nodes, bones, &o.node_id, o.property, o.value.clone());
    }
    fn set_target(nodes: &mut [Node], bones: &mut [Bone], id: &str, p: Property, v: AnimValue) {
        if let Some(bone) = bones.iter_mut().find(|b| b.id == id) {
            match (p, v) {
                (Property::X, AnimValue::Number(v)) => bone.x = v,
                (Property::Y, AnimValue::Number(v)) => bone.y = v,
                (Property::Rotation, AnimValue::Number(v)) => bone.rotation = v,
                _ => unreachable!("validated bone property"),
            }
        } else {
            set_property(nodes.iter_mut().find(|n| n.id == id).unwrap(), p, v);
        }
    }
}

#[cfg(test)]
mod parity {
    use super::*;

    fn show<T: std::fmt::Debug, E: std::fmt::Debug>(r: &Result<T, E>) -> String {
        format!("{r:?}")
    }

    fn contexts(p: &Project, time: f64) -> Vec<EvaluationContext> {
        let mut list = vec![EvaluationContext::default()];
        let playbacks: Vec<ClipPlayback> = p
            .clips
            .iter()
            .enumerate()
            .map(|(i, c)| ClipPlayback {
                clip_id: c.id.clone(),
                time: time * (i as f64 + 1.0) * 0.37,
            })
            .collect();
        let mut with_clips = EvaluationContext {
            clips: Some(playbacks.clone()),
            clip_weights: p
                .clips
                .iter()
                .enumerate()
                .map(|(i, c)| (c.id.clone(), [0.3, 0.7, 1.0][i % 3]))
                .collect(),
            ..Default::default()
        };
        // Overrides on valid and invalid targets (errors must match too).
        let first_node = p.nodes.first().map(|n| n.id.clone());
        let first_bone = p.bones.first().map(|b| b.id.clone());
        let mut overrides = vec![];
        if let Some(id) = &first_node {
            overrides.push(PropertyOverride {
                node_id: id.clone(),
                property: Property::X,
                value: AnimValue::Number(5.0),
            });
        }
        if let Some(id) = &first_bone {
            overrides.push(PropertyOverride {
                node_id: id.clone(),
                property: Property::Rotation,
                value: AnimValue::Number(20.0),
            });
        }
        with_clips.overrides = overrides.clone();
        list.push(with_clips.clone());
        let mut reversed = with_clips.clone();
        reversed.clips = Some(playbacks.into_iter().rev().collect());
        list.push(reversed);
        list.push(EvaluationContext {
            clips: Some(vec![]),
            overrides: overrides.clone(),
            ..Default::default()
        });
        let mut bad = overrides;
        bad.push(PropertyOverride {
            node_id: "missing".into(),
            property: Property::X,
            value: AnimValue::Number(1.0),
        });
        list.push(EvaluationContext {
            overrides: bad,
            ..Default::default()
        });
        if let Some(id) = &first_bone {
            list.push(EvaluationContext {
                overrides: vec![PropertyOverride {
                    node_id: id.clone(),
                    property: Property::X,
                    value: AnimValue::Number(1e31),
                }],
                ..Default::default()
            });
        }
        // Machines in a non-initial state, crossfading from another one.
        let mut machines = with_clips.clone();
        for m in p.state_machines.iter().filter(|m| m.states.len() > 1) {
            machines.states.insert(m.id.clone(), m.states[1].id.clone());
            machines.state_fades.insert(
                m.id.clone(),
                StateFade {
                    from: m.states[0].id.clone(),
                    progress: 0.4,
                },
            );
        }
        list.push(machines);
        let mut follow = with_clips;
        for f in &p.followers {
            follow.follow.insert(f.id.clone(), [3.0, -2.0]);
        }
        list.push(follow);
        list.push(EvaluationContext {
            follow: [("nope".to_string(), [1.0, 1.0])].into(),
            ..Default::default()
        });
        list
    }

    #[test]
    fn indexed_evaluation_matches_the_reference_path_on_every_example() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");
        let (mut checked, mut succeeded) = (0, 0);
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if !path.to_string_lossy().ends_with(".splinely.json") {
                continue;
            }
            let project = Project::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let index = project.eval_index().unwrap();
            for time in [0.0, 0.05, 0.5, 1.3, 2.71, 9.99] {
                for (n, context) in contexts(&project, time).iter().enumerate() {
                    let expected = project.evaluate_reference(time, context);
                    let actual = project.evaluate_frame(&index, time, context);
                    assert_eq!(
                        show(&expected),
                        show(&actual),
                        "{} t={time} context #{n}",
                        path.display()
                    );
                    let nodes = project.evaluate_with_context(time, context);
                    assert_eq!(
                        show(&expected.as_ref().map(|r| &r.0)),
                        show(&nodes.as_ref()),
                        "{} t={time} public nodes #{n}",
                        path.display()
                    );
                    if expected.is_ok() {
                        let bones = project.evaluate_bones(time, context);
                        assert_eq!(
                            show(&expected.as_ref().map(|r| &r.1)),
                            show(&bones.as_ref())
                        );
                    }
                    checked += 1;
                    succeeded += usize::from(expected.is_ok());
                }
            }
        }
        assert!(checked > 100, "parity test must cover the bundled examples");
        assert!(
            succeeded * 2 > checked,
            "most contexts must evaluate, not fail early"
        );
    }

    #[test]
    fn an_index_from_another_project_is_rejected() {
        let mut project = Project::default();
        project.nodes.push(Node::circle("a", 0., 0., 1.));
        let index = project.eval_index().unwrap();
        project.nodes.push(Node::circle("b", 0., 0., 1.));
        let err = project
            .evaluate_frame(&index, 0.0, &EvaluationContext::default())
            .unwrap_err();
        assert!(err.0.contains("does not belong"));
    }
}
