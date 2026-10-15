//! Shape authoring and arrangement commands: add/update/delete/duplicate/list
//! nodes, align/distribute/flip/group/order them, and render a PNG preview.
//! Arrangement uses the same `splinely_document` operations as the editor.
use crate::{bones::number, failure, load, save_project, Result};
use clap::{Subcommand, ValueEnum};
use serde_json::{json, Value};
use splinely_document::{
    AlignEdge, AlignTarget, Axis, BooleanOp, Color, Node, NodeKind, Project, Stack,
};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Kind {
    Rect,
    Circle,
    Text,
    Path,
    /// Regular polygon (path): --x/--y centre, --radius, --sides.
    Polygon,
    /// Star (path): --x/--y centre, --radius, --tips, --inner.
    Star,
    /// Stroked line (path): --x/--y to --x2/--y2.
    Line,
}

/// Style and transform options shared by `node add` and `node update`.
#[derive(clap::Args, Default)]
pub struct Appearance {
    /// Display name shown in the editor's layer list.
    #[arg(long)]
    name: Option<String>,
    /// Fill colour: #RRGGBB, #RRGGBBAA, CSS name (red) or rgb()/rgba().
    #[arg(long, alias = "color", value_parser = color)]
    fill: Option<Color>,
    /// Stroke colour (like --fill), or "none" to remove the stroke.
    #[arg(long, value_parser = stroke)]
    stroke: Option<StrokeArg>,
    #[arg(long, value_parser = number, allow_hyphen_values = true)]
    stroke_width: Option<f64>,
    /// Overall opacity 0..1.
    #[arg(long, value_parser = number, allow_hyphen_values = true)]
    opacity: Option<f64>,
    /// Transform translation (moves the whole shape).
    #[arg(long, value_parser = number, allow_hyphen_values = true)]
    tx: Option<f64>,
    #[arg(long, value_parser = number, allow_hyphen_values = true)]
    ty: Option<f64>,
    /// Rotation in degrees around the shape's local origin.
    #[arg(long, value_parser = number, allow_hyphen_values = true)]
    rotation: Option<f64>,
    #[arg(long, value_parser = number, allow_hyphen_values = true)]
    scale_x: Option<f64>,
    #[arg(long, value_parser = number, allow_hyphen_values = true)]
    scale_y: Option<f64>,
    /// ID of an existing group to move the node into; "" moves it to the top level.
    #[arg(long)]
    group: Option<String>,
    /// Gradient fill: "linear [ANGLE] C1 C2 …" or "radial C1 C2 …"; colours may
    /// carry an offset "gold@0.3" (else evenly spaced). "none" removes it.
    #[arg(long, value_parser = gradient, allow_hyphen_values = true)]
    gradient: Option<GradientArg>,
}

/// Parsed `--gradient`; `None` removes the gradient.
#[derive(Clone, Debug)]
pub struct GradientArg(Option<splinely_document::Gradient>);

/// Parse a gradient spec ("linear [ANGLE] C1 C2 …" or "radial C1 C2 …").
pub fn parse_gradient(text: &str) -> std::result::Result<splinely_document::Gradient, String> {
    gradient(text)?
        .0
        .ok_or_else(|| "A gradient keyframe cannot be none".to_string())
}

fn gradient(text: &str) -> std::result::Result<GradientArg, String> {
    let mut words = text.split_whitespace();
    let radial = match words.next() {
        Some("none") if text.trim() == "none" => return Ok(GradientArg(None)),
        Some("linear") => false,
        Some("radial") => true,
        _ => {
            return Err(
                "Expected \"linear [ANGLE] COLOURS…\", \"radial COLOURS…\" or \"none\"".into(),
            )
        }
    };
    let mut words: Vec<&str> = words.collect();
    let mut angle = 0.;
    if !radial {
        if let Some(first) = words.first() {
            if let Ok(value) = first.parse::<f64>() {
                angle = value;
                words.remove(0);
            }
        }
    }
    if !(2..=16).contains(&words.len()) {
        return Err("A gradient needs 2..16 colours".into());
    }
    let last = (words.len() - 1) as f64;
    let mut stops = vec![];
    for (i, word) in words.iter().enumerate() {
        let (colour, offset) = match word.rsplit_once('@') {
            Some((c, o)) => (
                c,
                o.parse::<f64>()
                    .map_err(|_| format!("Invalid offset in {word:?}"))?,
            ),
            None => (*word, i as f64 / last),
        };
        stops.push(splinely_document::GradientStop {
            offset,
            color: color(colour)?,
        });
    }
    Ok(GradientArg(Some(splinely_document::Gradient {
        radial,
        angle,
        stops,
    })))
}

