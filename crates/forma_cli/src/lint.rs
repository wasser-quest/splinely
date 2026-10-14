//! `check`: finds valid-but-probably-wrong authoring (hidden shapes, clips that
//! never play, unreadable text …) and suggests the exact command that fixes it.
use crate::{failure, load, Result};
use forma_document::{AnimValue, Bounds, Color, NodeKind, Project, Property};
use serde_json::{json, Value};

struct Finding {
    level: &'static str,
    code: &'static str,
    message: String,
    fix: String,
}

fn contains(outer: Bounds, inner: Bounds) -> bool {
    outer[0] <= inner[0] + 1e-6
        && outer[1] <= inner[1] + 1e-6
        && outer[0] + outer[2] >= inner[0] + inner[2] - 1e-6
        && outer[1] + outer[3] >= inner[1] + inner[3] - 1e-6
}

fn overlaps(a: Bounds, b: Bounds) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}

/// Relative luminance (sRGB, channels 0..1).
fn luminance(c: &Color) -> f64 {
    let f = |v: f64| {
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
}

fn contrast(a: &Color, b: &Color) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

fn findings(project: &Project, file: &str) -> Vec<Finding> {
    let mut out = vec![];
    let mut add = |level, code, message: String, fix: String| {
        out.push(Finding {
            level,
            code,
            message,
            fix,
        })
    };
    if project.nodes.is_empty() {
        add(
            "warning",
            "empty_project",
            "The project has no shapes.".into(),
            format!("forma-cli node add {file} --kind rect --x 0 --y 0 --width {} --height {} --fill '#223344' --id background", project.width, project.height),
        );
    }
    let bounds = project.authored_bounds().unwrap_or_default();
    let animated = |id: &str, property: Option<Property>| {
        project.clips.iter().any(|c| {
            c.tracks
                .iter()
                .any(|t| t.node_id == id && property.is_none_or(|p| t.property == p))
        })
    };
    let canvas: Bounds = [0., 0., project.width, project.height];
    let users = crate::clips::clip_users(project);
    for (i, node) in project.nodes.iter().enumerate() {
        let Some(&b) = bounds.get(&node.id) else {
            continue;
        };
        let moving = animated(&node.id, None);
        if node.visible && !overlaps(b, canvas) && !moving && node.bone_id.is_none() {
            add(
                "warning",
                "outside_canvas",
                format!(
                    "'{}' lies completely outside the {}×{} canvas (bounds {:?}).",
                    node.id, project.width, project.height, b
                ),
                format!(
                    "forma-cli arrange move {file} --nodes {} --dx {} --dy {}",
                    node.id,
                    crate::shapes::fmt_number(project.width / 2. - (b[0] + b[2] / 2.)),
                    crate::shapes::fmt_number(project.height / 2. - (b[1] + b[3] / 2.))
                ),
            );
        }
        if node.visible
            && (node.style.opacity <= 0.001)
            && !animated(&node.id, Some(Property::Opacity))
        {
            add(
                "info",
                "invisible",
                format!(
                    "'{}' has opacity 0 and no opacity animation, so it never shows.",
                    node.id
                ),
                format!(
                    "forma-cli node update {file} --node {} --opacity 1",
                    node.id
                ),
            );
        }
        if node.visible
            && node.style.fill.a <= 0.001
            && node.style.stroke.is_none()
            && node.gradient.is_none()
        {
            add(
                "info",
                "no_paint",
                format!("'{}' has a transparent fill and no stroke.", node.id),
                format!(
                    "forma-cli node update {file} --node {} --fill '#4080ff'",
                    node.id
                ),
            );
        }
        // An opaque, unrotated rectangle above a shape that covers it entirely.
        if node.visible && !moving {
            let cover = project.nodes[i + 1..].iter().find(|above| {
                above.visible
                    && matches!(above.kind, NodeKind::Rect { .. })
                    && above.transform.rotation == 0.
                    && above.style.opacity >= 0.999
                    && above.style.fill.a >= 0.999
                    && above.bone_id.is_none()
                    && !animated(&above.id, None)
                    && bounds.get(&above.id).is_some_and(|ab| contains(*ab, b))
            });
            if let Some(cover) = cover {
                add(
                    "warning",
                    "hidden_behind",
                    format!("'{}' is completely covered by the opaque rectangle '{}' above it (nodes are drawn back to front).", node.id, cover.id),
                    format!("forma-cli arrange order {file} --nodes {} --to back", cover.id),
                );
            }
        }
        if let NodeKind::Text { font_size, .. } = &node.kind {
            if *font_size * node.transform.scale_y.abs() < 8. {
                add(
                    "info",
                    "tiny_text",
                    format!("Text '{}' is only {font_size} units high.", node.id),
                    format!(
                        "forma-cli node update {file} --node {} --font-size 24",
                        node.id
                    ),
                );
            }
            // Contrast against the topmost plain shape behind the text centre.
            let centre = [b[0] + b[2] / 2., b[1] + b[3] / 2., 0., 0.];
            let behind = project.nodes[..i].iter().rev().find(|n| {
                n.visible
                    && n.gradient.is_none()
                    && n.style.fill.a > 0.5
                    && bounds.get(&n.id).is_some_and(|nb| contains(*nb, centre))
            });
            if let Some(behind) = behind {
                if node.gradient.is_none() && !animated(&node.id, Some(Property::Fill)) {
                    let ratio = contrast(&node.style.fill, &behind.style.fill);
                    if ratio < 2.0 {
                        let light = luminance(&behind.style.fill) < 0.4;
                        add(
                            "warning",
                            "low_contrast",
                            format!(
                                "Text '{}' is hard to read on '{}' (contrast {ratio:.1}:1).",
                                node.id, behind.id
                            ),
                            format!(
                                "forma-cli node update {file} --node {} --fill {}",
                                node.id,
                                if light { "white" } else { "'#202020'" }
                            ),
                        );
                    }
                }
            }
        }
    }
    for clip in &project.clips {
        if clip.tracks.is_empty() {
            add(
                "warning",
                "empty_clip",
                format!("Clip '{}' has no keyframes.", clip.id),
                format!("forma-cli keyframe {file} --clip {} --target NODE --property x --time 0 --value 0", clip.id),
            );
        }
        if !clip.autoplay && users.get(&clip.id).is_none() {
            add(
                "warning",
                "clip_never_plays",
                format!("Clip '{}' is neither autoplay nor started by any interaction, so it never plays in the exported SVG. Alternative: forma-cli on {file} --event click --target NODE --play {}", clip.id, clip.id),
                format!("forma-cli clip update {file} --id {} --autoplay", clip.id),
            );
        }
        for track in &clip.tracks {
            if track.keyframes.len() == 1 {
                add(
                    "info",
                    "single_keyframe",
                    format!("Clip '{}' animates {}.{} with a single keyframe (no motion).", clip.id, track.node_id, crate::keyframes::name(track.property)),
                    format!("forma-cli keyframe {file} --clip {} --target {} --property {} --time {} --value VALUE", clip.id, track.node_id, crate::keyframes::name(track.property), clip.duration.max(1.)),
                );
            }
            let values: Vec<&AnimValue> = track.keyframes.iter().map(|k| &k.value).collect();
            if values.len() > 1 && values.windows(2).all(|w| w[0] == w[1]) {
                add(
                    "info",
                    "no_change",
                    format!("Clip '{}': every keyframe of {}.{} has the same value.", clip.id, track.node_id, crate::keyframes::name(track.property)),
                    format!("forma-cli keyframe {file} --clip {} --target {} --property {} --time {} --value VALUE", clip.id, track.node_id, crate::keyframes::name(track.property), track.keyframes.last().map_or(1., |k| k.time)),
                );
            }
            let node = project.nodes.iter().find(|n| n.id == track.node_id);
            if let Some(node) = node {
                if track.property == Property::Fill && node.gradient.is_some() {
                    add(
                        "warning",
                        "fill_under_gradient",
                        format!("Clip '{}' animates the fill of '{}', but its gradient replaces the fill. Alternative: animate --property gradient instead.", clip.id, node.id),
                        format!("forma-cli node update {file} --node {} --gradient none", node.id),
                    );
                }
                if !node.visible {
                    add(
                        "info",
                        "animates_hidden",
                        format!("Clip '{}' animates hidden node '{}'.", clip.id, node.id),
                        format!(
                            "forma-cli layer hide {file} --node {} --hidden false",
                            node.id
                        ),
                    );
                }
            }
        }
    }
    for machine in &project.state_machines {
        for state in &machine.states {
            let reachable =
                state.id == machine.initial || machine.transitions.iter().any(|t| t.to == state.id);
            if !reachable {
                add(
                    "info",
                    "unreachable_state",
                    format!("State '{}' of '{}' is never entered.", state.id, machine.id),
                    format!(
                        "forma-cli on {file} --machine {} --event click --target NODE --to {}",
                        machine.id, state.id
                    ),
                );
            }
        }
        for t in &machine.transitions {
            if let Some(target) = &t.event.target {
                if project.nodes.iter().any(|n| &n.id == target && !n.visible) {
                    add(
                        "warning",
                        "event_on_hidden",
                        format!("'{}' in '{}' listens on hidden node '{target}', which cannot be clicked.", t.event.name, machine.id),
                        format!("forma-cli layer hide {file} --node {target} --hidden false"),
                    );
                }
            }
        }
    }
    let moves = project
        .clips
        .iter()
        .any(|c| c.autoplay && !c.tracks.is_empty());
    if !project.clips.is_empty() && !moves && project.state_machines.is_empty() {
        add(
            "info",
            "static_on_open",
            "Nothing moves when the SVG opens and there are no interactions.".into(),
            format!(
                "forma-cli clip update {file} --id {} --autoplay --loop",
                project.clips[0].id
            ),
        );
    }
    out
}

pub fn run(input: &str, strict: bool) -> Result<Value> {
    let project = load(input)?;
    let found = findings(&project, input);
    let warnings = found.iter().filter(|f| f.level == "warning").count();
    let list: Vec<Value> = found
        .iter()
        .map(|f| json!({"level": f.level, "code": f.code, "message": f.message, "fix": f.fix}))
        .collect();
    if strict && warnings > 0 {
        return Err(failure(
            "check_failed",
            format!(
                "{warnings} warning(s): {}",
                serde_json::to_string(&list).unwrap_or_default()
            ),
        )
        .hint("Apply the suggested fixes, then run check again."));
    }
    Ok(json!({
        "ok": true,
        "valid": true,
        "warnings": warnings,
        "infos": found.len() - warnings,
        "findings": list,
        "fixNote": "Each fix is a shell command; UPPERCASE words (NODE, VALUE) are placeholders to fill in.",
        "summary": {
            "size": [project.width, project.height],
            "nodes": project.nodes.len(),
            "clips": project.clips.iter().map(|c| json!({"id": c.id, "duration": c.duration, "autoplay": c.autoplay, "loop": c.looping})).collect::<Vec<_>>(),
            "interactions": project.state_machines.iter().map(|m| m.transitions.len()).sum::<usize>(),
        },
        "next": if warnings == 0 { "Looks good. Render a preview: forma-cli render FILE --time 0 --output preview.png" } else { "Fix the warnings (each has a ready command), then run check again." },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use forma_document::Node;

    fn codes(project: &Project) -> Vec<&'static str> {
        findings(project, "f.frm").iter().map(|f| f.code).collect()
    }

    #[test]
    fn finds_common_authoring_mistakes() {
        let mut project = Project {
            width: 400.,
            height: 300.,
            ..Project::default()
        };
        assert_eq!(codes(&project), ["empty_project"]);
        let mut text = Node::text("label", 20., 40., "Hi");
        text.style.fill = Color::rgb(30, 30, 30);
        let mut background = Node::rect("bg", 0., 0., 400., 300.);
        background.style.fill = Color::rgb(20, 20, 20);
        // Background added last: covers the text, which is also unreadable.
        project.nodes = vec![Node::circle("far", 900., 900., 10.), text, background];
        let found = codes(&project);
        assert!(found.contains(&"outside_canvas"), "{found:?}");
        assert!(found.contains(&"hidden_behind"), "{found:?}");
        project.nodes.rotate_right(1);
        let found = codes(&project);
        assert!(!found.contains(&"hidden_behind"), "{found:?}");
        assert!(found.contains(&"low_contrast"), "{found:?}");
        project.clips.push(forma_document::Clip {
            id: "idle".into(),
            name: "idle".into(),
            duration: 1.,
            looping: false,
            autoplay: false,
            blend_mode: forma_document::BlendMode::Override,
            weight: 1.,
            tracks: vec![],
        });
        let found = codes(&project);
        assert!(
            found.contains(&"clip_never_plays") && found.contains(&"empty_clip"),
            "{found:?}"
        );
    }
}
