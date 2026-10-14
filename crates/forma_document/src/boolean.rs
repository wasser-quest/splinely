//! Boolean path operations (union, difference, intersection, exclusion) that
//! keep curves.
//!
//! Every outline becomes cubic Bézier segments in scene space (lines stay
//! lines). Segments are split at all mutual intersections, found by recursive
//! subdivision with hull tests (exact for line pairs, including overlapping
//! collinear edges). Each piece is classified by probing just left and right of
//! its midpoint with an exact curve winding test against every input shape's
//! own fill rule. Pieces where the result changes from outside to inside are
//! kept, oriented with the result on the left, and linked into closed rings.
//! The result contains `L` and `C` commands only.
use crate::geometry::{apply, dist, lerp, matrix, parse, to_d, Cubic, SubPath, P};
use crate::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanOp {
    /// Everything covered by any shape.
    Union,
    /// The bottom-most shape minus all others.
    Difference,
    /// Only the area covered by every shape.
    Intersection,
    /// Area covered by an odd number of shapes.
    Exclusion,
}

impl std::str::FromStr for BooleanOp {
    type Err = DocumentError;
    fn from_str(s: &str) -> Result<Self, DocumentError> {
        Ok(match s {
            "union" => Self::Union,
            "difference" | "subtract" => Self::Difference,
            "intersection" | "intersect" => Self::Intersection,
            "exclusion" | "xor" => Self::Exclusion,
            _ => {
                return Err(DocumentError(format!(
                    "unknown boolean operation {s} (union, difference, intersection, exclusion)"
                )))
            }
        })
    }
}

/// One input shape: closed subpaths in scene space plus its fill rule.
struct Shape {
    subs: Vec<SubPath>,
    evenodd: bool,
}

const MAX_SEGMENTS: usize = 5_000;
/// Subdivision budget per segment pair (guards near-coincident curves).
const MAX_LEAVES: usize = 2_000;

fn combine(op: BooleanOp, flags: &[bool]) -> bool {
    match op {
        BooleanOp::Union => flags.iter().any(|&f| f),
        BooleanOp::Intersection => flags.iter().all(|&f| f),
        BooleanOp::Difference => flags[0] && !flags[1..].iter().any(|&f| f),
        BooleanOp::Exclusion => flags.iter().filter(|&&f| f).count() % 2 == 1,
    }
}

// ---------------------------------------------------------------- inside test

/// Parameters in [0, 1] where the derivative of one coordinate vanishes.
fn monotone_splits(a: f64, b: f64, c: f64, d: f64) -> Vec<f64> {
    // Derivative of the cubic Bernstein polynomial / 3: quadratic at² + bt + c.
    let qa = -a + 3. * b - 3. * c + d;
    let qb = 2. * (a - 2. * b + c);
    let qc = b - a;
    let mut roots = vec![];
    if qa.abs() < 1e-12 {
        if qb.abs() > 1e-12 {
            roots.push(-qc / qb);
        }
    } else {
        let disc = qb * qb - 4. * qa * qc;
        if disc >= 0. {
            let s = disc.sqrt();
            roots.push((-qb + s) / (2. * qa));
            roots.push((-qb - s) / (2. * qa));
        }
    }
    let mut ts: Vec<f64> = roots
        .into_iter()
        .filter(|t| *t > 1e-9 && *t < 1. - 1e-9)
        .collect();
    ts.sort_by(f64::total_cmp);
    ts
}

