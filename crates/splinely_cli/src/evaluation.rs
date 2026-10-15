//! Strict CLI JSON adapter for the core's stateless evaluation context.
use crate::{check_preserved, failure, load, read_json, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use splinely_document::{ClipPlayback, EvaluationContext, PropertyOverride};
use std::collections::BTreeMap;

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct Context {
    states: BTreeMap<String, String>,
    clips: Option<Vec<Playback>>,
    clip_weights: BTreeMap<String, f64>,
    variables: BTreeMap<String, Value>,
    overrides: Vec<PropertyOverride>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Playback {
    clip_id: String,
    time: f64,
}

// Preserve the existing invalid_time diagnostic for nonfinite inputs while
// applying the same integer precision guard used by JSON context clocks.
pub fn time(source: &str) -> std::result::Result<f64, String> {
    if let Ok(value) = source.parse::<f64>() {
        if !value.is_finite() {
            return Ok(value);
        }
    }
    crate::bones::number(source)
}

pub fn run(input: &str, time: f64, context: Option<&str>) -> Result<Value> {
    if !time.is_finite() || time < 0.0 {
        return Err(failure(
            "invalid_time",
            "Time must be finite and nonnegative",
        ));
    }
    if input == "-" && context == Some("-") {
        return Err(failure(
            "invalid_arguments",
            "Only one of INPUT and CONTEXT may read stdin",
        ));
    }
    let project = load(input)?;
    let context = if let Some(path) = context {
        let value = read_json(path)?;
        if !value.is_object() {
            return Err(failure("invalid_context", "Context must be a JSON object"));
        }
        let parsed: Context =
            serde_json::from_value(value.clone()).map_err(|e| failure("invalid_context", e))?;
        let canonical =
            serde_json::to_value(&parsed).map_err(|e| failure("serialization_error", e))?;
        check_preserved(&value, &canonical, "")?;
        EvaluationContext {
            states: parsed.states,
            clips: parsed.clips.map(|clips| {
                clips
                    .into_iter()
                    .map(|c| ClipPlayback {
                        clip_id: c.clip_id,
                        time: c.time,
                    })
                    .collect()
            }),
            clip_weights: parsed.clip_weights,
            variables: parsed.variables,
            overrides: parsed.overrides,
            ..Default::default()
        }
    } else {
        EvaluationContext::default()
    };
    let nodes = project
        .evaluate_with_context(time, &context)
        .map_err(|e| failure("evaluation_error", e))?;
    Ok(json!({"ok":true,"time":time,"nodeCount":nodes.len(),"nodes":nodes}))
}
