# Splinely CLI — autonomous project creation

`splinely-cli` is a local, provider-independent interface for AI tools. It uses the
same `splinely_document` model and validation as the editor. No API key, network,
GUI or JavaScript execution is needed to create/edit projects or export SVG.

```sh
cargo build --offline --locked -p splinely-cli
# executable: target/debug/splinely-cli
cargo test --offline --locked -p splinely-cli
```

**Start here (people and LLM agents):** `splinely-cli guide` prints a short
workflow with copy-paste recipes (scene, animation, interaction, batch) and
the common mistakes. Its recipes are executed verbatim by the test suite.

- Edit commands save back to their input when `--output` is omitted
  (validated, atomic). `export`, `render`, `convert` and `import-svg` still need
  `--output`.
- Every error is JSON with `code`, `message` and a `hint` naming the next step:
  closest IDs for typos ("Did you mean 'sun'?"), the valid flags for an unknown
  flag, the usage line for missing arguments.
- `check FILE` reports valid-but-wrong projects (shapes hidden behind an opaque
  rectangle, outside the canvas, unreadable text, clips that never play, …), each
  with a ready-to-run `fix` command; `--strict` turns warnings into an error.
- `batch FILE --steps steps.txt` runs many commands on one project, all or
  nothing.

## Single-file authored projects

Use **`.sply`** for the self-contained authored Project: geometry, animations,
states, bones, code, SVG-source metadata and embedded audio all live in one file.
It is strict UTF-8 canonical Project JSON, not ZIP or a binary container.
Legacy `.splinely.json` and `.json` remain supported with the same schema and strict
unknown-field, duplicate-member and integer rules. No sidecar SVG is needed for
authoring; `.sply` needs a compatible host to render, or an explicit SVG export.
Loading/saving/conversion never executes authored JavaScript or grants trust.
Editor/viewer dialogs and startup loading are not changed by this CLI/core work.
See [docs/sply-format.md](docs/sply-format.md) for format, limits and shared APIs.

```sh
splinely-cli new --demo --output scene.sply
splinely-cli example --output example.sply
splinely-cli import-svg drawing.svg --output drawing.sply
splinely-cli validate scene.sply
splinely-cli convert legacy.splinely.json --output scene.sply
splinely-cli convert scene.sply --output interoperable.json
splinely-cli export scene.sply --output scene.svg
```

Every project authoring operation accepts `.sply` input/output paths, including
atomic in-place edits. `convert` validates and canonicalizes Projects, not
exported SVG; it preserves supported authored values, not original JSON text.
Input `-` and `--output -` keep the JSON streaming protocol. Outputs remain
explicit to avoid surprising sibling-file overwrites. Project file reads/saves
are bounded to 64 MiB; CLI patch/context JSON inputs and project stdout outputs
also use this limit. SVG export is not a project-file save and has no such limit.

## Commands and protocol

Every successful command emits one JSON value followed by a newline on stdout.
Errors emit **only** JSON on stderr and exit with status 1:

```json
{"ok":false,"error":{"code":"not_found","message":"No node with ID 'Sun'","hint":"Did you mean 'sun'? Existing node IDs: background, sun, ground"}}
```

Error codes include `invalid_arguments`, `read_error`, `invalid_json`,
`invalid_project`, `validation_error`, `unknown_field`, `data_loss`, `write_error`,
`export_error`, `evaluation_error`, `invalid_context`, `invalid_time`, `not_found`,
`conversion_error`, `check_failed` and `stdout_error`. `hint` is always present
for these codes except `stdout_error`.
`--help` and `--version` are human-readable exceptions.

