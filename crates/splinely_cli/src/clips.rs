//! Clip management (`clip list/add/update/delete`) and interactions without
//! hand-written state machines (`on --event click --target ball --play travel`).
use crate::bones::number;
use crate::{failure, load, project_value, save_project, validated, Result};
use clap::Subcommand;
use splinely_document::{Easing, Project};
use serde_json::{json, Value};

/// CSS-like easing names, `cubic-bezier(x1,y1,x2,y2)` or four numbers.
pub fn easing(text: &str) -> std::result::Result<Easing, String> {
    let bezier = |x1, y1, x2, y2| Easing::CubicBezier { x1, y1, x2, y2 };
    let normalized = text.trim().to_lowercase().replace(['_', ' '], "-");
    Ok(match normalized.as_str() {
        "linear" => Easing::Linear,
        "step" | "hold" | "steps" => Easing::Step,
        "ease" => bezier(0.25, 0.1, 0.25, 1.),
        "ease-in" | "easein" => bezier(0.42, 0., 1., 1.),
        "ease-out" | "easeout" => bezier(0., 0., 0.58, 1.),
        "ease-in-out" | "easeinout" => bezier(0.42, 0., 0.58, 1.),
        _ => {
            let compact: String = text.to_lowercase().split_whitespace().collect();
            let inner = compact
                .trim_start_matches("cubic-bezier")
                .trim_start_matches("cubicbezier")
                .trim_matches(|c| c == '(' || c == ')');
            let numbers: Vec<f64> = inner
                .split(',')
                .map(|s| s.trim().parse::<f64>())
                .collect::<std::result::Result<_, _>>()
                .map_err(|_| EASING_HELP.to_string())?;
            let [x1, y1, x2, y2] = numbers[..] else {
                return Err(EASING_HELP.into());
            };
            if [x1, y1, x2, y2].iter().any(|v| !(0.0..=1.0).contains(v)) {
                return Err("cubic-bezier controls must be within 0..1".into());
            }
            bezier(x1, y1, x2, y2)
        }
    })
}

const EASING_HELP: &str =
    "Unknown easing; use linear, step, ease, ease-in, ease-out, ease-in-out or cubic-bezier(x1,y1,x2,y2)";

/// `--loop`, `--loop true` and `--loop false` all work.
pub fn flag(text: &str) -> std::result::Result<bool, String> {
    match text.to_lowercase().as_str() {
        "true" | "yes" | "1" | "on" => Ok(true),
        "false" | "no" | "0" | "off" => Ok(false),
        _ => Err("use true or false".into()),
    }
}

#[derive(Subcommand)]
pub enum ClipCommand {
    /// List clips with duration, flags, animated targets and who starts them.
    List { input: String },
    /// Create an empty clip (keyframes are added with `keyframe`).
    Add {
        input: String,
        /// New globally unique clip ID.
        #[arg(long, alias = "clip")]
        id: String,
        /// Length in seconds (default 1; `keyframe` extends it automatically).
        #[arg(long, default_value_t = 1.0, value_parser = number)]
        duration: f64,
        /// Start automatically when the SVG opens.
        #[arg(long, num_args = 0..=1, default_missing_value = "true", value_parser = flag)]
        autoplay: Option<bool>,
        /// Repeat forever.
        #[arg(long = "loop", num_args = 0..=1, default_missing_value = "true", value_parser = flag)]
        looping: Option<bool>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        output: String,
    },
    /// Change duration, autoplay, loop or name of a clip.
    Update {
        input: String,
        #[arg(long, alias = "clip")]
        id: String,
        #[arg(long, value_parser = number)]
        duration: Option<f64>,
        #[arg(long, num_args = 0..=1, default_missing_value = "true", value_parser = flag)]
        autoplay: Option<bool>,
        #[arg(long = "loop", num_args = 0..=1, default_missing_value = "true", value_parser = flag)]
        looping: Option<bool>,
        #[arg(long)]
        name: Option<String>,
        /// Set the easing of every keyframe in the clip.
        #[arg(long, value_parser = easing)]
        easing: Option<Easing>,
        #[arg(long)]
        output: String,
    },
    /// Delete a clip; fails while state actions still play/stop/blend it.
    Delete {
        input: String,
        #[arg(long, alias = "clip")]
        id: String,
        #[arg(long)]
        output: String,
    },
}

fn clip_index(project: &Project, id: &str) -> Result<usize> {
    project
        .clips
        .iter()
        .position(|c| c.id == id)
        .ok_or_else(|| {
            crate::assist::not_found("clip", id, project.clips.iter().map(|c| c.id.as_str()))
        })
}

