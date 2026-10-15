//! Deterministic arc-length motion shared by editor, CLI and native hosts.
//! Curves use 64 chords per segment, identically to the browser runtime.
use crate::geometry::{apply as map_point, dist, matrix, parse};
use crate::*;

pub(crate) fn validate(
    project: &Project,
    nodes: &std::collections::HashMap<&str, &Node>,
) -> Result<(), DocumentError> {
    for node in &project.nodes {
        let Some(binding) = &node.motion_path else {
            continue;
        };
        if !binding.progress.is_finite() || !(0.0..=1.0).contains(&binding.progress) {
            return Err(DocumentError("motion progress must be in 0..1".into()));
        }
        let path = nodes
            .get(binding.path.as_str())
            .copied()
            .ok_or_else(|| DocumentError(format!("unknown motion path {}", binding.path)))?;
        if path.id == node.id
            || path.motion_path.is_some()
            || path.bone_id.is_some()
            || node.bone_id.is_some()
        {
            return Err(DocumentError("motion targets and source paths must be unbound; chained motion paths are unsupported".into()));
        }
        let NodeKind::Path { d, .. } = &path.kind else {
            return Err(DocumentError(
                "motion path source must be a path node".into(),
            ));
        };
        sample(d, &path.transform, binding.progress)?;
    }
    Ok(())
}

fn sample(d: &str, transform: &Transform, progress: f64) -> Result<([f64; 2], f64), DocumentError> {
    // Deliberately the same bounded syntax as the path editor, not browser DOM geometry.
    if d.chars()
        .any(|c| c.is_ascii_alphabetic() && !"MmLlQqCcZzeE".contains(c))
    {
        return Err(DocumentError("motion paths support M/L/Q/C/Z only".into()));
    }
    let m = matrix(transform);
    let subs = parse(d, &|p| map_point(m, p))?;
    let mut chords = Vec::new();
    let mut total = 0.;
    for sub in subs {
        for seg in sub.segs {
            let mut a = seg.start();
            for i in 1..=64 {
                let b = seg.eval(i as f64 / 64.);
                let length = dist(a, b);
                if length > 1e-12 {
                    total += length;
                    chords.push((a, b, length));
                    if chords.len() > 1_000_000 {
                        return Err(DocumentError("motion path too complex".into()));
                    }
                }
                a = b;
            }
        }
    }
    if !total.is_finite() || total <= 1e-12 || chords.len() > 1_000_000 {
        return Err(DocumentError(
            "motion path must have finite nonzero length (at most 1000000 chords)".into(),
        ));
    }
    let mut distance = progress.clamp(0., 1.) * total;
    let last = chords.len() - 1;
    for (i, (a, b, length)) in chords.into_iter().enumerate() {
        // Roundoff from accumulated chord lengths must not flip the tangent
        // at an exact corner. Keep this relative tolerance identical in JS.
        if distance <= length + total * 1e-12 || i == last {
            let t = (distance / length).clamp(0., 1.);
            return Ok((
                [a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t],
                (b.1 - a.1).atan2(b.0 - a.0).to_degrees(),
            ));
        }
        distance -= length;
    }
    unreachable!()
}

pub(crate) fn apply(nodes: &mut [Node]) -> Result<(), DocumentError> {
    let sources = nodes.to_vec();
    for node in nodes {
        let Some(binding) = &node.motion_path else {
            continue;
        };
        let path = sources
            .iter()
            .find(|n| n.id == binding.path)
            .expect("validated path");
        let NodeKind::Path { d, .. } = &path.kind else {
            unreachable!()
        };
        let (point, angle) = sample(d, &path.transform, binding.progress)?;
        node.transform.x += point[0];
        node.transform.y += point[1];
        if binding.orient {
            node.transform.rotation += angle;
        }
    }
    Ok(())
}
