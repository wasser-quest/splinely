#![recursion_limit = "256"] // Complete discovery templates expand through serde_json::json!.

use clap::{Parser, Subcommand};
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{json, Value};
use splinely_document::{BrowserExportOptions, Node, Project, MAX_PROJECT_FILE_BYTES};
use std::io::{self, Read, Write};
use std::path::Path;

mod assist;
mod batch;
mod blending;
mod bones;
mod clips;
mod discovery;
mod evaluation;
mod guide;
mod keyframes;
mod lint;
mod paths;
mod scaffold;
mod shapes;
mod showcase;
mod vectors;

const RUNTIME: &str = include_str!("../../../runtime/src/splinely.js");

#[derive(Parser)]
#[command(
    name = "splinely-cli",
    version,
    about = "Create, validate, edit and export Splinely .sply projects (strict JSON)",
    after_help = "New here (human or LLM)? Run `splinely-cli guide` first: a short workflow with recipes.\nEvery edit validates before an atomic save; --output defaults to the input file.\nResults are JSON on stdout; errors are JSON on stderr with code, message and hint."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// START HERE: short workflow, recipes and pitfalls for authoring with the CLI (Markdown).
    Guide {
        /// Emit the guide wrapped in JSON instead of Markdown.
        #[arg(long)]
        json: bool,
    },
    /// Find likely mistakes (hidden shapes, clips that never play, unreadable text …) with fixes.
    Check {
        input: String,
        /// Exit with an error when warnings are found (for scripts).
        #[arg(long)]
        strict: bool,
    },
    /// Run many edit commands on one project, all or nothing (one command per line or a JSON array).
    Batch {
        /// Project to edit; may be missing when the first step is `new`.
        input: String,
        /// Steps file (or - for stdin): one command per line without the program name,
        /// input and --output, e.g. `node add --kind rect --id bg --width 800 --height 600`.
        #[arg(long)]
        steps: String,
        /// Where to save the result (default: the input).
        #[arg(long)]
        output: Option<String>,
    },
    /// List, add, update (duration/autoplay/loop/easing) or delete animation clips.
    Clip {
        #[command(subcommand)]
        command: clips::ClipCommand,
    },
    /// Make shapes interactive: on EVENT at TARGET play/stop clips or set properties.
    On {
        input: String,
        /// click, dblclick, pointerenter, pointerleave, pointerdown, pointerup, keydown, dragstart, drag, dragend, clip.end … or a custom emitted name.
        #[arg(long)]
        event: String,
        /// Node that receives the event (omit for any target).
        #[arg(long, alias = "node")]
        target: Option<String>,
        /// Clip(s) to start; repeatable.
        #[arg(long)]
        play: Vec<String>,
        /// Clip(s) to stop; repeatable.
        #[arg(long)]
        stop: Vec<String>,
        /// NODE.PROPERTY=VALUE, e.g. ball.fill=red or ball.opacity=0.5; repeatable.
        #[arg(long, allow_hyphen_values = true)]
        set: Vec<String>,
        /// State machine to add the transition to (created if missing).
        #[arg(long, default_value = "interactions")]
        machine: String,
        /// Only react in this state (default: the machine's initial state).
        #[arg(long)]
        from: Option<String>,
        /// Switch to this state (created if missing; default: stay).
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        output: String,
    },
    /// Emit a format guide and complete valid examples as JSON.
    Schema,
    /// List supported document features, commands, model, examples and limitations as JSON.
    Capabilities,
    /// Set an existing clip's authored blend settings; does not activate it.
    BlendClip {
        input: String,
        #[arg(long)]
        output: String,
        #[arg(long)]
        clip: String,
        /// Finite weight in [0,1].
        #[arg(long, default_value_t = 1.0, value_parser = bones::number, allow_hyphen_values = true)]
        weight: f64,
        /// Defaults to weighted; legacy document clips default to override.
        #[arg(long, value_enum, default_value = "weighted")]
        mode: blending::Mode,
    },
    /// Split a path's zero-based L/Q/C segment without changing its shape (M/Z not counted).
    PathInsert {
        input: String,
        /// Existing path node ID.
        id: String,
        #[arg(long)]
        output: String,
        #[arg(long)]
        segment: usize,
        /// Finite parameter strictly between 0 and 1.
        #[arg(long, default_value_t = 0.5, value_parser = bones::number, allow_hyphen_values = true)]
        t: f64,
    },
    /// Edit a normalized M/L/Q/C/Z path handle in local SVG units; rejects H/V/S/T/A.
    PathHandle {
        input: String,
        id: String,
        #[arg(long)]
        output: String,
        /// Zero-based parsed command index, including M and Z.
        #[arg(long)]
        command_index: usize,
        #[arg(long, value_enum)]
        handle: paths::Handle,
        #[arg(long, value_parser = bones::number, allow_hyphen_values = true)]
        x: f64,
        #[arg(long, value_parser = bones::number, allow_hyphen_values = true)]
        y: f64,
    },
    /// Create an empty project (or a demonstration project).
    New {
        #[arg(long)]
        output: String,
        #[arg(long)]
        demo: bool,
        /// Canvas width in SVG units (default 800).
        #[arg(long, value_parser = bones::number)]
        width: Option<f64>,
        /// Canvas height in SVG units (default 600).
        #[arg(long, value_parser = bones::number)]
        height: Option<f64>,
        #[arg(long)]
        name: Option<String>,
        /// Add a full-size background rectangle (ID "background", bottom layer) in this colour.
        #[arg(long)]
        background: Option<String>,
    },
    /// Convert a strict project between .sply and legacy JSON; never imports exported SVG.
    Convert {
        input: String,
        #[arg(long)]
        output: String,
    },
    /// Import supported SVG vectors as editable paths (rejects unsupported appearance).
    ImportSvg {
        input: String,
        #[arg(long)]
        output: String,
    },
    /// Insert/replace a keyframe (number, colour, gradient or path) on a node or bone.
    Keyframe {
        /// Project .sply/JSON path, or - for stdin. Locked nodes remain explicitly editable.
        input: String,
        #[arg(long)]
        output: String,
        /// Existing clip ID, or a new globally unique ID (creates an inactive clip).
        #[arg(long)]
        clip: String,
        /// Existing node or bone ID. Bones support only x, y and rotation.
        #[arg(long)]
        target: String,
        /// Property: x, y, rotation, scaleX, scaleY, opacity, width, height, radius,
        /// fontSize (with --value), fill/stroke (with --color), gradient (with --gradient) or d (with --path).
        /// x/y are the transform offset from the authored position (0 = where the shape was drawn).
        #[arg(long, value_parser = keyframes::property)]
        property: splinely_document::Property,
        /// Finite nonnegative seconds; extends clip duration when needed.
        #[arg(long, value_parser = bones::number, allow_hyphen_values = true)]
        time: f64,
        /// Finite numeric value; opacity 0..1, geometry nonnegative, rotation in degrees.
        #[arg(long, value_parser = bones::number, allow_hyphen_values = true, conflicts_with_all = ["gradient", "path", "color", "by"], required_unless_present_any = ["gradient", "path", "color", "by"])]
        value: Option<f64>,
        /// Numeric keyframe relative to the current static value (e.g. --property y --by -200 = 200 up).
        #[arg(long, value_parser = bones::number, allow_hyphen_values = true, conflicts_with_all = ["gradient", "path", "color"])]
        by: Option<f64>,
        /// Do not add the automatic keyframe at 0 s that holds the current value when a track is new.
        #[arg(long)]
        no_start_key: bool,
        /// Colour keyframe for fill/stroke: #RRGGBB, #RRGGBBAA, CSS name or rgb().
        #[arg(long, conflicts_with_all = ["gradient", "path"])]
        color: Option<String>,
        /// Transition from this keyframe to the next: linear, step, ease, ease-in, ease-out, ease-in-out, cubic-bezier(x1,y1,x2,y2).
        #[arg(long, value_parser = clips::easing)]
        easing: Option<splinely_document::Easing>,
        /// Also set the clip to start automatically (true/false; bare flag = true).
        #[arg(long, num_args = 0..=1, default_missing_value = "true", value_parser = clips::flag)]
        autoplay: Option<bool>,
        /// Also set the clip to repeat (true/false; bare flag = true).
        #[arg(long = "loop", num_args = 0..=1, default_missing_value = "true", value_parser = clips::flag)]
        looping: Option<bool>,
        /// Gradient keyframe like `node --gradient` ("linear 90 red blue"); same kind and stop count as the node's gradient.
        #[arg(long, allow_hyphen_values = true, conflicts_with = "path")]
        gradient: Option<String>,
        /// Path data keyframe; all keyframes of the track need the same structure (see `arrange morph`).
        #[arg(long, allow_hyphen_values = true)]
        path: Option<String>,
    },
    /// Validate a project. Use - to read JSON from stdin.
    Validate { input: String },
    /// Summarize a validated project.
    Inspect { input: String },
    /// Export a self-contained interactive SVG with embedded runtime.
    Export {
        input: String,
        #[arg(long)]
        output: String,
        /// Execute authored JavaScript in the exported browser SVG. Trusted code only.
        #[arg(long)]
        trusted_js: bool,
    },
    /// Apply RFC 7396 JSON Merge Patch; validate before atomically saving.
    Apply {
        input: String,
        #[arg(long)]
        patch: String,
        #[arg(long)]
        output: String,
    },
    /// Create a visible Pivot-style skeleton with a bone-driven clip.
    RigDemo {
        #[arg(long)]
        output: String,
    },
    /// Author a deterministic water-drop bounce on a softly lit blue ball.
    WaterDrop {
        #[arg(long)]
        output: String,
    },
    /// Author seeded figures with narration and automatic sequential state cycling.
    RandomFigures {
        #[arg(long)]
        output: String,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[arg(long, default_value_t = 6)]
        count: usize,
    },
    /// Edit bones using validated atomic project saves.
    Bone {
        #[command(subcommand)]
        command: bones::BoneCommand,
    },
    /// Generate trusted native plugin source only; never build or execute it.
    Plugin {
        #[command(subcommand)]
        command: scaffold::PluginCommand,
    },
    /// Convert a rectangle or circle to an editable path.
    ConvertToPath {
        input: String,
        #[arg(long, alias = "nodes", alias = "id")]
        node: String,
        #[arg(long)]
        output: String,
    },
    /// Restore the primitive from an unmodified converted path.
    ConvertBack {
        input: String,
        #[arg(long, alias = "nodes", alias = "id")]
        node: String,
        #[arg(long)]
        output: String,
    },
    /// Add, update, delete, duplicate or list shapes.
    Node {
        #[command(subcommand)]
        command: Box<shapes::NodeCommand>,
    },
    /// Align, distribute, flip, move, group or restack shapes (same logic as the editor).
    Arrange {
        #[command(subcommand)]
        command: shapes::ArrangeCommand,
    },
    /// Render a PNG at a time, or (output ending in .gif) a looping animated GIF
    /// of the autoplay clips. Static rendering, no scripts.
    Render {
        input: String,
        #[arg(long)]
        output: String,
        #[arg(long, default_value_t = 0.0, value_parser = evaluation::time, allow_hyphen_values = true)]
        time: f64,
        /// Output width in pixels (default: document width); height keeps the aspect ratio.
        #[arg(long)]
        width: Option<u32>,
        /// GIF frames per second (1..50, default 15).
        #[arg(long, default_value_t = 15.0, value_parser = bones::number)]
        fps: f64,
        /// GIF length in seconds (default: longest autoplay clip).
        #[arg(long, value_parser = bones::number)]
        duration: Option<f64>,
        /// Render this clip (even if not autoplay) instead of the autoplay clips.
        #[arg(long)]
        clip: Option<String>,
    },
    /// Reorder nodes or set visibility and editor locking metadata.
    Layer {
        #[command(subcommand)]
        command: vectors::LayerCommand,
    },
    /// Emit the complete demo project JSON on stdout, or save an authored .sply file.
    Example {
        #[arg(long)]
        output: Option<String>,
    },
    /// Return evaluated node values at a time in seconds (no side effects).
    Evaluate {
        input: String,
        #[arg(long, default_value_t = 0.0, value_parser = evaluation::time, allow_hyphen_values = true)]
        time: f64,
        /// Strict JSON context: states, clips with independent times, clipWeights, overrides. Use - for stdin.
        #[arg(long)]
        context: Option<String>,
    },
}

