//! SVG → editable document conversion with a structured report (see `docs/svg-import.md`).
//!
//! Stages: (1) strict preflight of the XML (allow-list, resource and size limits, `<use>`
//! expansion bound), (2) `<text>`/`<image>` elements are swapped for placeholder rectangles so
//! that usvg supplies document order, transforms and inherited paint, (3) usvg resolves the tree,
//! (4) the tree is walked into nodes; placeholders become text nodes / image diagnostics / nested
//! vector groups. Unsupported appearance fails the whole import; lossy cases are reported.
use crate::svg_import::{valid_import_id, validate_path};
use crate::svg_import_gradient::{map_paint, Mapped, Notes};
use crate::svg_import_image::{parse_data_uri, DataImage, ImageFormat};
use crate::svg_import_report::{limits, ImportReport, ImportStatus};
use crate::svg_import_text::{self as text, TextPlan};
use crate::*;
use roxmltree::{Document, Node as XNode};
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::ops::Range;
use usvg::tiny_skia_path::Transform as Ts;

const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

/// Shared state of one import (the top-level document and nested SVG images).
pub(crate) struct Ctx {
    pub report: ImportReport,
    pub nodes: Vec<Node>,
    pub groups: Vec<Group>,
    pub ids: HashSet<String>,
    images: usize,
    image_budget: usize,
    nested: usize,
    stops: usize,
}

impl Ctx {
    pub fn new() -> Self {
        Self {
            report: ImportReport::default(),
            nodes: vec![],
            groups: vec![],
            ids: HashSet::new(),
            images: 0,
            image_budget: limits::IMAGES_TOTAL_BYTES,
            nested: 0,
            stops: 0,
        }
    }
    pub fn reject(
        &mut self,
        code: &'static str,
        element: &str,
        id: Option<&str>,
        attribute: Option<&str>,
        message: impl Into<String>,
    ) -> DocumentError {
        let message = message.into();
        self.report.push(
            ImportStatus::Rejected,
            code,
            element,
            id,
            attribute,
            message.clone(),
        );
        DocumentError(message)
    }
    fn unique(&mut self, wanted: String) -> String {
        let mut id = wanted;
        while !self.ids.insert(id.clone()) {
            id.push('_');
        }
        id
    }
}

pub(crate) struct DocInfo {
    pub width: f64,
    pub height: f64,
    pub title: Option<String>,
}

#[derive(Clone, Copy)]
struct Par {
    ax: f64,
    ay: f64,
    none: bool,
    slice: bool,
}

#[derive(Clone)]
enum PhKind {
    Text(Vec<text::Run>),
    Image {
        image: DataImage,
        x: f64,
        y: f64,
        w: Option<f64>,
        h: Option<f64>,
        par: Par,
    },
}

#[derive(Clone)]
struct Placeholder {
    kind: PhKind,
    src_id: Option<String>,
    in_defs: bool,
    seen: bool,
}

#[derive(Default)]
struct Plan {
    edits: Vec<(Range<usize>, String)>,
    phs: Vec<Placeholder>,
    ph_ids: HashMap<String, usize>,
    candidates: usize,
    has_use: bool,
}

const CSS_SHAPE: &[&str] = &[
    "fill",
    "stroke",
    "fill-opacity",
    "stroke-opacity",
    "opacity",
    "stroke-width",
    "fill-rule",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "display",
    "visibility",
    "color",
    "paint-order",
];
const CSS_TEXT: &[&str] = &[
    "font-size",
    "font-family",
    "font-weight",
    "font-style",
    "text-anchor",
    "letter-spacing",
    "word-spacing",
    "text-decoration",
    "dominant-baseline",
    "baseline-shift",
    "writing-mode",
    "direction",
];
const CSS_STOP: &[&str] = &["stop-color", "stop-opacity"];
/// Accepted in `style`, but without effect on the document: reported as dropped.
const CSS_IGNORED: &[&str] = &[
    "line-height",
    "text-align",
    "font-variant",
    "font-stretch",
    "font-size-adjust",
    "font-variation-settings",
    "font-feature-settings",
    "unicode-bidi",
    "white-space",
    "text-anchor-x",
];
const KNOWN_ATTRS: &[&str] = &[
    "id",
    "d",
    "x",
    "y",
    "width",
    "height",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "x1",
    "y1",
    "x2",
    "y2",
    "fx",
    "fy",
    "fr",
    "points",
    "transform",
    "viewBox",
    "preserveAspectRatio",
    "version",
    "style",
    "gradientUnits",
    "gradientTransform",
    "spreadMethod",
    "offset",
    "dx",
    "dy",
    "rotate",
    "textLength",
    "lengthAdjust",
    "overflow",
    "marker",
    "href",
];
const REJECTED_ATTRS: &[&str] = &[
    "vector-effect",
    "stroke-alignment",
    "clip-path",
    "mask",
    "filter",
    "mix-blend-mode",
    "isolation",
    "marker-start",
    "marker-mid",
    "marker-end",
];
const ALLOWED_TAGS: &[&str] = &[
    "svg",
    "g",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "title",
    "desc",
    "metadata",
    "defs",
    "use",
    "symbol",
    "text",
    "tspan",
    "image",
    "linearGradient",
    "radialGradient",
    "stop",
];

fn is_svg(n: &XNode) -> bool {
    n.is_element() && n.tag_name().namespace() == Some(SVG_NS)
}
fn svg_name<'a>(n: &XNode<'a, '_>) -> &'a str {
    if is_svg(n) {
        n.tag_name().name()
    } else {
        ""
    }
}
fn href<'a>(el: XNode<'a, '_>) -> Option<&'a str> {
    el.attribute("href")
        .or_else(|| el.attribute((XLINK_NS, "href")))
}

