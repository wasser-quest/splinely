use crate::{failure, load, save_project, Result};
use splinely_document::{AnimValue, Clip, Easing, Keyframe, Property, Track};
use serde_json::Value;

pub fn property(source: &str) -> std::result::Result<Property, String> {
    let property: Property = serde_json::from_value(Value::String(source.into()))
        .map_err(|_| "Unknown property; use x, y, rotation, scaleX, scaleY, opacity, width, height, radius, fontSize, fill, stroke, gradient or d".to_string())?;
    Ok(property)
}

/// JSON name of a property ("scaleX").
pub fn name(property: Property) -> String {
    serde_json::to_value(property)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Optional clip and easing settings applied together with a keyframe.
#[derive(Default)]
pub struct KeyOptions {
    pub easing: Option<Easing>,
    pub autoplay: Option<bool>,
    pub looping: Option<bool>,
    /// The numeric value is an offset from the target's current static value.
    pub relative: bool,
    /// Do not add a key at 0 s holding the current value for a new track.
    pub no_start_key: bool,
}

/// Static (unanimated) value of a node or bone property.
fn base_value(
    project: &splinely_document::Project,
    target: &str,
    property: Property,
) -> Option<AnimValue> {
    if let Some(node) = project.nodes.iter().find(|n| n.id == target) {
        return node.property_value(property);
    }
    let bone = project.bones.iter().find(|b| b.id == target)?;
    match property {
        Property::X => Some(AnimValue::Number(bone.x)),
        Property::Y => Some(AnimValue::Number(bone.y)),
        Property::Rotation => Some(AnimValue::Number(bone.rotation)),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    input: &str,
    output: &str,
    clip_id: &str,
    target: &str,
    property: Property,
    time: f64,
    value: AnimValue,
    options: KeyOptions,
) -> Result<Value> {
    let mut project = load(input)?;
    // Each property kind takes its own value flag.
    let expected = match property {
        Property::Gradient => "--gradient",
        Property::D => "--path",
        Property::Fill | Property::Stroke => "--color",
        _ => "--value",
    };
    let given = match value {
        AnimValue::Gradient(_) => "--gradient",
        AnimValue::Path(_) => "--path",
        AnimValue::Color(_) => "--color",
        AnimValue::Number(_) => "--value",
    };
    if expected != given {
        return Err(failure(
            "invalid_arguments",
            format!("Property {} needs {expected}, not {given}", name(property)),
        )
        .hint("Use --value for numbers, --color for fill/stroke, --gradient for gradient and --path for d."));
    }
    if !time.is_finite() || time < 0.0 || matches!(value, AnimValue::Number(v) if !v.is_finite()) {
        return Err(failure(
            "invalid_arguments",
            "Time must be finite and nonnegative; value must be finite",
        ));
    }
    if !project.nodes.iter().any(|node| node.id == target)
        && !project.bones.iter().any(|bone| bone.id == target)
    {
        return Err(crate::assist::not_found(
            "node or bone",
            target,
            project
                .nodes
                .iter()
                .map(|n| n.id.as_str())
                .chain(project.bones.iter().map(|b| b.id.as_str())),
        ));
    }
    let base = base_value(&project, target, property);
    let value = match (options.relative, &value, &base) {
        (true, AnimValue::Number(by), Some(AnimValue::Number(b))) => AnimValue::Number(b + by),
        (true, ..) => {
            return Err(failure(
                "invalid_arguments",
                "--by works only with numeric properties",
            ));
        }
        _ => value,
    };
    let index = match project.clips.iter().position(|clip| clip.id == clip_id) {
        Some(index) => index,
        None => {
            project.clips.push(Clip {
                id: clip_id.into(),
                name: clip_id.into(),
                duration: time.max(0.001),
                looping: false,
                autoplay: false,
                blend_mode: splinely_document::BlendMode::Override,
                weight: 1.0,
                tracks: Vec::new(),
            });
            project.clips.len() - 1
        }
    };
    let mut started = false;
    let clip = &mut project.clips[index];
    clip.duration = clip.duration.max(time).max(0.001);
    if let Some(track) = clip
        .tracks
        .iter_mut()
        .find(|track| track.node_id == target && track.property == property)
    {
        match track
            .keyframes
            .binary_search_by(|key| key.time.partial_cmp(&time).expect("validated finite time"))
        {
            Ok(index) => {
                track.keyframes[index].value = value;
                if let Some(easing) = options.easing {
                    track.keyframes[index].easing = easing;
                }
            }
            Err(index) => track.keyframes.insert(
                index,
                Keyframe {
                    time,
                    value,
                    easing: options.easing.unwrap_or(Easing::Linear),
                },
            ),
        }
    } else {
        let mut keyframes = vec![];
        // A new track starts from the current look, so one key already moves.
        if time > 1e-9 && !options.no_start_key {
            if let Some(start) = base.clone().filter(|b| b.compatible(&value)) {
                keyframes.push(Keyframe {
                    time: 0.0,
                    value: start,
                    easing: options.easing.unwrap_or(Easing::Linear),
                });
                started = true;
            }
        }
        keyframes.push(Keyframe {
            time,
            value,
            easing: options.easing.unwrap_or(Easing::Linear),
        });
        clip.tracks.push(Track {
            node_id: target.into(),
            property,
            keyframes,
        });
    }
    if let Some(v) = options.autoplay {
        clip.autoplay = v;
    }
    if let Some(v) = options.looping {
        clip.looping = v;
    }
    let (autoplay, keys) = (
        clip.autoplay,
        clip.tracks
            .iter()
            .find(|t| t.node_id == target && t.property == property)
            .map_or(0, |t| t.keyframes.len()),
    );
    // Validates IDs, bone restrictions, node kinds and numeric ranges before writing.
    let mut result = save_project(&project, output)?;
    if output != "-" {
        let mut hints = vec![];
        if started {
            hints.push(format!("Added a starting keyframe at 0 s with the current {} (use --no-start-key to skip).", name(property)));
        }
        let pivot_matters = matches!(
            property,
            Property::Rotation | Property::ScaleX | Property::ScaleY
        );
        if let Some(node) = project
            .nodes
            .iter()
            .find(|n| n.id == target)
            .filter(|_| pivot_matters)
        {
            if !crate::shapes::pivot_is_centred(node) {
                hints.push(format!("{} turns/scales around its local origin (0,0), not its centre. For a centred effect run `node update FILE {target} --pivot center` first.", name(property)));
            }
        }
        if keys == 1 {
            hints.push(format!(
                "Only one keyframe for {target}.{} so far; add a second time to create motion.",
                name(property)
            ));
        }
        let started = crate::clips::clip_users(&project).get(clip_id).is_some();
        if !autoplay && !started {
            hints.push(format!("Clip '{clip_id}' does not play yet: add --autoplay (or `clip update FILE --id {clip_id} --autoplay`), or start it with `on FILE --event click --target NODE --play {clip_id}`."));
        }
        if !hints.is_empty() {
            result["hints"] = serde_json::json!(hints);
        }
    }
    Ok(result)
}
