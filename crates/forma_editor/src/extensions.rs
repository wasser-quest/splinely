//! Data-only, restart-discovered extensions. No executable plugin code is loaded.
//! See docs/drop-in-plugins.md for the strict manifest format and host integration.
use eframe::egui;
use forma_document::{Color, Node, NodeKind, Project};
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

type Result<T> = std::result::Result<T, String>;

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
                    "Plugins: no FORMA_PLUGIN_DIR, XDG_CONFIG_HOME or HOME configured".into(),
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
    ) -> Result<Project> {
        let plugin = self
            .plugins
            .iter()
            .find(|p| p.id == plugin_id)
            .ok_or("Unbekanntes Plugin")?;
        let tool = plugin
            .tools
            .iter()
            .chain(&plugin.palette)
            .find(|t| t.id == tool_id)
            .ok_or("Unbekanntes Plugin-Werkzeug")?;
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
                let selected = selected.ok_or("Zuerst eine Form auswählen")?;
                let node = next
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == selected)
                    .ok_or("Ausgewählte Form existiert nicht mehr")?;
                if node.locked {
                    return Err("Ausgewählte Form ist gesperrt".into());
                }
                style.apply(node);
            }
        }
        next.validate()
            .map_err(|e| format!("Plugin-Ergebnis abgelehnt: {e}"))?;
        check_output_size(&next)?;
        Ok(next)
    }

    /// Render in the right sidebar. Diagnostics remain visible, not just in logs.
    /// Returns at most one candidate per frame; no selection/history side effects.
    pub fn ui(
        &self,
        ui: &mut egui::Ui,
        project: &Project,
        selected: Option<&str>,
    ) -> Result<Option<Project>> {
        ui.heading("Drop-in-Plugins");
        if let Some(directory) = &self.directory {
            ui.small(format!("Ordner: {}", directory.display()));
        }
        ui.small("forma-plugin.json einlegen › Neustart. Nur Daten-Werkzeuge, kein Fremdcode.");
        for diagnostic in &self.diagnostics {
            ui.colored_label(egui::Color32::YELLOW, diagnostic);
        }
        if self.plugins.is_empty() {
            ui.label("Keine Plugins geladen (nach dem Installieren neu starten).");
        }
        for plugin in &self.plugins {
            let mut clicked = None;
            ui.push_id(&plugin.id, |ui| {
                ui.collapsing(&plugin.title, |ui| {
                    for (label, tools) in
                        [("Werkzeuge", &plugin.tools), ("Palette", &plugin.palette)]
                    {
                        if tools.is_empty() {
                            continue;
                        }
                        ui.label(label);
                        for tool in tools {
                            let enabled = !matches!(tool.action, Action::Style(_))
                                || selected.is_some_and(|id| {
                                    project.nodes.iter().any(|n| n.id == id && !n.locked)
                                });
                            if ui
                                .add_enabled(enabled, egui::Button::new(&tool.title))
                                .clicked()
                            {
                                clicked = Some(tool.id.as_str());
                            }
                        }
                    }
                });
            });
            if let Some(id) = clicked {
                return self.candidate(&plugin.id, id, project, selected).map(Some);
            }
        }
        Ok(None)
    }
}

fn default_directory() -> Option<PathBuf> {
    fn env_path(name: &str) -> Option<PathBuf> {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }
    env_path("FORMA_PLUGIN_DIR")
        .or_else(|| env_path("XDG_CONFIG_HOME").map(|p| p.join("forma/plugins")))
        .or_else(|| env_path("HOME").map(|p| p.join(".config/forma/plugins")))
}

fn read_plugin(path: &Path) -> Result<Option<Extension>> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() {
        return Err("Symlink plugin entries are not allowed".into());
    }
    if !meta.is_dir() {
        return Ok(None);
    }
    let manifest = path.join("forma-plugin.json");
    let meta = fs::symlink_metadata(&manifest)
        .map_err(|e| format!("Cannot read forma-plugin.json: {e}"))?;
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
    parse_manifest(&bytes).map(Some)
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
            let bounds = forma_document::path_bounds(&d).map_err(|e| e.to_string())?;
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
    let map = object(&value, &["id", "title", "tools", "palette"])?;
    let mut plugin = Extension {
        id: identifier(map)?,
        title: text(map, "title", 120)?,
        tools: vec![],
        palette: vec![],
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
                    _ => return Err("Only addShape and setStyle actions are supported; executable scripts are not allowed".into()),
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
    if plugin.tools.is_empty() && plugin.palette.is_empty() {
        return Err("Plugin must provide tools or palette entries".into());
    }
    Ok(plugin)
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
    let used: HashSet<&str> = project
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .chain(project.bones.iter().map(|n| n.id.as_str()))
        .chain(project.clips.iter().map(|n| n.id.as_str()))
        .chain(project.state_machines.iter().map(|n| n.id.as_str()))
        .chain(project.audio_assets.iter().map(|n| n.id.as_str()))
        .chain(project.audio_cues.iter().map(|n| n.id.as_str()))
        .chain(project.speech_cues.iter().map(|n| n.id.as_str()))
        .collect();
    for i in 1..=used.len().saturating_add(1) {
        let id = format!("plugin-node-{i}");
        if !used.contains(id.as_str()) {
            return Ok(id);
        }
    }
    Err("Unable to allocate a plugin node ID".into())
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
                "forma-extensions-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn put(&self, folder: &str, manifest: &[u8]) {
            let path = self.0.join(folder);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("forma-plugin.json"), manifest).unwrap();
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
        project.bones.push(forma_document::Bone {
            id: "bone".into(),
            parent_id: None,
            x: 0.,
            y: 0.,
            rotation: 15.,
            length: 20.,
        });
        let mut node = Node::rect("node", 10., 20., 30., 40.);
        node.bone_id = Some("bone".into());
        node.transform = forma_document::Transform {
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
        project.bones.push(forma_document::Bone {
            id: "plugin-node-2".into(),
            parent_id: None,
            x: 0.,
            y: 0.,
            rotation: 0.,
            length: 1.,
        });
        project.clips.push(forma_document::Clip {
            blend_mode: Default::default(),
            weight: 1.0,
            id: "plugin-node-3".into(),
            name: String::new(),
            duration: 1.,
            looping: false,
            autoplay: false,
            tracks: vec![],
        });
        let next = registry.candidate("one", "rect", &project, None).unwrap();
        assert_eq!(next.nodes.last().unwrap().id, "plugin-node-4");
        let again = registry.candidate("one", "rect", &next, None).unwrap();
        assert_eq!(again.nodes.last().unwrap().id, "plugin-node-5");
        assert_eq!(project.nodes.len(), 1);
        again.validate().unwrap();
    }

    #[test]
    fn shipped_templates_validate_and_output_is_bounded() {
        for bytes in [
            include_bytes!("../../../examples/plugins/basic-shapes/forma-plugin.json").as_slice(),
            include_bytes!("../../../examples/plugins/colors/forma-plugin.json").as_slice(),
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
            dir.0.join("real/forma-plugin.json"),
            dir.0.join("bad-manifest/forma-plugin.json"),
        )
        .unwrap();
        let registry = ExtensionRegistry::load_directory(&dir.0);
        assert_eq!(registry.plugins.len(), 1);
        assert_eq!(registry.diagnostics.len(), 2);
    }
}
