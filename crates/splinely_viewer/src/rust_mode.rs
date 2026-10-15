//! `--runtime rust`: the viewer links `splinely_runtime` and drives it in-process.
//!
//! The webview only shows a static SVG (`Project::to_static_svg`) plus two tiny
//! dependency-free scripts: `frame_applier.js` writes evaluated nodes into the
//! SVG and `rust_host.js` forwards pointer/keyboard input. Everything here is
//! GUI-free so it can be tested without a display; `launch_prepared` only adds
//! the timer and the `wry` IPC plumbing.
//!
//! Not reproduced (reported as diagnostics): `customJs` is never executed and
//! audio is not played. See `docs/native-runtime.md`.
use crate::{PreparedViewer, RuntimeMode, MAX_INPUT_BYTES, MAX_XML_NODES};
use serde::Deserialize;
use serde_json::{json, Value};
use splinely_document::{Color, Event, Node, NodeKind, Project};
use splinely_runtime::{HostAction, Request, Runtime};
use std::collections::BTreeSet;

pub const FRAME_APPLIER_JS: &str = include_str!("frame_applier.js");
pub const RUST_HOST_JS: &str = include_str!("rust_host.js");
/// Fixed simulation step (seconds). Wall-clock time is converted to whole steps.
pub const FIXED_STEP: f64 = 1.0 / 60.0;
/// Upper bound for one page->host IPC message.
pub const MAX_IPC_BYTES: usize = 16 * 1024;
pub const MAX_INPUT_EVENTS: usize = 64;
pub const MAX_TARGET_BYTES: usize = 256;
pub const MAX_PAGE_ERROR_BYTES: usize = 512;
const MAX_POINTER: f64 = 1e6;
/// DOM events the JS runtime forwards (`eventTypes` in runtime/src/splinely.js) plus its drag events.
pub const INPUT_EVENT_NAMES: [&str; 17] = [
    "click",
    "dblclick",
    "pointerenter",
    "pointerleave",
    "pointerdown",
    "pointermove",
    "pointerup",
    "pointercancel",
    "lostpointercapture",
    "wheel",
    "keydown",
    "keyup",
    "focus",
    "blur",
    "dragstart",
    "drag",
    "dragend",
];

