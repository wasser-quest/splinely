//! Versioned, editor-independent Splinely documents, evaluation, export and history.
mod animation;
mod arrange;
mod behavior;
mod bones;
mod boolean;
mod browser_host;
mod conversion;
mod geometry;
mod groups;
mod history;
mod ik;
mod json_loading;
mod model;
mod morph;
mod primitives;
mod project_file;
mod render;
mod svg;
mod svg_import;
mod text_outline;
mod validation;
mod values;

pub use animation::{ClipPlayback, EvaluationContext};
pub use arrange::{transformed_bounds, AlignEdge, AlignTarget, Axis, Bounds, Stack};
pub use behavior::*;
pub use boolean::BooleanOp;
pub use browser_host::BrowserExportOptions;
pub use history::*;
pub use model::*;
pub use morph::morph_paths;
pub use project_file::MAX_PROJECT_FILE_BYTES;
pub use svg_import::path_bounds;
pub use text_outline::{default_font_family, fontdb, system_fonts};

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
    let value: T = serde_json::from_value(input.clone())?;
    json_loading::check_preserved(&input, &serde_json::to_value(&value)?, "")?;
    Ok(value)
}

impl Project {
    pub fn to_json(&self) -> Result<String, DocumentError> {
        self.validate()?;
        Ok(serde_json::to_string_pretty(self)?)
    }
    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        let mut input = json_loading::load_value(json)?;
        drop_legacy_speech(&mut input);
        let project: Self = serde_json::from_value(input.clone())?;
        json_loading::check_preserved(&input, &serde_json::to_value(&project)?, "")?;
        project.validate()?;
        Ok(project)
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
