//! Drop-in plugins: a declarative editor part (tools/palette/behaviors, pure project data) and an
//! optional, never auto-loaded browser-runtime part (a hash-pinned JS file embedded only at an
//! explicitly trusted export). Discovery is restart-based; no code is executed here and native
//! libraries are never loaded. See docs/plugins.md and docs/drop-in-plugins.md.
//!
//! This module is GUI-free so the editor (egui sidebar) and `splinely-cli plugin ...` share the
//! exact same manifest validation and candidate/apply path.
use crate::{
    AnimValue, Clip, Color, Easing, Event, Keyframe, Node, NodeKind, Project, Property, State,
    StateMachine, Track, Transition,
};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const MAX_ENTRIES: usize = 128;
const MAX_MANIFEST: usize = 64 * 1024;
const MAX_TOOLS: usize = 64;
const MAX_ACTIONS: usize = 1024;
const MAX_OUTPUT: usize = 8 * 1024 * 1024;
const MAX_NODES: usize = 10_000;
/// Largest runtime entry file (UTF-8 JavaScript).
pub const MAX_RUNTIME_ENTRY: usize = 256 * 1024;
/// Runtime plugin API version understood by `runtime/src/splinely.js` (`PLUGIN_API_VERSION`).
pub const RUNTIME_API_VERSION: u32 = 1;
/// Plugin runtime files embedded into one export.
pub const MAX_EXPORT_PLUGINS: usize = 8;
// Bounds of one `addBehavior` merge.
const MAX_BEHAVIOR_TRACKS: usize = 8;
const MAX_BEHAVIOR_KEYFRAMES: usize = 32;
const MAX_BEHAVIOR_DURATION: f64 = 30.0;
const MAX_BEHAVIOR_VARIABLES: usize = 8;
const MAX_PROJECT_CLIPS: usize = 256;
const MAX_PROJECT_MACHINES: usize = 256;
/// DOM events a declarative behavior may be bound to (node-targeted pointer events).
pub const BEHAVIOR_EVENTS: &[&str] = &[
    "click",
    "dblclick",
    "pointerenter",
    "pointerleave",
    "pointerdown",
    "pointerup",
];

type Result<T> = std::result::Result<T, String>;

/// Why a candidate could not be produced. The editor translates the first four variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateError {
    UnknownPlugin,
    UnknownTool,
    NeedSelection,
    SelectionMissing,
    SelectionLocked,
    /// `Project::validate()` rejected the result (nothing was changed).
    Rejected(String),
    Other(String),
}
impl std::fmt::Display for CandidateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPlugin => f.write_str("Unknown plugin"),
            Self::UnknownTool => f.write_str("Unknown plugin tool"),
            Self::NeedSelection => f.write_str("Select a shape first"),
            Self::SelectionMissing => f.write_str("The selected shape no longer exists"),
            Self::SelectionLocked => f.write_str("The selected shape is locked"),
            Self::Rejected(e) => write!(f, "Plugin result rejected: {e}"),
            Self::Other(e) => f.write_str(e),
        }
    }
}
impl std::error::Error for CandidateError {}
impl From<String> for CandidateError {
    fn from(e: String) -> Self {
        Self::Other(e)
    }
}
impl From<&str> for CandidateError {
    fn from(e: &str) -> Self {
        Self::Other(e.into())
    }
}

#[derive(Clone, Debug)]
pub struct ExtensionRegistry {
    pub directory: Option<PathBuf>,
    /// Discovery failures are retained even when other plugins load successfully.
    pub diagnostics: Vec<String>,
    plugins: Vec<Extension>,
}
#[derive(Clone, Debug)]
pub struct Extension {
    pub id: String,
    pub title: String,
    pub tools: Vec<ExtensionTool>,
    pub palette: Vec<ExtensionTool>,
    /// Optional browser-runtime part. Never executed by the editor/CLI; see `RuntimePart`.
    pub runtime: Option<RuntimePart>,
}
/// The browser-runtime half of a plugin: one JavaScript file next to the manifest.
/// Read, size/UTF-8/lint checked and hashed at discovery; embedded only into an export that
/// carries the explicit trust flag (`--trusted-js`).
#[derive(Clone, Debug)]
pub struct RuntimePart {
    pub api_version: u32,
    /// File name inside the plugin folder (single path segment, `*.js`).
    pub entry: String,
    pub description: String,
    /// Size in bytes of the entry file.
    pub bytes: usize,
    /// `sha256:<64 hex>` of the exact entry file bytes (empty until the file is read).
    pub sha256: String,
    source: String,
}
impl RuntimePart {
    pub fn source(&self) -> &str {
        &self.source
    }
}
#[derive(Clone, Debug)]
pub struct ExtensionTool {
    pub id: String,
    pub title: String,
    action: Action,
}
#[derive(Clone, Debug)]
enum Action {
    Add(Box<Node>),
    Style(StylePatch),
    Behavior(Box<BehaviorSpec>),
}
/// Declarative behavior bound to the selected node at apply time.
#[derive(Clone, Debug)]
struct BehaviorSpec {
    title: String,
    event: String,
    duration: f64,
    tracks: Vec<(Property, Vec<Keyframe>)>,
    variables: BTreeMap<String, Value>,
    label: Option<String>,
}
#[derive(Clone, Debug, Default)]
struct StylePatch {
    fill: Option<Color>,
    stroke: Option<Option<Color>>,
    stroke_width: Option<f64>,
    opacity: Option<f64>,
}
impl StylePatch {
    fn apply(&self, node: &mut Node) {
        if let Some(v) = self.fill {
            node.style.fill = v;
        }
        if let Some(v) = self.stroke {
            node.style.stroke = v;
        }
        if let Some(v) = self.stroke_width {
            node.style.stroke_width = v;
        }
        if let Some(v) = self.opacity {
            node.style.opacity = v;
        }
    }
}

impl ExtensionRegistry {
    /// Call once at editor startup. Missing default folders are an empty registry.
    pub fn load_default() -> Self {
        let directory = default_directory();
        match directory {
            Some(path) => Self::load_directory(path),
            None => Self {
                directory: None,
                diagnostics: vec![
                    "Plugins: no SPLINELY_PLUGIN_DIR or per-user config directory (XDG_CONFIG_HOME/HOME, %APPDATA% or ~/Library) found".into(),
                ],
                plugins: vec![],
            },
        }
    }

