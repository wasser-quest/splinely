//! Conflict detection between the preserved SVG source text (`Project::raw_svg`) and the
//! visual document. The visual editor never rewrites `raw_svg`, and text edits of `raw_svg`
//! never reach the nodes, so after an SVG import the two can silently drift apart.
//! `Project::svg_import` stores fingerprints taken at import; comparing them shows which side
//! changed. The hashes are change detectors, not security measures.
use crate::{Project, SvgImportInfo};

/// Relation between the stored source text and the visual document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceSync {
    /// The project was not imported from SVG (or predates the baseline): nothing to compare.
    NoBaseline,
    /// Source text and visual document are unchanged since import.
    InSync,
    /// The visual document was edited; the stored source text is stale.
    VisualEdited,
    /// The source text was edited; the visual document does not reflect it.
    SourceEdited,
    /// Both changed: neither is derived from the other any more.
    Both,
}

impl SourceSync {
    pub fn is_conflict(self) -> bool {
        matches!(self, Self::VisualEdited | Self::SourceEdited | Self::Both)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoBaseline => "noBaseline",
            Self::InSync => "inSync",
            Self::VisualEdited => "visualEdited",
            Self::SourceEdited => "sourceEdited",
            Self::Both => "both",
        }
    }
}

fn fnv1a64(bytes: &[u8]) -> String {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("fnv1a64:{h:016x}")
}

impl Project {
    fn visual_hash(&self) -> String {
        let canonical = serde_json::to_vec(&(self.width, self.height, &self.groups, &self.nodes))
            .unwrap_or_default();
        fnv1a64(&canonical)
    }

    /// Fingerprints of the current source text and visual document.
    pub fn current_import_info(&self) -> SvgImportInfo {
        SvgImportInfo {
            source_hash: fnv1a64(self.raw_svg.as_bytes()),
            document_hash: self.visual_hash(),
        }
    }

    /// Compare the stored baseline with the current source text and visual document.
    pub fn source_sync(&self) -> SourceSync {
        let Some(base) = &self.svg_import else {
            return SourceSync::NoBaseline;
        };
        let now = self.current_import_info();
        match (
            now.source_hash != base.source_hash,
            now.document_hash != base.document_hash,
        ) {
            (false, false) => SourceSync::InSync,
            (false, true) => SourceSync::VisualEdited,
            (true, false) => SourceSync::SourceEdited,
            (true, true) => SourceSync::Both,
        }
    }

    /// Accept the current state of both sides as the new baseline (resolves a conflict
    /// by declaring the visual document authoritative). No-op without a baseline.
    pub fn accept_svg_source_state(&mut self) {
        if self.svg_import.is_some() {
            self.svg_import = Some(self.current_import_info());
        }
    }
}