| Command | Result |
| --- | --- |
| `guide` | Short Markdown workflow with recipes and pitfalls; read first |
| `check INPUT [--strict]` | Lint a valid project: findings with level, code, message and a runnable `fix` |
| `batch INPUT --steps FILE [--output PATH]` | Run many edit commands, all or nothing |
| `clip list\|add\|update\|delete INPUT …` | Manage clips: duration, `--autoplay`, `--loop`, name, easing of all keys |
| `on INPUT --event E [--target NODE] [--play CLIP] [--stop CLIP] [--set N.P=V] [--from S] [--to S]` | Add an interaction (state-machine transition) |
| `schema` | Complete canonical field/default guide, action/value templates and validated project examples (not formal JSON Schema) |
| `capabilities` | Features, limitations, model and command/argument metadata sourced from the CLI parser |
| `import-svg INPUT --output PATH` | Import supported editable vectors; reject unsupported appearance/resources |
| `keyframe INPUT --clip ID --target ID --property PROPERTY --time N (--value N \| --by N \| --color C \| --gradient SPEC \| --path D) [--easing E] [--autoplay] [--loop]` | Insert/replace a key (numeric, colour, gradient, path); creates the clip if needed and a start key at 0 s for new tracks |
| `blend-clip INPUT --clip ID [--weight 0..1] [--mode weighted\|override] --output PATH` | Set existing clip's authored blend settings; defaults weighted/1; does not activate it |
| `path-insert INPUT ID --segment N [--t 0.5] --output PATH` | Shape-preserving split of a zero-based L/Q/C segment (M/Z excluded) |
| `path-handle INPUT ID --command-index N --handle control1\|control2\|anchor --x N --y N --output PATH` | Edit a normalized path command handle and recompute bounds |
| `new --output PATH [--demo]` | Valid empty/demo project saved to PATH; JSON acknowledgement |
| `example [--output PATH]` | Complete demo Project JSON on stdout by default; optionally save `.sply`/JSON atomically |
| `convert INPUT --output PATH` | Validate/canonicalize between `.sply`, `.splinely.json` and `.json`; no container conversion or SVG import |
| `validate PATH` | Validates JSON, schema, references and numerical constraints; `{ "ok": true, "schemaVersion": 1 }` |
| `inspect PATH` | Dimensions, nodes, clips, machines, variables, audio summaries and code-presence flags |
| `apply INPUT --patch PATCH --output PATH` | RFC 7396 merge patch, followed by full validation and atomic output |
| `export INPUT --output SVG [--trusted-js]` | Interactive SVG containing project metadata, runtime and embedded assets |
| `evaluate INPUT [--time SECONDS] [--context JSON_PATH]` | Evaluate with optional states, ordered independent clip clocks, weights and final overrides; no side effects |
| `rig-demo --output PATH` | Visible Pivot-style skeleton with bound vector shapes and a looping bone clip |
| `water-drop --output PATH` | Deterministic polished blue-ball scene with looping drop squash/stretch, shadow and impact ripple |
| `random-figures --output PATH [--seed U64] [--count 3..12]` | Seeded geometric gallery with captions and an automatic sequential state cycle (defaults: seed 42, count 6) |
| `bone add INPUT --id ID --length N [--parent ID] [--x N --y N --rotation N] --output PATH` | Add a root or child bone |
| `bone update INPUT --id ID [--parent ID \| --root] [--x N --y N --rotation N --length N] --output PATH` | Update only supplied fields |
| `bone bind INPUT --node ID --bone ID --output PATH` | Bind without changing the node's authored local transform |
| `bone keyframe INPUT --bone ID --clip ID [--property x\|y\|rotation] --time N --value N --output PATH` | Insert/replace a numeric keyframe in an existing clip (default property: rotation) |
| `convert-to-path INPUT --node ID --output PATH` | Convert rect/circle geometry to a path, retaining the original primitive |
| `convert-back INPUT --node ID --output PATH` | Restore an unmodified converted path to its original primitive |
| `layer reorder INPUT --node ID --index N --output PATH` | Move a node to a zero-based final index, back to front |
| `layer hide INPUT --node ID [--hidden false] --output PATH` | Hide a node, or show it again |
| `layer lock INPUT --node ID [--locked false] --output PATH` | Set/clear editor locking metadata |
| `node add INPUT --kind rect\|circle\|text\|path\|polygon\|star\|line [--id ID] [geometry] [style] --output PATH` | Add a shape on top; returns its `id` (default `node-N`). Polygon/star/line become editable paths |
| `node update INPUT ID [geometry] [style] [--pivot center]` | Change only the given fields (ID also as `--node`); options that do not fit the shape kind are rejected |
| `node delete INPUT --nodes A,B --output PATH` | Delete shapes plus their tracks, state overrides, property actions and targeted events |
| `node duplicate INPUT --nodes A,B [--dx 10 --dy 10] --output PATH` | Copy shapes and their animation tracks (`ID-copy-N`) |
| `node list INPUT` | Shapes back to front: kind, name, group, bone, flags, colours and authored scene bounds |
| `arrange align INPUT --nodes … --edge left\|hcenter\|right\|top\|vcenter\|bottom [--to selection\|document] --output PATH` | Align to the common bounds (one node: to the document) |
| `arrange distribute INPUT --nodes … --axis horizontal\|vertical --output PATH` | Equal gaps between ≥3 shapes; outermost shapes stay |
| `arrange flip INPUT --nodes … --axis horizontal\|vertical --output PATH` | Mirror around the common centre (also rotated and bone-bound shapes) |
| `arrange move INPUT --nodes … [--dx N] [--dy N] --output PATH` | Move by a scene offset; bone-bound shapes move visually by the same amount |
| `arrange scale INPUT --nodes … --sx N [--sy N] [--anchor-x X --anchor-y Y] --output PATH` | Scale around a point (default: centre). Non-uniform only for shapes rotated by multiples of 90° |
| `arrange rotate INPUT --nodes … --degrees N [--pivot-x X --pivot-y Y] --output PATH` | Rotate clockwise around a point (default: centre) |
| `arrange boolean INPUT --nodes A,B,… --op union\|difference\|intersection\|exclusion --output PATH` | Replace ≥2 shapes with one path that keeps curves; returns its `id`. Difference = bottom-most minus the others |
| `arrange morph INPUT --source A --target B [--duration 1] --output PATH` | Make A a morph-ready path, add an autoplay clip morphing A into B's outline (as placed on the canvas), hide B; returns `clip` |
| `arrange group INPUT --nodes … [--name NAME] --output PATH` | Group nodes under their common group (nested groups); returns the new group `id`. Members are made contiguous in the stacking order |
| `arrange ungroup INPUT --nodes … --output PATH` | Dissolve the outermost group of each node (one level) |
| `arrange order INPUT --nodes … --to front\|forward\|backward\|back --output PATH` | Change stacking order, keeping the moved shapes' relative order |
| `render INPUT --output PNG [--time S] [--width PX]` | Static PNG preview at a time (resvg, no scripts, local fonts) |
| `render INPUT --output ANIM.gif [--fps 15] [--duration S] [--width PX] [--clip ID]` | Looping animated GIF of the autoplay clips (or only `--clip`) (default length: longest autoplay clip; 1..50 fps, max 600 frames, white background) |
| `plugin new --name NAME --output DIR [--editor-path DIR]` | Generate trusted native editor-only source; never build or execute |