#[derive(Debug)]
struct Failure {
    code: &'static str,
    message: String,
    /// What to do next (shown as "hint" in the JSON error).
    hint: Option<String>,
}

impl Failure {
    fn hint(mut self, hint: impl ToString) -> Self {
        self.hint = Some(hint.to_string());
        self
    }
}

type Result<T> = std::result::Result<T, Failure>;

fn failure(code: &'static str, message: impl ToString) -> Failure {
    Failure {
        code,
        message: message.to_string(),
        hint: None,
    }
}

fn read_json(path: &str) -> Result<Value> {
    let mut source = String::new();
    let reader: Box<dyn Read> = if path == "-" {
        Box::new(io::stdin())
    } else {
        Box::new(
            std::fs::File::open(path).map_err(|e| failure("read_error", format!("{path}: {e}")))?,
        )
    };
    reader
        .take(MAX_PROJECT_FILE_BYTES + 1)
        .read_to_string(&mut source)
        .map_err(|e| failure("read_error", format!("{path}: {e}")))?;
    if source.len() as u64 > MAX_PROJECT_FILE_BYTES {
        return Err(failure("read_error", "JSON input exceeds 64 MiB limit"));
    }
    reject_oversized_integers(&source)?;
    serde_json::from_str::<UniqueValue>(&source)
        .map(|v| v.0)
        .map_err(|e| failure("invalid_json", e))
}