fn escape(v: &str) -> String {
    v.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

/// Drop `url(#gradient)` references to gradients of this file; what is left must be URL-free.
fn strip_gradient_refs(value: &str, gradients: &HashSet<&str>) -> String {
    let mut v = value.to_owned();
    for id in gradients {
        v = v.replace(&format!("url(#{id})"), "");
    }
    v
}

fn len_px(value: Option<&str>, font: f64) -> Option<f64> {
    text::first_len(value?, font).0
}

fn preflight(
    xml: &Document,
    source: &str,
    ctx: &mut Ctx,
    depth: usize,
) -> Result<Plan, DocumentError> {
    let mut plan = Plan::default();
    let gradients: Vec<XNode> = xml
        .descendants()
        .filter(|n| matches!(svg_name(n), "linearGradient" | "radialGradient"))
        .collect();
    if gradients.len() > limits::GRADIENTS {
        return Err(ctx.reject(
            "limit-gradients",
            "linearGradient",
            None,
            None,
            format!("SVG gradient limit is {}", limits::GRADIENTS),
        ));
    }
    let gradient_ids: HashSet<&str> = gradients.iter().filter_map(|n| n.attribute("id")).collect();
    let mut by_id: HashMap<&str, XNode> = HashMap::new();
    let mut count = 0;
    for el in xml.descendants().filter(|n| n.is_element()) {
        if el.ancestors().count() > limits::DEPTH {
            return Err(ctx.reject(
                "limit-depth",
                el.tag_name().name(),
                el.attribute("id"),
                None,
                format!("SVG depth limit is {}", limits::DEPTH),
            ));
        }
        count += 1;
        if count > limits::ELEMENTS {
            return Err(ctx.reject(
                "limit-elements",
                el.tag_name().name(),
                None,
                None,
                format!("SVG element limit is {}", limits::ELEMENTS),
            ));
        }
        if let Some(id) = el.attribute("id") {
            if by_id.insert(id, el).is_some() {
                return Err(ctx.reject(
                    "duplicate-id",
                    el.tag_name().name(),
                    Some(id),
                    Some("id"),
                    format!("Duplicate SVG id: {id}"),
                ));
            }
        }
    }
    let root = xml.root_element();
    let mut paint_candidates = 0usize;
    for el in xml.descendants().filter(|n| n.is_element()) {
        let name = el.tag_name().name();
        let id = el.attribute("id");
        let ancestors: Vec<XNode> = el.ancestors().skip(1).filter(|n| n.is_element()).collect();
        // Metadata is kept only as raw source text, never interpreted or executed.
        if ancestors
            .iter()
            .any(|n| matches!(svg_name(n), "metadata" | "title" | "desc"))
            || ancestors.iter().any(|n| !is_svg(n))
        {
            continue;
        }
        if !is_svg(&el) {
            let parent_ok = el
                .parent()
                .is_some_and(|p| matches!(svg_name(&p), "svg" | "defs" | "metadata"));
            if !parent_ok {
                return Err(ctx.reject(
                    "foreign-element",
                    name,
                    id,
                    None,
                    "Foreign SVG element namespace unsupported",
                ));
            }
            ctx.report.push(
                ImportStatus::Dropped,
                "foreign-element",
                name,
                id,
                None,
                format!(
                    "element from another namespace ({}) has no equivalent and was dropped",
                    el.tag_name().namespace().unwrap_or("none")
                ),
            );
            continue;
        }
        if !ALLOWED_TAGS.contains(&name) {
            let (code, msg) = if name == "style" {
                (
                    "stylesheet-unsupported",
                    "SVG stylesheets unsupported; use inline solid presentation".to_owned(),
                )
            } else {
                (
                    "element-unsupported",
                    format!(
                        "Unsupported SVG element <{name}>; import cancelled (no partial document)"
                    ),
                )
            };
            return Err(ctx.reject(code, name, id, None, msg));
        }
        match name {
            "title" if el.parent().is_some_and(|p| p == root) => {
                if depth == 0 {
                    ctx.report.push(
                        ImportStatus::Imported,
                        "title",
                        name,
                        id,
                        None,
                        "used as the project name",
                    );
                    continue;
                }
                ctx.report.push(
                    ImportStatus::Dropped,
                    "metadata",
                    name,
                    id,
                    None,
                    "title of a nested SVG image is not used",
                );
                continue;
            }
            "title" | "desc" | "metadata" => {
                ctx.report.push(
                    ImportStatus::Dropped,
                    "metadata",
                    name,
                    id,
                    None,
                    "descriptive metadata is kept only in the preserved source text",
                );
                continue;
            }
            _ => {}
        }
        // Attribute rules.
        for attr in el.attributes() {
            let aname = attr.name();
            match attr.namespace() {
                None => {}
                Some(XLINK_NS) if aname == "href" => {}
                Some(text::XML_NS) => continue,
                Some(ns) => {
                    let prefix = el.lookup_prefix(ns).unwrap_or("ns");
                    ctx.report.push(
                        ImportStatus::Dropped,
                        "foreign-attribute",
                        name,
                        None,
                        Some(&format!("{prefix}:{aname}")),
                        "attribute of another namespace (editor metadata) is not imported",
                    );
                    continue;
                }
            }
            let value = attr.value();
            if aname.starts_with("on") || aname == "src" {
                return Err(ctx.reject(
                    "attribute-unsafe",
                    name,
                    id,
                    Some(aname),
                    format!("Unsafe SVG attribute {aname}"),
                ));
            }
            if aname == "href" {
                let ok = match name {
                    "image" => true,
                    "use" | "linearGradient" | "radialGradient" => value.starts_with('#'),
                    _ => false,
                };
                if !ok {
                    let (code, msg) = if matches!(name, "use" | "linearGradient" | "radialGradient")
                    {
                        (
                            "reference-external",
                            format!("only same-document #references are allowed in <{name}> href (nothing is fetched)"),
                        )
                    } else {
                        ("attribute-unsafe", "Unsafe SVG attribute href".to_owned())
                    };
                    return Err(ctx.reject(code, name, id, Some("href"), msg));
                }
                continue;
            }
            if aname == "style" {
                for declaration in value.split(';').filter(|s| !s.trim().is_empty()) {
                    let (prop, val) = declaration.split_once(':').ok_or_else(|| {
                        ctx.reject(
                            "css-malformed",
                            name,
                            id,
                            Some("style"),
                            "Malformed inline SVG CSS",
                        )
                    })?;
                    let (prop, val) = (prop.trim(), val.trim());
                    if (prop == "marker" && val == "none")
                        || (prop == "overflow" && val == "visible" && name != "svg")
                    {
                        continue;
                    }
                    if CSS_SHAPE.contains(&prop)
                        || CSS_TEXT.contains(&prop)
                        || CSS_STOP.contains(&prop)
                    {
                        continue;
                    }
                    if CSS_IGNORED.contains(&prop)
                        || prop.starts_with("font-variant")
                        || prop.starts_with("-inkscape-")
                    {
                        ctx.report.push(
                            ImportStatus::Dropped,
                            "css-ignored",
                            name,
                            id,
                            Some(prop),
                            "CSS property has no equivalent in the document",
                        );
                        continue;
                    }
                    return Err(ctx.reject(
                        "css-unsupported",
                        name,
                        id,
                        Some(prop),
                        format!("Unsupported inline SVG CSS property {prop}"),
                    ));
                }
            }
            // Only same-document gradient references are allowed (never anything fetched).
            let mut residue = value.to_owned();
            if matches!(aname, "fill" | "stroke" | "style") {
                residue = strip_gradient_refs(&residue, &gradient_ids);
            }
            let lower = residue.to_lowercase();
            if lower.contains("url(") || lower.contains("@import") || value.contains('\\') {
                return Err(ctx.reject(
                    "url-unsupported",
                    name,
                    id,
                    Some(aname),
                    "SVG URL resources (patterns, clip paths, external files) and references to missing gradients are not supported",
                ));
            }
            if REJECTED_ATTRS.contains(&aname) {
                return Err(ctx.reject(
                    "attribute-unsupported",
                    name,
                    id,
                    Some(aname),
                    format!("Unsupported SVG attribute {aname}"),
                ));
            }
            if !(KNOWN_ATTRS.contains(&aname)
                || CSS_SHAPE.contains(&aname)
                || CSS_TEXT.contains(&aname)
                || CSS_STOP.contains(&aname))
                && !aname.starts_with("xmlns")
            {
                ctx.report.push(
                    ImportStatus::Dropped,
                    "attribute-ignored",
                    name,
                    None,
                    Some(aname),
                    "attribute is not part of the supported subset and was ignored",
                );
            }
        }
        if let Some(d) = el.attribute("d") {
            validate_path(d).map_err(|e| ctx.reject("path-invalid", name, id, Some("d"), e.0))?;
        }
        let hidden = el
            .ancestors()
            .filter(|n| n.is_element())
            .any(|n| text::own_prop(n, "display").as_deref() == Some("none"));
        if hidden && text::own_prop(el, "display").as_deref() == Some("none") && name != "stop" {
            ctx.report.push(
                ImportStatus::Dropped,
                "element-hidden",
                name,
                id,
                Some("display"),
                "display:none elements are not rendered and were dropped",
            );
        }
        let in_defs = el
            .ancestors()
            .any(|n| matches!(svg_name(&n), "defs" | "symbol"));
        let in_text_or_image = ancestors
            .iter()
            .any(|n| matches!(svg_name(n), "text" | "image"));
        match name {
            "use" => {
                plan.has_use = true;
                match href(el).map(|h| h.trim_start_matches('#')) {
                    Some(t) if by_id.contains_key(t) => {}
                    _ if hidden => {}
                    _ => ctx.report.push(
                        ImportStatus::Dropped,
                        "use-unresolved",
                        name,
                        id,
                        Some("href"),
                        "<use> target does not exist; nothing is drawn",
                    ),
                }
            }
            "linearGradient" | "radialGradient" => {
                let n = el.children().filter(|c| svg_name(c) == "stop").count();
                ctx.stops += n;
                if n > limits::STOPS_PER_GRADIENT || ctx.stops > limits::STOPS_TOTAL {
                    return Err(ctx.reject(
                        "limit-stops",
                        name,
                        id,
                        None,
                        format!(
                            "SVG gradient stop limits are {} per gradient and {} per import",
                            limits::STOPS_PER_GRADIENT,
                            limits::STOPS_TOTAL
                        ),
                    ));
                }
                // href inheritance chain: bounded, acyclic.
                let mut seen = HashSet::new();
                let mut cur = el;
                let mut hops = 0;
                while let Some(target) =
                    href(cur).and_then(|h| by_id.get(h.trim_start_matches('#')).copied())
                {
                    if !seen.insert(cur.id()) || target == el {
                        return Err(ctx.reject(
                            "gradient-cycle",
                            name,
                            id,
                            Some("href"),
                            "gradient href chain is cyclic",
                        ));
                    }
                    hops += 1;
                    if hops > limits::GRADIENT_HREF_CHAIN {
                        return Err(ctx.reject(
                            "limit-gradient-chain",
                            name,
                            id,
                            Some("href"),
                            format!(
                                "gradient href chain limit is {}",
                                limits::GRADIENT_HREF_CHAIN
                            ),
                        ));
                    }
                    cur = target;
                }
            }
            "tspan" if !ancestors.iter().any(|n| svg_name(n) == "text") => {
                return Err(ctx.reject(
                    "text-child-unsupported",
                    name,
                    id,
                    None,
                    "<tspan> outside <text> is unsupported; import cancelled",
                ));
            }
            "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon" | "path" => {
                let size = |a: &str| el.attribute(a).and_then(|v| text::first_len(v, 12.).0);
                let empty = match name {
                    "rect" => {
                        size("width").is_some_and(|w| w <= 0.)
                            || size("height").is_some_and(|h| h <= 0.)
                            || el.attribute("width").is_none()
                            || el.attribute("height").is_none()
                    }
                    "circle" => size("r").is_none_or(|r| r <= 0.),
                    _ => false,
                };
                if empty && !hidden {
                    ctx.report.push(
                        ImportStatus::Dropped,
                        "element-empty",
                        name,
                        id,
                        None,
                        "shape has no positive size and draws nothing; dropped",
                    );
                } else if !in_defs && !hidden && !in_text_or_image {
                    paint_candidates += 1;
                }
            }
            "text" if !in_text_or_image && !hidden => {
                let tp: TextPlan =
                    text::plan(el).map_err(|e| ctx.reject(e.code, "text", id, None, e.message))?;
                for n in &tp.notes {
                    ctx.report.push(
                        n.status,
                        n.code,
                        "text",
                        id,
                        n.attribute.as_deref(),
                        n.message.clone(),
                    );
                }
                if tp.runs.is_empty() {
                    ctx.report.push(
                        ImportStatus::Dropped,
                        "text-empty",
                        "text",
                        id,
                        None,
                        "text without characters has nothing to import",
                    );
                    plan.edits.push((el.range(), String::new()));
                    continue;
                }
                let idx = plan.phs.len();
                let rect_id = id.map_or_else(|| format!("__splinely_ph_{idx}"), str::to_owned);
                plan.edits.push((
                    el.range(),
                    placeholder_rect(source, el, &rect_id, tp.bbox, true),
                ));
                plan.ph_ids.insert(rect_id, idx);
                plan.phs.push(Placeholder {
                    kind: PhKind::Text(tp.runs),
                    src_id: id.map(str::to_owned),
                    in_defs,
                    seen: false,
                });
            }
            "image" if !in_text_or_image && !hidden => {
                ctx.images += 1;
                if ctx.images > limits::IMAGES {
                    return Err(ctx.reject(
                        "limit-images",
                        name,
                        id,
                        None,
                        format!("SVG image limit is {}", limits::IMAGES),
                    ));
                }
                let remove = |plan: &mut Plan| plan.edits.push((el.range(), String::new()));
                if depth > 0 {
                    ctx.report.push(
                        ImportStatus::Dropped,
                        "image-nested-dropped",
                        name,
                        id,
                        Some("href"),
                        "images inside an imported SVG image are not imported (nesting limit 1)",
                    );
                    remove(&mut plan);
                    continue;
                }
                let Some(h) = href(el) else {
                    ctx.report.push(
                        ImportStatus::Dropped,
                        "image-no-href",
                        name,
                        id,
                        None,
                        "<image> without href draws nothing; dropped",
                    );
                    remove(&mut plan);
                    continue;
                };
                let image = parse_data_uri(h, &mut ctx.image_budget)
                    .map_err(|e| ctx.reject(e.code(), name, id, Some("href"), e.message()))?;
                let font = text::font_size(el);
                let (x, y) = (
                    len_px(el.attribute("x"), font).unwrap_or(0.),
                    len_px(el.attribute("y"), font).unwrap_or(0.),
                );
                let (w, h) = (
                    len_px(el.attribute("width"), font),
                    len_px(el.attribute("height"), font),
                );
                if w.is_some_and(|v| v <= 0.) || h.is_some_and(|v| v <= 0.) {
                    ctx.report.push(
                        ImportStatus::Dropped,
                        "element-empty",
                        name,
                        id,
                        None,
                        "image with zero width or height draws nothing; dropped",
                    );
                    remove(&mut plan);
                    continue;
                }
                let par = el
                    .attribute("preserveAspectRatio")
                    .and_then(|v| v.parse::<svgtypes::AspectRatio>().ok())
                    .unwrap_or(svgtypes::AspectRatio {
                        defer: false,
                        align: svgtypes::Align::XMidYMid,
                        slice: false,
                    });
                use svgtypes::Align as A;
                let (ax, ay) = match par.align {
                    A::XMinYMin => (0., 0.),
                    A::XMidYMin => (0.5, 0.),
                    A::XMaxYMin => (1., 0.),
                    A::XMinYMid => (0., 0.5),
                    A::XMaxYMid => (1., 0.5),
                    A::XMinYMax => (0., 1.),
                    A::XMidYMax => (0.5, 1.),
                    A::XMaxYMax => (1., 1.),
                    A::XMidYMid | A::None => (0.5, 0.5),
                };
                let (rw, rh) = (
                    w.unwrap_or(f64::from(image.width.max(1))),
                    h.unwrap_or(f64::from(image.height.max(1))),
                );
                let idx = plan.phs.len();
                let rect_id = id.map_or_else(|| format!("__splinely_ph_{idx}"), str::to_owned);
                plan.edits.push((
                    el.range(),
                    placeholder_rect(source, el, &rect_id, [x, y, rw, rh], false),
                ));
                plan.ph_ids.insert(rect_id, idx);
                plan.phs.push(Placeholder {
                    kind: PhKind::Image {
                        image,
                        x,
                        y,
                        w,
                        h,
                        par: Par {
                            ax,
                            ay,
                            none: par.align == A::None,
                            slice: par.slice,
                        },
                    },
                    src_id: id.map(str::to_owned),
                    in_defs,
                    seen: false,
                });
            }
            _ => {}
        }
    }
    plan.candidates = paint_candidates;
    // `<use>` expansion bound (billion laughs): total expanded element count is computed
    // with memoisation, cycles and over-deep chains are rejected before usvg sees the file.
    fn size(
        el: XNode,
        by_id: &HashMap<&str, XNode>,
        memo: &mut HashMap<roxmltree::NodeId, u64>,
        stack: &mut Vec<roxmltree::NodeId>,
        depth: usize,
    ) -> Result<u64, &'static str> {
        if let Some(v) = memo.get(&el.id()) {
            return Ok(*v);
        }
        if stack.contains(&el.id()) {
            return Err("use-cycle");
        }
        if depth > limits::DEPTH + limits::USE_DEPTH {
            return Err("limit-use-depth");
        }
        stack.push(el.id());
        let mut total = 1u64;
        for c in el.children().filter(|c| c.is_element()) {
            total = total.saturating_add(size(c, by_id, memo, stack, depth + 1)?);
        }
        if svg_name(&el) == "use" {
            if let Some(t) = href(el).and_then(|h| by_id.get(h.trim_start_matches('#'))) {
                total = total.saturating_add(size(*t, by_id, memo, stack, depth + 1)?);
            }
        }
        stack.pop();
        if total > limits::EXPANDED_ELEMENTS {
            return Err("limit-expansion");
        }
        memo.insert(el.id(), total);
        Ok(total)
    }
    if plan.has_use {
        let mut memo = HashMap::new();
        if let Err(code) = size(root, &by_id, &mut memo, &mut vec![], 0) {
            let msg = match code {
                "use-cycle" => "<use> references form a cycle".to_owned(),
                "limit-use-depth" => {
                    format!("<use> chain deeper than {} levels", limits::USE_DEPTH)
                }
                _ => format!(
                    "<use> expansion exceeds {} elements (billion-laughs guard)",
                    limits::EXPANDED_ELEMENTS
                ),
            };
            return Err(ctx.reject(code, "use", None, Some("href"), msg));
        }
    }
    Ok(plan)
}

