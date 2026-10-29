use crate::accessibility::export_id_in;
use crate::*;
use std::collections::HashMap;
use std::fmt::Write;

/// How the project is embedded in `<metadata id="splinely-project">` of an exported SVG.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmbedFormat {
    /// Legacy form: project JSON text (`data-encoding` absent).
    Json,
    /// Base64 of the binary `.sply` file, marked `data-encoding="sply-base64"`.
    #[default]
    Binary,
}

/// Largest SVG any export writes (bytes). Real assets stay under 0.5 MiB; the
/// cap turns pathological documents into an error instead of an out-of-memory abort.
pub const MAX_SVG_BYTES: usize = 256 * 1024 * 1024;
/// Most bytes of re-opened `<g>` tags per export. A group's ancestry is written
/// again whenever consecutive nodes leave it, so this bounds that amplification.
pub const MAX_SVG_GROUP_MARKUP_BYTES: usize = 32 * 1024 * 1024;

/// Options for [`Project::to_svg_with`] and [`Project::svg_from_bytes`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SvgOptions<'a> {
    /// How `<metadata id="splinely-project">` embeds the project.
    pub embed: EmbedFormat,
    /// Runtime source for the `<script>`. `None` writes no `<script>` at all, for
    /// hosts that already loaded the runtime and mount the SVG inline
    /// (`SplinelyMount` / `createRuntime` read the metadata).
    pub runtime: Option<&'a str>,
    /// Root `data-splinely-max-fps="N"`: frame cap for the runtime's auto-start (N >= 1).
    pub max_fps: Option<u32>,
    /// Root `data-splinely-native-animations="true|false"` for the runtime's auto-start.
    pub native_animations: Option<bool>,
}

fn too_large() -> DocumentError {
    DocumentError(format!(
        "SVG export exceeds {} MiB limit",
        MAX_SVG_BYTES / (1024 * 1024)
    ))
}