// Preserve integer safety before Serde can round out-of-range integer tokens.
fn reject_oversized_integers(source: &str) -> Result<()> {
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                } else if bytes[i] == b'"' {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
        } else if bytes[i] == b'-' || bytes[i].is_ascii_digit() {
            let start = i;
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_digit()
                    || matches!(bytes[i], b'.' | b'e' | b'E' | b'+' | b'-'))
            {
                i += 1;
            }
            let token = &source[start..i];
            if !token.contains(['.', 'e', 'E']) {
                let fits = if token.starts_with('-') {
                    token.parse::<i64>().is_ok()
                } else {
                    token.parse::<u64>().is_ok()
                };
                if !fits {
                    return Err(failure(
                        "data_loss",
                        "Integer token exceeds supported 64-bit range",
                    ));
                }
            }
        } else {
            i += 1;
        }
    }
    Ok(())
}

// Reject duplicate members before serde_json::Value can discard them. This also
// covers nested objects and patches, not only the top-level project.
struct UniqueValue(Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueValue;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("JSON with unique object members")
            }
            fn visit_bool<E: de::Error>(self, value: bool) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(Value::Bool(value)))
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(value.into()))
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(value.into()))
            }
            fn visit_f64<E: de::Error>(self, value: f64) -> std::result::Result<Self::Value, E> {
                serde_json::Number::from_f64(value)
                    .map(|v| UniqueValue(Value::Number(v)))
                    .ok_or_else(|| E::custom("Nonfinite JSON number"))
            }
            fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(value.to_owned())))
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<UniqueValue>()? {
                    values.push(value.0);
                }
                Ok(UniqueValue(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom(format!("Duplicate JSON member: {key}")));
                    }
                    values.insert(key, map.next_value::<UniqueValue>()?.0);
                }
                Ok(UniqueValue(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}

// Serde models may ignore unknown fields. Reject them rather than silently
// discarding authored data during apply or export. Missing/default fields are OK.
fn check_preserved(input: &Value, canonical: &Value, path: &str) -> Result<()> {
    match (input, canonical) {
        (Value::Object(a), Value::Object(b)) => {
            for (key, value) in a {
                let child = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                let other = b.get(key).ok_or_else(|| {
                    failure("unknown_field", format!("Unsupported field at {child}"))
                })?;
                check_preserved(value, other, &child)?;
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Err(failure("data_loss", format!("Array changed at {path}")));
            }
            for (index, (value, other)) in a.iter().zip(b).enumerate() {
                check_preserved(value, other, &format!("{path}/{index}"))?;
            }
        }
        (Value::Number(a), Value::Number(b)) => {
            let equivalent = if a == b {
                true
            } else if let Some(integer) = a
                .as_i64()
                .map(i128::from)
                .or_else(|| a.as_u64().map(i128::from))
            {
                b.as_f64()
                    .is_some_and(|number| number.fract() == 0.0 && number as i128 == integer)
            } else {
                a.as_f64() == b.as_f64()
            };
            if !equivalent {
                return Err(failure(
                    "data_loss",
                    format!("Number cannot be represented exactly at {path}"),
                ));
            }
        }
        _ if input != canonical => {
            return Err(failure("data_loss", format!("Value changed at {path}")));
        }
        _ => {}
    }
    Ok(())
}

fn validated(value: Value) -> Result<Project> {
    if !value.is_object() {
        return Err(failure("invalid_project", "Project must be a JSON object"));
    }
    let project: Project =
        serde_json::from_value(value.clone()).map_err(|e| failure("invalid_project", e))?;
    let canonical =
        serde_json::to_value(&project).map_err(|e| failure("serialization_error", e))?;
    check_preserved(&value, &canonical, "")?;
    project
        .validate()
        .map_err(|e| failure("validation_error", e))?;
    Ok(project)
}

fn load(path: &str) -> Result<Project> {
    validated(read_json(path)?)
}

fn merge_patch(target: &mut Value, patch: Value) {
    match patch {
        Value::Object(object) => {
            if !target.is_object() {
                *target = json!({});
            }
            let target = target.as_object_mut().expect("object initialized");
            for (key, value) in object {
                if value.is_null() {
                    target.remove(&key);
                } else {
                    merge_patch(target.entry(key).or_insert(Value::Null), value);
                }
            }
        }
        other => *target = other,
    }
}

fn atomic_write(path: &str, bytes: &[u8]) -> Result<()> {
    let destination = Path::new(path);
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| failure("write_error", format!("{path}: {e}")))?;
    temporary
        .write_all(bytes)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|e| failure("write_error", format!("{path}: {e}")))?;
    temporary
        .persist(destination)
        .map_err(|e| failure("write_error", format!("{path}: {e}")))?;
    Ok(())
}

