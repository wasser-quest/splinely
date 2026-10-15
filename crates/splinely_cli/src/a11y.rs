//! `a11y`: screen-reader data for the web player and exported SVG (project
//! title/description, per-node label/description/role/hidden, state
//! announcements). Without edit options it only reports what is set.
use crate::{failure, load, project_value, save_project, validated, Result};
use serde_json::{json, Map, Value};
use splinely_document::{A11yRole, Project};

pub struct Options {
    pub title: Option<String>,
    pub description: Option<String>,
    pub node: Option<String>,
    pub label: Option<String>,
    pub role: Option<String>,
    pub hidden: Option<bool>,
    pub announce: Vec<String>,
}

fn invalid(message: impl ToString) -> crate::Failure {
    failure("invalid_arguments", message)
}

/// Report for the current project: what is set and which interactive nodes still lack a label.
pub fn report(project: &Project) -> Value {
    let nodes: Vec<Value> = project
        .nodes
        .iter()
        .filter_map(|n| n.a11y.as_ref().map(|a| json!({"id": n.id, "a11y": a})))
        .collect();
    let announcements: Vec<Value> = project
        .state_machines
        .iter()
        .flat_map(|m| {
            m.states.iter().filter_map(|s| {
                s.announce
                    .as_ref()
                    .map(|text| json!({"machine": m.id, "state": s.id, "announce": text}))
            })
        })
        .collect();
    json!({
        "title": project.name,
        "description": project.description,
        "enabled": project.a11y_enabled(),
        "nodes": nodes,
        "announcements": announcements,
        "interactiveNodes": project.interactive_node_ids(),
        "interactiveWithoutLabel": project.unlabeled_interactive_nodes(),
    })
}

pub fn run(input: &str, output: Option<&str>, o: Options) -> Result<Value> {
    let project = load(input)?;
    let editing = o.title.is_some()
        || o.description.is_some()
        || o.label.is_some()
        || o.role.is_some()
        || o.hidden.is_some()
        || !o.announce.is_empty();
    if o.node.is_none() && (o.label.is_some() || o.role.is_some() || o.hidden.is_some()) {
        return Err(invalid("--label, --role and --hidden need --node ID")
            .hint("e.g. a11y FILE --node ball --label 'Start button'"));
    }
    if !editing {
        let mut result = report(&project);
        if let Some(id) = &o.node {
            if !project.nodes.iter().any(|n| &n.id == id) {
                return Err(not_found_node(&project, id));
            }
            result["nodes"] = json!(project
                .nodes
                .iter()
                .filter(|n| &n.id == id)
                .map(|n| json!({"id": n.id, "a11y": n.a11y}))
                .collect::<Vec<_>>());
        }
        return Ok(result);
    }
    let output = output.unwrap_or(input);
    if output == "-" || (input == "-" && output == input) {
        return Err(invalid("a11y needs a file to write; give --output FILE"));
    }
    let mut value = project_value(&project)?;
    let root = value.as_object_mut().expect("project object");
    if let Some(title) = &o.title {
        root.insert("name".into(), json!(title));
    }
    if o.node.is_none() {
        if let Some(text) = &o.description {
            if text.is_empty() {
                root.remove("description");
            } else {
                root.insert("description".into(), json!(text));
            }
        }
    }
    if let Some(id) = &o.node {
        if !project.nodes.iter().any(|n| &n.id == id) {
            return Err(not_found_node(&project, id));
        }
        let nodes = root
            .get_mut("nodes")
            .and_then(Value::as_array_mut)
            .expect("nodes");
        let node = nodes
            .iter_mut()
            .find(|n| n["id"] == id.as_str())
            .and_then(Value::as_object_mut)
            .expect("node exists");
        let mut a11y: Map<String, Value> = node
            .get("a11y")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut set = |key: &str, text: &Option<String>| {
            if let Some(text) = text {
                if text.is_empty() {
                    a11y.remove(key);
                } else {
                    a11y.insert(key.into(), json!(text));
                }
            }
        };
        set("label", &o.label);
        set("description", &o.description);
        if let Some(role) = &o.role {
            if !role.is_empty() && A11yRole::parse(role).is_none() {
                return Err(invalid(format!("Unknown role {role:?}")).hint(format!(
                    "Use one of: {} (or '' to clear).",
                    A11yRole::ALL.join(", ")
                )));
            }
        }
        set("role", &o.role);
        if let Some(hidden) = o.hidden {
            if hidden {
                a11y.insert("hidden".into(), json!(true));
            } else {
                a11y.remove("hidden");
            }
        }
        if a11y.is_empty() {
            node.remove("a11y");
        } else {
            node.insert("a11y".into(), Value::Object(a11y));
        }
    }
    for spec in &o.announce {
        let (target, text) = spec.split_once('=').ok_or_else(|| {
            invalid(format!(
                "--state-announce {spec:?} needs MACHINE.STATE=TEXT"
            ))
            .hint("e.g. --state-announce lamp.on='Lamp is on' (empty text removes it)")
        })?;
        let (machine, state) = target.split_once('.').ok_or_else(|| {
            invalid(format!(
                "--state-announce {spec:?} needs MACHINE.STATE=TEXT"
            ))
        })?;
        let entry = root
            .get_mut("stateMachines")
            .and_then(Value::as_array_mut)
            .and_then(|ms| ms.iter_mut().find(|m| m["id"] == machine))
            .ok_or_else(|| {
                crate::assist::not_found(
                    "state machine",
                    machine,
                    project.state_machines.iter().map(|m| m.id.as_str()),
                )
            })?;
        let state_object = entry
            .get_mut("states")
            .and_then(Value::as_array_mut)
            .and_then(|ss| ss.iter_mut().find(|s| s["id"] == state))
            .and_then(Value::as_object_mut)
            .ok_or_else(|| {
                crate::assist::not_found(
                    "state",
                    state,
                    project
                        .state_machines
                        .iter()
                        .filter(|m| m.id == machine)
                        .flat_map(|m| m.states.iter().map(|s| s.id.as_str())),
                )
            })?;
        if text.is_empty() {
            state_object.remove("announce");
        } else {
            state_object.insert("announce".into(), json!(text));
        }
    }
    let project = validated(value)?;
    let mut result = save_project(&project, output)?;
    if output != "-" {
        result["a11y"] = report(&project);
    }
    Ok(result)
}

fn not_found_node(project: &Project, id: &str) -> crate::Failure {
    crate::assist::not_found("node", id, project.nodes.iter().map(|n| n.id.as_str()))
}
