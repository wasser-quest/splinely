use crate::*;
use std::collections::{HashMap, HashSet};

type Result = std::result::Result<(), DocumentError>;
fn require(ok: bool, message: impl Into<String>) -> Result {
    if ok {
        Ok(())
    } else {
        Err(DocumentError(message.into()))
    }
}
fn finite(values: &[f64], context: &str) -> Result {
    require(
        values.iter().all(|v| v.is_finite() && v.abs() <= 1e7),
        format!("{context}: numbers must be finite and within +/- 10000000"),
    )
}
fn nonnegative(v: f64, context: &str) -> Result {
    require(
        v.is_finite() && (0.0..=1e7).contains(&v),
        format!("{context}: expected finite number in 0..10000000"),
    )
}
fn unit(v: f64, context: &str) -> Result {
    require(
        v.is_finite() && (0.0..=1.0).contains(&v),
        format!("{context}: expected number in 0..1"),
    )
}
fn id(value: &str) -> Result {
    let mut chars = value.chars();
    require(
        matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')),
        format!("invalid ID: {value:?}"),
    )
}
fn unique<'a>(ids: impl Iterator<Item = &'a str>, context: &str) -> Result {
    let mut seen = HashSet::new();
    for value in ids {
        id(value)?;
        require(
            seen.insert(value),
            format!("{context}: duplicate ID {value}"),
        )?;
    }
    Ok(())
}
fn color(c: Color) -> Result {
    for v in [c.r, c.g, c.b, c.a] {
        unit(v, "color")?;
    }
    Ok(())
}
fn scalar(v: &serde_json::Value) -> bool {
    v.is_null() || v.is_boolean() || v.is_number() || v.is_string()
}
fn same_type(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    (a.is_null() && b.is_null())
        || (a.is_boolean() && b.is_boolean())
        || (a.is_number() && b.is_number())
        || (a.is_string() && b.is_string())
}

/// ID lookups for one `validate` pass. IDs are unique across the checked
/// collections (`unique` runs first), so first-match scans and map lookups agree.
struct Index<'a> {
    nodes: HashMap<&'a str, &'a Node>,
    bones: HashSet<&'a str>,
    clips: HashMap<&'a str, &'a Clip>,
    audio_assets: HashSet<&'a str>,
}
impl<'a> Index<'a> {
    fn new(project: &'a Project) -> Self {
        Self {
            nodes: project.nodes.iter().map(|n| (n.id.as_str(), n)).collect(),
            bones: project.bones.iter().map(|b| b.id.as_str()).collect(),
            clips: project.clips.iter().map(|c| (c.id.as_str(), c)).collect(),
            audio_assets: project.audio_assets.iter().map(|a| a.id.as_str()).collect(),
        }
    }
}

