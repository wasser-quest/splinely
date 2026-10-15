use crate::*;

#[derive(Clone, Debug)]
pub enum Command {
    AddNode(Node),
    RemoveNode { id: String },
    ReplaceNode(Node),
    ReplaceProject(Project),
}
/// Full-document snapshots make transactions atomic and preserve source/assets.
/// The host must route edits through this history; direct edits invalidate it.
#[derive(Clone, Debug, Default)]
pub struct History {
    undo: Vec<(Project, Project)>,
    redo: Vec<(Project, Project)>,
    /// Human-readable names parallel to `undo`/`redo` (empty if unnamed).
    undo_labels: Vec<String>,
    redo_labels: Vec<String>,
}
impl History {
    pub fn execute(
        &mut self,
        project: &mut Project,
        command: Command,
    ) -> Result<(), DocumentError> {
        self.execute_batch(project, [command])
    }
    /// One undo step; intermediate states need not validate, final state must.
    pub fn execute_batch(
        &mut self,
        project: &mut Project,
        commands: impl IntoIterator<Item = Command>,
    ) -> Result<(), DocumentError> {
        let mut next = project.clone();
        for command in commands {
            match command {
                Command::AddNode(node) => next.nodes.push(node),
                Command::RemoveNode { id } => {
                    let index = next
                        .nodes
                        .iter()
                        .position(|n| n.id == id)
                        .ok_or_else(|| DocumentError(format!("unknown node {id}")))?;
                    next.nodes.remove(index);
                }
                Command::ReplaceNode(node) => {
                    let old = next
                        .nodes
                        .iter_mut()
                        .find(|n| n.id == node.id)
                        .ok_or_else(|| DocumentError(format!("unknown node {}", node.id)))?;
                    *old = node;
                }
                Command::ReplaceProject(replacement) => next = replacement,
            }
        }
        next.validate()?;
        if next != *project {
            self.undo.push((project.clone(), next.clone()));
            self.undo_labels.push(String::new());
            self.redo.clear();
            self.redo_labels.clear();
            *project = next;
        }
        Ok(())
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo(&mut self, project: &mut Project) -> bool {
        let Some((before, after)) = self.undo.pop() else {
            return false;
        };
        *project = before.clone();
        self.redo.push((before, after));
        self.redo_labels
            .push(self.undo_labels.pop().unwrap_or_default());
        true
    }
    pub fn redo(&mut self, project: &mut Project) -> bool {
        let Some((before, after)) = self.redo.pop() else {
            return false;
        };
        *project = after.clone();
        self.undo.push((before, after));
        self.undo_labels
            .push(self.redo_labels.pop().unwrap_or_default());
        true
    }
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.undo_labels.clear();
        self.redo_labels.clear();
    }
    /// Name the most recent undo step.
    pub fn set_last_label(&mut self, label: impl Into<String>) {
        if let Some(last) = self.undo_labels.last_mut() {
            *last = label.into();
        }
    }
    /// Step names: done steps oldest first, then undone steps (next redo first).
    pub fn labels(&self) -> (Vec<&str>, Vec<&str>) {
        (
            self.undo_labels.iter().map(String::as_str).collect(),
            self.redo_labels.iter().rev().map(String::as_str).collect(),
        )
    }
}
