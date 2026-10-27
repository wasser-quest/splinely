//! `inspect` for readers that cannot see images (LLMs): a textual picture of
//! the scene. Every node in draw order with world bounds and colour words,
//! plain-English layout sentences (containment, overlap, occlusion, relative
//! position, mirrored pairs), an optional ASCII preview and a short animation
//! summary. Shapes are measured from their own rasterized silhouettes (the same
//! renderer as `render`), so curves, strokes and text count as drawn.
use crate::{failure, Result};
use serde_json::{json, Map, Value};
use splinely_document::{
    transformed_bounds, Action, AnimValue, Bounds, Color, Easing, FollowMode, Node, NodeKind,
    Project,
};
use svgtypes::{PathParser, PathSegment};

/// How much `inspect` prints.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Detail {
    /// Compact: one entry per node, the most useful sentences, short key lists.
    #[default]
    Summary,
    /// Everything: transforms, local geometry, all relations and keyframes.
    Full,
}

pub struct Options {
    pub detail: Detail,
    /// Only describe this node (its entry, its relations, its animation).
    pub node: Option<String>,
    pub ascii: bool,
    pub cols: usize,
    /// Zoom the ASCII picture to the drawn shapes instead of the whole canvas.
    pub crop: bool,
}

/// Above this many drawable nodes, silhouettes fall back to bounding boxes.
const MASK_LIMIT: usize = 150;
const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
const SUMMARY_SENTENCES: usize = 60;
const SUMMARY_KEYS: usize = 6;

/// Covered pixels of one node inside its pixel box `[x0, y0, x1, y1)`.
struct Mask {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    bits: Vec<bool>,
}

