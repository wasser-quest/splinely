use crate::accessibility::NodeA11y;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Project {
    pub schema_version: u32,
    pub name: String,
    pub width: f64,
    pub height: f64,
    pub nodes: Vec<Node>,
    pub bones: Vec<Bone>,
    pub clips: Vec<Clip>,
    /// Two numeric variables drive normalized weights of weighted clips.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blend_spaces: Vec<BlendSpace>,
    pub state_machines: Vec<StateMachine>,
    pub variables: BTreeMap<String, serde_json::Value>,
    pub audio_assets: Vec<AudioAsset>,
    pub audio_cues: Vec<AudioCue>,
    pub custom_js: String,
    pub raw_svg: String,
    /// Optional accessible description of the whole scene (web player / SVG
    /// `<desc>`); the accessible title is `name`. Omitted when empty.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Nested shape groups; nodes reference their direct group via `Node::group`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<Group>,
    /// Editor ruler guides (not exported as visible geometry).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<GuideLine>,
    /// Shapes or bones that follow the pointer, e.g. eyes or a turning head.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub followers: Vec<Follower>,
    /// Set by SVG import: fingerprints of the imported source text and of the
    /// resulting visual document, used to show when the two disagree later.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub svg_import: Option<SvgImportInfo>,
}

/// Import baseline (see `Project::source_sync`). Hashes are non-cryptographic
/// change detectors (`fnv1a64:<16 hex>`), not integrity or security proofs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SvgImportInfo {
    /// Hash of `Project::raw_svg` right after import.
    pub source_hash: String,
    /// Hash of the visual document (size, groups, nodes) right after import.
    pub document_hash: String,
}

/// How a follower reacts to the pointer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FollowMode {
    /// Move towards the pointer (eyes, pupils): adds to x/y.
    #[default]
    Offset,
    /// Turn towards the pointer (head, arm, turret): adds to rotation.
    Rotate,
}

/// A node or bone that follows the pointer. The result is added on top of
/// clips, states and explicit changes, so it blends with everything else.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Follower {
    pub id: String,
    /// Node or bone ID.
    pub target: String,
    #[serde(default)]
    pub mode: FollowMode,
    /// Offset: share of the pointer distance; rotate: share of the angle (0..1).
    #[serde(default = "full_strength")]
    pub strength: f64,
    /// Offset: maximum distance in SVG units; rotate: maximum degrees either way.
    pub max: f64,
    /// Seconds to settle on a new pointer position (0 = immediate).
    #[serde(default)]
    pub smoothing: f64,
    /// Rotate: the direction (degrees, 0 = right, 90 = down) the target faces at rest.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rest_angle: f64,
    /// Scene point to measure from; by default the target's center or bone root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<[f64; 2]>,
    /// Only follow while this machine is in one of the listed states.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<FollowCondition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowCondition {
    pub machine: String,
    pub states: Vec<String>,
}

fn full_strength() -> f64 {
    1.0
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// A (possibly nested) group of nodes and other groups.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// Enclosing group; none for a top-level group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Bones that move and turn the whole group, created the first time the
    /// group itself is animated (see `Project::rig_group`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rig: Option<GroupRig>,
}

/// Hidden bone chain that animates a group as one piece. At rest it changes
/// nothing. `move_bone` x/y are offsets from where the group was drawn;
/// `turn_bone` rotation turns the group around the centre of its bounds.
/// Every shape of the group is bound to `attach_bone`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupRig {
    pub move_bone: String,
    pub turn_bone: String,
    pub attach_bone: String,
}

