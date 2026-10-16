//! Nested groups. A node belongs to at most one direct group; groups can sit
//! inside other groups. Grouping collects the selection under its lowest common
//! group, so grouping groups creates real hierarchies; ungrouping removes one
//! level. Group members are kept contiguous in the stacking order so the layer
//! tree and the exported nested `<g>` elements stay simple.
use crate::*;
use std::collections::HashMap;

impl Project {
    fn group(&self, id: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }

    /// Group IDs from the outermost group down to the node's direct group.
    pub fn group_path(&self, node: &str) -> Vec<String> {
        let mut path = vec![];
        let mut current = self
            .nodes
            .iter()
            .find(|n| n.id == node)
            .and_then(|n| n.group.clone());
        while let Some(id) = current {
            current = self.group(&id).and_then(|g| g.parent_id.clone());
            path.push(id);
            if path.len() > 32 {
                break;
            }
        }
        path.reverse();
        path
    }

    /// Whether `group` is `ancestor` or nested inside it.
    fn group_within(&self, group: &str, ancestor: &str) -> bool {
        let mut current = Some(group.to_owned());
        let mut steps = 0;
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            steps += 1;
            if steps > 32 {
                return false;
            }
            current = self.group(&id).and_then(|g| g.parent_id.clone());
        }
        false
    }

    /// All nodes inside a group, at any depth, in stacking order.
    pub fn nodes_in_group(&self, group: &str) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|n| {
                n.group
                    .as_deref()
                    .is_some_and(|g| self.group_within(g, group))
            })
            .map(|n| n.id.clone())
            .collect()
    }

    /// The nodes a click on `id` selects: everything in its outermost group.
    pub fn group_members(&self, id: &str) -> Vec<String> {
        match self.group_path(id).first() {
            Some(top) => self.nodes_in_group(top),
            None => vec![id.to_owned()],
        }
    }

    /// A display name not used by any group yet.
    pub fn unused_group_name(&self) -> String {
        (1..)
            .map(|i| format!("Gruppe {i}"))
            .find(|name| !self.groups.iter().any(|g| &g.name == name))
            .unwrap()
    }

    /// Group the nodes (and the groups they belong to below their lowest
    /// common group). Returns the new group's ID.
    pub fn group_nodes(
        &mut self,
        ids: &[String],
        name: Option<&str>,
    ) -> Result<String, DocumentError> {
        self.require_editable(ids)?;
        if ids.len() < 2 {
            return Err(DocumentError("grouping needs at least two shapes".into()));
        }
        let paths: Vec<Vec<String>> = ids.iter().map(|id| self.group_path(id)).collect();
        // Lowest common ancestor: longest shared prefix of all group paths.
        let mut common = paths[0].clone();
        for path in &paths[1..] {
            let shared = common.iter().zip(path).take_while(|(a, b)| a == b).count();
            common.truncate(shared);
        }
        // Grouping exactly one existing group again would only add a wrapper.
        let depth = common.len();
        let mut next = self.clone();
        let id = next.unused_id("group");
        next.groups.push(Group {
            id: id.clone(),
            name: name
                .map(str::to_owned)
                .unwrap_or_else(|| self.unused_group_name()),
            parent_id: common.last().cloned(),
            rig: None,
        });
        for (node, path) in ids.iter().zip(&paths) {
            match path.get(depth) {
                // The node sits directly in the common group.
                None => {
                    next.nodes.iter_mut().find(|n| &n.id == node).unwrap().group = Some(id.clone())
                }
                // Move the whole sub-group under the new group.
                Some(unit) => {
                    next.groups
                        .iter_mut()
                        .find(|g| &g.id == unit)
                        .unwrap()
                        .parent_id = Some(id.clone())
                }
            }
        }
        next.make_group_contiguous(&id);
        next.prune_empty_groups();
        next.validate()?;
        *self = next;
        Ok(id)
    }

    /// Remove the outermost group of each given node (one level per call).
    pub fn ungroup_nodes(&mut self, ids: &[String]) -> Result<(), DocumentError> {
        self.require_editable(ids)?;
        let mut tops: Vec<String> = ids
            .iter()
            .filter_map(|id| self.group_path(id).first().cloned())
            .collect();
        tops.sort();
        tops.dedup();
        if tops.is_empty() {
            return Err(DocumentError("no group selected".into()));
        }
        let mut next = self.clone();
        for top in tops {
            let parent = next.group(&top).and_then(|g| g.parent_id.clone());
            for g in next
                .groups
                .iter_mut()
                .filter(|g| g.parent_id.as_deref() == Some(&top))
            {
                g.parent_id = parent.clone();
            }
            for n in next
                .nodes
                .iter_mut()
                .filter(|n| n.group.as_deref() == Some(&top))
            {
                n.group = parent.clone();
            }
            next.groups.retain(|g| g.id != top);
        }
        next.validate()?;
        *self = next;
        Ok(())
    }

    /// Rename a group.
    pub fn rename_group(&mut self, id: &str, name: &str) -> Result<(), DocumentError> {
        let mut next = self.clone();
        next.groups
            .iter_mut()
            .find(|g| g.id == id)
            .ok_or_else(|| DocumentError(format!("missing group {id}")))?
            .name = name.to_owned();
        next.validate()?;
        *self = next;
        Ok(())
    }

    /// Move a group's nodes next to its topmost member, keeping their order.
    fn make_group_contiguous(&mut self, group: &str) {
        let members = self.nodes_in_group(group);
        let Some(top) = self.nodes.iter().rposition(|n| members.contains(&n.id)) else {
            return;
        };
        // Non-members below the topmost member stay below the group.
        let below = self.nodes[..top]
            .iter()
            .filter(|n| !members.contains(&n.id))
            .count();
        let (moved, mut rest): (Vec<Node>, Vec<Node>) = std::mem::take(&mut self.nodes)
            .into_iter()
            .partition(|n| members.contains(&n.id));
        rest.splice(below..below, moved);
        self.nodes = rest;
    }

    /// Drop groups that contain neither nodes nor other groups.
    pub fn prune_empty_groups(&mut self) {
        loop {
            let used = |g: &Group, p: &Project| {
                p.nodes.iter().any(|n| n.group.as_deref() == Some(&g.id))
                    || p.groups
                        .iter()
                        .any(|c| c.parent_id.as_deref() == Some(&g.id))
            };
            let before = self.groups.len();
            let keep: Vec<bool> = self.groups.iter().map(|g| used(g, self)).collect();
            let mut keep = keep.into_iter();
            self.groups.retain(|_| keep.next().unwrap());
            if self.groups.len() == before {
                break;
            }
        }
    }

    /// Prepare copies of nodes from `source` for insertion into `self`: their
    /// groups are recreated as new groups (same names and nesting) so copies
    /// never join the originals' groups. Node IDs are not changed here.
    pub fn adopt_group_structure(&mut self, copies: &mut [Node], source: &Project) {
        let mut mapping: HashMap<String, String> = HashMap::new();
        for node in copies.iter_mut() {
            let Some(direct) = node.group.clone() else {
                continue;
            };
            // Recreate the chain outermost-first.
            let mut chain = vec![];
            let mut current = Some(direct);
            while let Some(id) = current {
                current = source.group(&id).and_then(|g| g.parent_id.clone());
                chain.push(id);
                if chain.len() > 32 {
                    break;
                }
            }
            chain.reverse();
            let mut parent: Option<String> = None;
            for old in chain {
                let new = match mapping.get(&old) {
                    Some(new) => new.clone(),
                    None => {
                        let new = self.unused_id("group");
                        let name = source
                            .group(&old)
                            .map(|g| g.name.clone())
                            .unwrap_or_default();
                        self.groups.push(Group {
                            id: new.clone(),
                            name,
                            parent_id: parent.clone(),
                            rig: None,
                        });
                        mapping.insert(old, new.clone());
                        new
                    }
                };
                parent = Some(new);
            }
            node.group = parent;
        }
    }
}

