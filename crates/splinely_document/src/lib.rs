//! Versioned, editor-independent Splinely documents, evaluation, export and history.
mod accessibility;
mod animation;
mod arrange;
mod behavior;
mod binary;
mod blend_space;
mod bones;
mod boolean;
mod browser_host;
mod conversion;
pub mod crash;
mod follow;
mod geometry;
mod groups;
mod history;
mod ik;
mod isolated_preview;
mod json_loading;
mod model;
mod morph;
mod motion;
pub mod path_edit;
pub mod platform_dirs;
pub mod plugin;
mod primitives;
mod project_file;
mod render;
mod svg;
mod svg_convert;
mod svg_import;
mod svg_import_gradient;
mod svg_import_image;
mod svg_import_report;
mod svg_import_text;
mod svg_source;
mod text_outline;
mod tree;
mod validation;
mod values;

pub use accessibility::{
    A11yRole, NodeA11y, A11Y_DESCRIPTION_MAX, A11Y_LABEL_MAX, INTERACTIVE_EVENTS,
};
pub use animation::{ClipPlayback, EvalIndex, EvaluationContext};
pub use arrange::{transformed_bounds, AlignEdge, AlignTarget, Axis, Bounds, Stack};
pub use behavior::*;
pub use binary::{
    crc32c, decode_to_value, encode_value, SPLY_MAGIC, SPLY_MAJOR, SPLY_MAX_DEPTH, SPLY_MINOR,
};
pub use boolean::BooleanOp;
pub use browser_host::BrowserExportOptions;
pub use follow::{blend_values, smooth_toward, StateFade};
pub use groups::group_rig_bone;
pub use history::*;
pub use model::*;
pub use morph::morph_paths;
pub use project_file::MAX_PROJECT_FILE_BYTES;
pub use svg::EmbedFormat;
pub use svg_import::path_bounds;
pub use svg_import_report::{
    limits as svg_import_limits, ImportDiagnostic, ImportFailure, ImportOutcome, ImportReport,
    ImportStatus,
};
pub use svg_source::SourceSync;
pub use text_outline::{default_font_family, fontdb, system_fonts};

use serde::Deserialize;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentError(pub String);
impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for DocumentError {}
impl From<serde_json::Error> for DocumentError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

/// Parse a serializable wire type without duplicate members, unknown nested
/// fields, oversized integer tokens, or numeric loss. Missing defaults are allowed.
pub fn from_json_strict<T: serde::de::DeserializeOwned + serde::Serialize>(
    json: &str,
) -> Result<T, DocumentError> {
    let input = json_loading::load_value(json)?;
    let value = T::deserialize(&input)?;
    ensure_preserved(&input, &value)?;
    Ok(value)
}

/// `check_preserved(input, to_value(value))`, skipping the canonical `Value`
/// when the comparing serializer can prove success; otherwise the reference
/// check runs and reports the exact error.
fn ensure_preserved<T: serde::Serialize>(
    input: &serde_json::Value,
    value: &T,
) -> Result<(), DocumentError> {
    if tree::preserved_by(&tree::Tree::from_value(input), value) {
        return Ok(());
    }
    json_loading::check_preserved(input, &serde_json::to_value(value)?, "")
}

/// A strict-load failure with the stable machine-readable code the CLI reports
/// (`invalid_project`, `unknown_field`, `data_loss`, `validation_error`,
/// `serialization_error`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrictLoadError {
    pub code: &'static str,
    pub message: String,
}
impl fmt::Display for StrictLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for StrictLoadError {}

impl Project {
    /// Strictly load a project from an already parsed JSON value tree (for
    /// example `decode_to_value`): normalise, deserialize, reject unknown
    /// fields and any lossy change, then `validate()`. Takes the value by
    /// move; nothing is cloned.
    pub fn from_value_strict(mut value: serde_json::Value) -> Result<Self, StrictLoadError> {
        let fail = |code, message: &dyn fmt::Display| StrictLoadError {
            code,
            message: message.to_string(),
        };
        if !value.is_object() {
            return Err(fail("invalid_project", &"Project must be a JSON object"));
        }
        normalize_project_value(&mut value);
        let project = Self::deserialize(&value).map_err(|e| fail("invalid_project", &e))?;
        let canonical =
            serde_json::to_value(&project).map_err(|e| fail("serialization_error", &e))?;
        json_loading::check_preserved_coded(&value, &canonical, "")?;
        project
            .validate()
            .map_err(|e| fail("validation_error", &e))?;
        Ok(project)
    }
    pub fn to_json(&self) -> Result<String, DocumentError> {
        self.validate()?;
        Ok(serde_json::to_string_pretty(self)?)
    }
    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        let mut input = json_loading::load_value(json)?;
        normalize_project_value(&mut input);
        let project = Self::deserialize(&input)?;
        ensure_preserved(&input, &project)?;
        project.validate()?;
        Ok(project)
    }
}

/// Fast strict load straight from an arena (binary `.sply`). `None` means
/// "use the reference pipeline": the document would be normalized, fails to
/// deserialize, or the comparing serializer could not prove that nothing is lost.
pub(crate) fn fast_load(tree: &tree::Tree<'_>) -> Option<Project> {
    if would_normalize(tree) {
        return None;
    }
    let project = <Project as serde::Deserialize>::deserialize(tree.deserializer()).ok()?;
    tree::preserved_by(tree, &project).then_some(project)
}

