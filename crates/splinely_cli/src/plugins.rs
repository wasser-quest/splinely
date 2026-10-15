//! `plugin list|validate|apply`: the declarative editor part of drop-in plugins for scripts and
//! LLM agents, plus the shared `--plugin` loading used by `export` / `isolated-preview`.
//! Uses the exact `splinely_document::plugin` candidate path the editor sidebar uses: nothing is
//! executed, the result is validated as a whole and saved atomically (or not at all).
//! The runtime (browser) part is only ever embedded by `export --plugin ... --trusted-js`.
use crate::scaffold::PluginCommand;
use crate::{failure, load, save_project, scaffold, Result};
use serde_json::{json, Value};
use splinely_document::plugin::{
    load_runtime_bundle, CandidateError, Extension, ExtensionRegistry, RuntimeBundle,
};
use std::collections::BTreeSet;
use std::path::PathBuf;

pub fn run(command: PluginCommand) -> Result<Value> {
    match command {
        PluginCommand::List { dir } => list(dir),
        PluginCommand::Validate { path } => {
            let plugin = ExtensionRegistry::load_plugin(&path).map_err(|e| {
                failure("invalid_plugin", format!("{path}: {e}"))
                    .hint("Fix splinely-plugin.json / the runtime file; see docs/plugins.md")
            })?;
            Ok(json!({"ok":true,"valid":true,"executed":false,"plugin":describe(&plugin)}))
        }
        PluginCommand::Apply {
            plugin,
            tool,
            project,
            node,
            output,
        } => apply(&plugin, &tool, &project, node.as_deref(), output),
        other => scaffold::run(other),
    }
}

fn tool_list(tools: &[splinely_document::plugin::ExtensionTool]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| json!({"id":t.id,"title":t.title,"needsSelection":ExtensionRegistry::needs_selection(t)}))
        .collect()
}

fn describe(plugin: &Extension) -> Value {
    json!({
        "id":plugin.id,"title":plugin.title,
        "tools":tool_list(&plugin.tools),"palette":tool_list(&plugin.palette),
        "runtime":plugin.runtime.as_ref().map(|r| json!({
            "entry":r.entry,"apiVersion":r.api_version,"bytes":r.bytes,"sha256":r.sha256,
            "description":r.description,
            "exportedOnlyWith":"splinely-cli export --plugin PATH --trusted-js"})),
    })
}

fn list(dir: Option<PathBuf>) -> Result<Value> {
    let registry = match dir {
        Some(dir) => ExtensionRegistry::load_directory(dir),
        None => ExtensionRegistry::load_default(),
    };
    Ok(json!({
        "ok":true,"executed":false,
        "directory":registry.directory.as_ref().map(|d| d.display().to_string()),
        "plugins":registry.plugins().iter().map(describe).collect::<Vec<_>>(),
        "diagnostics":registry.diagnostics,
    }))
}

fn apply(
    plugin_path: &str,
    tool: &str,
    project_path: &str,
    node: Option<&str>,
    output: Option<String>,
) -> Result<Value> {
    let plugin = ExtensionRegistry::load_plugin(plugin_path).map_err(|e| {
        failure("invalid_plugin", format!("{plugin_path}: {e}"))
            .hint("Run `splinely-cli plugin validate PLUGIN` for details")
    })?;
    let id = plugin.id.clone();
    let registry = ExtensionRegistry::single(plugin);
    let project = load(project_path)?;
    let next = registry
        .candidate(&id, tool, &project, node)
        .map_err(|e| match e {
            CandidateError::UnknownTool => failure("unknown_tool", format!("Plugin {id} has no tool {tool:?}"))
                .hint("Run `splinely-cli plugin list --dir PARENT` or `plugin validate PLUGIN` to see tool ids"),
            CandidateError::NeedSelection => failure("invalid_arguments", "This tool needs a target node")
                .hint("Pass --node NODE_ID (see `inspect`)"),
            CandidateError::SelectionMissing => failure("unknown_node", format!("Node {node:?} does not exist"))
                .hint("Use an id from `inspect`"),
            CandidateError::SelectionLocked => failure("node_locked", "The node is locked; unlock it first"),
            CandidateError::Rejected(m) => failure("validation_error", m),
            other => failure("plugin_error", other),
        })?;
    let ids =
        |p: &splinely_document::Project| -> (BTreeSet<String>, BTreeSet<String>, BTreeSet<String>) {
            (
                p.clips.iter().map(|c| c.id.clone()).collect(),
                p.state_machines.iter().map(|m| m.id.clone()).collect(),
                p.variables.keys().cloned().collect(),
            )
        };
    let (c0, m0, v0) = ids(&project);
    let (c1, m1, v1) = ids(&next);
    let output = output.unwrap_or_else(|| project_path.to_owned());
    let mut result = save_project(&next, &output)?;
    result["plugin"] = json!(id);
    result["tool"] = json!(tool);
    result["node"] = json!(node);
    result["added"] = json!({
        "nodes": next.nodes.len().saturating_sub(project.nodes.len()),
        "clips": c1.difference(&c0).collect::<Vec<_>>(),
        "stateMachines": m1.difference(&m0).collect::<Vec<_>>(),
        "variables": v1.difference(&v0).collect::<Vec<_>>(),
    });
    result["executed"] = json!(false);
    Ok(result)
}

/// `export/isolated-preview --plugin PATH`: loads runtime parts. Plugin code is executable, so
/// without `trusted_js` this is an explicit error (never a silent drop).
pub fn bundles(paths: &[String], trusted_js: bool) -> Result<Vec<RuntimeBundle>> {
    if paths.is_empty() {
        return Ok(vec![]);
    }
    if !trusted_js {
        return Err(failure(
            "trust_required",
            "--plugin embeds executable JavaScript; add --trusted-js to confirm you reviewed it",
        )
        .hint("Plugin runtime code is not sandboxed. Review the file and its sha256 (`plugin validate PLUGIN`) first"));
    }
    paths
        .iter()
        .map(|p| {
            load_runtime_bundle(p).map_err(|e| {
                failure("invalid_plugin", format!("{p}: {e}")).hint(
                    "Give a plugin folder with a runtime part or a .js file; see docs/plugins.md",
                )
            })
        })
        .collect()
}
