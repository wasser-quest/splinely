//! Arrangement operations shared by the editor and the CLI: world bounds,
//! moving, aligning, distributing, flipping, grouping, stacking and deletion.
//!
//! All operations work on the authored pose (time 0, no clips, no state
//! overrides) so they edit base values rather than an animated snapshot.
//! Every mutating method validates the result and leaves `self` unchanged on error.
use crate::*;
use std::collections::{HashMap, HashSet};

/// Axis-aligned rectangle `[x, y, width, height]` in scene coordinates.
pub type Bounds = [f64; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignEdge {
    Left,
    HorizontalCenter,
    Right,
    Top,
    VerticalCenter,
    Bottom,
}

impl std::str::FromStr for AlignEdge {
    type Err = DocumentError;
    fn from_str(s: &str) -> Result<Self, DocumentError> {
        Ok(match s {
            "left" => Self::Left,
            "hcenter" | "center" => Self::HorizontalCenter,
            "right" => Self::Right,
            "top" => Self::Top,
            "vcenter" | "middle" => Self::VerticalCenter,
            "bottom" => Self::Bottom,
            _ => {
                return Err(DocumentError(format!(
                    "unknown edge {s} (left, hcenter, right, top, vcenter, bottom)"
                )))
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl std::str::FromStr for Axis {
    type Err = DocumentError;
    fn from_str(s: &str) -> Result<Self, DocumentError> {
        match s {
            "horizontal" | "h" | "x" => Ok(Self::Horizontal),
            "vertical" | "v" | "y" => Ok(Self::Vertical),
            _ => Err(DocumentError(format!(
                "unknown axis {s} (horizontal, vertical)"
            ))),
        }
    }
}

/// What alignment is measured against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignTarget {
    /// Union of the given nodes (a single node aligns to the document).
    Selection,
    /// The document canvas.
    Document,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stack {
    Raise,
    Lower,
    Top,
    Bottom,
}

fn union(a: Bounds, b: Bounds) -> Bounds {
    let x0 = a[0].min(b[0]);
    let y0 = a[1].min(b[1]);
    let x1 = (a[0] + a[2]).max(b[0] + b[2]);
    let y1 = (a[1] + a[3]).max(b[1] + b[3]);
    [x0, y0, x1 - x0, y1 - y0]
}

/// Untransformed local bounds of a shape. Text uses an approximate advance.
fn local_bounds(kind: &NodeKind) -> Bounds {
    match kind {
        NodeKind::Rect {
            x,
            y,
            width,
            height,
            ..
        } => [*x, *y, *width, *height],
        NodeKind::Circle { cx, cy, radius } => [cx - radius, cy - radius, radius * 2., radius * 2.],
        NodeKind::Path { bounds, .. } => *bounds,
        NodeKind::Text {
            x,
            y,
            text,
            font_size,
        } => {
            let width = text.chars().count() as f64 * font_size * 0.6;
            [*x, y - font_size, width, font_size * 1.2]
        }
    }
}

/// Scene bounds of a node whose transform is already in scene space.
pub fn transformed_bounds(node: &Node) -> Bounds {
    let [x, y, w, h] = local_bounds(&node.kind);
    let t = node.transform;
    let (sin, cos) = t.rotation.to_radians().sin_cos();
    let mut result: Option<Bounds> = None;
    for (px, py) in [(x, y), (x + w, y), (x, y + h), (x + w, y + h)] {
        let (sx, sy) = (px * t.scale_x, py * t.scale_y);
        let wx = t.x + sx * cos - sy * sin;
        let wy = t.y + sx * sin + sy * cos;
        let point = [wx, wy, 0., 0.];
        result = Some(result.map_or(point, |r| union(r, point)));
    }
    result.unwrap_or_default()
}

fn rotate(degrees: f64, x: f64, y: f64) -> (f64, f64) {
    let (sin, cos) = degrees.to_radians().sin_cos();
    (x * cos - y * sin, x * sin + y * cos)
}

impl Project {
    pub(crate) fn authored(&self) -> Project {
        let mut base = self.clone();
        base.clips.clear();
        base.state_machines.clear();
        base
    }

    pub(crate) fn authored_context() -> EvaluationContext {
        EvaluationContext {
            clips: Some(vec![]),
            ..Default::default()
        }
    }

    /// Scene bounds of every node in its authored pose (bones applied).
    pub fn authored_bounds(&self) -> Result<HashMap<String, Bounds>, DocumentError> {
        let nodes = self
            .authored()
            .evaluate_with_context(0., &Self::authored_context())?;
        Ok(nodes
            .iter()
            .map(|n| (n.id.clone(), transformed_bounds(n)))
            .collect())
    }

    /// Union of the authored scene bounds of `ids`.
    pub fn bounds_of(&self, ids: &[String]) -> Result<Bounds, DocumentError> {
        let all = self.authored_bounds()?;
        let mut result: Option<Bounds> = None;
        for id in ids {
            let b = *all
                .get(id)
                .ok_or_else(|| DocumentError(format!("missing node {id}")))?;
            result = Some(result.map_or(b, |r| union(r, b)));
        }
        result.ok_or_else(|| DocumentError("no nodes given".into()))
    }

    fn bone_rotations(&self) -> Result<HashMap<String, f64>, DocumentError> {
        Ok(self
            .authored()
            .evaluate_bones(0., &Self::authored_context())?
            .into_iter()
            .map(|p| (p.id, p.rotation))
            .collect())
    }

    pub(crate) fn require_editable(&self, ids: &[String]) -> Result<(), DocumentError> {
        if ids.is_empty() {
            return Err(DocumentError("no nodes given".into()));
        }
        let mut seen = HashSet::new();
        for id in ids {
            let node = self
                .nodes
                .iter()
                .find(|n| &n.id == id)
                .ok_or_else(|| DocumentError(format!("missing node {id}")))?;
            if node.locked {
                return Err(DocumentError(format!("node {id} is locked")));
            }
            if !seen.insert(id) {
                return Err(DocumentError(format!("node {id} given twice")));
            }
        }
        Ok(())
    }

    pub(crate) fn commit_checked(&mut self, next: Project) -> Result<(), DocumentError> {
        next.validate()?;
        *self = next;
        Ok(())
    }

    /// Move nodes by a scene-space delta; bone-bound nodes get the equivalent
    /// local delta so they move visually by exactly `(dx, dy)`.
    pub fn translate_nodes(
        &mut self,
        ids: &[String],
        dx: f64,
        dy: f64,
    ) -> Result<(), DocumentError> {
        let deltas: Vec<(String, f64, f64)> = ids.iter().map(|id| (id.clone(), dx, dy)).collect();
        self.translate_each(&deltas)
    }

    fn translate_each(&mut self, deltas: &[(String, f64, f64)]) -> Result<(), DocumentError> {
        let ids: Vec<String> = deltas.iter().map(|d| d.0.clone()).collect();
        self.require_editable(&ids)?;
        let rotations = self.bone_rotations()?;
        let mut next = self.clone();
        for (id, dx, dy) in deltas {
            let node = next.nodes.iter_mut().find(|n| &n.id == id).unwrap();
            let angle = node
                .bone_id
                .as_ref()
                .and_then(|b| rotations.get(b))
                .copied()
                .unwrap_or(0.);
            let (lx, ly) = rotate(-angle, *dx, *dy);
            node.transform.x += lx;
            node.transform.y += ly;
        }
        self.commit_checked(next)
    }

    /// Align nodes to an edge or centre of their common bounds or the document.
    /// A single node always aligns to the document.
    pub fn align_nodes(
        &mut self,
        ids: &[String],
        edge: AlignEdge,
        target: AlignTarget,
    ) -> Result<(), DocumentError> {
        self.require_editable(ids)?;
        let all = self.authored_bounds()?;
        let reference = if target == AlignTarget::Document || ids.len() == 1 {
            [0., 0., self.width, self.height]
        } else {
            self.bounds_of(ids)?
        };
        let deltas: Vec<_> = ids
            .iter()
            .map(|id| {
                let b = all[id];
                let (dx, dy) = match edge {
                    AlignEdge::Left => (reference[0] - b[0], 0.),
                    AlignEdge::HorizontalCenter => {
                        (reference[0] + reference[2] / 2. - (b[0] + b[2] / 2.), 0.)
                    }
                    AlignEdge::Right => (reference[0] + reference[2] - (b[0] + b[2]), 0.),
                    AlignEdge::Top => (0., reference[1] - b[1]),
                    AlignEdge::VerticalCenter => {
                        (0., reference[1] + reference[3] / 2. - (b[1] + b[3] / 2.))
                    }
                    AlignEdge::Bottom => (0., reference[1] + reference[3] - (b[1] + b[3])),
                };
                (id.clone(), dx, dy)
            })
            .collect();
        self.translate_each(&deltas)
    }

    /// Make the gaps between at least three nodes equal along an axis,
    /// keeping the outermost nodes in place.
    pub fn distribute_nodes(&mut self, ids: &[String], axis: Axis) -> Result<(), DocumentError> {
        self.require_editable(ids)?;
        if ids.len() < 3 {
            return Err(DocumentError(
                "distribute needs at least three nodes".into(),
            ));
        }
        let all = self.authored_bounds()?;
        let (pos, size) = match axis {
            Axis::Horizontal => (0, 2),
            Axis::Vertical => (1, 3),
        };
        let mut order: Vec<&String> = ids.iter().collect();
        order.sort_by(|a, b| all[*a][pos].total_cmp(&all[*b][pos]));
        let start = all[order[0]][pos];
        let end = order
            .iter()
            .map(|id| all[*id][pos] + all[*id][size])
            .fold(f64::NEG_INFINITY, f64::max);
        let total: f64 = order.iter().map(|id| all[*id][size]).sum();
        let gap = (end - start - total) / (order.len() - 1) as f64;
        let mut cursor = start;
        let mut deltas = vec![];
        for id in order {
            let delta = cursor - all[id][pos];
            deltas.push(match axis {
                Axis::Horizontal => (id.clone(), delta, 0.),
                Axis::Vertical => (id.clone(), 0., delta),
            });
            cursor += all[id][size] + gap;
        }
        self.translate_each(&deltas)
    }

    /// Mirror nodes in scene space around the centre of their common bounds.
    pub fn flip_nodes(&mut self, ids: &[String], axis: Axis) -> Result<(), DocumentError> {
        self.require_editable(ids)?;
        let before = self.authored_bounds()?;
        let group = self.bounds_of(ids)?;
        let rotations = self.bone_rotations()?;
        let mut next = self.clone();
        for id in ids {
            let node = next.nodes.iter_mut().find(|n| &n.id == id).unwrap();
            let bone = node
                .bone_id
                .as_ref()
                .and_then(|b| rotations.get(b))
                .copied()
                .unwrap_or(0.);
            // F·R(r) = R(-r)·F for an axis mirror F; in bone space the world
            // angle is bone + local, so local' = -local - 2·bone.
            // `+ 0.` turns -0.0 into 0.0 so editors do not show "-0".
            node.transform.rotation = -node.transform.rotation - 2. * bone + 0.;
            match axis {
                Axis::Horizontal => node.transform.scale_x = -node.transform.scale_x,
                Axis::Vertical => node.transform.scale_y = -node.transform.scale_y,
            }
        }
        // Re-centre: each node's mirrored bounds land mirrored inside the group.
        let after = next.authored_bounds()?;
        let deltas: Vec<_> = ids
            .iter()
            .map(|id| {
                let (b, a) = (before[id], after[id]);
                match axis {
                    Axis::Horizontal => {
                        let target = 2. * (group[0] + group[2] / 2.) - (b[0] + b[2] / 2.);
                        (id.clone(), target - (a[0] + a[2] / 2.), 0.)
                    }
                    Axis::Vertical => {
                        let target = 2. * (group[1] + group[3] / 2.) - (b[1] + b[3] / 2.);
                        (id.clone(), 0., target - (a[1] + a[3] / 2.))
                    }
                }
            })
            .collect();
        next.translate_each(&deltas)?;
        self.commit_checked(next)
    }

    /// Scene-space transforms (bones applied) of the authored pose.
    fn world_transforms(&self) -> Result<HashMap<String, Transform>, DocumentError> {
        Ok(self
            .authored()
            .evaluate_with_context(0., &Self::authored_context())?
            .into_iter()
            .map(|n| (n.id, n.transform))
            .collect())
    }

    /// Scale nodes in scene space around `anchor`. Non-uniform scaling is only
    /// exact for shapes rotated by a multiple of 90°, so other rotations are
    /// rejected (scale them uniformly). Negative factors mirror.
    pub fn scale_nodes(
        &mut self,
        ids: &[String],
        sx: f64,
        sy: f64,
        anchor: (f64, f64),
    ) -> Result<(), DocumentError> {
        self.require_editable(ids)?;
        if !(sx.is_finite() && sy.is_finite()) || sx.abs() < 1e-6 || sy.abs() < 1e-6 {
            return Err(DocumentError(
                "scale factors must be finite and nonzero".into(),
            ));
        }
        let world = self.world_transforms()?;
        let uniform = (sx - sy).abs() < 1e-12;
        let mut next = self.clone();
        let mut deltas = vec![];
        for id in ids {
            let w = world[id];
            let quarter = w.rotation.rem_euclid(90.);
            let axis_aligned = quarter < 1e-6 || 90. - quarter < 1e-6;
            if !uniform && !axis_aligned {
                return Err(DocumentError(format!(
                    "node {id} is rotated; scale it uniformly"
                )));
            }
            let swapped = !uniform && ((w.rotation.rem_euclid(180.) - 90.).abs() < 1e-6);
            let node = next.nodes.iter_mut().find(|n| &n.id == id).unwrap();
            if swapped {
                node.transform.scale_x *= sy;
                node.transform.scale_y *= sx;
            } else {
                node.transform.scale_x *= sx;
                node.transform.scale_y *= sy;
            }
            let target = (
                anchor.0 + (w.x - anchor.0) * sx,
                anchor.1 + (w.y - anchor.1) * sy,
            );
            deltas.push((id.clone(), target.0 - w.x, target.1 - w.y));
        }
        next.translate_each(&deltas)?;
        self.commit_checked(next)
    }

    /// Rotate nodes by `degrees` (clockwise on screen) around a scene point.
    pub fn rotate_nodes(
        &mut self,
        ids: &[String],
        degrees: f64,
        pivot: (f64, f64),
    ) -> Result<(), DocumentError> {
        self.require_editable(ids)?;
        if !degrees.is_finite() {
            return Err(DocumentError("rotation must be finite".into()));
        }
        let world = self.world_transforms()?;
        let mut next = self.clone();
        let mut deltas = vec![];
        for id in ids {
            let w = world[id];
            let node = next.nodes.iter_mut().find(|n| &n.id == id).unwrap();
            node.transform.rotation =
                (node.transform.rotation + degrees + 180.).rem_euclid(360.) - 180. + 0.;
            let (rx, ry) = rotate(degrees, w.x - pivot.0, w.y - pivot.1);
            deltas.push((id.clone(), pivot.0 + rx - w.x, pivot.1 + ry - w.y));
        }
        next.translate_each(&deltas)?;
        self.commit_checked(next)
    }

    /// Change stacking order. The relative order of the moved nodes is kept.
    pub fn restack_nodes(&mut self, ids: &[String], stack: Stack) -> Result<(), DocumentError> {
        self.require_editable(ids)?;
        let selected = |n: &Node| ids.contains(&n.id);
        let mut nodes = self.nodes.clone();
        match stack {
            Stack::Top | Stack::Bottom => {
                let (moved, rest): (Vec<_>, Vec<_>) = nodes.into_iter().partition(|n| selected(n));
                nodes = if stack == Stack::Top {
                    rest.into_iter().chain(moved).collect()
                } else {
                    moved.into_iter().chain(rest).collect()
                };
            }
            Stack::Raise => {
                for i in (0..nodes.len().saturating_sub(1)).rev() {
                    if selected(&nodes[i]) && !selected(&nodes[i + 1]) {
                        nodes.swap(i, i + 1);
                    }
                }
            }
            Stack::Lower => {
                for i in 1..nodes.len() {
                    if selected(&nodes[i]) && !selected(&nodes[i - 1]) {
                        nodes.swap(i, i - 1);
                    }
                }
            }
        }
        let mut next = self.clone();
        next.nodes = nodes;
        self.commit_checked(next)
    }

    /// Delete a node together with its tracks, state overrides, property actions
    /// and events targeting it. Undo is the caller's responsibility.
    pub fn remove_node_with_references(&mut self, id: &str) -> Result<(), DocumentError> {
        if !self.nodes.iter().any(|n| n.id == id) {
            return Err(DocumentError(format!("missing node {id}")));
        }
        let mut next = self.clone();
        next.nodes.retain(|n| n.id != id);
        let mut unbound = Vec::new();
        for node in &mut next.nodes {
            if node.motion_path.as_ref().is_some_and(|m| m.path == id) {
                unbound.push(node.id.clone());
                node.motion_path = None;
            }
        }
        let removed_progress = |target: &str, property: Property| {
            property == Property::MotionProgress && unbound.iter().any(|id| id == target)
        };
        for clip in &mut next.clips {
            clip.tracks
                .retain(|t| t.node_id != id && !removed_progress(&t.node_id, t.property));
        }
        let keep_action = |a: &Action| {
            !matches!(a, Action::SetProperty { node_id, property, .. } if node_id == id || removed_progress(node_id, *property))
                && !matches!(a, Action::Emit { event } if event.target.as_deref() == Some(id))
        };
        next.prune_empty_groups();
        next.followers.retain(|f| f.target != id);
        for machine in &mut next.state_machines {
            for state in &mut machine.states {
                state
                    .overrides
                    .retain(|o| o.node_id != id && !removed_progress(&o.node_id, o.property));
                state.on_enter.retain(keep_action);
                state.on_exit.retain(keep_action);
            }
            machine
                .transitions
                .retain(|t| t.event.target.as_deref() != Some(id));
            for transition in &mut machine.transitions {
                transition.actions.retain(keep_action);
            }
        }
        self.commit_checked(next)
    }

    /// `count` distinct IDs not used anywhere in the shared namespace.
    pub fn unused_ids(&self, prefix: &str, count: usize) -> Vec<String> {
        let mut ids: Vec<String> = Vec::with_capacity(count);
        for _ in 0..count {
            let id = self.unused_id_excluding(prefix, &ids);
            ids.push(id);
        }
        ids
    }

    /// A node ID not used anywhere in the shared ID namespace.
    pub fn unused_id(&self, prefix: &str) -> String {
        self.unused_id_excluding(prefix, &[])
    }

    /// Whether `id` is taken anywhere in the shared ID namespace.
    pub(crate) fn id_in_use(&self, id: &str) -> bool {
        self.unused_id_excluding_check(id, &[])
    }

    fn unused_id_excluding(&self, prefix: &str, reserved: &[String]) -> String {
        (1..)
            .map(|i| format!("{prefix}-{i}"))
            .find(|id| !self.unused_id_excluding_check(id, reserved))
            .unwrap()
    }

    fn unused_id_excluding_check(&self, id: &str, reserved: &[String]) -> bool {
        id == "splinely-project"
            || reserved.iter().any(|r| r == id)
            || self.nodes.iter().any(|n| n.id == id)
            || self.bones.iter().any(|n| n.id == id)
            || self.clips.iter().any(|n| n.id == id)
            || self.state_machines.iter().any(|n| n.id == id)
            || self.audio_assets.iter().any(|n| n.id == id)
            || self.audio_cues.iter().any(|n| n.id == id)
            || self
                .clips
                .iter()
                .flat_map(|c| &c.audio_cues)
                .any(|n| n.id == id)
            || self.followers.iter().any(|n| n.id == id)
            || self.groups.iter().any(|g| g.id == id)
            || self.blend_spaces.iter().any(|s| s.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn scene() -> Project {
        Project {
            nodes: vec![
                Node::rect("a", 10., 10., 20., 20.),
                Node::rect("b", 50., 40., 40., 10.),
                Node::circle("c", 200., 100., 10.),
            ],
            ..Project::default()
        }
    }

    fn close(a: Bounds, b: Bounds) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
    }

    #[test]
    fn bounds_follow_transform_rotation_and_scale() {
        let mut n = Node::rect("r", 0., 0., 10., 20.);
        n.transform.x = 100.;
        n.transform.rotation = 90.;
        assert!(close(transformed_bounds(&n), [80., 0., 20., 10.]));
        n.transform.rotation = 0.;
        n.transform.scale_x = -2.;
        assert!(close(transformed_bounds(&n), [80., 0., 20., 20.]));
    }

    #[test]
    fn align_left_center_and_document() {
        let mut p = scene();
        p.align_nodes(
            &ids(&["a", "b", "c"]),
            AlignEdge::Left,
            AlignTarget::Selection,
        )
        .unwrap();
        let b = p.authored_bounds().unwrap();
        for id in ["a", "b", "c"] {
            assert!((b[id][0] - 10.).abs() < 1e-9, "{id}");
        }
        p.align_nodes(
            &ids(&["a"]),
            AlignEdge::VerticalCenter,
            AlignTarget::Selection,
        )
        .unwrap();
        let b = p.authored_bounds().unwrap();
        assert!((b["a"][1] + b["a"][3] / 2. - 300.).abs() < 1e-9);
    }

    #[test]
    fn distribute_makes_gaps_equal_and_keeps_ends() {
        let mut p = scene();
        p.distribute_nodes(&ids(&["c", "a", "b"]), Axis::Horizontal)
            .unwrap();
        let b = p.authored_bounds().unwrap();
        assert!((b["a"][0] - 10.).abs() < 1e-9);
        assert!((b["c"][0] + b["c"][2] - 210.).abs() < 1e-9);
        let gap1 = b["b"][0] - (b["a"][0] + b["a"][2]);
        let gap2 = b["c"][0] - (b["b"][0] + b["b"][2]);
        assert!((gap1 - gap2).abs() < 1e-9);
        assert!(p
            .distribute_nodes(&ids(&["a", "b"]), Axis::Vertical)
            .is_err());
    }

    #[test]
    fn flip_mirrors_inside_the_selection_bounds() {
        let mut p = scene();
        let before = p.bounds_of(&ids(&["a", "b"])).unwrap();
        p.flip_nodes(&ids(&["a", "b"]), Axis::Horizontal).unwrap();
        assert!(close(p.bounds_of(&ids(&["a", "b"])).unwrap(), before));
        let b = p.authored_bounds().unwrap();
        // a was on the left edge; it is now on the right edge.
        assert!((b["a"][0] + b["a"][2] - (before[0] + before[2])).abs() < 1e-9);
        assert_eq!(p.nodes[0].transform.scale_x, -1.);
        // Flipping twice restores the original bounds.
        p.flip_nodes(&ids(&["a", "b"]), Axis::Horizontal).unwrap();
        assert!(close(
            p.authored_bounds().unwrap()["a"],
            [10., 10., 20., 20.]
        ));
    }

    #[test]
    fn rotated_flip_keeps_the_shape_in_place() {
        let mut p = scene();
        p.nodes[1].transform.rotation = 30.;
        let before = p.authored_bounds().unwrap()["b"];
        p.flip_nodes(&ids(&["b"]), Axis::Vertical).unwrap();
        assert_eq!(p.nodes[1].transform.rotation, -30.);
        assert!(close(p.authored_bounds().unwrap()["b"], before));
    }

    #[test]
    fn bone_bound_nodes_move_visually_by_the_scene_delta() {
        let mut p = scene();
        p.bones.push(Bone {
            id: "arm".into(),
            parent_id: None,
            x: 0.,
            y: 0.,
            rotation: 90.,
            length: 10.,
        });
        p.nodes[0].bone_id = Some("arm".into());
        let before = p.authored_bounds().unwrap()["a"];
        p.translate_nodes(&ids(&["a"]), 5., 0.).unwrap();
        let after = p.authored_bounds().unwrap()["a"];
        assert!((after[0] - before[0] - 5.).abs() < 1e-9);
        assert!((after[1] - before[1]).abs() < 1e-9);
    }

    #[test]
    fn scaling_and_rotation_around_an_anchor() {
        let mut p = scene();
        // Scale a and b by 2 around the top-left of a.
        p.scale_nodes(&ids(&["a", "b"]), 2., 2., (10., 10.))
            .unwrap();
        let b = p.authored_bounds().unwrap();
        assert!(close(b["a"], [10., 10., 40., 40.]));
        assert!(close(b["b"], [90., 70., 80., 20.]));
        // Non-uniform scaling of an axis-aligned shape.
        p.scale_nodes(&ids(&["c"]), 2., 0.5, (200., 100.)).unwrap();
        assert!(close(
            p.authored_bounds().unwrap()["c"],
            [180., 95., 40., 10.]
        ));
        // 90° rotation swaps the axes.
        let before = p.authored_bounds().unwrap()["c"];
        p.rotate_nodes(&ids(&["c"]), 90., (200., 100.)).unwrap();
        let after = p.authored_bounds().unwrap()["c"];
        assert!(close(after, [before[0] + 15., before[1] - 15., 10., 40.]));
        p.scale_nodes(&ids(&["c"]), 1., 3., (200., 100.)).unwrap();
        assert!((p.authored_bounds().unwrap()["c"][3] - 120.).abs() < 1e-9);
        // 30° rotated shapes can only be scaled uniformly.
        p.rotate_nodes(&ids(&["c"]), 30., (200., 100.)).unwrap();
        assert!(p.scale_nodes(&ids(&["c"]), 2., 1., (0., 0.)).is_err());
        assert!(p.scale_nodes(&ids(&["c"]), 0., 0., (0., 0.)).is_err());
        p.scale_nodes(&ids(&["c"]), 0.5, 0.5, (200., 100.)).unwrap();
        // Rotating a full turn in steps returns to the start.
        let start = p.authored_bounds().unwrap()["a"];
        for _ in 0..4 {
            p.rotate_nodes(&ids(&["a"]), 90., (0., 0.)).unwrap();
        }
        assert!(close(p.authored_bounds().unwrap()["a"], start));
    }

    #[test]
    fn stacking_and_locked_nodes() {
        let mut p = scene();
        p.restack_nodes(&ids(&["a"]), Stack::Top).unwrap();
        assert_eq!(p.nodes.last().unwrap().id, "a");
        p.restack_nodes(&ids(&["a"]), Stack::Lower).unwrap();
        assert_eq!(p.nodes[1].id, "a");
        p.restack_nodes(&ids(&["a", "c"]), Stack::Bottom).unwrap();
        assert_eq!(
            p.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            ["a", "c", "b"]
        );
        p.nodes[2].locked = true;
        let before = p.clone();
        assert!(p.translate_nodes(&ids(&["b"]), 1., 1.).is_err());
        assert_eq!(p, before);
        let json = p.to_json().unwrap();
        assert_eq!(Project::from_json(&json).unwrap(), p);
        assert!(!Project::default().to_json().unwrap().contains("group"));
    }

    #[test]
    fn removing_a_node_cleans_references() {
        let mut p = scene();
        p.clips.push(
            serde_json::from_value(serde_json::json!({
                "id": "move", "duration": 1,
                "tracks": [{"nodeId": "a", "property": "x",
                    "keyframes": [{"time": 0, "value": {"type": "number", "value": 1}}]}]
            }))
            .unwrap(),
        );
        p.followers.push(Follower {
            id: "look".into(),
            target: "a".into(),
            mode: FollowMode::Offset,
            strength: 1.,
            max: 5.,
            smoothing: 0.,
            rest_angle: 0.,
            origin: None,
            when: None,
        });
        p.validate().unwrap();
        let id = "a".to_string();
        p.remove_node_with_references(&id).unwrap();
        assert!(!p.nodes.iter().any(|n| n.id == id));
        assert!(p.followers.is_empty());
        assert!(p.remove_node_with_references("nope").is_err());
        assert_eq!(p.unused_id("node"), "node-1");
    }
}