impl Project {
    pub fn validate(&self) -> Result {
        require(
            self.schema_version == SCHEMA_VERSION,
            format!("unsupported schema version {}", self.schema_version),
        )?;
        require(
            self.width.is_finite()
                && self.width > 0.0
                && self.width <= 1e30
                && self.height.is_finite()
                && self.height > 0.0
                && self.height <= 1e30,
            "canvas dimensions must be positive, finite and <= 1e30",
        )?;
        unique(
            self.nodes
                .iter()
                .map(|n| n.id.as_str())
                .chain(self.bones.iter().map(|n| n.id.as_str()))
                .chain(self.clips.iter().map(|n| n.id.as_str()))
                .chain(self.blend_spaces.iter().map(|n| n.id.as_str()))
                .chain(self.state_machines.iter().map(|n| n.id.as_str()))
                .chain(self.audio_assets.iter().map(|n| n.id.as_str()))
                .chain(self.audio_cues.iter().map(|n| n.id.as_str()))
                .chain(
                    self.clips
                        .iter()
                        .flat_map(|c| &c.audio_cues)
                        .map(|n| n.id.as_str()),
                )
                .chain(self.groups.iter().map(|g| g.id.as_str()))
                .chain(self.followers.iter().map(|f| f.id.as_str())),
            "project",
        )?;
        let index = Index::new(self);
        let groups: HashMap<&str, &Group> =
            self.groups.iter().map(|g| (g.id.as_str(), g)).collect();
        for g in &self.groups {
            require(
                g.name.len() <= 200,
                format!("group {}: name too long", g.id),
            )?;
            if let Some(rig) = &g.rig {
                let parent = |id: &str| {
                    self.bones
                        .iter()
                        .find(|b| b.id == id)
                        .map(|b| b.parent_id.as_deref())
                };
                require(
                    parent(&rig.move_bone).is_some()
                        && parent(&rig.turn_bone) == Some(Some(rig.move_bone.as_str()))
                        && parent(&rig.attach_bone) == Some(Some(rig.turn_bone.as_str())),
                    format!(
                        "group {}: rig needs bones {} → {} → {} chained in that order",
                        g.id, rig.move_bone, rig.turn_bone, rig.attach_bone
                    ),
                )?;
            }
            // Walk up: parents must exist, no cycles, bounded depth.
            let mut current = g;
            for depth in 0.. {
                require(
                    depth < 32,
                    format!("group {}: nesting deeper than 32 or cyclic", g.id),
                )?;
                match &current.parent_id {
                    None => break,
                    Some(parent) => {
                        current = groups.get(parent.as_str()).copied().ok_or_else(|| {
                            DocumentError(format!("group {}: missing parent {parent}", g.id))
                        })?;
                    }
                }
            }
        }
        require(
            !index.nodes.contains_key("splinely-project"),
            "splinely-project is a reserved node ID",
        )?;
        let poses = crate::bones::solve_bones(&self.bones)?;
        crate::bones::check_attachments(&self.nodes, &poses)?;
        crate::motion::validate(self, &index.nodes)?;
        for n in &self.nodes {
            if let Some(original) = &n.original_kind {
                require(
                    matches!(n.kind, NodeKind::Path { .. }),
                    "originalKind requires a path node",
                )?;
                crate::conversion::generated_path(original)?;
            }
            if let Some(group) = &n.group {
                require(
                    groups.contains_key(group.as_str()),
                    format!("node {}: missing group {group}", n.id),
                )?;
            }
            if let Some(g) = &n.gradient {
                gradient(g, &n.id)?;
            }
            let t = n.transform;
            finite(&[t.x, t.y, t.rotation, t.scale_x, t.scale_y], &n.id)?;
            color(n.style.fill)?;
            if let Some(c) = n.style.stroke {
                color(c)?;
            }
            unit(n.style.opacity, "opacity")?;
            nonnegative(n.style.stroke_width, "stroke width")?;
            match &n.kind {
                NodeKind::Path { d, bounds, .. } => {
                    finite(bounds, &n.id)?;
                    nonnegative(bounds[2], &n.id)?;
                    nonnegative(bounds[3], &n.id)?;
                    let geometry = crate::path_bounds(d)?;
                    require(
                        bounds
                            .iter()
                            .zip(geometry)
                            .all(|(a, b)| (a - b).abs() <= 1e-4 * a.abs().max(b.abs()).max(1.)),
                        "path bounds disagree with geometry",
                    )?;
                }
                NodeKind::Rect {
                    x,
                    y,
                    width,
                    height,
                    rx,
                } => {
                    finite(&[*x, *y], &n.id)?;
                    for v in [*width, *height, *rx] {
                        nonnegative(v, &n.id)?;
                    }
                }
                NodeKind::Circle { cx, cy, radius } => {
                    finite(&[*cx, *cy], &n.id)?;
                    nonnegative(*radius, &n.id)?;
                }
                NodeKind::Text {
                    x, y, font_size, ..
                } => {
                    finite(&[*x, *y], &n.id)?;
                    nonnegative(*font_size, &n.id)?;
                }
            }
        }
        require(self.guides.len() <= 1000, "at most 1000 guides")?;
        for g in &self.guides {
            finite(&[g.position], "guide")?;
            require(g.position.abs() <= 1e7, "guide position out of range")?;
        }
        if let Some(info) = &self.svg_import {
            for hash in [&info.source_hash, &info.document_hash] {
                require(
                    hash.strip_prefix("fnv1a64:")
                        .is_some_and(|h| h.len() == 16 && h.bytes().all(|b| b.is_ascii_hexdigit())),
                    "svgImport hashes must look like fnv1a64:<16 hex digits>",
                )?;
            }
        }
        self.validate_blend_spaces()?;
        for c in &self.clips {
            validate_clip_weight(c.weight)?;
            require(
                c.duration.is_finite() && c.duration > 0.0,
                format!("clip {}: duration must be positive", c.id),
            )?;
            let mut targets = HashSet::new();
            for t in &c.tracks {
                require(
                    c.blend_mode != BlendMode::Additive
                        || !matches!(t.property, Property::Gradient | Property::D),
                    "additive clips support numeric and RGBA tracks, not gradients or paths",
                )?;
                require(
                    targets.insert((&t.node_id, t.property)),
                    format!("clip {}: duplicate property track", c.id),
                )?;
                require(!t.keyframes.is_empty(), "track has no keyframes")?;
                require(
                    crate::values::track_compatible(t),
                    format!(
                        "clip {}: {:?} keyframes of {} must share kind, stop count or path structure",
                        c.id, t.property, t.node_id
                    ),
                )?;
                let target = if index.bones.contains(t.node_id.as_str()) {
                    Target::Bone
                } else {
                    Target::Node(
                        index
                            .nodes
                            .get(t.node_id.as_str())
                            .copied()
                            .ok_or_else(|| DocumentError(format!("missing node {}", t.node_id)))?,
                    )
                };
                let mut previous = -1.0;
                for k in &t.keyframes {
                    require(
                        k.time.is_finite()
                            && k.time >= 0.0
                            && k.time <= c.duration
                            && k.time > previous,
                        "keyframe times must be strictly increasing and within clip duration",
                    )?;
                    previous = k.time;
                    check_property(target, t.property, &k.value)?;
                    if let Easing::CubicBezier { x1, y1, x2, y2 } = k.easing {
                        unit(x1, "bezier x1")?;
                        unit(x2, "bezier x2")?;
                        // Bounded easing prevents invalid geometry/opacity overshoot.
                        unit(y1, "bezier y1")?;
                        unit(y2, "bezier y2")?;
                    }
                }
            }
        }
        for (name, v) in &self.variables {
            require(
                !name.is_empty() && scalar(v),
                "variables must have nonempty names and scalar values",
            )?;
        }
        for m in &self.state_machines {
            unique(m.states.iter().map(|s| s.id.as_str()), "states")?;
            let state_ids: HashSet<&str> = m.states.iter().map(|s| s.id.as_str()).collect();
            let exists = |s: &str| state_ids.contains(s);
            require(
                exists(&m.initial),
                format!("machine {}: missing initial state", m.id),
            )?;
            for s in &m.states {
                for o in &s.overrides {
                    self.validate_property_in(&index, &o.node_id, o.property, &o.value)?;
                }
                for a in s.on_enter.iter().chain(&s.on_exit) {
                    self.validate_action(&index, a)?;
                }
            }
            for t in &m.transitions {
                require(
                    exists(&t.from) && exists(&t.to),
                    "transition references missing state",
                )?;
                self.validate_event(&index, &t.event)?;
                if let Some(g) = &t.guard {
                    require(
                        self.variables
                            .get(&g.variable)
                            .is_some_and(|v| same_type(v, &g.equals)),
                        "guard references missing variable or mismatched type",
                    )?;
                }
                for a in &t.actions {
                    self.validate_action(&index, a)?;
                }
                require(
                    t.blend.is_finite() && (0.0..=60.0).contains(&t.blend),
                    "transition blend must be 0..60 seconds",
                )?;
                match t.after {
                    Some(after) => require(
                        after.is_finite() && (0.0..=3600.0).contains(&after) && t.event.name == TIMER_EVENT,
                        "timer transition: after must be 0..3600 seconds and the event named \"after\"",
                    )?,
                    None => require(
                        t.event.name != TIMER_EVENT,
                        "event \"after\" is reserved for timer transitions (set after)",
                    )?,
                }
            }
        }
        for f in &self.followers {
            let node = index.nodes.contains_key(f.target.as_str());
            require(
                node || index.bones.contains(f.target.as_str()),
                format!("follower {}: unknown node or bone {}", f.id, f.target),
            )?;
            unit(f.strength, "follower strength")?;
            let max_limit = match f.mode {
                FollowMode::Offset => 1e7,
                FollowMode::Rotate => 180.0,
            };
            require(
                f.max.is_finite() && (0.0..=max_limit).contains(&f.max),
                format!("follower {}: max must be 0..{max_limit}", f.id),
            )?;
            require(
                f.smoothing.is_finite() && (0.0..=10.0).contains(&f.smoothing),
                format!("follower {}: smoothing must be 0..10 seconds", f.id),
            )?;
            finite(&[f.rest_angle], "follower restAngle")?;
            if let Some(origin) = f.origin {
                finite(&origin, "follower origin")?;
            }
            if let Some(when) = &f.when {
                let machine = self.state_machines.iter().find(|m| m.id == when.machine);
                require(
                    machine.is_some_and(|m| {
                        !when.states.is_empty()
                            && when
                                .states
                                .iter()
                                .all(|s| m.states.iter().any(|st| &st.id == s))
                    }),
                    format!("follower {}: unknown machine or state in when", f.id),
                )?;
            }
        }
        for a in &self.audio_assets {
            require(
                a.mime_type.strip_prefix("audio/").is_some_and(|subtype| {
                    !subtype.is_empty()
                        && subtype
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(&b))
                }),
                "invalid audio MIME type (expected audio/subtype without parameters)",
            )?;
            require(
                valid_base64(&a.data_base64),
                format!("asset {}: invalid base64", a.id),
            )?;
        }
        for clip in &self.clips {
            for cue in &clip.audio_cues {
                require(
                    cue.time < clip.duration,
                    "clip audio cue must start before clip end",
                )?;
            }
        }
        for c in self
            .audio_cues
            .iter()
            .chain(self.clips.iter().flat_map(|c| &c.audio_cues))
        {
            require(
                index.audio_assets.contains(c.asset_id.as_str()),
                "audio cue references missing asset",
            )?;
            nonnegative(c.time, "audio cue time")?;
            nonnegative(c.offset, "audio cue offset")?;
            unit(c.volume, "audio volume")?;
            if let Some(d) = c.duration {
                require(
                    d.is_finite() && d > 0.0,
                    "audio cue duration must be positive",
                )?;
            }
        }
        self.validate_accessibility()
    }
    pub(crate) fn validate_property(&self, node_id: &str, p: Property, v: &AnimValue) -> Result {
        let target = if self.bones.iter().any(|b| b.id == node_id) {
            Target::Bone
        } else {
            Target::Node(
                self.nodes
                    .iter()
                    .find(|n| n.id == node_id)
                    .ok_or_else(|| DocumentError(format!("missing node {node_id}")))?,
            )
        };
        check_property(target, p, v)
    }
    fn validate_property_in(
        &self,
        index: &Index,
        node_id: &str,
        p: Property,
        v: &AnimValue,
    ) -> Result {
        let target = if index.bones.contains(node_id) {
            Target::Bone
        } else {
            Target::Node(
                index
                    .nodes
                    .get(node_id)
                    .copied()
                    .ok_or_else(|| DocumentError(format!("missing node {node_id}")))?,
            )
        };
        check_property(target, p, v)
    }
    fn validate_event(&self, index: &Index, e: &Event) -> Result {
        require(!e.name.is_empty(), "event name is empty")?;
        if let Some(target) = &e.target {
            if e.name == "clip.end" {
                require(
                    index.clips.contains_key(target.as_str()) || target == "@timeline",
                    "clip.end event references missing clip",
                )?;
            } else {
                require(
                    index.nodes.contains_key(target.as_str()),
                    "event references missing node",
                )?;
            }
        }
        Ok(())
    }
    fn validate_action(&self, index: &Index, a: &Action) -> Result {
        match a {
            Action::SetProperty {
                node_id,
                property,
                value,
            } => self.validate_property_in(index, node_id, *property, value),
            Action::PlayClip { clip_id } | Action::StopClip { clip_id } => require(
                index.clips.contains_key(clip_id.as_str()),
                "action references missing clip",
            ),
            Action::BlendClip {
                clip_id,
                weight,
                duration,
            } => {
                require(
                    index
                        .clips
                        .get(clip_id.as_str())
                        .is_some_and(|c| c.blend_mode != BlendMode::Override),
                    "blendClip requires an existing weighted or additive clip",
                )?;
                validate_clip_weight(*weight)?;
                require(
                    duration.is_finite() && *duration >= 0.0,
                    "blendClip duration must be nonnegative and finite",
                )
            }
            Action::SetVariable { name, value } => {
                require(
                    self.variables
                        .get(name)
                        .is_some_and(|v| same_type(v, value)),
                    "action references missing variable or mismatched type",
                )?;
                // `validate` already ran `validate_blend_spaces` on this project.
                self.blend_space_weights_unchecked(&[(name.clone(), value.clone())].into())?;
                Ok(())
            }
            Action::Emit { event } => self.validate_event(index, event),
            Action::PlayAudio { asset_id } => require(
                index.audio_assets.contains(asset_id.as_str()),
                "action references missing audio asset",
            ),
        }
    }
}

