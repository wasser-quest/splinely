#![recursion_limit = "256"] // Complete discovery templates expand through serde_json::json!.

use clap::{Parser, Subcommand};
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{json, Value};
use splinely_document::{BrowserExportOptions, Node, Project, MAX_PROJECT_FILE_BYTES};
use std::io::{self, Read, Write};
use std::path::Path;

mod a11y;
mod assist;
mod batch;
mod blend_spaces;
mod blending;
mod bones;
mod clips;
mod discovery;
mod evaluation;
mod follow;
mod guide;
mod isolated_preview;
mod keyframes;
mod lint;
mod look;
mod paths;
mod plugins;
mod scaffold;
mod shapes;
mod showcase;
mod vectors;

const RUNTIME: &str = include_str!("../../../runtime/src/splinely.js");

#[derive(Parser)]
#[command(
    name = "splinely-cli",
    version,
    about = "Create, validate, edit and export Splinely .sply projects (binary by default; strict JSON also accepted)",
    after_help = "New here (human or LLM)? Run `splinely-cli guide` first: a short workflow with recipes.\nEvery edit validates before an atomic save; --output defaults to the input file.\nProjects load from binary .sply or JSON by sniffing (also from stdin); a file ending in .sply is saved as binary, other extensions as JSON.\nResults are JSON on stdout; errors are JSON on stderr with code, message and hint."
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
        #[arg(long, required_unless_present = "after")]
        event: Option<String>,
        /// Instead of an event: switch automatically after this many seconds in --from (timer).
        #[arg(long, conflicts_with_all = ["event", "target"], value_parser = bones::number)]
        after: Option<f64>,
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
        /// How the --to state looks: NODE.PROPERTY=VALUE; repeatable. Unlike
        /// --set these fade in over --blend and fade back when the state is left.
        #[arg(long = "state", allow_hyphen_values = true)]
        state_set: Vec<String>,
        /// Seconds to crossfade from the old state into the new one (0..60).
        #[arg(long, default_value_t = 0.0, value_parser = bones::number)]
        blend: f64,
        #[arg(long)]
        output: String,
    },
    /// Bind a node origin to a path; animate motionProgress with keyframe.
    MotionPath {
        input: String,
        #[arg(long)]
        target: String,
        #[arg(long, required_unless_present = "remove", conflicts_with = "remove")]
        path: Option<String>,
        #[arg(long, default_value_t = 0.0, value_parser = bones::number)]
        progress: f64,
        #[arg(long)]
        orient: bool,
        #[arg(long)]
        remove: bool,
        #[arg(long)]
        output: String,
    },
    /// Make a shape or bone follow the pointer (eyes, heads, arms). Works together
    /// with clicks, hover and every state machine.
    Follow {
        input: String,
        /// Node or bone that follows.
        #[arg(long, alias = "node")]
        target: String,
        /// offset: move towards the pointer (pupils); rotate: turn towards it (head).
        #[arg(long, default_value = "offset")]
        mode: String,
        /// Offset: maximum distance in SVG units (default 10); rotate: maximum degrees (default 30).
        #[arg(long, value_parser = bones::number)]
        max: Option<f64>,
        /// Share of the pointer distance or angle, 0..1.
        #[arg(long, default_value_t = 0.2, value_parser = bones::number)]
        strength: f64,
        /// Seconds to settle (0 = immediate, up to 10).
        #[arg(long, default_value_t = 0.15, value_parser = bones::number)]
        smoothing: f64,
        /// Rotate: direction the target faces at rest, degrees (0 = right, 90 = down).
        #[arg(long, default_value_t = 0.0, value_parser = bones::number)]
        rest_angle: f64,
        /// Scene point to measure from as X,Y (default: the target's center or bone root).
        #[arg(long)]
        origin: Option<String>,
        /// Only follow while this state machine is in one of --states.
        #[arg(long)]
        machine: Option<String>,
        /// Comma-separated states of --machine.
        #[arg(long, value_delimiter = ',')]
        states: Vec<String>,
        /// Follower ID (default: follow-TARGET).
        #[arg(long)]
        id: Option<String>,
        /// Remove the follower(s) of --target (or --id) instead of adding one.
        #[arg(long)]
        remove: bool,
        #[arg(long)]
        output: String,
    },
    /// Screen-reader data for the web player and SVG export: title/description, node labels,
    /// roles, decorative nodes and state announcements. Without options it reports what is set.
    A11y {
        input: String,
        /// Accessible title of the whole scene (the project name).
        #[arg(long)]
        title: Option<String>,
        /// Description of the scene, or of --node (empty text removes it).
        #[arg(long)]
        description: Option<String>,
        /// Node to label (needed for --label, --role, --hidden).
        #[arg(long, alias = "target")]
        node: Option<String>,
        /// Accessible name of --node; default for interactive nodes is its name or ID.
        #[arg(long)]
        label: Option<String>,
        /// button, link, img, group or heading (empty text removes it).
        #[arg(long)]
        role: Option<String>,
        /// true: decorative, hidden from screen readers; false: visible again.
        #[arg(long, value_parser = clap::value_parser!(bool))]
        hidden: Option<bool>,
        /// MACHINE.STATE=TEXT: text a live region announces when the state is entered; repeatable.
        #[arg(long = "state-announce")]
        state_announce: Vec<String>,
        /// Where to save the result (default: the input).
        #[arg(long)]
        output: Option<String>,
    },
    /// Emit a format guide and complete valid examples as JSON.
    Schema,
    /// List supported document features, commands, model, examples and limitations as JSON.
    Capabilities,
    /// Author and inspect two-parameter blend spaces (does not activate clips).
    BlendSpace {
        #[command(subcommand)]
        command: blend_spaces::BlendSpaceCommand,
    },
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
    /// Convert a strict project between binary .sply and JSON; never imports exported SVG.
    /// The output format follows the extension (.sply = binary, else JSON) unless --format is given.
    Convert {
        input: String,
        #[arg(long)]
        output: String,
        /// Force the output encoding: json or binary (default: by --output extension).
        #[arg(long, value_enum)]
        format: Option<Encoding>,
    },
    /// Print the canonical JSON of any project file (binary .sply or JSON) on stdout.
    Cat {
        /// Project path, or - for stdin.
        input: String,
    },
    /// Import SVG as editable paths, text and gradients with a structured import report
    /// (rejects unsupported appearance, external resources and malformed images).
    ImportSvg {
        input: String,
        #[arg(long)]
        output: String,
        /// Fail (nothing is written) when anything would be approximated or dropped.
        #[arg(long)]
        strict: bool,
        /// Also write the import report JSON to FILE (`-` = stderr). With `--output -`
        /// the report always goes to stderr (stdout carries the project) unless given here.
        #[arg(long, value_name = "FILE")]
        report: Option<String>,
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
        /// Existing node, bone or group ID. Bones and groups support only x, y and rotation.
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
    /// Look at a file without seeing images: every shape in draw order with colour words,
    /// world bounds and centre, plain-English layout (inside / overlaps / left of / mirrored
    /// pairs / hidden behind), animation summary, optional ASCII picture; plus file encoding
    /// facts (format, sizes, CRC) and counts.
    #[command(
        after_help = "Output (JSON): `scene` (canvas, background, nodes back to front with `looksLike`, `bounds` [x,y,w,h], `center`, `visiblePercent`, `inside`), `layout` (sentences), `animation` (clips/tracks/keys, interactions, timers, followers, bones), `ascii` (with --ascii), and the older fields (`file`, `stats`, `nodes`, `clips` …).\nExamples:\n  splinely-cli inspect face.sply --ascii\n  splinely-cli inspect face.sply --node pupil-left --detail full"
    )]
    Inspect {
        /// Project path, or - for stdin.
        input: String,
        /// summary (default): compact; full: transforms, local geometry, every relation and keyframe.
        #[arg(long, value_enum, default_value = "summary")]
        detail: look::Detail,
        /// Describe only this node: its entry (full detail), its relations and its animation.
        #[arg(long)]
        node: Option<String>,
        /// Add an ASCII-art picture: one letter per shape (see `ascii.legend`), '.' = empty.
        #[arg(long)]
        ascii: bool,
        /// Width of the ASCII picture in characters (8..200; rows follow the aspect ratio).
        #[arg(long, default_value_t = 64)]
        cols: usize,
        /// Zoom the ASCII picture to the drawn shapes (ignores a full-canvas background)
        /// instead of the whole canvas; `ascii.area` tells what is shown.
        #[arg(long, requires = "ascii")]
        crop: bool,
    },
    /// Export isolated browser preview HTML; custom JS requires consent in the page.
    IsolatedPreview {
        input: String,
        #[arg(long)]
        output: String,
        /// Also embed a plugin's runtime part (folder or .js). It runs only after the in-page
        /// consent, inside the sandboxed frame; the page lists id, file and sha256.
        #[arg(long = "plugin", value_name = "PLUGIN_DIR_OR_JS")]
        plugins: Vec<String>,
    },
    /// Export a self-contained interactive SVG with embedded runtime.
    Export {
        input: String,
        #[arg(long)]
        output: String,
        /// Execute authored JavaScript in the exported browser SVG. Trusted code only.
        #[arg(long)]
        trusted_js: bool,
        /// Embed a plugin's runtime (browser) part: a plugin folder or a .js file. Repeatable (max 8).
        /// Executable code with the same trust gate as customJs: requires --trusted-js, never a silent drop.
        #[arg(long = "plugin", value_name = "PLUGIN_DIR_OR_JS")]
        plugins: Vec<String>,
        /// How the project is embedded in the SVG: binary (default, compact base64 .sply) or json (legacy text).
        #[arg(long, value_enum, default_value = "binary")]
        embed: Embed,
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
    /// Drop-in plugins: `list`/`validate`/`apply` the declarative editor part; `new` generates trusted native editor-plugin source only (never built or executed). Runtime parts are embedded only by `export --plugin ... --trusted-js`.
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

/// File encoding of a saved project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Encoding {
    Json,
    Binary,
}