Use `-` for an input or patch to read stdin. Only one stream may consume stdin
per invocation. Use `--output -` for project JSON on stdout; SVG export instead
returns `{ "ok": true, "format": "svg", "svg": "<svg ...>..." }` so stdout
remains JSON. `example` without a file output has no acknowledgement wrapper. File output commands
return `{ "ok": true, "output": "...", "format": "..." }` (export also reports
byte count). Parent directories must already exist.

## Concrete autonomous workflow

An AI can request `schema`, author JSON from its examples, validate it, apply
changes, inspect/evaluate the result, and export. Treat any nonzero exit as a
failure; read stderr diagnostics and correct the proposal before retrying.

```sh
CLI=target/debug/splinely-cli
"$CLI" schema > format-guide.json
"$CLI" example > draft.sply
"$CLI" validate draft.sply
"$CLI" inspect draft.sply
"$CLI" evaluate draft.sply --time 0.5

# An AI writes this patch, with no SDK or provider dependency.
printf '%s\n' '{"name":"AI animation","variables":{"author":"agent"}}' > patch.json
"$CLI" apply draft.sply --patch patch.json --output final.sply
"$CLI" validate final.sply
"$CLI" export final.sply --output final.svg

# stdin authoring and in-place editing are also supported.
printf '%s\n' '{"schemaVersion":1,"name":"Minimal","width":640,"height":360,"nodes":[]}' |
  "$CLI" validate -
printf '%s\n' '{"name":"Revised"}' |
  "$CLI" apply final.sply --patch - --output final.sply
```

`schema` emits a usable format guide plus fully validated examples, **not** a
formal JSON Schema. Project fields are camelCase. Shapes are tagged by
`kind.type` (`rect`, `circle`, `text`, `path`); geometry, easing, animation values and actions use a `type` tag; track properties are strings. Colors have
`r/g/b/a` channels in 0..1. Coordinates are SVG units; times are seconds. Track
properties `x`/`y` animate transform translation rather than geometry position.
The demo contains all three shape kinds, a looping motion clip, and a click
state machine. Use its objects as templates. Schema version 1 is supported;
`schema.model` covers every supported field/default, property, easing and action,
including blending, bones, audio and evaluation contexts. `capabilities`
provides the same model with actual command metadata. See [LLM.md](LLM.md) and
[prompts/splinely-author.md](prompts/splinely-author.md) for an authoring checklist and
a copy-paste prompt for another LLM.

