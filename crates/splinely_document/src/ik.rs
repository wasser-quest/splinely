use crate::{DocumentError, Project};

impl Project {
    /// Authoring-time two-bone IK. Returns a validated pose; runtime uses ordinary
    /// rotations. Lower bone must be a direct child with no joint offset.
    /// Unreachable targets clamp to the nearest reachable radius.
    pub fn pose_two_bone_ik(
        &self,
        upper_id: &str,
        lower_id: &str,
        target: [f64; 2],
        bend_positive: bool,
    ) -> Result<Self, DocumentError> {
        self.validate()?;
        if self.bones.iter().any(|bone| bone.rotation.abs() > 1e6) {
            return Err(DocumentError("IK requires bone rotations within +/- 1000000 degrees; normalize large rotations first".into()));
        }
        if !target.iter().all(|v| v.is_finite() && v.abs() <= 1e30) {
            return Err(DocumentError(
                "IK target must be finite and within +/- 1e30".into(),
            ));
        }
        let upper = self
            .bones
            .iter()
            .position(|b| b.id == upper_id)
            .ok_or_else(|| DocumentError(format!("missing upper bone {upper_id}")))?;
        let lower = self
            .bones
            .iter()
            .position(|b| b.id == lower_id)
            .ok_or_else(|| DocumentError(format!("missing lower bone {lower_id}")))?;
        let a = &self.bones[upper];
        let b = &self.bones[lower];
        if b.parent_id.as_deref() != Some(upper_id) || b.x != 0.0 || b.y != 0.0 {
            return Err(DocumentError(
                "two-bone IK requires a direct child with zero joint offset".into(),
            ));
        }
        let pose = crate::bones::solve_bones(&self.bones)?
            .into_iter()
            .find(|p| p.id == upper_id)
            .unwrap();
        let dx = target[0] - pose.x;
        let dy = target[1] - pose.y;
        let distance = dx
            .hypot(dy)
            .clamp((a.length - b.length).abs(), a.length + b.length);
        let angle = if dx == 0.0 && dy == 0.0 {
            pose.rotation.to_radians()
        } else {
            dy.atan2(dx)
        };
        // Normalize first to avoid cancellation and overflow for huge lengths.
        let scale = a.length.max(b.length);
        let l1 = a.length / scale;
        let l2 = b.length / scale;
        if l1 * l2 == 0.0 {
            return Err(DocumentError("IK length ratio is too extreme".into()));
        }
        let d = distance / scale;
        let sum = l1 + l2;
        let difference = (l1 - l2).abs();
        let folded = ((sum - d).max(0.) * (sum + d)).sqrt();
        let extended = ((d - difference).max(0.) * (d + difference)).sqrt();
        let sign = if bend_positive { 1.0 } else { -1.0 };
        let elbow = 2.0 * folded.atan2(extended) * sign;
        let shoulder_offset = if d == 0.0 {
            std::f64::consts::FRAC_PI_2 * sign
        } else {
            let cosine = ((d * d + (l1 - l2) * sum) / (2.0 * d * l1)).clamp(-1., 1.);
            let sine = (folded * extended / (2.0 * d * l1)).clamp(0., 1.);
            sine.atan2(cosine) * sign
        };
        let shoulder = angle - shoulder_offset;
        let parent_angle = pose.rotation - a.rotation;
        let mut next = self.clone();
        let nearest = |angle: f64, previous: f64| {
            previous + (angle - previous + 180.0).rem_euclid(360.0) - 180.0
        };
        next.bones[upper].rotation = nearest(shoulder.to_degrees() - parent_angle, a.rotation);
        next.bones[lower].rotation = nearest(elbow.to_degrees(), b.rotation);
        next.validate()?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use crate::*;
    fn rig() -> Project {
        Project {
            bones: vec![
                Bone {
                    id: "a".into(),
                    parent_id: None,
                    x: 50.,
                    y: 70.,
                    rotation: 0.,
                    length: 100.,
                },
                Bone {
                    id: "b".into(),
                    parent_id: Some("a".into()),
                    x: 0.,
                    y: 0.,
                    rotation: 0.,
                    length: 100.,
                },
            ],
            ..Default::default()
        }
    }
    fn tip(p: &Project) -> [f64; 2] {
        let context = EvaluationContext {
            clips: Some(vec![]),
            ..Default::default()
        };
        let pose = p.evaluate_bones(0., &context).unwrap().pop().unwrap();
        [
            pose.x + pose.length * pose.rotation.to_radians().cos(),
            pose.y + pose.length * pose.rotation.to_radians().sin(),
        ]
    }
    #[test]
    fn reachable_both_bends_and_unreachable_clamp() {
        for bend in [true, false] {
            let p = rig()
                .pose_two_bone_ik("a", "b", [130., 150.], bend)
                .unwrap();
            let t = tip(&p);
            assert!((t[0] - 130.).abs() < 1e-8 && (t[1] - 150.).abs() < 1e-8);
        }
        let p = rig().pose_two_bone_ik("a", "b", [500., 70.], true).unwrap();
        assert!((tip(&p)[0] - 250.).abs() < 1e-8);
        assert!(rig()
            .pose_two_bone_ik("a", "b", [f64::NAN, 0.], true)
            .is_err());
    }
    #[test]
    fn validates_topology_and_folded_pose() {
        let mut p = rig();
        p.bones[1].x = 1.;
        assert!(p.pose_two_bone_ik("a", "b", [10., 20.], true).is_err());
        let p = rig().pose_two_bone_ik("a", "b", [50., 70.], true).unwrap();
        let t = tip(&p);
        assert!((t[0] - 50.).abs() < 1e-8 && (t[1] - 70.).abs() < 1e-8);
    }
    #[test]
    fn precision_guards_and_nearly_folded_lengths() {
        let mut p = rig();
        p.bones[0].rotation = 1e20;
        assert!(p.pose_two_bone_ik("a", "b", [100., 100.], true).is_err());
        p.bones[0].rotation = 0.;
        p.bones[0].x = 0.;
        p.bones[0].y = 0.;
        p.bones[0].length = 1e10;
        p.bones[1].length = 1e10;
        let posed = p.pose_two_bone_ik("a", "b", [1., 0.], true).unwrap();
        let actual = tip(&posed);
        assert!(
            (actual[0] - 1.).abs() < 1e-4 && actual[1].abs() < 1e-4,
            "{actual:?}"
        );
    }
}
