//! Bounded, all-or-nothing vector import. Unsupported appearance is rejected, never silently flattened.
use crate::svg_import_report::{ImportFailure, ImportOutcome, ImportStatus};
use crate::*;

pub(crate) fn validate_path(d: &str) -> Result<(), DocumentError> {
    use svgtypes::PathSegment::*;
    if d.is_empty() || d.len() > 1_048_576 {
        return Err(DocumentError(
            "Path data must be nonempty and <= 1 MiB".into(),
        ));
    }
    let mut count = 0;
    for segment in svgtypes::PathParser::from(d) {
        let values = match segment.map_err(|e| DocumentError(format!("Invalid SVG path: {e}")))? {
            MoveTo { x, y, .. } | LineTo { x, y, .. } | SmoothQuadratic { x, y, .. } => vec![x, y],
            HorizontalLineTo { x, .. } => vec![x],
            VerticalLineTo { y, .. } => vec![y],
            CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
                ..
            } => vec![x1, y1, x2, y2, x, y],
            SmoothCurveTo { x2, y2, x, y, .. } => vec![x2, y2, x, y],
            Quadratic { x1, y1, x, y, .. } => vec![x1, y1, x, y],
            EllipticalArc {
                rx,
                ry,
                x_axis_rotation,
                x,
                y,
                ..
            } => vec![rx, ry, x_axis_rotation, x, y],
            ClosePath { .. } => vec![],
        };
        if values.iter().any(|v| !v.is_finite() || v.abs() > 1e7) {
            return Err(DocumentError(
                "SVG path coordinate limit is +/- 10000000".into(),
            ));
        }
        count += 1;
        if count > 50_000 {
            return Err(DocumentError("SVG path segment limit is 50000".into()));
        }
    }
    if count == 0 {
        return Err(DocumentError("SVG path is empty".into()));
    }
    Ok(())
}

/// Authoritative tight local bounds used by validation and vector editing.
/// Arc conversion follows the same SVG geometry pipeline as native rendering.
pub fn path_bounds(d: &str) -> Result<[f64; 4], DocumentError> {
    validate_path(d)?;
    use svgtypes::SimplePathSegment::*;
    let mut builder = usvg::tiny_skia_path::PathBuilder::new();
    for segment in svgtypes::SimplifyingPathParser::from(d) {
        let segment = segment.map_err(|e| DocumentError(format!("Invalid SVG path: {e}")))?;
        let values = match segment {
            MoveTo { x, y } | LineTo { x, y } => vec![x, y],
            CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => vec![x1, y1, x2, y2, x, y],
            Quadratic { x1, y1, x, y } => vec![x1, y1, x, y],
            ClosePath => vec![],
        };
        if values.iter().any(|v| !v.is_finite() || v.abs() > 1e7) {
            return Err(DocumentError(
                "Absolute SVG path coordinate limit is +/- 10000000".into(),
            ));
        }
        match segment {
            MoveTo { x, y } => builder.move_to(x as f32, y as f32),
            LineTo { x, y } => builder.line_to(x as f32, y as f32),
            CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => builder.cubic_to(
                x1 as f32, y1 as f32, x2 as f32, y2 as f32, x as f32, y as f32,
            ),
            Quadratic { x1, y1, x, y } => builder.quad_to(x1 as f32, y1 as f32, x as f32, y as f32),
            ClosePath => builder.close(),
        }
    }
    let path = builder
        .finish()
        .ok_or_else(|| DocumentError("Path needs drawable geometry".into()))?;
    let bounds = path
        .compute_tight_bounds()
        .ok_or_else(|| DocumentError("Path bounds overflow".into()))?;
    Ok([
        bounds.x() as f64,
        bounds.y() as f64,
        bounds.width() as f64,
        bounds.height() as f64,
    ])
}

pub(crate) fn valid_import_id(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(),Some(c) if c.is_ascii_alphabetic() || c=='_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

impl Project {
    /// Import geometry as editable, animatable paths; see [`Project::from_svg_report`].
    pub fn from_svg(source: &str) -> Result<Self, DocumentError> {
        Self::from_svg_report(source)
            .map(|o| o.project)
            .map_err(|f| f.error)
    }

    /// Import an SVG as editable nodes and return a structured report of what was imported,
    /// approximated and dropped. No XML/JS is inserted into the exported DOM; nothing is ever
    /// fetched. Unsupported appearance (clipping, masks, filters, dashes, patterns, external or
    /// malformed images …) cancels the whole import, leaving no partial document; the failure
    /// carries the report with the rejecting diagnostic.
    pub fn from_svg_report(source: &str) -> Result<ImportOutcome, ImportFailure> {
        let mut ctx = svg_convert::Ctx::new();
        let converted = svg_convert::convert_document(
            source,
            &mut ctx,
            0,
            &|_, _| usvg::tiny_skia_path::Transform::identity(),
            "",
            None,
            1.,
        );
        let info = match converted {
            Ok(info) => info,
            Err(error) => {
                return Err(ImportFailure {
                    error,
                    report: ctx.report,
                })
            }
        };
        let mut project = Self {
            width: info.width,
            height: info.height,
            raw_svg: source.into(),
            nodes: std::mem::take(&mut ctx.nodes),
            groups: std::mem::take(&mut ctx.groups),
            name: info.title.unwrap_or_else(|| "Imported SVG".into()),
            ..Self::default()
        };
        project.prune_empty_groups();
        let checked = if project.nodes.is_empty() {
            Err(DocumentError(
                "SVG contains no supported visible geometry".into(),
            ))
        } else {
            project.validate()
        };
        if let Err(error) = checked {
            ctx.report.push(
                ImportStatus::Rejected,
                "project-invalid",
                "svg",
                None,
                None,
                error.0.clone(),
            );
            return Err(ImportFailure {
                error,
                report: ctx.report,
            });
        }
        project.svg_import = Some(project.current_import_info());
        Ok(ImportOutcome {
            project,
            report: ctx.report,
        })
    }
}