#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "camelCase", deny_unknown_fields)]
enum Ipc {
    Ready {},
    Ack {},
    Error {
        message: String,
    },
    Input {
        #[serde(default)]
        pointer: Option<Point>,
        #[serde(default)]
        leave: bool,
        #[serde(default)]
        events: Vec<InputEvent>,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Point {
    x: f64,
    y: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputEvent {
    name: String,
    #[serde(default)]
    target: Option<String>,
}

/// What a validated IPC message asked for.
#[derive(Debug, PartialEq)]
pub enum IpcOutcome {
    /// The page applied its first frame and listens for input.
    Ready,
    /// The page applied the last frame; the host may send the next one.
    Ack,
    /// Input was forwarded to the runtime (`dropped` events failed validation or execution).
    Input { forwarded: usize, dropped: usize },
    /// The page reported a (sanitised) problem.
    PageError(String),
}

/// A `splinely_runtime::Runtime` plus the frame diffing and IPC validation around it.
pub struct RustSession {
    runtime: Runtime,
    targets: BTreeSet<String>,
    sent: Vec<Node>,
    latest: Vec<Node>,
    halted: Option<String>,
    notes: Vec<String>,
    reported: usize,
    audio_ignored: u64,
    callbacks_ignored: u64,
    ticks: u64,
}

impl RustSession {
    pub fn new(project: &Project) -> Result<Self, String> {
        let targets = project
            .nodes
            .iter()
            .map(|n| n.id.clone())
            .chain(project.groups.iter().map(|g| g.id.clone()))
            .collect();
        let runtime = Runtime::new(project.clone()).map_err(|e| e.to_string())?;
        let mut session = Self {
            runtime,
            targets,
            sent: vec![],
            latest: vec![],
            halted: None,
            notes: vec![],
            reported: 0,
            audio_ignored: 0,
            callbacks_ignored: 0,
            ticks: 0,
        };
        // Drains the host output produced by initial state entries.
        session.execute(Request::Evaluate)?;
        Ok(session)
    }

    pub fn node_count(&self) -> usize {
        self.latest.len()
    }
    pub fn ticks(&self) -> u64 {
        self.ticks
    }
    /// Set after a failed tick; the simulation is stopped and the reason is shown.
    pub fn halted(&self) -> Option<&str> {
        self.halted.as_deref()
    }

    fn execute(&mut self, request: Request) -> Result<(), String> {
        let response = self.runtime.execute(request).map_err(|e| e.to_string())?;
        for action in &response.host_actions {
            match action {
                HostAction::PlayAudio { .. }
                | HostAction::AudioCue { .. }
                | HostAction::ClipAudioCue { .. } => {
                    self.audio_ignored += 1;
                    if self.audio_ignored == 1 {
                        self.notes.push("Audio request ignored: --runtime rust cannot play audio (use --runtime js).".into());
                    }
                }
                HostAction::Callback { .. } => {
                    self.callbacks_ignored += 1;
                    if self.callbacks_ignored == 1 {
                        self.notes.push(
                            "Callback request ignored: --runtime rust has no callback host.".into(),
                        );
                    }
                }
                HostAction::Event { .. } => {}
            }
        }
        self.latest = response.frame.nodes;
        Ok(())
    }

    /// Advance one fixed step. A failure halts the session (state is unchanged by the failed step).
    pub fn tick(&mut self) {
        if self.halted.is_some() {
            return;
        }
        match self.execute(Request::Tick { delta: FIXED_STEP }) {
            Ok(()) => self.ticks += 1,
            Err(error) => {
                let message = format!("Rust runtime stopped: {error}");
                self.notes.push(message.clone());
                self.halted = Some(message);
            }
        }
    }

    /// Validate and apply one page->host message. `Err` means it was rejected untouched.
    pub fn handle_ipc(&mut self, body: &str) -> Result<IpcOutcome, String> {
        if body.len() > MAX_IPC_BYTES {
            return Err(format!("IPC message exceeds {MAX_IPC_BYTES} bytes"));
        }
        let message: Ipc =
            serde_json::from_str(body).map_err(|e| format!("invalid IPC message: {e}"))?;
        match message {
            Ipc::Ready {} => Ok(IpcOutcome::Ready),
            Ipc::Ack {} => Ok(IpcOutcome::Ack),
            Ipc::Error { message } => {
                if message.len() > MAX_PAGE_ERROR_BYTES {
                    return Err("page error message too long".into());
                }
                Ok(IpcOutcome::PageError(
                    message.chars().filter(|c| !c.is_control()).collect(),
                ))
            }
            Ipc::Input {
                pointer,
                leave,
                events,
            } => {
                if events.len() > MAX_INPUT_EVENTS {
                    return Err(format!("more than {MAX_INPUT_EVENTS} input events"));
                }
                if leave && pointer.is_some() {
                    return Err("input has both pointer and leave".into());
                }
                if let Some(p) = &pointer {
                    if !(p.x.is_finite()
                        && p.y.is_finite()
                        && p.x.abs() <= MAX_POINTER
                        && p.y.abs() <= MAX_POINTER)
                    {
                        return Err("pointer coordinates out of range".into());
                    }
                }
                if self.halted.is_some() {
                    return Ok(IpcOutcome::Input {
                        forwarded: 0,
                        dropped: events.len(),
                    });
                }
                let (mut forwarded, mut dropped) = (0, 0);
                if let Some(p) = pointer {
                    self.forward(
                        Request::Pointer { x: p.x, y: p.y },
                        &mut forwarded,
                        &mut dropped,
                    );
                }
                if leave {
                    self.forward(Request::PointerLeave, &mut forwarded, &mut dropped);
                }
                for event in events {
                    let target = event.target.filter(|t| !t.is_empty());
                    let valid = INPUT_EVENT_NAMES.contains(&event.name.as_str())
                        && target.as_ref().is_none_or(|t| {
                            t.len() <= MAX_TARGET_BYTES && self.targets.contains(t)
                        });
                    if !valid {
                        dropped += 1;
                        continue;
                    }
                    let request = Request::Event {
                        event: Event {
                            name: event.name,
                            target,
                        },
                    };
                    self.forward(request, &mut forwarded, &mut dropped);
                }
                Ok(IpcOutcome::Input { forwarded, dropped })
            }
        }
    }

    /// Host-side event injection (not reachable from the page, so no name allow-list).
    pub fn send_event(&mut self, name: &str, target: Option<&str>) -> Result<(), String> {
        self.execute(Request::Event {
            event: Event {
                name: name.into(),
                target: target.map(Into::into),
            },
        })
    }

    fn forward(&mut self, request: Request, forwarded: &mut usize, dropped: &mut usize) {
        match self.execute(request) {
            Ok(()) => *forwarded += 1,
            Err(error) => {
                *dropped += 1;
                // Requests are atomic: the runtime state is unchanged, so keep running.
                if self.notes.len() < 20 {
                    self.notes
                        .push(format!("Rust runtime rejected input: {error}"));
                }
            }
        }
    }

    /// Diagnostics produced since the last call (shown in the page and on stderr).
    pub fn take_notes(&mut self) -> Vec<String> {
        let fresh = self.notes[self.reported..].to_vec();
        self.reported = self.notes.len();
        fresh
    }

    /// Full frame message (all nodes) for the page's first paint.
    pub fn full_message(&mut self) -> String {
        self.sent = self.latest.clone();
        message_json(self.latest.iter())
    }

    /// Message with only the nodes that changed since the last message, or `None`.
    pub fn frame_message(&mut self) -> Option<String> {
        let changed: Vec<&Node> = self
            .latest
            .iter()
            .enumerate()
            .filter(|(i, n)| self.sent.get(*i) != Some(*n))
            .map(|(_, n)| n)
            .collect();
        if changed.is_empty() {
            return None;
        }
        let message = message_json(changed.into_iter());
        self.sent = self.latest.clone();
        Some(message)
    }
}

fn color(c: Color) -> Value {
    json!([c.r, c.g, c.b, c.a])
}

/// Compact per-node record understood by `frame_applier.js`.
pub fn node_json(node: &Node) -> Value {
    let (t, s) = (node.transform, node.style);
    let mut out = json!({
        "id": node.id,
        "tf": [t.x, t.y, t.rotation, t.scale_x, t.scale_y],
        "op": s.opacity,
        "sw": s.stroke_width,
        "fill": color(s.fill),
        "stroke": s.stroke.map(color),
    });
    // Only the properties the JS runtime animates: width/height/r/font-size/d.
    out["geo"] = match &node.kind {
        NodeKind::Path { d, .. } => json!({ "d": d }),
        NodeKind::Rect { width, height, .. } => json!({ "width": width, "height": height }),
        NodeKind::Circle { radius, .. } => json!({ "r": radius }),
        NodeKind::Text { font_size, .. } => json!({ "fs": font_size }),
    };
    if let Some(g) = &node.gradient {
        out["grad"] = json!({
            "radial": g.radial,
            "angle": g.angle,
            "stops": g.stops.iter().map(|s| json!([s.offset, s.color.r, s.color.g, s.color.b, s.color.a])).collect::<Vec<_>>(),
        });
    }
    out
}

fn message_json<'a>(nodes: impl Iterator<Item = &'a Node>) -> String {
    json!({ "v": 1, "nodes": nodes.map(node_json).collect::<Vec<_>>() }).to_string()
}

/// Frame message for an arbitrary node list (used by tests and `--check`).
pub fn frame_json(nodes: &[Node]) -> String {
    message_json(nodes.iter())
}

/// Project features that rust mode cannot reproduce, one sentence each.
pub fn unsupported_features(project: &Project) -> Vec<String> {
    let mut out = vec![];
    if !project.custom_js.trim().is_empty() {
        out.push(format!(
            "customJs ({} bytes) is NOT executed in --runtime rust mode; scripted behaviour is missing. Use --runtime js --trusted-js to run it after review.",
            project.custom_js.len()
        ));
    }
    let clip_cues: usize = project.clips.iter().map(|c| c.audio_cues.len()).sum();
    if !project.audio_assets.is_empty() || !project.audio_cues.is_empty() || clip_cues > 0 {
        out.push(format!(
            "Audio is not supported in --runtime rust mode: {} asset(s) and {} cue(s) stay silent and there is no Start-audio control.",
            project.audio_assets.len(),
            project.audio_cues.len() + clip_cues
        ));
    }
    out
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// JSON that is safe inside an inline `<script>`.
fn script_json(value: &Value) -> String {
    value
        .to_string()
        .replace('<', "\\u003c")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// Pointer capture is only needed for projects whose machines react to drag events.
fn drag_config(project: &Project) -> Value {
    let mut all = false;
    let mut targets = BTreeSet::new();
    for transition in project.state_machines.iter().flat_map(|m| &m.transitions) {
        if transition.after.is_none()
            && matches!(
                transition.event.name.as_str(),
                "dragstart" | "drag" | "dragend"
            )
        {
            match &transition.event.target {
                Some(t) => {
                    targets.insert(t.clone());
                }
                None => all = true,
            }
        }
    }
    json!({ "dragAll": all, "dragTargets": targets })
}

/// Build the self-contained page: static SVG, applier, host glue and the first frame.
/// Without a native host (`window.ipc`) it simply shows that first frame.
pub fn wrap_rust_page(
    static_svg: &str,
    first_frame: &str,
    config: &Value,
    diagnostics: &[String],
) -> Result<String, Box<dyn std::error::Error>> {
    if static_svg.len() as u64 > MAX_INPUT_BYTES {
        return Err("SVG exceeds the 32 MiB playback limit".into());
    }
    let doc = roxmltree::Document::parse_with_options(
        static_svg,
        roxmltree::ParsingOptions {
            nodes_limit: MAX_XML_NODES,
            ..Default::default()
        },
    )
    .map_err(|e| format!("invalid SVG XML: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "svg"
        || root.tag_name().namespace() != Some("http://www.w3.org/2000/svg")
    {
        return Err("generated SVG has no SVG root".into());
    }
    let lines: String = diagnostics
        .iter()
        .map(|d| format!("<div>{}</div>", escape_html(d)))
        .collect();
    let hidden = if diagnostics.is_empty() {
        " hidden"
    } else {
        ""
    };
    let initial: Value = serde_json::from_str(first_frame)?;
    // No eval, no frames, no network: only the inline scripts below run.
    Ok(format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src 'none'; media-src 'none'; frame-src 'none'; connect-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'">
<title>Splinely Viewer</title><style>html,body{{margin:0;width:100%;height:100%;overflow:hidden;background:#20242b}}svg{{display:block;width:100%;height:100%}}#splinely-diagnostics{{position:fixed;left:0;right:0;top:0;padding:6px 10px;background:#7a2e00;color:#fff;font:12px/1.4 sans-serif;pointer-events:none;z-index:2}}#splinely-diagnostics[hidden]{{display:none}}</style></head>
<body>{static_svg}
<div id="splinely-diagnostics" role="status"{hidden}>{lines}</div>
<script>{FRAME_APPLIER_JS}</script>
<script>{RUST_HOST_JS}</script>
<script>(function(){{var svg=document.querySelector('svg');var post=window.ipc&&window.ipc.postMessage?function(m){{window.ipc.postMessage(m);}}:function(){{}};var applier=SplinelyFrameApplier.createFrameApplier(svg);var host=SplinelyRustHost.createRustHost({{window:window,document:document,svg:svg,applier:applier,post:post,config:{config}}});window.__splinelyFrame=host.onFrame;window.__splinelyDiag=host.diagnostic;host.start({initial});}})();</script>
</body></html>"#,
        config = script_json(config),
        initial = script_json(&initial),
    ))
}

/// Build the rust-mode `PreparedViewer`. `Err` is a reason to fall back to the JS runtime.
pub(crate) fn prepare_project(project: &Project) -> Result<PreparedViewer, String> {
    let mut session = RustSession::new(project)?;
    let static_svg = project.to_static_svg(0.0).map_err(|e| e.to_string())?;
    let first = session.full_message();
    let diagnostics = unsupported_features(project);
    let html = wrap_rust_page(&static_svg, &first, &drag_config(project), &diagnostics)
        .map_err(|e| e.to_string())?;
    Ok(PreparedViewer {
        html,
        title: project.name.clone(),
        is_svg_input: false,
        custom_js_disabled: !project.custom_js.trim().is_empty(),
        runtime: RuntimeMode::Rust,
        diagnostics,
        rust_session: Some(session),
    })
}
