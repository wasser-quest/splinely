//! Deterministic arc-length motion shared by editor, CLI and native hosts.
//! Curves use 64 chords per segment, identically to the browser runtime.
//! Each distinct source path is flattened once per call and bindings locate
//! their point by binary search on long paths, so the work is O(chords + bindings).
//! The chord limits match the JS runtime's normalize() exactly.
use crate::geometry::{apply as map_point, dist, matrix, parse};
use crate::*;
use std::collections::HashMap;

/// Most sampled chords (64 per segment, zero-length ones included) in one motion path.
pub const MOTION_MAX_CHORDS: usize = 100_000;
/// Most sampled chords summed over the distinct motion source paths of a document,
/// counting each source's `d` plus every `d` keyframe and state override value
/// targeting it, independent of transforms. Also enforced per evaluation.
/// Real assets use none (tests: a few hundred).
pub const MOTION_TOTAL_CHORDS: usize = 100_000;
/// Paths with more chords locate by binary search (same chord as the walk, as in JS).
const SEARCH_FROM: usize = 1024;

type Chord = ((f64, f64), (f64, f64), f64);

pub(crate) fn validate(
    project: &Project,
    nodes: &HashMap<&str, &Node>,
) -> Result<(), DocumentError> {
    let mut sources: HashMap<&str, &Node> = HashMap::new();
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
        if !matches!(path.kind, NodeKind::Path { .. }) {
            return Err(DocumentError(
                "motion path source must be a path node".into(),
            ));
        }
        sources.insert(&path.id, path);
    }
    if sources.is_empty() {
        return Ok(());
    }
    // Sampling success does not depend on progress: flatten each source once.
    // Counted per segment, before length checks, so neither a tiny base scale
    // nor zero-length or unsampleable values slip past the budget.
    let mut total = 0usize;
    let mut count = |d: &str, transform: &Transform, strict: bool| {
        if strict {
            total += flatten(d, transform, &mut |_| ())?.2;
        } else {
            // Animated values that cannot be sampled fail at evaluation instead (as in JS).
            match sampled(d) {
                Ok(n) => total += n,
                Err(e) if e.0 == TOO_COMPLEX => return Err(e),
                Err(_) => {}
            }
        }
        if total > MOTION_TOTAL_CHORDS {
            return Err(DocumentError(format!(
                "motion paths too complex: more than {MOTION_TOTAL_CHORDS} chords"
            )));
        }
        Ok(())
    };
    for node in &project.nodes {
        if let (Some(_), NodeKind::Path { d, .. }) = (sources.get(node.id.as_str()), &node.kind) {
            count(d, &node.transform, true)?;
        }
    }
    let animated = project
        .clips
        .iter()
        .flat_map(|c| &c.tracks)
        .filter(|t| t.property == Property::D)
        .flat_map(|t| {
            t.keyframes
                .iter()
                .map(move |k| (t.node_id.as_str(), &k.value))
        })
        .chain(
            project
                .state_machines
                .iter()
                .flat_map(|m| &m.states)
                .flat_map(|s| &s.overrides)
                .filter(|o| o.property == Property::D)
                .map(|o| (o.node_id.as_str(), &o.value)),
        );
    for (id, value) in animated {
        if let (Some(source), AnimValue::Path(d)) = (sources.get(id), value) {
            count(d, &source.transform, false)?;
        }
    }
    Ok(())
}

const TOO_COMPLEX: &str = "motion path too complex";

/// Deliberately the same bounded syntax as the path editor, not browser DOM geometry.
fn check_syntax(d: &str) -> Result<(), DocumentError> {
    if d.chars()
        .any(|c| c.is_ascii_alphabetic() && !"MmLlQqCcZzeE".contains(c))
    {
        return Err(DocumentError("motion paths support M/L/Q/C/Z only".into()));
    }
    Ok(())
}

/// Sampled chords of `d`: 64 per drawn segment, zero-length ones included
/// (`parse` drops those later, JS samples them), independent of transforms.
fn sampled(d: &str) -> Result<usize, DocumentError> {
    use svgtypes::SimplePathSegment::*;
    check_syntax(d)?;
    let (mut start, mut current, mut n) = ((0., 0.), (0., 0.), 0usize);
    for segment in svgtypes::SimplifyingPathParser::from(d) {
        match segment.map_err(|e| DocumentError(format!("Invalid SVG path: {e}")))? {
            MoveTo { x, y } => (start, current) = ((x, y), (x, y)),
            LineTo { x, y } | Quadratic { x, y, .. } | CurveTo { x, y, .. } => {
                current = (x, y);
                n += 64;
            }
            ClosePath => {
                n += if dist(current, start) > 1e-9 { 64 } else { 0 };
                current = start;
            }
        }
        if n > MOTION_MAX_CHORDS {
            return Err(DocumentError(TOO_COMPLEX.into()));
        }
    }
    Ok(n)
}

