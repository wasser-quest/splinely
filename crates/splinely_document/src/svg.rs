use crate::*;
use std::fmt::Write;

impl Project {
    /// Export a standalone SVG. The caller supplies the bundled runtime source,
    /// avoiding a build-time dependency on a concurrently developed JS file.
    pub fn to_svg(&self, runtime: &str) -> Result<String, DocumentError> {
        let mut svg = self.svg_start(0.0)?;
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
    pub fn to_static_svg(&self, time: f64) -> Result<String, DocumentError> {
        let mut svg = self.svg_start(time)?;
        svg.push_str("</svg>\n");
        Ok(svg)
    }
    fn svg_start(&self, time: f64) -> Result<String, DocumentError> {
        let nodes = self.evaluate(time)?;
        let mut svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" data-splinely-version=\"{}\">\n", self.width,self.height,self.width,self.height,self.schema_version);
        writeln!(
            svg,
            "<!-- Created with Splinely {} -->",
            env!("CARGO_PKG_VERSION")
        )
        .unwrap();
        writeln!(svg, "<metadata><splinely:generator xmlns:splinely=\"urn:splinely:metadata\" name=\"Splinely\" version=\"{}\" schemaVersion=\"{}\"/></metadata>", env!("CARGO_PKG_VERSION"), self.schema_version).unwrap();
        writeln!(svg, "<title>{}</title>", xml(&self.name)?).unwrap();
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
            let common = format!("id=\"{}\" data-splinely-node=\"{}\" transform=\"translate({} {}) rotate({}) scale({} {})\" fill=\"{}\" fill-opacity=\"{}\" stroke=\"{}\" stroke-opacity=\"{}\" stroke-width=\"{}\" opacity=\"{}\"", xml(&node.id)?,xml(&node.id)?,t.x,t.y,t.rotation,t.scale_x,t.scale_y,fill,fill_opacity,s.stroke.map(css_color).unwrap_or_else(|| "none".into()),s.stroke.map(|c| c.a).unwrap_or(1.0),s.stroke_width,s.opacity);
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
        let taken = |id: &str| {
            self.nodes.iter().any(|n| n.id == id)
                || self.bones.iter().any(|n| n.id == id)
                || self.clips.iter().any(|n| n.id == id)
                || self.state_machines.iter().any(|n| n.id == id)
                || self.audio_assets.iter().any(|n| n.id == id)
                || self.audio_cues.iter().any(|n| n.id == id)
                || self.speech_cues.iter().any(|n| n.id == id)
                || self.groups.iter().any(|g| g.id == id)
        };
        let mut id = format!("{node}-gradient");
        while taken(&id) {
            id.push('_');
        }
        id
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
