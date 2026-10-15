//! Native-only clip handles. Document weights and tracks are never mutated.
use splinely_document::{BlendMode, ClipPlayback, EvaluationContext, Project};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
struct Ramp {
    from: f64,
    to: f64,
    elapsed: f64,
    duration: f64,
}
#[derive(Clone, Debug)]
pub(super) struct Layer {
    pub time: f64,
    pub active: bool,
    pub playing: bool,
    pub weight: f64,
    ramp: Option<Ramp>,
}
#[derive(Default)]
pub(super) struct BlendPreview {
    pub enabled: bool,
    pub layers: BTreeMap<String, Layer>,
}
impl BlendPreview {
    fn layer(&mut self, project: &Project, id: &str) -> Result<&mut Layer, String> {
        let clip = project.clips.iter().find(|c| c.id == id).ok_or_else(|| {
            splinely_editor::i18n::trf("Unbekannter Vorschau-Clip: {id}", &[id.to_string()])
        })?;
        Ok(self.layers.entry(id.into()).or_insert(Layer {
            time: 0.,
            active: false,
            playing: false,
            weight: clip.weight,
            ramp: None,
        }))
    }
    pub fn play(&mut self, project: &Project, id: &str) -> Result<(), String> {
        let duration = project
            .clips
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| {
                splinely_editor::i18n::trf("Unbekannter Vorschau-Clip: {id}", &[id.to_string()])
            })?
            .duration;
        let layer = self.layer(project, id)?;
        if layer.time >= duration {
            layer.time = 0.;
        }
        layer.active = true;
        layer.playing = true;
        self.enabled = true;
        Ok(())
    }
    pub fn pause(&mut self, id: &str) {
        if let Some(layer) = self.layers.get_mut(id) {
            layer.playing = false;
        }
    }
    pub fn stop(&mut self, project: &Project, id: &str) -> Result<(), String> {
        let layer = self.layer(project, id)?;
        layer.active = false;
        layer.playing = false;
        layer.time = 0.;
        layer.ramp = None;
        self.enabled = true;
        Ok(())
    }
    pub fn fade(
        &mut self,
        project: &Project,
        id: &str,
        weight: f64,
        duration: f64,
    ) -> Result<(), String> {
        if !weight.is_finite()
            || !(0.0..=1.0).contains(&weight)
            || !duration.is_finite()
            || duration < 0.
        {
            return Err(splinely_editor::i18n::tr(
                "Mischvorschau braucht ein Gewicht in [0, 1] und eine Dauer ≥ 0",
            )
            .into());
        }
        let clip = project.clips.iter().find(|c| c.id == id).ok_or_else(|| {
            splinely_editor::i18n::trf("Unbekannter Vorschau-Clip: {id}", &[id.to_string()])
        })?;
        if clip.blend_mode == BlendMode::Override {
            return Err(splinely_editor::i18n::trf(
                "Mischvorschau braucht einen gewichteten Clip: {id}",
                &[id.to_string()],
            ));
        }
        let layer = self.layer(project, id)?;
        layer.ramp = if duration == 0. {
            layer.weight = weight;
            None
        } else {
            Some(Ramp {
                from: layer.weight,
                to: weight,
                elapsed: 0.,
                duration,
            })
        };
        // Changing weight alone never activates a clip or switches manual selection.
        Ok(())
    }
    pub fn apply(&self, project: &Project, context: &mut EvaluationContext) {
        context.clip_weights = self
            .layers
            .iter()
            .filter(|(id, _)| project.clips.iter().any(|c| &c.id == *id))
            .map(|(id, layer)| (id.clone(), layer.weight))
            .collect();
        if self.enabled {
            context.clips = Some(
                project
                    .clips
                    .iter()
                    .filter_map(|clip| {
                        self.layers
                            .get(&clip.id)
                            .filter(|l| l.active)
                            .map(|l| ClipPlayback {
                                clip_id: clip.id.clone(),
                                time: l.time,
                            })
                    })
                    .collect(),
            );
        }
    }
    pub fn ticking(&self) -> bool {
        self.layers.values().any(|l| l.playing || l.ramp.is_some())
    }
    /// Stop at the next completion boundary so its actions affect the remaining delta.
    pub fn step(&mut self, project: &Project, dt: f64) -> (f64, Vec<String>) {
        self.layers
            .retain(|id, _| project.clips.iter().any(|c| &c.id == id));
        let mut step = dt;
        for clip in &project.clips {
            if let Some(layer) = self.layers.get(&clip.id) {
                if layer.playing && !clip.looping {
                    step = step.min((clip.duration - layer.time).max(0.));
                }
            }
        }
        let mut ended = Vec::new();
        for clip in &project.clips {
            let Some(layer) = self.layers.get_mut(&clip.id) else {
                continue;
            };
            if let Some(ramp) = &mut layer.ramp {
                ramp.elapsed = (ramp.elapsed + step).min(ramp.duration);
                layer.weight = ramp.from + (ramp.to - ramp.from) * (ramp.elapsed / ramp.duration);
                if ramp.elapsed >= ramp.duration {
                    layer.ramp = None;
                }
            }
            if !layer.playing {
                continue;
            }
            layer.time += step;
            if clip.looping {
                layer.time %= clip.duration;
            } else if layer.time >= clip.duration {
                layer.time = clip.duration;
                layer.playing = false;
                ended.push(clip.id.clone());
            }
        }
        (step, ended)
    }
}