    /// Explicit directory API also permits testing without mutating process env.
    pub fn load_directory(directory: impl Into<PathBuf>) -> Self {
        let directory = directory.into();
        let mut registry = Self {
            directory: Some(directory.clone()),
            diagnostics: vec![],
            plugins: vec![],
        };
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return registry,
            Err(e) => {
                registry
                    .diagnostics
                    .push(format!("Cannot read plugin directory: {e}"));
                return registry;
            }
        };
        // Never choose a filesystem-order-dependent subset of an oversized folder.
        let mut paths = Vec::new();
        for entry in entries.take(MAX_ENTRIES + 1) {
            match entry {
                Ok(entry) => paths.push(entry.path()),
                Err(e) => {
                    registry
                        .diagnostics
                        .push(format!("Cannot enumerate plugins: {e}"));
                    return registry;
                }
            }
        }
        if paths.len() > MAX_ENTRIES {
            registry.diagnostics.push(format!(
                "Plugin directory exceeds {MAX_ENTRIES} entries; nothing loaded"
            ));
            return registry;
        }
        paths.sort();
        let mut parsed = Vec::new();
        for path in paths {
            let label: String = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .chars()
                .take(160)
                .collect();
            match read_plugin(&path) {
                Ok(Some(plugin)) => parsed.push(plugin),
                Ok(None) => (),
                Err(e) => registry.diagnostics.push(format!("{label}: {e}")),
            }
        }
        let mut counts = BTreeMap::new();
        for plugin in &parsed {
            *counts.entry(plugin.id.clone()).or_insert(0usize) += 1;
        }
        for (id, count) in &counts {
            if *count > 1 {
                registry
                    .diagnostics
                    .push(format!("Duplicate plugin ID {id}; all copies rejected"));
            }
        }
        parsed.retain(|p| counts[&p.id] == 1);
        parsed.sort_by(|a, b| a.id.cmp(&b.id));
        if parsed
            .iter()
            .map(|p| p.tools.len() + p.palette.len())
            .sum::<usize>()
            > MAX_ACTIONS
        {
            registry.diagnostics.push(format!(
                "Registry exceeds {MAX_ACTIONS} actions; nothing loaded"
            ));
        } else {
            registry.plugins = parsed;
        }
        registry
    }

    pub fn plugins(&self) -> &[Extension] {
        &self.plugins
    }

    /// Returns a validated, atomic replacement; the input is never mutated.
    /// The host must commit through its normal history/dirty-state mechanism.
    pub fn candidate(
        &self,
        plugin_id: &str,
        tool_id: &str,
        project: &Project,
        selected: Option<&str>,
    ) -> std::result::Result<Project, CandidateError> {
        let plugin = self
            .plugins
            .iter()
            .find(|p| p.id == plugin_id)
            .ok_or(CandidateError::UnknownPlugin)?;
        let tool = plugin
            .tools
            .iter()
            .chain(&plugin.palette)
            .find(|t| t.id == tool_id)
            .ok_or(CandidateError::UnknownTool)?;
        if project.nodes.len() > MAX_NODES {
            return Err("Plugin output node limit exceeded".into());
        }
        check_output_size(project)?;
        let mut next = project.clone();
        match &tool.action {
            Action::Add(template) => {
                if next.nodes.len() >= MAX_NODES {
                    return Err("Plugin output node limit exceeded".into());
                }
                let mut node = (**template).clone();
                node.id = fresh_id(project)?;
                next.nodes.push(node);
            }
            Action::Style(style) => {
                let node = editable_selection(&mut next, selected)?;
                style.apply(node);
            }
            Action::Behavior(spec) => apply_behavior(spec, &tool.id, &mut next, selected)?,
        }
        next.validate()
            .map_err(|e| CandidateError::Rejected(e.to_string()))?;
        check_output_size(&next)?;
        Ok(next)
    }

    /// Whether a tool needs an unlocked selected node (the editor disables its button otherwise).
    pub fn needs_selection(tool: &ExtensionTool) -> bool {
        matches!(tool.action, Action::Style(_) | Action::Behavior(_))
    }

    /// Reads and validates ONE plugin folder (the CLI's `plugin validate`/`apply` path).
    /// Unlike discovery, a missing manifest or a non-directory is an error.
    pub fn load_plugin(path: impl AsRef<Path>) -> Result<Extension> {
        let path = path.as_ref();
        let meta = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if !meta.is_dir() {
            return Err(format!("{} is not a plugin directory", path.display()));
        }
        read_plugin(path)?.ok_or_else(|| "Not a plugin directory".into())
    }

    /// A registry holding exactly one already-validated plugin.
    pub fn single(plugin: Extension) -> Self {
        Self {
            directory: None,
            diagnostics: vec![],
            plugins: vec![plugin],
        }
    }
}

fn editable_selection<'a>(
    project: &'a mut Project,
    selected: Option<&str>,
) -> std::result::Result<&'a mut Node, CandidateError> {
    let selected = selected.ok_or(CandidateError::NeedSelection)?;
    let node = project
        .nodes
        .iter_mut()
        .find(|n| n.id == selected)
        .ok_or(CandidateError::SelectionMissing)?;
    if node.locked {
        return Err(CandidateError::SelectionLocked);
    }
    Ok(node)
}

fn default_directory() -> Option<PathBuf> {
    fn env_path(name: &str) -> Option<PathBuf> {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }
    env_path("SPLINELY_PLUGIN_DIR").or_else(|| {
        // XDG on Linux, %APPDATA% on Windows, ~/Library/Application Support on macOS.
        crate::platform_dirs::base_dir(crate::platform_dirs::Kind::Config)
            .map(|p| p.join("splinely").join("plugins"))
    })
}

