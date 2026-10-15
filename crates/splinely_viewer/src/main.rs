use clap::{Parser, ValueEnum};
use splinely_viewer::{launch_prepared, prepare, RuntimeMode, ViewerOptions, ViewerResult};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

fn write_output(input: &Path, output: &Path, html: &str) -> ViewerResult<()> {
    if output == input || (output.exists() && same_file::is_same_file(input, output)?) {
        return Err("--output must not overwrite the input (including hard-link aliases)".into());
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(html.as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(output)
        .map_err(|e| format!("cannot replace {}: {e}", output.display()))?;
    Ok(())
}

#[derive(Clone, Copy, ValueEnum)]
enum RuntimeArg {
    /// The JavaScript runtime inside the webview (default).
    Js,
    /// The Rust runtime (splinely_runtime) linked into this process; the webview only shows its frames.
    Rust,
}

#[derive(Parser)]
#[command(
    name = "splinely-viewer",
    version,
    about = "Native playback of trusted interactive SVGs and authored Splinely projects",
    after_help = "RUNTIMES:\n  --runtime js    (default) the JavaScript runtime runs in the webview; SVG and project input.\n  --runtime rust  the viewer starts the Rust runtime in-process and shows its frames; project input\n                  only. customJs is NOT executed and audio is NOT played (a diagnostic is shown);\n                  --trusted-js is rejected. If the Rust runtime cannot load the project, the viewer\n                  reports why and falls back to the JS runtime."
)]
struct Args {
    /// Trusted SVG or authored Project path (binary or JSON .sply/.json detected automatically).
    input: PathBuf,
    /// Treat input as project JSON regardless of filename.
    #[arg(long)]
    project: bool,
    /// Execute reviewed project customJs (arbitrary code, not sandboxed). Not available with --runtime rust.
    #[arg(long)]
    trusted_js: bool,
    /// Runtime that animates the project: `js` (default) or `rust` (in-process splinely_runtime, projects only).
    #[arg(long, value_enum, default_value = "js", value_name = "RUNTIME")]
    runtime: RuntimeArg,
    /// Validate and prepare without opening a GUI or executing JavaScript.
    #[arg(long)]
    check: bool,
    /// Write the self-contained HTML wrapper (also works with --check).
    #[arg(long, value_name = "HTML")]
    output: Option<PathBuf>,
}

fn run() -> ViewerResult<()> {
    let args = Args::parse();
    let prepared = prepare(
        &args.input,
        ViewerOptions {
            project: args.project,
            trusted_js: args.trusted_js,
            runtime: match args.runtime {
                RuntimeArg::Js => RuntimeMode::Js,
                RuntimeArg::Rust => RuntimeMode::Rust,
            },
        },
    )?;
    for diagnostic in &prepared.diagnostics {
        eprintln!("splinely-viewer: {diagnostic}");
    }
    if prepared.is_svg_input {
        eprintln!("Warning: SVG is active content. Open only trusted SVGs; scripts may execute. This viewer is not a sandbox.");
    }
    if prepared.custom_js_disabled && prepared.runtime == RuntimeMode::Js {
        eprintln!("Project customJs disabled; use --trusted-js only after reviewing the code.");
    }
    if let Some(output) = args.output {
        write_output(&args.input, &output, &prepared.html)
            .map_err(|e| format!("cannot write {}: {e}", output.display()))?;
    }
    if args.check {
        println!(
            "OK: {} (prepared {} bytes of HTML; no scripts executed; runtime: {}{})",
            args.input.display(),
            prepared.html.len(),
            prepared.runtime.as_str(),
            prepared
                .rust_session
                .as_ref()
                .map(|s| format!(", {} nodes evaluated by splinely_runtime", s.node_count()))
                .unwrap_or_default()
        );
        return Ok(());
    }
    launch_prepared(prepared)
}

fn main() {
    splinely_document::crash::install(
        "viewer",
        env!("CARGO_PKG_VERSION"),
        || "en".to_owned(),
        true,
    );
    if let Err(error) = run() {
        eprintln!("splinely-viewer: {error}");
        std::process::exit(1);
    }
}