impl Mask {
    fn empty() -> Self {
        Mask {
            x0: 0,
            y0: 0,
            x1: 0,
            y1: 0,
            bits: vec![],
        }
    }
    fn get(&self, x: usize, y: usize) -> bool {
        x >= self.x0
            && x < self.x1
            && y >= self.y0
            && y < self.y1
            && self.bits[(y - self.y0) * (self.x1 - self.x0) + x - self.x0]
    }
    fn from_fn(w: usize, h: usize, covered: impl Fn(usize, usize) -> bool) -> Self {
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        for y in 0..h {
            for x in 0..w {
                if covered(x, y) {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        if x1 == 0 {
            return Mask::empty();
        }
        let mut bits = Vec::with_capacity((x1 - x0) * (y1 - y0));
        for y in y0..y1 {
            for x in x0..x1 {
                bits.push(covered(x, y));
            }
        }
        Mask {
            x0,
            y0,
            x1,
            y1,
            bits,
        }
    }
    fn count(&self) -> usize {
        self.bits.iter().filter(|b| **b).count()
    }
    fn intersection(&self, other: &Mask) -> usize {
        let (x0, y0) = (self.x0.max(other.x0), self.y0.max(other.y0));
        let (x1, y1) = (self.x1.min(other.x1), self.y1.min(other.y1));
        let mut n = 0;
        for y in y0..y1.max(y0) {
            for x in x0..x1.max(x0) {
                if self.get(x, y) && other.get(x, y) {
                    n += 1;
                }
            }
        }
        n
    }
}

/// One node as drawn at time 0.
struct Item {
    node: Node,
    bounds: Bounds,
    drawable: bool,
    /// Why it is not drawn (hidden, transparent …).
    unseen: Option<&'static str>,
    colour: String,
    shape: String,
    label: String,
    mask: Mask,
    /// Covered pixels and pixels where this node is the topmost one.
    pixels: usize,
    visible_pixels: usize,
    container: Option<usize>,
}

struct Sentence {
    text: String,
    ids: Vec<String>,
    /// Shown in summary mode too.
    important: bool,
}

fn r1(v: f64) -> f64 {
    let v = (v * 10.).round() / 10.;
    if v == 0. {
        0.
    } else {
        v
    }
}

fn r2(v: f64) -> f64 {
    let v = (v * 100.).round() / 100.;
    if v == 0. {
        0.
    } else {
        v
    }
}

fn fmt(v: f64) -> String {
    format!("{}", r2(v))
}

fn hex(c: Color) -> String {
    let b = |v: f64| (v.clamp(0., 1.) * 255.).round() as u8;
    let mut s = format!("#{:02X}{:02X}{:02X}", b(c.r), b(c.g), b(c.b));
    if c.a < 1. {
        s.push_str(&format!("{:02X}", b(c.a)));
    }
    s
}

/// Plain colour word for a colour ("peach", "dark blue", "white").
pub fn colour_name(c: Color) -> String {
    if c.a <= 0.01 {
        return "transparent".into();
    }
    let (r, g, b) = (c.r.clamp(0., 1.), c.g.clamp(0., 1.), c.b.clamp(0., 1.));
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.;
    let d = max - min;
    let name = if d < 0.08 || (d / (1. - (2. * l - 1.).abs()).max(1e-9)) < 0.12 {
        match l {
            l if l > 0.93 => "white",
            l if l > 0.7 => "light gray",
            l if l > 0.4 => "gray",
            l if l > 0.15 => "dark gray",
            _ => "black",
        }
        .to_string()
    } else {
        let h = if max == r {
            60. * ((g - b) / d).rem_euclid(6.)
        } else if max == g {
            60. * ((b - r) / d + 2.)
        } else {
            60. * ((r - g) / d + 4.)
        };
        let base = match h {
            h if !(15.0..345.0).contains(&h) => {
                if max < 0.55 {
                    "dark red"
                } else if min > 0.6 {
                    "pink"
                } else {
                    "red"
                }
            }
            h if h < 45. => {
                if max < 0.8 {
                    "brown"
                } else if min > 0.35 {
                    "peach"
                } else {
                    "orange"
                }
            }
            h if h < 70. => {
                if max < 0.6 {
                    "olive"
                } else if min > 0.6 {
                    "cream"
                } else {
                    "yellow"
                }
            }
            h if h < 165. => "green",
            h if h < 195. => "cyan",
            h if h < 255. => "blue",
            h if h < 290. => "purple",
            _ => "pink",
        };
        let fixed = matches!(
            base,
            "brown" | "peach" | "cream" | "olive" | "dark red" | "pink"
        );
        if !fixed && l < 0.3 {
            format!("dark {base}")
        } else if !fixed && l > 0.75 {
            format!("light {base}")
        } else {
            base.to_string()
        }
    };
    if c.a < 0.95 {
        format!("translucent {name}")
    } else {
        name
    }
}

/// Facts about a path's data.
struct PathFacts {
    subpaths: usize,
    segments: usize,
    curves: usize,
    lines: usize,
    closed: bool,
    commands: Map<String, Value>,
}

fn path_facts(d: &str) -> PathFacts {
    let mut facts = PathFacts {
        subpaths: 0,
        segments: 0,
        curves: 0,
        lines: 0,
        closed: true,
        commands: Map::new(),
    };
    let mut open = false;
    for segment in PathParser::from(d).flatten() {
        let letter = match segment {
            PathSegment::MoveTo { .. } => {
                if open {
                    facts.closed = false;
                }
                facts.subpaths += 1;
                open = true;
                "M"
            }
            PathSegment::ClosePath { .. } => {
                open = false;
                "Z"
            }
            PathSegment::LineTo { .. } => "L",
            PathSegment::HorizontalLineTo { .. } => "H",
            PathSegment::VerticalLineTo { .. } => "V",
            PathSegment::CurveTo { .. } => "C",
            PathSegment::SmoothCurveTo { .. } => "S",
            PathSegment::Quadratic { .. } => "Q",
            PathSegment::SmoothQuadratic { .. } => "T",
            PathSegment::EllipticalArc { .. } => "A",
        };
        match letter {
            "L" | "H" | "V" => {
                facts.lines += 1;
                facts.segments += 1;
            }
            "C" | "S" | "Q" | "T" | "A" => {
                facts.curves += 1;
                facts.segments += 1;
            }
            _ => {}
        }
        let n = facts
            .commands
            .get(letter)
            .and_then(Value::as_u64)
            .unwrap_or(0);
        facts.commands.insert(letter.into(), json!(n + 1));
    }
    if open || facts.subpaths == 0 {
        facts.closed = false;
    }
    facts
}

fn path_shape(facts: &PathFacts, w: f64, h: f64) -> String {
    let aspect = if h > 0. { w / h } else { f64::INFINITY };
    let elongation = if aspect > 1.6 {
        "wide "
    } else if aspect < 1. / 1.6 {
        "tall "
    } else {
        ""
    };
    let base = if facts.subpaths > 1 {
        format!("compound shape ({} parts)", facts.subpaths)
    } else if facts.closed && facts.lines == 0 {
        if elongation.is_empty() {
            "round blob".into()
        } else {
            format!("{elongation}blob")
        }
    } else if facts.closed && facts.curves == 0 {
        // A closing Z adds one more edge when the last point is not the first.
        match facts.lines + 1 {
            3 => "triangle".into(),
            4 => "quadrilateral".into(),
            n => format!("{n}-sided polygon"),
        }
    } else if facts.closed {
        format!("{elongation}shape with curved and straight edges")
    } else if facts.curves == 0 && facts.lines == 1 {
        "line".into()
    } else if facts.curves == 0 {
        "polyline".into()
    } else {
        "open curve".into()
    };
    base
}

fn approx(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance * a.abs().max(b.abs()).max(1e-9)
}

fn shape_word(node: &Node, bounds: Bounds) -> String {
    let t = node.transform;
    match &node.kind {
        NodeKind::Circle { .. } => {
            if approx(t.scale_x.abs(), t.scale_y.abs(), 0.05) {
                "circle".into()
            } else {
                "ellipse".into()
            }
        }
        NodeKind::Rect {
            width, height, rx, ..
        } => {
            let square = approx(width * t.scale_x.abs(), height * t.scale_y.abs(), 0.05);
            let base = if square { "square" } else { "rectangle" };
            if *rx > 0. {
                format!("rounded {base}")
            } else {
                base.into()
            }
        }
        NodeKind::Text { text, .. } => {
            let short: String = text.chars().take(30).collect();
            let more = if text.chars().count() > 30 { "…" } else { "" };
            format!("text \"{short}{more}\"")
        }
        NodeKind::Path { d, .. } => path_shape(&path_facts(d), bounds[2], bounds[3]),
    }
}

fn colour_word(node: &Node) -> String {
    if let Some(g) = &node.gradient {
        let first = g
            .stops
            .first()
            .map(|s| colour_name(s.color))
            .unwrap_or_default();
        let last = g
            .stops
            .last()
            .map(|s| colour_name(s.color))
            .unwrap_or_default();
        return if first == last {
            first
        } else {
            format!("{first}-to-{last}")
        };
    }
    if node.style.fill.a > 0.01 {
        return colour_name(node.style.fill);
    }
    match node.style.stroke {
        Some(s) if s.a > 0.01 && node.style.stroke_width > 0. => {
            format!("{} outline", colour_name(s))
        }
        _ => "transparent".into(),
    }
}

/// Words for a node's name when it adds information beyond kind and ID.
fn informative_name(node: &Node) -> Option<&str> {
    let n = node.name.trim();
    let generic = [
        "",
        "circle",
        "rect",
        "rectangle",
        "path",
        "text",
        "ellipse",
        "shape",
    ];
    (!generic.contains(&n.to_lowercase().as_str()) && n != node.id).then_some(n)
}

fn label(node: &Node, colour: &str, shape: &str) -> String {
    let what = format!("{colour} {shape}");
    match informative_name(node) {
        Some(name) => format!("{} \"{name}\" ({what})", node.id),
        None => format!("{} ({what})", node.id),
    }
}

fn why_unseen(node: &Node) -> Option<&'static str> {
    let s = node.style;
    let stroke = s.stroke.is_some_and(|c| c.a > 0.01) && s.stroke_width > 0.;
    let fill = node.gradient.is_some() || s.fill.a > 0.01;
    if !node.visible {
        Some("hidden (visible = false)")
    } else if s.opacity <= 0.01 {
        Some("fully transparent (opacity 0)")
    } else if !fill && !stroke {
        Some("invisible: no fill and no stroke")
    } else {
        None
    }
}

/// Render one node alone (opaque) and keep the pixels it covers.
fn silhouette(project: &Project, node: &Node, width: u32) -> Option<(usize, usize, Mask)> {
    let mut alone = node.clone();
    alone.bone_id = None;
    alone.motion_path = None;
    alone.group = None;
    alone.a11y = None;
    alone.visible = true;
    alone.style.opacity = 1.;
    if alone.gradient.take().is_some() || alone.style.fill.a > 0.01 {
        alone.style.fill.a = 1.;
    }
    if let Some(stroke) = alone.style.stroke.as_mut() {
        if stroke.a > 0.01 {
            stroke.a = 1.;
        }
    }
    let scene = Project {
        width: project.width,
        height: project.height,
        nodes: vec![alone],
        ..Project::default()
    };
    let (w, h, rgba) = scene.render_rgba(0., width).ok()?;
    let (w, h) = (w as usize, h as usize);
    let mask = Mask::from_fn(w, h, |x, y| rgba[(y * w + x) * 4 + 3] >= 128);
    Some((w, h, mask))
}

/// Region word for a point inside a box: "upper-left", "centre", "bottom" …
fn region(px: f64, py: f64, b: Bounds) -> &'static str {
    let fx = if b[2] > 0. { (px - b[0]) / b[2] } else { 0.5 };
    let fy = if b[3] > 0. { (py - b[1]) / b[3] } else { 0.5 };
    let col = if fx < 0.4 {
        0
    } else if fx > 0.6 {
        2
    } else {
        1
    };
    let row = if fy < 0.4 {
        0
    } else if fy > 0.6 {
        2
    } else {
        1
    };
    [
        ["upper-left", "upper-centre", "upper-right"],
        ["left-centre", "centre", "right-centre"],
        ["lower-left", "lower-centre", "lower-right"],
    ][row][col]
}

fn centre(b: Bounds) -> (f64, f64) {
    (b[0] + b[2] / 2., b[1] + b[3] / 2.)
}

/// "left of", "above-right of" … for `a` relative to `b`.
fn direction(a: Bounds, b: Bounds) -> Option<String> {
    let (ax, ay) = centre(a);
    let (bx, by) = centre(b);
    let horizontal = if a[0] + a[2] <= b[0] || ax < bx - (a[2] + b[2]) / 4. {
        Some("left")
    } else if a[0] >= b[0] + b[2] || ax > bx + (a[2] + b[2]) / 4. {
        Some("right")
    } else {
        None
    };
    let vertical = if a[1] + a[3] <= b[1] || ay < by - (a[3] + b[3]) / 4. {
        Some("above")
    } else if a[1] >= b[1] + b[3] || ay > by + (a[3] + b[3]) / 4. {
        Some("below")
    } else {
        None
    };
    match (vertical, horizontal) {
        (Some(v), Some(h)) => Some(format!("{v} and to the {h} of")),
        (Some(v), None) => Some(v.to_string()),
        (None, Some(h)) => Some(format!("{h} of")),
        (None, None) => None,
    }
}

fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.
    } else {
        (100. * part as f64 / whole as f64).round()
    }
}