fn read_plugin(path: &Path) -> Result<Option<Extension>> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() {
        return Err("Symlink plugin entries are not allowed".into());
    }
    if !meta.is_dir() {
        return Ok(None);
    }
    let manifest = path.join("splinely-plugin.json");
    let meta = fs::symlink_metadata(&manifest)
        .map_err(|e| format!("Cannot read splinely-plugin.json: {e}"))?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("Manifest must be a regular, non-symlink file".into());
    }
    if meta.len() > MAX_MANIFEST as u64 {
        return Err("Manifest exceeds 65536 bytes".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(manifest)
        .map_err(|e| e.to_string())?
        .take((MAX_MANIFEST + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_MANIFEST {
        return Err("Manifest exceeds 65536 bytes".into());
    }
    let mut plugin = parse_manifest(&bytes)?;
    if let Some(runtime) = &mut plugin.runtime {
        read_runtime_entry(path, runtime)?;
    }
    Ok(Some(plugin))
}

/// Reads `runtime.entry` next to the manifest: regular non-symlink file, bounded read, UTF-8,
/// best-effort lint, SHA-256 of the exact bytes.
fn read_runtime_entry(dir: &Path, runtime: &mut RuntimePart) -> Result<()> {
    let file = dir.join(&runtime.entry);
    let meta = fs::symlink_metadata(&file)
        .map_err(|e| format!("Cannot read runtime entry {}: {e}", runtime.entry))?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("Runtime entry must be a regular, non-symlink file".into());
    }
    if meta.len() > MAX_RUNTIME_ENTRY as u64 {
        return Err(format!("Runtime entry exceeds {MAX_RUNTIME_ENTRY} bytes"));
    }
    let mut bytes = Vec::new();
    fs::File::open(&file)
        .map_err(|e| e.to_string())?
        .take((MAX_RUNTIME_ENTRY + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_RUNTIME_ENTRY {
        return Err(format!("Runtime entry exceeds {MAX_RUNTIME_ENTRY} bytes"));
    }
    runtime.sha256 = sha256_hex(&bytes);
    runtime.bytes = bytes.len();
    runtime.source = check_runtime_source(&bytes)?;
    Ok(())
}

/// UTF-8, no NUL/control characters other than TAB/LF/CR, and a best-effort scan for network,
/// dynamic-code and import tokens. **This is a lint, not a sandbox**: trivially bypassable
/// (`globalThis["fe"+"tch"]`). Trusted exports run the file with full page privileges.
pub fn check_runtime_source(bytes: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "Runtime entry must be valid UTF-8")?;
    if text.trim().is_empty() {
        return Err("Runtime entry is empty".into());
    }
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
    {
        return Err("Runtime entry contains control characters".into());
    }
    for token in RUNTIME_FORBIDDEN {
        if text.contains(token) {
            return Err(format!(
                "Runtime entry mentions {token:?} (network, dynamic code and imports are not allowed; best-effort check, not a sandbox)"
            ));
        }
    }
    Ok(text.to_owned())
}
/// Substring lint list for runtime entries (case-sensitive).
const RUNTIME_FORBIDDEN: &[&str] = &[
    "http:",
    "https:",
    "ws://",
    "wss://",
    "fetch(",
    "XMLHttpRequest",
    "WebSocket",
    "EventSource",
    "sendBeacon",
    "importScripts",
    "import(",
    "import.meta",
    "eval(",
    "Function(",
    "</script",
    "<!--",
];

/// One plugin runtime file ready to be embedded into a trusted export. Obtained only from a
/// validated plugin folder or an explicit `.js` path (never from project data).
#[derive(Clone, Debug)]
pub struct RuntimeBundle {
    /// Plugin id (also the runtime `plugin.id` the file must declare).
    pub id: String,
    pub entry: String,
    pub api_version: u32,
    pub bytes: usize,
    pub sha256: String,
    source: String,
}
impl RuntimeBundle {
    pub fn source(&self) -> &str {
        &self.source
    }
}
impl Extension {
    /// The runtime part as an export bundle (`None` for editor-only plugins).
    pub fn runtime_bundle(&self) -> Option<RuntimeBundle> {
        self.runtime.as_ref().map(|r| RuntimeBundle {
            id: self.id.clone(),
            entry: r.entry.clone(),
            api_version: r.api_version,
            bytes: r.bytes,
            sha256: r.sha256.clone(),
            source: r.source.clone(),
        })
    }
}
/// Loads the runtime file named by `--plugin`: a plugin folder (its manifest's `runtime.entry`)
/// or a bare `*.js` file (same size/UTF-8/lint checks; id = sanitized file stem).
pub fn load_runtime_bundle(path: impl AsRef<Path>) -> Result<RuntimeBundle> {
    let path = path.as_ref();
    let meta = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.file_type().is_symlink() {
        return Err("Symlink plugin paths are not allowed".into());
    }
    if meta.is_dir() {
        let plugin = ExtensionRegistry::load_plugin(path)?;
        return plugin
            .runtime_bundle()
            .ok_or_else(|| format!("Plugin {} has no runtime part", plugin.id));
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| n.ends_with(".js"))
        .ok_or("Runtime plugin must be a plugin folder or a .js file")?;
    if name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        return Err("Runtime file name must be <=64 chars of [A-Za-z0-9_.-]".into());
    }
    let stem = name.trim_end_matches(".js");
    let mut id: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    if !id.starts_with(|c: char| c.is_ascii_alphabetic()) {
        id.insert_str(0, "plugin-");
    }
    id.truncate(64);
    let mut runtime = RuntimePart {
        api_version: RUNTIME_API_VERSION,
        entry: name.to_owned(),
        description: String::new(),
        bytes: 0,
        sha256: String::new(),
        source: String::new(),
    };
    read_runtime_entry(
        path.parent().unwrap_or_else(|| Path::new(".")),
        &mut runtime,
    )?;
    Ok(RuntimeBundle {
        id,
        entry: runtime.entry,
        api_version: runtime.api_version,
        bytes: runtime.bytes,
        sha256: runtime.sha256,
        source: runtime.source,
    })
}

/// `<` `>` `&` and line separators as `\u` escapes so the JSON is safe in XML text and in
/// JavaScript source.
fn json_xml_safe(value: &Value) -> String {
    serde_json::to_string(value)
        .unwrap_or_default()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
        .replace('\u{fffe}', "\\ufffe")
        .replace('\u{ffff}', "\\uffff")
}

impl Project {
    /// Trusted export with hash-pinned runtime plugins. Refuses unless `options.trusted_js` is
    /// set: embedded plugin code has the same privileges and the same explicit trust gate as
    /// `customJs`. Writes `<metadata id="splinely-plugins">` (id, entry, apiVersion, bytes,
    /// sha256) so viewers and the isolated preview can show exactly what was embedded.
    pub fn to_svg_with_plugins(
        &self,
        runtime: &str,
        options: crate::BrowserExportOptions,
        embed: crate::EmbedFormat,
        plugins: &[RuntimeBundle],
    ) -> std::result::Result<String, crate::DocumentError> {
        if plugins.is_empty() {
            return self.to_svg_with_options_embed(runtime, options, embed);
        }
        if !options.trusted_js {
            return Err(crate::DocumentError(
                "runtime plugins are executable code and require the explicit trusted_js option"
                    .into(),
            ));
        }
        if plugins.len() > MAX_EXPORT_PLUGINS {
            return Err(crate::DocumentError(format!(
                "at most {MAX_EXPORT_PLUGINS} runtime plugins per export"
            )));
        }
        let mut seen = HashSet::new();
        for p in plugins {
            if !seen.insert(p.id.as_str()) {
                return Err(crate::DocumentError(format!(
                    "duplicate runtime plugin id {}",
                    p.id
                )));
            }
        }
        let meta: Vec<Value> = plugins
            .iter()
            .map(|p| {
                serde_json::json!({"id":p.id,"entry":p.entry,"apiVersion":p.api_version,"bytes":p.bytes,"sha256":p.sha256})
            })
            .collect();
        let code: Vec<Value> = plugins
            .iter()
            .map(|p| serde_json::json!({"id":p.id,"sha256":p.sha256,"source":p.source}))
            .collect();
        let mut script = runtime.to_owned();
        script.push_str(
            "\n;(function(){\nconst svg=document.currentScript && document.currentScript.ownerSVGElement;\nif(!svg || !svg.splinely || !svg.splinely.use)return;\nconst splinely=svg.splinely;\nconst bundles=",
        );
        script.push_str(&json_xml_safe(&Value::Array(code)));
        script.push_str(
            ";\nfor(const b of bundles){\n  try{\n    const plugin=new Function('\"use strict\";\\n'+b.source)();\n    if(!plugin || plugin.id!==b.id)throw new Error('plugin id must equal '+b.id);\n    splinely.use(plugin,{source:b.id,sha256:b.sha256});\n  }catch(error){splinely.reportError(new Error('Splinely plugin '+b.id+': '+String(error)));}\n}\n})();\n",
        );
        let svg = self.to_svg_with_options_embed(&script, options, embed)?;
        let block = format!(
            "<metadata id=\"splinely-plugins\" data-trust=\"explicit-trusted-js\">{}</metadata>\n",
            json_xml_safe(&Value::Array(meta))
        );
        let at = svg
            .find("<script type=\"application/ecmascript\">")
            .ok_or_else(|| crate::DocumentError("exported SVG has no script element".into()))?;
        let mut out = String::with_capacity(svg.len() + block.len());
        out.push_str(&svg[..at]);
        out.push_str(&block);
        out.push_str(&svg[at..]);
        Ok(out)
    }
}

// No direct serde dependency in the editor: explicit Value allowlists provide
// deny_unknown_fields semantics at EVERY object level before document decoding.
fn object<'a>(value: &'a Value, allowed: &[&str]) -> Result<&'a Map<String, Value>> {
    let map = value.as_object().ok_or("Expected an object")?;
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!(
                "Unknown field {:?}",
                key.chars().take(80).collect::<String>()
            ));
        }
    }
    Ok(map)
}
fn required<'a>(map: &'a Map<String, Value>, key: &str) -> Result<&'a Value> {
    map.get(key).ok_or_else(|| format!("Missing {key}"))
}
fn text(map: &Map<String, Value>, key: &str, max: usize) -> Result<String> {
    let s = required(map, key)?
        .as_str()
        .ok_or_else(|| format!("{key} must be a string"))?;
    if s.is_empty() || s.len() > max || s.chars().any(char::is_control) {
        return Err(format!(
            "{key} must be nonempty, bounded text without control characters"
        ));
    }
    Ok(s.into())
}
fn identifier(map: &Map<String, Value>) -> Result<String> {
    let s = text(map, "id", 64)?;
    if !s
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
        || !s.as_bytes()[0].is_ascii_alphabetic()
    {
        return Err(
            "ID must start with an ASCII letter and contain only letters, digits, _, - or .".into(),
        );
    }
    Ok(s)
}
fn number(value: &Value, min: f64, max: f64) -> Result<f64> {
    value
        .as_f64()
        .filter(|v| v.is_finite() && *v >= min && *v <= max)
        .ok_or_else(|| format!("Expected finite number in {min}..{max}"))
}
fn color(value: &Value) -> Result<Color> {
    let s = value.as_str().ok_or("Color must be #RRGGBB or #RRGGBBAA")?;
    let bytes = s.as_bytes();
    if !matches!(bytes.len(), 7 | 9)
        || bytes[0] != b'#'
        || !bytes[1..].iter().all(u8::is_ascii_hexdigit)
    {
        return Err("Color must be #RRGGBB or #RRGGBBAA (no URLs)".into());
    }
    let channel = |i| u8::from_str_radix(&s[i..i + 2], 16).unwrap() as f64 / 255.;
    Ok(Color {
        r: channel(1),
        g: channel(3),
        b: channel(5),
        a: if bytes.len() == 9 { channel(7) } else { 1. },
    })
}
fn style(value: &Value) -> Result<StylePatch> {
    let map = object(value, &["fill", "stroke", "strokeWidth", "opacity"])?;
    if map.is_empty() {
        return Err("Style patch must not be empty".into());
    }
    Ok(StylePatch {
        fill: map.get("fill").map(color).transpose()?,
        stroke: map
            .get("stroke")
            .map(|v| {
                if v.is_null() {
                    Ok(None)
                } else {
                    color(v).map(Some)
                }
            })
            .transpose()?,
        stroke_width: map
            .get("strokeWidth")
            .map(|v| number(v, 0., 1e7))
            .transpose()?,
        opacity: map.get("opacity").map(|v| number(v, 0., 1.)).transpose()?,
    })
}
fn shape(value: &Value) -> Result<NodeKind> {
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .ok_or("Shape requires type")?;
    let allowed: &[&str] = match kind {
        "rect" => &["type", "x", "y", "width", "height", "rx"],
        "circle" => &["type", "cx", "cy", "radius"],
        "path" => &["type", "d", "evenodd"],
        _ => return Err("Only rect, circle and path shapes are supported".into()),
    };
    let map = object(value, allowed)?;
    let n = |key: &str, min| number(required(map, key)?, min, 1e7);
    Ok(match kind {
        "rect" => NodeKind::Rect {
            x: n("x", -1e7)?,
            y: n("y", -1e7)?,
            width: n("width", 0.)?,
            height: n("height", 0.)?,
            rx: map
                .get("rx")
                .map(|v| number(v, 0., 1e7))
                .transpose()?
                .unwrap_or(0.),
        },
        "circle" => NodeKind::Circle {
            cx: n("cx", -1e7)?,
            cy: n("cy", -1e7)?,
            radius: n("radius", 0.)?,
        },
        _ => {
            let d = text(map, "d", 16 * 1024)?;
            let bounds = crate::path_bounds(&d).map_err(|e| e.to_string())?;
            let evenodd = map
                .get("evenodd")
                .map(|v| v.as_bool().ok_or("evenodd must be boolean".to_string()))
                .transpose()?
                .unwrap_or(false);
            NodeKind::Path { d, bounds, evenodd }
        }
    })
}
fn parse_manifest(bytes: &[u8]) -> Result<Extension> {
    if bytes.len() > MAX_MANIFEST {
        return Err("Manifest exceeds 65536 bytes".into());
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|e| format!("Invalid JSON: {e}"))?;
    reject_duplicate_keys(bytes)?;
    let map = object(&value, &["id", "title", "tools", "palette", "runtime"])?;
    let mut plugin = Extension {
        id: identifier(map)?,
        title: text(map, "title", 120)?,
        tools: vec![],
        palette: vec![],
        runtime: map.get("runtime").map(runtime_part).transpose()?,
    };
    let mut ids = HashSet::new();
    for key in ["tools", "palette"] {
        let Some(value) = map.get(key) else {
            continue;
        };
        let list = value
            .as_array()
            .ok_or_else(|| format!("{key} must be an array"))?;
        if list.len() > MAX_TOOLS {
            return Err(format!("{key} exceeds {MAX_TOOLS} entries"));
        }
        for item in list {
            let entry = object(
                item,
                if key == "tools" {
                    &["id", "title", "action"]
                } else {
                    &["id", "title", "style"]
                },
            )?;
            let id = identifier(entry)?;
            if !ids.insert(id.clone()) {
                return Err(format!("Duplicate tool/palette ID {id}"));
            }
            let title = text(entry, "title", 120)?;
            let action = if key == "palette" {
                Action::Style(style(required(entry, "style")?)?)
            } else {
                let value = required(entry, "action")?;
                match value.get("type").and_then(Value::as_str) {
                    Some("addShape") => {
                        let action = object(value, &["type", "shape", "style"])?;
                        let mut node = Node::new("plugin-template", shape(required(action, "shape")?)?);
                        node.name = title.clone();
                        if let Some(value) = action.get("style") { style(value)?.apply(&mut node); }
                        let mut probe = Project::default();
                        probe.nodes.push(node.clone());
                        probe.validate().map_err(|e| e.to_string())?;
                        Action::Add(Box::new(node))
                    }
                    Some("setStyle") => {
                        let action = object(value, &["type", "style"])?;
                        Action::Style(style(required(action, "style")?)?)
                    }
                    Some("addBehavior") => {
                        let action = object(value, &["type", "behavior"])?;
                        Action::Behavior(Box::new(behavior(required(action, "behavior")?, &title)?))
                    }
                    _ => return Err("Only addShape, setStyle and addBehavior actions are supported; executable scripts are not allowed".into()),
                }
            };
            let tool = ExtensionTool { id, title, action };
            if key == "tools" {
                plugin.tools.push(tool);
            } else {
                plugin.palette.push(tool);
            }
        }
    }
    if plugin.tools.is_empty() && plugin.palette.is_empty() && plugin.runtime.is_none() {
        return Err("Plugin must provide tools, palette entries or a runtime part".into());
    }
    Ok(plugin)
}

