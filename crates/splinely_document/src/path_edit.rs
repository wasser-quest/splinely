//! Pure, editor-independent path node editing on absolute SVG segments.
//!
//! Every operation takes a segment list and returns a new one; nothing is
//! mutated in place and invalid requests leave the input untouched. Handles
//! address points by segment index, so callers should re-resolve selections
//! after structural edits (delete, break, join, reverse, paste).
//!
//! A subpath is `Move` + drawing segments + optional `Close`. When the last
//! drawing segment of a closed subpath ends exactly on the start point, that
//! end is the same node as the `Move` anchor (the "coincident" anchor); it is
//! never listed separately and moves together with the `Move` anchor.
use std::collections::BTreeSet;
use std::fmt::Write;

pub type Point = [f64; 2];

/// SVG path segment limit shared with the editor.
pub const MAX_SEGMENTS: usize = 50_000;
/// Screen distances closer than this count as a tie; anchors win ties.
pub const HIT_TIE_PX: f64 = 0.5;

#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    Move(Point),
    Line(Point),
    Quadratic {
        control: Point,
        end: Point,
    },
    Cubic {
        first: Point,
        second: Point,
        end: Point,
    },
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HandleKind {
    Anchor,
    FirstControl,
    SecondControl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Handle {
    pub segment: usize,
    pub kind: HandleKind,
}
impl Handle {
    pub fn anchor(segment: usize) -> Self {
        Self {
            segment,
            kind: HandleKind::Anchor,
        }
    }
}

/// How the two tangents at an anchor relate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeType {
    /// Independent ("split") tangents; a corner.
    Cusp,
    /// Collinear tangents, lengths kept.
    Smooth,
    /// Collinear tangents of equal length.
    Symmetric,
    /// Tangent derived from the neighbouring anchors (Catmull-Rom style).
    Auto,
}

pub fn valid_point(p: Point) -> Result<(), String> {
    if p.iter().any(|v| !v.is_finite() || v.abs() > 1e7) {
        Err("Path coordinates must be finite and within +/- 10000000".into())
    } else {
        Ok(())
    }
}

fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}
fn diff(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}
fn scale(a: Point, s: f64) -> Point {
    [a[0] * s, a[1] * s]
}
fn length(a: Point) -> f64 {
    a[0].hypot(a[1])
}
fn normalize(a: Point) -> Option<Point> {
    let l = length(a);
    (l > 1e-12 && l.is_finite()).then(|| scale(a, 1. / l))
}
fn lerp(a: Point, b: Point, t: f64) -> Point {
    add(a, scale(diff(b, a), t))
}

/// Parse SVG path data into absolute segments (H/V/S/T expanded, arcs as cubics).
pub fn parse(d: &str) -> Result<Vec<Segment>, String> {
    crate::path_bounds(d).map_err(|e| e.to_string())?;
    let escaped = d
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;");
    let svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"><path d=\"{escaped}\"/></svg>");
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let path = match tree.root().children() {
        [usvg::Node::Path(path)] => path,
        _ => return Err("SVG parser did not produce a single editable path".into()),
    };
    let point = |p: usvg::tiny_skia_path::Point| [p.x as f64, p.y as f64];
    use usvg::tiny_skia_path::PathSegment::*;
    Ok(path
        .data()
        .segments()
        .map(|s| match s {
            MoveTo(p) => Segment::Move(point(p)),
            LineTo(p) => Segment::Line(point(p)),
            QuadTo(c, p) => Segment::Quadratic {
                control: point(c),
                end: point(p),
            },
            CubicTo(a, b, p) => Segment::Cubic {
                first: point(a),
                second: point(b),
                end: point(p),
            },
            Close => Segment::Close,
        })
        .collect())
}

/// Serialize segments as absolute SVG path data.
pub fn path_data(segments: &[Segment]) -> String {
    let mut d = String::new();
    for s in segments {
        match *s {
            Segment::Move([x, y]) => write!(d, "M{x} {y} "),
            Segment::Line([x, y]) => write!(d, "L{x} {y} "),
            Segment::Quadratic {
                control: [x1, y1],
                end: [x, y],
            } => write!(d, "Q{x1} {y1} {x} {y} "),
            Segment::Cubic {
                first: [x1, y1],
                second: [x2, y2],
                end: [x, y],
            } => write!(d, "C{x1} {y1} {x2} {y2} {x} {y} "),
            Segment::Close => write!(d, "Z "),
        }
        .expect("writing to String cannot fail");
    }
    d.trim_end().into()
}

/// All handles in segment order (controls before the anchor they lead into).
pub fn handles(segments: &[Segment]) -> Vec<(Handle, Point)> {
    let mut out = Vec::new();
    for (segment, s) in segments.iter().enumerate() {
        let mut push = |kind, p| out.push((Handle { segment, kind }, p));
        match *s {
            Segment::Move(p) | Segment::Line(p) => push(HandleKind::Anchor, p),
            Segment::Quadratic { control, end } => {
                push(HandleKind::FirstControl, control);
                push(HandleKind::Anchor, end);
            }
            Segment::Cubic { first, second, end } => {
                push(HandleKind::FirstControl, first);
                push(HandleKind::SecondControl, second);
                push(HandleKind::Anchor, end);
            }
            Segment::Close => {}
        }
    }
    out
}