fn project_value(project: &Project) -> Result<Value> {
    serde_json::to_value(project).map_err(|e| failure("serialization_error", e))
}

fn save_project(project: &Project, output: &str) -> Result<Value> {
    project
        .validate()
        .map_err(|e| failure("validation_error", e))?;
    if output == "-" {
        let value = project_value(project)?;
        let bytes = serde_json::to_vec(&value).map_err(|e| failure("serialization_error", e))?;
        if bytes.len() as u64 >= MAX_PROJECT_FILE_BYTES {
            return Err(failure("write_error", "Project file exceeds 64 MiB limit"));
        }
        return Ok(value);
    }
    project
        .save_file(output)
        .map_err(|e| failure("write_error", e))?;
    Ok(json!({"ok": true, "output": output, "format": "splinely-project"}))
}

fn demo() -> Result<Project> {
    let project = Project {
        name: "Splinely autonomous demo".into(),
        width: 640.0,
        height: 360.0,
        nodes: vec![
            Node::rect("background", 0.0, 0.0, 640.0, 360.0),
            Node::circle("ball", 80.0, 180.0, 32.0),
            Node::text("label", 32.0, 48.0, "Click the circle"),
        ],
        ..Project::default()
    };
    let mut value = project_value(&project)?;
    value["nodes"][0]["style"]["fill"] = json!({"r":0.06,"g":0.08,"b":0.14,"a":1.0});
    value["nodes"][1]["style"]["fill"] = json!({"r":0.2,"g":0.7,"b":1.0,"a":1.0});
    value["nodes"][2]["style"]["fill"] = json!({"r":1.0,"g":1.0,"b":1.0,"a":1.0});
    value["clips"] = json!([{
        "id":"travel", "name":"Travel", "duration":2.0, "looping":true, "autoplay":true,
        "tracks":[{"nodeId":"ball","property":"x","keyframes":[
            {"time":0.0,"value":{"type":"number","value":0.0},"easing":{"type":"linear"}},
            {"time":1.0,"value":{"type":"number","value":480.0},"easing":{"type":"linear"}},
            {"time":2.0,"value":{"type":"number","value":0.0},"easing":{"type":"linear"}}
        ]}]
    }]);
    value["stateMachines"] = json!([{
        "id":"interaction", "initial":"idle",
        "states":[{"id":"idle","onEnter":[],"overrides":[]},{"id":"clicked","onEnter":[],"overrides":[]}],
        "transitions":[{"from":"idle","to":"clicked","event":{"name":"click","target":"ball"},"guard":null,
            "actions":[{"type":"setProperty","nodeId":"ball","property":"fill","value":{"type":"color","value":{"r":1.0,"g":0.4,"b":0.2,"a":1.0}}}]}]
    }]);
    validated(value)
}

