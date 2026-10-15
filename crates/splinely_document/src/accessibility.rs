//! Screen-reader semantics for the web player and exported SVG.
//!
//! Data: `Project::name` (accessible title), `Project::description`,
//! `Node::a11y` and `State::announce`. Everything is optional and omitted when
//! empty; a project without any of it exports byte-identically to before.
//! The semantics are **browser-only**: the native runtime, the viewer and the
//! editor canvas validate and preserve the fields but expose nothing to
//! assistive technology.
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Longest node label / state announcement, in characters.
pub const A11Y_LABEL_MAX: usize = 200;
/// Longest project or node description, in characters.
pub const A11Y_DESCRIPTION_MAX: usize = 1000;

/// Events that make a targeted node interactive (pointer, keyboard, focus, drag).
pub const INTERACTIVE_EVENTS: &[&str] = &[
    "click",
    "dblclick",
    "pointerenter",
    "pointerleave",
    "pointerdown",
    "pointermove",
    "pointerup",
    "pointercancel",
    "wheel",
    "keydown",
    "keyup",
    "focus",
    "blur",
    "dragstart",
    "drag",
    "dragend",
];

/// Allowlisted ARIA roles for a node. Anything else is rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum A11yRole {
    Button,
    Link,
    Img,
    Group,
    Heading,
}
impl A11yRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::Link => "link",
            Self::Img => "img",
            Self::Group => "group",
            Self::Heading => "heading",
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "button" => Self::Button,
            "link" => Self::Link,
            "img" => Self::Img,
            "group" => Self::Group,
            "heading" => Self::Heading,
            _ => return None,
        })
    }
    pub const ALL: [&'static str; 5] = ["button", "link", "img", "group", "heading"];
}

/// Optional per-node accessibility data.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NodeA11y {
    /// Accessible name; default for interactive nodes is the node name or ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Longer description (`aria-description`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Role override (interactive nodes default to `button`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<A11yRole>,
    /// Decorative: hidden from assistive technology (`aria-hidden="true"`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}
impl NodeA11y {
    pub fn is_empty(&self) -> bool {
        self.label.is_none() && self.description.is_none() && self.role.is_none() && !self.hidden
    }
}

fn text_ok(text: &str, max: usize, what: &str) -> Result<(), DocumentError> {
    let bad = text.trim().is_empty()
        || text.trim() != text
        || text.chars().count() > max
        || text
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{feff}' | '\u{fffe}' | '\u{ffff}'));
    if bad {
        Err(DocumentError(format!(
            "{what}: expected 1..{max} characters without control characters or surrounding spaces"
        )))
    } else {
        Ok(())
    }
}

impl Project {
    /// Node IDs targeted by a state-machine event that the pointer, keyboard or
    /// focus can trigger. These are focusable and keyboard-operable in the web player.
    pub fn interactive_node_ids(&self) -> BTreeSet<&str> {
        self.state_machines
            .iter()
            .flat_map(|m| &m.transitions)
            .filter(|t| t.after.is_none() && INTERACTIVE_EVENTS.contains(&t.event.name.as_str()))
            .filter_map(|t| t.event.target.as_deref())
            .filter(|id| self.nodes.iter().any(|n| n.id == *id))
            .collect()
    }
    /// Does any state announce text to screen readers (live region)?
    pub fn has_announcements(&self) -> bool {
        self.state_machines
            .iter()
            .flat_map(|m| &m.states)
            .any(|s| s.announce.is_some())
    }
    /// Does the project opt in to web-player accessibility semantics? Without
    /// a description, node `a11y` or state `announce`, nothing is added.
    pub fn a11y_enabled(&self) -> bool {
        !self.description.is_empty()
            || self.nodes.iter().any(|n| n.a11y.is_some())
            || self.has_announcements()
    }
    /// Interactive nodes (not hidden) lacking an explicit accessible label.
    pub fn unlabeled_interactive_nodes(&self) -> Vec<&str> {
        let interactive = self.interactive_node_ids();
        self.nodes
            .iter()
            .filter(|n| interactive.contains(n.id.as_str()))
            .filter(|n| {
                n.a11y
                    .as_ref()
                    .is_none_or(|a| a.label.is_none() && !a.hidden)
            })
            .map(|n| n.id.as_str())
            .collect()
    }
    /// Root role of an exported SVG: `group` when interactive nodes or a live
    /// region need to stay reachable inside it, otherwise `img`.
    pub(crate) fn a11y_root_role(&self, runtime: bool) -> &'static str {
        if runtime && (!self.interactive_node_ids().is_empty() || self.has_announcements()) {
            "group"
        } else {
            "img"
        }
    }

    pub(crate) fn validate_accessibility(&self) -> Result<(), DocumentError> {
        if !self.description.is_empty() {
            text_ok(
                &self.description,
                A11Y_DESCRIPTION_MAX,
                "project description",
            )?;
        }
        let interactive = self.interactive_node_ids();
        for node in &self.nodes {
            let Some(a) = &node.a11y else { continue };
            let ctx = format!("node {}: a11y", node.id);
            if let Some(label) = &a.label {
                text_ok(label, A11Y_LABEL_MAX, &format!("{ctx}.label"))?;
            }
            if let Some(description) = &a.description {
                text_ok(
                    description,
                    A11Y_DESCRIPTION_MAX,
                    &format!("{ctx}.description"),
                )?;
            }
            if a.hidden {
                if a.label.is_some() || a.description.is_some() || a.role.is_some() {
                    return Err(DocumentError(format!(
                        "{ctx}: hidden nodes cannot also have label, description or role"
                    )));
                }
                if interactive.contains(node.id.as_str()) {
                    return Err(DocumentError(format!(
                        "{ctx}: an interactive node cannot be hidden from assistive technology"
                    )));
                }
            }
        }
        for m in &self.state_machines {
            for s in &m.states {
                if let Some(text) = &s.announce {
                    text_ok(
                        text,
                        A11Y_LABEL_MAX,
                        &format!("machine {} state {}: announce", m.id, s.id),
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Attributes for one node element in the SVG export (leading space, XML-escaped).
    pub(crate) fn a11y_node_attributes(
        &self,
        node: &Node,
        escape: impl Fn(&str) -> Result<String, DocumentError>,
    ) -> Result<String, DocumentError> {
        let Some(a) = &node.a11y else {
            return Ok(String::new());
        };
        if a.hidden {
            return Ok(" aria-hidden=\"true\"".into());
        }
        let mut out = String::new();
        if let Some(role) = a.role {
            out.push_str(&format!(" role=\"{}\"", role.as_str()));
        }
        if let Some(label) = &a.label {
            out.push_str(&format!(" aria-label=\"{}\"", escape(label)?));
        }
        if let Some(description) = &a.description {
            out.push_str(&format!(" aria-description=\"{}\"", escape(description)?));
        }
        Ok(out)
    }

    /// Export ID not used by any project element (shared ID namespace).
    pub(crate) fn export_id(&self, base: &str) -> String {
        let taken = |id: &str| {
            self.nodes.iter().any(|n| n.id == id)
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
        };
        let mut id = base.to_string();
        while taken(&id) {
            id.push('_');
        }
        id
    }
}