/// Flatten `d` into chords, passing each nonzero one to `chord`;
/// returns (nonzero count, total length, sampled chords).
fn flatten(
    d: &str,
    transform: &Transform,
    chord: &mut dyn FnMut(Chord),
) -> Result<(usize, f64, usize), DocumentError> {
    let sampled = sampled(d)?;
    let m = matrix(transform);
    let subs = parse(d, &|p| map_point(m, p))?;
    let mut count = 0;
    let mut total = 0.;
    for sub in subs {
        for seg in sub.segs {
            let mut a = seg.start();
            for i in 1..=64 {
                let b = seg.eval(i as f64 / 64.);
                let length = dist(a, b);
                if length > 1e-12 {
                    total += length;
                    chord((a, b, length));
                    count += 1;
                }
                a = b;
            }
        }
    }
    if !total.is_finite() || total <= 1e-12 {
        return Err(DocumentError(format!(
            "motion path must have finite nonzero length (at most {MOTION_MAX_CHORDS} chords)"
        )));
    }
    Ok((count, total, sampled))
}

/// Point and tangent angle at `progress`; `cum` holds prefix lengths on long paths.
fn locate(chords: &[Chord], cum: &[f64], total: f64, progress: f64) -> ([f64; 2], f64) {
    let mut distance = progress.clamp(0., 1.) * total;
    let last = chords.len() - 1;
    // Roundoff from accumulated chord lengths must not flip the tangent
    // at an exact corner. Keep this relative tolerance identical in JS.
    let eps = total * 1e-12;
    let mut i = 0;
    if cum.is_empty() {
        while i < last && distance > chords[i].2 + eps {
            distance -= chords[i].2;
            i += 1;
        }
    } else {
        // First chord the walk above would stop at, as in the JS runtime.
        let mut hi = last;
        while i < hi {
            let mid = (i + hi) / 2;
            if distance - cum[mid] <= chords[mid].2 + eps {
                hi = mid;
            } else {
                i = mid + 1;
            }
        }
        distance -= cum[i];
    }
    let (a, b, length) = chords[i];
    let t = (distance / length).clamp(0., 1.);
    (
        [a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t],
        (b.1 - a.1).atan2(b.0 - a.0).to_degrees(),
    )
}

pub(crate) fn apply(nodes: &mut [Node]) -> Result<(), DocumentError> {
    // Bindings grouped by source index; sources are unbound, so targets never alias them.
    let index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .rev()
        .collect();
    let mut by_source: Vec<(usize, Vec<usize>)> = vec![];
    let mut slot: HashMap<usize, usize> = HashMap::new();
    for (i, node) in nodes.iter().enumerate() {
        let Some(binding) = &node.motion_path else {
            continue;
        };
        let source = *index.get(binding.path.as_str()).expect("validated path");
        let s = *slot.entry(source).or_insert_with(|| {
            by_source.push((source, vec![]));
            by_source.len() - 1
        });
        by_source[s].1.push(i);
    }
    drop(index);
    let mut budget = 0usize;
    for (source, targets) in by_source {
        let NodeKind::Path { d, .. } = &nodes[source].kind else {
            unreachable!()
        };
        // Animated `d` is flattened as evaluated; `flatten` caps each path.
        let mut chords = vec![];
        let (count, total, sampled) =
            flatten(d, &nodes[source].transform, &mut |c| chords.push(c))?;
        budget += sampled;
        if budget > MOTION_TOTAL_CHORDS {
            return Err(DocumentError(format!(
                "motion paths too complex: more than {MOTION_TOTAL_CHORDS} chords"
            )));
        }
        let mut cum = vec![];
        if count > SEARCH_FROM {
            cum.reserve(count + 1);
            cum.push(0.);
            for c in &chords {
                cum.push(cum[cum.len() - 1] + c.2);
            }
        }
        for i in targets {
            let node = &mut nodes[i];
            let binding = node.motion_path.as_ref().expect("bound node");
            let (point, angle) = locate(&chords, &cum, total, binding.progress);
            node.transform.x += point[0];
            node.transform.y += point[1];
            if binding.orient {
                node.transform.rotation += angle;
            }
        }
    }
    Ok(())
}