impl Encoding {
    fn name(self) -> &'static str {
        match self {
            Encoding::Json => "json",
            Encoding::Binary => "binary",
        }
    }
    /// `.sply` (any case) is binary; every other path is JSON text.
    fn for_path(path: &str) -> Self {
        let sply = Path::new(path)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("sply"));
        if sply {
            Encoding::Binary
        } else {
            Encoding::Json
        }
    }
}

/// How an exported SVG embeds the project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Embed {
    Json,
    Binary,
}

#[derive(Debug)]
struct Failure {
    code: &'static str,
    message: String,
    /// What to do next (shown as "hint" in the JSON error).
    hint: Option<String>,
    /// Structured SVG import report of a refused import (shown as "importReport").
    import_report: Option<Value>,
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
        import_report: None,
    }
}

/// Read at most 64 MiB from a file, or from stdin for `-`.
fn read_bytes(path: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let reader: Box<dyn Read> = if path == "-" {
        Box::new(io::stdin())
    } else {
        Box::new(
            std::fs::File::open(path).map_err(|e| failure("read_error", format!("{path}: {e}")))?,
        )
    };
    reader
        .take(MAX_PROJECT_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| failure("read_error", format!("{path}: {e}")))?;
    if bytes.len() as u64 > MAX_PROJECT_FILE_BYTES {
        return Err(failure("read_error", "Input exceeds 64 MiB limit"));
    }
    Ok(bytes)
}