/// Winding contribution of one cubic for a ray from `p` towards +x.
fn crossings(seg: &Cubic, p: P, winding: &mut i32, count: &mut u32) {
    let ys = seg.p.map(|q| q.1);
    let min = ys.iter().copied().fold(f64::INFINITY, f64::min);
    let max = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if p.1 < min || p.1 > max {
        return;
    }
    let mut ts = vec![0.];
    if !seg.line {
        ts.extend(monotone_splits(ys[0], ys[1], ys[2], ys[3]));
    }
    ts.push(1.);
    for w in ts.windows(2) {
        let (a, b) = (seg.eval(w[0]), seg.eval(w[1]));
        // Same half-open rule as for polygons, applied per monotone piece.
        if (a.1 <= p.1) == (b.1 <= p.1) {
            continue;
        }
        let (mut lo, mut hi) = (w[0], w[1]);
        let rising = b.1 > a.1;
        for _ in 0..60 {
            let mid = (lo + hi) / 2.;
            let below = seg.eval(mid).1 <= p.1;
            if below == rising {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        if seg.eval((lo + hi) / 2.).0 > p.0 {
            *count += 1;
            *winding += if rising { 1 } else { -1 };
        }
    }
}

fn inside(shape: &Shape, p: P) -> bool {
    let (mut winding, mut count) = (0, 0);
    for sub in &shape.subs {
        for seg in &sub.segs {
            crossings(seg, p, &mut winding, &mut count);
        }
    }
    if shape.evenodd {
        count % 2 == 1
    } else {
        winding != 0
    }
}

// ------------------------------------------------------------- intersections

fn overlap(a: [f64; 4], b: [f64; 4], eps: f64) -> bool {
    a[0] <= b[2] + eps && b[0] <= a[2] + eps && a[1] <= b[3] + eps && b[1] <= a[3] + eps
}

fn flat(c: &Cubic, tol: f64) -> bool {
    if c.line {
        return true;
    }
    let [a, b, cc, d] = c.p;
    let chord = (d.0 - a.0, d.1 - a.1);
    let len = chord.0.hypot(chord.1);
    let off = |q: P| {
        if len < 1e-12 {
            dist(q, a)
        } else {
            ((q.0 - a.0) * chord.1 - (q.1 - a.1) * chord.0).abs() / len
        }
    };
    off(b) <= tol && off(cc) <= tol
}

/// Chord–chord intersection parameters (inclusive ends), if any.
fn chord_cross(a: P, b: P, c: P, d: P) -> Option<(f64, f64)> {
    let r = (b.0 - a.0, b.1 - a.1);
    let s = (d.0 - c.0, d.1 - c.1);
    let denom = r.0 * s.1 - r.1 * s.0;
    if denom.abs() < 1e-14 {
        return None;
    }
    let q = (c.0 - a.0, c.1 - a.1);
    let t = (q.0 * s.1 - q.1 * s.0) / denom;
    let u = (q.0 * r.1 - q.1 * r.0) / denom;
    let e = 1e-9;
    (t >= -e && t <= 1. + e && u >= -e && u <= 1. + e).then_some((t.clamp(0., 1.), u.clamp(0., 1.)))
}

/// Parameter of `p` on the straight segment a–b if it lies on it (interior).
fn on_line(a: P, b: P, p: P, eps: f64) -> Option<f64> {
    let r = (b.0 - a.0, b.1 - a.1);
    let len2 = r.0 * r.0 + r.1 * r.1;
    if len2 < 1e-24 {
        return None;
    }
    let t = ((p.0 - a.0) * r.0 + (p.1 - a.1) * r.1) / len2;
    (t > 1e-9 && t < 1. - 1e-9 && dist(lerp(a, b, t), p) <= eps).then_some(t)
}

struct Search<'a> {
    tol: f64,
    leaves: usize,
    out: &'a mut Vec<(f64, f64)>,
}

impl Search<'_> {
    fn run(&mut self, a: &Cubic, ra: (f64, f64), b: &Cubic, rb: (f64, f64)) {
        if self.leaves > MAX_LEAVES || !overlap(a.hull(), b.hull(), self.tol) {
            return;
        }
        if flat(a, self.tol) && flat(b, self.tol) {
            self.leaves += 1;
            if let Some((t, u)) = chord_cross(a.start(), a.end(), b.start(), b.end()) {
                self.out
                    .push((ra.0 + t * (ra.1 - ra.0), rb.0 + u * (rb.1 - rb.0)));
            }
            return;
        }
        // Split the larger (or the curved) one.
        if !flat(a, self.tol) && (a.length() >= b.length() || flat(b, self.tol)) {
            let (l, r) = a.split(0.5);
            let m = (ra.0 + ra.1) / 2.;
            self.run(&l, (ra.0, m), b, rb);
            self.run(&r, (m, ra.1), b, rb);
        } else {
            let (l, r) = b.split(0.5);
            let m = (rb.0 + rb.1) / 2.;
            self.run(a, ra, &l, (rb.0, m));
            self.run(a, ra, &r, (m, rb.1));
        }
    }
}

/// Closest parameter of `p` on a curve (coarse sampling + refinement).
fn project(seg: &Cubic, p: P) -> (f64, f64) {
    let mut best = (0., f64::INFINITY);
    for i in 0..=32 {
        let t = i as f64 / 32.;
        let d = dist(seg.eval(t), p);
        if d < best.1 {
            best = (t, d);
        }
    }
    let mut step = 1. / 64.;
    for _ in 0..40 {
        for t in [best.0 - step, best.0 + step] {
            if (0. ..=1.).contains(&t) {
                let d = dist(seg.eval(t), p);
                if d < best.1 {
                    best = (t, d);
                }
            }
        }
        step /= 2.;
    }
    best
}