const COPIED: &[&str] = &[
    "transform",
    "opacity",
    "fill",
    "stroke",
    "stroke-width",
    "fill-opacity",
    "stroke-opacity",
    "fill-rule",
    "display",
    "visibility",
    "color",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "paint-order",
];

/// `<rect>` standing in for a `<text>`/`<image>`: same paint, transform and visibility.
fn placeholder_rect(source: &str, el: XNode, id: &str, bbox: [f64; 4], paint: bool) -> String {
    let start = el.range().start;
    let qname_end = source[start + 1..]
        .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .map_or(start + 1, |i| start + 1 + i);
    let qname = &source[start + 1..qname_end];
    let tag = match qname.rsplit_once(':') {
        Some((prefix, _)) => format!("{prefix}:rect"),
        None => "rect".to_owned(),
    };
    let mut s = format!(
        "<{tag} id=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
        escape(id),
        bbox[0],
        bbox[1],
        bbox[2],
        bbox[3]
    );
    for a in el.attributes().filter(|a| a.namespace().is_none()) {
        let n = a.name();
        if COPIED.contains(&n) && (paint || !matches!(n, "fill" | "stroke")) {
            let _ = write!(s, " {n}=\"{}\"", escape(a.value()));
        }
    }
    if let Some(style) = el.attribute("style") {
        let kept: Vec<&str> = style
            .split(';')
            .filter(|d| {
                d.split_once(':').is_some_and(|(k, _)| {
                    COPIED.contains(&k.trim()) && (paint || !matches!(k.trim(), "fill" | "stroke"))
                })
            })
            .collect();
        if !kept.is_empty() {
            let _ = write!(s, " style=\"{}\"", escape(&kept.join(";")));
        }
    }
    if !paint {
        s.push_str(" fill=\"#000\" stroke=\"none\"");
    }
    s.push_str("/>");
    s
}

