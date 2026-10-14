use crate::{failure, load, project_value, save_project, validated, Result};
use clap::ValueEnum;
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Mode {
    Override,
    Weighted,
}

pub fn run(input: &str, output: &str, id: &str, weight: f64, mode: Mode) -> Result<Value> {
    if !weight.is_finite() || !(0.0..=1.0).contains(&weight) {
        return Err(failure(
            "invalid_arguments",
            "Weight must be finite and in [0,1]",
        ));
    }
    let mut value = project_value(&load(input)?)?;
    let clip = value["clips"]
        .as_array_mut()
        .expect("canonical clips array")
        .iter_mut()
        .find(|clip| clip["id"].as_str() == Some(id))
        .ok_or_else(|| failure("not_found", format!("No clip with ID {id}")))?;
    clip["weight"] = json!(weight);
    clip["blendMode"] = json!(match mode {
        Mode::Override => "override",
        Mode::Weighted => "weighted",
    });
    // Includes checking blendClip actions when changing a weighted clip to override.
    save_project(&validated(value)?, output)
}