impl Project {
    /// Export a standalone SVG embedding the project as base64 binary `.sply`.
    /// The caller supplies the bundled runtime source, avoiding a build-time
    /// dependency on a concurrently developed JS file.
    pub fn to_svg(&self, runtime: &str) -> Result<String, DocumentError> {
        self.to_svg_embed(runtime, EmbedFormat::Binary)
    }
    /// Export a standalone SVG with an explicit project embedding format.
    pub fn to_svg_embed(&self, runtime: &str, embed: EmbedFormat) -> Result<String, DocumentError> {
        self.to_svg_with(&SvgOptions {
            embed,
            runtime: Some(runtime),
            ..Default::default()
        })
    }
    /// Export an interactive SVG as configured by `options` (validates once).
    pub fn to_svg_with(&self, options: &SvgOptions) -> Result<String, DocumentError> {
        self.validate()?;
        self.export_svg(options, None)
    }
    /// Load `.sply` (or project JSON) bytes and export them in one step, like
    /// `from_bytes` + `to_svg_with`. Binary input holding exactly the canonical
    /// members (nothing normalized or defaulted) is embedded byte for byte
    /// instead of being re-encoded; it decodes to the same project either way.
    pub fn svg_from_bytes(bytes: &[u8], options: &SvgOptions) -> Result<String, DocumentError> {
        if Self::is_binary(bytes) {
            let (project, exact) = Self::from_binary_exact(bytes)?;
            return project.export_svg(options, exact.then_some(bytes));
        }
        Self::from_bytes(bytes)?.export_svg(options, None)
    }
    /// `options` export of a validated project; `source` is its own `.sply` encoding.
    fn export_svg(
        &self,
        options: &SvgOptions,
        source: Option<&[u8]>,
    ) -> Result<String, DocumentError> {
        if options.max_fps == Some(0) {
            return Err(DocumentError("max_fps must be at least 1".into()));
        }
        let mut svg = self.svg_start(0.0, true, true, options)?;
        match options.embed {
            EmbedFormat::Json => {
                // JSON control characters are already escaped by serde. XML-forbidden
                // Unicode noncharacters must also be JSON escaped, not discarded.
                let json = serde_json::to_string(self)?
                    .replace('\u{fffe}', "\\ufffe")
                    .replace('\u{ffff}', "\\uffff");
                let json = xml(&json)?;
                if svg.len() + json.len() > MAX_SVG_BYTES {
                    return Err(too_large());
                }
                writeln!(svg, "<metadata id=\"splinely-project\">{json}</metadata>").unwrap();
            }
            EmbedFormat::Binary => {
                let encoded;
                let bytes = match source {
                    Some(bytes) => bytes,
                    None => {
                        encoded = crate::binary::encode_value(&serde_json::to_value(self)?)?;
                        &encoded
                    }
                };
                if svg.len() + bytes.len().div_ceil(3) * 4 > MAX_SVG_BYTES {
                    return Err(too_large());
                }
                // Base64 is plain ASCII (A-Za-z0-9+/=): always XML-safe, no escaping needed.
                writeln!(
                    svg,
                    "<metadata id=\"splinely-project\" data-encoding=\"sply-base64\">{}</metadata>",
                    crate::binary::base64_encode(bytes)
                )
                .unwrap();
            }
        }
        if let Some(runtime) = options.runtime {
            ensure_xml(runtime)?;
            if svg.len() + runtime.len() > MAX_SVG_BYTES {
                return Err(too_large());
            }
            svg.reserve(runtime.len() + 64);
            svg.push_str("<script type=\"application/ecmascript\"><![CDATA[");
            for (i, part) in runtime.split("]]>").enumerate() {
                if i > 0 {
                    svg.push_str("]]]]><![CDATA[>");
                }
                svg.push_str(part);
            }
            svg.push_str("]]></script>\n");
        }
        svg.push_str("</svg>\n");
        if svg.len() > MAX_SVG_BYTES {
            return Err(too_large());
        }
        Ok(svg)
    }
    /// Read the project back out of an exported SVG's `splinely-project` metadata
    /// (`data-encoding="sply-base64"` binary, or legacy JSON text).
    ///
    /// Lookup matches the JS runtime (`readProject`): the block is an element in
    /// the SVG namespace that is a direct child of the root `svg`, with
    /// `id="splinely-project"` or a `data-splinely-project` attribute. Exactly
    /// one such element must exist; decoys elsewhere (other namespaces, nested
    /// `svg`/`foreignObject`) are ignored and a second candidate is an error.
    pub fn from_svg_metadata(svg: &str) -> Result<Project, DocumentError> {
        const SVG_NS: &str = "http://www.w3.org/2000/svg";
        let doc = roxmltree::Document::parse(svg)
            .map_err(|e| DocumentError(format!("invalid SVG XML: {e}")))?;
        let root = doc.root_element();
        let mut candidates = root.children().filter(|n| {
            n.is_element()
                && n.tag_name().namespace() == Some(SVG_NS)
                && (n.attribute("id") == Some("splinely-project")
                    || n.has_attribute("data-splinely-project"))
        });
        let block = candidates
            .next()
            .ok_or_else(|| DocumentError("Missing splinely-project metadata".into()))?;
        if candidates.next().is_some() {
            return Err(DocumentError(
                "Multiple splinely-project metadata blocks".into(),
            ));
        }
        let text_len: usize = block
            .descendants()
            .filter(|n| n.is_text())
            .filter_map(|n| n.text())
            .map(str::len)
            .sum();
        let encoding = block.attribute("data-encoding");
        if encoding == Some("sply-base64") && text_len > crate::binary::MAX_BASE64_TEXT {
            return Err(DocumentError(
                "splinely-project exceeds 64 MiB limit".into(),
            ));
        }
        let text: String = block
            .descendants()
            .filter(|n| n.is_text())
            .filter_map(|n| n.text())
            .collect();
        match encoding {
            Some("sply-base64") => {
                let clean: String = text.split_ascii_whitespace().collect();
                let bytes = crate::binary::base64_decode_canonical(&clean).ok_or_else(|| {
                    DocumentError("splinely-project is not canonical base64".into())
                })?;
                Project::from_binary(&bytes)
            }
            None | Some("") | Some("json") => Project::from_json(&text),
            Some(other) => Err(DocumentError(format!(
                "unsupported splinely-project data-encoding: {other}"
            ))),
        }
    }
    pub fn to_static_svg(&self, time: f64) -> Result<String, DocumentError> {
        let mut svg = self.svg_start(time, false, false, &SvgOptions::default())?;
        svg.push_str("</svg>\n");
        Ok(svg)
    }
    /// Root element and drawing; `validated` skips the `validate` in `evaluate`.
    fn svg_start(
        &self,
        time: f64,
        runtime: bool,
        validated: bool,
        options: &SvgOptions,
    ) -> Result<String, DocumentError> {
        let nodes = if validated {
            let index = crate::animation::EvalIndex::build(self);
            self.evaluate_frame(&index, time, &EvaluationContext::default())?
                .0
        } else {
            self.evaluate(time)?
        };
        // Accessibility semantics only for projects that opted in (see accessibility.rs).
        let a11y = self.a11y_enabled();
        // Shared-namespace IDs, collected once for every generated export ID.
        let taken = if a11y || nodes.iter().any(|n| n.gradient.is_some()) {
            self.export_ids()
        } else {
            Default::default()
        };
        let (title_id, desc_id) = (
            export_id_in(&taken, "splinely-title"),
            export_id_in(&taken, "splinely-desc"),
        );
        let root_a11y = if a11y {
            let mut attrs = format!(
                " role=\"{}\" aria-labelledby=\"{title_id}\"",
                self.a11y_root_role(runtime)
            );
            if !self.description.is_empty() {
                write!(attrs, " aria-describedby=\"{desc_id}\"").unwrap();
            }
            attrs
        } else {
            String::new()
        };
        let mut host = String::new();
        if let Some(fps) = options.max_fps {
            write!(host, " data-splinely-max-fps=\"{fps}\"").unwrap();
        }
        if let Some(native) = options.native_animations {
            write!(host, " data-splinely-native-animations=\"{native}\"").unwrap();
        }
        let mut svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" data-splinely-version=\"{}\"{root_a11y}{host}>\n", self.width,self.height,self.width,self.height,self.schema_version);
        writeln!(
            svg,
            "<!-- Created with Splinely {} -->",
            env!("CARGO_PKG_VERSION")
        )
        .unwrap();
        writeln!(svg, "<metadata><splinely:generator xmlns:splinely=\"urn:splinely:metadata\" name=\"Splinely\" version=\"{}\" schemaVersion=\"{}\"/></metadata>", env!("CARGO_PKG_VERSION"), self.schema_version).unwrap();
        if a11y {
            writeln!(svg, "<title id=\"{title_id}\">{}</title>", xml(&self.name)?).unwrap();
            if !self.description.is_empty() {
                writeln!(
                    svg,
                    "<desc id=\"{desc_id}\">{}</desc>",
                    xml(&self.description)?
                )
                .unwrap();
            }
        } else {
            writeln!(svg, "<title>{}</title>", xml(&self.name)?).unwrap();
        }
        // Open <g> elements, outermost first, mirroring the group hierarchy.
        let groups: HashMap<&str, &Group> = self
            .groups
            .iter()
            .rev()
            .map(|g| (g.id.as_str(), g))
            .collect();
        let mut open: Vec<&str> = vec![];
        let mut path: Vec<&str> = vec![];
        let mut group_markup = 0usize;
        for node in nodes {
            if !node.visible {
                continue;
            }
            if svg.len() > MAX_SVG_BYTES {
                return Err(too_large());
            }
            // Same walk as `group_path`, from the node's own group (validated, depth <= 32).
            path.clear();
            let mut current = node
                .group
                .as_deref()
                .and_then(|id| groups.get_key_value(id));
            while let Some((&id, g)) = current {
                path.push(id);
                if path.len() > 32 {
                    break;
                }
                current = g.parent_id.as_deref().and_then(|p| groups.get_key_value(p));
            }
            path.reverse();
            let shared = open.iter().zip(&path).take_while(|(a, b)| a == b).count();
            while open.len() > shared {
                open.pop();
                svg.push_str("</g>\n");
            }
            for &id in &path[shared..] {
                let start = svg.len();
                let name = groups.get(id).map(|g| g.name.as_str()).unwrap_or_default();
                writeln!(
                    svg,
                    "<g id=\"{}\" data-splinely-group=\"{}\" aria-label=\"{}\">",
                    xml(id)?,
                    xml(id)?,
                    xml(name)?
                )
                .unwrap();
                group_markup += svg.len() - start;
                if group_markup > MAX_SVG_GROUP_MARKUP_BYTES {
                    return Err(DocumentError(format!(
                        "SVG export: nested group markup exceeds {} MiB",
                        MAX_SVG_GROUP_MARKUP_BYTES / (1024 * 1024)
                    )));
                }
                open.push(id);
            }
            let t = node.transform;
            let s = node.style;
            let (fill, fill_opacity) = match &node.gradient {
                Some(gradient) => {
                    let id = export_id_in(&taken, &format!("{}-gradient", node.id));
                    writeln!(svg, "<defs>{}</defs>", gradient.svg_element(&id)).unwrap();
                    (format!("url(#{id})"), 1.0)
                }
                None => (css_color(s.fill), s.fill.a),
            };
            let a11y_attrs = self.a11y_node_attributes(&node, xml)?;
            let common = format!("id=\"{}\" data-splinely-node=\"{}\" transform=\"translate({} {}) rotate({}) scale({} {})\" fill=\"{}\" fill-opacity=\"{}\" stroke=\"{}\" stroke-opacity=\"{}\" stroke-width=\"{}\" opacity=\"{}\"{a11y_attrs}", xml(&node.id)?,xml(&node.id)?,t.x,t.y,t.rotation,t.scale_x,t.scale_y,fill,fill_opacity,s.stroke.map(css_color).unwrap_or_else(|| "none".into()),s.stroke.map(|c| c.a).unwrap_or(1.0),s.stroke_width,s.opacity);
            match node.kind {
                NodeKind::Path {d,evenodd,..} => writeln!(svg,"<path {common} d=\"{}\" fill-rule=\"{}\"/>",xml(&d)?,if evenodd {"evenodd"}else{"nonzero"}).unwrap(),
                NodeKind::Rect {x,y,width,height,rx} => writeln!(svg,"<rect {common} x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" rx=\"{rx}\"/>").unwrap(),
                NodeKind::Circle {cx,cy,radius} => writeln!(svg,"<circle {common} cx=\"{cx}\" cy=\"{cy}\" r=\"{radius}\"/>").unwrap(),
                NodeKind::Text {x,y,text,font_size} => writeln!(svg,"<text {common} x=\"{x}\" y=\"{y}\" font-size=\"{font_size}\">{}</text>",xml(&text)?).unwrap(),
            }
        }
        for _ in open {
            svg.push_str("</g>\n");
        }
        if svg.len() > MAX_SVG_BYTES {
            return Err(too_large());
        }
        Ok(svg)
    }
}
impl Project {
    /// Export ID of a node's gradient, unique within the shared ID namespace.
    pub fn gradient_id(&self, node: &str) -> String {
        self.export_id(&format!("{node}-gradient"))
    }
}
fn css_color(c: Color) -> String {
    format!("rgb({}% {}% {}%)", c.r * 100.0, c.g * 100.0, c.b * 100.0)
}
fn ensure_xml(s: &str) -> Result<(), DocumentError> {
    // Byte scan: XML 1.0 forbids C0 controls except tab/LF/CR, and U+FFFE/U+FFFF
    // (UTF-8 EF BF BE/BF; EF always starts a character). Surrogates cannot occur in `str`.
    let b = s.as_bytes();
    let ok = b.iter().all(|&c| c >= 0x20 || matches!(c, 9 | 10 | 13))
        && !b
            .windows(3)
            .any(|w| w[0] == 0xef && w[1] == 0xbf && w[2] >= 0xbe);
    if ok {
        Ok(())
    } else {
        Err(DocumentError(
            "export contains a character forbidden by XML 1.0".into(),
        ))
    }
}
fn xml(s: &str) -> Result<String, DocumentError> {
    ensure_xml(s)?;
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Character references avoid XML line-ending normalization.
            '\r' => out.push_str("&#13;"),
            _ => out.push(c),
        }
    }
    Ok(out)
}
