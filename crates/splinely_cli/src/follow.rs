//! Pointer followers (`follow --target pupil --max 6`): shapes or bones that
//! track the pointer on top of clips, states and interactions.
use crate::{failure, load, project_value, save_project, validated, Result};
use serde_json::{json, Value};

pub struct Options {
    pub target: String,
    pub mode: String,
    pub max: Option<f64>,
    pub strength: f64,
    pub smoothing: f64,
    pub rest_angle: f64,
    pub origin: Option<String>,
    pub machine: Option<String>,
    pub states: Vec<String>,
    pub id: Option<String>,
    pub remove: bool,
}

pub fn run(input: &str, output: &str, o: Options) -> Result<Value> {
    let project = load(input)?;
    let exists = project.nodes.iter().any(|n| n.id == o.target)
        || project.bones.iter().any(|b| b.id == o.target);
    if !exists && !o.remove {
        return Err(crate::assist::not_found(
            "node or bone",
            &o.target,
            project
                .nodes
                .iter()
                .map(|n| n.id.as_str())
                .chain(project.bones.iter().map(|b| b.id.as_str())),
        ));
    }
    let mut value = project_value(&project)?;
    let followers = value
        .as_object_mut()
        .expect("object")
        .entry("followers")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("array");
    if o.remove {
        let before = followers.len();
        followers.retain(|f| match &o.id {
            Some(id) => f["id"] != id.as_str(),
            None => f["target"] != o.target.as_str(),
        });
        if followers.len() == before {
            return Err(
                failure("not_found", format!("No follower for {}", o.target))
                    .hint("List followers with `inspect` (field followers)."),
            );
        }
        let project = validated(value)?;
        return save_project(&project, output);
    }
    let mode = match o.mode.to_ascii_lowercase().as_str() {
        "offset" | "move" => "offset",
        "rotate" | "turn" => "rotate",
        other => {
            return Err(
                failure("invalid_arguments", format!("Unknown mode {other:?}"))
                    .hint("Use --mode offset (pupils) or --mode rotate (heads, arms)."),
            )
        }
    };
    let max = o.max.unwrap_or(if mode == "rotate" { 30.0 } else { 10.0 });
    let mut follower = json!({
        "id": o.id.clone().unwrap_or_else(|| format!("follow-{}", o.target)),
        "target": o.target,
        "mode": mode,
        "strength": o.strength,
        "max": max,
        "smoothing": o.smoothing,
    });
    if o.rest_angle != 0.0 {
        follower["restAngle"] = json!(o.rest_angle);
    }
    if let Some(origin) = &o.origin {
        let numbers: Vec<f64> = origin
            .split(',')
            .map(|v| crate::bones::number(v.trim()))
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| failure("invalid_arguments", e))?;
        let [x, y] = numbers[..] else {
            return Err(
                failure("invalid_arguments", "--origin needs X,Y").hint("e.g. --origin 120,80")
            );
        };
        follower["origin"] = json!([x, y]);
    }
    match (&o.machine, o.states.is_empty()) {
        (Some(machine), false) => {
            follower["when"] = json!({"machine": machine, "states": o.states})
        }
        (None, true) => {}
        _ => {
            return Err(
                failure("invalid_arguments", "--machine and --states go together")
                    .hint("e.g. --machine hover --states in"),
            )
        }
    }
    let id = follower["id"].clone();
    followers.retain(|f| f["id"] != id);
    followers.push(follower);
    let project = validated(value)?;
    let mut result = save_project(&project, output)?;
    if output != "-" {
        result["follower"] = id;
    }
    Ok(result)
}