fn bounds_json(b: Bounds) -> Value {
    json!([r1(b[0]), r1(b[1]), r1(b[2]), r1(b[3])])
}

/// Build all items with silhouettes, visibility and containers.
fn items(project: &Project) -> Result<Vec<Item>> {
    let nodes = project
        .evaluate(0.)
        .map_err(|e| failure("evaluation_error", e))?;
    let items: Vec<Item> = nodes
        .into_iter()
        .map(|node| {
            let bounds = transformed_bounds(&node);
            let unseen = why_unseen(&node);
            let colour = colour_word(&node);
            let shape = shape_word(&node, bounds);
            let label = label(&node, &colour, &shape);
            Item {
                node,
                bounds,
                drawable: unseen.is_none(),
                unseen,
                colour,
                shape,
                label,
                mask: Mask::empty(),
                pixels: 0,
                visible_pixels: 0,
                container: None,
            }
        })
        .collect();
    Ok(items)
}

/// Silhouettes at `mask_width` pixels across the canvas, visible pixels and containers.
fn rasterize(project: &Project, items: &mut [Item], mask_width: u32) -> (usize, usize) {
    let scale = mask_width as f64 / project.width;
    let mw = mask_width as usize;
    let mh = ((project.height * scale).ceil() as usize).max(1);
    let drawable = items.iter().filter(|i| i.drawable).count();
    // Rasterize silhouettes (same renderer as `render`); many nodes → boxes.
    for item in items.iter_mut().filter(|i| i.drawable) {
        let rendered = (drawable <= MASK_LIMIT)
            .then(|| silhouette(project, &item.node, mask_width))
            .flatten();
        item.mask = match rendered {
            Some((_, _, mask)) => mask,
            None => {
                // Bounding box as the silhouette (approximate).
                let [x, y, w, h] = item.bounds.map(|v| v * scale);
                let clamp = |v: f64, max: usize| (v.round().max(0.) as usize).min(max);
                let (x0, y0) = (clamp(x, mw), clamp(y, mh));
                let (x1, y1) = (clamp(x + w, mw).max(x0), clamp(y + h, mh).max(y0));
                if x1 == x0 || y1 == y0 {
                    Mask::empty()
                } else {
                    Mask {
                        x0,
                        y0,
                        x1,
                        y1,
                        bits: vec![true; (x1 - x0) * (y1 - y0)],
                    }
                }
            }
        };
        item.pixels = item.mask.count();
    }
    // Topmost owner per pixel gives what is actually seen.
    for &k in owner_map(items, mw, mh)
        .iter()
        .filter(|k| **k != usize::MAX)
    {
        items[k].visible_pixels += 1;
    }
    // Container: the smallest other shape that covers ≥ 95 % of this one.
    for a in 0..items.len() {
        if !items[a].drawable || items[a].pixels == 0 {
            continue;
        }
        let mut best: Option<(usize, usize)> = None;
        for b in 0..items.len() {
            if a == b
                || !items[b].drawable
                || items[b].pixels <= items[a].pixels
                || is_background(&items[b], project)
            {
                continue;
            }
            let shared = items[a].mask.intersection(&items[b].mask);
            if shared as f64 >= 0.95 * items[a].pixels as f64
                && best.is_none_or(|(_, p)| items[b].pixels < p)
            {
                best = Some((b, items[b].pixels));
            }
        }
        items[a].container = best.map(|(b, _)| b);
    }
    (mw, mh)
}

fn owner_map(items: &[Item], mw: usize, mh: usize) -> Vec<usize> {
    let mut owner = vec![usize::MAX; mw * mh];
    for (k, item) in items.iter().enumerate().filter(|(_, i)| i.drawable) {
        let m = &item.mask;
        for y in m.y0..m.y1.min(mh) {
            for x in m.x0..m.x1.min(mw) {
                if m.get(x, y) {
                    owner[y * mw + x] = k;
                }
            }
        }
    }
    owner
}

fn is_background(item: &Item, project: &Project) -> bool {
    let b = item.bounds;
    item.drawable
        && b[0] <= 0.5
        && b[1] <= 0.5
        && b[0] + b[2] >= project.width - 0.5
        && b[1] + b[3] >= project.height - 0.5
        && matches!(
            item.node.kind,
            NodeKind::Rect { .. } | NodeKind::Path { .. }
        )
}

