//! Onion skinning: faint ghosts of the animated shapes before (reddish) and
//! after (bluish) the playhead, so motion can be judged without scrubbing.
//! Count, spacing (keyframes or fixed interval), opacity and colours come from
//! the settings. Only shapes animated by the clip are shown.
use super::*;
use std::collections::HashSet;

impl Editor {
    /// Clip-local times of the nearest keys before and after the playhead.
    #[cfg(test)]
    pub(super) fn onion_times(&self) -> (Option<f64>, Option<f64>) {
        let Some(clip) = self.project.clips.get(self.active_clip) else {
            return (None, None);
        };
        let times = clip
            .tracks
            .iter()
            .flat_map(|t| t.keyframes.iter().map(|k| k.time));
        let (mut before, mut after): (Option<f64>, Option<f64>) = (None, None);
        for t in times {
            if t < self.time - 1e-6 && before.is_none_or(|b| t > b) {
                before = Some(t);
            }
            if t > self.time + 1e-6 && after.is_none_or(|a| t < a) {
                after = Some(t);
            }
        }
        (before, after)
    }

    /// Ghost times as (time, before?, distance 1..): neighbouring keyframes or
    /// fixed steps around the playhead, nearest first on each side.
    pub(super) fn onion_ghost_times(&self) -> Vec<(f64, bool, usize)> {
        let Some(clip) = self.project.clips.get(self.active_clip) else {
            return vec![];
        };
        let s = &self.settings;
        let (before, after): (Vec<f64>, Vec<f64>) = match s.onion_mode {
            settings::OnionMode::Keyframes => {
                let mut times: Vec<f64> = clip
                    .tracks
                    .iter()
                    .flat_map(|t| t.keyframes.iter().map(|k| k.time))
                    .collect();
                times.sort_by(f64::total_cmp);
                times.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
                let before = times
                    .iter()
                    .rev()
                    .filter(|t| **t < self.time - 1e-6)
                    .take(s.onion_before)
                    .copied()
                    .collect();
                let after = times
                    .iter()
                    .filter(|t| **t > self.time + 1e-6)
                    .take(s.onion_after)
                    .copied()
                    .collect();
                (before, after)
            }
            settings::OnionMode::Interval => {
                let end = clip.duration.max(self.time);
                let step = s.onion_interval.max(0.01);
                let before = (1..=s.onion_before)
                    .map(|k| self.time - k as f64 * step)
                    .filter(|t| *t >= -1e-9)
                    .collect();
                let after = (1..=s.onion_after)
                    .map(|k| self.time + k as f64 * step)
                    .filter(|t| *t <= end + 1e-9)
                    .collect();
                (before, after)
            }
        };
        let side = |times: Vec<f64>, is_before: bool| {
            times
                .into_iter()
                .enumerate()
                .map(move |(i, t)| (t, is_before, i + 1))
        };
        // Farthest first, so nearer ghosts paint on top.
        let mut ghosts: Vec<_> = side(before, true).chain(side(after, false)).collect();
        ghosts.sort_by_key(|g| std::cmp::Reverse(g.2));
        ghosts
    }

    /// Node IDs animated by the active clip, including shapes bound to animated bones.
    fn animated_nodes(&self) -> HashSet<String> {
        let Some(clip) = self.project.clips.get(self.active_clip) else {
            return HashSet::new();
        };
        let targets: HashSet<&str> = clip.tracks.iter().map(|t| t.node_id.as_str()).collect();
        // Bones move their children too.
        let mut bones: HashSet<String> = self
            .project
            .bones
            .iter()
            .filter(|b| targets.contains(b.id.as_str()))
            .map(|b| b.id.clone())
            .collect();
        loop {
            let more: Vec<String> = self
                .project
                .bones
                .iter()
                .filter(|b| {
                    !bones.contains(&b.id)
                        && b.parent_id.as_ref().is_some_and(|p| bones.contains(p))
                })
                .map(|b| b.id.clone())
                .collect();
            if more.is_empty() {
                break;
            }
            bones.extend(more);
        }
        self.project
            .nodes
            .iter()
            .filter(|n| {
                targets.contains(n.id.as_str())
                    || n.bone_id.as_ref().is_some_and(|b| bones.contains(b))
            })
            .map(|n| n.id.clone())
            .collect()
    }

