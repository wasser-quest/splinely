use crate::*;
use std::collections::BTreeMap;

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
    /// Applied after all clip tracks.
    pub overrides: Vec<PropertyOverride>,
}
#[derive(Clone, Debug)]
pub struct ClipPlayback {
    pub clip_id: String,
    pub time: f64,
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
        let (mut nodes, bones) = self.evaluate_local(time, context)?;
        let poses = crate::bones::solve_bones(&bones)?;
        crate::bones::attach_nodes(&mut nodes, &poses)?;
        Ok(nodes)
    }
    pub fn evaluate_bones(
        &self,
        time: f64,
        context: &EvaluationContext,
    ) -> Result<Vec<BonePose>, DocumentError> {
        let (_, bones) = self.evaluate_local(time, context)?;
        crate::bones::solve_bones(&bones)
    }
    fn evaluate_local(
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
            for o in &m.states.iter().find(|s| &s.id == state).unwrap().overrides {
                apply_override(&mut nodes, &mut bones, o);
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
                    .ok_or_else(|| DocumentError(format!("unknown clip {}", playback.clip_id)))?;
                selected.push((clip, playback.time));
            }
        } else {
            selected.extend(self.clips.iter().filter(|c| c.autoplay).map(|c| (c, time)));
        }
        let mut group = Vec::new();
        for (clip, sample_time) in selected {
            if clip.blend_mode == BlendMode::Weighted {
                let weight = context
                    .clip_weights
                    .get(&clip.id)
                    .copied()
                    .unwrap_or(clip.weight);
                group.push((clip, sample_time, weight));
            } else {
                apply_weighted_group(&mut nodes, &mut bones, &group);
                group.clear();
                apply_clip(&mut nodes, &mut bones, clip, sample_time);
            }
        }
        apply_weighted_group(&mut nodes, &mut bones, &group);
        for o in &context.overrides {
            self.validate_property(&o.node_id, o.property, &o.value)?;
            apply_override(&mut nodes, &mut bones, o);
        }
        Ok((nodes, bones))
    }
}
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
pub(crate) fn set_property(n: &mut Node, p: Property, v: AnimValue) {
    match (p, v) {
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