fn node_json(item: &Item, index: usize, project: &Project, full: bool, items: &[Item]) -> Value {
    let n = &item.node;
    let b = item.bounds;
    let (cx, cy) = centre(b);
    let mut o = Map::new();
    o.insert("index".into(), json!(index));
    o.insert("id".into(), json!(n.id));
    if full || !n.name.is_empty() {
        o.insert("name".into(), json!(n.name));
    }
    let kind = match &n.kind {
        NodeKind::Path { .. } => "path",
        NodeKind::Rect { .. } => "rect",
        NodeKind::Circle { .. } => "circle",
        NodeKind::Text { .. } => "text",
    };
    o.insert("kind".into(), json!(kind));
    o.insert(
        "looksLike".into(),
        json!(format!("{} {}", item.colour, item.shape)),
    );
    let group = project.group_path(&n.id);
    if full || !group.is_empty() {
        o.insert("group".into(), json!(group));
    }
    o.insert("visible".into(), json!(n.visible));
    if let Some(why) = item.unseen {
        o.insert("notDrawn".into(), json!(why));
    }
    o.insert("fill".into(), json!(hex(n.style.fill)));
    if full || n.style.stroke.is_some() {
        o.insert(
            "stroke".into(),
            match n.style.stroke {
                Some(c) => json!(format!("{} width {}", hex(c), fmt(n.style.stroke_width))),
                None => Value::Null,
            },
        );
    }
    if full || n.gradient.is_some() {
        o.insert(
            "gradient".into(),
            match &n.gradient {
                Some(g) => json!(format!(
                    "{} {}: {}",
                    if g.radial { "radial" } else { "linear" },
                    if g.radial {
                        String::new()
                    } else {
                        format!("{}°", fmt(g.angle))
                    },
                    g.stops
                        .iter()
                        .map(|s| format!("{}@{}", hex(s.color), fmt(s.offset)))
                        .collect::<Vec<_>>()
                        .join(" → ")
                )
                .replace("  ", " ")),
                None => Value::Null,
            },
        );
    }
    o.insert("opacity".into(), json!(r2(n.style.opacity)));
    o.insert("bounds".into(), bounds_json(b));
    o.insert("center".into(), json!([r1(cx), r1(cy)]));
    let t = n.transform;
    match &n.kind {
        NodeKind::Circle { radius, .. } => {
            o.insert(
                "geometry".into(),
                json!({"center":[r1(cx),r1(cy)],"radiusX":r1(radius*t.scale_x.abs()),"radiusY":r1(radius*t.scale_y.abs())}),
            );
        }
        NodeKind::Rect {
            width, height, rx, ..
        } => {
            o.insert(
                "geometry".into(),
                json!({"width":r1(width*t.scale_x.abs()),"height":r1(height*t.scale_y.abs()),"cornerRadius":r1(*rx),"rotation":r1(t.rotation)}),
            );
        }
        NodeKind::Text {
            text, font_size, ..
        } => {
            o.insert("text".into(), json!(text));
            o.insert("fontSize".into(), json!(r1(font_size * t.scale_y.abs())));
        }
        NodeKind::Path { d, .. } => {
            let f = path_facts(d);
            let mut p = json!({"segments":f.segments,"points":f.segments + f.subpaths,
                "subpaths":f.subpaths,"closed":f.closed,"curves":f.curves,"straight":f.lines});
            if full {
                p["commands"] = Value::Object(f.commands);
                p["dLength"] = json!(d.len());
            }
            o.insert("path".into(), p);
        }
    }
    if item.drawable {
        o.insert(
            "canvasRegion".into(),
            json!(region(cx, cy, [0., 0., project.width, project.height])),
        );
        o.insert(
            "visiblePercent".into(),
            json!(percent(item.visible_pixels, item.pixels)),
        );
    }
    if let Some(c) = item.container {
        o.insert("inside".into(), json!(items[c].node.id));
    }
    if full {
        o.insert("transform".into(), json!(t));
        o.insert("localGeometry".into(), json!(n.kind));
        o.insert("boneId".into(), json!(n.bone_id));
        o.insert("motionPath".into(), json!(n.motion_path));
        let behind: Vec<&str> = items
            .iter()
            .enumerate()
            .skip(index + 1)
            .filter(|(_, o)| o.drawable && o.mask.intersection(&item.mask) > 0)
            .map(|(_, o)| o.node.id.as_str())
            .collect();
        o.insert("coveredBy".into(), json!(behind));
    }
    Value::Object(o)
}

/// Containers from the direct one outwards.
fn chain(items: &[Item], mut k: Option<usize>) -> Vec<usize> {
    let mut out = vec![];
    while let Some(c) = k {
        if out.contains(&c) {
            break;
        }
        out.push(c);
        k = items[c].container;
    }
    out
}

/// " (shifted 20% left and 10% up from its middle)" when clearly off-centre.
fn offset_words(inner: Bounds, outer: Bounds) -> String {
    let ((ix, iy), (ox, oy)) = (centre(inner), centre(outer));
    let dx = if outer[2] > 0. {
        100. * (ix - ox) / outer[2]
    } else {
        0.
    };
    let dy = if outer[3] > 0. {
        100. * (iy - oy) / outer[3]
    } else {
        0.
    };
    let mut parts = vec![];
    if dx.abs() >= 8. {
        parts.push(format!(
            "{}% {}",
            dx.abs().round(),
            if dx < 0. { "left" } else { "right" }
        ));
    }
    if dy.abs() >= 8. {
        parts.push(format!(
            "{}% {}",
            dy.abs().round(),
            if dy < 0. { "up" } else { "down" }
        ));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" (shifted {} from its middle)", parts.join(" and "))
    }
}

