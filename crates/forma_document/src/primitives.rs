//! Generated primitive shapes (regular polygon, star, line). They are stored as
//! ordinary editable paths, so every tool, export and runtime supports them.
use crate::*;

fn fmt(v: f64) -> String {
    let rounded = (v * 1000.).round() / 1000.;
    if rounded == 0. {
        "0".into()
    } else {
        format!("{rounded}")
    }
}

fn path_node(id: impl Into<String>, d: String) -> Result<Node, DocumentError> {
    let bounds = crate::path_bounds(&d)?;
    Ok(Node::new(
        id,
        NodeKind::Path {
            d,
            bounds,
            evenodd: false,
        },
    ))
}

fn ring(points: &[(f64, f64)]) -> String {
    let mut d = String::new();
    for (i, (x, y)) in points.iter().enumerate() {
        d.push_str(if i == 0 { "M" } else { " L" });
        d.push_str(&format!("{} {}", fmt(*x), fmt(*y)));
    }
    d.push_str(" Z");
    d
}

impl Node {
    /// Regular polygon with `sides` corners (3..=64), first corner pointing up.
    pub fn polygon(
        id: impl Into<String>,
        cx: f64,
        cy: f64,
        radius: f64,
        sides: u32,
    ) -> Result<Self, DocumentError> {
        if !(3..=64).contains(&sides) || !(radius.is_finite() && radius > 0.) {
            return Err(DocumentError(
                "polygon needs 3..64 sides and a positive radius".into(),
            ));
        }
        let points: Vec<_> = (0..sides)
            .map(|i| {
                let a =
                    (i as f64 / sides as f64) * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
                (cx + radius * a.cos(), cy + radius * a.sin())
            })
            .collect();
        path_node(id, ring(&points))
    }

    /// Star with `points` tips (3..=64); `inner` is the inner radius ratio (0..1).
    pub fn star(
        id: impl Into<String>,
        cx: f64,
        cy: f64,
        radius: f64,
        points: u32,
        inner: f64,
    ) -> Result<Self, DocumentError> {
        if !(3..=64).contains(&points)
            || !(radius.is_finite() && radius > 0.)
            || !(inner > 0. && inner < 1.)
        {
            return Err(DocumentError(
                "star needs 3..64 points, a positive radius and an inner ratio in (0, 1)".into(),
            ));
        }
        let corners: Vec<_> = (0..points * 2)
            .map(|i| {
                let r = if i % 2 == 0 { radius } else { radius * inner };
                let a = (i as f64 / (points * 2) as f64) * std::f64::consts::TAU
                    - std::f64::consts::FRAC_PI_2;
                (cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        path_node(id, ring(&corners))
    }

    /// Straight stroked line without fill (black, 2 units wide).
    pub fn line(
        id: impl Into<String>,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    ) -> Result<Self, DocumentError> {
        if (x1 - x2).abs() < 1e-9 && (y1 - y2).abs() < 1e-9 {
            return Err(DocumentError("line needs two different points".into()));
        }
        let mut node = path_node(
            id,
            format!("M{} {} L{} {}", fmt(x1), fmt(y1), fmt(x2), fmt(y2)),
        )?;
        node.style.fill.a = 0.;
        node.style.stroke = Some(Color::rgb(0, 0, 0));
        node.style.stroke_width = 2.;
        Ok(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_have_expected_bounds_and_validate() {
        let hex = Node::polygon("h", 50., 50., 50., 6).unwrap();
        let NodeKind::Path { bounds, d, .. } = &hex.kind else {
            panic!()
        };
        assert_eq!(d.matches('L').count(), 5);
        assert!((bounds[1] - 0.).abs() < 1e-9 && (bounds[3] - 100.).abs() < 1e-9);
        let star = Node::star("s", 0., 0., 10., 5, 0.5).unwrap();
        let NodeKind::Path { d, .. } = &star.kind else {
            panic!()
        };
        assert_eq!(d.matches('L').count(), 9);
        let line = Node::line("l", 0., 0., 30., 40.).unwrap();
        assert_eq!(line.style.fill.a, 0.);
        assert!(Node::polygon("x", 0., 0., 1., 2).is_err());
        assert!(Node::star("x", 0., 0., 1., 5, 1.).is_err());
        assert!(Node::line("x", 1., 1., 1., 1.).is_err());
        let project = Project {
            nodes: vec![hex, star, line],
            ..Project::default()
        };
        project.validate().unwrap();
    }
}
