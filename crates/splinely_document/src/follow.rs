//! State crossfades and pointer followers: the shared math every host (browser
//! runtime, native runtime, editor preview) uses so they all agree.
use crate::*;
use std::collections::BTreeMap;

/// A machine that is fading from `from` into its current state.
#[derive(Clone, Debug, PartialEq)]
pub struct StateFade {
    pub from: String,
    /// Linear progress 0..1; eased when applied.
    pub progress: f64,
}

/// Mix two values for a crossfade. Numbers, colors, gradients and matching
/// paths blend; values that can't blend switch halfway.
pub fn blend_values(a: &AnimValue, b: &AnimValue, t: f64) -> AnimValue {
    let t = t.clamp(0., 1.);
    if a.compatible(b) {
        a.interpolate(b, t)
    } else if t < 0.5 {
        a.clone()
    } else {
        b.clone()
    }
}

/// Smoothstep keeps crossfades from starting or ending abruptly.
pub(crate) fn ease_fade(t: f64) -> f64 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// Move `current` towards `goal`. `smoothing` is roughly the time in seconds to
/// get there (an exponential approach that is frame-rate independent).
pub fn smooth_toward(current: [f64; 2], goal: [f64; 2], smoothing: f64, dt: f64) -> [f64; 2] {
    if smoothing <= 0. || !dt.is_finite() {
        return goal;
    }
    let keep = (-4. * dt.max(0.) / smoothing).exp();
    [
        goal[0] + (current[0] - goal[0]) * keep,
        goal[1] + (current[1] - goal[1]) * keep,
    ]
}

fn normalize_degrees(mut angle: f64) -> f64 {
    angle %= 360.;
    if angle > 180. {
        angle -= 360.;
    } else if angle < -180. {
        angle += 360.;
    }
    angle
}

impl Project {
    /// The scene point a follower measures the pointer from.
    pub fn follower_origin(&self, follower: &Follower) -> [f64; 2] {
        if let Some(origin) = follower.origin {
            return origin;
        }
        if let Some(node) = self.nodes.iter().find(|n| n.id == follower.target) {
            let [x, y, w, h] = crate::arrange::transformed_bounds(node);
            return [x + w / 2., y + h / 2.];
        }
        crate::bones::solve_bones(&self.bones)
            .ok()
            .and_then(|poses| poses.into_iter().find(|p| p.id == follower.target))
            .map_or([0., 0.], |p| [p.x, p.y])
    }

    /// Whether the follower's `when` condition holds for these machine states
    /// (missing machines are in their initial state).
    pub fn follower_active(&self, follower: &Follower, states: &BTreeMap<String, String>) -> bool {
        let Some(when) = &follower.when else {
            return true;
        };
        let current = states.get(&when.machine).cloned().or_else(|| {
            self.state_machines
                .iter()
                .find(|m| m.id == when.machine)
                .map(|m| m.initial.clone())
        });
        current.is_some_and(|state| when.states.contains(&state))
    }

    /// Where the follower wants to be: an x/y offset, or `[degrees, 0]` for
    /// rotate mode. Without a pointer, or while inactive, that is the rest pose.
    pub fn follower_goal(
        &self,
        follower: &Follower,
        pointer: Option<[f64; 2]>,
        states: &BTreeMap<String, String>,
    ) -> [f64; 2] {
        let Some([px, py]) = pointer else {
            return [0., 0.];
        };
        if !self.follower_active(follower, states) {
            return [0., 0.];
        }
        let [ox, oy] = self.follower_origin(follower);
        let (dx, dy) = (px - ox, py - oy);
        match follower.mode {
            FollowMode::Offset => {
                let (x, y) = (dx * follower.strength, dy * follower.strength);
                let length = x.hypot(y);
                if length > follower.max && length > 0. {
                    let scale = follower.max / length;
                    [x * scale, y * scale]
                } else {
                    [x, y]
                }
            }
            FollowMode::Rotate => {
                if dx == 0. && dy == 0. {
                    return [0., 0.];
                }
                let angle = normalize_degrees(dy.atan2(dx).to_degrees() - follower.rest_angle);
                [
                    (angle * follower.strength).clamp(-follower.max, follower.max),
                    0.,
                ]
            }
        }
    }