fn sentences(items: &[Item], project: &Project) -> Vec<Sentence> {
    let mut out = vec![];
    let mut say = |text: String, ids: Vec<&str>, important: bool| {
        out.push(Sentence {
            text,
            ids: ids.into_iter().map(String::from).collect(),
            important,
        })
    };
    let canvas = [0., 0., project.width, project.height];
    // Where each shape is, and what is not seen.
    for item in items {
        let id = item.node.id.as_str();
        if let Some(why) = item.unseen {
            say(
                format!("{} is not drawn: {why}.", item.label),
                vec![id],
                true,
            );
            continue;
        }
        let b = item.bounds;
        let (cx, cy) = centre(b);
        if b[0] >= project.width || b[1] >= project.height || b[0] + b[2] <= 0. || b[1] + b[3] <= 0.
        {
            say(
                format!("{} lies completely outside the canvas.", item.label),
                vec![id],
                true,
            );
            continue;
        }
        if is_background(item, project) {
            say(
                format!("{} fills the whole canvas (background).", item.label),
                vec![id],
                true,
            );
            continue;
        }
        let size = format!("{}×{} units", r1(b[2]).round(), r1(b[3]).round());
        match item.container {
            Some(c) => {
                let parent = &items[c];
                let place = region(cx, cy, parent.bounds);
                say(
                    format!(
                        "{} is inside {}, {place} of it{}; {size}.",
                        item.label,
                        parent.label,
                        offset_words(b, parent.bounds)
                    ),
                    vec![id, &parent.node.id],
                    true,
                );
            }
            None => {
                let share = 100. * b[2] * b[3] / (project.width * project.height);
                say(
                    format!(
                        "{} is in the {} of the canvas; {size}, ~{}% of the canvas.",
                        item.label,
                        region(cx, cy, canvas),
                        share.round().max(if share > 0. { 1. } else { 0. }),
                    ),
                    vec![id],
                    true,
                );
            }
        }
        if item.pixels == 0 {
            say(
                format!("{} is too small or thin to show in a preview.", item.label),
                vec![id],
                true,
            );
        }
    }
    // Occlusion and overlap: `back` is drawn before `front`.
    for back in 0..items.len() {
        let a = &items[back];
        if !a.drawable || a.pixels == 0 {
            continue;
        }
        if a.visible_pixels == 0 {
            let front: Vec<&str> = items[back + 1..]
                .iter()
                .filter(|f| f.drawable && f.mask.intersection(&a.mask) > 0)
                .map(|f| f.node.id.as_str())
                .collect();
            let mut ids = vec![a.node.id.as_str()];
            ids.extend(&front);
            say(
                format!(
                    "{} is completely hidden behind {} (drawn in front of it).",
                    a.label,
                    front.join(", ")
                ),
                ids,
                true,
            );
            continue;
        }
        for (k, f) in items.iter().enumerate().skip(back + 1) {
            if !f.drawable || f.pixels == 0 {
                continue;
            }
            let shared = f.mask.intersection(&a.mask);
            if shared == 0 {
                continue;
            }
            let pct = percent(shared, a.pixels);
            let pct_front = percent(shared, f.pixels);
            let ids = vec![f.node.id.as_str(), a.node.id.as_str()];
            if chain(items, f.container).contains(&back) {
                say(
                    format!("{} is drawn on top of {}.", f.label, a.label),
                    ids,
                    false,
                );
            } else if chain(items, a.container).contains(&k) {
                say(
                    format!(
                        "{} is inside {} but drawn BEHIND it, so {pct}% of it is covered.",
                        a.label, f.label
                    ),
                    ids,
                    true,
                );
            } else {
                let (fx, fy) = centre(f.bounds);
                let at = region(fx, fy, a.bounds);
                let text = if pct < 1. && pct_front < 1. {
                    format!("{} touches {} at its {at}.", f.label, a.label)
                } else if f.colour == a.colour {
                    format!(
                        "{} overlaps {} at its {at}; same colour, so they read as one connected shape.",
                        f.label, a.label
                    )
                } else {
                    format!(
                        "{} overlaps {} at its {at} and, being in front, covers {pct}% of it.",
                        f.label, a.label
                    )
                };
                say(text, ids, true);
            }
        }
    }
    // Relative position among siblings (same container): nearest neighbour.
    let mut seen_pairs: Vec<(usize, usize)> = vec![];
    for (i, a) in items.iter().enumerate() {
        if !a.drawable || is_background(a, project) {
            continue;
        }
        let (ax, ay) = centre(a.bounds);
        let nearest = items
            .iter()
            .enumerate()
            .filter(|(j, b)| {
                *j != i && b.drawable && b.container == a.container && !is_background(b, project)
            })
            .map(|(j, b)| {
                let (bx, by) = centre(b.bounds);
                (j, (bx - ax).hypot(by - ay))
            })
            .min_by(|x, y| x.1.total_cmp(&y.1));
        if let Some((j, _)) = nearest {
            let pair = (i.min(j), i.max(j));
            if seen_pairs.contains(&pair) {
                continue;
            }
            seen_pairs.push(pair);
            let (p, q) = (&items[pair.0], &items[pair.1]);
            if let Some(dir) = direction(p.bounds, q.bounds) {
                say(
                    format!("{} is {dir} {}.", p.node.id, q.node.id),
                    vec![&p.node.id, &q.node.id],
                    true,
                );
            } else {
                say(
                    format!(
                        "{} and {} are at about the same place.",
                        p.node.id, q.node.id
                    ),
                    vec![&p.node.id, &q.node.id],
                    true,
                );
            }
        }
    }
    // Mirrored pairs: same look and size, side by side or one above the other.
    for i in 0..items.len() {
        for j in i + 1..items.len() {
            let (a, b) = (&items[i], &items[j]);
            // Same container, or containers that are themselves alike (pupils in eyes).
            let alike_frames = match (a.container, b.container) {
                (Some(x), Some(y)) => {
                    x == y
                        || (items[x].colour == items[y].colour && items[x].shape == items[y].shape)
                }
                (x, y) => x == y,
            };
            if !a.drawable
                || !b.drawable
                || !alike_frames
                || a.colour != b.colour
                || a.shape != b.shape
                || !approx(a.bounds[2], b.bounds[2], 0.15)
                || !approx(a.bounds[3], b.bounds[3], 0.15)
            {
                continue;
            }
            let ((ax, ay), (bx, by)) = (centre(a.bounds), centre(b.bounds));
            let (w, h) = (a.bounds[2].max(b.bounds[2]), a.bounds[3].max(b.bounds[3]));
            // Frame to compare the axis with: the shared container, else the
            // nearest common outer container, else the canvas.
            let common = chain(items, a.container)
                .into_iter()
                .find(|c| chain(items, b.container).contains(c));
            let frame = common.map_or(canvas, |c| items[c].bounds);
            let what = common.map_or("the canvas".to_string(), |c| items[c].node.id.clone());
            if (ay - by).abs() <= 0.15 * h && (ax - bx).abs() >= 0.5 * w {
                let axis = (ax + bx) / 2.;
                let (left, right) = if ax < bx { (a, b) } else { (b, a) };
                let off = axis - centre(frame).0;
                let relation = if off.abs() <= 0.06 * frame[2] {
                    format!(
                        "centred on {what} (its middle is x≈{})",
                        r1(centre(frame).0)
                    )
                } else {
                    format!(
                        "{} units {} of the middle of {what}",
                        r1(off.abs()),
                        if off < 0. { "left" } else { "right" }
                    )
                };
                say(
                    format!(
                        "{} (left) and {} (right) are a matching pair side by side, mirrored about x≈{}, {relation}.",
                        left.node.id,
                        right.node.id,
                        r1(axis)
                    ),
                    vec![&a.node.id, &b.node.id],
                    true,
                );
            } else if (ax - bx).abs() <= 0.15 * w && (ay - by).abs() >= 0.5 * h {
                let axis = (ay + by) / 2.;
                let (top, bottom) = if ay < by { (a, b) } else { (b, a) };
                say(
                    format!(
                        "{} (top) and {} (bottom) are a matching pair one above the other, mirrored about y≈{}.",
                        top.node.id,
                        bottom.node.id,
                        r1(axis)
                    ),
                    vec![&a.node.id, &b.node.id],
                    true,
                );
            }
        }
    }
    out
}