/// Read-only twin of `normalize_project_value`: true if it could change anything.
/// Must stay at least as broad as `drop_legacy_speech` + `drop_explicit_defaults`.
fn would_normalize(t: &tree::Tree<'_>) -> bool {
    use tree::{View, ROOT};
    let is_null = |id: Option<u32>| id.is_some_and(|id| matches!(t.view(id), View::Null));
    let is_zero = |id: Option<u32>| {
        id.is_some_and(|id| match t.view(id) {
            View::U64(0) => true,
            View::F64(f) => f == 0.0,
            _ => false,
        })
    };
    let ids =
        |id: Option<u32>| -> Vec<u32> { id.and_then(|id| t.array_ids(id)).unwrap_or(&[]).to_vec() };
    let is_speak = |action: u32| {
        t.get(action, "type")
            .is_some_and(|ty| matches!(t.view(ty), View::Str("speak")))
    };
    if !matches!(t.view(ROOT), View::Object(_)) {
        return false;
    }
    if t.get(ROOT, "speechCues").is_some() {
        return true;
    }
    for key in ["followers", "groups", "guides"] {
        if t.get(ROOT, key).is_some_and(|id| t.is_empty_array(id)) {
            return true;
        }
    }
    for follower in ids(t.get(ROOT, "followers")) {
        if is_zero(t.get(follower, "restAngle"))
            || is_null(t.get(follower, "origin"))
            || is_null(t.get(follower, "when"))
        {
            return true;
        }
    }
    for node in ids(t.get(ROOT, "nodes")) {
        if is_null(t.get(node, "motionPath")) {
            return true;
        }
    }
    for machine in ids(t.get(ROOT, "stateMachines")) {
        for state in ids(t.get(machine, "states")) {
            if t.get(state, "onExit")
                .is_some_and(|id| t.is_empty_array(id))
                || ids(t.get(state, "onEnter")).into_iter().any(is_speak)
            {
                return true;
            }
        }
        for transition in ids(t.get(machine, "transitions")) {
            if is_zero(t.get(transition, "blend"))
                || is_null(t.get(transition, "after"))
                || ids(t.get(transition, "actions")).into_iter().any(is_speak)
            {
                return true;
            }
        }
    }
    false
}

/// Prepare authored project JSON for strict loading: drop data of removed
/// features (speech) and optional fields written out with their defaults.
pub fn normalize_project_value(project: &mut serde_json::Value) {
    drop_legacy_speech(project);
    drop_explicit_defaults(project);
}

/// Optional fields that are omitted when empty may still be written out
/// explicitly by hand or by an LLM; accept them instead of reporting them as
/// unknown fields.
fn drop_explicit_defaults(project: &mut serde_json::Value) {
    let Some(root) = project.as_object_mut() else {
        return;
    };
    for key in ["followers", "groups", "guides"] {
        if root
            .get(key)
            .is_some_and(|v| v.as_array().is_some_and(Vec::is_empty))
        {
            root.remove(key);
        }
    }
    if let Some(followers) = root.get_mut("followers").and_then(|f| f.as_array_mut()) {
        for follower in followers.iter_mut().filter_map(|f| f.as_object_mut()) {
            if follower.get("restAngle").and_then(|v| v.as_f64()) == Some(0.0) {
                follower.remove("restAngle");
            }
            for key in ["origin", "when"] {
                if follower.get(key).is_some_and(|v| v.is_null()) {
                    follower.remove(key);
                }
            }
        }
    }
    if let Some(nodes) = root.get_mut("nodes").and_then(|n| n.as_array_mut()) {
        for node in nodes.iter_mut().filter_map(|n| n.as_object_mut()) {
            if node.get("motionPath").is_some_and(|v| v.is_null()) {
                node.remove("motionPath");
            }
        }
    }
    let machines = root.get_mut("stateMachines").and_then(|m| m.as_array_mut());
    for machine in machines.into_iter().flatten() {
        let states = machine.get_mut("states").and_then(|s| s.as_array_mut());
        for state in states
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_object_mut())
        {
            if state
                .get("onExit")
                .is_some_and(|v| v.as_array().is_some_and(Vec::is_empty))
            {
                state.remove("onExit");
            }
        }
        let transitions = machine
            .get_mut("transitions")
            .and_then(|t| t.as_array_mut());
        for transition in transitions
            .into_iter()
            .flatten()
            .filter_map(|t| t.as_object_mut())
        {
            if transition.get("blend").and_then(|v| v.as_f64()) == Some(0.0) {
                transition.remove("blend");
            }
            if transition.get("after").is_some_and(|v| v.is_null()) {
                transition.remove("after");
            }
        }
    }
}

/// Speech was removed in 0.3; older projects still load without their
/// `speechCues` and `speak` actions.
fn drop_legacy_speech(project: &mut serde_json::Value) {
    let Some(root) = project.as_object_mut() else {
        return;
    };
    root.remove("speechCues");
    let Some(machines) = root.get_mut("stateMachines").and_then(|m| m.as_array_mut()) else {
        return;
    };
    let is_speak = |a: &serde_json::Value| a.get("type").and_then(|t| t.as_str()) == Some("speak");
    for machine in machines {
        let states = machine.get_mut("states").and_then(|s| s.as_array_mut());
        for state in states.into_iter().flatten() {
            if let Some(actions) = state.get_mut("onEnter").and_then(|a| a.as_array_mut()) {
                actions.retain(|a| !is_speak(a));
            }
        }
        let transitions = machine
            .get_mut("transitions")
            .and_then(|t| t.as_array_mut());
        for transition in transitions.into_iter().flatten() {
            if let Some(actions) = transition.get_mut("actions").and_then(|a| a.as_array_mut()) {
                actions.retain(|a| !is_speak(a));
            }
        }
    }
}