fn runtime_part(value: &Value) -> Result<RuntimePart> {
    let map = object(value, &["apiVersion", "entry", "description"])?;
    let api = map
        .get("apiVersion")
        .and_then(Value::as_u64)
        .ok_or("runtime.apiVersion must be an integer")?;
    if api != RUNTIME_API_VERSION as u64 {
        return Err(format!(
            "Unsupported runtime.apiVersion {api}; this build supports {RUNTIME_API_VERSION}"
        ));
    }
    let entry = text(map, "entry", 64)?;
    let ok = entry.ends_with(".js")
        && entry.as_bytes()[0].is_ascii_alphanumeric()
        && entry
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
        && !entry.contains("..");
    if !ok {
        return Err("runtime.entry must be a single file name ending in .js (letters, digits, _, - or .; no directories)".into());
    }
    Ok(RuntimePart {
        api_version: RUNTIME_API_VERSION,
        entry,
        description: if map.contains_key("description") {
            text(map, "description", 240)?
        } else {
            String::new()
        },
        bytes: 0,
        sha256: String::new(),
        source: String::new(),
    })
}

fn easing_name(value: Option<&Value>) -> Result<Easing> {
    let Some(value) = value else {
        return Ok(Easing::Linear);
    };
    let bezier = |x1, y1, x2, y2| Easing::CubicBezier { x1, y1, x2, y2 };
    Ok(match value.as_str() {
        Some("linear") => Easing::Linear,
        Some("step") => Easing::Step,
        Some("ease") => bezier(0.25, 0.1, 0.25, 1.),
        Some("ease-in") => bezier(0.42, 0., 1., 1.),
        Some("ease-out") => bezier(0., 0., 0.58, 1.),
        Some("ease-in-out") => bezier(0.42, 0., 0.58, 1.),
        _ => {
            return Err(
                "easing must be linear, step, ease, ease-in, ease-out or ease-in-out".into(),
            )
        }
    })
}

