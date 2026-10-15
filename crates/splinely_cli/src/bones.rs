use crate::{failure, load, project_value, save_project, validated, Result};
use clap::{Subcommand, ValueEnum};
use splinely_document::{Node, Project};
use serde_json::{json, Value};

// Clap must not silently round integer tokens before model validation sees them.
pub fn number(source: &str) -> std::result::Result<f64, String> {
    crate::reject_oversized_integers(source).map_err(|e| e.message)?;
    let value: Value = serde_json::from_str(source).map_err(|e| e.to_string())?;
    let number = value
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or("Expected a finite JSON number")?;
    crate::check_preserved(&value, &json!(number), "argument").map_err(|e| e.message)?;
    Ok(number)
}

#[derive(Clone, Copy, ValueEnum)]
pub enum BoneProperty {
    X,
    Y,
    Rotation,
}
impl BoneProperty {
    fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Rotation => "rotation",
        }
    }
}

#[derive(Subcommand)]
pub enum BoneCommand {
    /// Add a root or child bone; child offsets are relative to the parent tip.
    Add {
        input: String,
        #[arg(long)]
        id: String,
        #[arg(long)]
        parent: Option<String>,
        #[arg(long, default_value = "0", value_parser = number, allow_hyphen_values = true)]
        x: f64,
        #[arg(long, default_value = "0", value_parser = number, allow_hyphen_values = true)]
        y: f64,
        #[arg(long, default_value = "0", value_parser = number, allow_hyphen_values = true)]
        rotation: f64,
        #[arg(long, value_parser = number, allow_hyphen_values = true)]
        length: f64,
        #[arg(long)]
        output: String,
    },
    /// Update a bone. --root clears its parent; omitted fields stay unchanged.
    Update {
        input: String,
        #[arg(long)]
        id: String,
        #[arg(long, conflicts_with = "root")]
        parent: Option<String>,
        #[arg(long)]
        root: bool,
        #[arg(long, value_parser = number, allow_hyphen_values = true)]
        x: Option<f64>,
        #[arg(long, value_parser = number, allow_hyphen_values = true)]
        y: Option<f64>,
        #[arg(long, value_parser = number, allow_hyphen_values = true)]
        rotation: Option<f64>,
        #[arg(long, value_parser = number, allow_hyphen_values = true)]
        length: Option<f64>,
        #[arg(long)]
        output: String,
    },
    /// Bind a node to a bone without changing its authored local transform.
    Bind {
        input: String,
        #[arg(long, alias = "nodes")]
        node: String,
        #[arg(long)]
        bone: String,
        #[arg(long)]
        output: String,
    },
    /// Insert/replace a numeric keyframe in an existing clip, sorted by time.
    Keyframe {
        input: String,
        #[arg(long)]
        bone: String,
        #[arg(long)]
        clip: String,
        #[arg(long, value_enum, default_value = "rotation")]
        property: BoneProperty,
        #[arg(long, value_parser = number, allow_hyphen_values = true)]
        time: f64,
        #[arg(long, value_parser = number, allow_hyphen_values = true)]
        value: f64,
        #[arg(long)]
        output: String,
    },
}

fn find<'a>(value: &'a mut Value, collection: &str, id: &str) -> Result<&'a mut Value> {
    let ids: Vec<String> = value[collection]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i["id"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    value[collection]
        .as_array_mut()
        .and_then(|items| items.iter_mut().find(|item| item["id"] == id))
        .ok_or_else(|| {
            let what = collection.trim_end_matches('s');
            crate::assist::not_found(what, id, ids.iter().map(String::as_str))
        })
}
fn edit(input: &str) -> Result<Value> {
    project_value(&load(input)?)
}
fn save(value: Value, output: &str) -> Result<Value> {
    save_project(&validated(value)?, output)
}

