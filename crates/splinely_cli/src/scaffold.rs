use crate::{atomic_write, failure, Result};
use clap::Subcommand;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const MANIFEST: &str = include_str!("../../../plugin-templates/editor/Cargo.toml.template");
const SOURCE: &str = include_str!("../../../plugin-templates/editor/src/lib.rs");
const README: &str = include_str!("../../../plugin-templates/editor/README.md");

#[derive(Subcommand)]
pub enum PluginCommand {
    /// Generate trusted static editor-only Rust source; never build or execute it.
    New {
        #[arg(long)]
        name: String,
        /// New directory; must not exist. Its parent must already exist.
        #[arg(long)]
        output: String,
        /// Path to the splinely_editor crate (defaults to this checkout).
        #[arg(long)]
        editor_path: Option<PathBuf>,
    },
    /// List drop-in plugins (editor tools and runtime part) in a directory; nothing is executed.
    List {
        /// Plugin directory (default: the editor's plugin folder, or SPLINELY_PLUGIN_DIR).
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Validate one plugin folder: manifest, behaviors and runtime file (size, UTF-8, lint, sha256).
    Validate {
        /// Plugin folder containing splinely-plugin.json.
        path: String,
    },
    /// Apply one plugin tool to a project via the editor's candidate path; validated, atomic.
    Apply {
        /// Plugin folder containing splinely-plugin.json.
        plugin: String,
        /// Tool or palette id inside the plugin.
        tool: String,
        /// Project file to edit (.sply or JSON; `-` reads stdin).
        #[arg(long)]
        project: String,
        /// Target node for tools that need one (`addBehavior`, `setStyle`).
        #[arg(long, alias = "id")]
        node: Option<String>,
        /// Where to save (default: overwrite --project).
        #[arg(long)]
        output: Option<String>,
    },
}

pub fn run(command: PluginCommand) -> Result<Value> {
    let PluginCommand::New {
        name,
        output,
        editor_path,
    } = command
    else {
        return Err(failure("invalid_arguments", "Not a scaffold command"));
    };
    if !name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        || !name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'-' | b'_'))
    {
        return Err(failure("invalid_arguments", "Plugin name must start with a lowercase ASCII letter and contain only lowercase letters, digits, '-' or '_'"));
    }
    if output == "-" {
        return Err(failure(
            "invalid_arguments",
            "Plugin output must be a new directory, not stdout",
        ));
    }
    let destination = Path::new(&output);
    // symlink_metadata also detects dangling symlinks; never overwrite authored source.
    match std::fs::symlink_metadata(destination) {
        Ok(_) => return Err(failure("write_error", "Plugin output already exists")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(failure("write_error", e)),
    }
    let editor = editor_path
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../splinely_editor"));
    let editor = editor
        .canonicalize()
        .map_err(|e| failure("read_error", e))?;
    if !editor.join("Cargo.toml").is_file() {
        return Err(failure(
            "invalid_arguments",
            "Editor path must contain Cargo.toml",
        ));
    }
    let editor = editor
        .to_str()
        .ok_or_else(|| failure("invalid_arguments", "Editor path must be UTF-8"))?;
    let manifest = MANIFEST
        .replace("{{NAME}}", &json!(name).to_string())
        .replace("{{EDITOR_PATH}}", &json!(editor).to_string());
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let staging = tempfile::tempdir_in(parent).map_err(|e| failure("write_error", e))?;
    std::fs::create_dir(staging.path().join("src")).map_err(|e| failure("write_error", e))?;
    for (path, source) in [
        ("Cargo.toml", manifest.as_str()),
        ("src/lib.rs", SOURCE),
        ("README.md", README),
    ] {
        let path = staging.path().join(path);
        let path = path
            .to_str()
            .ok_or_else(|| failure("write_error", "Output path must be UTF-8"))?;
        atomic_write(path, source.as_bytes())?;
    }
    // Reserve the destination so an existing file/directory can never be replaced.
    // Rename over our empty reservation publishes all generated files together.
    std::fs::create_dir(destination).map_err(|e| failure("write_error", e))?;
    if let Err(error) = std::fs::rename(staging.path(), destination) {
        let _ = std::fs::remove_dir(destination);
        return Err(failure("write_error", error));
    }
    Ok(
        json!({"ok":true,"output":output,"format":"rust-editor-plugin","name":name,
        "files":["Cargo.toml","src/lib.rs","README.md"],"editorOnly":true,"executed":false}),
    )
}