/// Bone of a group rig that carries `property`, if groups can animate it.
pub fn group_rig_bone(rig: &GroupRig, property: Property) -> Option<&str> {
    match property {
        Property::X | Property::Y => Some(&rig.move_bone),
        Property::Rotation => Some(&rig.turn_bone),
        _ => None,
    }
}

impl Project {
    /// Whether `id` is a group.
    pub fn is_group(&self, id: &str) -> bool {
        self.group(id).is_some()
    }

    /// Make group `id` animatable as one piece and return its rig.
    ///
    /// Adds three bones (move → turn → attach) and binds every shape of the
    /// group to the attach bone. The chain is the identity at rest, so nothing
    /// moves and existing shape animations keep working. Calling it again
    /// returns the existing rig. Shapes that are already bound to bones or to
    /// motion paths cannot join a rig.
    pub fn rig_group(&mut self, id: &str) -> Result<GroupRig, DocumentError> {
        let group = self
            .group(id)
            .ok_or_else(|| DocumentError(format!("missing group {id}")))?;
        if let Some(rig) = &group.rig {
            return Ok(rig.clone());
        }
        let members = self.nodes_in_group(id);
        if members.is_empty() {
            return Err(DocumentError(format!(
                "group {id} has no shapes to animate"
            )));
        }
        for node in self.nodes.iter().filter(|n| members.contains(&n.id)) {
            if let Some(bone) = &node.bone_id {
                return Err(DocumentError(format!(
                    "group {id}: shape {} is already bound to bone {bone}; animate that bone, or unbind the shape to animate the group as a whole",
                    node.id
                )));
            }
            if node.motion_path.is_some() {
                return Err(DocumentError(format!(
                    "group {id}: shape {} follows a motion path; remove it to animate the group as a whole",
                    node.id
                )));
            }
        }
        let [x, y, w, h] = self.bounds_of(&members)?;
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);