/// Picture of `area` (scene units) as `cols` characters per line.
fn ascii(
    items: &[Item],
    project: &Project,
    (mw, mh): (usize, usize),
    cols: usize,
    area: Bounds,
) -> Value {
    let rows = ((cols as f64 * area[3] / area[2] / 2.).round() as usize).max(1);
    let scale = mw as f64 / project.width;
    let px = |v: f64, max: usize| ((v * scale).floor().max(0.) as usize).min(max);
    let owner = owner_map(items, mw, mh);
    let mut letter: Vec<Option<u8>> = vec![None; items.len()];
    let mut next = 0;
    for (k, item) in items.iter().enumerate() {
        if item.drawable && item.visible_pixels > 0 {
            letter[k] = Some(LETTERS.get(next).copied().unwrap_or(b'#'));
            next += 1;
        }
    }
    let mut grid = vec![vec![b'.'; cols]; rows];
    let mut shown = vec![false; items.len()];
    // Best cell (largest owned pixel count) of each node, for tiny shapes.
    let mut best: Vec<(usize, usize, usize)> = vec![(0, 0, 0); items.len()];
    for (r, line) in grid.iter_mut().enumerate() {
        let y0 = px(area[1] + r as f64 * area[3] / rows as f64, mh - 1);
        let y1 = px(area[1] + (r + 1) as f64 * area[3] / rows as f64, mh).max(y0 + 1);
        for (c, cell) in line.iter_mut().enumerate() {
            let x0 = px(area[0] + c as f64 * area[2] / cols as f64, mw - 1);
            let x1 = px(area[0] + (c + 1) as f64 * area[2] / cols as f64, mw).max(x0 + 1);
            let mut counts: Vec<(usize, usize)> = vec![];
            let mut total = 0;
            for y in y0..y1.min(mh) {
                for x in x0..x1.min(mw) {
                    total += 1;
                    let k = owner[y * mw + x];
                    if k == usize::MAX {
                        continue;
                    }
                    match counts.iter_mut().find(|(o, _)| *o == k) {
                        Some((_, n)) => *n += 1,
                        None => counts.push((k, 1)),
                    }
                }
            }
            for &(k, n) in &counts {
                if n > best[k].0 {
                    best[k] = (n, r, c);
                }
            }
            if let Some(&(k, n)) = counts.iter().max_by_key(|(k, n)| (*n, *k)) {
                if 2 * n >= total.max(1) {
                    *cell = letter[k].unwrap_or(b'#');
                    shown[k] = true;
                }
            }
        }
    }
    for (k, &(n, r, c)) in best.iter().enumerate() {
        if n > 0 && !shown[k] {
            grid[r][c] = letter[k].unwrap_or(b'#');
        }
    }
    let lines: Vec<String> = grid
        .into_iter()
        .map(|l| String::from_utf8(l).expect("ascii"))
        .collect();
    let legend: Map<String, Value> = items
        .iter()
        .zip(&letter)
        .filter_map(|(item, l)| l.map(|l| ((l as char).to_string(), json!(item.label))))
        .collect();
    json!({
        "cols": cols, "rows": rows,
        "area": bounds_json(area),
        "cell": [r2(area[2] / cols as f64), r2(area[3] / rows as f64)],
        "note": "Each character is one cell of `cell` = [width, height] canvas units; the picture shows `area` = [x, y, width, height] (top row = top of the area). A letter is the topmost shape covering most of that cell (see legend); '.' is empty. Tiny shapes keep at least one cell.",
        "lines": lines,
        "legend": legend,
    })
}

fn value_text(v: &AnimValue) -> String {
    match v {
        AnimValue::Number(n) => fmt(*n),
        AnimValue::Color(c) => format!("{} {}", hex(*c), colour_name(*c)),
        AnimValue::Gradient(g) => format!(
            "gradient {}",
            g.stops
                .iter()
                .map(|s| hex(s.color))
                .collect::<Vec<_>>()
                .join("→")
        ),
        AnimValue::Path(d) => format!("path ({} segments)", path_facts(d).segments),
    }
}

fn easing_text(e: &Easing) -> String {
    match *e {
        Easing::Linear => "linear".into(),
        Easing::Step => "step".into(),
        Easing::CubicBezier { x1, y1, x2, y2 } => match [x1, y1, x2, y2] {
            [0.25, 0.1, 0.25, 1.] => "ease".into(),
            [0.42, 0., 1., 1.] => "ease-in".into(),
            [0., 0., 0.58, 1.] => "ease-out".into(),
            [0.42, 0., 0.58, 1.] => "ease-in-out".into(),
            _ => format!("cubic-bezier({x1},{y1},{x2},{y2})"),
        },
    }
}