## Autonomous animation showcases

```sh
CLI=target/debug/splinely-cli
"$CLI" water-drop --output water-drop.sply
"$CLI" export water-drop.sply --output water-drop.svg --trusted-js
"$CLI" random-figures --seed 42 --count 6 --output random-figures.sply
"$CLI" export random-figures.sply --output random-figures.svg --trusted-js

# Rebuild the checked-in JSON, SVG and HTML pairs via the real CLI.
python3 scripts/generate-showcase.py
# Optional: --output /tmp/splinely-showcase --seed 123 --count 8
# Original anime character: CLI import-svg → apply → validate → export
python3 scripts/generate-anime-demo.py
python3 scripts/anime-smoke.py # Firefox: trusted Hallo/replay, bone motion
# Open examples/showcase.html (includes anime-hello.html).
# If file-origin restrictions interfere, serve this checkout over localhost:
python3 -m http.server 8000
```

The new authoring commands only create **validated canonical schema-1 projects**;
there is no hidden export or JS execution. The generated controller
is reviewable in `customJs` and runs only with `export --trusted-js`. These SVG exports embed all scene data and runtime;
the HTML wrappers load the adjacent SVG via **`object`, never `img`**, using only
local relative resources. Both the SVG and HTML offer keyboard-accessible Play /
resume and Pause controls. HTML reads `object.contentDocument.documentElement.splinely`,
observes `state.change`, and reports machine state, timeline time and
diagnostics. It never drives frames. CSP or local
file-origin restrictions may block scripting/observation; direct SVG or localhost
are alternatives. Script-capable `object` is not a sandbox for untrusted projects.

### Assumptions and supported effects

- Water drop uses a fixed 2.8-second canonical clip. Numeric translation,
  scale squash/stretch, opacity, layered sphere lighting, shadow and an outlined
  ripple provide shape effects. **No path-`d` morph tracks or SVG filters are
  claimed**: those are not canonical animation properties. Core `evaluate` can
  sample this deformation without JS. Drop/highlight share coordinated tracks.
- Randomness happens at authoring time: an explicitly wrapping 64-bit LCG chooses
  circles, rounded squares, regular polygons, sizes and motion amplitudes.
  Palette/layout are curated rather than unrestricted. Identical seed/count
  produce identical JSON; count is bounded to 3..12. Seed is saved as a decimal
  string variable so the full u64 range remains exact in browser JavaScript.
- The gallery begins in `idle`. A user Play gesture sends
  `START` through the existing machine API. Then the **sole playing nonloop clip**
  ends every 4.8 seconds and canonical untargeted `clip.end` transitions visit
  `figure-0 → figure-1 → … → figure-0`. Each transition stops the old clip and
  the next state's entry plays its clip and reveals its caption. This
  is sequential reachability, not unchecked random state selection. Do not add
  concurrent nonloop clips without distinguishing their completion events:
  canonical event targets name nodes, not clip IDs.
- State overrides reveal only the active caption/figure; no persistent explicit
  visibility patches mask later states. Pause holds the current sample and
  resume continues it, including a completion arriving at the pause boundary.
  An initial `START` is required once; subsequent cycling is automatic.
- Without trusted JS, drop animation still works but authored controls do not;
  the gallery stays idle until a trusted host sends `START` itself.

Verification (Node is required for the additional runtime integration tests):

```sh
cargo test --offline --locked -p splinely-cli
cargo build --offline --locked -p splinely-cli
node --test crates/splinely_cli/tests/showcase-runtime.test.cjs
```

Tests cover canonical round-trips, seed repeatability/variation, loop continuity,
independent opt-ins, invalid-count atomicity, all-state automatic cycling and
wraparound, pause/resume and wall-clock completion boundaries.
The generated pairs were also checked in Firefox: object mounting, trusted Play
and Pause, automatic visitation of all six gallery states and wraparound,
direct-file SVG Start, and local-file wrapper observation.

## Patching and safety

- Merge patch is RFC 7396: objects recursively merge; `null` deletes an object
  member; arrays and scalar values replace the target entirely. To edit one
  node or keyframe, provide the **complete replacement array**, not an ID map.
- Example clearing animations: `{"clips":[]}`. Removing nodes may also require
  replacing tracks, machines and other references to retain a valid project.
