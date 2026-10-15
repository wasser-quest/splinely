//! Cubic Bézier geometry shared by morphing and curve-preserving booleans.
//! Every path segment becomes a cubic; straight lines remember that they are
//! lines so they can be written back as `L`.
use crate::*;

pub(crate) type P = (f64, f64);

pub(crate) fn lerp(a: P, b: P, t: f64) -> P {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

pub(crate) fn dist(a: P, b: P) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// 2×3 affine matrix `[a, b, c, d, e, f]`: x' = a x + c y + e, y' = b x + d y + f.
pub(crate) type Matrix = [f64; 6];

pub(crate) fn matrix(t: &Transform) -> Matrix {
    let (sin, cos) = t.rotation.to_radians().sin_cos();
    [
        cos * t.scale_x,
        sin * t.scale_x,
        -sin * t.scale_y,
        cos * t.scale_y,
        t.x,
        t.y,
    ]
}

pub(crate) fn invert(m: Matrix) -> Option<Matrix> {
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1e-12 {
        return None;
    }
    let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    Some([a, b, c, d, -(a * m[4] + c * m[5]), -(b * m[4] + d * m[5])])
}

pub(crate) fn apply(m: Matrix, p: P) -> P {
    (
        m[0] * p.0 + m[2] * p.1 + m[4],
        m[1] * p.0 + m[3] * p.1 + m[5],
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Cubic {
    pub p: [P; 4],
    /// Straight segment (control points on the chord).
    pub line: bool,
}

impl Cubic {
    pub fn line(a: P, b: P) -> Self {
        Self {
            p: [a, lerp(a, b, 1. / 3.), lerp(a, b, 2. / 3.), b],
            line: true,
        }
    }

    pub fn curve(a: P, c1: P, c2: P, b: P) -> Self {
        Self {
            p: [a, c1, c2, b],
            line: false,
        }
    }

    pub fn start(&self) -> P {
        self.p[0]
    }

    pub fn end(&self) -> P {
        self.p[3]
    }

    pub fn eval(&self, t: f64) -> P {
        let [a, b, c, d] = self.p;
        let ab = lerp(a, b, t);
        let bc = lerp(b, c, t);
        let cd = lerp(c, d, t);
        lerp(lerp(ab, bc, t), lerp(bc, cd, t), t)
    }

    /// Tangent direction at t (falls back to the chord for degenerate ends).
    pub fn tangent(&self, t: f64) -> P {
        let [a, b, c, d] = self.p;
        let u = 1. - t;
        let dx = 3. * u * u * (b.0 - a.0) + 6. * u * t * (c.0 - b.0) + 3. * t * t * (d.0 - c.0);
        let dy = 3. * u * u * (b.1 - a.1) + 6. * u * t * (c.1 - b.1) + 3. * t * t * (d.1 - c.1);
        if dx.hypot(dy) > 1e-12 {
            (dx, dy)
        } else {
            (d.0 - a.0, d.1 - a.1)
        }
    }

    pub fn split(&self, t: f64) -> (Cubic, Cubic) {
        let [a, b, c, d] = self.p;
        let ab = lerp(a, b, t);
        let bc = lerp(b, c, t);
        let cd = lerp(c, d, t);
        let abc = lerp(ab, bc, t);
        let bcd = lerp(bc, cd, t);
        let m = lerp(abc, bcd, t);
        (
            Cubic {
                p: [a, ab, abc, m],
                line: self.line,
            },
            Cubic {
                p: [m, bcd, cd, d],
                line: self.line,
            },
        )
    }

    /// The part between parameters t0 < t1.
    pub fn sub(&self, t0: f64, t1: f64) -> Cubic {
        let (_, right) = self.split(t0);
        if t1 >= 1. {
            return right;
        }
        let (left, _) = right.split((t1 - t0) / (1. - t0));
        left
    }

    pub fn reversed(&self) -> Cubic {
        let [a, b, c, d] = self.p;
        Cubic {
            p: [d, c, b, a],
            line: self.line,
        }
    }

    /// Control-polygon bounding box `[min_x, min_y, max_x, max_y]`.
    pub fn hull(&self) -> [f64; 4] {
        let xs = self.p.map(|p| p.0);
        let ys = self.p.map(|p| p.1);
        [
            xs.iter().copied().fold(f64::INFINITY, f64::min),
            ys.iter().copied().fold(f64::INFINITY, f64::min),
            xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ]
    }

    /// Approximate arc length (control polygon / chord average).
    pub fn length(&self) -> f64 {
        let [a, b, c, d] = self.p;
        (dist(a, b) + dist(b, c) + dist(c, d) + dist(a, d)) / 2.
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SubPath {
    pub segs: Vec<Cubic>,
    pub closed: bool,
}

impl SubPath {
    pub fn start(&self) -> P {
        self.segs[0].start()
    }

    /// Signed area of the closed outline (flattened), positive = clockwise in SVG.
    pub fn area(&self) -> f64 {
        let mut points = vec![];
        for s in &self.segs {
            for i in 0..8 {
                points.push(s.eval(i as f64 / 8.));
            }
        }
        let n = points.len();
        (0..n)
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % n]);
                a.0 * b.1 - b.0 * a.1
            })
            .sum::<f64>()
            / 2.
    }

    pub fn centroid(&self) -> P {
        let n = self.segs.len() as f64;
        let (sx, sy) = self
            .segs
            .iter()
            .map(|s| s.start())
            .fold((0., 0.), |acc, p| (acc.0 + p.0, acc.1 + p.1));
        (sx / n, sy / n)
    }

    pub fn reversed(&self) -> SubPath {
        SubPath {
            segs: self.segs.iter().rev().map(Cubic::reversed).collect(),
            closed: self.closed,
        }
    }
}

/// Parse path data into subpaths of cubic segments (absolute coordinates,
/// mapped through `map`). Closed subpaths get an explicit closing segment.
pub(crate) fn parse(d: &str, map: &dyn Fn(P) -> P) -> Result<Vec<SubPath>, DocumentError> {
    use svgtypes::SimplePathSegment::*;
    let mut out: Vec<SubPath> = vec![];
    let mut segs: Vec<Cubic> = vec![];
    let (mut start, mut current) = ((0., 0.), (0., 0.));
    let finish = |segs: &mut Vec<Cubic>, out: &mut Vec<SubPath>, closed: bool| {
        if !segs.is_empty() {
            out.push(SubPath {
                segs: std::mem::take(segs),
                closed,
            });
        }
    };
    for segment in svgtypes::SimplifyingPathParser::from(d) {
        match segment.map_err(|e| DocumentError(format!("Invalid SVG path: {e}")))? {
            MoveTo { x, y } => {
                finish(&mut segs, &mut out, false);
                start = (x, y);
                current = start;
            }
            LineTo { x, y } => {
                segs.push(Cubic::line(map(current), map((x, y))));
                current = (x, y);
            }
            Quadratic { x1, y1, x, y } => {
                let c = (x1, y1);
                let c1 = lerp(current, c, 2. / 3.);
                let c2 = lerp((x, y), c, 2. / 3.);
                segs.push(Cubic::curve(map(current), map(c1), map(c2), map((x, y))));
                current = (x, y);
            }
            CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                segs.push(Cubic::curve(
                    map(current),
                    map((x1, y1)),
                    map((x2, y2)),
                    map((x, y)),
                ));
                current = (x, y);
            }
            ClosePath => {
                if dist(current, start) > 1e-9 {
                    segs.push(Cubic::line(map(current), map(start)));
                }
                finish(&mut segs, &mut out, true);
                current = start;
            }
        }
    }
    finish(&mut segs, &mut out, false);
    // Drop zero-length segments (they confuse matching and intersection).
    for sub in &mut out {
        sub.segs.retain(|s| s.length() > 1e-12);
    }
    out.retain(|s| !s.segs.is_empty());
    Ok(out)
}