    /// Add the current follower values on top of evaluated nodes and bones.
    pub(crate) fn apply_followers(
        &self,
        nodes: &mut [Node],
        bones: &mut [Bone],
        index: &crate::animation::EvalIndex,
        values: &BTreeMap<String, [f64; 2]>,
    ) {
        for follower in &self.followers {
            let Some([a, b]) = values.get(&follower.id).copied() else {
                continue;
            };
            if let Some(node) = index.node(&follower.target).map(|i| &mut nodes[i]) {
                match follower.mode {
                    FollowMode::Offset => {
                        node.transform.x += a;
                        node.transform.y += b;
                    }
                    FollowMode::Rotate => node.transform.rotation += a,
                }
            } else if let Some(bone) = index.bone(&follower.target).map(|i| &mut bones[i]) {
                match follower.mode {
                    FollowMode::Offset => {
                        bone.x += a;
                        bone.y += b;
                    }
                    FollowMode::Rotate => bone.rotation += a,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eye() -> (Project, Follower) {
        let mut project = Project::default();
        project.nodes.push(Node::circle("pupil", 100., 100., 5.));
        let follower = Follower {
            id: "look".into(),
            target: "pupil".into(),
            mode: FollowMode::Offset,
            strength: 0.5,
            max: 10.,
            smoothing: 0.,
            rest_angle: 0.,
            origin: None,
            when: None,
        };
        project.followers.push(follower.clone());
        (project, follower)
    }

    #[test]
    fn offsets_are_scaled_and_clamped_to_the_maximum() {
        let (project, f) = eye();
        let none = BTreeMap::new();
        assert_eq!(
            project.follower_goal(&f, Some([108., 100.]), &none),
            [4., 0.]
        );
        let far = project.follower_goal(&f, Some([100., 400.]), &none);
        assert!((far[1] - 10.).abs() < 1e-9 && far[0].abs() < 1e-9);
        assert_eq!(project.follower_goal(&f, None, &none), [0., 0.]);
    }

    #[test]
    fn rotation_is_relative_to_the_rest_angle_and_limited() {
        let (mut project, mut f) = eye();
        f.mode = FollowMode::Rotate;
        f.strength = 1.;
        f.max = 45.;
        f.rest_angle = 0.;
        project.followers[0] = f.clone();
        let none = BTreeMap::new();
        let up = project.follower_goal(&f, Some([100., 0.]), &none);
        assert_eq!(up, [-45., 0.]);
        let slight = project.follower_goal(&f, Some([200., 110.]), &none);
        assert!((slight[0] - (10f64).atan2(100.).to_degrees()).abs() < 1e-9);
    }

    #[test]
    fn followers_only_act_in_their_states() {
        let (mut project, mut f) = eye();
        project.state_machines.push(StateMachine {
            id: "mood".into(),
            initial: "calm".into(),
            states: ["calm", "curious"]
                .map(|id| State {
                    id: id.into(),
                    on_enter: vec![],
                    overrides: vec![],
                    on_exit: vec![],
                    announce: None,
                })
                .into(),
            transitions: vec![],
        });
        f.when = Some(FollowCondition {
            machine: "mood".into(),
            states: vec!["curious".into()],
        });
        project.followers[0] = f.clone();
        project.validate().unwrap();
        let mut states = BTreeMap::new();
        assert_eq!(
            project.follower_goal(&f, Some([108., 100.]), &states),
            [0., 0.]
        );
        states.insert("mood".into(), "curious".into());
        assert_eq!(
            project.follower_goal(&f, Some([108., 100.]), &states),
            [4., 0.]
        );
    }

    #[test]
    fn smoothing_approaches_the_goal_independent_of_frame_rate() {
        let one = smooth_toward([0., 0.], [10., 0.], 0.5, 0.25);
        let mut two = [0., 0.];
        for _ in 0..2 {
            two = smooth_toward(two, [10., 0.], 0.5, 0.125);
        }
        assert!((one[0] - two[0]).abs() < 1e-9);
        assert!(one[0] > 8. && one[0] < 10.);
        assert_eq!(smooth_toward([0., 0.], [3., 4.], 0., 0.1), [3., 4.]);
    }

    #[test]
    fn values_blend_or_switch_halfway() {
        let a = AnimValue::Number(0.);
        let b = AnimValue::Number(10.);
        assert_eq!(blend_values(&a, &b, 0.25), AnimValue::Number(2.5));
        let path = AnimValue::Path("M0 0 L1 1".into());
        assert_eq!(blend_values(&a, &path, 0.4), a);
        assert_eq!(blend_values(&a, &path, 0.6), path);
    }
}
