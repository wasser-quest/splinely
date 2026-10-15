use crate::*;
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

impl Project {
    /// Export a standalone SVG embedding the project as base64 binary `.sply`.
    /// The caller supplies the bundled runtime source, avoiding a build-time
    /// dependency on a concurrently developed JS file.
    pub fn to_svg(&self, runtime: &str) -> Result<String, DocumentError> {
        self.to_svg_embed(runtime, EmbedFormat::Binary)
    }
    /// Export a standalone SVG with an explicit project embedding format.
    pub fn to_svg_embed(&self, runtime: &str, embed: EmbedFormat) -> Result<String, DocumentError> {
        let mut svg = self.svg_start(0.0, true)?;
        match embed {
            EmbedFormat::Json => {
                // JSON control characters are already escaped by serde. XML-forbidden
                // Unicode noncharacters must also be JSON escaped, not discarded.
                let json = serde_json::to_string(self)?
                    .replace('\u{fffe}', "\\ufffe")
                    .replace('\u{ffff}', "\\uffff");
                writeln!(
                    svg,
                    "<metadata id=\"splinely-project\">{}</metadata>",
                    xml(&json)?
                )
                .unwrap();
            }
            EmbedFormat::Binary => {
                // Base64 is plain ASCII (A-Za-z0-9+/=): always XML-safe, no escaping needed.
                writeln!(
                    svg,
                    "<metadata id=\"splinely-project\" data-encoding=\"sply-base64\">{}</metadata>",
                    crate::binary::base64_encode(&self.to_binary()?)
                )
                .unwrap();
            }
        }
        ensure_xml(runtime)?;
        writeln!(
            svg,
            "<script type=\"application/ecmascript\"><![CDATA[{}]]></script>",
            runtime.replace("]]>", "]]]]><![CDATA[>")
        )
        .unwrap();
        svg.push_str("</svg>\n");
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
        let mut svg = self.svg_start(time, false)?;
        svg.push_str("</svg>\n");
        Ok(svg)
    }
    fn svg_start(&self, time: f64, runtime: bool) -> Result<String, DocumentError> {
        let nodes = self.evaluate(time)?;
        // Accessibility semantics only for projects that opted in (see accessibility.rs).
        let a11y = self.a11y_enabled();
        let (title_id, desc_id) = (
            self.export_id("splinely-title"),
            self.export_id("splinely-desc"),
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
        let mut svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" data-splinely-version=\"{}\"{root_a11y}>\n", self.width,self.height,self.width,self.height,self.schema_version);
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
        let mut open: Vec<String> = vec![];
        for node in nodes {
            if !node.visible {
                continue;
            }
            let path = self.group_path(&node.id);
            let shared = open.iter().zip(&path).take_while(|(a, b)| a == b).count();
            while open.len() > shared {
                open.pop();
                svg.push_str("</g>\n");
            }
            for id in &path[shared..] {
                let name = self
                    .groups
                    .iter()
                    .find(|g| &g.id == id)
                    .map(|g| g.name.as_str())
                    .unwrap_or_default();
                writeln!(
                    svg,
                    "<g id=\"{}\" data-splinely-group=\"{}\" aria-label=\"{}\">",
                    xml(id)?,
                    xml(id)?,
                    xml(name)?
                )
                .unwrap();
                open.push(id.clone());
            }
            let t = node.transform;
            let s = node.style;
            let (fill, fill_opacity) = match &node.gradient {
                Some(gradient) => {
                    let id = self.gradient_id(&node.id);
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
    if s.chars().all(
        |c| matches!(c as u32, 9 | 10 | 13 | 0x20..=0xd7ff | 0xe000..=0xfffd | 0x10000..=0x10ffff),
    ) {
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