// --------------------------------------------------------------------- core

/// Run a boolean operation; returns closed subpaths of the result, each
/// oriented so the result lies on its left.
fn boolean_subpaths(shapes: &[Shape], op: BooleanOp) -> Result<Vec<SubPath>, DocumentError> {
    let segs: Vec<Cubic> = shapes
        .iter()
        .flat_map(|s| s.subs.iter().flat_map(|sub| sub.segs.iter().copied()))
        .collect();
    if segs.len() > MAX_SEGMENTS {
        return Err(DocumentError(format!(
            "boolean operation limited to {MAX_SEGMENTS} segments"
        )));
    }
    let extent = segs
        .iter()
        .flat_map(|s| s.p.iter().flat_map(|p| [p.0.abs(), p.1.abs()]))
        .fold(1., f64::max);
    let tol = extent * 1e-9;
    let merge = extent * 1e-6;

    // 1. Split parameters for every segment.
    let mut splits: Vec<Vec<f64>> = vec![vec![]; segs.len()];
    let hulls: Vec<[f64; 4]> = segs.iter().map(Cubic::hull).collect();
    let mut order: Vec<usize> = (0..segs.len()).collect();
    order.sort_by(|&i, &j| hulls[i][0].total_cmp(&hulls[j][0]));
    for (k, &i) in order.iter().enumerate() {
        for &j in &order[k + 1..] {
            if hulls[j][0] > hulls[i][2] + merge {
                break;
            }
            if !overlap(hulls[i], hulls[j], merge) {
                continue;
            }
            let (a, b) = (&segs[i], &segs[j]);
            if a.line && b.line {
                // Overlapping collinear edges: split each at the other's ends.
                for p in [b.start(), b.end()] {
                    if let Some(t) = on_line(a.start(), a.end(), p, merge) {
                        splits[i].push(t);
                    }
                }
                for p in [a.start(), a.end()] {
                    if let Some(u) = on_line(b.start(), b.end(), p, merge) {
                        splits[j].push(u);
                    }
                }
            }
            let mut found = vec![];
            Search {
                tol,
                leaves: 0,
                out: &mut found,
            }
            .run(a, (0., 1.), b, (0., 1.));
            for (t, u) in found {
                splits[i].push(t);
                splits[j].push(u);
            }
            // T-junctions where an end of one curve touches the other.
            if !(a.line && b.line) {
                for (seg, other, list) in [(a, b, i), (b, a, j)] {
                    for p in [other.start(), other.end()] {
                        let (t, d) = project(seg, p);
                        if d <= merge && t > 1e-6 && t < 1. - 1e-6 {
                            splits[list].push(t);
                        }
                    }
                }
            }
        }
    }

    // 2. Pieces with merged endpoints.
    let mut points: HashMap<(i64, i64), Vec<P>> = HashMap::new();
    let mut canonical = |p: P| -> P {
        let cell = ((p.0 / merge).floor() as i64, (p.1 / merge).floor() as i64);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(list) = points.get(&(cell.0 + dx, cell.1 + dy)) {
                    if let Some(q) = list.iter().find(|q| dist(**q, p) <= merge) {
                        return *q;
                    }
                }
            }
        }
        points.entry(cell).or_default().push(p);
        p
    };
    let mut pieces: Vec<Cubic> = vec![];
    for (i, seg) in segs.iter().enumerate() {
        let mut ts = std::mem::take(&mut splits[i]);
        ts.retain(|t| *t > 1e-9 && *t < 1. - 1e-9);
        ts.push(0.);
        ts.push(1.);
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        for w in ts.windows(2) {
            let mut piece = seg.sub(w[0], w[1]);
            let (s, e) = (canonical(piece.start()), canonical(piece.end()));
            if s == e && piece.length() < merge * 4. {
                continue;
            }
            // Move the ends onto the shared points, controls along with them.
            let (ds, de) = (
                (s.0 - piece.p[0].0, s.1 - piece.p[0].1),
                (e.0 - piece.p[3].0, e.1 - piece.p[3].1),
            );
            piece.p[0] = s;
            piece.p[1] = (piece.p[1].0 + ds.0, piece.p[1].1 + ds.1);
            piece.p[2] = (piece.p[2].0 + de.0, piece.p[2].1 + de.1);
            piece.p[3] = e;
            pieces.push(piece);
        }
    }
    // Coincident pieces (shared outlines of two shapes) are kept once.
    let key = |p: P| {
        (
            (p.0 / (merge * 4.)).round() as i64,
            (p.1 / (merge * 4.)).round() as i64,
        )
    };
    let mut seen = std::collections::HashSet::new();
    pieces.retain(|c| {
        let (a, b) = (key(c.start()), key(c.end()));
        let ends = if a <= b { (a, b) } else { (b, a) };
        seen.insert((ends, key(c.eval(0.5))))
    });

    // 3. Classify by probing both sides of each piece's midpoint.
    let mut kept: Vec<Cubic> = vec![];
    for piece in pieces {
        let m = piece.eval(0.5);
        let t = piece.tangent(0.5);
        let len = t.0.hypot(t.1);
        if len == 0. {
            continue;
        }
        let n = (-t.1 / len, t.0 / len);
        let probe = (piece.length() * 1e-4).clamp(extent * 1e-8, extent * 1e-4);
        let left = (m.0 + n.0 * probe, m.1 + n.1 * probe);
        let right = (m.0 - n.0 * probe, m.1 - n.1 * probe);
        let at_left: Vec<bool> = shapes.iter().map(|s| inside(s, left)).collect();
        let at_right: Vec<bool> = shapes.iter().map(|s| inside(s, right)).collect();
        match (combine(op, &at_left), combine(op, &at_right)) {
            (true, false) => kept.push(piece),
            (false, true) => kept.push(piece.reversed()),
            _ => {}
        }
    }

    // 4. Link oriented pieces into closed rings.
    let pkey = |p: P| ((p.0 / merge).round() as i64, (p.1 / merge).round() as i64);
    let mut outgoing: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, c) in kept.iter().enumerate() {
        outgoing.entry(pkey(c.start())).or_default().push(i);
    }
    let mut used = vec![false; kept.len()];
    let mut result = vec![];
    for first in 0..kept.len() {
        if used[first] {
            continue;
        }
        let mut ring = vec![];
        let mut current = first;
        loop {
            used[current] = true;
            ring.push(kept[current]);
            let end = kept[current].end();
            let incoming = kept[current].tangent(1.);
            let incoming = incoming.1.atan2(incoming.0);
            let next = outgoing.get(&pkey(end)).and_then(|list| {
                list.iter().copied().filter(|&i| !used[i]).max_by(|&i, &j| {
                    let turn = |k: usize| {
                        let o = kept[k].tangent(0.);
                        let mut d = o.1.atan2(o.0) - incoming;
                        while d <= -std::f64::consts::PI {
                            d += std::f64::consts::TAU;
                        }
                        while d > std::f64::consts::PI {
                            d -= std::f64::consts::TAU;
                        }
                        d
                    };
                    turn(i).total_cmp(&turn(j))
                })
            });
            match next {
                Some(n) => current = n,
                None => break,
            }
        }
        if ring.len() >= 2 || (ring.len() == 1 && !ring[0].line) {
            result.push(SubPath {
                segs: ring,
                closed: true,
            });
        }
    }
    Ok(result)
}