fn property_name(p: splinely_document::Property) -> String {
    serde_json::to_value(p)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

/// Describe an animation target: node, bone or the group its rig bone moves.
fn target_name(project: &Project, id: &str) -> String {
    if let Some(g) = project.groups.iter().find(|g| {
        g.rig.as_ref().is_some_and(|r| {
            [&r.move_bone, &r.turn_bone, &r.attach_bone].contains(&&id.to_string())
        })
    }) {
        return format!("group {} (via rig bone {id})", g.id);
    }
    if project.bones.iter().any(|b| b.id == id) {
        return format!("bone {id}");
    }
    id.to_string()
}

fn action_text(a: &Action) -> String {
    match a {
        Action::SetProperty {
            node_id,
            property,
            value,
        } => format!(
            "set {node_id}.{} = {}",
            property_name(*property),
            value_text(value)
        ),
        Action::PlayClip { clip_id } => format!("play clip {clip_id}"),
        Action::StopClip { clip_id } => format!("stop clip {clip_id}"),
        Action::BlendClip {
            clip_id,
            weight,
            duration,
        } => format!(
            "blend clip {clip_id} to weight {} over {}s",
            fmt(*weight),
            fmt(*duration)
        ),
        Action::SetVariable { name, value } => format!("set variable {name} = {value}"),
        Action::Emit { event } => format!("emit event {}", event.name),
        Action::PlayAudio { asset_id } => format!("play audio {asset_id}"),
    }
}

fn mentions(focus: Option<&str>, ids: &[&str]) -> bool {
    focus.is_none_or(|f| ids.contains(&f))
}

fn animation(project: &Project, full: bool, focus: Option<&str>) -> Value {
    // Bones a focused node is bound to also count as "its" animation.
    let focus_bone = focus.and_then(|f| {
        project
            .nodes
            .iter()
            .find(|n| n.id == f)
            .and_then(|n| n.bone_id.clone())
    });
    let relevant =
        |id: &str| focus.is_none() || focus == Some(id) || focus_bone.as_deref() == Some(id);
    let clips: Vec<Value> = project
        .clips
        .iter()
        .filter_map(|clip| {
            let tracks: Vec<Value> = clip
                .tracks
                .iter()
                .filter(|t| relevant(&t.node_id))
                .map(|t| {
                    let keys: Vec<String> = t
                        .keyframes
                        .iter()
                        .map(|k| {
                            let easing = if matches!(k.easing, Easing::Linear) {
                                String::new()
                            } else {
                                format!(" ({})", easing_text(&k.easing))
                            };
                            format!("{}s {}{easing}", fmt(k.time), value_text(&k.value))
                        })
                        .collect();
                    let shown = if full || keys.len() <= SUMMARY_KEYS {
                        keys.join(" → ")
                    } else {
                        format!(
                            "{} → … ({} more) → {}",
                            keys[..SUMMARY_KEYS - 1].join(" → "),
                            keys.len() - SUMMARY_KEYS,
                            keys.last().expect("keys")
                        )
                    };
                    json!({"target": target_name(project, &t.node_id), "property": property_name(t.property),
                        "keyframes": t.keyframes.len(), "keys": shown})
                })
                .collect();
            if focus.is_some() && tracks.is_empty() {
                return None;
            }
            let mut c = json!({"id": clip.id, "duration": r2(clip.duration), "autoplay": clip.autoplay,
                "loop": clip.looping, "tracks": tracks});
            if !clip.name.is_empty() && clip.name != clip.id {
                c["name"] = json!(clip.name);
            }
            if full {
                c["blendMode"] = json!(clip.blend_mode);
                c["weight"] = json!(clip.weight);
                c["audioCues"] = json!(clip.audio_cues.len());
            }
            Some(c)
        })
        .collect();
    let mut interactions = vec![];
    let mut machines = vec![];
    for m in &project.state_machines {
        let mut timers = vec![];
        for t in &m.transitions {
            let actions: Vec<String> = t.actions.iter().map(action_text).collect();
            let mut ids: Vec<&str> = t.event.target.iter().map(String::as_str).collect();
            for a in &t.actions {
                if let Action::SetProperty { node_id, .. } = a {
                    ids.push(node_id);
                }
            }
            let entered: Vec<String> = m
                .states
                .iter()
                .filter(|s| s.id == t.to)
                .flat_map(|s| &s.overrides)
                .map(|o| {
                    format!(
                        "{}.{} = {}",
                        o.node_id,
                        property_name(o.property),
                        value_text(&o.value)
                    )
                })
                .collect();
            ids.extend(
                m.states
                    .iter()
                    .filter(|s| s.id == t.to)
                    .flat_map(|s| &s.overrides)
                    .map(|o| o.node_id.as_str()),
            );
            if !mentions(focus, &ids) {
                continue;
            }
            let mut text = match t.after {
                Some(after) => format!("after {}s in {}", fmt(after), t.from),
                None => format!(
                    "{} on {} (state {})",
                    t.event.name,
                    t.event.target.as_deref().unwrap_or("anything"),
                    t.from
                ),
            };
            if t.from != t.to {
                text.push_str(&format!(" → state {}", t.to));
            }
            if let Some(g) = &t.guard {
                text.push_str(&format!(" if {} == {}", g.variable, g.equals));
            }
            if !actions.is_empty() {
                text.push_str(&format!(": {}", actions.join("; ")));
            }
            if !entered.is_empty() && t.from != t.to {
                text.push_str(&format!(
                    "; state {} looks like {}",
                    t.to,
                    entered.join(", ")
                ));
            }
            if t.blend > 0. {
                text.push_str(&format!(" (blend {}s)", fmt(t.blend)));
            }
            let text = format!("[{}] {text}", m.id);
            if t.after.is_some() {
                timers.push(text);
            } else {
                interactions.push(json!(text));
            }
        }
        if focus.is_none() || !timers.is_empty() {
            machines.push(json!({"id": m.id, "initial": m.initial,
                "states": m.states.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
                "transitions": m.transitions.len(), "timers": timers}));
        }
    }
    let followers: Vec<Value> = project
        .followers
        .iter()
        .filter(|f| relevant(&f.target))
        .map(|f| {
            let what = match f.mode {
                FollowMode::Offset => {
                    format!("moves towards the pointer, up to {} units", fmt(f.max))
                }
                FollowMode::Rotate => format!("turns towards the pointer, up to {}°", fmt(f.max)),
            };
            let when = f
                .when
                .as_ref()
                .map(|w| format!(" while {} is in {}", w.machine, w.states.join("/")))
                .unwrap_or_default();
            json!(format!(
                "{} {what} (strength {}, smoothing {}s){when}",
                target_name(project, &f.target),
                fmt(f.strength),
                fmt(f.smoothing)
            ))
        })
        .collect();
    let rig_bones: Vec<&str> = project
        .groups
        .iter()
        .filter_map(|g| g.rig.as_ref())
        .flat_map(|r| {
            [
                r.move_bone.as_str(),
                r.turn_bone.as_str(),
                r.attach_bone.as_str(),
            ]
        })
        .collect();
    let bones: Vec<Value> = project
        .bones
        .iter()
        .filter(|b| full || !rig_bones.contains(&b.id.as_str()))
        .filter(|b| relevant(&b.id))
        .map(|b| {
            let bound: Vec<&str> = project
                .nodes
                .iter()
                .filter(|n| n.bone_id.as_deref() == Some(&b.id))
                .map(|n| n.id.as_str())
                .collect();
            let parent = match &b.parent_id {
                Some(p) => format!("child of {p}, offset ({}, {})", fmt(b.x), fmt(b.y)),
                None => format!("root at ({}, {})", fmt(b.x), fmt(b.y)),
            };
            json!(format!(
                "{}: {parent}, rotation {}°, length {}{}",
                b.id,
                fmt(b.rotation),
                fmt(b.length),
                if bound.is_empty() {
                    String::new()
                } else {
                    format!(", moves {}", bound.join(", "))
                }
            ))
        })
        .collect();
    let motion: Vec<Value> = project
        .nodes
        .iter()
        .filter(|n| relevant(&n.id))
        .filter_map(|n| {
            n.motion_path.as_ref().map(|m| {
                json!(format!(
                    "{} rides along path {} (progress {}{})",
                    n.id,
                    m.path,
                    fmt(m.progress),
                    if m.orient { ", turns with it" } else { "" }
                ))
            })
        })
        .collect();
    let mut out = json!({"clips": clips, "interactions": interactions, "stateMachines": machines,
        "followers": followers, "bones": bones, "motionPaths": motion});
    let hidden_rig = project.bones.len()
        - project
            .bones
            .iter()
            .filter(|b| !rig_bones.contains(&b.id.as_str()))
            .count();
    if !full && hidden_rig > 0 {
        out["groupRigBones"] = json!(hidden_rig);
    }
    let playing: Vec<&str> = project
        .clips
        .iter()
        .filter(|c| c.autoplay)
        .map(|c| c.id.as_str())
        .collect();
    out["summary"] = json!(if project.clips.is_empty()
        && project.state_machines.is_empty()
        && project.followers.is_empty()
    {
        "Static: no clips, interactions or followers.".to_string()
    } else {
        format!(
            "{} clip(s), {} play on open{}; {} interaction(s); {} follower(s).",
            project.clips.len(),
            playing.len(),
            if playing.is_empty() {
                String::new()
            } else {
                format!(" ({})", playing.join(", "))
            },
            project
                .state_machines
                .iter()
                .flat_map(|m| &m.transitions)
                .filter(|t| t.after.is_none())
                .count(),
            project.followers.len()
        )
    });
    out
}

/// Union of the drawn, non-background shapes on the canvas, padded by 5 %.
fn content_area(items: &[Item], project: &Project) -> Option<Bounds> {
    let mut area: Option<[f64; 4]> = None;
    for i in items
        .iter()
        .filter(|i| i.drawable && !is_background(i, project))
    {
        let [x, y, w, h] = i.bounds;
        area = Some(match area {
            None => [x, y, x + w, y + h],
            Some([a, b, c, d]) => [a.min(x), b.min(y), c.max(x + w), d.max(y + h)],
        });
    }
    let [x0, y0, x1, y1] = area?;
    let pad = 0.05 * (x1 - x0).max(y1 - y0);
    let (x0, y0) = ((x0 - pad).max(0.), (y0 - pad).max(0.));
    let (x1, y1) = (
        (x1 + pad).min(project.width),
        (y1 + pad).min(project.height),
    );
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1 - x0, y1 - y0])
}