/// A straight editor guide: vertical at `x = position` or horizontal at `y = position`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideLine {
    pub vertical: bool,
    pub position: f64,
}
impl Default for Project {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            name: "Untitled".into(),
            width: 800.0,
            height: 600.0,
            nodes: vec![],
            bones: vec![],
            clips: vec![],
            blend_spaces: vec![],
            state_machines: vec![],
            variables: BTreeMap::new(),
            audio_assets: vec![],
            audio_cues: vec![],
            custom_js: String::new(),
            raw_svg: String::new(),
            description: String::new(),
            groups: vec![],
            guides: vec![],
            followers: vec![],
            svg_import: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub bone_id: Option<String>,
    #[serde(default = "default_visible")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub original_kind: Option<Box<NodeKind>>,
    /// ID of the node's direct group in `Project::groups` (none = top level).
    /// Groups are exported as nested `<g>` elements and never change rendering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Optional gradient fill. When present it replaces `style.fill` in exports
    /// and previews; fill colour animation then has no visible effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<Gradient>,
    /// Move the local origin along an unbound path in scene coordinates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion_path: Option<MotionPath>,
    /// Optional screen-reader semantics for the web player and SVG export
    /// (see `accessibility.rs`); omitted when empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a11y: Option<NodeA11y>,
    #[serde(default)]
    pub transform: Transform,
    #[serde(default)]
    pub style: Style,
    pub kind: NodeKind,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionPath {
    pub path: String,
    #[serde(default)]
    pub progress: f64,
    #[serde(default)]
    pub orient: bool,
}

fn default_visible() -> bool {
    true
}
impl Node {
    pub fn rect(id: impl Into<String>, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self::new(
            id,
            NodeKind::Rect {
                x,
                y,
                width,
                height,
                rx: 0.0,
            },
        )
    }
    pub fn circle(id: impl Into<String>, cx: f64, cy: f64, radius: f64) -> Self {
        Self::new(id, NodeKind::Circle { cx, cy, radius })
    }
    pub fn text(id: impl Into<String>, x: f64, y: f64, text: impl Into<String>) -> Self {
        Self::new(
            id,
            NodeKind::Text {
                x,
                y,
                text: text.into(),
                font_size: 16.0,
            },
        )
    }
    pub fn new(id: impl Into<String>, kind: NodeKind) -> Self {
        Self {
            id: id.into(),
            name: String::new(),
            bone_id: None,
            visible: true,
            locked: false,
            original_kind: None,
            group: None,
            gradient: None,
            motion_path: None,
            a11y: None,
            transform: Transform::default(),
            style: Style::default(),
            kind,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NodeKind {
    /// SVG paths imported with absolute geometry. Bounds support selection, not clipping.
    Path {
        d: String,
        bounds: [f64; 4],
        #[serde(default)]
        evenodd: bool,
    },
    Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        #[serde(default)]
        rx: f64,
    },
    Circle {
        cx: f64,
        cy: f64,
        radius: f64,
    },
    Text {
        x: f64,
        y: f64,
        text: String,
        font_size: f64,
    },
}
/// Local rig geometry: children are offset from their parent's tip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bone {
    pub id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub length: f64,
}

/// Evaluated world-space bone root, rotation in degrees, and unchanged length.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BonePose {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub length: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Transform {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub scale_x: f64,
    pub scale_y: f64,
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Style {
    pub fill: Color,
    pub stroke: Option<Color>,
    pub stroke_width: f64,
    pub opacity: f64,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            fill: Color::rgb(80, 140, 240),
            stroke: None,
            stroke_width: 1.0,
            opacity: 1.0,
        }
    }
}
/// Linear or radial gradient in the shape's bounding box (0..1 units).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gradient {
    /// Radial (centre outwards) instead of linear.
    #[serde(default)]
    pub radial: bool,
    /// Linear direction in degrees: 0 = left to right, 90 = top to bottom.
    #[serde(default)]
    pub angle: f64,
    /// 2..16 stops with nondecreasing offsets in 0..1.
    pub stops: Vec<GradientStop>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    pub offset: f64,
    pub color: Color,
}
impl Gradient {
    /// Two-stop linear gradient.
    pub fn linear(angle: f64, from: Color, to: Color) -> Self {
        Self {
            radial: false,
            angle,
            stops: vec![
                GradientStop {
                    offset: 0.,
                    color: from,
                },
                GradientStop {
                    offset: 1.,
                    color: to,
                },
            ],
        }
    }
    /// SVG gradient element with the given (already unique, valid) ID.
    pub fn svg_element(&self, id: &str) -> String {
        let mut out = if self.radial {
            format!("<radialGradient id=\"{id}\" cx=\"0.5\" cy=\"0.5\" r=\"0.5\">")
        } else {
            let (sin, cos) = self.angle.to_radians().sin_cos();
            // Round away floating noise such as cos(90°) = 6e-17.
            let r = |v: f64| (v * 1e6).round() / 1e6 + 0.;
            let (cos, sin) = (r(cos), r(sin));
            format!(
                "<linearGradient id=\"{id}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">",
                0.5 - cos / 2.,
                0.5 - sin / 2.,
                0.5 + cos / 2.,
                0.5 + sin / 2.
            )
        };
        for stop in &self.stops {
            let c = stop.color;
            out.push_str(&format!(
                "<stop offset=\"{}\" stop-color=\"rgb({}% {}% {}%)\" stop-opacity=\"{}\"/>",
                stop.offset,
                c.r * 100.,
                c.g * 100.,
                c.b * 100.,
                c.a
            ));
        }
        out.push_str(if self.radial {
            "</radialGradient>"
        } else {
            "</linearGradient>"
        });
        out
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}
impl Color {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f64 / 255.0,
            g: g as f64 / 255.0,
            b: b as f64 / 255.0,
            a: 1.0,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlendMode {
    #[default]
    Override,
    Weighted,
    /// Adds weight * (sample - immutable document rest pose).
    Additive,
}
/// Inverse-distance-squared blend space. Samples drive selected clips only.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlendSpace {
    pub id: String,
    pub parameters: [String; 2],
    pub samples: Vec<BlendSpaceSample>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlendSpaceSample {
    pub clip_id: String,
    pub position: [f64; 2],
}
fn default_clip_weight() -> f64 {
    1.0
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    /// Cues scheduled on this clip's local clock by the browser runtime.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio_cues: Vec<AudioCue>,
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub duration: f64,
    #[serde(default)]
    pub looping: bool,
    #[serde(default)]
    pub autoplay: bool,
    #[serde(default)]
    pub blend_mode: BlendMode,
    #[serde(default = "default_clip_weight")]
    pub weight: f64,
    #[serde(default)]
    pub tracks: Vec<Track>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub node_id: String,
    pub property: Property,
    pub keyframes: Vec<Keyframe>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub time: f64,
    pub value: AnimValue,
    #[serde(default)]
    pub easing: Easing,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum AnimValue {
    Number(f64),
    Color(Color),
    /// Whole gradient; keyframes of one track share stop count and kind.
    Gradient(Gradient),
    /// Path data; keyframes of one track share the same command structure
    /// (see `morph_paths`) and are interpolated number by number.
    Path(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Property {
    /// Normalized arc-length position on the node's motion path.
    MotionProgress,
    X,
    Y,
    Rotation,
    ScaleX,
    ScaleY,
    Opacity,
    Fill,
    Stroke,
    Width,
    Height,
    Radius,
    FontSize,
    /// Gradient fill (node must have a gradient with matching kind/stop count).
    Gradient,
    /// Path data of a path node (morphing between compatible shapes).
    D,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Easing {
    #[default]
    Linear,
    Step,
    CubicBezier {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateMachine {
    pub id: String,
    pub initial: String,
    pub states: Vec<State>,
    #[serde(default)]
    pub transitions: Vec<Transition>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub id: String,
    #[serde(default)]
    pub on_enter: Vec<Action>,
    #[serde(default)]
    pub overrides: Vec<PropertyOverride>,
    /// Actions run when a transition leaves this state (before its own actions).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_exit: Vec<Action>,
    /// Text announced politely to screen readers (web-player live region)
    /// when a transition enters this state. Browser-only; omitted when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub announce: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyOverride {
    pub node_id: String,
    pub property: Property,
    pub value: AnimValue,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub from: String,
    pub to: String,
    /// The triggering event; timer transitions (`after`) use the name "after".
    #[serde(default = "timer_event")]
    pub event: Event,
    #[serde(default)]
    pub guard: Option<Guard>,
    #[serde(default)]
    pub actions: Vec<Action>,
    /// Seconds over which the previous state's overrides fade into the new
    /// state's (0 = switch immediately).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub blend: f64,
    /// Fire automatically this many seconds after entering `from` instead of
    /// on an event (if the guard holds then).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<f64>,
}

/// Event name of timer transitions.
pub const TIMER_EVENT: &str = "after";

fn timer_event() -> Event {
    Event {
        name: TIMER_EVENT.into(),
        target: None,
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub name: String,
    #[serde(default)]
    pub target: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guard {
    pub variable: String,
    pub equals: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Action {
    SetProperty {
        node_id: String,
        property: Property,
        value: AnimValue,
    },
    PlayClip {
        clip_id: String,
    },
    StopClip {
        clip_id: String,
    },
    BlendClip {
        clip_id: String,
        weight: f64,
        #[serde(default)]
        duration: f64,
    },
    SetVariable {
        name: String,
        value: serde_json::Value,
    },
    Emit {
        event: Event,
    },
    PlayAudio {
        asset_id: String,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioAsset {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub data_base64: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioCue {
    pub id: String,
    pub asset_id: String,
    pub time: f64,
    pub offset: f64,
    pub duration: Option<f64>,
    pub volume: f64,
}