- Input must already validate. A patch cannot mask an invalid original input.
- Unknown fields (including nested fields) and duplicate JSON members are
  errors, not silently dropped. Integers that lose precision when converted to
  model floating-point fields are rejected with `data_loss`; integer tokens outside the supported signed/unsigned 64-bit range are rejected before parsing; decimal numbers
  follow the model's IEEE-754 double precision. Store scalar metadata in
  `variables` with nonempty names, not unsupported model fields.
- Missing defaultable fields are filled by the model. Deleting such a field
  with `null` restores its default when serialized; `null` cannot express a
  literal null object member in merge patch. A top-level null/scalar/array
  replacement cannot validate as a project and fails.
- Every output file uses a synced temporary file in the destination directory
  and atomic replacement. Invalid JSON, unknown fields, invalid references or
  model constraints fail before writing. In-place apply is supported; failures
  leave the destination unchanged before commit. A `stdout_error` after commit
  can mean the file was successfully saved but its acknowledgement could not
  be delivered; inspect the file before retrying. There is no multi-writer locking, so callers
  should serialize edits. Atomic replacement is not a power-loss durability
  guarantee for the directory entry and does not preserve old file permissions.

## Export and evaluation limits

Exports are fully embedded; the shared browser runtime is compiled into the
binary. Open SVG directly or in a script-capable browser context (not `<img>`).
CSP/browser restrictions may disable scripts; audio needs a user gesture. `customJs` is preserved but
not executed by default. `--trusted-js` explicitly adds a host bootstrap that executes authored code with the instance-local `splinely` API. Only enable this for reviewed, trusted projects; dynamic compilation may be blocked by CSP. The bundled runtime needs no network. If a host enables
arbitrary JavaScript, review it or use an isolated context; it may use a network.

`rawSvg` is preserved metadata, not injected/rendered. Only rect/circle/text
and path geometry is currently supported. Evaluation is the core's deterministic
animation evaluator, not a browser: it does not execute custom JS, audio
or state-machine side effects. Time must be finite and nonnegative.
The authored `.sply` Project (or compatible legacy JSON), not exported SVG, is
the authoritative editable document.

## Shapes, arrangement and PNG previews

`node` and `arrange` reuse the editor's document operations (`splinely_document`),
so CLI and editor give identical results. Arrangement works on the **authored
pose** (time 0, no clips or state overrides) and edits base transforms. Locked
nodes are rejected. Every command validates and saves atomically; on error the
input file is unchanged.

Geometry options: `--x --y` (rect/text position, circle centre), `--width --height
--rx` (rect), `--radius` (circle), `--text --font-size` (text), `--d --evenodd`
(path, bounds are computed); for `node add` only: `--sides` (polygon, default 6),
`--tips --inner` (star, default 5 / 0.45), `--x2 --y2` (line end; `--x --y` is the
start, polygon/star use `--x --y --radius` as centre and radius). Style/transform options: `--name --fill --stroke
--stroke-width --opacity --tx --ty --rotation --scale-x --scale-y --group --gradient`.
`--gradient "linear 90 navy gold@0.7 white"` (angle optional, 0 = left to right) or
`--gradient "radial white orange"` sets a gradient fill (stops evenly spaced unless
`@offset`); `--gradient none` removes it. Gradients replace the fill colour in exports.
Colours accept `#RRGGBB`, `#RRGGBBAA`, CSS names (`tomato`) and `rgb()/rgba()`;
`--stroke none` removes the stroke, `--group ""` leaves a group.

```sh
CLI=target/debug/splinely-cli
"$CLI" new --output poster.sply
"$CLI" node add poster.sply --kind rect --x 40 --y 40 --width 200 --height 120 --fill tomato --output poster.sply
"$CLI" node add poster.sply --kind circle --id sun --x 600 --y 120 --radius 60 --fill gold --output poster.sply
"$CLI" node add poster.sply --kind text --text "Hallo" --font-size 48 --x 300 --y 400 --output poster.sply
"$CLI" arrange align poster.sply --nodes node-1,sun --edge top --output poster.sply
"$CLI" arrange align poster.sply --nodes node-2 --edge hcenter --output poster.sply   # centre on the canvas
"$CLI" arrange group poster.sply --nodes node-1,sun --name Kopf --output poster.sply
"$CLI" node list poster.sply
"$CLI" render poster.sply --output poster.png --width 800
```

