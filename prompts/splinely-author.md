# Copy-paste prompt: Splinely CLI author

You are authoring a Splinely project (animated, optionally interactive SVG) with my
**existing local repository and its `splinely-cli`**, not an imagined SDK. My
requirements follow at the end. If you have a terminal, build, run and verify;
otherwise return the exact commands/files for me to run and say they are untested.

## 1. Get oriented (always first)

```sh
export TMPDIR=/home/user/svg_editor/.work-tmp; mkdir -p "$TMPDIR"
cargo build --offline --locked -p splinely-cli      # only if target/debug/splinely-cli is missing/stale
CLI=target/debug/splinely-cli
"$CLI" guide                                     # workflow, recipes, coordinates, pitfalls
"$CLI" COMMAND --help                            # exact flags of a command you are about to use
```

Use `schema`/`capabilities` only when you need a field the commands do not
cover (bones, blending, audio, speech, guards/variables). No downloads, remote
installers or new dependencies; on missing cached crates report the blocker.

## 2. Build with commands, not hand-written JSON

- `new --output F --width W --height H --background COLOUR` – background first
  (nodes are drawn back to front).
- `node add F --kind rect|circle|text|path|polygon|star|line --id NAME …` with
  `--fill/--stroke/--stroke-width/--opacity/--gradient`; give every shape a
  meaningful ID. Text x/y = left baseline; centre with `arrange align`.
- Layout: `arrange align|distribute|move|scale|rotate|flip|order|group`,
  shapes: `arrange boolean`, `arrange morph`, `convert-to-path`.
- Animation: `keyframe F --clip C --target ID --property P --time T`
  with `--value`, `--by` (relative), `--color`, `--gradient` or `--path`;
  `--easing ease-in-out` etc.; `--autoplay --loop` on the clip. A new track
  starts from the current value automatically. Use `--pivot center` (node add/
  update) before rotating or scaling around the centre.
- Clips: `clip list|add|update|delete`. Interactions: `on F --event click
  --target ID --play CLIP --set ID.fill=red [--from S --to S]`.
- Many steps: `batch F --steps steps.txt` (all or nothing).
- `--output` is optional for edits (saves in place). Every edit validates;
  on failure nothing is written – read `error.hint`, fix, retry. Never bypass
  validation, invent fields or fudge path bounds.
- Hand-written JSON/`apply` (RFC 7396; arrays replace entirely) only for
  features without commands; keep all unrelated entries and validate.

## 3. Verify before you report

1. `"$CLI" check F` – fix every warning (each finding has a runnable `fix`).
2. `"$CLI" render F --time T --output shot.png` at start/middle/end (and
   `--output anim.gif`); if you can view images, LOOK at them and correct
   layout, colours and motion. `evaluate F --time T` gives exact values.
3. `"$CLI" export F --output scene.svg` without trust flags.

## Rules that stay non-negotiable

- `customJs` runs only with `export --trusted-js` after I explicitly approve
  reviewed code; `rawSvg` is preserved, never rendered. Browser speech needs
  `export --speech` and system voices; `embed-speech` uses a trusted local
  eSpeak only. Audio playback needs a user gesture – never fake one.
- `plugin new` writes editor-only native source; never build/run it without my
  approval.
- Unsupported: images, filters, masks, clipping, arbitrary SVG appearance,
  runtime IK fields. Say so and offer an approximation with shapes/paths.
- Do not claim browser playback, audio or speech worked from CLI output alone.

Return: the commands you ran (or for me to run), output paths, `check` result,
which preview images you inspected, unsupported requests and any trust/speech
decision. Ask only for genuinely missing requirements.

## My scene/task requirements

[Replace this line with the scene, size, animation, interactions, output paths
and any explicit trust/speech choices.]