        let mut next = self.clone();
        let fresh = |suffix: &str, taken: &[String]| {
            let base = format!("{id}-{suffix}");
            if next.id_in_use(&base) || taken.contains(&base) {
                next.unused_ids(&base, taken.len() + 1)
                    .into_iter()
                    .find(|candidate| !taken.contains(candidate))
                    .expect("unused_ids returns enough IDs")
            } else {
                base
            }
        };
        let mut ids = Vec::new();
        for suffix in ["move", "turn", "attach"] {
            let next_id = fresh(suffix, &ids);
            ids.push(next_id);
        }
        let rig = GroupRig {
            move_bone: ids[0].clone(),
            turn_bone: ids[1].clone(),
            attach_bone: ids[2].clone(),
        };
        // Child offsets start at the parent tip; every bone is 1 unit long.
        // turn sits on the centre; attach returns to the scene origin, so the
        // whole chain is the identity until move/turn are animated.
        next.bones.push(Bone {
            id: rig.move_bone.clone(),
            parent_id: None,
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            length: 1.0,
        });
        next.bones.push(Bone {
            id: rig.turn_bone.clone(),
            parent_id: Some(rig.move_bone.clone()),
            x: cx - 1.0,
            y: cy,
            rotation: 0.0,
            length: 1.0,
        });
        next.bones.push(Bone {
            id: rig.attach_bone.clone(),
            parent_id: Some(rig.turn_bone.clone()),
            x: -cx - 1.0,
            y: -cy,
            rotation: 0.0,
            length: 1.0,
        });
        for node in next.nodes.iter_mut().filter(|n| members.contains(&n.id)) {
            node.bone_id = Some(rig.attach_bone.clone());
        }
        next.groups
            .iter_mut()
            .find(|g| g.id == id)
            .expect("group checked above")
            .rig = Some(rig.clone());
        next.validate()?;
        *self = next;
        Ok(rig)
    }

    /// Resolve an animation target: a group becomes the rig bone that carries
    /// `property` (creating the rig on first use); anything else is returned
    /// unchanged.
    pub fn animation_target(
        &mut self,
        target: &str,
        property: Property,
    ) -> Result<String, DocumentError> {
        if !self.is_group(target) {
            return Ok(target.to_owned());
        }
        if !matches!(property, Property::X | Property::Y | Property::Rotation) {
            return Err(DocumentError(format!(
                "group {target}: groups animate x, y and rotation; animate {property:?} on the shapes inside the group"
            )));
        }
        let rig = self.rig_group(target)?;
        Ok(group_rig_bone(&rig, property)
            .expect("property checked above")
            .to_owned())
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
            nodes: ["a", "b", "c", "d", "e"]
                .iter()
                .enumerate()
                .map(|(i, id)| Node::rect(*id, i as f64 * 20., 0., 10., 10.))
                .collect(),
            ..Project::default()
        }
    }

    fn order(p: &Project) -> Vec<&str> {
        p.nodes.iter().map(|n| n.id.as_str()).collect()
    }

    fn rigged() -> (Project, GroupRig) {
        let mut p = scene();
        let group = p.group_nodes(&ids(&["a", "b"]), None).unwrap();
        let rig = p.rig_group(&group).unwrap();
        (p, rig)
    }

    fn pose(p: &Project, id: &str) -> Transform {
        p.evaluate(0.)
            .unwrap()
            .into_iter()
            .find(|n| n.id == id)
            .unwrap()
            .transform
    }

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    #[test]
    fn group_rig_is_identity_at_rest_and_turns_around_the_centre() {
        let before = scene().evaluate(0.).unwrap();
        let (mut p, rig) = rigged();
        for node in &before {
            assert_eq!(pose(&p, &node.id), node.transform, "{}", node.id);
        }
        assert!(p
            .nodes
            .iter()
            .filter(|n| n.id == "a" || n.id == "b")
            .all(|n| n.bone_id.as_deref() == Some(rig.attach_bone.as_str())));
        // a and b span x 0..30, y 0..10: centre (15, 5). Turning a quarter
        // moves a's origin (0, 0) to (20, -10).
        p.bones
            .iter_mut()
            .find(|b| b.id == rig.turn_bone)
            .unwrap()
            .rotation = 90.;
        let a = pose(&p, "a");
        close(a.x, 20.);
        close(a.y, -10.);
        close(a.rotation, 90.);
        p.bones
            .iter_mut()
            .find(|b| b.id == rig.move_bone)
            .unwrap()
            .y = 7.;
        close(pose(&p, "a").y, -3.);
    }

    #[test]
    fn group_rig_is_reused_and_avoids_taken_ids() {
        let mut p = scene();
        p.nodes.push(Node::rect("group-1-move", 200., 0., 5., 5.));
        let group = p.group_nodes(&ids(&["a", "b"]), None).unwrap();
        let rig = p.rig_group(&group).unwrap();
        assert_eq!(rig.move_bone, "group-1-move-1");
        assert_eq!(rig.turn_bone, "group-1-turn");
        let bones = p.bones.len();
        assert_eq!(p.rig_group(&group).unwrap(), rig);
        assert_eq!(p.bones.len(), bones);
    }

    #[test]
    fn group_rig_refuses_bone_bound_shapes_and_changes_nothing() {
        let mut p = scene();
        p.bones.push(Bone {
            id: "arm".into(),
            parent_id: None,
            x: 0.,
            y: 0.,
            rotation: 0.,
            length: 1.,
        });
        p.nodes[0].bone_id = Some("arm".into());
        let group = p.group_nodes(&ids(&["a", "b"]), None).unwrap();
        let before = p.clone();
        let error = p.rig_group(&group).unwrap_err();
        assert!(error.0.contains("bound to bone arm"), "{error:?}");
        assert_eq!(p, before);
    }

    #[test]
    fn animation_targets_map_groups_to_rig_bones() {
        let (mut p, rig) = rigged();
        assert_eq!(
            p.animation_target("group-1", Property::X).unwrap(),
            rig.move_bone
        );
        assert_eq!(
            p.animation_target("group-1", Property::Y).unwrap(),
            rig.move_bone
        );
        assert_eq!(
            p.animation_target("group-1", Property::Rotation).unwrap(),
            rig.turn_bone
        );
        assert!(p.animation_target("group-1", Property::Opacity).is_err());
        assert_eq!(p.animation_target("c", Property::Opacity).unwrap(), "c");
        assert_eq!(group_rig_bone(&rig, Property::ScaleX), None);
    }

    #[test]
    fn broken_group_rigs_fail_validation() {
        let (mut p, rig) = rigged();
        p.bones.retain(|b| b.id != rig.turn_bone);
        p.bones
            .iter_mut()
            .find(|b| b.id == rig.attach_bone)
            .unwrap()
            .parent_id = Some(rig.move_bone.clone());
        assert!(p.validate().unwrap_err().0.contains("rig needs bones"));
    }

    #[test]
    fn grouping_nests_and_ungrouping_removes_one_level() {
        let mut p = scene();
        let inner = p.group_nodes(&ids(&["a", "b"]), None).unwrap();
        assert_eq!(p.groups[0].name, "Gruppe 1");
        assert_eq!(p.group_members("a"), ids(&["a", "b"]));
        // Group the group with c: a nested hierarchy.
        let outer = p
            .group_nodes(&ids(&["a", "b", "c"]), Some("Figur"))
            .unwrap();
        assert_eq!(
            p.group(&inner).unwrap().parent_id.as_deref(),
            Some(outer.as_str())
        );
        assert_eq!(p.group_path("a"), vec![outer.clone(), inner.clone()]);
        assert_eq!(p.group_path("c"), vec![outer.clone()]);
        assert_eq!(p.group_members("b"), ids(&["a", "b", "c"]));
        assert_eq!(p.group_members("d"), ids(&["d"]));
        p.validate().unwrap();
        // Ungroup removes only the outer level.
        p.ungroup_nodes(&ids(&["a"])).unwrap();
        assert_eq!(p.groups.len(), 1);
        assert_eq!(p.group_path("a"), vec![inner.clone()]);
        assert!(p
            .nodes
            .iter()
            .find(|n| n.id == "c")
            .unwrap()
            .group
            .is_none());
        p.ungroup_nodes(&ids(&["a", "b"])).unwrap();
        assert!(p.groups.is_empty());
        assert!(p.ungroup_nodes(&ids(&["a"])).is_err());
    }

    #[test]
    fn grouping_inside_a_group_creates_a_subgroup() {
        let mut p = scene();
        let outer = p.group_nodes(&ids(&["a", "b", "c"]), None).unwrap();
        let inner = p.group_nodes(&ids(&["a", "b"]), None).unwrap();
        assert_eq!(
            p.group(&inner).unwrap().parent_id.as_deref(),
            Some(outer.as_str())
        );
        assert_eq!(p.group_members("a"), ids(&["a", "b", "c"]));
    }

    #[test]
    fn grouped_nodes_become_contiguous_at_the_topmost_member() {
        let mut p = scene();
        p.group_nodes(&ids(&["a", "d"]), None).unwrap();
        assert_eq!(order(&p), ["b", "c", "a", "d", "e"]);
    }

    #[test]
    fn deleting_members_prunes_empty_groups_and_validation_rejects_bad_links() {
        let mut p = scene();
        p.group_nodes(&ids(&["a", "b"]), None).unwrap();
        p.remove_node_with_references("a").unwrap();
        assert_eq!(p.groups.len(), 1);
        p.remove_node_with_references("b").unwrap();
        assert!(p.groups.is_empty());

        let mut bad = scene();
        bad.nodes[0].group = Some("ghost".into());
        assert!(bad.validate().is_err());
        let mut cyclic = scene();
        cyclic.groups = vec![
            Group {
                id: "g1".into(),
                name: String::new(),
                parent_id: Some("g2".into()),
                rig: None,
            },
            Group {
                id: "g2".into(),
                name: String::new(),
                parent_id: Some("g1".into()),
                rig: None,
            },
        ];
        assert!(cyclic.validate().is_err());
        let mut clash = scene();
        clash.groups = vec![Group {
            id: "a".into(),
            name: String::new(),
            parent_id: None,
            rig: None,
        }];
        assert!(
            clash.validate().is_err(),
            "group IDs share the node namespace"
        );
    }

    #[test]
    fn copies_get_their_own_group_structure() {
        let mut p = scene();
        let outer = p.group_nodes(&ids(&["a", "b"]), Some("Kopf")).unwrap();
        let mut copies: Vec<Node> = p
            .nodes
            .iter()
            .filter(|n| n.group.is_some())
            .cloned()
            .collect();
        let source = p.clone();
        p.adopt_group_structure(&mut copies, &source);
        for (i, copy) in copies.iter_mut().enumerate() {
            copy.id = format!("copy{i}");
        }
        assert_eq!(copies[0].group, copies[1].group);
        assert_ne!(copies[0].group.as_deref(), Some(outer.as_str()));
        p.nodes.extend(copies);
        p.validate().unwrap();
        assert_eq!(p.groups.len(), 2);
        assert_eq!(p.groups[1].name, "Kopf");
        assert_eq!(p.group_members("copy0"), ids(&["copy0", "copy1"]));
    }
}
