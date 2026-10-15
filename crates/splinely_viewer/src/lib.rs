//! Native playback of trusted SVGs and validated Splinely projects.
use base64::{engine::general_purpose::STANDARD, Engine};
use splinely_document::{BrowserExportOptions, Project};
use std::{fs::File, io::Read, path::Path};

pub type ViewerResult<T> = Result<T, Box<dyn std::error::Error>>;
pub const MAX_INPUT_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_XML_NODES: u32 = 200_000;
const RUNTIME: &str = include_str!("../../../runtime/src/splinely.js");

#[derive(Clone, Copy, Debug, Default)]
pub struct ViewerOptions {
    /// Force JSON project input, regardless of extension.
    pub project: bool,
    /// Execute project customJs. Never needed to execute existing SVG scripts.
    pub trusted_js: bool,
}

pub struct PreparedViewer {
    pub html: String,
    pub title: String,
    pub is_svg_input: bool,
    pub custom_js_disabled: bool,
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
    let input = String::from_utf8(bytes).map_err(|e| format!("input must be UTF-8: {e}"))?;
    let project_input = options.project
        || path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("json") || s.eq_ignore_ascii_case("sply"));
    let (svg, title, custom_js_disabled) = if project_input {
        let project = Project::from_json(&input)?;
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

#[cfg(feature = "webview")]
pub fn launch_prepared(prepared: PreparedViewer) -> ViewerResult<()> {
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
    let mut event_loop = EventLoopBuilder::<String>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title(format!("Splinely Viewer — {}", prepared.title))
        .with_inner_size(tao::dpi::LogicalSize::new(1000.0, 720.0))
        .build(&event_loop)?;
    let builder = WebViewBuilder::new()
        .with_html(prepared.html)
        // No external navigation/popups; this is not a script sandbox.
        .with_navigation_handler(|url| {
            url == "about:blank" || url.starts_with("data:image/svg+xml;base64,")
        })
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny);
    #[cfg(target_os = "macos")]
    let builder = {
        use wry::WebViewBuilderExtDarwin;
        let proxy = event_loop.create_proxy();
        builder.with_on_web_content_process_terminate_handler(move || {
            let _ = proxy.send_event("native WebView renderer process terminated".into());
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
                let _ = proxy.send_event(format!(
                    "native WebView renderer process terminated: {reason:?}"
                ));
            });
    }
    let mut failure = None;
    let exit_code = event_loop.run_return(|event, _, control_flow| {
        let _keep_alive = (&window, &webview);
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::UserEvent(message) => {
                failure = Some(message);
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
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