/// Actions (machine/transition) that start, stop or blend each clip.
pub fn clip_users(project: &Project) -> Value {
    let value = project_value(project).unwrap_or(Value::Null);
    let mut users = serde_json::Map::new();
    let mut note = |clip: &str, text: String| {
        users
            .entry(clip.to_owned())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap()
            .push(json!(text));
    };
    for machine in value["stateMachines"].as_array().into_iter().flatten() {
        let mid = machine["id"].as_str().unwrap_or("");
        for t in machine["transitions"].as_array().into_iter().flatten() {
            for a in t["actions"].as_array().into_iter().flatten() {
                if let Some(clip) = a["clipId"].as_str() {
                    let target = t["event"]["target"]
                        .as_str()
                        .map(|t| format!(" on {t}"))
                        .unwrap_or_default();
                    note(
                        clip,
                        format!(
                            "{} in {mid}: {}{target} ({} → {})",
                            a["type"].as_str().unwrap_or(""),
                            t["event"]["name"].as_str().unwrap_or(""),
                            t["from"].as_str().unwrap_or(""),
                            t["to"].as_str().unwrap_or("")
                        ),
                    );
                }
            }
        }
        for s in machine["states"].as_array().into_iter().flatten() {
            for a in s["onEnter"].as_array().into_iter().flatten() {
                if let Some(clip) = a["clipId"].as_str() {
                    note(
                        clip,
                        format!(
                            "{} in {mid}: entering state {}",
                            a["type"].as_str().unwrap_or(""),
                            s["id"].as_str().unwrap_or("")
                        ),
                    );
                }
            }
        }
    }
    Value::Object(users)
}

pub fn run(command: ClipCommand) -> Result<Value> {
    match command {
        ClipCommand::List { input } => {
            let project = load(&input)?;
            let users = clip_users(&project);
            let clips: Vec<Value> = project
                .clips
                .iter()
                .map(|c| {
                    json!({
                        "id": c.id, "name": c.name, "duration": c.duration, "autoplay": c.autoplay,
                        "loop": c.looping, "blendMode": c.blend_mode, "weight": c.weight,
                        "tracks": c.tracks.iter().map(|t| json!({
                            "target": t.node_id, "property": t.property,
                            "times": t.keyframes.iter().map(|k| k.time).collect::<Vec<_>>(),
                        })).collect::<Vec<_>>(),
                        "startedBy": users.get(&c.id).cloned().unwrap_or(json!([])),
                    })
                })
                .collect();
            Ok(json!({"ok": true, "clips": clips}))
        }
        ClipCommand::Add {
            input,
            id,
            duration,
            autoplay,
            looping,
            name,
            output,
        } => {
            let mut project = load(&input)?;
            if project.clips.iter().any(|c| c.id == id) {
                return Err(
                    failure("invalid_arguments", format!("Clip '{id}' already exists"))
                        .hint(format!("Use `clip update FILE --id {id} …` to change it.")),
                );
            }
            project.clips.push(splinely_document::Clip {
                id: id.clone(),
                name: name.unwrap_or_else(|| id.clone()),
                duration,
                looping: looping.unwrap_or(false),
                autoplay: autoplay.unwrap_or(false),
                blend_mode: splinely_document::BlendMode::Override,
                weight: 1.0,
                tracks: vec![],
            });
            let mut result = save_project(&project, &output)?;
            result["clip"] = json!(id);
            Ok(result)
        }
        ClipCommand::Update {
            input,
            id,
            duration,
            autoplay,
            looping,
            name,
            easing,
            output,
        } => {
            let mut project = load(&input)?;
            let index = clip_index(&project, &id)?;
            let clip = &mut project.clips[index];
            if let Some(duration) = duration {
                let last = clip
                    .tracks
                    .iter()
                    .flat_map(|t| t.keyframes.iter().map(|k| k.time))
                    .fold(0.0_f64, f64::max);
                if duration < last {
                    return Err(failure(
                        "invalid_arguments",
                        format!(
                            "Duration {duration} is shorter than the last keyframe at {last} s"
                        ),
                    )
                    .hint("Move or delete later keyframes first, or choose a longer duration."));
                }
                clip.duration = duration;
            }
            if let Some(v) = autoplay {
                clip.autoplay = v;
            }
            if let Some(v) = looping {
                clip.looping = v;
            }
            if let Some(v) = name {
                clip.name = v;
            }
            if let Some(e) = easing {
                for key in clip.tracks.iter_mut().flat_map(|t| t.keyframes.iter_mut()) {
                    key.easing = e;
                }
            }
            save_project(&project, &output)
        }
        ClipCommand::Delete { input, id, output } => {
            let mut project = load(&input)?;
            let index = clip_index(&project, &id)?;
            if let Some(users) = clip_users(&project).get(&id) {
                return Err(failure(
                    "invalid_arguments",
                    format!("Clip '{id}' is still used: {users}"),
                )
                .hint("Remove those actions first (edit stateMachines with `apply`)."));
            }
            project.clips.remove(index);
            save_project(&project, &output)
        }
    }
}

