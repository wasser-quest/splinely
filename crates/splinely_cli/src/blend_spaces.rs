//! Focused, atomic authoring for two-parameter blend spaces.
use crate::{failure, load, save_project, Result};
use clap::Subcommand;
use serde_json::{json, Value};
use splinely_document::{BlendSpace, BlendSpaceSample};

#[derive(Subcommand)]
pub enum BlendSpaceCommand {
    /// List authored spaces and their current normalized weight multipliers.
    List { input: String },
    /// Create/replace a space. Referenced clips must already be weighted.
    Set {
        input: String,
        #[arg(long)]
        id: String,
        /// Numeric variable for the horizontal axis; created at zero if missing.
        #[arg(long)]
        x: String,
        /// Numeric variable for the vertical axis; created at zero if missing.
        #[arg(long)]
        y: String,
        /// CLIP:X:Y; repeat for each sample. Does not activate clips.
        #[arg(long, required = true, allow_hyphen_values = true)]
        sample: Vec<String>,
        #[arg(long)]
        output: String,
    },
    /// Remove a space without removing its variables or clips.
    Remove {
        input: String,
        #[arg(long)]
        id: String,
        #[arg(long)]
        output: String,
    },
    /// Set an authored numeric blend parameter (runtime changes use setVariable).
    Parameter {
        input: String,
        #[arg(long)]
        name: String,
        #[arg(long, allow_hyphen_values = true, value_parser = crate::bones::number)]
        value: f64,
        #[arg(long)]
        output: String,
    },
    /// Inspect normalized multipliers at a point without changing the project.
    Weights {
        input: String,
        #[arg(long)]
        id: String,
        #[arg(long, allow_hyphen_values = true, value_parser = crate::bones::number)]
        x: f64,
        #[arg(long, allow_hyphen_values = true, value_parser = crate::bones::number)]
        y: f64,
    },
}
fn sample(text: &str) -> Result<BlendSpaceSample> {
    let parts: Vec<_> = text.split(':').collect();
    if parts.len() != 3 {
        return Err(failure("invalid_arguments", "Sample must be CLIP:X:Y"));
    }
    let number = |s: &str| crate::bones::number(s).map_err(|e| failure("invalid_arguments", e));
    Ok(BlendSpaceSample {
        clip_id: parts[0].into(),
        position: [number(parts[1])?, number(parts[2])?],
    })
}
pub fn run(command: BlendSpaceCommand) -> Result<Value> {
    match command {
        BlendSpaceCommand::List { input } => {
            let project = load(&input)?;
            let weights = project
                .blend_space_weights(&Default::default())
                .map_err(|e| failure("validation_error", e))?;
            Ok(json!({"ok":true,"blendSpaces":project.blend_spaces,"weights":weights}))
        }
        BlendSpaceCommand::Set {
            input,
            id,
            x,
            y,
            sample: samples,
            output,
        } => {
            let mut project = load(&input)?;
            let space = BlendSpace {
                id: id.clone(),
                parameters: [x, y],
                samples: samples.iter().map(|s| sample(s)).collect::<Result<_>>()?,
            };
            for name in &space.parameters {
                project.variables.entry(name.clone()).or_insert(json!(0));
            }
            if let Some(existing) = project.blend_spaces.iter_mut().find(|s| s.id == id) {
                *existing = space;
            } else {
                project.blend_spaces.push(space);
            }
            save_project(&project, &output)
        }
        BlendSpaceCommand::Remove { input, id, output } => {
            let mut project = load(&input)?;
            let index = project
                .blend_spaces
                .iter()
                .position(|s| s.id == id)
                .ok_or_else(|| failure("not_found", format!("No blend space {id}")))?;
            project.blend_spaces.remove(index);
            save_project(&project, &output)
        }
        BlendSpaceCommand::Parameter {
            input,
            name,
            value,
            output,
        } => {
            let mut project = load(&input)?;
            if !project
                .blend_spaces
                .iter()
                .any(|s| s.parameters.contains(&name))
            {
                return Err(failure("not_found", "No such blend-space parameter"));
            }
            project.variables.insert(name, json!(value));
            save_project(&project, &output)
        }
        BlendSpaceCommand::Weights { input, id, x, y } => {
            let project = load(&input)?;
            let space = project
                .blend_spaces
                .iter()
                .find(|s| s.id == id)
                .ok_or_else(|| failure("not_found", format!("No blend space {id}")))?;
            let weights = space
                .weights([x, y])
                .map_err(|e| failure("invalid_arguments", e))?;
            let weights: std::collections::BTreeMap<_, _> = space
                .samples
                .iter()
                .zip(weights)
                .map(|(s, w)| (&s.clip_id, w))
                .collect();
            Ok(json!({"ok":true,"id":id,"weights":weights}))
        }
    }
}
