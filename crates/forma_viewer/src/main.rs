use clap::Parser;
use forma_viewer::{launch_prepared, prepare, ViewerOptions, ViewerResult};
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

#[derive(Parser)]
#[command(
    name = "forma-viewer",
    version,
    about = "Native playback of trusted interactive SVGs and authored Forma projects"
)]
struct Args {
    /// Trusted SVG or authored Project path (.frm/.json detected automatically).
    input: PathBuf,
    /// Treat input as project JSON regardless of filename.
    #[arg(long)]
    project: bool,
    /// Execute reviewed project customJs (arbitrary code, not sandboxed).
    #[arg(long)]
    trusted_js: bool,
    /// Enable browser speech for project export.
    #[arg(long)]
    speech: bool,
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
            speech: args.speech,
        },
    )?;
    if prepared.is_svg_input {
        eprintln!("Warning: SVG is active content. Open only trusted SVGs; scripts may execute. This viewer is not a sandbox.");
    }
    if prepared.custom_js_disabled {
        eprintln!("Project customJs disabled; use --trusted-js only after reviewing the code.");
    }
    if let Some(output) = args.output {
        write_output(&args.input, &output, &prepared.html)
            .map_err(|e| format!("cannot write {}: {e}", output.display()))?;
    }
    if args.check {
        println!(
            "OK: {} (prepared {} bytes of HTML; no scripts executed)",
            args.input.display(),
            prepared.html.len()
        );
        return Ok(());
    }
    launch_prepared(prepared)
}

fn main() {
    if let Err(error) = run() {
        eprintln!("forma-viewer: {error}");
        std::process::exit(1);
    }
}
