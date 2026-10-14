//! Text-to-path conversion using local system fonts (via usvg's text shaping).
//! The resulting path keeps the node's ID, transform, style and animation of
//! shared properties; it no longer depends on fonts being installed.
use crate::*;
use std::sync::Arc;
pub use usvg::fontdb;

/// Serif family usvg should use, preferring common Linux fonts. usvg's own
/// default (Times New Roman) is missing on many systems.
pub fn default_font_family(fonts: &fontdb::Database) -> String {
    [
        "Noto Serif",
        "Liberation Serif",
        "DejaVu Serif",
        "Times New Roman",
    ]
    .iter()
    .find(|name| {
        fonts
            .query(&fontdb::Query {
                families: &[fontdb::Family::Name(name)],
                ..Default::default()
            })
            .is_some()
    })
    .map(|name| (*name).to_owned())
    .or_else(|| {
        fonts
            .faces()
            .next()
            .and_then(|face| face.families.first().map(|(name, _)| name.clone()))
    })
    .unwrap_or_else(|| "serif".into())
}

/// System fonts, loaded once per process (loading takes ~100 ms).
pub fn system_fonts() -> Arc<fontdb::Database> {
    static FONTS: std::sync::OnceLock<Arc<fontdb::Database>> = std::sync::OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone()
}

fn collect(group: &usvg::Group, d: &mut String) {
    for child in group.children() {
        match child {
            usvg::Node::Group(g) => collect(g, d),
            usvg::Node::Text(t) => collect(t.flattened(), d),
            usvg::Node::Path(p) => {
                let Some(path) = p.data().clone().transform(p.abs_transform()) else {
                    continue;
                };
                use usvg::tiny_skia_path::PathSegment::*;
                let n = |v: f32| {
                    let r = (v as f64 * 1000.).round() / 1000.;
                    format!("{}", r + 0.)
                };
                for segment in path.segments() {
                    let part = match segment {
                        MoveTo(p) => format!("M{} {} ", n(p.x), n(p.y)),
                        LineTo(p) => format!("L{} {} ", n(p.x), n(p.y)),
                        QuadTo(c, p) => format!("Q{} {} {} {} ", n(c.x), n(c.y), n(p.x), n(p.y)),
                        CubicTo(a, b, p) => format!(
                            "C{} {} {} {} {} {} ",
                            n(a.x),
                            n(a.y),
                            n(b.x),
                            n(b.y),
                            n(p.x),
                            n(p.y)
                        ),
                        Close => "Z ".into(),
                    };
                    d.push_str(&part);
                }
            }
            usvg::Node::Image(_) => {}
        }
    }
}

impl Project {
    /// Replace a text node's glyphs with an editable path (same ID and style).
    pub fn convert_text_to_path(
        &mut self,
        id: &str,
        fonts: Arc<fontdb::Database>,
    ) -> Result<(), DocumentError> {
        let index = self
            .nodes
            .iter()
            .position(|n| n.id == id)
            .ok_or_else(|| DocumentError(format!("missing node {id}")))?;
        let NodeKind::Text {
            x,
            y,
            text,
            font_size,
        } = &self.nodes[index].kind
        else {
            return Err(DocumentError(format!("node {id} is not text")));
        };
        if text.trim().is_empty() {
            return Err(DocumentError("empty text has no outline".into()));
        }
        let escaped = text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"><text x=\"{x}\" y=\"{y}\" font-size=\"{font_size}\">{escaped}</text></svg>"
        );
        let options = usvg::Options {
            font_family: default_font_family(&fonts),
            fontdb: fonts,
            ..Default::default()
        };
        let tree = usvg::Tree::from_str(&svg, &options)
            .map_err(|e| DocumentError(format!("text layout failed: {e}")))?;
        let mut d = String::new();
        collect(tree.root(), &mut d);
        let d = d.trim().to_owned();
        if d.is_empty() {
            return Err(DocumentError(
                "no font outlines available for this text (install a system font)".into(),
            ));
        }
        let bounds = crate::path_bounds(&d)?;
        let mut next = self.clone();
        next.nodes[index].kind = NodeKind::Path {
            d,
            bounds,
            evenodd: false,
        };
        next.nodes[index].original_kind = None;
        next.validate().map_err(|e| {
            DocumentError(format!(
                "cannot convert {id}: {e} (remove font-size animation first)"
            ))
        })?;
        *self = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_becomes_a_path_at_the_same_place() {
        let fonts = system_fonts();
        if fonts.faces().next().is_none() {
            return; // No fonts on this machine; nothing to outline.
        }
        let mut p = Project::default();
        let mut node = Node::text("t", 100., 200., "Hallo");
        node.style.fill = Color::rgb(255, 0, 0);
        if let NodeKind::Text { font_size, .. } = &mut node.kind {
            *font_size = 40.;
        }
        p.nodes.push(node);
        p.convert_text_to_path("t", fonts.clone()).unwrap();
        let NodeKind::Path { bounds, .. } = &p.nodes[0].kind else {
            panic!("expected a path")
        };
        // Baseline at y=200: glyphs sit above it and start near x=100.
        assert!(bounds[0] >= 99. && bounds[0] < 110., "{bounds:?}");
        assert!(bounds[1] < 200. && bounds[1] > 150., "{bounds:?}");
        assert!(bounds[2] > 60., "{bounds:?}");
        assert_eq!(p.nodes[0].style.fill, Color::rgb(255, 0, 0));
        assert!(
            p.convert_text_to_path("t", fonts).is_err(),
            "already a path"
        );
    }
}