fn schema() -> Result<Value> {
    Ok(json!({
        "format":"splinely-project", "schemaVersion":1,
        "description":"Author JSON using these complete examples. Unknown fields are rejected. All commands validate through splinely_document. Coordinates and times are finite numbers; times are seconds; colors use channels 0..1. IDs must be unique and references must resolve.",
        "encoding":{"fieldNames":"camelCase", "enums":"geometry, easing, animation values and actions are objects tagged by type; properties are strings", "geometry":"node.kind", "translation":"node.transform.x/y (not geometry coordinates)"},
        "model":discovery::model(),
        "minimalTask":{"schemaVersion":1,"name":"Minimal","width":640,"height":360,"nodes":[]},
        "blendClipExample":{"id":"overlay","duration":2,"looping":true,"autoplay":false,"blendMode":"weighted","weight":0.5,"tracks":[]},
        "blendActionExample":{"type":"blendClip","clipId":"overlay","weight":0,"duration":0.25},
        "pathEditing":{"insert":"path-insert INPUT ID --segment N --t 0.5 --output PATH; zero-based L/Q/C segments, M/Z excluded; shape-preserving de Casteljau split","handle":"path-handle INPUT ID --command-index N --handle control1|control2|anchor --x X --y Y --output PATH; parsed commands include M/Z; Q has control1 only","syntax":"M/L/Q/C/Z absolute or relative, normalized to absolute; H/V/S/T/A explicitly rejected","bounds":"Recomputed tight local bounds through core; all unrelated data and originalKind retained"},
        "nodeKinds":["rect","circle","text","path"],
        "nodeMetadata":{"visible":{"default":true},"locked":{"default":false,"meaning":"Editor locking metadata, not a CLI write restriction"},"originalKind":{"default":null,"meaning":"Original rect/circle geometry retained by conversion; restoration requires unchanged path geometry"}},
        "layerOrder":"nodes array is back to front; reorder uses a zero-based final index",
        "conversion":"Rect/circle only; conversion can fail if geometry or existing animation references are incompatible. No references are silently removed.",
        "boneExample":{"id":"upper-arm","parentId":null,"x":320.0,"y":140.0,"rotation":30.0,"length":65.0},
        "childBoneExample":{"id":"forearm","parentId":"upper-arm","x":0.0,"y":0.0,"rotation":20.0,"length":55.0},
        "bindingExample":{"boneId":"upper-arm","transform":{"x":0.0,"y":0.0,"rotation":0.0,"scaleX":1.0,"scaleY":1.0}},
        "boneTrackExample":{"nodeId":"upper-arm","property":"rotation","keyframes":[{"time":0.0,"value":{"type":"number","value":30.0},"easing":{"type":"linear"}}]},
        "boneProperties":["x","y","rotation"],
        "keyframeCommand":{"numericProperties":["x","y","rotation","scaleX","scaleY","opacity","width","height","radius","fontSize"],"target":"Existing node or bone ID; bones support only x/y/rotation; width/height require rect, radius circle, fontSize text","time":"Finite nonnegative seconds; duration grows to max(existing duration, time, 0.001)","replacement":"Same target/property/time replaces only value, retaining easing; insertion is sorted and linear","newClip":"Name equals ID; looping and autoplay false; blendMode override, weight 1; ID must be globally unique","locking":"Explicit document edit; locked editor metadata does not prevent writing"},
        "boneCoordinates":"Root x/y are scene coordinates. Child x/y offset from parent tip; child rotation is relative in degrees. Bound node transform is local to bone root. Bone IDs share the global ID namespace.",
        "properties":["x","y","rotation","scaleX","scaleY","opacity","fill","stroke","width","height","radius","fontSize"],
        "easingExamples":[{"type":"linear"},{"type":"step"},{"type":"cubicBezier","x1":0.42,"y1":0.0,"x2":0.58,"y2":1.0}],
        "valueExamples":[{"type":"number","value":42.0},{"type":"color","value":{"r":1.0,"g":0.0,"b":0.0,"a":1.0}}],
        "audioAssetExample":{"id":"voice","name":"Voice","mimeType":"audio/wav","dataBase64":"UklGRiYAAABXQVZFZm10IBAAAAABAAEAQB8AAIA+AAACABAAZGF0YQIAAAAAAA=="},
        "audioCueExample":{"id":"hello","assetId":"voice","time":1.5,"offset":0.0,"duration":null,"volume":1.0},
        "guardExample":{"variable":"enabled","equals":true},
        "overrideExample":{"nodeId":"ball","property":"opacity","value":{"type":"number","value":0.5}},
        "actionExamples":[{"type":"setProperty","nodeId":"ball","property":"x","value":{"type":"number","value":10.0}},{"type":"playClip","clipId":"motion"},{"type":"stopClip","clipId":"motion"},{"type":"blendClip","clipId":"overlay","weight":0.5,"duration":0.25},{"type":"setVariable","name":"enabled","value":true},{"type":"emit","event":{"name":"GO","target":null}},{"type":"playAudio","assetId":"voice"}],
        "mergePatch":{"standard":"RFC 7396","objects":"recursively merge","null":"remove member","arrays":"replace entire array","otherValues":"replace target; result must validate as a project"},
        "limitations":["Schema 1 only; rect/circle/text/path geometry", "rawSvg is preserved data, not rendered", "Evaluation does not run JS, state actions or audio", "customJs is preserved; export executes it only with --trusted-js"],
        "examples":{"empty":project_value(&Project::default())?,"demo":project_value(&demo()?)?,"rig":project_value(&bones::rig_demo()?)?,"complete":discovery::complete_example()?}
    }))
}

