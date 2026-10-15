# Declarative drop-in plugins (no recompilation)

These plugins are JSON data, not executable extensions. Overview of all plugin kinds,
the trust model and a full walkthrough: [`plugins.md`](plugins.md). Valid plugins are
discovered and automatically activated at editor startup (the editor sidebar section
“Extensions” lists them and keeps load errors visible). Install or edit a manifest and **restart the editor**;
there is no enable step, code compilation, or live reload.

## Installation

The first nonempty environment setting wins:

1. `SPLINELY_PLUGIN_DIR` (the plugin root itself)
2. `$XDG_CONFIG_HOME/splinely/plugins`
3. Linux: `$HOME/.config/splinely/plugins`; Windows: `%APPDATA%\splinely\plugins`; macOS: `~/Library/Application Support/splinely/plugins` (Windows/macOS ungetestet)

Paths are used literally (no shell expansion). Each immediate child directory must
contain `splinely-plugin.json`, for example:

```text
~/.config/splinely/plugins/
  colors/splinely-plugin.json
  basic-shapes/splinely-plugin.json
```

Copy the child directories from `examples/plugins/` into that root. Do not copy the
root `plugins` directory into another `plugins` directory. A missing root is fine.
Malformed plugins are skipped, with persistent diagnostics in the plugin sidebar;
other valid plugins still activate. No recursive directory discovery occurs.

## Manifest

Only `id`, `title`, `tools`, `palette`, and `runtime` are accepted at the top level. `id` and
`title` are required. At least one tool, palette entry or `runtime` part is required. All object
levels reject unknown fields, including fields in shapes, styles, and actions.
The implementation uses `serde_json::Value` and explicit allowlists equivalent to
`deny_unknown_fields`, because the editor has no direct `serde` dependency.
Duplicate JSON object keys (including escaped spellings of the same key) are
also rejected at every level.

Plugin and tool IDs are 1–64 ASCII bytes: start with a letter, then letters,
digits, `_`, `-`, or `.`. Tool and palette IDs share one namespace per plugin.
Titles are nonempty, at most 120 UTF-8 bytes, without control characters.
All copies of a duplicated plugin ID are rejected, not just the later copy.
Plugins are ordered by ID; buttons preserve manifest array order. Discovery and
diagnostics are deterministic for an unchanged directory.

A minimal palette:

```json
{
  "id": "my-colors",
  "title": "My colors",
  "palette": [
    { "id": "blue", "title": "Blue fill", "style": { "fill": "#2563EB" } }
  ]
}
```

A shape template:

```json
{
  "id": "my-shapes",
  "title": "My shapes",
  "tools": [{
    "id": "box", "title": "Add box",
    "action": {
      "type": "addShape",
      "shape": { "type": "rect", "x": 20, "y": 20, "width": 100, "height": 60 },
      "style": { "fill": "#38BDF8", "stroke": "#0F172A", "strokeWidth": 2 }
    }
  }]
}
```

### Tools and shapes

Each tool requires `id`, `title`, and `action`. Only three action types exist:

- `{"type":"addShape","shape":{...},"style":{...}}`: append one node.
  `style` is optional; omitted properties use document defaults. The title becomes
  the node name. The node is visible, unlocked, identity-transformed, and unbound
  to bones. IDs are allocated against all project-level ID namespaces, including
  both nodes and bones; existing nodes are never replaced.
- `{"type":"setStyle","style":{...}}`: patch the selected, unlocked node.
  Missing or locked selections are errors (the UI disables these buttons).