#[derive(Clone, Copy)]
enum Target<'a> {
    Bone,
    Node(&'a Node),
}

fn gradient(g: &Gradient, node: &str) -> Result {
    finite(&[g.angle], node)?;
    require(
        (2..=16).contains(&g.stops.len()),
        format!("node {node}: gradient needs 2..16 stops"),
    )?;
    let mut previous = 0.;
    for stop in &g.stops {
        unit(stop.offset, "gradient stop offset")?;
        require(
            stop.offset >= previous,
            format!("node {node}: gradient stop offsets must not decrease"),
        )?;
        previous = stop.offset;
        color(stop.color)?;
    }
    Ok(())
}

fn check_property(target: Target, p: Property, v: &AnimValue) -> Result {
    let n = match target {
        Target::Bone => {
            return match (p, v) {
                (Property::X | Property::Y | Property::Rotation, AnimValue::Number(v)) => {
                    finite(&[*v], "bone property")
                }
                _ => Err(DocumentError(
                    "bones support only numeric x, y and rotation".into(),
                )),
            };
        }
        Target::Node(n) => n,
    };
    match (p, v) {
        (Property::Fill | Property::Stroke, AnimValue::Color(c)) => color(*c)?,
        (Property::Gradient, AnimValue::Gradient(g)) => {
            gradient(g, &n.id)?;
            let base = n.gradient.as_ref().ok_or_else(|| {
                DocumentError(format!(
                    "node {}: gradient animation needs a gradient fill",
                    n.id
                ))
            })?;
            require(
                base.radial == g.radial && base.stops.len() == g.stops.len(),
                format!(
                    "node {}: animated gradient must keep the kind and stop count",
                    n.id
                ),
            )?;
        }
        (Property::D, AnimValue::Path(d)) => {
            require(
                matches!(n.kind, NodeKind::Path { .. }),
                "path data animation needs a path node",
            )?;
            crate::path_bounds(d)?;
        }
        (Property::Fill | Property::Stroke | Property::Gradient | Property::D, _)
        | (_, AnimValue::Color(_) | AnimValue::Gradient(_) | AnimValue::Path(_)) => {
            return Err(DocumentError("property value type mismatch".into()))
        }
        (Property::MotionProgress, AnimValue::Number(v)) => {
            require(
                n.motion_path.is_some(),
                "motionProgress requires a motionPath binding",
            )?;
            unit(*v, "motion progress")?;
        }
        (Property::Opacity, AnimValue::Number(v)) => unit(*v, "opacity")?,
        (
            Property::Width | Property::Height | Property::Radius | Property::FontSize,
            AnimValue::Number(v),
        ) => nonnegative(*v, "geometry value")?,
        (_, AnimValue::Number(v)) => finite(&[*v], "property")?,
    }
    require(
        match p {
            Property::Width | Property::Height => matches!(n.kind, NodeKind::Rect { .. }),
            Property::Radius => matches!(n.kind, NodeKind::Circle { .. }),
            Property::FontSize => matches!(n.kind, NodeKind::Text { .. }),
            _ => true,
        },
        "property is unsupported for node kind",
    )
}

pub(crate) fn validate_clip_weight(weight: f64) -> Result {
    unit(weight, "clip weight")
}

// Strict canonical RFC 4648 alphabet/padding, without another dependency.
fn valid_base64(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() & 3 != 0 {
        return false;
    }
    let padding = b.iter().rev().take_while(|&&c| c == b'=').count();
    if padding > 2 {
        return false;
    }
    let data = &b[..b.len() - padding];
    let digit = |c: u8| -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    if !data.iter().all(|&c| digit(c).is_some()) {
        return false;
    }
    match padding {
        1 => data.last().is_some_and(|&c| digit(c).unwrap() & 3 == 0),
        2 => data.last().is_some_and(|&c| digit(c).unwrap() & 15 == 0),
        _ => true,
    }
}