fn behavior_property(name: &str) -> Result<Property> {
    Ok(match name {
        "x" => Property::X,
        "y" => Property::Y,
        "rotation" => Property::Rotation,
        "scaleX" => Property::ScaleX,
        "scaleY" => Property::ScaleY,
        "opacity" => Property::Opacity,
        "fill" => Property::Fill,
        "stroke" => Property::Stroke,
        _ => {
            return Err(
                "behavior tracks support x, y, rotation, scaleX, scaleY, opacity, fill and stroke"
                    .into(),
            )
        }
    })
}

fn behavior(value: &Value, title: &str) -> Result<BehaviorSpec> {
    let map = object(value, &["trigger", "clip", "variables", "a11yLabel"])?;
    let trigger = object(required(map, "trigger")?, &["event"])?;
    let event = text(trigger, "event", 32)?;
    if !BEHAVIOR_EVENTS.contains(&event.as_str()) {
        return Err(format!(
            "trigger.event must be one of {}",
            BEHAVIOR_EVENTS.join(", ")
        ));
    }
    let clip = object(required(map, "clip")?, &["duration", "tracks"])?;
    let duration = number(required(clip, "duration")?, 0., MAX_BEHAVIOR_DURATION)?;
    if duration <= 0. {
        return Err("clip.duration must be greater than 0".into());
    }
    let list = required(clip, "tracks")?
        .as_array()
        .ok_or("clip.tracks must be an array")?;
    if list.is_empty() || list.len() > MAX_BEHAVIOR_TRACKS {
        return Err(format!(
            "clip.tracks needs 1..={MAX_BEHAVIOR_TRACKS} entries"
        ));
    }
    let mut tracks: Vec<(Property, Vec<Keyframe>)> = Vec::new();
    for item in list {
        let track = object(item, &["property", "keyframes"])?;
        let name = text(track, "property", 16)?;
        let property = behavior_property(&name)?;
        if tracks.iter().any(|(p, _)| *p == property) {
            return Err(format!("Duplicate track for {name}"));
        }
        let frames = required(track, "keyframes")?
            .as_array()
            .ok_or("keyframes must be an array")?;
        if frames.len() < 2 || frames.len() > MAX_BEHAVIOR_KEYFRAMES {
            return Err(format!(
                "keyframes needs 2..={MAX_BEHAVIOR_KEYFRAMES} entries"
            ));
        }
        let mut keyframes = Vec::new();
        let mut previous = -1.0;
        for frame in frames {
            let frame = object(frame, &["time", "value", "easing"])?;
            let time = number(required(frame, "time")?, 0., duration)?;
            if time <= previous {
                return Err("keyframe times must be strictly increasing".into());
            }
            previous = time;
            let raw = required(frame, "value")?;
            let value = match property {
                Property::Fill | Property::Stroke => AnimValue::Color(color(raw)?),
                Property::Opacity => AnimValue::Number(number(raw, 0., 1.)?),
                Property::ScaleX | Property::ScaleY => AnimValue::Number(number(raw, -1e4, 1e4)?),
                _ => AnimValue::Number(number(raw, -1e7, 1e7)?),
            };
            keyframes.push(Keyframe {
                time,
                value,
                easing: easing_name(frame.get("easing"))?,
            });
        }
        tracks.push((property, keyframes));
    }
    let mut variables = BTreeMap::new();
    if let Some(value) = map.get("variables") {
        let list = value.as_object().ok_or("variables must be an object")?;
        if list.len() > MAX_BEHAVIOR_VARIABLES {
            return Err(format!(
                "variables exceeds {MAX_BEHAVIOR_VARIABLES} entries"
            ));
        }
        for (name, initial) in list {
            let name_ok = name.len() <= 64
                && name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b));
            let value_ok = match initial {
                Value::Bool(_) => true,
                Value::Number(n) => n.as_f64().is_some_and(f64::is_finite),
                Value::String(s) => s.len() <= 120 && !s.chars().any(char::is_control),
                _ => false,
            };
            if !name_ok || !value_ok {
                return Err(format!(
                    "variable {name:?}: name must be an ASCII identifier (<=64) and the value a boolean, finite number or short string"
                ));
            }
            variables.insert(name.clone(), initial.clone());
        }
    }
    let label = match map.get("a11yLabel") {
        None => None,
        Some(_) => {
            let label = text(map, "a11yLabel", 200)?;
            if label.trim() != label {
                return Err("a11yLabel must not have leading or trailing whitespace".into());
            }
            Some(label)
        }
    };
    Ok(BehaviorSpec {
        title: title.to_owned(),
        event,
        duration,
        tracks,
        variables,
        label,
    })
}