impl Project {
    /// Combine shapes with a boolean operation. The shapes (not locked, not
    /// text) are replaced by one path in scene coordinates that takes the style
    /// of the bottom-most shape and its layer position. Curves stay curves.
    /// Returns the new ID.
    pub fn boolean_nodes(
        &mut self,
        ids: &[String],
        op: BooleanOp,
    ) -> Result<String, DocumentError> {
        self.require_editable_for_boolean(ids)?;
        if ids.len() < 2 {
            return Err(DocumentError(
                "boolean operations need at least two shapes".into(),
            ));
        }
        let posed: HashMap<String, Node> = self
            .authored()
            .evaluate_with_context(0., &Self::authored_context())?
            .into_iter()
            .map(|n| (n.id.clone(), n))
            .collect();
        // Bottom-most first, as in the document order.
        let order: Vec<&Node> = self.nodes.iter().filter(|n| ids.contains(&n.id)).collect();
        let mut shapes = vec![];
        for node in &order {
            let (d, evenodd) = match &node.kind {
                NodeKind::Path { d, evenodd, .. } => (d.clone(), *evenodd),
                NodeKind::Text { .. } => {
                    return Err(DocumentError(format!(
                        "node {}: convert text to a path before boolean operations",
                        node.id
                    )))
                }
                kind => match crate::conversion::generated_path(kind)? {
                    NodeKind::Path { d, .. } => (d, false),
                    _ => unreachable!(),
                },
            };
            let m = matrix(&posed[&node.id].transform);
            let mut subs = parse(&d, &|p| apply(m, p))?;
            // Filling closes open subpaths implicitly.
            for sub in &mut subs {
                let (start, end) = (sub.start(), sub.segs.last().unwrap().end());
                if dist(start, end) > 1e-9 {
                    sub.segs.push(Cubic::line(end, start));
                }
                sub.closed = true;
            }
            shapes.push(Shape { subs, evenodd });
        }
        let result = boolean_subpaths(&shapes, op)?;
        if result.is_empty() {
            return Err(DocumentError(
                "the result of this boolean operation is empty".into(),
            ));
        }
        let d = to_d(&result, true);
        let bounds = crate::path_bounds(&d)?;
        let bottom = order[0];
        let id = self.unused_id("path");
        let mut node = Node::new(
            id.clone(),
            NodeKind::Path {
                d,
                bounds,
                evenodd: false,
            },
        );
        node.name = match op {
            BooleanOp::Union => "Vereinigung",
            BooleanOp::Difference => "Differenz",
            BooleanOp::Intersection => "Schnittmenge",
            BooleanOp::Exclusion => "Ausschluss",
        }
        .into();
        node.style = bottom.style;
        node.gradient = bottom.gradient.clone();
        node.group = bottom.group.clone();
        let index = self.nodes.iter().position(|n| n.id == bottom.id).unwrap();
        let mut next = self.clone();
        next.nodes.insert(index, node);
        for id in ids {
            next.remove_node_with_references(id)?;
        }
        next.validate()?;
        *self = next;
        Ok(id)
    }

