//! Native playback of trusted SVGs and validated Splinely projects.
use base64::{engine::general_purpose::STANDARD, Engine};
use splinely_document::{BrowserExportOptions, Project};
use std::{fs::File, io::Read, path::Path};

pub mod rust_mode;
pub use rust_mode::RustSession;

pub type ViewerResult<T> = Result<T, Box<dyn std::error::Error>>;
pub const MAX_INPUT_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_XML_NODES: u32 = 200_000;
const RUNTIME: &str = include_str!("../../../runtime/src/splinely.js");

/// Which runtime animates a project. `Js` is the default and unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RuntimeMode {
    /// `runtime/src/splinely.js` inside the webview.
    #[default]
    Js,
    /// `splinely_runtime` in this process; the webview only shows its frames.
    Rust,
}

impl RuntimeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Js => "js",
            Self::Rust => "rust",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ViewerOptions {
    /// Force JSON project input, regardless of extension.
    pub project: bool,
    /// Execute project customJs. Never needed to execute existing SVG scripts.
    pub trusted_js: bool,
    /// Runtime to use. `Rust` needs a project (not SVG) and rejects `trusted_js`.
    pub runtime: RuntimeMode,
}

pub struct PreparedViewer {
    pub html: String,
    pub title: String,
    pub is_svg_input: bool,
    pub custom_js_disabled: bool,
    /// Runtime actually used; `Js` after a fallback from a requested `Rust`.
    pub runtime: RuntimeMode,
    /// Things that are not reproduced (rust mode) or why a fallback happened; show them to the user.
    pub diagnostics: Vec<String>,
    /// Present in rust mode only; owned by the event loop in `launch_prepared`.
    pub rust_session: Option<RustSession>,
}

/// Read at most 32 MiB, validate, and produce a portable HTML playback wrapper.
/// Does not execute JavaScript or initialize a GUI.
pub fn prepare(path: &Path, options: ViewerOptions) -> ViewerResult<PreparedViewer> {
    let file = File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(format!("{} exceeds the 32 MiB input limit", path.display()).into());
    }
    // Binary `.sply` is recognised by its magic bytes, whatever the extension.
    let binary = Project::is_binary(&bytes);
    let project_input = options.project
        || binary
        || path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("json") || s.eq_ignore_ascii_case("sply"));
    let rust = options.runtime == RuntimeMode::Rust;
    if rust && !project_input {
        return Err("--runtime rust needs a Splinely project (.sply or .json); an SVG has no project to evaluate. Use the default --runtime js for SVG files.".into());
    }
    if rust && options.trusted_js {
        return Err("--trusted-js cannot be combined with --runtime rust: customJs is never executed in rust mode".into());
    }
    let mut diagnostics = vec![];
    let (svg, title, custom_js_disabled) = if project_input {
        // Sniffs binary vs JSON text; the same strict validation applies to both.
        let project = Project::from_bytes(&bytes)?;
        if rust {
            match rust_mode::prepare_project(&project) {
                Ok(prepared) => return Ok(prepared),
                Err(reason) => diagnostics.push(format!(
                    "--runtime rust is unavailable for this project ({reason}); falling back to the JS runtime."
                )),
            }
        }
        let disabled = !options.trusted_js && !project.custom_js.is_empty();
        let svg = project.to_svg_with_options(
            RUNTIME,
            BrowserExportOptions {
                trusted_js: options.trusted_js,
                audio_controls: true,
            },
        )?;
        (svg, project.name, disabled)
    } else {
        let input = String::from_utf8(bytes).map_err(|e| format!("input must be UTF-8: {e}"))?;
        (
            input,
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            false,
        )
    };
    let html = wrap_svg(&svg)?;
    Ok(PreparedViewer {
        html,
        title,
        is_svg_input: !project_input,
        custom_js_disabled,
        runtime: RuntimeMode::Js,
        diagnostics,
        rust_session: None,
    })
}