fn same_json_type(a: &Value, b: &Value) -> bool {
    matches!(
        (a, b),
        (Value::Bool(_), Value::Bool(_))
            | (Value::Number(_), Value::Number(_))
            | (Value::String(_), Value::String(_))
    )
}

/// Merge one behavior into `next` (a clone of the project): one clip, one state machine,
/// optional variables and an accessible label. IDs are allocated against every ID namespace.
/// Existing project data is never replaced; the caller validates the whole project afterwards.
fn apply_behavior(
    spec: &BehaviorSpec,
    tool_id: &str,
    next: &mut Project,
    selected: Option<&str>,
) -> std::result::Result<(), CandidateError> {
    let node_id = editable_selection(next, selected)?.id.clone();
    if next.clips.len() >= MAX_PROJECT_CLIPS || next.state_machines.len() >= MAX_PROJECT_MACHINES {
        return Err("Plugin output clip/state machine limit exceeded".into());
    }
    for (name, initial) in &spec.variables {
        match next.variables.get(name) {
            None => {
                next.variables.insert(name.clone(), initial.clone());
            }
            Some(old) if same_json_type(old, initial) => {}
            Some(_) => {
                return Err(format!("Variable {name} already exists with a different type").into())
            }
        }
    }
    let clip_id = fresh_id_with(next, &format!("plugin-{tool_id}-clip"))?;
    next.clips.push(Clip {
        audio_cues: vec![],
        id: clip_id.clone(),
        name: spec.title.clone(),
        duration: spec.duration,
        looping: false,
        autoplay: false,
        blend_mode: Default::default(),
        weight: 1.0,
        tracks: spec
            .tracks
            .iter()
            .map(|(property, keyframes)| Track {
                node_id: node_id.clone(),
                property: *property,
                keyframes: keyframes.clone(),
            })
            .collect(),
    });
    let machine_id = fresh_id_with(next, &format!("plugin-{tool_id}-machine"))?;
    next.state_machines.push(StateMachine {
        id: machine_id,
        initial: "idle".into(),
        states: vec![State {
            id: "idle".into(),
            on_enter: vec![],
            overrides: vec![],
            on_exit: vec![],
            announce: None,
        }],
        transitions: vec![Transition {
            from: "idle".into(),
            to: "idle".into(),
            event: Event {
                name: spec.event.clone(),
                target: Some(node_id.clone()),
            },
            guard: None,
            // Stop first so a retrigger restarts the clip even while it is still playing.
            actions: vec![
                crate::Action::StopClip {
                    clip_id: clip_id.clone(),
                },
                crate::Action::PlayClip { clip_id },
            ],
            blend: 0.0,
            after: None,
        }],
    });
    if let Some(label) = &spec.label {
        let node = editable_selection(next, Some(&node_id))?;
        let a11y = node.a11y.get_or_insert_with(Default::default);
        // Never overwrite an author's label.
        if a11y.label.is_none() {
            a11y.label = Some(label.clone());
        }
    }
    Ok(())
}

/// SHA-256 (FIPS 180-4) as `sha256:<hex>`. Used to pin plugin sources; implemented here to keep
/// the dependency set unchanged.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in message.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut out = String::from("sha256:");
    for word in h {
        out.push_str(&format!("{word:08x}"));
    }
    out
}

// Run only after serde_json accepts the input. String tokens followed by ':'
// are object keys; escape-aware scanning avoids confusing punctuation in values.
fn reject_duplicate_keys(bytes: &[u8]) -> Result<()> {
    let mut objects: Vec<HashSet<String>> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => objects.push(HashSet::new()),
            b'}' => {
                objects.pop();
            }
            b'"' => {
                let start = i;
                i += 1;
                while bytes[i] != b'"' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                let end = i + 1;
                let mut next = end;
                while next < bytes.len() && bytes[next].is_ascii_whitespace() {
                    next += 1;
                }
                if bytes.get(next) == Some(&b':') {
                    let key: String =
                        serde_json::from_slice(&bytes[start..end]).map_err(|e| e.to_string())?;
                    if !objects.last_mut().ok_or("Invalid JSON object")?.insert(key) {
                        return Err("Duplicate JSON object key".into());
                    }
                }
            }
            _ => (),
        }
        i += 1;
    }
    Ok(())
}