fn run(cli: Cli) -> Result<Value> {
    match cli.command {
        Command::Schema => schema(),
        Command::Capabilities => discovery::capabilities(),
        Command::BlendClip {
            input,
            output,
            clip,
            weight,
            mode,
        } => blending::run(&input, &output, &clip, weight, mode),
        Command::PathInsert {
            input,
            id,
            output,
            segment,
            t,
        } => paths::insert(&input, &output, &id, segment, t),
        Command::PathHandle {
            input,
            id,
            output,
            command_index,
            handle,
            x,
            y,
        } => paths::handle(&input, &output, &id, command_index, handle, x, y),
        Command::Keyframe {
            input,
            output,
            clip,
            target,
            property,
            time,
            value,
            by,
            no_start_key,
            color,
            gradient,
            path,
            easing,
            autoplay,
            looping,
        } => {
            let value = match (value.or(by), color, gradient, path) {
                (Some(v), ..) => splinely_document::AnimValue::Number(v),
                (_, Some(c), ..) => splinely_document::AnimValue::Color(
                    shapes::parse_color(&c).map_err(|e| failure("invalid_arguments", e))?,
                ),
                (_, _, Some(spec), _) => splinely_document::AnimValue::Gradient(
                    shapes::parse_gradient(&spec).map_err(|e| failure("invalid_arguments", e))?,
                ),
                (_, _, _, Some(d)) => splinely_document::AnimValue::Path(d),
                _ => {
                    return Err(failure(
                        "invalid_arguments",
                        "Give --value, --color, --gradient or --path",
                    ))
                }
            };
            let options = keyframes::KeyOptions {
                easing,
                autoplay,
                looping,
                relative: by.is_some(),
                no_start_key,
            };
            keyframes::run(
                &input, &output, &clip, &target, property, time, value, options,
            )
        }
        Command::RigDemo { output } => save_project(&bones::rig_demo()?, &output),
        Command::WaterDrop { output } => save_project(&showcase::water_drop()?, &output),
        Command::RandomFigures {
            output,
            seed,
            count,
        } => save_project(&showcase::random_figures(seed, count)?, &output),
        Command::Bone { command } => bones::run(command),
        Command::Plugin { command } => scaffold::run(command),
        Command::ConvertToPath {
            input,
            node,
            output,
        } => vectors::convert(&input, &node, &output, false),
        Command::ConvertBack {
            input,
            node,
            output,
        } => vectors::convert(&input, &node, &output, true),
        Command::Layer { command } => vectors::run(command),
        Command::Convert { input, output } => save_project(&load(&input)?, &output),
        Command::Example { output } => save_project(&demo()?, output.as_deref().unwrap_or("-")),
        Command::New {
            output,
            demo: use_demo,
            width,
            height,
            name,
            background,
        } => {
            let mut project = if use_demo {
                demo()?
            } else {
                Project::default()
            };
            if let Some(w) = width {
                project.width = w;
            }
            if let Some(h) = height {
                project.height = h;
            }
            if let Some(name) = name {
                project.name = name;
            }
            if let Some(colour) = background {
                let fill =
                    shapes::parse_color(&colour).map_err(|e| failure("invalid_arguments", e))?;
                if project.nodes.iter().any(|n| n.id == "background") {
                    return Err(failure(
                        "invalid_arguments",
                        "The demo already has a background node",
                    ));
                }
                let mut node = Node::rect("background", 0.0, 0.0, project.width, project.height);
                node.name = "Background".into();
                node.style.fill = fill;
                project.nodes.insert(0, node);
            }
            save_project(&project, &output)
        }
        Command::Guide { json: as_json } => {
            if as_json {
                Ok(json!({"ok": true, "guide": guide::TEXT}))
            } else {
                let _ = io::stdout().write_all(guide::TEXT.as_bytes());
                std::process::exit(0);
            }
        }
        Command::Check { input, strict } => lint::run(&input, strict),
        Command::Batch {
            input,
            steps,
            output,
        } => batch::run(&input, &steps, output.as_deref()),
        Command::Clip { command } => clips::run(command),
        Command::On {
            input,
            event,
            target,
            play,
            stop,
            set,
            machine,
            from,
            to,
            output,
        } => clips::on(
            &input, &output, &event, target, play, stop, set, &machine, from, to,
        ),
        Command::ImportSvg { input, output } => {
            let source = std::fs::read_to_string(&input).map_err(|e| failure("read_error", e))?;
            let project = Project::from_svg(&source).map_err(|e| failure("unsupported_svg", e))?;
            save_project(&project, &output)
        }
        Command::Validate { input } => {
            let project = load(&input)?;
            Ok(json!({"ok":true,"schemaVersion":project.schema_version}))
        }
        Command::Inspect { input } => {
            let project = load(&input)?;
            Ok(
                json!({"ok":true,"schemaVersion":project.schema_version,"name":project.name,
                "width":project.width,"height":project.height,
                "nodes":project.nodes.iter().map(|n| json!({"id":n.id,"name":n.name,"kind":n.kind,"visible":n.visible,"locked":n.locked,"originalKind":n.original_kind})).collect::<Vec<_>>(),
                "bones":project_value(&project)?["bones"],
                "clips":project.clips,"stateMachines":project.state_machines,
                "variables":project.variables,"audioAssets":project.audio_assets.iter().map(|a| json!({"id":a.id,"name":a.name,"mimeType":a.mime_type,"base64Length":a.data_base64.len()})).collect::<Vec<_>>(),
                "audioCues":project.audio_cues,
                "hasCustomJs":!project.custom_js.is_empty(),"hasRawSvg":!project.raw_svg.is_empty()}),
            )
        }
        Command::Export {
            input,
            output,
            trusted_js,
        } => {
            let project = load(&input)?;
            let svg = project
                .to_svg_with_options(
                    RUNTIME,
                    BrowserExportOptions {
                        trusted_js,
                        audio_controls: true,
                    },
                )
                .map_err(|e| failure("export_error", e))?;
            if output == "-" {
                Ok(json!({"ok":true,"format":"svg","svg":svg}))
            } else {
                atomic_write(&output, svg.as_bytes())?;
                Ok(json!({"ok":true,"output":output,"format":"svg","bytes":svg.len()}))
            }
        }
        Command::Apply {
            input,
            patch,
            output,
        } => {
            if input == "-" && patch == "-" {
                return Err(failure(
                    "invalid_arguments",
                    "Only one of INPUT and PATCH may read stdin",
                ));
            }
            let mut value = project_value(&load(&input)?)?;
            merge_patch(&mut value, read_json(&patch)?);
            save_project(&validated(value)?, &output)
        }
        Command::Evaluate {
            input,
            time,
            context,
        } => evaluation::run(&input, time, context.as_deref()),
        Command::Node { command } => shapes::run_node(*command),
        Command::Arrange { command } => shapes::run_arrange(command),
        Command::Render {
            input,
            output,
            time,
            width,
            fps,
            duration,
            clip,
        } => shapes::render(&input, &output, time, width, fps, duration, clip.as_deref()),
    }
}