fn decompose(t: Ts) -> (Transform, bool, bool) {
    let (a, b, c, d) = (t.sx as f64, t.ky as f64, t.kx as f64, t.sy as f64);
    let sx = a.hypot(b);
    if sx < 1e-9 {
        return (Transform::default(), false, true);
    }
    let sy = (a * d - b * c) / sx;
    let skewed = sy.abs() < 1e-9 || (a * c + b * d).abs() / (sx * sy.abs()) > 1e-4;
    let r = |v: f64| (v * 1e6).round() / 1e6 + 0.;
    (
        Transform {
            x: r(t.tx as f64),
            y: r(t.ty as f64),
            rotation: r(b.atan2(a).to_degrees()),
            scale_x: r(sx),
            scale_y: r(sy),
        },
        skewed,
        false,
    )
}

struct Walk<'a> {
    ctx: &'a mut Ctx,
    plan: Plan,
    prefix: &'a str,
    base: Ts,
    depth: usize,
    emitted: usize,
}

impl Walk<'_> {
    fn visit(
        &mut self,
        group: &usvg::Group,
        parent_opacity: f32,
        parent_group: Option<String>,
    ) -> Result<(), DocumentError> {
        let paints = |g: &usvg::Group| {
            fn count(g: &usvg::Group) -> usize {
                g.children()
                    .iter()
                    .map(|n| match n {
                        usvg::Node::Group(g) => count(g),
                        usvg::Node::Path(p) => usize::from(p.is_visible()),
                        _ => 2,
                    })
                    .sum()
            }
            count(g)
        };
        if (group.opacity().get() != 1. && paints(group) > 1)
            || group.clip_path().is_some()
            || group.mask().is_some()
            || !group.filters().is_empty()
            || group.blend_mode() != usvg::BlendMode::Normal
        {
            return Err(self.ctx.reject(
                "group-effects",
                "g",
                (!group.id().is_empty()).then(|| group.id()),
                None,
                "SVG group opacity, clipping, masks, filters or blending unsupported; import cancelled",
            ));
        }
        for child in group.children() {
            match child {
                usvg::Node::Group(g) => {
                    let mut inner = parent_group.clone();
                    if !g.id().is_empty()
                        && valid_import_id(g.id())
                        && g.id() != "splinely-project"
                        && !self.plan.ph_ids.contains_key(g.id())
                    {
                        let id = self.ctx.unique(format!("{}{}", self.prefix, g.id()));
                        self.ctx.groups.push(Group {
                            id: id.clone(),
                            name: g.id().to_owned(),
                            parent_id: parent_group.clone(),
                        });
                        self.ctx.report.push(
                            ImportStatus::Imported,
                            "group",
                            "g",
                            Some(&id),
                            None,
                            "became a named group",
                        );
                        inner = Some(id);
                    }
                    self.visit(g, parent_opacity * group.opacity().get(), inner)?;
                }
                usvg::Node::Path(path) => {
                    self.path(path, parent_opacity * group.opacity().get(), &parent_group)?
                }
                _ => {
                    return Err(self.ctx.reject(
                        "node-unsupported",
                        "image",
                        None,
                        None,
                        "SVG images/text are not supported for vector import",
                    ))
                }
            }
        }
        Ok(())
    }

    fn push_node(&mut self, node: Node, element: &str, message: &str) -> Result<(), DocumentError> {
        if self.ctx.nodes.len() >= limits::NODES {
            return Err(self.ctx.reject(
                "limit-nodes",
                element,
                Some(&node.id),
                None,
                format!("imported node limit is {}", limits::NODES),
            ));
        }
        self.ctx.report.push(
            ImportStatus::Imported,
            "node",
            element,
            Some(&node.id),
            None,
            message,
        );
        self.ctx.nodes.push(node);
        Ok(())
    }

    /// Fill, stroke and opacity of a node from the usvg path. `baked`: geometry already
    /// carries `abs` (path nodes); otherwise the node keeps a transform (text).
    #[allow(clippy::too_many_arguments)]
    fn style(
        &mut self,
        node: &mut Node,
        path: &usvg::Path,
        opacity: f32,
        to_space: Ts,
        bbox: [f64; 4],
        element: &str,
        stroke_scale: Option<f64>,
    ) -> Result<(), DocumentError> {
        let mut notes: Notes = vec![];
        let solid = |ctx: &mut Ctx, p: &usvg::Paint, o: f32, id: &str| match p {
            usvg::Paint::Color(c) => {
                let mut c = Color::rgb(c.red, c.green, c.blue);
                c.a = o as f64;
                Ok(c)
            }
            _ => Err(ctx.reject(
                "paint-unsupported",
                element,
                Some(id),
                None,
                "SVG patterns unsupported; import cancelled",
            )),
        };
        node.style.opacity = opacity as f64;
        node.style.fill = match path.fill() {
            Some(f) => match map_paint(f.paint(), f.opacity().get(), to_space, bbox, &mut notes) {
                Mapped::Gradient(g) => {
                    let first = g.stops[0].color;
                    node.gradient = Some(g);
                    first
                }
                Mapped::Solid(c) => c,
                Mapped::NotGradient => solid(self.ctx, f.paint(), f.opacity().get(), &node.id)?,
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
                return Err(self.ctx.reject(
                    "stroke-unsupported",
                    element,
                    Some(&node.id),
                    Some("stroke-linecap"),
                    "SVG non-default stroke caps/joins/dashes unsupported; import cancelled",
                ));
            }
            node.style.stroke = Some(
                match map_paint(s.paint(), s.opacity().get(), to_space, bbox, &mut notes) {
                    Mapped::Gradient(g) => {
                        notes.push((
                            "gradient-stroke",
                            "stroke gradients are not supported; the first stop colour is used"
                                .into(),
                        ));
                        g.stops[0].color
                    }
                    Mapped::Solid(c) => c,
                    Mapped::NotGradient => solid(self.ctx, s.paint(), s.opacity().get(), &node.id)?,
                },
            );
            node.style.stroke_width = s.width().get() as f64 * stroke_scale.unwrap_or(1.);
        }
        if path.paint_order() != usvg::PaintOrder::FillAndStroke {
            return Err(self.ctx.reject(
                "paint-order-unsupported",
                element,
                Some(&node.id),
                Some("paint-order"),
                "SVG stroke-first paint order unsupported",
            ));
        }
        for (code, message) in notes {
            self.ctx.report.push(
                ImportStatus::Approximated,
                code,
                element,
                Some(&node.id),
                None,
                message,
            );
        }
        Ok(())
    }

    fn path(
        &mut self,
        path: &usvg::Path,
        opacity: f32,
        parent_group: &Option<String>,
    ) -> Result<(), DocumentError> {
        if let Some(idx) = self.plan.ph_ids.get(path.id()).copied() {
            return self.placeholder(idx, path, opacity, parent_group);
        }
        self.emitted += 1;
        let src_id = (!path.id().is_empty()).then(|| path.id());
        if !path.is_visible() {
            self.ctx.report.push(
                ImportStatus::Dropped,
                "element-hidden",
                "path",
                src_id,
                Some("visibility"),
                "invisible element (visibility:hidden or no paint) was dropped",
            );
            return Ok(());
        }
        let abs = path.abs_transform().post_concat(self.base);
        let transformed = path.data().clone().transform(abs).ok_or_else(|| {
            self.ctx.reject(
                "transform-invalid",
                "path",
                src_id,
                None,
                "Invalid SVG transform",
            )
        })?;
        let bounds = transformed.compute_tight_bounds().ok_or_else(|| {
            self.ctx.reject(
                "bounds-overflow",
                "path",
                src_id,
                None,
                "SVG path bounds overflow",
            )
        })?;
        let mut d = String::new();
        for segment in transformed.segments() {
            use usvg::tiny_skia_path::PathSegment::*;
            let _ = match segment {
                MoveTo(p) => write!(d, "M{} {} ", p.x, p.y),
                LineTo(p) => write!(d, "L{} {} ", p.x, p.y),
                QuadTo(a, b) => write!(d, "Q{} {} {} {} ", a.x, a.y, b.x, b.y),
                CubicTo(a, b, c) => {
                    write!(d, "C{} {} {} {} {} {} ", a.x, a.y, b.x, b.y, c.x, c.y)
                }
                Close => write!(d, "Z "),
            };
        }
        let evenodd = path
            .fill()
            .is_some_and(|f| f.rule() == usvg::FillRule::EvenOdd);
        let wanted = if path.id().is_empty()
            || path.id() == "splinely-project"
            || !valid_import_id(path.id())
        {
            format!("svg-path-{}", self.ctx.nodes.len() + 1)
        } else {
            path.id().to_owned()
        };
        let id = self.ctx.unique(format!("{}{}", self.prefix, wanted));
        let bbox = [
            bounds.x() as f64,
            bounds.y() as f64,
            bounds.width() as f64,
            bounds.height() as f64,
        ];
        let mut node = Node::new(
            id,
            NodeKind::Path {
                d,
                bounds: bbox,
                evenodd,
            },
        );
        node.group = parent_group.clone();
        let mut scale = None;
        if path.stroke().is_some() {
            let sx = (abs.sx * abs.sx + abs.ky * abs.ky).sqrt() as f64;
            let sy = (abs.kx * abs.kx + abs.sy * abs.sy).sqrt() as f64;
            if sx == 0.
                || sy == 0.
                || (sx - sy).abs() > 1e-5 * sx.max(sy)
                || (abs.sx as f64 * abs.kx as f64 + abs.ky as f64 * abs.sy as f64).abs()
                    > 1e-5 * sx * sy
            {
                return Err(self.ctx.reject(
                    "stroke-transform",
                    "path",
                    src_id,
                    Some("stroke"),
                    "SVG nonuniform transformed stroke unsupported",
                ));
            }
            scale = Some(sx);
        }
        self.style(&mut node, path, opacity, abs, bbox, "path", scale)?;
        self.push_node(
            node,
            "path",
            "became an editable path with baked transforms",
        )
    }

    fn placeholder(
        &mut self,
        idx: usize,
        path: &usvg::Path,
        opacity: f32,
        parent_group: &Option<String>,
    ) -> Result<(), DocumentError> {
        self.plan.phs[idx].seen = true;
        let ph = self.plan.phs[idx].clone();
        let src_id = ph.src_id.as_deref();
        let abs = path.abs_transform().post_concat(self.base);
        match ph.kind {
            PhKind::Text(runs) => {
                if !path.is_visible() {
                    self.ctx.report.push(
                        ImportStatus::Dropped,
                        "element-hidden",
                        "text",
                        src_id,
                        Some("visibility"),
                        "invisible text was dropped",
                    );
                    return Ok(());
                }
                let (transform, skewed, degenerate) = decompose(abs);
                if degenerate {
                    self.ctx.report.push(
                        ImportStatus::Dropped,
                        "text-degenerate-transform",
                        "text",
                        src_id,
                        Some("transform"),
                        "transform collapses the text to nothing; dropped",
                    );
                    return Ok(());
                }
                let first = match src_id {
                    Some(i) if valid_import_id(i) && i != "splinely-project" => i.to_owned(),
                    _ => format!("svg-text-{}", self.ctx.nodes.len() + 1),
                };
                for (i, run) in runs.iter().enumerate() {
                    let wanted = if i == 0 {
                        first.clone()
                    } else {
                        format!("{first}-{}", i + 1)
                    };
                    let id = self.ctx.unique(format!("{}{}", self.prefix, wanted));
                    let mut node = Node::new(
                        id,
                        NodeKind::Text {
                            x: run.x,
                            y: run.y,
                            text: run.text.clone(),
                            font_size: run.font_size,
                        },
                    );
                    node.transform = transform;
                    node.group = parent_group.clone();
                    self.style(
                        &mut node,
                        path,
                        opacity,
                        Ts::identity(),
                        run.bbox(),
                        "text",
                        None,
                    )?;
                    if skewed {
                        self.ctx.report.push(
                            ImportStatus::Approximated,
                            "text-skew",
                            "text",
                            Some(&node.id),
                            Some("transform"),
                            "skew in the text transform is not representable; rotation and scale are kept",
                        );
                    }
                    self.push_node(node, "text", "became an editable text node")?;
                }
            }
            PhKind::Image {
                image,
                x,
                y,
                w,
                h,
                par,
            } => {
                let mime = image.format.mime();
                if !path.is_visible() {
                    self.ctx.report.push(
                        ImportStatus::Dropped,
                        "element-hidden",
                        "image",
                        src_id,
                        Some("visibility"),
                        "invisible image was dropped",
                    );
                    return Ok(());
                }
                if image.format != ImageFormat::Svg {
                    self.ctx.report.push(
                        ImportStatus::Dropped,
                        "image-raster",
                        "image",
                        src_id,
                        Some("href"),
                        format!(
                            "{mime} {}x{} ({} bytes) at x={x} y={y} was validated but not imported: the document has no image node kind or image asset store",
                            image.width,
                            image.height,
                            image.bytes.len()
                        ),
                    );
                    return Ok(());
                }
                if self.depth > 0 {
                    return Ok(());
                }
                self.ctx.nested += 1;
                if self.ctx.nested > limits::NESTED_SVGS {
                    return Err(self.ctx.reject(
                        "limit-nested-svgs",
                        "image",
                        src_id,
                        Some("href"),
                        format!("nested SVG image limit is {}", limits::NESTED_SVGS),
                    ));
                }
                let source = String::from_utf8(image.bytes).map_err(|_| {
                    self.ctx.reject(
                        "image-malformed",
                        "image",
                        src_id,
                        Some("href"),
                        "SVG image is not UTF-8",
                    )
                })?;
                let wanted = match src_id {
                    Some(i) if valid_import_id(i) => i.to_owned(),
                    _ => format!("svg-image-{}", self.ctx.nested),
                };
                let gid = self.ctx.unique(format!("{}{}", self.prefix, wanted));
                self.ctx.groups.push(Group {
                    id: gid.clone(),
                    name: wanted,
                    parent_id: parent_group.clone(),
                });
                let place = move |nw: f64, nh: f64| -> Ts {
                    let (iw, ih) = (w.unwrap_or(nw), h.unwrap_or(nh));
                    let (sx, sy) = if par.none {
                        (iw / nw, ih / nh)
                    } else {
                        let s = if par.slice {
                            (iw / nw).max(ih / nh)
                        } else {
                            (iw / nw).min(ih / nh)
                        };
                        (s, s)
                    };
                    let (tx, ty) = (x + (iw - nw * sx) * par.ax, y + (ih - nh * sy) * par.ay);
                    Ts::from_row(sx as f32, 0., 0., sy as f32, tx as f32, ty as f32)
                        .post_concat(abs)
                };
                convert_document(
                    &source,
                    self.ctx,
                    self.depth + 1,
                    &place,
                    &format!("{gid}-"),
                    Some(gid.clone()),
                    opacity,
                )?;
                self.ctx.report.push(
                    ImportStatus::Imported,
                    "image-svg",
                    "image",
                    Some(&gid),
                    Some("href"),
                    "nested SVG image imported as a group of vector nodes",
                );
                self.ctx.report.push(
                    ImportStatus::Approximated,
                    "image-clip",
                    "image",
                    Some(&gid),
                    None,
                    if par.slice {
                        "preserveAspectRatio slice: the image box is not clipped, overflow stays visible"
                    } else {
                        "the image box is not used as a clip; content outside it stays visible"
                    },
                );
            }
        }
        Ok(())
    }
}