/// Geometry options; which apply depends on the shape kind.
#[derive(clap::Args, Default)]
pub struct Geometry {
    /// Rect top-left, text left baseline, or circle/polygon/star centre (alias --cx).
    #[arg(long, alias = "cx", value_parser = number, allow_hyphen_values = true)]
    x: Option<f64>,
    #[arg(long, alias = "cy", value_parser = number, allow_hyphen_values = true)]
    y: Option<f64>,
    #[arg(long, alias = "w", value_parser = number, allow_hyphen_values = true)]
    width: Option<f64>,
    #[arg(long, alias = "h", value_parser = number, allow_hyphen_values = true)]
    height: Option<f64>,
    /// Rect corner radius.
    #[arg(long, value_parser = number, allow_hyphen_values = true)]
    rx: Option<f64>,
    #[arg(long, alias = "r", value_parser = number, allow_hyphen_values = true)]
    radius: Option<f64>,
    #[arg(long, allow_hyphen_values = true)]
    text: Option<String>,
    #[arg(long, alias = "size", value_parser = number, allow_hyphen_values = true)]
    font_size: Option<f64>,
    /// SVG path data (local coordinates); bounds are computed.
    #[arg(long, allow_hyphen_values = true)]
    d: Option<String>,
    /// Even-odd fill rule for paths.
    #[arg(long, action = clap::ArgAction::Set)]
    evenodd: Option<bool>,
    /// Polygon corners (3..64, default 6).
    #[arg(long)]
    sides: Option<u32>,
    /// Star tips (3..64, default 5).
    #[arg(long)]
    tips: Option<u32>,
    /// Star inner radius ratio (0..1, default 0.45).
    #[arg(long, value_parser = number)]
    inner: Option<f64>,
    /// Line end point.
    #[arg(long, value_parser = number, allow_hyphen_values = true)]
    x2: Option<f64>,
    #[arg(long, value_parser = number, allow_hyphen_values = true)]
    y2: Option<f64>,
}

