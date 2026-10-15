//! Structured outcome of an SVG import: what became editable document content,
//! what was approximated and what was dropped. Nothing is dropped silently.
use crate::{DocumentError, Project};
use serde::Serialize;

/// Hard limits of the importer. Every limit failure names the limit.
pub mod limits {
    /// Maximum SVG source size.
    pub const SOURCE_BYTES: usize = 4 * 1024 * 1024;
    /// Maximum XML nodes (roxmltree `nodes_limit`).
    pub const XML_NODES: u32 = 30_000;
    /// Maximum element count of the source.
    pub const ELEMENTS: usize = 10_000;
    /// Maximum element nesting depth.
    pub const DEPTH: usize = 128;
    /// Maximum element count after `<use>` expansion (stops billion-laughs chains).
    pub const EXPANDED_ELEMENTS: u64 = 50_000;
    /// Maximum `<use>` hops plus nesting in one expansion path.
    pub const USE_DEPTH: usize = 256;
    /// Maximum gradient elements.
    pub const GRADIENTS: usize = 512;
    /// Maximum `<stop>` elements of one gradient and of the whole document.
    pub const STOPS_PER_GRADIENT: usize = 256;
    pub const STOPS_TOTAL: usize = 4096;
    /// Maximum length of an `href` inheritance chain between gradients.
    pub const GRADIENT_HREF_CHAIN: usize = 64;
    /// Maximum characters of one imported text segment, and segments per `<text>`.
    pub const TEXT_CHARS: usize = 65_536;
    pub const TEXT_SEGMENTS: usize = 256;
    /// `<image>` elements, decoded bytes of one image, and of all images together.
    pub const IMAGES: usize = 32;
    pub const IMAGE_BYTES: usize = 3 * 1024 * 1024;
    pub const IMAGES_TOTAL_BYTES: usize = 8 * 1024 * 1024;
    /// Largest pixel edge / pixel count accepted for a validated raster image.
    pub const IMAGE_EDGE: u32 = 16_384;
    pub const IMAGE_PIXELS: u64 = 100_000_000;
    /// Nested SVG images: size, count and nesting depth (nested images of a nested image are dropped).
    pub const NESTED_SVG_BYTES: usize = 512 * 1024;
    pub const NESTED_SVGS: usize = 8;
    pub const NESTED_DEPTH: usize = 1;
    /// Maximum nodes in the imported project.
    pub const NODES: usize = 20_000;
    /// Maximum diagnostics kept in a report (counts stay exact).
    pub const DIAGNOSTICS: usize = 5_000;
}

/// What happened to a piece of the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportStatus {
    /// Became editable document content as written.
    Imported,
    /// Became document content, but not identical to the source.
    Approximated,
    /// Not part of the document (reason in the message).
    Dropped,
    /// Made the import fail (only in the report of a failed import).
    Rejected,
}

impl ImportStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Imported => "imported",
            Self::Approximated => "approximated",
            Self::Dropped => "dropped",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDiagnostic {
    pub status: ImportStatus,
    /// Stable machine-readable code, e.g. `text-anchor`, `gradient-spread`.
    pub code: &'static str,
    /// Source element name (`text`, `linearGradient`, `image` …).
    pub element: String,
    /// Source `id` or the resulting document ID, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Source attribute or CSS property concerned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribute: Option<String>,
    /// English explanation (the editor localises by `code`).
    pub message: String,
    /// How many identical occurrences this entry stands for (>= 1).
    pub count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub diagnostics: Vec<ImportDiagnostic>,
    /// Exact totals by status (sums of `count`), even when `truncated`.
    pub imported: usize,
    pub approximated: usize,
    pub dropped: usize,
    pub rejected: usize,
    /// Some `imported` entries were left out to bound the report size.
    pub truncated: bool,
}

impl ImportReport {
    pub fn push(
        &mut self,
        status: ImportStatus,
        code: &'static str,
        element: &str,
        id: Option<&str>,
        attribute: Option<&str>,
        message: impl Into<String>,
    ) {
        match status {
            ImportStatus::Imported => self.imported += 1,
            ImportStatus::Approximated => self.approximated += 1,
            ImportStatus::Dropped => self.dropped += 1,
            ImportStatus::Rejected => self.rejected += 1,
        }
        // Repeated approximations/drops of the same kind collapse into one counted entry.
        if status != ImportStatus::Imported {
            if let Some(existing) = self.diagnostics.iter_mut().find(|d| {
                d.status == status
                    && d.code == code
                    && d.element == element
                    && d.attribute.as_deref() == attribute
                    && d.id.as_deref() == id
            }) {
                existing.count += 1;
                return;
            }
        } else if self.diagnostics.len() >= limits::DIAGNOSTICS {
            // Lossy entries are never discarded; only `imported` ones are.
            self.truncated = true;
            return;
        }
        self.diagnostics.push(ImportDiagnostic {
            status,
            code,
            element: element.into(),
            id: id.map(Into::into),
            attribute: attribute.map(Into::into),
            message: message.into(),
            count: 1,
        });
    }
    /// True when anything was approximated, dropped or rejected.
    pub fn has_losses(&self) -> bool {
        self.approximated + self.dropped + self.rejected > 0
    }
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
    /// Lossy entries only (approximated, dropped, rejected).
    pub fn losses(&self) -> impl Iterator<Item = &ImportDiagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.status != ImportStatus::Imported)
    }
}

/// A successful import.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportOutcome {
    pub project: Project,
    pub report: ImportReport,
}

/// A refused import: the error plus the diagnostics that led to it. No partial document exists.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportFailure {
    pub error: DocumentError,
    pub report: ImportReport,
}

impl std::fmt::Display for ImportFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for ImportFailure {}
