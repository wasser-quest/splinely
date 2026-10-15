use crate::*;
use std::collections::{HashMap, HashSet};

fn checked(values: &[f64]) -> Result<(), DocumentError> {
    if values.iter().all(|v| v.is_finite() && v.abs() <= 1e30) {
        Ok(())
    } else {
        Err(DocumentError(
            "bone pose/attachment numbers must be finite and within +/- 1e30 (overflow)".into(),
        ))
    }
}

fn rotate(angle: f64, x: f64, y: f64) -> (f64, f64) {
    let (s, c) = angle.rem_euclid(360.0).to_radians().sin_cos();
    (c * x - s * y, s * x + c * y)
}

pub(crate) fn solve_bones(bones: &[Bone]) -> Result<Vec<BonePose>, DocumentError> {
    let mut by_id = HashMap::new();
    for bone in bones {
        if by_id.insert(bone.id.as_str(), bone).is_some() {
            return Err(DocumentError(format!("duplicate bone ID {}", bone.id)));
        }
        checked(&[bone.x, bone.y, bone.rotation, bone.length])?;
        if bone.length <= 0.0 {
            return Err(DocumentError(format!(
                "bone {}: length must be positive",
                bone.id
            )));
        }
    }
    let mut poses = Vec::with_capacity(bones.len());
    for bone in bones {
        let mut chain = Vec::new();
        let mut seen = HashSet::new();
        let mut current = bone;
        loop {
            if !seen.insert(current.id.as_str()) {
                return Err(DocumentError(format!("bone cycle at {}", current.id)));
            }
            chain.push(current);
            if chain.len() > 128 {
                return Err(DocumentError("bone depth exceeds 128".into()));
            }
            match &current.parent_id {
                Some(parent) => {
                    current = by_id.get(parent.as_str()).copied().ok_or_else(|| {
                        DocumentError(format!("bone {}: missing parent {parent}", current.id))
                    })?
                }
                None => break,
            }
        }
        let (mut tip_x, mut tip_y, mut angle) = (0.0, 0.0, 0.0);
        let (mut x, mut y) = (0.0, 0.0);
        for local in chain.into_iter().rev() {
            let (dx, dy) = rotate(angle, local.x, local.y);
            x = tip_x + dx;
            y = tip_y + dy;
            angle += local.rotation;
            checked(&[x, y, angle])?;
            let (dx, dy) = rotate(angle, local.length, 0.0);
            tip_x = x + dx;
            tip_y = y + dy;
            checked(&[tip_x, tip_y])?;
        }
        poses.push(BonePose {
            id: bone.id.clone(),
            x,
            y,
            rotation: angle,
            length: bone.length,
        });
    }
    Ok(poses)
}

/// Apply exactly once to local nodes, never to previously evaluated nodes.
pub(crate) fn attach_nodes(nodes: &mut [Node], poses: &[BonePose]) -> Result<(), DocumentError> {
    let by_id: HashMap<_, _> = poses.iter().map(|p| (p.id.as_str(), p)).collect();
    for node in nodes {
        if let Some(id) = &node.bone_id {
            let pose = by_id
                .get(id.as_str())
                .ok_or_else(|| DocumentError(format!("node {}: missing bone {id}", node.id)))?;
            let t = &mut node.transform;
            let (x, y) = rotate(pose.rotation, t.x, t.y);
            t.x = pose.x + x;
            t.y = pose.y + y;
            t.rotation += pose.rotation;
            checked(&[t.x, t.y, t.rotation, t.scale_x, t.scale_y])?;
        }
    }
    Ok(())
}
