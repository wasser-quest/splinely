//! Shape morphing: make two arbitrary paths structurally identical (same
//! subpaths, same number of cubic segments, matched start points and winding)
//! so they can be interpolated number by number in Rust and in the runtime.
use crate::geometry::{apply, dist, invert, matrix, parse, to_d, Cubic, SubPath, P};
use crate::*;

/// Split the longest segments until the subpath has `count` segments.
fn subdivide_to(sub: &mut SubPath, count: usize) {
    while sub.segs.len() < count {
        let (index, _) = sub
            .segs
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.length().total_cmp(&b.1.length()))
            .unwrap();
        let (a, b) = sub.segs[index].split(0.5);
        sub.segs.splice(index..=index, [a, b]);
    }
}

/// A subpath collapsed to a single point with `count` segments.
fn point_subpath(at: P, count: usize, closed: bool) -> SubPath {
    SubPath {
        segs: vec![Cubic::curve(at, at, at, at); count.max(1)],
        closed,
    }
}

/// Rotate a closed subpath's start so its vertices best match `reference`.
fn best_rotation(sub: &SubPath, reference: &SubPath) -> SubPath {
    if !sub.closed || !reference.closed {
        return sub.clone();
    }
    let n = sub.segs.len();
    let cost = |k: usize| -> f64 {
        (0..n)
            .map(|i| {
                let a = reference.segs[i].start();
                let b = sub.segs[(i + k) % n].start();
                dist(a, b).powi(2)
            })
            .sum()
    };
    let k = (0..n)
        .min_by(|a, b| cost(*a).total_cmp(&cost(*b)))
        .unwrap_or(0);
    SubPath {
        segs: (0..n).map(|i| sub.segs[(i + k) % n]).collect(),
        closed: sub.closed,
    }
}

/// Make two path data strings morph-compatible. Returns both outlines with
/// identical command structure (all segments as cubic curves).
pub fn morph_paths(from: &str, to: &str) -> Result<(String, String), DocumentError> {
    let mut a = parse(from, &|p| p)?;
    let mut b = parse(to, &|p| p)?;
    if a.is_empty() || b.is_empty() {
        return Err(DocumentError("morphing needs two visible outlines".into()));
    }
    // Largest subpaths first so the main outlines pair up.
    a.sort_by(|x, y| y.area().abs().total_cmp(&x.area().abs()));
    b.sort_by(|x, y| y.area().abs().total_cmp(&x.area().abs()));
    // Missing subpaths grow out of (or shrink into) a point.
    while a.len() < b.len() {
        let other = &b[a.len()];
        a.push(point_subpath(
            other.centroid(),
            other.segs.len(),
            other.closed,
        ));
    }
    while b.len() < a.len() {
        let other = &a[b.len()];
        b.push(point_subpath(
            other.centroid(),
            other.segs.len(),
            other.closed,
        ));
    }
    for (sa, sb) in a.iter_mut().zip(b.iter_mut()) {
        let closed = sa.closed || sb.closed;
        sa.closed = closed;
        sb.closed = closed;
        // Same drawing direction, so the shape does not turn inside out.
        if sa.area().signum() != sb.area().signum() && sa.area() != 0. && sb.area() != 0. {
            *sb = sb.reversed();
        }
        let count = sa.segs.len().max(sb.segs.len());
        subdivide_to(sa, count);
        subdivide_to(sb, count);
        *sb = best_rotation(sb, sa);
    }
    Ok((to_d(&a, false), to_d(&b, false)))
}

fn local_path(node: &Node) -> Result<String, DocumentError> {
    match &node.kind {
        NodeKind::Path { d, .. } => Ok(d.clone()),
        NodeKind::Text { .. } => Err(DocumentError(format!(
            "node {}: convert text to a path before morphing",
            node.id
        ))),
        kind => match crate::conversion::generated_path(kind)? {
            NodeKind::Path { d, .. } => Ok(d),
            _ => unreachable!(),
        },
    }
}

