# Author Forma with another LLM

Copy [prompts/forma-author.md](prompts/forma-author.md) into another LLM and add
your scene requirements. The prompt sends the model to `forma-cli guide` first:
a short workflow with tested recipes, coordinate rules and the usual mistakes.
Agents then build with commands (not hand-written JSON), run `check` (lint with
runnable fixes), look at `render` previews and export. An LLM without terminal
access can return the commands for you to run; it must not claim it tested them.

Why results stay good:

- **Forgiving input:** `--output` defaults to the input; aliases such as `--cx`,
  `--r`, `--color`, `--node/--nodes`; `node update FILE ID`.
- **Self-correcting errors:** JSON errors carry a `hint` – closest IDs for
  typos, the valid flags for unknown ones, the usage line, the next command.
- **No silent no-ops:** a new keyframe track starts from the current value,
  `keyframe` warns when its clip never plays, `check` finds hidden/off-canvas
  shapes, unreadable text, dead clips and states.
- **Atomic multi-step work:** `batch` applies all steps or none.

## Local setup — no download required

From this installed repository:

```sh
export TMPDIR=/home/user/svg_editor/.work-tmp
mkdir -p "$TMPDIR"
# Use the existing binary; rebuild locally if absent or stale.
cargo build --offline --locked -p forma-cli
CLI=target/debug/forma-cli
"$CLI" guide
"$CLI" --help
"$CLI" schema > "$TMPDIR/forma-schema.json"
"$CLI" capabilities > "$TMPDIR/forma-capabilities.json"
"$CLI" new --output "$TMPDIR/project.frm" --width 800 --height 450 --background white
```

Use an already installed `forma-cli` on trusted PATH instead if appropriate;
check its help/schema because installed versions may differ. Do not invent a
release URL, fetch code or install dependencies as an authoring step. Builds/tests
use `--offline --locked`; if cached dependencies are missing, report the blocker
rather than downloading or changing the lockfile.

## Feature checklist

The returned schema is the authority for field names/defaults/templates; actual
`validate` is the authority for acceptance. It is a format guide, not formal JSON
Schema. All canonical document fields are writable as JSON or with `apply`.

| Feature | Authoring route |
| --- | --- |
| Canvas/name/schema version | `new`, JSON, `apply` |
| Rect/circle/text/path and local geometry | `node add/update/delete/duplicate/list`, JSON/`apply`, restricted `import-svg` |
| Transforms, RGBA fill/stroke, opacity/stroke width, linear/radial gradients | `node update` (colours as hex, CSS name or rgb(); `--gradient`), JSON/`apply` |
| Align, distribute, flip, move, scale, rotate, group, stacking order, boolean union/difference/intersection/exclusion | `arrange …` (authored pose, same logic as the editor) |
| Polygon, star, line | `node add --kind polygon\|star\|line` (stored as paths) |
| Gradient and path-data (morph) animation | `keyframe --property gradient --gradient …`, `arrange morph`, `keyframe --property d --path …` |
| Clips: duration, autoplay, loop, easing | `clip list/add/update/delete`, `keyframe --autoplay --loop --easing` |
| Interactions (click/hover/keys → play, stop, set, switch state) | `on --event … --target … --play/--stop/--set/--to` |
| Lint with fixes; atomic multi-step edits | `check [--strict]`, `batch --steps` |
| Back-to-front order, visible/locked, originalKind | `layer`, conversion, JSON; locking is editor metadata |
| Path point/handle insertion/editing | `path-insert`, `path-handle`; M/L/Q/C/Z only |
| Bone hierarchy/binding/local animation | `bone`, `rig-demo`, JSON/`apply`; no runtime IK field |
| Numeric and colour tracks; linear/step/ease/cubicBezier | `keyframe --value/--by/--color --easing`, JSON/`apply` |
| Override/weighted clips and blendClip actions | `blend-clip`, JSON/`apply`; independent sampling via context |
| Parallel state machines, states, guards, overrides, events, every action | `on` for events/actions/states; guards, overrides and other actions via JSON/`apply` |
| Scalar typed variables | JSON/`apply` |
| Embedded audio assets/cues/playAudio | JSON/`apply`, installed local `embed-speech` |
| Browser speech cues/speak | JSON/`apply`; separate export `--speech` opt-in |
| customJs/rawSvg | Preserved JSON; rawSvg inert; JS runs only with explicit trusted export |
| Trusted native plugin source | `plugin new`; source-only, editor-only, not a project field |
| Seeded/deterministic starter scenes | `random-figures`, `water-drop`, `rig-demo`, `new --demo` |
| Validation, inspection, independent clocks/weights, SVG | `validate`, `inspect`, `evaluate --context`, `export` |
| Visual self-check | `render --time S --output shot.png` (static PNG), `render --output anim.gif` (animated GIF); no scripts |

## Review and verify

1. Give the LLM the prompt, requested features and local discovery outputs.
2. Prefer commands (or `batch`); write JSON only for features without commands,
   to a new file, then `validate`. Do not overwrite a good
   project with an unvalidated draft. `apply` validates before atomic replacement;
   arrays replace entirely, so retain all unrelated array entries/references.
3. `check` and fix all warnings; `render` previews and inspect them; `inspect`,
   then `evaluate` representative times and contexts. Explicit context
   states/clip times/weights are snapshots; transitions/actions/fades are not run.
4. Export without trust flags first. Review authored JavaScript before explicitly
   choosing `--trusted-js`; choose `--speech` independently only when wanted.
5. Browser-test interactive SVG directly or via `<object>`, not `<img>`. Audio
   requires a user gesture; browser voices/offline speech are not guaranteed.
   Core evaluation cannot establish browser playback, audible speech or trust.

```sh
"$CLI" apply "$TMPDIR/project.json" --patch authored.patch.json --output "$TMPDIR/project.json"
"$CLI" validate "$TMPDIR/project.json"
"$CLI" inspect "$TMPDIR/project.json"
"$CLI" evaluate "$TMPDIR/project.json" --time 0.5
# Optional context JSON uses states, clips:[{clipId,time}], clipWeights, overrides.
"$CLI" export "$TMPDIR/project.json" --output "$TMPDIR/project.svg"
TMPDIR=/home/user/svg_editor/.work-tmp cargo test --offline --locked -p forma-cli
```

Outputs are JSON (help/version excepted); errors are JSON stderr with nonzero
exit. `--output -` emits project JSON, but SVG export wraps its string in JSON.
See [CLI.md](CLI.md) for commands/protocol and [BLENDING_SPEC.md](BLENDING_SPEC.md)
for weighted ordering and runtime fade semantics.