/// Preserve XML script/CDATA semantics by loading SVG as its own document, not
/// inserting XML into the HTML parser. CSS overrides authored root dimensions.
pub fn wrap_svg(svg: &str) -> ViewerResult<String> {
    if svg.len() as u64 > MAX_INPUT_BYTES {
        return Err("SVG exceeds the 32 MiB playback limit".into());
    }
    let doc = roxmltree::Document::parse_with_options(
        svg,
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
        return Err("input must have an SVG root in the SVG namespace".into());
    }
    // Insert a namespaced style before the closing root, including prefixed roots.
    let end = root.range().end;
    let close = svg[..end].rfind("</");
    let mut responsive = svg.to_owned();
    let style = "<style xmlns=\"http://www.w3.org/2000/svg\">:root{width:100% !important;height:100% !important}</style>";
    if root.children().next().is_some() || !svg[..end].trim_end().ends_with("/>") {
        responsive.insert_str(close.ok_or("missing SVG closing tag")?, style);
    } // An empty self-closing SVG needs no playback resizing.
    let encoded = STANDARD.encode(responsive);
    Ok(format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline' 'unsafe-eval'; style-src 'unsafe-inline'; img-src data: blob:; media-src data: blob:; frame-src data:; connect-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'">
<title>Splinely Viewer</title><style>html,body{{margin:0;width:100%;height:100%;overflow:hidden;background:#20242b}}iframe{{border:0;width:100%;height:100%;display:block}}</style></head>
<body><iframe title="Splinely SVG playback" src="data:image/svg+xml;base64,{encoded}"></iframe></body></html>"#
    ))
}

/// Launch on the main thread; return after close or a reported renderer failure.
pub fn launch(path: &Path, options: ViewerOptions) -> ViewerResult<()> {
    launch_prepared(prepare(path, options)?)
}

/// Messages posted to the event loop from webview callbacks.
#[cfg(feature = "webview")]
enum UserEvent {
    // Only sent by the Linux and macOS renderer-termination handlers.
    #[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
    Failure(String),
    /// Raw page->host IPC body (size-checked before it is queued).
    Ipc(String),
}

#[cfg(feature = "webview")]
pub fn launch_prepared(mut prepared: PreparedViewer) -> ViewerResult<()> {
    use std::time::{Duration, Instant};
    use tao::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoopBuilder},
        platform::run_return::EventLoopExtRunReturn,
        window::WindowBuilder,
    };
    use wry::{NewWindowResponse, WebViewBuilder};
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY")
        .filter(|v| !v.is_empty())
        .is_none()
        && std::env::var_os("WAYLAND_DISPLAY")
            .filter(|v| !v.is_empty())
            .is_none()
    {
        return Err("native viewer requires DISPLAY or WAYLAND_DISPLAY; use --check for headless validation".into());
    }
    let mut session = prepared.rust_session.take();
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title(format!("Splinely Viewer — {}", prepared.title))
        .with_inner_size(tao::dpi::LogicalSize::new(1000.0, 720.0))
        .build(&event_loop)?;
    let rust_mode = session.is_some();
    let builder = WebViewBuilder::new()
        .with_html(prepared.html)
        // No external navigation/popups; this is not a script sandbox. The rust-mode
        // page is self-contained and never needs a data: document.
        .with_navigation_handler(move |url| {
            url == "about:blank" || (!rust_mode && url.starts_with("data:image/svg+xml;base64,"))
        })
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny);
    // The only capability the page gains in rust mode: a size-limited IPC string channel.
    let builder = if rust_mode {
        let proxy = event_loop.create_proxy();
        builder.with_ipc_handler(move |request| {
            let body = request.body();
            if body.len() <= rust_mode::MAX_IPC_BYTES {
                let _ = proxy.send_event(UserEvent::Ipc(body.clone()));
            } else {
                eprintln!(
                    "splinely-viewer: dropped oversized IPC message ({} bytes)",
                    body.len()
                );
            }
        })
    } else {
        builder
    };
    #[cfg(target_os = "macos")]
    let builder = {
        use wry::WebViewBuilderExtDarwin;
        let proxy = event_loop.create_proxy();
        builder.with_on_web_content_process_terminate_handler(move || {
            let _ = proxy.send_event(UserEvent::Failure(
                "native WebView renderer process terminated".into(),
            ));
        })
    };
    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(
            window
                .default_vbox()
                .ok_or("native GTK container unavailable")?,
        )?
    };
    #[cfg(not(target_os = "linux"))]
    let webview = builder.build(&window)?;
    #[cfg(target_os = "linux")]
    {
        use webkit2gtk::WebViewExt;
        use wry::WebViewExtUnix;
        let proxy = event_loop.create_proxy();
        webview
            .webview()
            .connect_web_process_terminated(move |_, reason| {
                let _ = proxy.send_event(UserEvent::Failure(format!(
                    "native WebView renderer process terminated: {reason:?}"
                )));
            });
    }
    let step = Duration::from_secs_f64(rust_mode::FIXED_STEP);
    // Rust mode: ticking starts when the page reports `ready`; one frame in flight at a time.
    let (mut ready, mut in_flight, mut next_tick) = (false, false, Instant::now());
    let mut rejected = 0u32;
    let show = |webview: &wry::WebView, notes: Vec<String>| {
        for note in notes {
            eprintln!("splinely-viewer: {note}");
            let literal = serde_json::Value::String(note)
                .to_string()
                .replace('<', "\\u003c");
            let _ = webview.evaluate_script(&format!(
                "window.__splinelyDiag&&window.__splinelyDiag({literal})"
            ));
        }
    };
    let mut failure = None;
    let exit_code = event_loop.run_return(|event, _, control_flow| {
        let _keep_alive = (&window, &webview);
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::UserEvent(UserEvent::Failure(message)) => {
                failure = Some(message);
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(UserEvent::Ipc(body)) => {
                if let Some(session) = session.as_mut() {
                    match session.handle_ipc(&body) {
                        Ok(rust_mode::IpcOutcome::Ready) => {
                            ready = true;
                            next_tick = Instant::now() + step;
                        }
                        Ok(rust_mode::IpcOutcome::Ack) => in_flight = false,
                        Ok(rust_mode::IpcOutcome::Input { .. }) => {}
                        Ok(rust_mode::IpcOutcome::PageError(message)) => {
                            eprintln!("splinely-viewer: page: {message}")
                        }
                        Err(reason) => {
                            // Malformed/oversized/unknown messages are ignored; log a few.
                            rejected += 1;
                            if rejected <= 10 {
                                eprintln!("splinely-viewer: rejected IPC message: {reason}");
                            }
                        }
                    }
                    show(&webview, session.take_notes());
                }
            }
            _ => {}
        }
        if let (Some(session), true) = (session.as_mut(), ready) {
            // Whole fixed steps for the elapsed wall-clock time; a long stall drops the backlog.
            let now = Instant::now();
            let mut steps = 0;
            while now >= next_tick && steps < 5 {
                session.tick();
                next_tick += step;
                steps += 1;
            }
            if now >= next_tick {
                next_tick = now + step;
            }
            if !in_flight {
                if let Some(message) = session.frame_message() {
                    if webview
                        .evaluate_script(&format!("window.__splinelyFrame({message})"))
                        .is_ok()
                    {
                        in_flight = true;
                    }
                }
            }
            show(&webview, session.take_notes());
            if *control_flow != ControlFlow::Exit && session.halted().is_none() {
                *control_flow = ControlFlow::WaitUntil(next_tick);
            }
        }
    });
    if let Some(message) = failure {
        return Err(message.into());
    }
    if exit_code != 0 {
        return Err(format!("native event loop exited with status {exit_code}").into());
    }
    Ok(())
}

#[cfg(not(feature = "webview"))]
pub fn launch_prepared(_prepared: PreparedViewer) -> ViewerResult<()> {
    Err("native viewer unavailable: rebuild with the webview feature (enabled by default)".into())
}