- `{"type":"addBehavior","behavior":{...}}`: merge one declarative clip + state machine (+ optional
  variables and accessibility label) bound to the selected, unlocked node, e.g. “wiggle on hover”.
  Pure project data, so it runs on the Rust and the JavaScript runtime. Fields, limits and exactly
  what is merged: [`plugins.md`](plugins.md#writing-the-editor-part-kind-1-addbehavior). The whole
  candidate passes `Project::validate()`; ids are collision free; at most 256 clips/machines.

Shapes use document geometry names:

| `type` | Required fields | Optional fields |
| --- | --- | --- |
| `rect` | `x`, `y`, `width`, `height` | `rx` (default 0) |
| `circle` | `cx`, `cy`, `radius` | none |
| `path` | `d` (SVG path data) | `evenodd` (boolean, default false) |

Coordinates are finite numbers within ±10,000,000; sizes and corner radius are
within 0–10,000,000. Path data is at most 16 KiB and must parse as valid document
geometry. Path bounds are computed, never trusted from the manifest. Templates
are checked by `Project::validate()` during discovery.

### Styles and palettes

Styles accept only `fill`, `stroke`, `strokeWidth`, and `opacity`. At least one
property is required. Colors are strings `#RRGGBB` or `#RRGGBBAA` (case-insensitive
hex); alpha is the final byte. `stroke: null` removes a stroke. To make the fill
transparent use `"#00000000"`, not `null`. `strokeWidth` is a finite number in
0–10,000,000; `opacity` is in 0–1. Omitted properties remain unchanged.

Each palette entry requires `id`, `title`, and `style`, and behaves exactly like
`setStyle`. It can change several style properties atomically. Style operations
preserve geometry, transforms, bone bindings, names, visibility and locks.
See `examples/plugins/colors/splinely-plugin.json` for a complete color palette and
`examples/plugins/basic-shapes/splinely-plugin.json` for all three template shapes.

### Optional runtime part

`"runtime": {"apiVersion": 1, "entry": "runtime.js", "description": "…"}` names **one JavaScript file in the
same folder** for the browser runtime. The editor and `Project::validate()` never execute it. Discovery only
reads it (UTF-8, ≤ 256 KiB, best-effort lint, SHA-256) and shows entry/hash in the sidebar. It is embedded into an
exported SVG only by `splinely-cli export --plugin DIR --trusted-js` (explicit trust, like `customJs`; **not a
sandbox**). Hook API, trust model and the `hover-wiggle` example: [`plugins.md`](plugins.md).
Command line: `splinely-cli plugin list|validate|apply` (see `CLI.md`).

## Safety and bounds

- The *editor part* contains no scripts, shell commands, Python, JavaScript, WebAssembly, native
  libraries, dynamic imports, or executable hooks. Unknown action types are errors. The only executable
  thing a folder can carry is the opt-in `runtime.entry` file above, which the editor never runs.
- No image fields, file references, URLs, or network fetches. Colors cannot use
  `url(...)`; shapes cannot embed external images. Other files in a plugin folder
  are never opened. This feature does not sanitize preexisting project content.
- At most 128 entries in the root (including non-directory entries); an oversized
  root loads nothing, rather than choosing a filesystem-order-dependent subset.
  Symlink entries/manifests are rejected by metadata checks. Manifests must be
  regular files. These checks are not a filesystem sandbox against concurrent
  replacement between checking and opening: use a user-controlled plugin root
  and do not replace entries during startup.
- Each manifest is at most 65,536 bytes, with a bounded read even if it grows.
  JSON uses serde_json's default recursion limit. There are at most 64 tools and
  64 palette entries per plugin; at most 1,024 actions in the active registry.
  Exceeding the global action limit activates no plugins and produces a diagnostic.
- Candidate projects have at most 10,000 nodes and at most 8 MiB of compact JSON.
  Inputs already over these budgets cannot be edited by plugins. Output sizing
  uses a bounded writer instead of allocating a serialized copy.
- Actions clone the input, apply one operation, validate the whole project with
  `Project::validate()`, and only then return a replacement. Errors leave the
  original untouched. Locked selected nodes cannot be styled. Adding a shape
  does not modify any existing node, even when a locked node is selected.

These are intentionally limited **declarative plugins**, not arbitrary Anki-style
Python add-ons. They cannot implement new editor logic or run arbitrary code.
The separate trusted, compiled ECS `EditorPlugin` API (`ecs.rs`, `plugins.rs`)
remains available for Rust extensions linked into the editor. That API is not a
sandbox and is not used by drop-in discovery.

## Host integration contract

The GUI-free core lives in `splinely_document::plugin` (shared by the editor and the CLI, so
`plugin apply` takes exactly the same candidate path as the editor button).
`crates/splinely_editor/src/extensions.rs` is a thin egui wrapper around it:

```rust,ignore
let mut extensions = splinely_editor::extensions::ExtensionRegistry::load_default();
// In the right sidebar:
match extensions.ui(ui, &project, selected_node_id.as_deref()) {
    Ok(Some(candidate)) => { /* commit via existing history/dirty/projection path (undoable) */ }
    Ok(None) => {}
    Err(message) => { /* display in the editor's visible error/status area */ }
}
```

`ui` returns at most one candidate per frame and never mutates project or history. The host suppresses
commits during playback or other non-editable modes. Errors stay visible in the section (persistent
`last_error`, diagnostics of broken plugins, a `⚠ N` count in the header). Without egui:
`splinely_document::plugin::ExtensionRegistry::{load_directory, load_plugin, plugins, candidate}`
(`candidate(plugin_id, tool_id, project, selected) -> Result<Project, CandidateError>`) and
`splinely_document::plugin::load_runtime_bundle` + `Project::to_svg_with_plugins` for the export.
Do not reload every frame.