pub fn handle_point(segments: &[Segment], handle: Handle) -> Option<Point> {
    match (segments.get(handle.segment)?, handle.kind) {
        (Segment::Move(p) | Segment::Line(p), HandleKind::Anchor) => Some(*p),
        (Segment::Quadratic { end, .. } | Segment::Cubic { end, .. }, HandleKind::Anchor) => {
            Some(*end)
        }
        (Segment::Quadratic { control, .. }, HandleKind::FirstControl) => Some(*control),
        (Segment::Cubic { first, .. }, HandleKind::FirstControl) => Some(*first),
        (Segment::Cubic { second, .. }, HandleKind::SecondControl) => Some(*second),
        _ => None,
    }
}

fn set_handle(segments: &mut [Segment], handle: Handle, p: Point) -> Result<(), String> {
    let slot = match (segments.get_mut(handle.segment), handle.kind) {
        (Some(Segment::Move(p) | Segment::Line(p)), HandleKind::Anchor) => p,
        (Some(Segment::Quadratic { end, .. } | Segment::Cubic { end, .. }), HandleKind::Anchor) => {
            end
        }
        (Some(Segment::Quadratic { control, .. }), HandleKind::FirstControl) => control,
        (Some(Segment::Cubic { first, .. }), HandleKind::FirstControl) => first,
        (Some(Segment::Cubic { second, .. }), HandleKind::SecondControl) => second,
        _ => return Err("Path handle no longer exists".into()),
    };
    *slot = p;
    Ok(())
}

/// Nearest candidate within `radius` (screen pixels). Anchors win ties
/// (within [`HIT_TIE_PX`]) so a control lying on its anchor never hides it.
pub fn pick_handle(
    candidates: impl IntoIterator<Item = (Handle, f64)>,
    radius: f64,
) -> Option<Handle> {
    if !radius.is_finite() || radius < 0. {
        return None;
    }
    let mut best: Option<(Handle, f64)> = None;
    for (h, d) in candidates {
        if !d.is_finite() || d > radius {
            continue;
        }
        let better = match best {
            None => true,
            Some((b, bd)) => {
                if (d - bd).abs() <= HIT_TIE_PX {
                    let anchor = h.kind == HandleKind::Anchor;
                    let best_anchor = b.kind == HandleKind::Anchor;
                    (anchor && !best_anchor) || (anchor == best_anchor && d < bd)
                } else {
                    d < bd
                }
            }
        };
        if better {
            best = Some((h, d));
        }
    }
    best.map(|(h, _)| h)
}

// ---------------------------------------------------------------------------
// Subpath model