/// Write subpaths back as path data (`L` for lines, `C` for curves).
pub(crate) fn to_d(subpaths: &[SubPath], keep_lines: bool) -> String {
    let n = crate::values::path_number;
    let mut d = String::new();
    for sub in subpaths {
        let s = sub.start();
        d.push_str(&format!("M{} {}", n(s.0), n(s.1)));
        for seg in &sub.segs {
            if keep_lines && seg.line {
                d.push_str(&format!(" L{} {}", n(seg.p[3].0), n(seg.p[3].1)));
            } else {
                let [_, a, b, e] = seg.p;
                d.push_str(&format!(
                    " C{} {} {} {} {} {}",
                    n(a.0),
                    n(a.1),
                    n(b.0),
                    n(b.1),
                    n(e.0),
                    n(e.1)
                ));
            }
        }
        if sub.closed {
            d.push_str(" Z");
        }
        d.push(' ');
    }
    d.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_split_and_write() {
        let subs = parse("M0 0 H10 Q10 10 0 10 Z M20 20 L30 20", &|p| p).unwrap();
        assert_eq!(subs.len(), 2);
        assert!(subs[0].closed && !subs[1].closed);
        assert_eq!(subs[0].segs.len(), 3, "line, quad and closing line");
        let (a, b) = subs[0].segs[1].split(0.5);
        assert_eq!(a.end(), b.start());
        let sub = subs[0].segs[1].sub(0.25, 0.75);
        let mid = subs[0].segs[1].eval(0.5);
        assert!(dist(sub.eval(0.5), mid) < 1e-9);
        assert_eq!(to_d(&subs[1..], true), "M20 20 L30 20");
        assert!(subs[0].area().abs() > 50.);
    }
}
