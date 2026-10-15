//! `<text>`/`<tspan>` planning for the SVG import.
//!
//! Choice (documented in docs/svg-import.md): the document's `Text` node has only
//! `x`, `y`, `text`, `font_size`. One `<text>` becomes one node; `<tspan>`s are
//! concatenated into it, except a tspan that carries its own `x`/`y`/`dx`/`dy`,
//! which starts a further node (so multi-line Inkscape text becomes one node per line).
//! Font family, weight, style, spacing and decoration are not stored: reported as dropped.
//! Glyph advances are unknown here, so widths use the editor's own 0.6 em estimate.
use crate::svg_import_report::{limits, ImportStatus};
use roxmltree::Node as XNode;

pub(crate) const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
/// Average advance in em, identical to the runtime's text box heuristic.
pub(crate) const ADVANCE_EM: f64 = 0.6;
/// Line box height in em, identical to the runtime's text box heuristic.
pub(crate) const LINE_EM: f64 = 1.2;

#[derive(Clone, Debug)]
pub(crate) struct Run {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub font_size: f64,
}

impl Run {
    pub fn width(&self) -> f64 {
        ADVANCE_EM * self.font_size * self.text.chars().count() as f64
    }
    /// `[x, y, w, h]` of the estimated box.
    pub fn bbox(&self) -> [f64; 4] {
        [
            self.x,
            self.y - self.font_size,
            self.width(),
            LINE_EM * self.font_size,
        ]
    }
}

pub(crate) struct Note {
    pub status: ImportStatus,
    pub code: &'static str,
    pub attribute: Option<String>,
    pub message: String,
}

pub(crate) struct TextPlan {
    pub runs: Vec<Run>,
    pub bbox: [f64; 4],
    pub notes: Vec<Note>,
}

pub(crate) struct TextError {
    pub code: &'static str,
    pub message: String,
}

/// Presentation attribute or inline-style declaration of this very element (style wins).
pub(crate) fn own_prop(el: XNode, name: &str) -> Option<String> {
    let mut found = el.attribute(name).map(|v| v.trim().to_owned());
    if let Some(style) = el.attribute("style") {
        for decl in style.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                if k.trim() == name {
                    found = Some(v.trim().trim_end_matches("!important").trim().to_owned());
                }
            }
        }
    }
    found
}

/// Inherited property value (nearest ancestor-or-self that sets it).
pub(crate) fn inherited_prop(el: XNode, name: &str) -> Option<String> {
    el.ancestors()
        .filter(|n| n.is_element())
        .find_map(|n| own_prop(n, name).filter(|v| v != "inherit"))
}

fn parse_font_size(value: &str, parent: f64) -> Option<f64> {
    let v = value.trim();
    let keyword = match v {
        "xx-small" => Some(7.),
        "x-small" => Some(8.),
        "small" => Some(10.),
        "medium" => Some(12.),
        "large" => Some(14.),
        "x-large" => Some(17.),
        "xx-large" => Some(20.),
        "larger" => Some(parent * 1.2),
        "smaller" => Some(parent / 1.2),
        _ => None,
    };
    let size = keyword.or_else(|| {
        let l: svgtypes::Length = v.parse().ok()?;
        use svgtypes::LengthUnit::*;
        Some(match l.unit {
            None | Px => l.number,
            Em => l.number * parent,
            Ex => l.number * parent / 2.,
            Pt => l.number * 4. / 3.,
            Pc => l.number * 16.,
            Mm => l.number * 96. / 25.4,
            Cm => l.number * 96. / 2.54,
            In => l.number * 96.,
            Percent => l.number / 100. * parent,
        })
    })?;
    (size.is_finite() && size > 0.).then_some(size.min(100_000.))
}

/// Resolved font size in user units (`em`/`%` relative to the inherited size; default 12).
pub(crate) fn font_size(el: XNode) -> f64 {
    let mut size = 12.;
    for n in el
        .ancestors()
        .filter(|n| n.is_element())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        if let Some(v) = own_prop(n, "font-size") {
            if let Some(s) = parse_font_size(&v, size) {
                size = s;
            }
        }
    }
    size
}

/// First number of an SVG number/length list; `Some(count)` of entries as second value.
pub(crate) fn first_len(value: &str, font: f64) -> (Option<f64>, usize) {
    let parts: Vec<&str> = value
        .split([' ', ',', '\t', '\n', '\r'])
        .filter(|s| !s.is_empty())
        .collect();
    let first = parts.first().and_then(|p| {
        let l: svgtypes::Length = p.parse().ok()?;
        use svgtypes::LengthUnit::*;
        let v = match l.unit {
            None | Px => l.number,
            Em => l.number * font,
            Ex => l.number * font / 2.,
            Pt => l.number * 4. / 3.,
            Pc => l.number * 16.,
            Mm => l.number * 96. / 25.4,
            Cm => l.number * 96. / 2.54,
            In => l.number * 96.,
            Percent => return Option::None,
        };
        v.is_finite().then_some(v.clamp(-1e7, 1e7))
    });
    (first, parts.len())
}