fn fresh_id(project: &Project) -> Result<String> {
    fresh_id_with(project, "plugin-node")
}
fn fresh_id_with(project: &Project, prefix: &str) -> Result<String> {
    let used: HashSet<&str> = project
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .chain(project.bones.iter().map(|n| n.id.as_str()))
        .chain(project.clips.iter().map(|n| n.id.as_str()))
        .chain(project.state_machines.iter().map(|n| n.id.as_str()))
        .chain(project.audio_assets.iter().map(|n| n.id.as_str()))
        .chain(project.audio_cues.iter().map(|n| n.id.as_str()))
        .chain(
            project
                .clips
                .iter()
                .flat_map(|c| &c.audio_cues)
                .map(|n| n.id.as_str()),
        )
        .chain(project.followers.iter().map(|n| n.id.as_str()))
        .chain(project.blend_spaces.iter().map(|n| n.id.as_str()))
        .chain(project.groups.iter().map(|n| n.id.as_str()))
        .collect();
    first_free(&used, prefix)
}
/// `<prefix>-<n>` with the smallest `n` not used by any ID namespace of the project.
fn first_free(used: &HashSet<&str>, prefix: &str) -> Result<String> {
    for i in 1..=used.len().saturating_add(1) {
        let id = format!("{prefix}-{i}");
        if !used.contains(id.as_str()) {
            return Ok(id);
        }
    }
    Err("Unable to allocate a plugin ID".into())
}

