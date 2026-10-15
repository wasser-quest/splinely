//! Bounded, all-or-nothing vector import. Unsupported appearance is rejected, never silently flattened.
use crate::*;
use std::{collections::HashSet, fmt::Write};

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

fn valid_import_id(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(),Some(c) if c.is_ascii_alphabetic() || c=='_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

impl Project {
    /// Import geometry as editable, animatable paths. No XML/JS is inserted into the exported DOM.
    /// Currently supports solid fills, ordinary butt/miter strokes, shapes and group transforms.
    pub fn from_svg(source: &str) -> Result<Self, DocumentError> {
        if source.len() > 4 * 1024 * 1024 {
            return Err(DocumentError("SVG import limit is 4 MiB".into()));
        }
        let xml = roxmltree::Document::parse_with_options(
            source,
            roxmltree::ParsingOptions {
                allow_dtd: false,
                nodes_limit: 30_000,
            },
        )
        .map_err(|e| DocumentError(format!("SVG XML: {e}")))?;
        let root = xml.root_element();
        if root.tag_name().name() != "svg"
            || root.tag_name().namespace() != Some("http://www.w3.org/2000/svg")
        {
            return Err(DocumentError(
                "SVG root must be <svg> in the SVG namespace".into(),
            ));
        }
        let mut ids = HashSet::new();
        let gradient_ids: Vec<&str> = xml
            .descendants()
            .filter(|n| matches!(n.tag_name().name(), "linearGradient" | "radialGradient"))
            .filter_map(|n| n.attribute("id"))
            .collect();
        let mut count = 0;
        for el in xml.descendants().filter(|n| n.is_element()) {
            if el.ancestors().count() > 128 {
                return Err(DocumentError("SVG depth limit is 128".into()));
            }
            count += 1;
            if count > 10_000 {
                return Err(DocumentError("SVG element limit is 10000".into()));
            }
            if let Some(id) = el.attribute("id") {
                if !ids.insert(id) {
                    return Err(DocumentError(format!("Duplicate SVG id: {id}")));
                }
            }
            // Metadata is retained as data, never interpreted as geometry or executed.
            if el.ancestors().any(|n| {
                n.is_element() && matches!(n.tag_name().name(), "metadata" | "title" | "desc")
            }) {
                continue;
            }
            if el.ancestors().any(|n| {
                n.is_element()
                    && n.tag_name().name() == "namedview"
                    && n.tag_name().namespace()
                        == Some("http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd")
            }) {
                continue;
            }
            if el.tag_name().namespace() != Some("http://www.w3.org/2000/svg") {
                return Err(DocumentError(
                    "Foreign SVG element namespace unsupported".into(),
                ));
            }
            let tag = el.tag_name().name();
            if el.ancestors().any(|n| n.has_tag_name("defs"))
                && matches!(tag, "linearGradient" | "radialGradient" | "stop")
            {
                continue;
            }
            if !matches!(
                tag,
                "svg"
                    | "g"
                    | "path"
                    | "rect"
                    | "circle"
                    | "ellipse"
                    | "line"
                    | "polyline"
                    | "polygon"
                    | "title"
                    | "desc"
                    | "metadata"
                    | "defs"
                    | "style"
            ) {
                return Err(DocumentError(format!(
                    "Unsupported SVG element <{tag}>; import cancelled (no partial document)"
                )));
            }
            if tag == "style" {
                return Err(DocumentError(
                    "SVG stylesheets unsupported; use inline solid presentation".into(),
                ));
            }
            for attr in el.attributes() {
                if attr.name() == "style" {
                    for declaration in attr.value().split(';').filter(|s| !s.trim().is_empty()) {
                        let (name, value) = declaration
                            .split_once(':')
                            .ok_or_else(|| DocumentError("Malformed inline SVG CSS".into()))?;
                        if (name.trim() == "marker" && value.trim() == "none")
                            || (name.trim() == "overflow"
                                && value.trim() == "visible"
                                && tag != "svg")
                        {
                            continue;
                        }
                        if !matches!(
                            name.trim(),
                            "fill"
                                | "stroke"
                                | "fill-opacity"
                                | "stroke-opacity"
                                | "opacity"
                                | "stroke-width"
                                | "fill-rule"
                                | "stroke-linecap"
                                | "stroke-linejoin"
                                | "stroke-miterlimit"
                                | "stroke-dasharray"
                                | "stroke-dashoffset"
                                | "display"
                                | "visibility"
                                | "color"
                        ) {
                            return Err(DocumentError(format!(
                                "Unsupported inline SVG CSS property {}",
                                name.trim()
                            )));
                        }
                    }
                }
                if attr.name().starts_with("on") || matches!(attr.name(), "href" | "src") {
                    return Err(DocumentError(format!(
                        "Unsafe SVG attribute {}",
                        attr.name()
                    )));
                }
                // Only local references to gradients defined in this file are allowed.
                let mut value = attr.value().to_owned();
                if matches!(attr.name(), "fill" | "style") {
                    for id in &gradient_ids {
                        value = value.replace(&format!("url(#{id})"), "");
                    }
                }
                if value.to_lowercase().contains("url(")
                    || attr.value().to_lowercase().contains("@import")
                    || attr.value().contains('\\')
                {
                    return Err(DocumentError(
                        "SVG URL resources/gradients are not supported".into(),
                    ));
                }
                if matches!(
                    attr.name(),
                    "vector-effect"
                        | "stroke-alignment"
                        | "clip-path"
                        | "mask"
                        | "filter"
                        | "mix-blend-mode"
                        | "isolation"
                        | "marker-start"
                        | "marker-mid"
                        | "marker-end"
                ) {
                    return Err(DocumentError(format!(
                        "Unsupported SVG attribute {}",
                        attr.name()
                    )));
                }
            }
            if tag == "style"
                && el.text().is_some_and(|s| {
                    s.to_lowercase().contains("url(")
                        || s.to_lowercase().contains("@import")
                        || s.contains('\\')
                })
            {
                return Err(DocumentError("SVG CSS resources are not supported".into()));
            }
            if let Some(d) = el.attribute("d") {
                validate_path(d)?;
            }
        }
        // XML preflight excludes every image/href: usvg cannot fetch a file or network image here.
        let options = usvg::Options {
            image_href_resolver: usvg::ImageHrefResolver {
                resolve_data: Box::new(|_, _, _| None),
                resolve_string: Box::new(|_, _| None),
            },
            ..Default::default()
        };
        let tree = usvg::Tree::from_xmltree(&xml, &options)
            .map_err(|e| DocumentError(format!("SVG: {e}")))?;
        let mut project = Self {
            width: tree.size().width() as f64,
            height: tree.size().height() as f64,
            raw_svg: source.into(),
            ..Self::default()
        };
        project.name = root
            .children()
            .find(|n| n.has_tag_name("title"))
            .and_then(|n| n.text())
            .unwrap_or("Imported SVG")
            .into();
        fn paints(group: &usvg::Group) -> usize {
            group
                .children()
                .iter()
                .map(|n| match n {
                    usvg::Node::Group(g) => paints(g),
                    usvg::Node::Path(p) => usize::from(p.is_visible()),
                    _ => 2,
                })
                .sum()
        }
        /// Approximate an SVG gradient fill: stop colours/offsets and the
        /// direction are kept; exact end points are mapped to the bounding box.
        fn gradient(paint: &usvg::Paint, opacity: f32, path: &usvg::Path) -> Option<Gradient> {
            let (radial, base, (dx, dy)): (bool, &usvg::BaseGradient, (f32, f32)) = match paint {
                usvg::Paint::LinearGradient(g) => (false, g, (g.x2() - g.x1(), g.y2() - g.y1())),
                usvg::Paint::RadialGradient(g) => (true, g, (1., 0.)),
                _ => return None,
            };
            // Direction in scene space: gradient transform, then the path's.
            let t = base.transform();
            let a = path.abs_transform();
            let (x, y) = (t.sx * dx + t.kx * dy, t.ky * dx + t.sy * dy);
            let (x, y) = (a.sx * x + a.kx * y, a.ky * x + a.sy * y);
            let mut stops: Vec<GradientStop> = base
                .stops()
                .iter()
                .map(|stop| {
                    let c = stop.color();
                    let mut color = Color::rgb(c.red, c.green, c.blue);
                    color.a = (stop.opacity().get() * opacity) as f64;
                    GradientStop {
                        offset: stop.offset().get() as f64,
                        color,
                    }
                })
                .collect();
            if stops.len() == 1 {
                stops.push(stops[0]);
                stops[1].offset = 1.;
            }
            if !(2..=16).contains(&stops.len()) {
                return None;
            }
            Some(Gradient {
                radial,
                angle: ((y as f64).atan2(x as f64).to_degrees() * 1000.).round() / 1000. + 0.,
                stops,
            })
        }
        /// A `<g id="…">` from the source becomes a Splinely group.
        #[allow(clippy::too_many_arguments)]
        fn visit(
            group: &usvg::Group,
            out: &mut Vec<Node>,
            groups: &mut Vec<Group>,
            ids: &mut HashSet<String>,
            parent_opacity: f32,
            parent_group: Option<String>,
        ) -> Result<(), DocumentError> {
            if (group.opacity().get() != 1. && paints(group) > 1)
                || group.clip_path().is_some()
                || group.mask().is_some()
                || !group.filters().is_empty()
                || group.blend_mode() != usvg::BlendMode::Normal
            {
                return Err(DocumentError("SVG group opacity, clipping, masks, filters or blending unsupported; import cancelled".into()));
            }
            for child in group.children() {
                match child {
                    usvg::Node::Group(g) => {
                        let mut inner = parent_group.clone();
                        if !g.id().is_empty()
                            && valid_import_id(g.id())
                            && g.id() != "splinely-project"
                        {
                            let mut id = g.id().to_owned();
                            while !ids.insert(id.clone()) {
                                id.push('_');
                            }
                            groups.push(Group {
                                id: id.clone(),
                                name: g.id().to_owned(),
                                parent_id: parent_group.clone(),
                            });
                            inner = Some(id);
                        }
                        visit(
                            g,
                            out,
                            groups,
                            ids,
                            parent_opacity * group.opacity().get(),
                            inner,
                        )?
                    }
                    usvg::Node::Path(path) => {
                        if !path.is_visible() {
                            continue;
                        }
                        let transformed = path
                            .data()
                            .clone()
                            .transform(path.abs_transform())
                            .ok_or_else(|| DocumentError("Invalid SVG transform".into()))?;
                        let bounds = transformed
                            .compute_tight_bounds()
                            .ok_or_else(|| DocumentError("SVG path bounds overflow".into()))?;
                        let mut d = String::new();
                        for segment in transformed.segments() {
                            use usvg::tiny_skia_path::PathSegment::*;
                            match segment {
                                MoveTo(p) => write!(d, "M{} {} ", p.x, p.y),
                                LineTo(p) => write!(d, "L{} {} ", p.x, p.y),
                                QuadTo(a, b) => write!(d, "Q{} {} {} {} ", a.x, a.y, b.x, b.y),
                                CubicTo(a, b, c) => {
                                    write!(d, "C{} {} {} {} {} {} ", a.x, a.y, b.x, b.y, c.x, c.y)
                                }
                                Close => write!(d, "Z "),
                            }
                            .unwrap();
                        }
                        let evenodd = path
                            .fill()
                            .is_some_and(|f| f.rule() == usvg::FillRule::EvenOdd);
                        let mut id = if path.id().is_empty()
                            || path.id() == "splinely-project"
                            || !valid_import_id(path.id())
                        {
                            format!("svg-path-{}", out.len() + 1)
                        } else {
                            path.id().into()
                        };
                        while !ids.insert(id.clone()) {
                            id.push('_');
                        }
                        let mut node = Node::new(
                            id,
                            NodeKind::Path {
                                d,
                                bounds: [
                                    bounds.x() as f64,
                                    bounds.y() as f64,
                                    bounds.width() as f64,
                                    bounds.height() as f64,
                                ],
                                evenodd,
                            },
                        );
                        let paint = |p: &usvg::Paint,
                                     opacity: f32|
                         -> Result<Color, DocumentError> {
                            match p {
                                usvg::Paint::Color(c) => {
                                    let mut c = Color::rgb(c.red, c.green, c.blue);
                                    c.a = opacity as f64;
                                    Ok(c)
                                }
                                _ => Err(DocumentError(
                                    "SVG gradients/patterns unsupported; import cancelled".into(),
                                )),
                            }
                        };
                        node.style.opacity = (parent_opacity * group.opacity().get()) as f64;
                        node.group = parent_group.clone();
                        node.style.fill = match path.fill() {
                            Some(f) => match gradient(f.paint(), f.opacity().get(), path) {
                                Some(g) => {
                                    let first = g.stops[0].color;
                                    node.gradient = Some(g);
                                    first
                                }
                                None => paint(f.paint(), f.opacity().get())?,
                            },
                            None => Color {
                                r: 0.,
                                g: 0.,
                                b: 0.,
                                a: 0.,
                            },
                        };
                        if let Some(s) = path.stroke() {
                            if s.linecap() != usvg::LineCap::Butt
                                || s.linejoin() != usvg::LineJoin::Miter
                                || s.dasharray().is_some()
                                || s.miterlimit().get() != 4.
                            {
                                return Err(DocumentError("SVG non-default stroke caps/joins/dashes unsupported; import cancelled".into()));
                            }
                            let t = path.abs_transform();
                            let sx = (t.sx * t.sx + t.ky * t.ky).sqrt();
                            let sy = (t.kx * t.kx + t.sy * t.sy).sqrt();
                            if sx == 0.
                                || sy == 0.
                                || (sx - sy).abs() > 1e-5 * sx.max(sy)
                                || (t.sx * t.kx + t.ky * t.sy).abs() > 1e-5 * sx * sy
                            {
                                return Err(DocumentError(
                                    "SVG nonuniform transformed stroke unsupported".into(),
                                ));
                            }
                            node.style.stroke = Some(paint(s.paint(), s.opacity().get())?);
                            node.style.stroke_width = (s.width().get() * sx) as f64;
                        }
                        if path.paint_order() != usvg::PaintOrder::FillAndStroke {
                            return Err(DocumentError(
                                "SVG stroke-first paint order unsupported".into(),
                            ));
                        }
                        out.push(node);
                    }
                    _ => {
                        return Err(DocumentError(
                            "SVG images/text are not supported for vector import".into(),
                        ))
                    }
                }
            }
            Ok(())
        }
        visit(
            tree.root(),
            &mut project.nodes,
            &mut project.groups,
            &mut HashSet::new(),
            1.,
            None,
        )?;
        project.prune_empty_groups();
        if project.nodes.is_empty() {
            return Err(DocumentError(
                "SVG contains no supported visible geometry".into(),
            ));
        }
        project.validate()?;
        Ok(project)
    }
}