pub fn run(command: BoneCommand) -> Result<Value> {
    match command {
        BoneCommand::Add {
            input,
            id,
            parent,
            x,
            y,
            rotation,
            length,
            output,
        } => {
            let mut value = edit(&input)?;
            value["bones"].as_array_mut().ok_or_else(|| failure("invalid_project", "Core bone schema is unavailable"))?
                .push(json!({"id":id,"parentId":parent,"x":x,"y":y,"rotation":rotation,"length":length}));
            save(value, &output)
        }
        BoneCommand::Update {
            input,
            id,
            parent,
            root,
            x,
            y,
            rotation,
            length,
            output,
        } => {
            let mut value = edit(&input)?;
            let bone = find(&mut value, "bones", &id)?;
            if root {
                bone["parentId"] = Value::Null;
            } else if let Some(parent) = parent {
                bone["parentId"] = json!(parent);
            }
            for (field, number) in [
                ("x", x),
                ("y", y),
                ("rotation", rotation),
                ("length", length),
            ] {
                if let Some(number) = number {
                    bone[field] = json!(number);
                }
            }
            save(value, &output)
        }
        BoneCommand::Bind {
            input,
            node,
            bone,
            output,
        } => {
            let mut value = edit(&input)?;
            find(&mut value, "bones", &bone)?;
            find(&mut value, "nodes", &node)?["boneId"] = json!(bone);
            save(value, &output)
        }
        BoneCommand::Keyframe {
            input,
            bone,
            clip,
            property,
            time,
            value: number,
            output,
        } => {
            let mut value = edit(&input)?;
            find(&mut value, "bones", &bone)?;
            let clip = find(&mut value, "clips", &clip)?;
            if time < 0.0 || time > clip["duration"].as_f64().unwrap_or(0.0) {
                return Err(failure(
                    "invalid_time",
                    "Keyframe time must be within clip duration",
                ));
            }
            let tracks = clip["tracks"]
                .as_array_mut()
                .expect("validated clip tracks");
            let index = tracks
                .iter()
                .position(|t| t["nodeId"] == bone && t["property"] == property.name())
                .unwrap_or_else(|| {
                    tracks.push(json!({"nodeId":bone,"property":property.name(),"keyframes":[]}));
                    tracks.len() - 1
                });
            let keys = tracks[index]["keyframes"]
                .as_array_mut()
                .expect("validated keyframes");
            let key = json!({"time":time,"value":{"type":"number","value":number},"easing":{"type":"linear"}});
            if let Some(existing) = keys.iter_mut().find(|k| k["time"].as_f64() == Some(time)) {
                *existing = key;
            } else {
                keys.push(key);
            }
            keys.sort_by(|a, b| {
                a["time"]
                    .as_f64()
                    .unwrap()
                    .total_cmp(&b["time"].as_f64().unwrap())
            });
            save(value, &output)
        }
    }
}

pub fn rig_demo() -> Result<Project> {
    let mut project = Project {
        name: "Pivot-style bone demo".into(),
        width: 640.0,
        height: 480.0,
        ..Project::default()
    };
    project
        .nodes
        .push(Node::rect("background", 0.0, 0.0, 640.0, 480.0));
    let definitions = [
        ("torso", None, 320.0, 245.0, -90.0, 95.0),
        ("neck", Some("torso"), 0.0, 0.0, 0.0, 25.0),
        ("left-arm", Some("torso"), 0.0, 0.0, -135.0, 65.0),
        ("left-forearm", Some("left-arm"), 0.0, 0.0, -25.0, 55.0),
        ("right-arm", Some("torso"), 0.0, 0.0, 135.0, 65.0),
        ("right-forearm", Some("right-arm"), 0.0, 0.0, 25.0, 55.0),
        ("left-thigh", None, 320.0, 245.0, 115.0, 80.0),
        ("left-shin", Some("left-thigh"), 0.0, 0.0, -20.0, 75.0),
        ("right-thigh", None, 320.0, 245.0, 65.0, 80.0),
        ("right-shin", Some("right-thigh"), 0.0, 0.0, 20.0, 75.0),
    ];
    for (id, _, _, _, _, length) in definitions {
        project
            .nodes
            .push(Node::rect(format!("{id}-shape"), 0.0, -5.0, length, 10.0));
    }
    project.nodes.push(Node::circle("head", 25.0, 0.0, 22.0));
    let mut value = project_value(&project)?;
    value["bones"] = json!(definitions.iter().map(|(id, parent, x, y, rotation, length)|
        json!({"id":id,"parentId":parent,"x":x,"y":y,"rotation":rotation,"length":length})).collect::<Vec<_>>());
    value["nodes"][0]["style"]["fill"] = json!({"r":0.95,"g":0.96,"b":0.98,"a":1.0});
    for (index, (id, _, _, _, _, _)) in definitions.iter().enumerate() {
        value["nodes"][index + 1]["boneId"] = json!(id);
        value["nodes"][index + 1]["style"]["fill"] = json!({"r":0.12,"g":0.22,"b":0.4,"a":1.0});
    }
    value["nodes"][11]["boneId"] = json!("neck");
    value["clips"] = json!([{"id":"wave","name":"Wave","duration":2.0,"looping":true,"autoplay":true,
    "tracks":[{"nodeId":"right-forearm","property":"rotation","keyframes":[
        {"time":0.0,"value":{"type":"number","value":25.0}},
        {"time":1.0,"value":{"type":"number","value":-110.0}},
        {"time":2.0,"value":{"type":"number","value":25.0}}
    ]}]}]);
    validated(value)
}