fn main() {
    splinely_document::crash::install("cli", env!("CARGO_PKG_VERSION"), || "en".to_owned(), false);
    let args = assist::prepare_args(std::env::args_os().collect());
    let parsed = assist::command()
        .try_get_matches_from(&args)
        .and_then(|matches| <Cli as clap::FromArgMatches>::from_arg_matches(&matches));
    let result = match parsed {
        Ok(cli) => run(cli),
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp
                    | clap::error::ErrorKind::DisplayVersion
                    | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            ) =>
        {
            // Ignore a closed pipe (`splinely-cli --help | head`).
            let _ = write!(io::stdout(), "{error}");
            return;
        }
        Err(error) => Err(assist::argument_failure(&args, &error)),
    };
    match result {
        Ok(value) => {
            let stdout = io::stdout();
            let mut out = stdout.lock();
            if let Err(error) = serde_json::to_writer(&mut out, &value)
                .map_err(io::Error::other)
                .and_then(|_| out.write_all(b"\n"))
            {
                report(failure("stdout_error", error));
            }
        }
        Err(error) => report(error),
    }
}

fn report(error: Failure) -> ! {
    let hint = error
        .hint
        .clone()
        .or_else(|| assist::default_hint(error.code));
    let mut body = json!({"code":error.code,"message":error.message});
    if let Some(hint) = hint {
        body["hint"] = json!(hint);
    }
    eprintln!("{}", json!({"ok":false,"error":body}));
    std::process::exit(1);
}