/// Fields added to `inspect` output: `scene`, `layout`, `animation` and,
/// with `--ascii`, `ascii`.
pub fn describe(project: &Project, options: &Options) -> Result<Map<String, Value>> {
    let full = options.detail == Detail::Full;
    let focus = options.node.as_deref();
    if let Some(id) = focus {
        if !project.nodes.iter().any(|n| n.id == id) {
            return Err(crate::assist::not_found(
                "node",
                id,
                project.nodes.iter().map(|n| n.id.as_str()),
            ));
        }
    }
    if !(8..=200).contains(&options.cols) {
        return Err(failure("invalid_arguments", "--cols must be within 8..200"));
    }
    let mut items = items(project)?;
    let canvas = [0., 0., project.width, project.height];
    let area = if options.crop {
        content_area(&items, project).unwrap_or(canvas)
    } else {
        canvas
    };
    // Enough pixels per ASCII cell inside the shown area; fewer for big scenes.
    let drawn = items.iter().filter(|i| i.drawable).count();
    let cap = if drawn > 40 { 1024. } else { 2048. };
    let mask_width = ((options.cols * 4) as f64 * project.width / area[2]).clamp(320., cap) as u32;
    let size = rasterize(project, &mut items, mask_width);
    let background = items
        .iter()
        .find(|i| is_background(i, project))
        .map(|i| format!("{} ({})", i.colour, i.node.id))
        .unwrap_or_else(|| "none (transparent canvas)".into());
    let nodes: Vec<Value> = items
        .iter()
        .enumerate()
        .filter(|(_, i)| focus.is_none_or(|f| f == i.node.id))
        .map(|(k, i)| node_json(i, k, project, full || focus.is_some(), &items))
        .collect();
    let biggest = items
        .iter()
        .filter(|i| i.drawable && !is_background(i, project))
        .max_by_key(|i| i.pixels)
        .map(|i| {
            let (cx, cy) = centre(i.bounds);
            format!(
                " The largest shape is {} in the {} of the canvas.",
                i.label,
                region(cx, cy, [0., 0., project.width, project.height])
            )
        })
        .unwrap_or_default();
    let summary = format!(
        "{}×{} canvas, background: {background}. {} node(s), {drawn} drawn, back to front: {}.{biggest}",
        r1(project.width),
        r1(project.height),
        items.len(),
        items
            .iter()
            .filter(|i| i.drawable)
            .map(|i| i.node.id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let groups: Vec<Value> = project
        .groups
        .iter()
        .map(|g| {
            json!({"id": g.id, "name": g.name, "parent": g.parent_id,
                "members": project.nodes_in_group(&g.id)})
        })
        .collect();
    let mut scene = json!({
        "summary": summary,
        "canvas": {"width": project.width, "height": project.height, "background": background,
            "origin": "top-left, x right, y down"},
        "time": 0,
        "order": "back to front (later nodes are drawn on top)",
        "nodes": nodes,
    });
    if !groups.is_empty() {
        scene["groups"] = json!(groups);
    }
    let all = sentences(&items, project);
    let mut layout: Vec<String> = vec![];
    let mut omitted = 0;
    for s in all
        .iter()
        .filter(|s| focus.is_none_or(|f| s.ids.iter().any(|i| i == f)))
    {
        if full || focus.is_some() || (s.important && layout.len() < SUMMARY_SENTENCES) {
            layout.push(s.text.clone());
        } else {
            omitted += 1;
        }
    }
    if omitted > 0 {
        layout.push(format!(
            "… {omitted} more relation(s); use --detail full to list them all."
        ));
    }
    let mut out = Map::new();
    out.insert("scene".into(), scene);
    out.insert("layout".into(), json!(layout));
    if options.ascii {
        out.insert(
            "ascii".into(),
            ascii(&items, project, size, options.cols, area),
        );
    }
    out.insert("animation".into(), animation(project, full, focus));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_words_are_plain() {
        let c = |r, g, b| colour_name(Color::rgb(r, g, b));
        assert_eq!(c(255, 255, 255), "white");
        assert_eq!(c(0, 0, 0), "black");
        assert_eq!(c(224, 159, 112), "peach");
        assert_eq!(c(194, 108, 46), "brown");
        assert_eq!(c(255, 0, 0), "red");
        assert_eq!(c(11, 29, 58), "dark blue");
        assert_eq!(c(255, 165, 0), "orange");
    }

    #[test]
    fn path_facts_count_segments_and_closure() {
        let f = path_facts("M0 0 L10 0 L5 8 Z");
        assert_eq!((f.subpaths, f.segments, f.closed), (1, 2, true));
        assert_eq!(path_shape(&f, 10., 8.), "triangle");
        let f = path_facts("M0 0 C1 1 2 2 3 3");
        assert!(!f.closed);
        assert_eq!(path_shape(&f, 3., 3.), "open curve");
    }

    #[test]
    fn directions_and_regions() {
        assert_eq!(
            direction([0., 0., 10., 10.], [20., 0., 10., 10.]).unwrap(),
            "left of"
        );
        assert_eq!(
            direction([0., 0., 10., 10.], [20., 30., 10., 10.]).unwrap(),
            "above and to the left of"
        );
        assert_eq!(region(1., 1., [0., 0., 10., 10.]), "upper-left");
        assert_eq!(region(5., 5., [0., 0., 10., 10.]), "centre");
    }
}