fn utf8<'a>(bytes: &'a [u8], path: &str) -> Result<&'a str> {
    std::str::from_utf8(bytes).map_err(|_| {
        failure(
            "read_error",
            format!("{path}: stream did not contain valid UTF-8"),
        )
    })
}

/// Strict JSON text (patches, contexts). Binary `.sply` is not accepted here.
fn read_json(path: &str) -> Result<Value> {
    let bytes = read_bytes(path)?;
    if Project::is_binary(&bytes) {
        return Err(failure(
            "invalid_json",
            format!("{path}: expected JSON text but found binary .sply"),
        )
        .hint("Use `splinely-cli cat FILE` to print a binary project as JSON."));
    }
    json_value(utf8(&bytes, path)?)
}

fn json_value(source: &str) -> Result<Value> {
    reject_oversized_integers(source)?;
    serde_json::from_str::<UniqueValue>(source)
        .map(|v| v.0)
        .map_err(|e| failure("invalid_json", e))
}

/// Decode a project's raw value tree from binary `.sply` or JSON text.
fn project_bytes_value(bytes: &[u8], path: &str) -> Result<Value> {
    if Project::is_binary(bytes) {
        splinely_document::decode_to_value(bytes)
            .map_err(|e| failure("invalid_binary", format!("{path}: {e}")))
    } else {
        json_value(utf8(bytes, path)?)
    }
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
    Project::from_value_strict(value).map_err(|e| failure(e.code, e.message))
}