/// Convert one SVG document (top level or nested image) into `ctx`.
/// `place(w, h)` gives the matrix from the document's own size to the scene.
pub(crate) fn convert_document(
    source: &str,
    ctx: &mut Ctx,
    depth: usize,
    place: &dyn Fn(f64, f64) -> Ts,
    prefix: &str,
    parent_group: Option<String>,
    parent_opacity: f32,
) -> Result<DocInfo, DocumentError> {
    let (limit, label) = if depth == 0 {
        (limits::SOURCE_BYTES, "4 MiB".to_owned())
    } else {
        (
            limits::NESTED_SVG_BYTES,
            format!("{} bytes", limits::NESTED_SVG_BYTES),
        )
    };
    if source.len() > limit {
        return Err(ctx.reject(
            "limit-source",
            "svg",
            None,
            None,
            format!("SVG import limit is {label}"),
        ));
    }
    let opts = roxmltree::ParsingOptions {
        allow_dtd: false,
        nodes_limit: limits::XML_NODES,
    };
    let xml = Document::parse_with_options(source, opts)
        .map_err(|e| ctx.reject("svg-malformed", "svg", None, None, format!("SVG XML: {e}")))?;
    let root = xml.root_element();
    if root.tag_name().name() != "svg" || root.tag_name().namespace() != Some(SVG_NS) {
        return Err(ctx.reject(
            "svg-root",
            root.tag_name().name(),
            None,
            None,
            "SVG root must be <svg> in the SVG namespace",
        ));
    }
    let mut plan = preflight(&xml, source, ctx, depth)?;
    let title = (depth == 0)
        .then(|| {
            root.children()
                .find(|n| svg_name(n) == "title")
                .and_then(|n| n.text())
                .map(str::to_owned)
        })
        .flatten();
    // Swap text/image elements for placeholders (usvg never sees them).
    let mut edits = std::mem::take(&mut plan.edits);
    edits.sort_by_key(|(r, _)| r.start);
    let mut rewritten = String::with_capacity(source.len());
    let mut at = 0;
    for (range, replacement) in &edits {
        if range.start < at {
            continue;
        }
        rewritten.push_str(&source[at..range.start]);
        rewritten.push_str(replacement);
        at = range.end;
    }
    rewritten.push_str(&source[at..]);
    let xml2 = Document::parse_with_options(&rewritten, opts)
        .map_err(|e| ctx.reject("svg-malformed", "svg", None, None, format!("SVG XML: {e}")))?;
    // No href resolver: nothing is ever read from files or the network; fonts stay empty
    // because text became placeholders.
    let options = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree = usvg::Tree::from_xmltree(&xml2, &options)
        .map_err(|e| ctx.reject("svg-invalid", "svg", None, None, format!("SVG: {e}")))?;
    let (width, height) = (tree.size().width() as f64, tree.size().height() as f64);
    let candidates = plan.candidates;
    let has_use = plan.has_use;
    let mut walk = Walk {
        ctx,
        plan,
        prefix,
        base: place(width, height),
        depth,
        emitted: 0,
    };
    walk.visit(tree.root(), parent_opacity, parent_group)?;
    let emitted = walk.emitted;
    let unseen: Vec<(bool, Option<String>)> = walk
        .plan
        .phs
        .iter()
        .filter(|p| !p.seen && !p.in_defs)
        .map(|p| (matches!(p.kind, PhKind::Text(_)), p.src_id.clone()))
        .collect();
    for (is_text, id) in unseen {
        walk.ctx.report.push(
            ImportStatus::Dropped,
            if is_text {
                "text-not-rendered"
            } else {
                "image-not-rendered"
            },
            if is_text { "text" } else { "image" },
            id.as_deref(),
            None,
            "element is not rendered (hidden, empty or unreachable) and was dropped",
        );
    }
    if !has_use && candidates > emitted {
        for _ in 0..candidates - emitted {
            walk.ctx.report.push(
                ImportStatus::Dropped,
                "element-not-rendered",
                "shape",
                None,
                None,
                "shape without fill or stroke (or otherwise not rendered by the SVG engine) was dropped",
            );
        }
    }
    Ok(DocInfo {
        width,
        height,
        title,
    })
}