#[derive(Subcommand)]
pub enum NodeCommand {
    /// Add a rect, circle, text or path on top; prints the new ID.
    Add {
        input: String,
        #[arg(long, value_enum)]
        kind: Kind,
        /// New globally unique ID (default: node-N).
        #[arg(long)]
        id: Option<String>,
        /// `center`: rotate/scale (and animate them) around the shape's centre instead of 0,0.
        #[arg(long, value_enum)]
        pivot: Option<Pivot>,
        #[command(flatten)]
        geometry: Geometry,
        #[command(flatten)]
        appearance: Appearance,
        #[arg(long)]
        output: String,
    },
    /// Change only the given geometry, style or transform fields of a node.
    Update {
        input: String,
        /// Node ID (or give it as the second positional argument).
        #[arg(long, alias = "id", required_unless_present = "node_id")]
        node: Option<String>,
        #[arg(value_name = "NODE", conflicts_with = "node")]
        node_id: Option<String>,
        /// `center`: move the local origin to the shape's centre (looks the same; rotation/scale then pivot there).
        #[arg(long, value_enum)]
        pivot: Option<Pivot>,
        #[command(flatten)]
        geometry: Geometry,
        #[command(flatten)]
        appearance: Appearance,
        #[arg(long)]
        output: String,
    },
    /// Delete nodes with their animation tracks and state references.
    Delete {
        input: String,
        /// Comma-separated node IDs.
        /// Comma-separated node IDs.
        #[arg(long, alias = "node", value_delimiter = ',', required = true)]
        nodes: Vec<String>,
        #[arg(long)]
        output: String,
    },
    /// Copy nodes (with their animation tracks), offset by --dx/--dy.
    Duplicate {
        input: String,
        /// Comma-separated node IDs.
        #[arg(long, alias = "node", value_delimiter = ',', required = true)]
        nodes: Vec<String>,
        #[arg(long, default_value_t = 10.0, value_parser = number, allow_hyphen_values = true)]
        dx: f64,
        #[arg(long, default_value_t = 10.0, value_parser = number, allow_hyphen_values = true)]
        dy: f64,
        #[arg(long)]
        output: String,
    },
    /// List nodes back to front with kind, group, flags and authored scene bounds.
    List { input: String },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Edge {
    Left,
    Hcenter,
    Right,
    Top,
    Vcenter,
    Bottom,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Direction {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Order {
    Front,
    Forward,
    Backward,
    Back,
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq)]
pub enum Relative {
    Selection,
    Document,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Boolean {
    Union,
    Difference,
    Intersection,
    Exclusion,
}

#[derive(Subcommand)]
pub enum ArrangeCommand {
    /// Scale nodes in scene space around a point (default: centre of their bounds).
    /// Non-uniform factors require shapes rotated by multiples of 90°.
    Scale {
        input: String,
        /// Comma-separated node IDs.
        #[arg(long, alias = "node", value_delimiter = ',', required = true)]
        nodes: Vec<String>,
        /// Uniform factor, or the x factor together with --sy.
        #[arg(long, value_parser = number, allow_hyphen_values = true)]
        sx: f64,
        #[arg(long, value_parser = number, allow_hyphen_values = true)]
        sy: Option<f64>,
        #[arg(long, value_parser = number, allow_hyphen_values = true, requires = "anchor_y")]
        anchor_x: Option<f64>,
        #[arg(long, value_parser = number, allow_hyphen_values = true, requires = "anchor_x")]
        anchor_y: Option<f64>,
        #[arg(long)]
        output: String,
    },
    /// Rotate nodes clockwise by degrees around a point (default: centre of their bounds).
    Rotate {
        input: String,
        /// Comma-separated node IDs.
        #[arg(long, alias = "node", value_delimiter = ',', required = true)]
        nodes: Vec<String>,
        #[arg(long, value_parser = number, allow_hyphen_values = true)]
        degrees: f64,
        #[arg(long, value_parser = number, allow_hyphen_values = true, requires = "pivot_y")]
        pivot_x: Option<f64>,
        #[arg(long, value_parser = number, allow_hyphen_values = true, requires = "pivot_x")]
        pivot_y: Option<f64>,
        #[arg(long)]
        output: String,
    },
    /// Morph SOURCE into TARGET's outline: SOURCE becomes a morph-ready path, a new
    /// autoplay clip holds both shapes, TARGET is hidden. Prints the clip ID.
    Morph {
        input: String,
        #[arg(long)]
        source: String,
        #[arg(long)]
        target: String,
        #[arg(long, default_value_t = 1.0, value_parser = number)]
        duration: f64,
        #[arg(long)]
        output: String,
    },
    /// Replace ≥2 shapes (no text) with one path: union, difference (bottom-most
    /// minus the others), intersection or exclusion. Curves stay curves.
    Boolean {
        input: String,
        /// Comma-separated node IDs.
        #[arg(long, alias = "node", value_delimiter = ',', required = true)]
        nodes: Vec<String>,
        #[arg(long, value_enum)]
        op: Boolean,
        #[arg(long)]
        output: String,
    },
    /// Align nodes to an edge/centre of their common bounds or the document.
    Align {
        input: String,
        /// Comma-separated node IDs.
        #[arg(long, alias = "node", value_delimiter = ',', required = true)]
        nodes: Vec<String>,
        #[arg(long, value_enum)]
        edge: Edge,
        /// A single node always aligns to the document.
        #[arg(long, value_enum, default_value = "selection")]
        to: Relative,
        #[arg(long)]
        output: String,
    },
    /// Make gaps between three or more nodes equal; outer nodes stay.
    Distribute {
        input: String,
        /// Comma-separated node IDs.
        #[arg(long, alias = "node", value_delimiter = ',', required = true)]
        nodes: Vec<String>,
        #[arg(long, value_enum)]
        axis: Direction,
        #[arg(long)]
        output: String,
    },
    /// Mirror nodes around the centre of their common bounds.
    Flip {
        input: String,
        /// Comma-separated node IDs.
        #[arg(long, alias = "node", value_delimiter = ',', required = true)]
        nodes: Vec<String>,
        #[arg(long, value_enum)]
        axis: Direction,
        #[arg(long)]
        output: String,
    },
    /// Move nodes by a scene-space offset (bone-bound nodes included).
    Move {
        input: String,
        /// Comma-separated node IDs.
        #[arg(long, alias = "node", value_delimiter = ',', required = true)]
        nodes: Vec<String>,
        #[arg(long, default_value_t = 0.0, value_parser = number, allow_hyphen_values = true)]
        dx: f64,
        #[arg(long, default_value_t = 0.0, value_parser = number, allow_hyphen_values = true)]
        dy: f64,
        #[arg(long)]
        output: String,
    },
    /// Group nodes (nested under their common group). Prints the new group ID.
    Group {
        input: String,
        /// Comma-separated node IDs.
        #[arg(long, alias = "node", value_delimiter = ',', required = true)]
        nodes: Vec<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        output: String,
    },
    /// Dissolve the outermost group of each node (one level).
    Ungroup {
        input: String,
        /// Comma-separated node IDs.
        #[arg(long, alias = "node", value_delimiter = ',', required = true)]
        nodes: Vec<String>,
        #[arg(long)]
        output: String,
    },
    /// Change stacking order: front, forward, backward, back.
    Order {
        input: String,
        /// Comma-separated node IDs.
        #[arg(long, alias = "node", value_delimiter = ',', required = true)]
        nodes: Vec<String>,
        #[arg(long, value_enum)]
        to: Order,
        #[arg(long)]
        output: String,
    },
}

/// Parsed `--stroke` value; `None` removes the stroke.
#[derive(Clone, Copy, Debug)]
pub struct StrokeArg(Option<Color>);

fn color(text: &str) -> std::result::Result<Color, String> {
    let parsed: svgtypes::Color = text.trim().parse().map_err(|_| {
        format!("Unknown colour {text:?}; use #RRGGBB, #RRGGBBAA, a CSS name or rgb()")
    })?;
    Ok(Color {
        r: parsed.red as f64 / 255.,
        g: parsed.green as f64 / 255.,
        b: parsed.blue as f64 / 255.,
        a: parsed.alpha as f64 / 255.,
    })
}

/// Colour parser shared with other commands.
pub fn parse_color(text: &str) -> std::result::Result<Color, String> {
    color(text)
}

fn stroke(text: &str) -> std::result::Result<StrokeArg, String> {
    if text.trim() == "none" {
        Ok(StrokeArg(None))
    } else {
        color(text).map(|c| StrokeArg(Some(c)))
    }
}

fn hex(c: &Color) -> String {
    let byte = |v: f64| (v.clamp(0., 1.) * 255.).round() as u8;
    if c.a >= 1. {
        format!("#{:02x}{:02x}{:02x}", byte(c.r), byte(c.g), byte(c.b))
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            byte(c.r),
            byte(c.g),
            byte(c.b),
            byte(c.a)
        )
    }
}

fn invalid(message: impl ToString) -> crate::Failure {
    failure("invalid_arguments", message)
}

/// Local centre of a node's geometry (text: its anchor point).
fn local_centre(kind: &NodeKind) -> (f64, f64) {
    match kind {
        NodeKind::Rect {
            x,
            y,
            width,
            height,
            ..
        } => (x + width / 2., y + height / 2.),
        NodeKind::Circle { cx, cy, .. } => (*cx, *cy),
        NodeKind::Text { x, y, .. } => (*x, *y),
        NodeKind::Path { bounds, .. } => (bounds[0] + bounds[2] / 2., bounds[1] + bounds[3] / 2.),
    }
}

/// Whether rotation/scale already happen around the shape's centre.
pub fn pivot_is_centred(node: &Node) -> bool {
    let (cx, cy) = local_centre(&node.kind);
    cx.abs() < 1e-6 && cy.abs() < 1e-6
}

/// Translate path data (arcs and shorthands become curves).
fn translate_d(d: &str, dx: f64, dy: f64) -> Result<String> {
    use std::fmt::Write;
    use svgtypes::SimplePathSegment as S;
    let n = crate::shapes::fmt_number;
    let mut out = String::new();
    for segment in svgtypes::SimplifyingPathParser::from(d) {
        let segment = segment.map_err(|e| invalid(format!("Cannot read path data: {e}")))?;
        let _ = match segment {
            S::MoveTo { x, y } => write!(out, "M{} {} ", n(x + dx), n(y + dy)),
            S::LineTo { x, y } => write!(out, "L{} {} ", n(x + dx), n(y + dy)),
            S::Quadratic { x1, y1, x, y } => write!(
                out,
                "Q{} {} {} {} ",
                n(x1 + dx),
                n(y1 + dy),
                n(x + dx),
                n(y + dy)
            ),
            S::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => write!(
                out,
                "C{} {} {} {} {} {} ",
                n(x1 + dx),
                n(y1 + dy),
                n(x2 + dx),
                n(y2 + dy),
                n(x + dx),
                n(y + dy)
            ),
            S::ClosePath => write!(out, "Z "),
        };
    }
    Ok(out.trim_end().to_owned())
}

pub fn fmt_number(v: f64) -> String {
    let r = (v * 1e4).round() / 1e4 + 0.;
    format!("{r}")
}

/// Move a node's local origin to its centre without changing how it looks,
/// so rotation and scale (also animated) happen around the centre.
fn centre_pivot(project: &mut Project, index: usize) -> Result<()> {
    let (cx, cy) = local_centre(&project.nodes[index].kind);
    if cx.abs() < 1e-9 && cy.abs() < 1e-9 {
        return Ok(());
    }
    let node = &mut project.nodes[index];
    match &mut node.kind {
        NodeKind::Rect { x, y, .. } | NodeKind::Text { x, y, .. } => {
            *x -= cx;
            *y -= cy;
        }
        NodeKind::Circle { cx: x, cy: y, .. } => {
            *x = 0.;
            *y = 0.;
        }
        NodeKind::Path { d, evenodd, .. } => {
            let moved = translate_d(d, -cx, -cy)?;
            node.kind = path_kind(moved, *evenodd)?;
        }
    }
    let t = &mut node.transform;
    let (s, c) = t.rotation.to_radians().sin_cos();
    let (lx, ly) = (cx * t.scale_x, cy * t.scale_y);
    let (ox, oy) = (lx * c - ly * s, lx * s + ly * c);
    t.x += ox;
    t.y += oy;
    // Keep animated positions and path keyframes where they were.
    let id = node.id.clone();
    for track in project
        .clips
        .iter_mut()
        .flat_map(|c| c.tracks.iter_mut())
        .filter(|t| t.node_id == id)
    {
        for key in &mut track.keyframes {
            match (&track.property, &mut key.value) {
                (splinely_document::Property::X, splinely_document::AnimValue::Number(v)) => {
                    *v += ox
                }
                (splinely_document::Property::Y, splinely_document::AnimValue::Number(v)) => {
                    *v += oy
                }
                (splinely_document::Property::D, splinely_document::AnimValue::Path(d)) => {
                    *d = translate_d(d, -cx, -cy)?
                }
                _ => {}
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Pivot {
    /// Rotate and scale around the centre of the shape.
    Center,
}

fn path_kind(d: String, evenodd: bool) -> Result<NodeKind> {
    let bounds = splinely_document::path_bounds(&d).map_err(|e| failure("validation_error", e))?;
    Ok(NodeKind::Path { d, bounds, evenodd })
}

fn apply_geometry(node: &mut Node, g: Geometry) -> Result<()> {
    let unsupported = |field: &str| invalid(format!("--{field} does not apply to this shape kind"));
    for (field, set) in [
        ("sides", g.sides.is_some()),
        ("tips", g.tips.is_some()),
        ("inner", g.inner.is_some()),
        ("x2", g.x2.is_some()),
        ("y2", g.y2.is_some()),
    ] {
        if set {
            return Err(invalid(format!(
                "--{field} only applies when adding a polygon, star or line"
            )));
        }
    }
    match &mut node.kind {
        NodeKind::Rect {
            x,
            y,
            width,
            height,
            rx,
        } => {
            for (field, set) in [
                ("radius", g.radius.is_some()),
                ("text", g.text.is_some()),
                ("font-size", g.font_size.is_some()),
                ("d", g.d.is_some()),
                ("evenodd", g.evenodd.is_some()),
            ] {
                if set {
                    return Err(unsupported(field));
                }
            }
            *x = g.x.unwrap_or(*x);
            *y = g.y.unwrap_or(*y);
            *width = g.width.unwrap_or(*width);
            *height = g.height.unwrap_or(*height);
            *rx = g.rx.unwrap_or(*rx);
        }
        NodeKind::Circle { cx, cy, radius } => {
            for (field, set) in [
                ("width", g.width.is_some()),
                ("height", g.height.is_some()),
                ("rx", g.rx.is_some()),
                ("text", g.text.is_some()),
                ("font-size", g.font_size.is_some()),
                ("d", g.d.is_some()),
                ("evenodd", g.evenodd.is_some()),
            ] {
                if set {
                    return Err(unsupported(field));
                }
            }
            *cx = g.x.unwrap_or(*cx);
            *cy = g.y.unwrap_or(*cy);
            *radius = g.radius.unwrap_or(*radius);
        }
        NodeKind::Text {
            x,
            y,
            text,
            font_size,
        } => {
            for (field, set) in [
                ("width", g.width.is_some()),
                ("height", g.height.is_some()),
                ("rx", g.rx.is_some()),
                ("radius", g.radius.is_some()),
                ("d", g.d.is_some()),
                ("evenodd", g.evenodd.is_some()),
            ] {
                if set {
                    return Err(unsupported(field));
                }
            }
            *x = g.x.unwrap_or(*x);
            *y = g.y.unwrap_or(*y);
            if let Some(t) = g.text {
                *text = t;
            }
            *font_size = g.font_size.unwrap_or(*font_size);
        }
        NodeKind::Path { d, evenodd, .. } => {
            for (field, set) in [
                ("x", g.x.is_some()),
                ("y", g.y.is_some()),
                ("width", g.width.is_some()),
                ("height", g.height.is_some()),
                ("rx", g.rx.is_some()),
                ("radius", g.radius.is_some()),
                ("text", g.text.is_some()),
                ("font-size", g.font_size.is_some()),
            ] {
                if set {
                    return Err(unsupported(field));
                }
            }
            let next_d = g.d.unwrap_or_else(|| d.clone());
            let next_evenodd = g.evenodd.unwrap_or(*evenodd);
            if next_d != *d {
                // Edited geometry is no longer the original primitive.
                node.original_kind = None;
            }
            node.kind = path_kind(next_d, next_evenodd)?;
        }
    }
    Ok(())
}

fn apply_appearance(node: &mut Node, a: Appearance) {
    if let Some(name) = a.name {
        node.name = name;
    }
    if let Some(fill) = a.fill {
        node.style.fill = fill;
    }
    if let Some(StrokeArg(stroke)) = a.stroke {
        node.style.stroke = stroke;
        if stroke.is_some() && node.style.stroke_width == 0. && a.stroke_width.is_none() {
            node.style.stroke_width = 1.;
        }
    }
    if let Some(width) = a.stroke_width {
        node.style.stroke_width = width;
    }
    if let Some(opacity) = a.opacity {
        node.style.opacity = opacity;
    }
    let t = &mut node.transform;
    t.x = a.tx.unwrap_or(t.x);
    t.y = a.ty.unwrap_or(t.y);
    t.rotation = a.rotation.unwrap_or(t.rotation);
    t.scale_x = a.scale_x.unwrap_or(t.scale_x);
    t.scale_y = a.scale_y.unwrap_or(t.scale_y);
    if let Some(GradientArg(gradient)) = a.gradient {
        node.gradient = gradient;
    }
    if let Some(group) = a.group {
        node.group = (!group.is_empty()).then_some(group);
    }
}

fn find(project: &Project, id: &str) -> Result<usize> {
    project
        .nodes
        .iter()
        .position(|n| n.id == id)
        .ok_or_else(|| {
            crate::assist::not_found("node", id, project.nodes.iter().map(|n| n.id.as_str()))
        })
}

fn check_nodes(project: &Project, ids: &[String]) -> Result<()> {
    for id in ids {
        find(project, id)?;
    }
    Ok(())
}

fn saved(project: &Project, output: &str, extra: Value) -> Result<Value> {
    let mut result = save_project(project, output)?;
    if let (Some(object), Value::Object(extra)) = (result.as_object_mut(), extra) {
        if output != "-" {
            object.extend(extra);
        }
    }
    Ok(result)
}

pub fn run_node(command: NodeCommand) -> Result<Value> {
    match command {
        NodeCommand::Add {
            input,
            kind,
            id,
            pivot,
            geometry,
            appearance,
            output,
        } => {
            let mut project = load(&input)?;
            let id = id.unwrap_or_else(|| project.unused_id("node"));
            let generated = matches!(kind, Kind::Polygon | Kind::Star | Kind::Line);
            let (x, y) = (geometry.x.unwrap_or(0.), geometry.y.unwrap_or(0.));
            let shape_error = |e: splinely_document::DocumentError| failure("validation_error", e);
            let base = match kind {
                Kind::Polygon => Node::polygon(
                    id.clone(),
                    x,
                    y,
                    geometry.radius.unwrap_or(50.),
                    geometry.sides.unwrap_or(6),
                )
                .map_err(shape_error)?,
                Kind::Star => Node::star(
                    id.clone(),
                    x,
                    y,
                    geometry.radius.unwrap_or(50.),
                    geometry.tips.unwrap_or(5),
                    geometry.inner.unwrap_or(0.45),
                )
                .map_err(shape_error)?,
                Kind::Line => Node::line(
                    id.clone(),
                    x,
                    y,
                    geometry.x2.unwrap_or(x + 100.),
                    geometry.y2.unwrap_or(y),
                )
                .map_err(shape_error)?,
                Kind::Rect => Node::rect(id.clone(), 0., 0., 100., 100.),
                Kind::Circle => Node::circle(id.clone(), 0., 0., 50.),
                Kind::Text => Node::text(id.clone(), 0., 0., "Text"),
                Kind::Path => {
                    let d = geometry
                        .d
                        .clone()
                        .ok_or_else(|| invalid("--kind path requires --d"))?;
                    Node::new(id.clone(), path_kind(d, false)?)
                }
            };
            let mut node = base;
            if !generated {
                apply_geometry(&mut node, geometry)?;
            }
            apply_appearance(&mut node, appearance);
            project.nodes.push(node);
            if pivot.is_some() {
                let last = project.nodes.len() - 1;
                centre_pivot(&mut project, last)?;
            }
            project
                .validate()
                .map_err(|e| failure("validation_error", e))?;
            saved(&project, &output, json!({"id": id}))
        }
        NodeCommand::Update {
            input,
            node,
            node_id,
            pivot,
            geometry,
            appearance,
            output,
        } => {
            let node = node.or(node_id).unwrap_or_default();
            let mut project = load(&input)?;
            let index = find(&project, &node)?;
            apply_geometry(&mut project.nodes[index], geometry)?;
            apply_appearance(&mut project.nodes[index], appearance);
            if pivot.is_some() {
                centre_pivot(&mut project, index)?;
            }
            project.prune_empty_groups();
            saved(&project, &output, json!({"id": node}))
        }
        NodeCommand::Delete {
            input,
            nodes,
            output,
        } => {
            let mut project = load(&input)?;
            check_nodes(&project, &nodes)?;
            for id in &nodes {
                project
                    .remove_node_with_references(id)
                    .map_err(|e| failure("validation_error", e))?;
            }
            saved(&project, &output, json!({"deleted": nodes}))
        }
        NodeCommand::Duplicate {
            input,
            nodes,
            dx,
            dy,
            output,
        } => {
            let mut project = load(&input)?;
            check_nodes(&project, &nodes)?;
            let mut created = vec![];
            for id in &nodes {
                let index = find(&project, id)?;
                let mut copy = project.nodes[index].clone();
                copy.id = project.unused_id(&format!("{id}-copy"));
                copy.locked = false;
                copy.transform.x += dx;
                copy.transform.y += dy;
                for clip in &mut project.clips {
                    let tracks: Vec<_> = clip
                        .tracks
                        .iter()
                        .filter(|t| &t.node_id == id)
                        .cloned()
                        .map(|mut t| {
                            t.node_id = copy.id.clone();
                            t
                        })
                        .collect();
                    clip.tracks.extend(tracks);
                }
                created.push(copy.id.clone());
                project.nodes.insert(index + 1, copy);
            }
            saved(&project, &output, json!({"created": created}))
        }
        NodeCommand::List { input } => {
            let project = load(&input)?;
            let bounds = project
                .authored_bounds()
                .map_err(|e| failure("evaluation_error", e))?;
            let nodes: Vec<Value> = project
                .nodes
                .iter()
                .map(|n| {
                    let kind = match n.kind {
                        NodeKind::Rect { .. } => "rect",
                        NodeKind::Circle { .. } => "circle",
                        NodeKind::Text { .. } => "text",
                        NodeKind::Path { .. } => "path",
                    };
                    json!({
                        "id": n.id,
                        "name": n.name,
                        "kind": kind,
                        "group": n.group,
                        "groupPath": project.group_path(&n.id),
                        "boneId": n.bone_id,
                        "visible": n.visible,
                        "locked": n.locked,
                        "fill": hex(&n.style.fill),
                        "gradient": n.gradient.as_ref().map(|g| if g.radial { "radial" } else { "linear" }),
                        "stroke": n.style.stroke.as_ref().map(hex),
                        "bounds": bounds.get(&n.id),
                    })
                })
                .collect();
            Ok(
                json!({"ok": true, "width": project.width, "height": project.height, "nodes": nodes, "groups": project.groups}),
            )
        }
    }
}

fn axis(direction: Direction) -> Axis {
    match direction {
        Direction::Horizontal => Axis::Horizontal,
        Direction::Vertical => Axis::Vertical,
    }
}

fn edited(
    input: &str,
    nodes: &[String],
    edit: impl FnOnce(&mut Project) -> std::result::Result<(), splinely_document::DocumentError>,
) -> Result<Project> {
    let mut project = load(input)?;
    check_nodes(&project, nodes)?;
    edit(&mut project).map_err(|e| failure("validation_error", e))?;
    Ok(project)
}

fn centre(project: &Project, nodes: &[String]) -> Result<(f64, f64)> {
    let b = project
        .bounds_of(nodes)
        .map_err(|e| failure("evaluation_error", e))?;
    Ok((b[0] + b[2] / 2., b[1] + b[3] / 2.))
}

pub fn run_arrange(command: ArrangeCommand) -> Result<Value> {
    let (project, output) = match command {
        ArrangeCommand::Scale {
            input,
            nodes,
            sx,
            sy,
            anchor_x,
            anchor_y,
            output,
        } => {
            let mut project = load(&input)?;
            check_nodes(&project, &nodes)?;
            let anchor = match (anchor_x, anchor_y) {
                (Some(x), Some(y)) => (x, y),
                _ => centre(&project, &nodes)?,
            };
            project
                .scale_nodes(&nodes, sx, sy.unwrap_or(sx), anchor)
                .map_err(|e| failure("validation_error", e))?;
            (project, output)
        }
        ArrangeCommand::Rotate {
            input,
            nodes,
            degrees,
            pivot_x,
            pivot_y,
            output,
        } => {
            let mut project = load(&input)?;
            check_nodes(&project, &nodes)?;
            let pivot = match (pivot_x, pivot_y) {
                (Some(x), Some(y)) => (x, y),
                _ => centre(&project, &nodes)?,
            };
            project
                .rotate_nodes(&nodes, degrees, pivot)
                .map_err(|e| failure("validation_error", e))?;
            (project, output)
        }
        ArrangeCommand::Morph {
            input,
            source,
            target,
            duration,
            output,
        } => {
            let mut project = load(&input)?;
            check_nodes(&project, &[source.clone(), target.clone()])?;
            let id = project
                .create_morph(&source, &target, duration)
                .map_err(|e| failure("validation_error", e))?;
            return saved(&project, &output, json!({"clip": id}));
        }
        ArrangeCommand::Boolean {
            input,
            nodes,
            op,
            output,
        } => {
            let mut project = load(&input)?;
            check_nodes(&project, &nodes)?;
            let op = match op {
                Boolean::Union => BooleanOp::Union,
                Boolean::Difference => BooleanOp::Difference,
                Boolean::Intersection => BooleanOp::Intersection,
                Boolean::Exclusion => BooleanOp::Exclusion,
            };
            let id = project
                .boolean_nodes(&nodes, op)
                .map_err(|e| failure("validation_error", e))?;
            return saved(&project, &output, json!({"id": id}));
        }
        ArrangeCommand::Align {
            input,
            nodes,
            edge,
            to,
            output,
        } => {
            let edge = match edge {
                Edge::Left => AlignEdge::Left,
                Edge::Hcenter => AlignEdge::HorizontalCenter,
                Edge::Right => AlignEdge::Right,
                Edge::Top => AlignEdge::Top,
                Edge::Vcenter => AlignEdge::VerticalCenter,
                Edge::Bottom => AlignEdge::Bottom,
            };
            let target = if to == Relative::Document {
                AlignTarget::Document
            } else {
                AlignTarget::Selection
            };
            (
                edited(&input, &nodes, |p| p.align_nodes(&nodes, edge, target))?,
                output,
            )
        }
        ArrangeCommand::Distribute {
            input,
            nodes,
            axis: direction,
            output,
        } => (
            edited(&input, &nodes, |p| {
                p.distribute_nodes(&nodes, axis(direction))
            })?,
            output,
        ),
        ArrangeCommand::Flip {
            input,
            nodes,
            axis: direction,
            output,
        } => (
            edited(&input, &nodes, |p| p.flip_nodes(&nodes, axis(direction)))?,
            output,
        ),
        ArrangeCommand::Move {
            input,
            nodes,
            dx,
            dy,
            output,
        } => (
            edited(&input, &nodes, |p| p.translate_nodes(&nodes, dx, dy))?,
            output,
        ),
        ArrangeCommand::Group {
            input,
            nodes,
            name,
            output,
        } => {
            let mut project = load(&input)?;
            check_nodes(&project, &nodes)?;
            let id = project
                .group_nodes(&nodes, name.as_deref())
                .map_err(|e| failure("validation_error", e))?;
            return saved(&project, &output, json!({"id": id}));
        }
        ArrangeCommand::Ungroup {
            input,
            nodes,
            output,
        } => (edited(&input, &nodes, |p| p.ungroup_nodes(&nodes))?, output),
        ArrangeCommand::Order {
            input,
            nodes,
            to,
            output,
        } => {
            let stack = match to {
                Order::Front => Stack::Top,
                Order::Forward => Stack::Raise,
                Order::Backward => Stack::Lower,
                Order::Back => Stack::Bottom,
            };
            (
                edited(&input, &nodes, |p| p.restack_nodes(&nodes, stack))?,
                output,
            )
        }
    };
    save_project(&project, &output)
}

/// Render a PNG at a time or an animated GIF (by output extension) — a quick
/// visual check for scripts and AIs. Uses the shared `splinely_document` renderer.
pub fn render(
    input: &str,
    output: &str,
    time: f64,
    width: Option<u32>,
    fps: f64,
    duration: Option<f64>,
    clip: Option<&str>,
) -> Result<Value> {
    if !time.is_finite() || time < 0. {
        return Err(failure(
            "invalid_time",
            "Time must be finite and nonnegative",
        ));
    }
    let mut project = load(input)?;
    if let Some(clip) = clip {
        if !project.clips.iter().any(|c| c.id == clip) {
            return Err(crate::assist::not_found(
                "clip",
                clip,
                project.clips.iter().map(|c| c.id.as_str()),
            ));
        }
        // Show exactly this clip, as if it were the only autoplay clip.
        for c in &mut project.clips {
            c.autoplay = c.id == clip;
        }
    }
    let width = width.unwrap_or(project.width.ceil() as u32);
    if !(1..=8192).contains(&width) {
        return Err(invalid("--width must be within 1..8192"));
    }
    let gif = output.to_lowercase().ends_with(".gif");
    let bytes = if gif {
        let duration = duration.unwrap_or_else(|| project.animation_duration());
        if duration <= 0. {
            return Err(invalid("No autoplay clip, so nothing moves").hint(
                "Pass --clip ID to render a specific clip, or make one autoplay with `clip update FILE --id ID --autoplay`.",
            ));
        }
        project
            .render_gif(duration, fps, width, Color::rgb(255, 255, 255))
            .map_err(|e| failure("export_error", e))?
    } else {
        project
            .render_png(time, width)
            .map_err(|e| failure("export_error", e))?
    };
    crate::atomic_write(output, &bytes)?;
    let height = ((project.height * width as f64 / project.width).ceil() as u32).max(1);
    Ok(
        json!({"ok": true, "output": output, "format": if gif { "gif" } else { "png" }, "width": width, "height": height, "time": time, "bytes": bytes.len()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_parse_and_format() {
        let c = color("#ff8000").unwrap();
        assert_eq!((c.r, c.a), (1., 1.));
        assert_eq!(hex(&c), "#ff8000");
        assert_eq!(hex(&color("#11223380").unwrap()), "#11223380");
        assert_eq!(hex(&color("red").unwrap()), "#ff0000");
        assert_eq!(hex(&color("rgb(0, 128, 255)").unwrap()), "#0080ff");
        assert!(color("#12345").is_err());
        assert!(color("notacolor").is_err());
        assert!(stroke("none").unwrap().0.is_none());
        let g = gradient("linear 90 red blue@0.8").unwrap().0.unwrap();
        assert_eq!((g.angle, g.stops.len(), g.stops[1].offset), (90., 2, 0.8));
        let g = gradient("radial white gold navy").unwrap().0.unwrap();
        assert!(g.radial);
        assert_eq!(g.stops[1].offset, 0.5);
        assert!(gradient("none").unwrap().0.is_none());
        assert!(gradient("linear red").is_err());
        assert!(gradient("conic red blue").is_err());
    }
}