fn check_output_size(project: &Project) -> Result<()> {
    struct Budget(usize);
    impl Write for Budget {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.0 {
                return Err(io::Error::other("Plugin output exceeds 8 MiB"));
            }
            self.0 -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Budget(MAX_OUTPUT), project).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "splinely-extensions-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn put(&self, folder: &str, manifest: &[u8]) {
            let path = self.0.join(folder);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("splinely-plugin.json"), manifest).unwrap();
        }
        fn plugin(&self, folder: &str, id: &str) {
            self.put(folder, &serde_json::to_vec(&json!({
                "id": id, "title": id,
                "palette": [{"id":"blue", "title":"Blue", "style":{"fill":"#123456", "stroke":"#FF000080", "strokeWidth":4, "opacity":0.5}}],
                "tools": [{"id":"rect", "title":"Rectangle", "action":{"type":"addShape", "shape":{"type":"rect", "x":1, "y":2, "width":30, "height":40}}}]
            })).unwrap());
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn palette(style: Value) -> Vec<u8> {
        serde_json::to_vec(&json!({"id":"test", "title":"Test", "palette":[{"id":"style", "title":"Style", "style":style}]})).unwrap()
    }

    #[test]
    fn discovery_is_sorted_and_reports_bad_plugins() {
        let dir = Temp::new();
        dir.plugin("first-folder", "zebra");
        dir.plugin("last-folder", "alpha");
        dir.put("broken", b"not JSON");
        fs::write(dir.0.join("ignored.txt"), b"ignored").unwrap();
        let registry = ExtensionRegistry::load_directory(&dir.0);
        assert_eq!(
            registry
                .plugins()
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "zebra"]
        );
        assert_eq!(registry.diagnostics.len(), 1);
        assert!(registry.diagnostics[0].contains("broken: Invalid JSON"));
        assert!(ExtensionRegistry::load_directory(dir.0.join("absent"))
            .plugins()
            .is_empty());
        let again = ExtensionRegistry::load_directory(&dir.0);
        assert_eq!(registry.diagnostics, again.diagnostics);
    }

    #[test]
    fn rejects_all_duplicate_ids_and_oversized_manifest() {
        let dir = Temp::new();
        dir.plugin("one", "duplicate");
        dir.plugin("two", "duplicate");
        dir.plugin("valid", "valid");
        dir.put("huge", &vec![b' '; MAX_MANIFEST + 1]);
        let registry = ExtensionRegistry::load_directory(&dir.0);
        assert_eq!(registry.plugins.len(), 1);
        assert_eq!(registry.plugins[0].id, "valid");
        assert!(registry
            .diagnostics
            .iter()
            .any(|s| s.contains("Duplicate plugin ID duplicate")));
        assert!(registry
            .diagnostics
            .iter()
            .any(|s| s.contains("exceeds 65536")));
    }

    #[test]
    fn unknown_fields_scripts_urls_and_invalid_numbers_are_rejected() {
        let good: Value = serde_json::from_slice(&palette(json!({"fill":"#FFFFFF"}))).unwrap();
        for field in ["script", "python", "shell", "library", "image", "url"] {
            let mut manifest = good.clone();
            manifest[field] = json!("https://example.invalid/payload");
            assert!(parse_manifest(&serde_json::to_vec(&manifest).unwrap()).is_err());
        }
        for style in [
            json!({"fill":"url(https://example.invalid/x)"}),
            json!({"fill":"#fff"}),
            json!({"opacity":1.1}),
            json!({"strokeWidth":-1}),
            json!({"strokeWidth":1e8}),
            json!({"fill":null}),
            json!({"stroke":"#GG0000"}),
            json!({"opacity":"1"}),
            json!({"script":"evil"}),
            json!({}),
        ] {
            assert!(parse_manifest(&palette(style)).is_err());
        }
        for action in [
            json!({"type":"script", "code":"evil"}),
            json!({"type":"addShape", "shape":{"type":"image", "url":"https://example.invalid/x"}}),
            json!({"type":"setStyle", "style":{"fill":"#FFFFFF"}, "script":"evil"}),
            json!({"type":"addShape", "shape":{"type":"rect", "x":0, "y":0, "width":1, "height":1, "script":"evil"}}),
        ] {
            let manifest = json!({"id":"test", "title":"Test", "tools":[{"id":"bad", "title":"Bad", "action":action}]});
            assert!(parse_manifest(&serde_json::to_vec(&manifest).unwrap()).is_err());
        }
        assert!(parse_manifest(br##"{"id":"test","title":"Test","palette":[{"id":"a","title":"A","style":{"opacity":2,"opacity":1}}]}"##).is_err());
        assert!(parse_manifest(br##"{"id":"test","title":"Test","palette":[{"id":"a","title":"A","style":{"fill":"#FFFFFF","\u0066ill":"#000000"}}]}"##).is_err());
        // Punctuation and escaped quotes in string values are not object keys.
        let manifest = json!({"id":"test", "title":"a \"quoted\": {title}", "palette":[{"id":"a", "title":"a", "style":{"fill":"#FFFFFF"}}]});
        assert!(parse_manifest(&serde_json::to_vec(&manifest).unwrap()).is_ok());
    }

    #[test]
    fn tool_and_directory_limits_are_enforced() {
        let mut manifest: Value =
            serde_json::from_slice(&palette(json!({"fill":"#FFFFFF"}))).unwrap();
        let entry = manifest["palette"][0].clone();
        manifest["palette"] = Value::Array(vec![entry.clone(); MAX_TOOLS + 1]);
        assert!(parse_manifest(&serde_json::to_vec(&manifest).unwrap())
            .unwrap_err()
            .contains("exceeds 64"));
        manifest["palette"] = Value::Array(vec![entry.clone(), entry]);
        assert!(parse_manifest(&serde_json::to_vec(&manifest).unwrap())
            .unwrap_err()
            .contains("Duplicate tool"));
        let dir = Temp::new();
        for i in 0..=MAX_ENTRIES {
            fs::write(dir.0.join(format!("entry-{i}")), b"").unwrap();
        }
        let registry = ExtensionRegistry::load_directory(&dir.0);
        assert!(registry.plugins().is_empty());
        assert!(registry.diagnostics[0].contains("exceeds 128"));
    }

    #[test]
    fn global_action_budget_rejects_registry_without_partial_activation() {
        let dir = Temp::new();
        for i in 0..17 {
            let entries: Vec<_> = (0..64).map(|n| json!({"id":format!("color-{n}"),"title":"Color", "style":{"fill":"#123456"}})).collect();
            dir.put(
                &format!("folder-{i}"),
                &serde_json::to_vec(
                    &json!({"id":format!("plugin-{i}"),"title":"Colors","palette":entries}),
                )
                .unwrap(),
            );
        }
        let registry = ExtensionRegistry::load_directory(&dir.0);
        assert!(registry.plugins().is_empty());
        assert!(registry.diagnostics[0].contains("exceeds 1024 actions"));
    }

    #[test]
    fn style_is_atomic_locked_safe_and_preserves_transform_and_binding() {
        let dir = Temp::new();
        dir.plugin("one", "one");
        let registry = ExtensionRegistry::load_directory(&dir.0);
        let mut project = Project::default();
        project.bones.push(crate::Bone {
            id: "bone".into(),
            parent_id: None,
            x: 0.,
            y: 0.,
            rotation: 15.,
            length: 20.,
        });
        let mut node = Node::rect("node", 10., 20., 30., 40.);
        node.bone_id = Some("bone".into());
        node.transform = crate::Transform {
            x: 12.,
            y: 34.,
            rotation: 67.,
            scale_x: -2.,
            scale_y: 0.5,
        };
        project.nodes.push(node);
        project.validate().unwrap();
        let before = project.clone();
        let candidate = registry
            .candidate("one", "blue", &project, Some("node"))
            .unwrap();
        assert_eq!(project, before);
        let mut expected = project.nodes[0].clone();
        expected.style = candidate.nodes[0].style;
        assert_eq!(candidate.nodes[0], expected);
        assert_eq!(candidate.nodes[0].style.opacity, 0.5);
        candidate.validate().unwrap();
        assert!(registry.candidate("one", "blue", &project, None).is_err());
        assert!(registry
            .candidate("one", "blue", &project, Some("missing"))
            .is_err());
        project.nodes[0].locked = true;
        let locked = project.clone();
        assert!(registry
            .candidate("one", "blue", &project, Some("node"))
            .is_err());
        assert_eq!(project, locked);
        let added = registry
            .candidate("one", "rect", &project, Some("node"))
            .unwrap();
        assert_eq!(added.nodes[0], project.nodes[0]);
        // Full-project validation rejects unrelated invalid data without mutation.
        project.width = -1.;
        let invalid = project.clone();
        assert!(registry.candidate("one", "rect", &project, None).is_err());
        assert_eq!(project, invalid);
    }

    #[test]
    fn generated_ids_avoid_node_bone_and_other_project_ids() {
        let dir = Temp::new();
        dir.plugin("one", "one");
        let registry = ExtensionRegistry::load_directory(&dir.0);
        let mut project = Project::default();
        project
            .nodes
            .push(Node::circle("plugin-node-1", 0., 0., 1.));
        project.bones.push(crate::Bone {
            id: "plugin-node-2".into(),
            parent_id: None,
            x: 0.,
            y: 0.,
            rotation: 0.,
            length: 1.,
        });
        project.clips.push(crate::Clip {
            audio_cues: vec![],
            blend_mode: Default::default(),
            weight: 1.0,
            id: "plugin-node-3".into(),
            name: String::new(),
            duration: 1.,
            looping: false,
            autoplay: false,
            tracks: vec![],
        });
        project.audio_assets.push(crate::AudioAsset {
            id: "sound".into(),
            name: String::new(),
            mime_type: "audio/wav".into(),
            data_base64: "AQID".into(),
        });
        project.clips[0].audio_cues.push(crate::AudioCue {
            id: "plugin-node-4".into(),
            asset_id: "sound".into(),
            time: 0.,
            offset: 0.,
            duration: None,
            volume: 1.,
        });
        let next = registry.candidate("one", "rect", &project, None).unwrap();
        assert_eq!(next.nodes.last().unwrap().id, "plugin-node-5");
        let again = registry.candidate("one", "rect", &next, None).unwrap();
        assert_eq!(again.nodes.last().unwrap().id, "plugin-node-6");
        assert_eq!(project.nodes.len(), 1);
        again.validate().unwrap();
    }

    #[test]
    fn shipped_templates_validate_and_output_is_bounded() {
        for bytes in [
            include_bytes!("../../../examples/plugins/basic-shapes/splinely-plugin.json")
                .as_slice(),
            include_bytes!("../../../examples/plugins/colors/splinely-plugin.json").as_slice(),
        ] {
            let plugin = parse_manifest(bytes).unwrap();
            let id = plugin.id.clone();
            let tools: Vec<_> = plugin
                .tools
                .iter()
                .chain(&plugin.palette)
                .map(|t| t.id.clone())
                .collect();
            let registry = ExtensionRegistry {
                directory: None,
                diagnostics: vec![],
                plugins: vec![plugin],
            };
            let mut project = Project::default();
            project.nodes.push(Node::rect("selected", 0., 0., 1., 1.));
            for tool in tools {
                registry
                    .candidate(&id, &tool, &project, Some("selected"))
                    .unwrap()
                    .validate()
                    .unwrap();
            }
        }
        let dir = Temp::new();
        dir.plugin("one", "one");
        let registry = ExtensionRegistry::load_directory(&dir.0);
        let mut project = Project {
            name: "x".repeat(MAX_OUTPUT),
            ..Project::default()
        };
        assert!(registry.candidate("one", "rect", &project, None).is_err());
        project.name.clear();
        project.nodes = (0..MAX_NODES)
            .map(|i| Node::rect(format!("n{i}"), 0., 0., 1., 1.))
            .collect();
        assert!(registry.candidate("one", "rect", &project, None).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_plugins_and_manifests_are_rejected() {
        use std::os::unix::fs::symlink;
        let dir = Temp::new();
        dir.plugin("real", "real");
        symlink(dir.0.join("real"), dir.0.join("linked")).unwrap();
        fs::create_dir(dir.0.join("bad-manifest")).unwrap();
        symlink(
            dir.0.join("real/splinely-plugin.json"),
            dir.0.join("bad-manifest/splinely-plugin.json"),
        )
        .unwrap();
        let registry = ExtensionRegistry::load_directory(&dir.0);
        assert_eq!(registry.plugins.len(), 1);
        assert_eq!(registry.diagnostics.len(), 2);
    }
}