impl Project {
    /// Animate `source` into the outline of `target` (as seen on the canvas).
    /// The source becomes a morph-ready path; a new autoplay clip of
    /// `duration` seconds holds the two shapes; the target is hidden.
    /// Returns the clip ID.
    pub fn create_morph(
        &mut self,
        source: &str,
        target: &str,
        duration: f64,
    ) -> Result<String, DocumentError> {
        if source == target {
            return Err(DocumentError("choose two different shapes to morph".into()));
        }
        if !(duration.is_finite() && duration > 0.) {
            return Err(DocumentError("morph duration must be positive".into()));
        }
        self.require_editable(&[source.to_owned()])?;
        let posed: std::collections::HashMap<String, Node> = self
            .authored()
            .evaluate_with_context(0., &Self::authored_context())?
            .into_iter()
            .map(|n| (n.id.clone(), n))
            .collect();
        let find = |id: &str| {
            self.nodes
                .iter()
                .find(|n| n.id == id)
                .ok_or_else(|| DocumentError(format!("missing node {id}")))
        };
        let (src, dst) = (find(source)?, find(target)?);
        let from = local_path(src)?;
        // Target outline in the source's local coordinates.
        let to_world = matrix(&posed[target].transform);
        let to_local = invert(matrix(&posed[source].transform))
            .ok_or_else(|| DocumentError(format!("node {source} has zero scale")))?;
        let target_subs = parse(&local_path(dst)?, &|p| apply(to_local, apply(to_world, p)))?;
        let (a, b) = morph_paths(&from, &to_d(&target_subs, false))?;
        let mut next = self.clone();
        let node = next.nodes.iter_mut().find(|n| n.id == source).unwrap();
        let evenodd = matches!(node.kind, NodeKind::Path { evenodd: true, .. });
        node.kind = NodeKind::Path {
            bounds: crate::path_bounds(&a)?,
            d: a.clone(),
            evenodd,
        };
        node.original_kind = None;
        next.nodes
            .iter_mut()
            .find(|n| n.id == target)
            .unwrap()
            .visible = false;
        let id = next.unused_id("morph");
        next.clips.push(Clip {
            id: id.clone(),
            name: "Morph".into(),
            duration,
            looping: false,
            autoplay: true,
            blend_mode: BlendMode::Override,
            weight: 1.,
            tracks: vec![Track {
                node_id: source.to_owned(),
                property: Property::D,
                keyframes: vec![
                    Keyframe {
                        time: 0.,
                        value: AnimValue::Path(a),
                        easing: Easing::CubicBezier {
                            x1: 0.42,
                            y1: 0.,
                            x2: 0.58,
                            y2: 1.,
                        },
                    },
                    Keyframe {
                        time: duration,
                        value: AnimValue::Path(b),
                        easing: Easing::Linear,
                    },
                ],
            }],
        });
        next.validate()?;
        *self = next;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_and_circle_become_compatible() {
        let square = "M0 0 H100 V100 H0 Z";
        let circle = crate::conversion::generated_path(&NodeKind::Circle {
            cx: 50.,
            cy: 50.,
            radius: 50.,
        })
        .unwrap();
        let NodeKind::Path { d: circle, .. } = circle else {
            panic!()
        };
        let (a, b) = morph_paths(square, &circle).unwrap();
        let (va, vb) = (AnimValue::Path(a.clone()), AnimValue::Path(b.clone()));
        assert!(va.compatible(&vb), "{a}\n{b}");
        // The ends still draw the original shapes.
        assert!((crate::path_bounds(&a).unwrap()[2] - 100.).abs() < 1e-6);
        assert!((crate::path_bounds(&b).unwrap()[2] - 100.).abs() < 0.01);
        // Halfway is a valid, in-between outline.
        let AnimValue::Path(mid) = va.interpolate(&vb, 0.5) else {
            panic!()
        };
        assert!(crate::path_bounds(&mid).is_ok());
    }

    #[test]
    fn different_subpath_counts_grow_from_points() {
        let (a, b) =
            morph_paths("M0 0 H10 V10 H0 Z", "M0 0 H10 V10 H0 Z M20 0 H30 V10 H20 Z").unwrap();
        assert_eq!(a.matches('M').count(), 2);
        assert!(AnimValue::Path(a).compatible(&AnimValue::Path(b)));
    }

    #[test]
    fn project_morph_creates_a_clip_in_source_coordinates() {
        let mut p = Project::default();
        let mut star = Node::star("star", 0., 0., 50., 5, 0.5).unwrap();
        star.transform.x = 300.;
        star.transform.y = 200.;
        p.nodes = vec![Node::rect("box", 10., 10., 80., 80.), star];
        let clip = p.create_morph("box", "star", 2.).unwrap();
        assert!(!p.nodes[1].visible, "target hidden");
        // At the end the source looks like the star, at the star's place.
        let end = p.evaluate(2.).unwrap();
        let b = crate::arrange::transformed_bounds(&end[0]);
        assert!((b[0] + b[2] / 2. - 300.).abs() < 1., "{b:?}");
        assert!((b[1] + b[3] / 2. - 200.).abs() < 5., "{b:?}");
        let start = p.evaluate(0.).unwrap();
        let b = crate::arrange::transformed_bounds(&start[0]);
        assert!(
            (b[0] - 10.).abs() < 1e-6 && (b[2] - 80.).abs() < 1e-6,
            "{b:?}"
        );
        assert!(p.clips.iter().any(|c| c.id == clip && c.autoplay));
        let json = p.to_json().unwrap();
        assert_eq!(Project::from_json(&json).unwrap(), p);
        assert!(p.create_morph("box", "box", 1.).is_err());
    }
}