/// Known browser/runtime events for `on --event`.
pub const EVENTS: &[&str] = &[
    "click",
    "dblclick",
    "pointerenter",
    "pointerleave",
    "pointerdown",
    "pointerup",
    "pointermove",
    "wheel",
    "keydown",
    "keyup",
    "focus",
    "blur",
    "dragstart",
    "drag",
    "dragend",
    "clip.end",
];

/// One `--set NODE.PROPERTY=VALUE` assignment as a setProperty action.
fn set_action(project: &Project, spec: &str) -> Result<Value> {
    let usage = || {
        failure("invalid_arguments", format!("Bad --set {spec:?}"))
            .hint("Use NODE.PROPERTY=VALUE, e.g. ball.fill=red, ball.opacity=0.5 or ball.x=120")
    };
    let (target, value) = spec.split_once('=').ok_or_else(usage)?;
    let (node, property) = target.rsplit_once('.').ok_or_else(usage)?;
    if !project.nodes.iter().any(|n| n.id == node) {
        return Err(crate::assist::not_found(
            "node",
            node,
            project.nodes.iter().map(|n| n.id.as_str()),
        ));
    }
    let value = match property {
        "fill" | "stroke" => {
            let c = crate::shapes::parse_color(value.trim())
                .map_err(|e| failure("invalid_arguments", e))?;
            json!({"type": "color", "value": c})
        }
        _ => {
            let v = number(value.trim()).map_err(|e| failure("invalid_arguments", e))?;
            json!({"type": "number", "value": v})
        }
    };
    Ok(json!({"type": "setProperty", "nodeId": node, "property": property, "value": value}))
}

#[allow(clippy::too_many_arguments)]
pub fn on(
    input: &str,
    output: &str,
    event: &str,
    target: Option<String>,
    play: Vec<String>,
    stop: Vec<String>,
    set: Vec<String>,
    machine: &str,
    from: Option<String>,
    to: Option<String>,
) -> Result<Value> {
    let project = load(input)?;
    if let Some(t) = &target {
        if !project.nodes.iter().any(|n| &n.id == t) {
            return Err(crate::assist::not_found(
                "node",
                t,
                project.nodes.iter().map(|n| n.id.as_str()),
            ));
        }
    }
    let mut actions = vec![];
    for clip in &play {
        clip_index(&project, clip)?;
        actions.push(json!({"type": "playClip", "clipId": clip}));
    }
    for clip in &stop {
        clip_index(&project, clip)?;
        actions.push(json!({"type": "stopClip", "clipId": clip}));
    }
    for spec in &set {
        actions.push(set_action(&project, spec)?);
    }
    if actions.is_empty() && from == to {
        return Err(
            failure("invalid_arguments", "Nothing happens on this event")
                .hint("Add --play CLIP, --stop CLIP, --set NODE.PROPERTY=VALUE or --to STATE."),
        );
    }
    let mut value = project_value(&project)?;
    let machines = value["stateMachines"].as_array_mut().expect("array");
    let index = match machines.iter().position(|m| m["id"] == machine) {
        Some(i) => i,
        None => {
            machines.push(json!({"id": machine, "initial": "idle", "states": [{"id": "idle"}], "transitions": []}));
            machines.len() - 1
        }
    };
    let m = &mut machines[index];
    let initial = m["initial"].as_str().unwrap_or("idle").to_owned();
    let from = from.unwrap_or(initial);
    let to = to.unwrap_or_else(|| from.clone());
    for state in [&from, &to] {
        let states = m["states"].as_array_mut().expect("array");
        if !states.iter().any(|s| s["id"] == state.as_str()) {
            states.push(json!({"id": state}));
        }
    }
    m["transitions"].as_array_mut().expect("array").push(json!({
        "from": from, "to": to,
        "event": {"name": event, "target": target},
        "actions": actions,
    }));
    let project = validated(value)?;
    let mut result = save_project(&project, output)?;
    if output != "-" {
        result["machine"] = json!(machine);
        if !EVENTS.contains(&event) {
            result["warning"] = json!(format!(
                "'{event}' is not a built-in event; it only fires when emitted by an emit action or host code. Built-in: {}",
                EVENTS.join(", ")
            ));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easings_parse_names_and_beziers() {
        assert_eq!(easing("linear").unwrap(), Easing::Linear);
        assert_eq!(
            easing("Ease-In-Out").unwrap(),
            Easing::CubicBezier {
                x1: 0.42,
                y1: 0.,
                x2: 0.58,
                y2: 1.
            }
        );
        assert_eq!(
            easing("cubic-bezier(0.1, 0.2, 0.3, 0.4)").unwrap(),
            Easing::CubicBezier {
                x1: 0.1,
                y1: 0.2,
                x2: 0.3,
                y2: 0.4
            }
        );
        assert!(easing("bounce").is_err());
        assert!(easing("cubic-bezier(0,2,0,1)").is_err());
    }
}
