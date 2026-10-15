use crate::*;
use std::collections::{BTreeMap, HashSet};

const LIMIT: f64 = 1e7;
fn parameter(value: Option<&serde_json::Value>) -> Result<f64, DocumentError> {
    value
        .and_then(|v| v.as_f64())
        .filter(|v| v.is_finite() && v.abs() <= LIMIT)
        .ok_or_else(|| {
            DocumentError(
                "blend-space parameters must be numeric, finite and within +/- 10000000".into(),
            )
        })
}
impl BlendSpace {
    /// Normalized inverse squared distance; exact sample positions are one-hot.
    /// Scaling by the nearest squared distance avoids reciprocal overflow.
    pub fn weights(&self, position: [f64; 2]) -> Result<Vec<f64>, DocumentError> {
        if self.samples.is_empty()
            || self.samples.len() > 256
            || !position
                .iter()
                .chain(self.samples.iter().flat_map(|s| &s.position))
                .all(|v| v.is_finite() && v.abs() <= LIMIT)
        {
            return Err(DocumentError(
                "invalid blend-space position or empty samples".into(),
            ));
        }
        let distances: Vec<f64> = self
            .samples
            .iter()
            .map(|s| (s.position[0] - position[0]).powi(2) + (s.position[1] - position[1]).powi(2))
            .collect();
        let nearest = distances.iter().copied().fold(f64::INFINITY, f64::min);
        if nearest == 0.0 {
            let index = distances.iter().position(|d| *d == 0.0).unwrap();
            return Ok((0..distances.len())
                .map(|i| if i == index { 1.0 } else { 0.0 })
                .collect());
        }
        let raw: Vec<f64> = distances.iter().map(|d| nearest / d).collect();
        let sum: f64 = raw.iter().sum();
        Ok(raw.iter().map(|w| w / sum).collect())
    }
}
impl Project {
    pub(crate) fn validate_blend_spaces(&self) -> Result<(), DocumentError> {
        let mut clips = HashSet::new();
        let weighted: HashSet<&str> = self
            .clips
            .iter()
            .filter(|c| c.blend_mode == BlendMode::Weighted)
            .map(|c| c.id.as_str())
            .collect();
        for space in &self.blend_spaces {
            if space.parameters[0] == space.parameters[1]
                || space.samples.is_empty()
                || space.samples.len() > 256
            {
                return Err(DocumentError(format!(
                    "blend space {} needs distinct parameters and 1..256 samples",
                    space.id
                )));
            }
            for name in &space.parameters {
                parameter(self.variables.get(name))?;
            }
            for (i, sample) in space.samples.iter().enumerate() {
                if !sample
                    .position
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= LIMIT)
                {
                    return Err(DocumentError(
                        "blend-space sample position out of range".into(),
                    ));
                }
                if !clips.insert(&sample.clip_id) || !weighted.contains(sample.clip_id.as_str()) {
                    return Err(DocumentError(format!(
                        "blend-space sample {} needs a unique existing weighted clip",
                        sample.clip_id
                    )));
                }
                if space.samples[..i].iter().any(|s| {
                    (s.position[0] - sample.position[0]).powi(2)
                        + (s.position[1] - sample.position[1]).powi(2)
                        <= 1e-12
                }) {
                    return Err(DocumentError(
                        "blend-space samples must be more than 0.000001 apart".into(),
                    ));
                }
            }
        }
        Ok(())
    }
    /// Multipliers for clip weights; does not activate clips or change their clocks.
    /// Missing parameter overrides use authored variables, even for inactive clips.
    pub fn blend_space_weights(
        &self,
        variables: &BTreeMap<String, serde_json::Value>,
    ) -> Result<BTreeMap<String, f64>, DocumentError> {
        self.validate_blend_spaces()?;
        self.blend_space_weights_unchecked(variables)
    }
    /// `blend_space_weights` for callers that just ran `validate_blend_spaces`.
    pub(crate) fn blend_space_weights_unchecked(
        &self,
        variables: &BTreeMap<String, serde_json::Value>,
    ) -> Result<BTreeMap<String, f64>, DocumentError> {
        let mut result = BTreeMap::new();
        for space in &self.blend_spaces {
            let mut point = [0.; 2];
            for (i, name) in space.parameters.iter().enumerate() {
                point[i] = parameter(variables.get(name).or_else(|| self.variables.get(name)))?;
            }
            for (sample, weight) in space.samples.iter().zip(space.weights(point)?) {
                result.insert(sample.clip_id.clone(), weight);
            }
        }
        Ok(result)
    }
}
