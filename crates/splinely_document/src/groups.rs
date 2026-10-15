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
            },
            Group {
                id: "g2".into(),
                name: String::new(),
                parent_id: Some("g1".into()),
            },
        ];
        assert!(cyclic.validate().is_err());
        let mut clash = scene();
        clash.groups = vec![Group {
            id: "a".into(),
            name: String::new(),
            parent_id: None,
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