fn collapse(raw: &str, preserve: bool) -> String {
    let mapped: String = raw
        .chars()
        .map(|c| {
            if matches!(c, '\n' | '\r' | '\t') {
                ' '
            } else {
                c
            }
        })
        .collect();
    if preserve {
        return mapped;
    }
    let mut out = String::with_capacity(mapped.len());
    let mut space = false;
    for c in mapped.chars() {
        if c == ' ' {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(c);
            space = false;
        }
    }
    out.trim().to_owned()
}

struct State {
    runs: Vec<Run>,
    notes: Vec<Note>,
    text: String,
    x: f64,
    y: f64,
    font: f64,
    anchor: String,
    preserve: bool,
    segments: usize,
    chars: usize,
    err: Option<TextError>,
}

impl State {
    fn note(&mut self, status: ImportStatus, code: &'static str, attr: &str, message: String) {
        if !self
            .notes
            .iter()
            .any(|n| n.code == code && n.attribute.as_deref() == Some(attr))
        {
            self.notes.push(Note {
                status,
                code,
                attribute: Some(attr.to_owned()),
                message,
            });
        }
    }
    fn flush(&mut self) {
        let text = collapse(&std::mem::take(&mut self.text), self.preserve);
        let chars = text.chars().count();
        if chars == 0 {
            return;
        }
        self.segments += 1;
        self.chars += chars;
        if self.chars > limits::TEXT_CHARS || self.segments > limits::TEXT_SEGMENTS {
            self.err.get_or_insert(TextError {
                code: "text-limit",
                message: format!(
                    "text exceeds the limits of {} characters / {} segments",
                    limits::TEXT_CHARS,
                    limits::TEXT_SEGMENTS
                ),
            });
            return;
        }
        let mut run = Run {
            text,
            x: self.x,
            y: self.y,
            font_size: self.font,
        };
        let width = run.width();
        let advance = width;
        match self.anchor.as_str() {
            "middle" => run.x -= width / 2.,
            "end" => run.x -= width,
            _ => {}
        }
        // The cursor continues from the unshifted start of the run.
        self.x += advance;
        self.runs.push(run);
    }
}

fn walk(node: XNode, st: &mut State) {
    for child in node.children() {
        if st.err.is_some() {
            return;
        }
        if child.is_text() {
            st.text.push_str(child.text().unwrap_or(""));
        } else if child.is_element() && child.tag_name().name() == "tspan" {
            let fs = font_size(child);
            let get = |name: &str| own_prop(child, name);
            let list = |name: &str, st: &mut State| -> Option<f64> {
                let v = get(name)?;
                let (first, n) = first_len(&v, fs);
                if n > 1 {
                    st.note(
                        ImportStatus::Approximated,
                        "text-glyph-positions",
                        name,
                        format!("per-glyph {name} list: only the first value is used"),
                    );
                }
                first
            };
            let (x, y, dx, dy) = (list("x", st), list("y", st), list("dx", st), list("dy", st));
            if x.is_some() || y.is_some() || dx.is_some() || dy.is_some() {
                st.flush();
                st.x = x.unwrap_or(st.x) + dx.unwrap_or(0.);
                st.y = y.unwrap_or(st.y) + dy.unwrap_or(0.);
                st.font = fs;
            } else if (fs - st.font).abs() > 1e-9 {
                st.note(
                    ImportStatus::Approximated,
                    "text-tspan-style",
                    "font-size",
                    "a tspan with a different font-size is merged into its text; the node's size is used"
                        .into(),
                );
            }
            if let Some(a) = get("text-anchor") {
                if a != st.anchor {
                    st.note(
                        ImportStatus::Approximated,
                        "text-tspan-style",
                        "text-anchor",
                        "a tspan with its own text-anchor is merged into its text".into(),
                    );
                }
            }
            for prop in ["fill", "stroke"] {
                if get(prop).is_some() {
                    st.note(
                        ImportStatus::Approximated,
                        "text-tspan-style",
                        prop,
                        format!("a tspan with its own {prop} is merged into its text; the text element's paint is used"),
                    );
                }
            }
            walk(child, st);
        } else if child.is_element() {
            st.err.get_or_insert(TextError {
                code: "text-child-unsupported",
                message: format!(
                    "<{}> inside <text> is unsupported; import cancelled",
                    child.tag_name().name()
                ),
            });
        }
    }
}