Boolean operations keep curves: outlines are split at their exact (subdivision)
intersections and written back as `L`/`C` commands. They work in scene space and
keep the bottom-most shape's style, gradient, layer position and group.

Animated gradients: `keyframe --property gradient --gradient SPEC` keys the whole
gradient (kind and stop count must match the node's gradient); angle, stop
offsets and colours interpolate. Morphing: `arrange morph` normalises two
outlines to the same structure (cubic segments, matched subpaths, start point
and winding); `keyframe --property d --path D` adds further keys of that
structure. Both also run in the exported browser runtime. Text cannot take part (convert it in an external tool first).
The result is a normal path that every editor tool, export and runtime supports.

Groups live in `project.groups` (`id`, `name`, `parentId`) and nodes reference
their direct group via `group`. `node list` also returns `groupPath` (outermost
first) and all groups; exports wrap members in nested `<g id="…">` elements.
`import-svg` keeps `<g id="…">` as groups and approximates linear/radial
gradient fills (stops and direction). `convert-to-path` also outlines text with
local system fonts. Ruler guides are stored in `project.guides`.

`node list` reports approximate text bounds (0.6 × font size per character).
`render` is a quick visual check for scripts and AIs: it renders the same static
SVG as an export at `--time` (autoplay clips and initial states applied), without
JavaScript, audio or network access. Fonts come from the local system.

## Bones, vectors and layers

Root bone x/y are scene coordinates. Child x/y offset from the parent's tip;
child rotation is relative in degrees. A bound node's transform is local to the
bone root. Bone IDs share the global ID namespace. Missing parents, cycles,
invalid lengths and dangling bindings fail validation before any write.
Keyframes target bone IDs via track `nodeId`; only numeric `x`, `y`, `rotation`
are supported. Times must lie within the existing clip duration. Insertion sorts
by time; replacing an existing time uses linear easing. CLI numeric bone
arguments reject nonfinite or lossy integer values.

```sh
splinely-cli rig-demo --output rig.sply
splinely-cli bone update rig.sply --id right-forearm --rotation -45 --output rig.sply
splinely-cli bone keyframe rig.sply --bone right-forearm --clip wave \
  --time 1 --value -90 --output rig.sply
splinely-cli convert-to-path rig.sply --node torso-shape --output rig.sply
splinely-cli convert-back rig.sply --node torso-shape --output rig.sply
splinely-cli layer reorder rig.sply --node head --index 11 --output rig.sply
splinely-cli layer hide rig.sply --node background --output rig.sply
splinely-cli layer lock rig.sply --node torso-shape --output rig.sply
```

Conversion retains `originalKind` (default `null`) and preserves IDs, transforms,
styles, visibility and bindings. Only positive-size rect/circle geometry is
convertible; text and existing paths fail. Reverse conversion requires exactly
unchanged generated path geometry (including bounds and fill rule). Incompatible
animation/state references fail full-project validation; none are silently
removed. All project edits support stdin input, stdout output and atomic in-place
writes under the same protocol as `apply`.

Layer order is the `nodes` array, back to front. `visible` defaults to `true`;
`locked` defaults to `false`. Locking is editor metadata, **not** an access-control
or CLI write restriction: explicit CLI conversion, binding and layer operations
can still edit locked nodes. Hide/lock preserve every other field.

## Trusted static plugin scaffolds

```sh
splinely-cli plugin new --name preview-decoration --output ./preview-decoration
# Only after reviewing all generated source and dependencies:
cargo test --manifest-path ./preview-decoration/Cargo.toml
```

`plugin new` generates `Cargo.toml`, `src/lib.rs`, and `README.md` into a **new**
directory. The parent must already exist; existing destinations are never
replaced. Names start with a lowercase ASCII letter and use only lowercase
letters, digits, hyphens or underscores. `--editor-path` points at a local
`splinely_editor` crate and defaults to this checkout; the generated manifest stores
its absolute path and is an independent Cargo workspace. The acknowledgement uses
`format: "rust-editor-plugin"`, `editorOnly: true`, and `executed: false`.

The source implements the public `EditorPlugin` trait and exposes
`register(&mut splinely_editor::ecs::Projection) -> Result<(), String>`, which calls
`Projection::add_plugin`. A trusted native host must explicitly link the source
crate and call registration. Duplicate plugin types are rejected. The example
uses a disabled-by-default `Settings` resource and `Phase::Decorate`; it only
changes disposable preview nodes, not JSON or browser/SVG exports.

Native plugins are **trusted static code with full process privileges**, not a
sandbox or dynamic loading mechanism. The CLI does not build, run, install,
fetch or automatically register generated code. Manual Cargo commands can run
build scripts and tests; review dependencies too. Project JSON and browser
JavaScript cannot request native plugin loading. See the generated README for
registration, lifecycle and export limitations.

## Guided authoring: `guide`, `check`, `batch`, `clip`, `on`

```sh
splinely-cli guide                       # Markdown; --json wraps it
splinely-cli check scene.sply             # {"warnings":N,"findings":[{level,code,message,fix}],"summary":…}
splinely-cli batch scene.sply --steps steps.txt [--output other.sply]
splinely-cli clip list scene.sply         # duration, autoplay, loop, tracks with key times, startedBy
splinely-cli clip add scene.sply --id pulse --duration 0.6 [--autoplay] [--loop]
splinely-cli clip update scene.sply --id pulse --autoplay --loop false --easing ease-out
splinely-cli clip delete scene.sply --id pulse   # refuses while actions still use it
splinely-cli on scene.sply --event click --target sun --play pulse --set sun.fill=red
splinely-cli on scene.sply --event click --target lamp --from off --to on --set lamp.opacity=1
```

`batch` steps are one command per line (or a JSON array of strings or argument
arrays) without the program name, the project path and `--output`; `#` starts a
comment and a trailing `\` continues a line. `new` (or another starter) may be
the first step. Every step runs on a private copy; the result is saved only if
all steps succeed, otherwise the error names the failing step and nothing
changes. Steps cannot read stdin.

`on` adds a transition to a state machine (default `interactions`, created with
an `idle` state). Without `--from/--to` it stays in the machine's initial state,
so the reaction repeats on every event. `--set NODE.PROPERTY=VALUE` accepts
colours for fill/stroke and numbers otherwise. Built-in events: click, dblclick,
pointerenter, pointerleave, pointerdown, pointerup, pointermove, wheel, keydown,
keyup, focus, blur, dragstart, drag, dragend, clip.end; other names only fire
via `emit` actions or host code (the result carries a `warning`).

`new` accepts `--width`, `--height`, `--name` and `--background COLOUR` (a
full-size rectangle `background` as the bottom layer). `node add|update` accept
`--pivot center`, which moves the local origin to the shape's centre without
changing its look, so rotation and scale (also animated) pivot there. Common
aliases are accepted: `--cx/--cy` for `--x/--y`, `--r` for `--radius`,
`--w/--h`, `--size` for `--font-size`, `--color` for `--fill`, `--node`/`--nodes`
and `--id` for node IDs; `node update FILE ID` also takes the ID positionally.
`render --clip ID` previews a clip that is not autoplay.

## Keyframes (`keyframe`)

```sh
splinely-cli keyframe project.sply --clip motion --target ball --property x --time 0.5 --value 120
splinely-cli keyframe project.sply --clip motion --target ball --property y --time 1 --by -80 --easing ease-out --autoplay --loop
splinely-cli keyframe project.sply --clip motion --target ball --property fill --time 1 --color "#ff8800"
splinely-cli keyframe rig.sply --output rig.sply --clip pose --target upper-arm --property rotation --time 1 --value -30
```

Value flags: `--value` (numbers), `--by` (number relative to the target's
current static value), `--color` (fill/stroke), `--gradient` (gradient) and
`--path` (d). `--easing linear|step|ease|ease-in|ease-out|ease-in-out|cubic-bezier(x1,y1,x2,y2)`
sets the transition from this key to the next. `--autoplay`/`--loop` (bare =
true, or `true`/`false`) set the clip flags in the same call.

When a key at a time > 0 creates a **new track**, a key at 0 s holding the
current static value is added first (same easing), so a single command already
animates from the current look; `--no-start-key` disables this. The result
lists `hints`: the added start key, a pivot reminder for rotation/scale on
shapes whose origin is not their centre, and a reminder when the clip is
neither autoplay nor started by an interaction.

This is an explicit document edit: editor `locked` metadata does not prohibit
keyframe changes. Lock flags and all unrelated project data remain intact.
Input may be `-` for stdin; `--output -` emits the validated project JSON.
Other outputs use validated atomic saves, including in-place edits. Invalid
input, IDs, properties or values leave existing output files unchanged.

`--target` must identify an existing node or bone. `--clip` selects an existing
clip or creates one with that globally unique, valid project ID, its ID as name,
`looping: false` and `autoplay: false`. Creating a clip does not activate it or
change state machines. Duration becomes `max(existing duration, time, 0.001)`
seconds; other clip settings and tracks are preserved.

Numeric properties use schema camelCase: `x`, `y`, `rotation`, `scaleX`, `scaleY`,
`opacity`, `width`, `height`, `radius`, `fontSize`. Bones support only `x`, `y`,
`rotation`. Width/height require rectangles, radius circles, and fontSize text.
Fill/stroke take `--color`; gradient and d take `--gradient`/`--path`.
Time must be finite and nonnegative. Opacity is in 0..1; geometry values are
in 0..10000000; other numeric values are within +/-10000000. Rotation is in
degrees. Translation edits transform x/y, not primitive geometry coordinates.

The command finds or adds the target/property track and inserts keys in time
order, with linear easing for new keys unless `--easing` is given. At an
existing equal time (including signed zero), it replaces the value and keeps
the key's easing unless `--easing` is given.
It never appends duplicate keys or property tracks. JSON keyframe values use
`{"type":"number","value":120}`; `splinely-cli schema` and `keyframe --help`
describe the supported properties and edit behavior.

## Weighted clips and independent evaluation clocks

```sh
CLI=target/debug/splinely-cli
"$CLI" blend-clip project.sply --clip walk --mode weighted --weight 0.75 --output project.sply
# Author actions through apply: {"type":"blendClip","clipId":"walk","weight":0,"duration":0.25}
```

`blend-clip` edits only an existing clip's `blendMode`/`weight`; it does not play,
restart, stop or create clips. Document defaults remain `override` and weight 1;
the command defaults to `weighted` and weight 1. Full validation precedes atomic
saving (including in-place). Switching to override fails if a `blendClip` action
still references that clip. Weight is finite in 0..1. Blend action duration is
finite and nonnegative (default 0); it requires a weighted clip and changes a
runtime weight/ramp, not activation. Fade-to-zero is not stop; weights do not
change audio volume. See [BLENDING_SPEC.md](BLENDING_SPEC.md).

Adjacent weighted clips form a group; override clips flush it. For each property,
with baseline B and sum of contributing weights S, the result is
`B*max(0,1-S) + sum(sample_i*weight_i/max(1,S))`. Missing tracks contribute no
weight; zero weights preserve baseline, positive stroke uses transparent absent
baseline. Numbers/RGBA blend; rotation is scalar, not shortest-angle interpolation.
Bones blend locally before forward kinematics. State overrides precede clips;
explicit evaluation overrides follow clips.

`evaluate --context context.json` accepts strict camelCase JSON, for example:

```json
{
  "states": {"interaction":"clicked"},
  "clips": [{"clipId":"travel","time":0.5}, {"clipId":"overlay","time":1.25}],
  "clipWeights": {"overlay":0.25},
  "overrides": [{"nodeId":"ball","property":"opacity","value":{"type":"number","value":0.9}}]
}
```

These IDs exist in `schema.examples.complete`. Missing states use initial states.
Missing/null `clips` uses autoplay clips at `--time` (default 0); `[]` disables all
clips. Explicit ordered clip entries use independent finite nonnegative times:
looping times wrap, nonloop times clamp. Missing weights use authored weights.
All supplied weight IDs/ranges validate, even inactive and override clips;
override-mode weights do not affect sampling. Duplicate playback entries are
accepted by core and sampled repeatedly; normally supply each clip once.
Overrides use the document property/type/range constraints, including bone
x/y/rotation. Unknown fields, duplicate JSON keys and lossy integer conversions
fail; malformed context types report `invalid_context`, invalid references/ranges
report `evaluation_error`. `--context -` reads stdin, but cannot be combined with
input `-`. Evaluation never writes or mutates the project and does not execute
state entry actions, transitions, JS, audio or fade ramps. To check a fade
midpoint supply its effective weight explicitly; this is a snapshot, not a
browser playback simulation.

Path authoring commands accept M/L/Q/C/Z (absolute or relative, normalized to
absolute); H/V/S/T/A are rejected, not approximated. `path-insert --t` must be
strictly between 0 and 1. Handle command indices include M/Z; Q supports only
control1, C supports control1/control2, and applicable commands support anchor.
Edits recompute tight bounds and preserve unrelated data and `originalKind`;
changed geometry generally prevents `convert-back`.
