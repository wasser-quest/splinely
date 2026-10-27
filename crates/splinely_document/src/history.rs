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
    /// Caller-reported byte sizes parallel to `undo`/`redo` (0 if unreported).
    undo_sizes: Vec<usize>,
    redo_sizes: Vec<usize>,
    limits: HistoryLimits,
    dropped: usize,
}

/// Caps on retained undo steps. When exceeded, the oldest steps are dropped
/// (never the newest one); `None` means unlimited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HistoryLimits {
    pub max_steps: Option<usize>,
    pub max_bytes: Option<usize>,
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
            self.undo_sizes.push(0);
            self.redo.clear();
            self.redo_labels.clear();
            self.redo_sizes.clear();
            *project = next;
            self.enforce_limits();
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
        self.redo_sizes
            .push(self.undo_sizes.pop().unwrap_or_default());
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
        self.undo_sizes
            .push(self.redo_sizes.pop().unwrap_or_default());
        true
    }
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.undo_labels.clear();
        self.redo_labels.clear();
        self.undo_sizes.clear();
        self.redo_sizes.clear();
    }
    /// Set retention caps; applies immediately to existing steps.
    pub fn set_limits(&mut self, limits: HistoryLimits) {
        self.limits = limits;
        self.enforce_limits();
    }
    pub fn limits(&self) -> HistoryLimits {
        self.limits
    }
    /// Record the approximate memory cost of the most recent undo step and
    /// enforce the byte cap.
    pub fn set_last_size(&mut self, bytes: usize) {
        if let Some(last) = self.undo_sizes.last_mut() {
            *last = bytes;
        }
        self.enforce_limits();
    }
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
    /// Sum of reported sizes of all retained (undo and redo) steps.
    pub fn bytes(&self) -> usize {
        self.undo_sizes.iter().chain(&self.redo_sizes).sum()
    }
    /// Number of old steps dropped by the limits since the last call.
    pub fn take_dropped(&mut self) -> usize {
        std::mem::take(&mut self.dropped)
    }
    /// Drop the oldest undo steps until the caps hold; the newest step stays.
    fn enforce_limits(&mut self) {
        let mut drop = 0;
        while self.undo.len() - drop > 1 && {
            let steps = self.undo.len() - drop;
            let bytes: usize = self.undo_sizes[drop..].iter().chain(&self.redo_sizes).sum();
            self.limits.max_steps.is_some_and(|m| steps > m.max(1))
                || self.limits.max_bytes.is_some_and(|m| bytes > m)
        } {
            drop += 1;
        }
        if drop > 0 {
            self.undo.drain(..drop);
            self.undo_labels.drain(..drop);
            self.undo_sizes.drain(..drop);
            self.dropped += drop;
        }
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