#[derive(Clone, Copy, Debug, PartialEq)]
enum Edge {
    Line,
    Quad(Point),
    Cubic(Point, Point),
}
impl Edge {
    fn segment(self, end: Point) -> Segment {
        match self {
            Edge::Line => Segment::Line(end),
            Edge::Quad(control) => Segment::Quadratic { control, end },
            Edge::Cubic(first, second) => Segment::Cubic { first, second, end },
        }
    }
    fn reversed(self) -> Self {
        match self {
            Edge::Cubic(a, b) => Edge::Cubic(b, a),
            other => other,
        }
    }
    fn translated(self, d: Point) -> Self {
        match self {
            Edge::Line => Edge::Line,
            Edge::Quad(c) => Edge::Quad(add(c, d)),
            Edge::Cubic(a, b) => Edge::Cubic(add(a, d), add(b, d)),
        }
    }
    /// Exact cubic controls; lines get thirds, quadratics are degree-elevated.
    fn cubic(self, start: Point, end: Point) -> (Point, Point) {
        match self {
            Edge::Line => (lerp(start, end, 1. / 3.), lerp(end, start, 1. / 3.)),
            Edge::Quad(c) => (lerp(start, c, 2. / 3.), lerp(end, c, 2. / 3.)),
            Edge::Cubic(a, b) => (a, b),
        }
    }
    /// Outer controls where a line contributes a retracted handle.
    fn outer(self, start: Point, end: Point) -> (Point, Point) {
        match self {
            Edge::Line => (start, end),
            other => other.cubic(start, end),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Sub {
    anchors: Vec<Point>,
    /// Source segment of each anchor (for handle lookup).
    anchor_segs: Vec<usize>,
    /// Open: `anchors.len() - 1` edges. Closed: `anchors.len()` edges, the last
    /// one returning to `anchors[0]`.
    edges: Vec<Edge>,
    /// Source segment of each edge (the `Close` index for implicit closing lines).
    edge_segs: Vec<usize>,
    closed: bool,
    /// The closing edge was written as its own segment before `Z`.
    explicit_close: bool,
}
impl Sub {
    fn n(&self) -> usize {
        self.anchors.len()
    }
    fn in_edge(&self, k: usize) -> Option<usize> {
        if k > 0 {
            Some(k - 1)
        } else if self.closed {
            self.edges.len().checked_sub(1)
        } else {
            None
        }
    }
    fn out_edge(&self, k: usize) -> Option<usize> {
        (k < self.edges.len()).then_some(k)
    }
    fn edge_start(&self, e: usize) -> Point {
        self.anchors[e]
    }
    fn edge_end(&self, e: usize) -> Point {
        self.anchors[(e + 1) % self.n()]
    }
    fn is_endpoint(&self, k: usize) -> bool {
        !self.closed && (k == 0 || k + 1 == self.n())
    }
    /// Source handle of the control at the start (anchor `e`) side of edge `e`.
    fn start_control(&self, e: usize) -> Option<Handle> {
        match self.edges[e] {
            Edge::Line => None,
            Edge::Quad(_) | Edge::Cubic(..) => Some(Handle {
                segment: self.edge_segs[e],
                kind: HandleKind::FirstControl,
            }),
        }
    }
    fn end_control(&self, e: usize) -> Option<Handle> {
        match self.edges[e] {
            Edge::Line => None,
            Edge::Quad(_) => Some(Handle {
                segment: self.edge_segs[e],
                kind: HandleKind::FirstControl,
            }),
            Edge::Cubic(..) => Some(Handle {
                segment: self.edge_segs[e],
                kind: HandleKind::SecondControl,
            }),
        }
    }
    fn ensure_cubic(&mut self, e: usize) -> (Point, Point) {
        let (a, b) = self.edges[e].cubic(self.edge_start(e), self.edge_end(e));
        self.edges[e] = Edge::Cubic(a, b);
        (a, b)
    }
    fn set_cubic(&mut self, e: usize, first: Point, second: Point) {
        let (start, end) = (self.edge_start(e), self.edge_end(e));
        self.edges[e] = if first == start && second == end {
            Edge::Line
        } else {
            Edge::Cubic(first, second)
        };
    }
    /// Merge the edges `from..from+count` (cyclic) into one edge.
    fn merged(&self, from: usize, count: usize) -> Edge {
        if count == 1 {
            return self.edges[from];
        }
        let m = self.edges.len();
        let last = (from + count - 1) % m;
        let (first, _) = self.edges[from].outer(self.edge_start(from), self.edge_end(from));
        let (_, second) = self.edges[last].outer(self.edge_start(last), self.edge_end(last));
        let (start, end) = (self.edge_start(from), self.edge_end(last));
        if first == start && second == end {
            Edge::Line
        } else {
            Edge::Cubic(first, second)
        }
    }
    fn reversed(&self) -> Sub {
        let n = self.n();
        let mut out = Sub {
            closed: self.closed,
            ..Default::default()
        };
        if self.closed {
            out.anchors.push(self.anchors[0]);
            out.anchors.extend(self.anchors[1..].iter().rev());
            out.edges = self.edges.iter().rev().map(|e| e.reversed()).collect();
        } else {
            out.anchors = self.anchors.iter().rev().copied().collect();
            out.edges = self.edges.iter().rev().map(|e| e.reversed()).collect();
        }
        debug_assert_eq!(out.n(), n);
        out
    }
    /// Open copy of anchors `from..=from+count` (cyclic) and the edges between.
    fn open_run(&self, from: usize, count: usize) -> Sub {
        let n = self.n();
        let mut out = Sub::default();
        for i in 0..=count {
            out.anchors.push(self.anchors[(from + i) % n]);
        }
        for i in 0..count {
            out.edges.push(self.edges[(from + i) % self.edges.len()]);
        }
        out
    }
}

fn subpaths(segments: &[Segment]) -> Result<Vec<Sub>, String> {
    let mut subs: Vec<Sub> = Vec::new();
    for (i, s) in segments.iter().enumerate() {
        match *s {
            Segment::Move(p) => subs.push(Sub {
                anchors: vec![p],
                anchor_segs: vec![i],
                ..Default::default()
            }),
            Segment::Close => {
                let sub = subs
                    .last_mut()
                    .filter(|s| !s.closed)
                    .ok_or("Unsupported path structure: close without an open subpath")?;
                sub.closed = true;
                let n = sub.n();
                if n > 1 && sub.anchors[n - 1] == sub.anchors[0] {
                    sub.anchors.pop();
                    sub.anchor_segs.pop();
                    sub.explicit_close = true;
                } else {
                    sub.edges.push(Edge::Line);
                    sub.edge_segs.push(i);
                }
            }
            ref drawing => {
                let sub = subs.last_mut().filter(|s| !s.closed).ok_or(
                    "Unsupported path structure: drawing command without a preceding move",
                )?;
                let (edge, end) = match *drawing {
                    Segment::Line(p) => (Edge::Line, p),
                    Segment::Quadratic { control, end } => (Edge::Quad(control), end),
                    Segment::Cubic { first, second, end } => (Edge::Cubic(first, second), end),
                    Segment::Move(_) | Segment::Close => unreachable!(),
                };
                sub.edges.push(edge);
                sub.edge_segs.push(i);
                sub.anchors.push(end);
                sub.anchor_segs.push(i);
            }
        }
    }
    Ok(subs)
}

/// Rebuild segments; also returns the anchor handle of every `(sub, k)`.
fn build(subs: &[Sub]) -> (Vec<Segment>, Vec<Vec<Handle>>) {
    let mut out = Vec::new();
    let mut index = Vec::new();
    for sub in subs {
        let n = sub.n();
        let mut anchors = Vec::with_capacity(n);
        if n == 0 {
            index.push(anchors);
            continue;
        }
        anchors.push(Handle::anchor(out.len()));
        out.push(Segment::Move(sub.anchors[0]));
        for (e, edge) in sub.edges.iter().enumerate() {
            let closing = sub.closed && e + 1 == sub.edges.len();
            if closing {
                if *edge != Edge::Line || sub.explicit_close {
                    out.push(edge.segment(sub.anchors[0]));
                }
            } else {
                anchors.push(Handle::anchor(out.len()));
                out.push(edge.segment(sub.anchors[e + 1]));
            }
        }
        if sub.closed {
            out.push(Segment::Close);
        }
        index.push(anchors);
    }
    (out, index)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Loc {
    sub: usize,
    k: usize,
}

fn locate(subs: &[Sub], h: Handle) -> Option<Loc> {
    if h.kind != HandleKind::Anchor {
        return None;
    }
    subs.iter().enumerate().find_map(|(si, sub)| {
        if let Some(k) = sub.anchor_segs.iter().position(|&s| s == h.segment) {
            return Some(Loc { sub: si, k });
        }
        (sub.closed && sub.explicit_close && sub.edge_segs.last() == Some(&h.segment))
            .then_some(Loc { sub: si, k: 0 })
    })
}

fn locate_all(subs: &[Sub], anchors: &[Handle]) -> Result<Vec<Loc>, String> {
    let mut out: Vec<Loc> = Vec::new();
    for &h in anchors {
        let loc = locate(subs, h).ok_or("Path anchor no longer exists")?;
        if !out.contains(&loc) {
            out.push(loc);
        }
    }
    Ok(out)
}

fn finish(subs: &[Sub]) -> Result<(Vec<Segment>, Vec<Vec<Handle>>), String> {
    let (segments, index) = build(subs);
    if segments.is_empty() {
        return Err("A path needs at least one subpath with two anchors".into());
    }
    if segments.len() > MAX_SEGMENTS {
        return Err("SVG path segment limit is 50000".into());
    }
    for (_, p) in handles(&segments) {
        valid_point(p)?;
    }
    Ok((segments, index))
}

// ---------------------------------------------------------------------------
// Queries

/// One handle per distinct node (coincident closing ends are omitted).
pub fn anchors(segments: &[Segment]) -> Vec<(Handle, Point)> {
    let Ok(subs) = subpaths(segments) else {
        return handles(segments)
            .into_iter()
            .filter(|(h, _)| h.kind == HandleKind::Anchor)
            .collect();
    };
    subs.iter()
        .flat_map(|s| {
            s.anchor_segs
                .iter()
                .zip(&s.anchors)
                .map(|(&i, &p)| (Handle::anchor(i), p))
        })
        .collect()
}

/// Map a coincident closing end to its `Move` anchor; None if not an anchor.
pub fn canonical_anchor(segments: &[Segment], h: Handle) -> Option<Handle> {
    let subs = subpaths(segments).ok()?;
    let loc = locate(&subs, h)?;
    Some(Handle::anchor(subs[loc.sub].anchor_segs[loc.k]))
}

/// Anchors whose mapped position lies inside the (unordered) rectangle corners.
pub fn anchors_in_rect(
    segments: &[Segment],
    corner_a: Point,
    corner_b: Point,
    to_screen: impl Fn(Point) -> Option<Point>,
) -> Vec<Handle> {
    let (x0, x1) = (corner_a[0].min(corner_b[0]), corner_a[0].max(corner_b[0]));
    let (y0, y1) = (corner_a[1].min(corner_b[1]), corner_a[1].max(corner_b[1]));
    anchors(segments)
        .into_iter()
        .filter_map(|(h, p)| {
            let [x, y] = to_screen(p)?;
            (x >= x0 && x <= x1 && y >= y0 && y <= y1).then_some(h)
        })
        .collect()
}

/// Controls worth showing: both controls of every edge touching a selected anchor.
pub fn visible_controls(segments: &[Segment], selected: &BTreeSet<Handle>) -> BTreeSet<Handle> {
    let mut out = BTreeSet::new();
    let Ok(subs) = subpaths(segments) else {
        return out;
    };
    for &h in selected {
        let Some(loc) = locate(&subs, h) else {
            continue;
        };
        let sub = &subs[loc.sub];
        for e in [sub.in_edge(loc.k), sub.out_edge(loc.k)]
            .into_iter()
            .flatten()
        {
            out.extend(sub.start_control(e));
            out.extend(sub.end_control(e));
        }
    }
    out
}

/// Infer the node type from geometry (never returns `Auto`).
pub fn node_type(segments: &[Segment], anchor: Handle) -> Option<NodeType> {
    let subs = subpaths(segments).ok()?;
    let loc = locate(&subs, anchor)?;
    let sub = &subs[loc.sub];
    let (Some(ie), Some(oe)) = (sub.in_edge(loc.k), sub.out_edge(loc.k)) else {
        return Some(NodeType::Cusp);
    };
    let a = sub.anchors[loc.k];
    let hin = match sub.edges[ie] {
        Edge::Line => return Some(NodeType::Cusp),
        Edge::Quad(c) | Edge::Cubic(_, c) => diff(c, a),
    };
    let hout = match sub.edges[oe] {
        Edge::Line => return Some(NodeType::Cusp),
        Edge::Quad(c) | Edge::Cubic(c, _) => diff(c, a),
    };
    let (lin, lout) = (length(hin), length(hout));
    if lin < 1e-9 || lout < 1e-9 {
        return Some(NodeType::Cusp);
    }
    let cross = (hin[0] * hout[1] - hin[1] * hout[0]) / (lin * lout);
    let dot = (hin[0] * hout[0] + hin[1] * hout[1]) / (lin * lout);
    Some(if cross.abs() > 1e-3 || dot > 0. {
        NodeType::Cusp
    } else if (lin - lout).abs() <= 1e-6 * lin.max(lout).max(1.) {
        NodeType::Symmetric
    } else {
        NodeType::Smooth
    })
}

/// Segment indices of edges whose two end anchors are both in `anchors`.
pub fn edges_between(segments: &[Segment], anchors: &[Handle]) -> Vec<usize> {
    let Ok(subs) = subpaths(segments) else {
        return vec![];
    };
    let locs: Vec<Loc> = anchors.iter().filter_map(|&h| locate(&subs, h)).collect();
    let mut out = Vec::new();
    for loc in &locs {
        let sub = &subs[loc.sub];
        if let Some(e) = sub.out_edge(loc.k) {
            let next = (loc.k + 1) % sub.n();
            if locs.contains(&Loc {
                sub: loc.sub,
                k: next,
            }) {
                out.push(sub.edge_segs[e]);
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

// ---------------------------------------------------------------------------
// Edits

/// Move handles by `delta`. Anchors carry their adjacent controls (and a
/// coincident closing end) along; each point moves at most once.
pub fn translate_anchors(
    segments: &[Segment],
    handles_to_move: &[Handle],
    delta: Point,
) -> Result<Vec<Segment>, String> {
    valid_point(delta)?;
    let subs = subpaths(segments)?;
    let mut moved = BTreeSet::new();
    for &h in handles_to_move {
        if h.kind != HandleKind::Anchor {
            handle_point(segments, h).ok_or("Path handle no longer exists")?;
            moved.insert(h);
            continue;
        }
        let loc = locate(&subs, h).ok_or("Path anchor no longer exists")?;
        let sub = &subs[loc.sub];
        moved.insert(Handle::anchor(sub.anchor_segs[loc.k]));
        if loc.k == 0 && sub.closed && sub.explicit_close {
            moved.insert(Handle::anchor(*sub.edge_segs.last().unwrap()));
        }
        if let Some(c) = sub.in_edge(loc.k).and_then(|e| sub.end_control(e)) {
            moved.insert(c);
        }
        if let Some(c) = sub.out_edge(loc.k).and_then(|e| sub.start_control(e)) {
            moved.insert(c);
        }
    }
    let mut out = segments.to_vec();
    for h in moved {
        let p = add(handle_point(&out, h).expect("checked above"), delta);
        valid_point(p)?;
        set_handle(&mut out, h, p)?;
    }
    Ok(out)
}

/// Remove anchors. Neighbouring edges merge into one edge that keeps the outer
/// controls; subpaths left with fewer than two anchors are dropped.
pub fn delete_anchors(segments: &[Segment], anchors: &[Handle]) -> Result<Vec<Segment>, String> {
    let subs = subpaths(segments)?;
    let locs = locate_all(&subs, anchors)?;
    if locs.is_empty() {
        return Err("No anchors selected".into());
    }
    let mut out = Vec::new();
    for (si, sub) in subs.iter().enumerate() {
        let keep: Vec<usize> = (0..sub.n())
            .filter(|&k| !locs.contains(&Loc { sub: si, k }))
            .collect();
        if keep.len() == sub.n() {
            out.push(sub.clone());
            continue;
        }
        if keep.len() < 2 {
            continue;
        }
        let mut next = Sub {
            closed: sub.closed,
            ..Default::default()
        };
        for (i, &k) in keep.iter().enumerate() {
            next.anchors.push(sub.anchors[k]);
            let following = match keep.get(i + 1) {
                Some(&j) => Some((k, j - k)),
                None if sub.closed => Some((k, sub.n() - k + keep[0])),
                None => None,
            };
            if let Some((from, count)) = following {
                next.edges.push(sub.merged(from, count));
            }
        }
        out.push(next);
    }
    if out.is_empty() {
        return Err("Deleting these anchors would leave no path; delete the shape instead".into());
    }
    finish(&out).map(|(s, _)| s)
}

fn edit_anchors(
    segments: &[Segment],
    anchors: &[Handle],
    mut edit: impl FnMut(&mut Sub, usize),
) -> Result<(Vec<Segment>, Vec<Handle>), String> {
    let mut subs = subpaths(segments)?;
    let locs = locate_all(&subs, anchors)?;
    for loc in &locs {
        edit(&mut subs[loc.sub], loc.k);
    }
    let (out, index) = finish(&subs)?;
    Ok((out, locs.iter().map(|l| index[l.sub][l.k]).collect()))
}

/// Outgoing tangent direction at anchor `k` derived from its neighbours.
fn auto_direction(sub: &Sub, k: usize) -> Option<Point> {
    let a = sub.anchors[k];
    let prev = sub.in_edge(k).map(|e| sub.edge_start(e));
    let next = sub.out_edge(k).map(|e| sub.edge_end(e));
    match (prev, next) {
        (Some(p), Some(n)) => normalize(diff(n, p)).or_else(|| {
            normalize(diff(n, a))
                .zip(normalize(diff(a, p)))
                .and_then(|(o, i)| normalize(add(o, i)))
        }),
        (None, Some(n)) => normalize(diff(n, a)),
        (Some(p), None) => normalize(diff(a, p)),
        (None, None) => None,
    }
}

fn apply_node_type(sub: &mut Sub, k: usize, ty: NodeType) {
    let a = sub.anchors[k];
    let (ie, oe) = (sub.in_edge(k), sub.out_edge(k));
    match (ie, oe) {
        (Some(ie), Some(oe)) if ie != oe => {
            let p = sub.edge_start(ie);
            let n = sub.edge_end(oe);
            match ty {
                NodeType::Cusp => {}
                NodeType::Auto => {
                    let Some(u) = auto_direction(sub, k) else {
                        return;
                    };
                    let (first_in, _) = sub.ensure_cubic(ie);
                    let (_, second_out) = sub.ensure_cubic(oe);
                    let lin = length(diff(a, p)) / 3.;
                    let lout = length(diff(n, a)) / 3.;
                    sub.set_cubic(ie, first_in, diff(a, scale(u, lin)));
                    sub.set_cubic(oe, add(a, scale(u, lout)), second_out);
                }
                NodeType::Smooth | NodeType::Symmetric => {
                    let (first_in, second_in) = sub.ensure_cubic(ie);
                    let (first_out, second_out) = sub.ensure_cubic(oe);
                    let (hin, hout) = (diff(second_in, a), diff(first_out, a));
                    let (mut lin, mut lout) = (length(hin), length(hout));
                    let din = normalize(hin).or_else(|| normalize(diff(p, a)));
                    let dout = normalize(hout).or_else(|| normalize(diff(n, a)));
                    if lin < 1e-12 {
                        lin = length(diff(p, a)) / 3.;
                    }
                    if lout < 1e-12 {
                        lout = length(diff(n, a)) / 3.;
                    }
                    let u = din
                        .zip(dout)
                        .and_then(|(i, o)| normalize(diff(o, i)))
                        .or_else(|| auto_direction(sub, k));
                    let Some(u) = u else {
                        return;
                    };
                    if ty == NodeType::Symmetric {
                        let avg = (lin + lout) / 2.;
                        lin = avg;
                        lout = avg;
                    }
                    sub.set_cubic(ie, first_in, diff(a, scale(u, lin)));
                    sub.set_cubic(oe, add(a, scale(u, lout)), second_out);
                }
            }
        }
        // Open endpoint: only Auto has a meaning (aim at the neighbour).
        (None, Some(oe)) if ty == NodeType::Auto => {
            let n = sub.edge_end(oe);
            let (_, second) = sub.ensure_cubic(oe);
            sub.set_cubic(oe, lerp(a, n, 1. / 3.), second);
        }
        (Some(ie), None) if ty == NodeType::Auto => {
            let p = sub.edge_start(ie);
            let (first, _) = sub.ensure_cubic(ie);
            sub.set_cubic(ie, first, lerp(a, p, 1. / 3.));
        }
        _ => {}
    }
}

/// Set the node type of each anchor. Cusp ("split tangent") keeps both
/// handles where they are and only marks them as independent: since node
/// types are not persisted it leaves the geometry unchanged. Smooth and
/// Symmetric align the handles (creating missing ones from the neighbours);
/// Auto derives both from the neighbouring anchors. Returns the new handles
/// of the given anchors.
pub fn set_node_type(
    segments: &[Segment],
    anchors: &[Handle],
    ty: NodeType,
) -> Result<(Vec<Segment>, Vec<Handle>), String> {
    edit_anchors(segments, anchors, |sub, k| apply_node_type(sub, k, ty))
}

/// Collapse both handles of each anchor onto it; fully retracted edges become lines.
pub fn retract_handles(
    segments: &[Segment],
    anchors: &[Handle],
) -> Result<(Vec<Segment>, Vec<Handle>), String> {
    edit_anchors(segments, anchors, |sub, k| {
        let a = sub.anchors[k];
        if let Some(e) = sub.in_edge(k) {
            if sub.edges[e] != Edge::Line {
                let (first, _) = sub.ensure_cubic(e);
                sub.set_cubic(e, first, a);
            }
        }
        if let Some(e) = sub.out_edge(k) {
            if sub.edges[e] != Edge::Line {
                let (_, second) = sub.ensure_cubic(e);
                sub.set_cubic(e, a, second);
            }
        }
    })
}

/// Pull handles out of a corner: every retracted side (line or zero-length
/// control) gets an automatic handle; existing handles are kept.
pub fn extend_handles(
    segments: &[Segment],
    anchors: &[Handle],
) -> Result<(Vec<Segment>, Vec<Handle>), String> {
    edit_anchors(segments, anchors, |sub, k| {
        let a = sub.anchors[k];
        let Some(u) = auto_direction(sub, k) else {
            return;
        };
        let (ie, oe) = (sub.in_edge(k), sub.out_edge(k));
        if let Some(e) = ie {
            let retracted = match sub.edges[e] {
                Edge::Line => true,
                Edge::Quad(c) | Edge::Cubic(_, c) => c == a,
            };
            if retracted {
                let len = length(diff(a, sub.edge_start(e))) / 3.;
                let (first, _) = sub.ensure_cubic(e);
                sub.edges[e] = Edge::Cubic(first, diff(a, scale(u, len)));
            }
        }
        if let Some(e) = oe {
            let retracted = match sub.edges[e] {
                Edge::Line => true,
                Edge::Quad(c) | Edge::Cubic(c, _) => c == a,
            };
            if retracted {
                let len = length(diff(sub.edge_end(e), a)) / 3.;
                let (_, second) = sub.ensure_cubic(e);
                sub.edges[e] = Edge::Cubic(add(a, scale(u, len)), second);
            }
        }
    })
}

/// Break subpaths at the given anchors. An open subpath splits in two (each
/// keeps a copy of the anchor); a closed one opens there. Endpoints of open
/// subpaths cannot be broken.
pub fn break_at_anchors(segments: &[Segment], anchors: &[Handle]) -> Result<Vec<Segment>, String> {
    let subs = subpaths(segments)?;
    let locs = locate_all(&subs, anchors)?;
    if locs.is_empty() {
        return Err("No anchors selected".into());
    }
    let mut out = Vec::new();
    for (si, sub) in subs.iter().enumerate() {
        let mut cuts: Vec<usize> = locs.iter().filter(|l| l.sub == si).map(|l| l.k).collect();
        cuts.sort_unstable();
        if cuts.is_empty() {
            out.push(sub.clone());
            continue;
        }
        let open = if sub.closed {
            let start = cuts[0];
            let rotated = sub.open_run(start, sub.edges.len());
            cuts = cuts
                .iter()
                .skip(1)
                .map(|&k| (k + sub.n() - start) % sub.n())
                .collect();
            rotated
        } else {
            if cuts.iter().any(|&k| sub.is_endpoint(k)) {
                return Err("Cannot break a path at its endpoint".into());
            }
            sub.clone()
        };
        let mut from = 0;
        for k in cuts.into_iter().chain([open.n() - 1]) {
            if k > from {
                out.push(open.open_run(from, k - from));
            }
            from = k;
        }
    }
    finish(&out).map(|(s, _)| s)
}

pub fn break_at(segments: &[Segment], anchor: Handle) -> Result<Vec<Segment>, String> {
    break_at_anchors(segments, &[anchor])
}

/// Join two open endpoints. Same subpath: closes it. Different subpaths: they
/// are concatenated (reversing one if needed). Coincident endpoints merge into
/// one node; otherwise a straight segment connects them.
pub fn join_endpoints(segments: &[Segment], a: Handle, b: Handle) -> Result<Vec<Segment>, String> {
    let mut subs = subpaths(segments)?;
    let la = locate(&subs, a).ok_or("Path anchor no longer exists")?;
    let lb = locate(&subs, b).ok_or("Path anchor no longer exists")?;
    let endpoint = |l: Loc| subs[l.sub].is_endpoint(l.k);
    if !endpoint(la) || !endpoint(lb) {
        return Err("Select two endpoints of open subpaths to join".into());
    }
    if la == lb {
        return Err("Select two different endpoints to join".into());
    }
    let coincident = |p: Point, q: Point| length(diff(p, q)) <= 1e-9;
    if la.sub == lb.sub {
        let s = &mut subs[la.sub];
        let n = s.n();
        if coincident(s.anchors[0], s.anchors[n - 1]) && n > 2 {
            s.anchors.pop();
            s.explicit_close = true;
        } else {
            s.edges.push(Edge::Line);
        }
        s.closed = true;
        return finish(&subs).map(|(s, _)| s);
    }
    let mut first = subs[la.sub].clone();
    if la.k == 0 && first.n() > 1 {
        first = first.reversed();
    }
    let mut second = subs[lb.sub].clone();
    if lb.k != 0 {
        second = second.reversed();
    }
    let last = *first.anchors.last().unwrap();
    if coincident(last, second.anchors[0]) {
        first.anchors.extend(&second.anchors[1..]);
    } else {
        first.edges.push(Edge::Line);
        first.anchors.extend(&second.anchors);
    }
    first.edges.extend(second.edges);
    let (keep, remove) = (la.sub.min(lb.sub), la.sub.max(lb.sub));
    subs[keep] = first;
    subs.remove(remove);
    finish(&subs).map(|(s, _)| s)
}

fn edit_edge(
    segments: &[Segment],
    edge_segment: usize,
    edit: impl FnOnce(&mut Sub, usize),
) -> Result<Vec<Segment>, String> {
    let mut subs = subpaths(segments)?;
    let (si, e) = subs
        .iter()
        .enumerate()
        .find_map(|(si, s)| {
            s.edge_segs
                .iter()
                .position(|&i| i == edge_segment)
                .map(|e| (si, e))
        })
        .ok_or("Path segment is not an editable edge")?;
    edit(&mut subs[si], e);
    finish(&subs).map(|(s, _)| s)
}

/// Make the edge drawn by `edge_segment` straight.
pub fn segment_to_line(segments: &[Segment], edge_segment: usize) -> Result<Vec<Segment>, String> {
    edit_edge(segments, edge_segment, |sub, e| sub.edges[e] = Edge::Line)
}

/// Turn the edge drawn by `edge_segment` into an equivalent cubic (thirds for
/// lines, exact degree elevation for quadratics), ready for bending.
pub fn segment_to_curve(segments: &[Segment], edge_segment: usize) -> Result<Vec<Segment>, String> {
    edit_edge(segments, edge_segment, |sub, e| {
        sub.ensure_cubic(e);
    })
}

/// Reverse the direction of every subpath containing one of `anchors`.
pub fn reverse_subpaths(segments: &[Segment], anchors: &[Handle]) -> Result<Vec<Segment>, String> {
    let mut subs = subpaths(segments)?;
    let locs = locate_all(&subs, anchors)?;
    let mut done = BTreeSet::new();
    for loc in locs {
        if done.insert(loc.sub) {
            subs[loc.sub] = subs[loc.sub].reversed();
        }
    }
    finish(&subs).map(|(s, _)| s)
}

pub fn reverse_subpath(segments: &[Segment], anchor: Handle) -> Result<Vec<Segment>, String> {
    reverse_subpaths(segments, &[anchor])
}

/// Copy selected anchors as standalone subpaths: every run of consecutive
/// selected anchors (with the edges between them) becomes an open subpath; a
/// fully selected closed subpath is copied closed. Single isolated anchors
/// carry no geometry and are skipped. Empty if nothing qualifies.
pub fn copy_anchors(segments: &[Segment], anchors: &[Handle]) -> Result<Vec<Segment>, String> {
    let subs = subpaths(segments)?;
    let locs = locate_all(&subs, anchors)?;
    let mut out = Vec::new();
    for (si, sub) in subs.iter().enumerate() {
        let n = sub.n();
        let selected: Vec<bool> = (0..n).map(|k| locs.contains(&Loc { sub: si, k })).collect();
        if selected.iter().all(|s| *s) && sub.closed {
            let mut whole = sub.clone();
            whole.explicit_close = false;
            out.push(whole);
            continue;
        }
        // Start runs where the previous anchor (cyclic for closed) is unselected.
        let starts = (0..n).filter(|&k| {
            selected[k]
                && if k == 0 {
                    !sub.closed || !selected[n - 1]
                } else {
                    !selected[k - 1]
                }
        });
        for start in starts {
            let mut count = 0;
            while count + 1 < n
                && sub.out_edge((start + count) % n).is_some()
                && selected[(start + count + 1) % n]
            {
                count += 1;
            }
            if count > 0 {
                out.push(sub.open_run(start, count));
            }
        }
    }
    Ok(build(&out).0)
}

/// Append copied subpaths moved by `offset`; returns the pasted anchors.
pub fn paste_subpaths(
    segments: &[Segment],
    clip: &[Segment],
    offset: Point,
) -> Result<(Vec<Segment>, Vec<Handle>), String> {
    valid_point(offset)?;
    let mut subs = subpaths(segments)?;
    let pasted = subpaths(clip)?;
    if pasted.iter().all(|s| s.n() < 2) {
        return Err("Nothing to paste".into());
    }
    let first = subs.len();
    subs.extend(pasted.into_iter().map(|mut s| {
        for p in &mut s.anchors {
            *p = add(*p, offset);
        }
        for e in &mut s.edges {
            *e = e.translated(offset);
        }
        s
    }));
    let (out, index) = finish(&subs)?;
    Ok((out, index[first..].iter().flatten().copied().collect()))
}

#[cfg(test)]
#[path = "path_edit_tests.rs"]
mod tests;