pub(crate) fn plan(text: XNode) -> Result<TextPlan, TextError> {
    let font = font_size(text);
    let preserve = text
        .ancestors()
        .filter(|n| n.is_element())
        .find_map(|n| n.attribute((XML_NS, "space")))
        == Some("preserve");
    let mut st = State {
        runs: vec![],
        notes: vec![],
        text: String::new(),
        x: 0.,
        y: 0.,
        font,
        anchor: inherited_prop(text, "text-anchor").unwrap_or_else(|| "start".into()),
        preserve,
        segments: 0,
        chars: 0,
        err: None,
    };
    let list = |name: &str, st: &mut State| -> Option<f64> {
        let v = text.attribute(name)?;
        let (first, n) = first_len(v, font);
        if n > 1 {
            st.note(
                ImportStatus::Approximated,
                "text-glyph-positions",
                name,
                format!("per-glyph {name} list: only the first value is used"),
            );
        }
        first
    };
    let (x, y, dx, dy) = (
        list("x", &mut st),
        list("y", &mut st),
        list("dx", &mut st),
        list("dy", &mut st),
    );
    st.x = x.unwrap_or(0.) + dx.unwrap_or(0.);
    st.y = y.unwrap_or(0.) + dy.unwrap_or(0.);
    if text.attribute("rotate").is_some() {
        st.note(
            ImportStatus::Dropped,
            "text-rotate",
            "rotate",
            "per-glyph rotate is not supported".into(),
        );
    }
    for attr in ["textLength", "lengthAdjust"] {
        if text.attribute(attr).is_some() {
            st.note(
                ImportStatus::Dropped,
                "text-length",
                attr,
                format!("{attr} is not supported; natural text width is used"),
            );
        }
    }
    walk(text, &mut st);
    st.flush();
    if let Some(e) = st.err.take() {
        return Err(e);
    }
    // Properties the model cannot store, looked up on the text element, its ancestors and tspans.
    let mut holders: Vec<XNode> = text.ancestors().filter(|n| n.is_element()).collect();
    holders.extend(text.descendants().filter(|n| n.is_element() && n != &text));
    let seen = |prop: &str, ignore: &[&str]| -> Option<String> {
        holders
            .iter()
            .filter_map(|n| own_prop(*n, prop))
            .find(|v| !ignore.contains(&v.as_str()) && v != "inherit")
    };
    let dropped: [(&str, &[&str], &str); 8] = [
        (
            "font-family",
            &[],
            "the font family is not stored; the editor's default font is used",
        ),
        (
            "font-weight",
            &["normal", "400"],
            "font weight is not stored",
        ),
        ("font-style", &["normal"], "font style is not stored"),
        (
            "letter-spacing",
            &["normal", "0", "0px"],
            "letter spacing is not stored",
        ),
        (
            "word-spacing",
            &["normal", "0", "0px"],
            "word spacing is not stored",
        ),
        (
            "text-decoration",
            &["none"],
            "text decoration is not stored",
        ),
        (
            "dominant-baseline",
            &["auto", "alphabetic"],
            "dominant-baseline is not stored; the y coordinate is the baseline",
        ),
        (
            "baseline-shift",
            &["baseline", "0"],
            "baseline-shift is not stored",
        ),
    ];
    for (prop, ignore, msg) in dropped {
        if let Some(v) = seen(prop, ignore) {
            st.note(
                ImportStatus::Dropped,
                match prop {
                    "font-family" => "text-font-family",
                    "font-weight" | "font-style" => "text-font-style",
                    "letter-spacing" | "word-spacing" => "text-spacing",
                    "text-decoration" => "text-decoration",
                    _ => "text-baseline",
                },
                prop,
                format!("{msg} (source: {prop}: {v})"),
            );
        }
    }
    if let Some(v) = seen("writing-mode", &["lr-tb", "horizontal-tb", "lr"]) {
        st.note(
            ImportStatus::Dropped,
            "text-direction",
            "writing-mode",
            format!("only horizontal left-to-right text is stored (writing-mode: {v})"),
        );
    }
    if let Some(v) = seen("direction", &["ltr"]) {
        st.note(
            ImportStatus::Dropped,
            "text-direction",
            "direction",
            format!("only left-to-right text is stored (direction: {v})"),
        );
    }
    if st.anchor != "start" && !st.runs.is_empty() {
        let a = st.anchor.clone();
        st.note(
            ImportStatus::Approximated,
            "text-anchor",
            "text-anchor",
            format!("text-anchor: {a} becomes a start-anchored x using an estimated width ({ADVANCE_EM} em per character)"),
        );
    }
    let runs = std::mem::take(&mut st.runs);
    let notes = std::mem::take(&mut st.notes);
    let mut bbox: Option<[f64; 4]> = None;
    for r in &runs {
        let b = r.bbox();
        bbox = Some(match bbox {
            None => b,
            Some(a) => {
                let (x0, y0) = (a[0].min(b[0]), a[1].min(b[1]));
                let (x1, y1) = (
                    (a[0] + a[2]).max(b[0] + b[2]),
                    (a[1] + a[3]).max(b[1] + b[3]),
                );
                [x0, y0, x1 - x0, y1 - y0]
            }
        });
    }
    Ok(TextPlan {
        bbox: bbox.unwrap_or([0., 0., 1., 1.]),
        runs,
        notes,
    })
}