    /// Ghosts of the previous/next key poses, drawn over the canvas.
    pub(super) fn paint_onion(&mut self, painter: &egui::Painter, document_rect: Rect) {
        if !self.onion_skin || self.playing || self.tab != Tab::Canvas {
            return;
        }
        let animated = self.animated_nodes();
        if animated.is_empty() {
            return;
        }
        let ghosts = self.onion_ghost_times();
        let s = &self.settings;
        let (opacity, color_before, color_after) =
            (s.onion_opacity, s.onion_color_before, s.onion_color_after);
        let per_side = |before: bool| {
            if before {
                s.onion_before
            } else {
                s.onion_after
            }
            .max(1)
        };
        let (count_before, count_after) = (per_side(true), per_side(false));
        if self.onion_rasters.len() < ghosts.len() {
            self.onion_rasters
                .resize_with(ghosts.len(), Default::default);
        }
        let mut context = self.preview_context();
        for (slot, (time, before, distance)) in ghosts.into_iter().enumerate() {
            // Farther ghosts fade out.
            let count = if before { count_before } else { count_after };
            let fade = (count + 1 - distance.min(count)) as f32 / count as f32;
            let alpha = (opacity * fade * 255.).round().clamp(4., 255.) as u8;
            let [r, g, b] = if before { color_before } else { color_after };
            let tint = Color32::from_rgba_unmultiplied(r, g, b, alpha);
            if let Some(clip) = self.project.clips.get(self.active_clip) {
                context.clips = Some(vec![ClipPlayback {
                    clip_id: clip.id.clone(),
                    time,
                }]);
            }
            let Ok(nodes) = self.project.evaluate_with_context(time, &context) else {
                continue;
            };
            let scene: Vec<SceneNode> = nodes
                .iter()
                .filter(|n| animated.contains(&n.id))
                .filter_map(forma_editor::projection::project_node)
                .collect();
            let (width, height) = (self.project.width, self.project.height);
            if let Err(error) = self.onion_rasters[slot].paint_tinted(
                painter,
                document_rect,
                width,
                height,
                &scene,
                tint,
            ) {
                self.status = format!("Zwiebelhaut: {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghost_times_are_the_neighbouring_keys() {
        let mut editor = Editor::default();
        let keys: Vec<f64> = editor.project.clips[0].tracks[0]
            .keyframes
            .iter()
            .map(|k| k.time)
            .collect();
        assert!(keys.len() >= 3);
        editor.time = (keys[0] + keys[1]) / 2.;
        assert_eq!(editor.onion_times(), (Some(keys[0]), Some(keys[1])));
        editor.time = keys[1];
        assert_eq!(editor.onion_times(), (Some(keys[0]), Some(keys[2])));
        let animated = editor.animated_nodes();
        assert!(animated.contains(&editor.project.clips[0].tracks[0].node_id));
        assert!(
            animated.len() < editor.project.nodes.len(),
            "static shapes are not ghosted"
        );
        // Two ghosts per side at keyframes, farthest first.
        editor.settings.onion_before = 2;
        editor.settings.onion_after = 2;
        editor.time = keys[2];
        let ghosts = editor.onion_ghost_times();
        assert!(ghosts.contains(&(keys[1], true, 1)));
        assert!(ghosts.contains(&(keys[0], true, 2)));
        assert_eq!(ghosts[0].2, 2);
        // Fixed interval: steps around the playhead, never before 0.
        editor.settings.onion_mode = settings::OnionMode::Interval;
        editor.settings.onion_interval = 0.1;
        editor.time = 0.15;
        let mut times: Vec<f64> = editor
            .onion_ghost_times()
            .iter()
            .map(|g| (g.0 * 100.).round() / 100.)
            .collect();
        times.sort_by(f64::total_cmp);
        assert_eq!(times, vec![0.05, 0.25, 0.35]);
    }
}