    fn require_editable_for_boolean(&self, ids: &[String]) -> Result<(), DocumentError> {
        for id in ids {
            let node = self
                .nodes
                .iter()
                .find(|n| &n.id == id)
                .ok_or_else(|| DocumentError(format!("missing node {id}")))?;
            if node.locked {
                return Err(DocumentError(format!("node {id} is locked")));
            }
        }
        let mut unique = ids.to_vec();
        unique.sort();
        unique.dedup();
        if unique.len() != ids.len() {
            return Err(DocumentError("a node was given twice".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(d: &str) -> Shape {
        Shape {
            subs: parse(d, &|p| p).unwrap(),
            evenodd: false,
        }
    }

    /// Area of the result, flattened finely (consistent orientation sums holes).
    fn area(subs: &[SubPath]) -> f64 {
        subs.iter()
            .map(|s| {
                let mut pts = vec![];
                for seg in &s.segs {
                    for i in 0..64 {
                        pts.push(seg.eval(i as f64 / 64.));
                    }
                }
                let n = pts.len();
                (0..n)
                    .map(|i| {
                        let (a, b) = (pts[i], pts[(i + 1) % n]);
                        a.0 * b.1 - b.0 * a.1
                    })
                    .sum::<f64>()
                    / 2.
            })
            .sum::<f64>()
            .abs()
    }

    fn square(x: f64, y: f64, size: f64) -> String {
        format!("M{x} {y} H{} V{} H{x} Z", x + size, y + size)
    }

    fn circle(cx: f64, cy: f64, r: f64) -> String {
        match crate::conversion::generated_path(&NodeKind::Circle { cx, cy, radius: r }).unwrap() {
            NodeKind::Path { d, .. } => d,
            _ => unreachable!(),
        }
    }

    #[test]
    fn overlapping_squares() {
        let shapes = [shape(&square(0., 0., 10.)), shape(&square(5., 5., 10.))];
        for (op, expected) in [
            (BooleanOp::Union, 175.),
            (BooleanOp::Intersection, 25.),
            (BooleanOp::Difference, 75.),
            (BooleanOp::Exclusion, 150.),
        ] {
            let subs = boolean_subpaths(&shapes, op).unwrap();
            assert!(
                (area(&subs) - expected).abs() < 1e-6,
                "{op:?}: {}",
                area(&subs)
            );
        }
    }

    #[test]
    fn shared_edges_and_holes() {
        let subs = boolean_subpaths(
            &[shape(&square(0., 0., 10.)), shape(&square(10., 0., 10.))],
            BooleanOp::Union,
        )
        .unwrap();
        assert_eq!(subs.len(), 1);
        assert!((area(&subs) - 200.).abs() < 1e-6);
        let subs = boolean_subpaths(
            &[shape(&square(0., 0., 30.)), shape(&square(10., 10., 10.))],
            BooleanOp::Difference,
        )
        .unwrap();
        assert_eq!(subs.len(), 2);
        assert!((area(&subs) - 800.).abs() < 1e-6);
        assert!(boolean_subpaths(
            &[shape(&square(0., 0., 1.)), shape(&square(5., 5., 1.))],
            BooleanOp::Intersection
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn circles_keep_their_curves() {
        let pi = std::f64::consts::PI;
        // Two unit-ish circles of radius 50, centres 60 apart.
        let shapes = [shape(&circle(0., 0., 50.)), shape(&circle(60., 0., 50.))];
        let lens = {
            // Area of the lens: 2 r² acos(d/2r) − (d/2) √(4r² − d²)
            let (r, d) = (50_f64, 60_f64);
            2. * r * r * (d / (2. * r)).acos() - d / 2. * (4. * r * r - d * d).sqrt()
        };
        let union = boolean_subpaths(&shapes, BooleanOp::Union).unwrap();
        assert_eq!(union.len(), 1);
        assert!(
            (area(&union) - (2. * pi * 2500. - lens)).abs() < 20.,
            "{}",
            area(&union)
        ); // cubic circle approximation
        assert!(union[0].segs.iter().all(|s| !s.line), "curves stay curves");
        assert!(
            union[0].segs.len() < 12,
            "few pieces, not a polyline: {}",
            union[0].segs.len()
        );
        let inter = boolean_subpaths(&shapes, BooleanOp::Intersection).unwrap();
        assert!((area(&inter) - lens).abs() < 5., "{}", area(&inter));
        let diff = boolean_subpaths(&shapes, BooleanOp::Difference).unwrap();
        assert!((area(&diff) - (pi * 2500. - lens)).abs() < 10.);
        // A ring: circle minus a smaller concentric circle.
        let ring = boolean_subpaths(
            &[shape(&circle(0., 0., 50.)), shape(&circle(0., 0., 20.))],
            BooleanOp::Difference,
        )
        .unwrap();
        assert_eq!(ring.len(), 2);
        assert!((area(&ring) - pi * (2500. - 400.)).abs() < 10.);
    }

    #[test]
    fn curve_and_line_mixtures() {
        // Square with a circle bulging out of its right side.
        let shapes = [shape(&square(0., 0., 100.)), shape(&circle(100., 50., 30.))];
        let union = boolean_subpaths(&shapes, BooleanOp::Union).unwrap();
        let pi = std::f64::consts::PI;
        assert!(
            (area(&union) - (10_000. + pi * 900. / 2.)).abs() < 5.,
            "{}",
            area(&union)
        );
        assert!(union[0].segs.iter().any(|s| s.line) && union[0].segs.iter().any(|s| !s.line));
    }

    #[test]
    fn project_boolean_replaces_shapes_and_respects_transforms() {
        let mut p = Project::default();
        let mut a = Node::rect("a", 0., 0., 100., 100.);
        a.style.fill = Color::rgb(255, 0, 0);
        let mut b = Node::circle("b", 0., 0., 50.);
        b.transform.x = 100.;
        b.transform.y = 50.;
        p.nodes = vec![a, b, Node::text("t", 0., 0., "x")];
        let id = p
            .boolean_nodes(&["a".into(), "b".into()], BooleanOp::Union)
            .unwrap();
        assert_eq!(p.nodes.len(), 2);
        assert_eq!(p.nodes[0].id, id);
        assert_eq!(p.nodes[0].style.fill, Color::rgb(255, 0, 0));
        let NodeKind::Path { bounds, d, .. } = &p.nodes[0].kind else {
            panic!("expected a path")
        };
        assert!(d.contains('C'), "curves preserved: {d}");
        assert!(
            (bounds[2] - 150.).abs() < 0.01 && (bounds[3] - 100.).abs() < 0.01,
            "{bounds:?}"
        );
        let before = p.clone();
        assert!(p
            .boolean_nodes(&[id.clone(), "t".into()], BooleanOp::Union)
            .is_err());
        assert!(p.boolean_nodes(&[id], BooleanOp::Union).is_err());
        assert_eq!(p, before);
        p.validate().unwrap();
    }
}