/// Load a project from binary `.sply` or JSON (sniffed; `-` reads stdin).
fn load(path: &str) -> Result<Project> {
    let bytes = read_bytes(path)?;
    validated(project_bytes_value(&bytes, path)?)
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
    save_project_as(project, output, None)
}

/// Save with the encoding implied by the extension unless `encoding` is given.
/// `-` prints JSON on stdout (binary cannot be sent to a terminal/LLM).
fn save_project_as(project: &Project, output: &str, encoding: Option<Encoding>) -> Result<Value> {
    project
        .validate()
        .map_err(|e| failure("validation_error", e))?;
    if output == "-" {
        if encoding == Some(Encoding::Binary) {
            return Err(failure(
                "invalid_arguments",
                "Binary output cannot be written to stdout; give --output FILE.sply",
            ));
        }
        let value = project_value(project)?;
        let bytes = serde_json::to_vec(&value).map_err(|e| failure("serialization_error", e))?;
        if bytes.len() as u64 >= MAX_PROJECT_FILE_BYTES {
            return Err(failure("write_error", "Project file exceeds 64 MiB limit"));
        }
        return Ok(value);
    }
    let encoding = encoding.unwrap_or_else(|| Encoding::for_path(output));
    match encoding {
        Encoding::Binary => project.save_file_binary(output),
        Encoding::Json => project.save_file(output),
    }
    .map_err(|e| failure("write_error", e))?;
    Ok(
        json!({"ok": true, "output": output, "format": "splinely-project", "encoding": encoding.name()}),
    )
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
        "keyframeCommand":{"numericProperties":["motionProgress","x","y","rotation","scaleX","scaleY","opacity","width","height","radius","fontSize"],"target":"Existing node, bone or group ID; bones and groups support only x/y/rotation (a group animates through its rig bones: x/y offset from where it was drawn, rotation around its centre); width/height require rect, radius circle, fontSize text","time":"Finite nonnegative seconds; duration grows to max(existing duration, time, 0.001)","replacement":"Same target/property/time replaces only value, retaining easing; insertion is sorted and linear","newClip":"Name equals ID; looping and autoplay false; blendMode override, weight 1; ID must be globally unique","locking":"Explicit document edit; locked editor metadata does not prevent writing"},
        "boneCoordinates":"Root x/y are scene coordinates. Child x/y offset from parent tip; child rotation is relative in degrees. Bound node transform is local to bone root. Bone IDs share the global ID namespace.",
        "properties":["motionProgress","x","y","rotation","scaleX","scaleY","opacity","fill","stroke","width","height","radius","fontSize"],
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
        Command::MotionPath {
            input,
            target,
            path,
            progress,
            orient,
            remove,
            output,
        } => {
            let mut project = load(&input)?;
            let node = project
                .nodes
                .iter_mut()
                .find(|n| n.id == target)
                .ok_or_else(|| failure("invalid_arguments", format!("unknown node {target}")))?;
            node.motion_path = if remove {
                None
            } else {
                Some(splinely_document::MotionPath {
                    path: path.unwrap(),
                    progress,
                    orient,
                })
            };
            save_project(&project, &output)
        }
        Command::Capabilities => discovery::capabilities(),
        Command::BlendSpace { command } => blend_spaces::run(command),
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
        Command::Plugin { command } => plugins::run(command),
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
        Command::Convert {
            input,
            output,
            format,
        } => save_project_as(&load(&input)?, &output, format),
        Command::Cat { input } => save_project(&load(&input)?, "-"),
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
        Command::A11y {
            input,
            title,
            description,
            node,
            label,
            role,
            hidden,
            state_announce,
            output,
        } => a11y::run(
            &input,
            output.as_deref(),
            a11y::Options {
                title,
                description,
                node,
                label,
                role,
                hidden,
                announce: state_announce,
            },
        ),
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
            state_set,
            blend,
            after,
            output,
        } => clips::on(
            &input,
            &output,
            event.as_deref().unwrap_or(splinely_document::TIMER_EVENT),
            target,
            play,
            stop,
            set,
            &machine,
            from,
            to,
            state_set,
            blend,
            after,
        ),
        Command::Follow {
            input,
            target,
            mode,
            max,
            strength,
            smoothing,
            rest_angle,
            origin,
            machine,
            states,
            id,
            remove,
            output,
        } => follow::run(
            &input,
            &output,
            follow::Options {
                target,
                mode,
                max,
                strength,
                smoothing,
                rest_angle,
                origin,
                machine,
                states,
                id,
                remove,
            },
        ),
        Command::ImportSvg {
            input,
            output,
            strict,
            report: report_path,
        } => {
            let bytes = read_bytes(&input)?;
            let source = utf8(&bytes, &input)?;
            let outcome = Project::from_svg_report(source).map_err(|f| {
                let mut e = failure("unsupported_svg", &f.error);
                e.import_report = Some(f.report.to_json());
                e
            })?;
            let import_report = outcome.report.to_json();
            if strict && outcome.report.has_losses() {
                let mut e = failure(
                    "svg_import_lossy",
                    format!(
                        "--strict: {} approximated and {} dropped item(s); nothing was written",
                        outcome.report.approximated, outcome.report.dropped
                    ),
                )
                .hint("Run without --strict to import with the listed losses, or remove them from the SVG");
                e.import_report = Some(import_report);
                return Err(e);
            }
            let mut result = save_project(&outcome.project, &output)?;
            match report_path.as_deref() {
                Some("-") => eprintln!("{}", json!({"importReport": import_report})),
                Some(path) => std::fs::write(
                    path,
                    serde_json::to_vec_pretty(&import_report)
                        .map_err(|e| failure("serialization_error", e))?,
                )
                .map_err(|e| failure("write_error", format!("{path}: {e}")))?,
                None if output == "-" => {
                    eprintln!("{}", json!({"importReport": import_report}))
                }
                None => {}
            }
            if output != "-" {
                result["report"] = import_report;
            }
            Ok(result)
        }
        Command::Validate { input } => {
            let project = load(&input)?;
            Ok(json!({"ok":true,"schemaVersion":project.schema_version}))
        }
        Command::Inspect {
            input,
            detail,
            node,
            ascii,
            cols,
            crop,
        } => inspect(
            &input,
            &look::Options {
                detail,
                node,
                ascii,
                cols,
                crop,
            },
        ),
        Command::IsolatedPreview {
            input,
            output,
            plugins,
        } => isolated_preview::export(input, output, plugins),
        Command::Export {
            input,
            output,
            trusted_js,
            plugins,
            embed,
        } => {
            let project = load(&input)?;
            let bundles = plugins::bundles(&plugins, trusted_js)?;
            let svg = export_svg(&project, trusted_js, embed, &bundles)?;
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

/// Project summary plus how the file is encoded (binary header facts, sizes, CRC),
/// and a textual picture of the scene (see `look`).
/// A damaged or hostile binary file fails with the typed `invalid_binary` error.
fn inspect(input: &str, options: &look::Options) -> Result<Value> {
    let bytes = read_bytes(input)?;
    let project = validated(project_bytes_value(&bytes, input)?)?;
    let json_bytes = serde_json::to_vec_pretty(&project)
        .map_err(|e| failure("serialization_error", e))?
        .len()
        + 1;
    let binary_bytes = project
        .to_binary()
        .map_err(|e| failure("serialization_error", e))?
        .len();
    let mut file = json!({
        "format": if Project::is_binary(&bytes) { "binary" } else { "json" },
        "bytes": bytes.len(),
        "schemaVersion": project.schema_version,
        "equivalent": {"jsonBytes": json_bytes, "binaryBytes": binary_bytes},
    });
    if Project::is_binary(&bytes) {
        // Validated by the decoder above, so the 32-byte header is present.
        let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
        let strings = u32_at(16) as usize;
        file["formatVersion"] = json!(format!("{}.{}", u16_at(8), u16_at(10)));
        file["stringCount"] = json!(strings);
        file["sections"] = json!({
            "header": 32,
            "stringOffsets": 4 * (strings + 1),
            "strings": u32_at(20),
            "body": u32_at(24),
        });
        file["crc32c"] = json!(format!("{:08x}", u32_at(28)));
        file["crcOk"] = json!(splinely_document::crc32c(&bytes[32..]) == u32_at(28));
    }
    let tracks: usize = project.clips.iter().map(|c| c.tracks.len()).sum();
    let keyframes: usize = project
        .clips
        .iter()
        .flat_map(|c| &c.tracks)
        .map(|t| t.keyframes.len())
        .sum();
    let view = look::describe(&project, options)?;
    let mut result = json!({"ok":true,"schemaVersion":project.schema_version,"name":project.name,
        "file":file,
        "stats":{"nodes":project.nodes.len(),"bones":project.bones.len(),"clips":project.clips.len(),
            "tracks":tracks,"keyframes":keyframes,"stateMachines":project.state_machines.len(),
            "audioAssets":project.audio_assets.len(),"audioCues":project.audio_cues.len(),
            "followers":project.followers.len()},
        "width":project.width,"height":project.height,
        "nodes":project.nodes.iter().map(|n| json!({"id":n.id,"name":n.name,"kind":n.kind,"visible":n.visible,"locked":n.locked,"originalKind":n.original_kind,"motionPath":n.motion_path})).collect::<Vec<_>>(),
        "bones":project_value(&project)?["bones"],
        "clips":project.clips,"stateMachines":project.state_machines,
        "variables":project.variables,"audioAssets":project.audio_assets.iter().map(|a| json!({"id":a.id,"name":a.name,"mimeType":a.mime_type,"base64Length":a.data_base64.len()})).collect::<Vec<_>>(),
        "audioCues":project.audio_cues,
        "followers":project.followers,"hasCustomJs":!project.custom_js.is_empty(),"hasRawSvg":!project.raw_svg.is_empty(),"svgSourceSync":project.source_sync().as_str()});
    result.as_object_mut().expect("object").extend(view);
    Ok(result)
}

fn export_svg(
    project: &Project,
    trusted_js: bool,
    embed: Embed,
    bundles: &[splinely_document::plugin::RuntimeBundle],
) -> Result<String> {
    let options = BrowserExportOptions {
        trusted_js,
        audio_controls: true,
    };
    let embed = match embed {
        Embed::Json => splinely_document::EmbedFormat::Json,
        Embed::Binary => splinely_document::EmbedFormat::Binary,
    };
    project
        .to_svg_with_plugins(RUNTIME, options, embed, bundles)
        .map_err(|e| failure("export_error", e))
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
    if let Some(report) = &error.import_report {
        body["importReport"] = report.clone();
    }
    eprintln!("{}", json!({"ok":false,"error":body}));
    std::process::exit(1);
}
