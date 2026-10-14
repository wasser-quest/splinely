//! `forma-cli guide`: the short, task-oriented manual an LLM (or a person)
//! should read first. Kept compact on purpose; `schema`/`capabilities` hold
//! the exhaustive reference.

pub const TEXT: &str = r##"# Forma CLI – quick guide

Forma projects (.frm, strict JSON) hold vector shapes, animation clips and
interactions; `export` turns them into one self-contained animated SVG.

## Rules that keep results good
1. Every edit validates the whole project and saves atomically. A failed command
   writes nothing – read the JSON error (`code`, `message`, `hint`) and retry.
2. `--output` is optional for edits: the input file is updated in place.
3. IDs: letters, digits, `_ . -`; start with a letter; unique across shapes,
   clips, bones and machines. Give every shape a meaningful `--id`.
4. Nodes are drawn back to front: add the background FIRST (or `new --background`).
5. After building: `check FILE` (finds hidden shapes, clips that never play,
   unreadable text … each with a ready fix), then `render FILE --output a.png`
   and look at the picture if you can view images.
6. Prefer CLI commands over hand-written JSON. For many steps use `batch`.

## Coordinates
- Canvas: `new --width W --height H` (default 800×600), origin top-left, y down.
- rect: `--x --y` top-left, `--width --height`, `--rx` corner radius.
- circle: `--x --y` centre, `--radius`. text: `--x --y` left baseline, `--font-size`.
- polygon/star: `--x --y` centre, `--radius` (`--sides`, `--tips`, `--inner`).
- line: `--x --y` to `--x2 --y2` (give `--stroke` and `--stroke-width`).
- path: `--d "M0 0 L100 0 L50 80 Z"` in local units (bounds computed for you).
- Colours: `#RRGGBB`, `#RRGGBBAA`, CSS names (`gold`), `rgb(…)`/`rgba(…)`.
- Gradients: `--gradient "linear 90 skyblue white"` (angle: 0 = left→right,
  90 = top→bottom), `--gradient "radial white@0 navy@1"`.
- Animation `x`/`y` animate the transform: `--value 0` = where the shape was
  drawn. Easiest: `--by -200` (relative to the current value) = 200 units up.
  rotation in degrees (clockwise), scaleX/scaleY factors, opacity 0..1.
- Rotation and scale turn around the shape's LOCAL ORIGIN (0,0), not its centre.
  For spinning/pulsing shapes add `--pivot center` (on `node add` or
  `node update F ID --pivot center`); the shape looks the same afterwards.

## Recipe: a scene that animates on open
```sh
F=scene.frm
forma-cli new --output $F --width 800 --height 450 --name "Sunrise" --background "#0b1d3a"
forma-cli node add $F --kind circle --id sun --x 400 --y 420 --radius 60 --fill orange --pivot center
forma-cli node add $F --kind rect --id ground --x 0 --y 380 --width 800 --height 70 --fill "#2f5d2a"
forma-cli keyframe $F --clip rise --target sun --property y --time 3 --by -260 --easing ease-out --autoplay --loop
forma-cli keyframe $F --clip rise --target sun --property fill --time 3 --color "#fff6c0"
forma-cli check $F
forma-cli render $F --time 1.5 --output preview.png     # or --output anim.gif
forma-cli export $F --output scene.svg
```
`keyframe` creates the clip if needed and extends its duration. On a new track
it also adds a key at 0 s holding the current value, so ONE keyframe already
animates from the current look to the new value (`--no-start-key` to skip).
`--easing` shapes the motion FROM that keyframe to the next (the start key
uses the same easing). A clip plays only if it is autoplay (`--autoplay`,
`clip update F --id ID --autoplay`) or started by an interaction (`on`).

## Recipe: interaction (no JSON needed)
```sh
forma-cli clip add $F --id pulse --duration 0.6
for P in scaleX scaleY; do
  forma-cli keyframe $F --clip pulse --target sun --property $P --time 0.3 --value 1.3 --easing ease-in-out
  forma-cli keyframe $F --clip pulse --target sun --property $P --time 0.6 --value 1
done
forma-cli on $F --event click --target sun --play pulse --set sun.fill=red
forma-cli on $F --event pointerenter --target ground --set ground.opacity=0.8
```
Events: click, dblclick, pointerenter, pointerleave, pointerdown, pointerup,
keydown, dragstart, drag, dragend, clip.end. `--to STATE`/`--from STATE` build
real state machines (e.g. toggle: `--from off --to on`, then `--from on --to off`).

## Recipe: many steps in one call (all or nothing)
```sh
cat > steps.txt <<'EOF'
new --width 600 --height 400 --background white
node add --kind rect --id card --x 150 --y 100 --width 300 --height 200 --rx 24 --fill "#3355ff"
node add --kind text --id title --x 200 --y 210 --text "Hello" --font-size 48 --fill white
arrange align --nodes card,title --edge hcenter
arrange align --nodes card,title --edge vcenter
keyframe --clip intro --target card --property opacity --time 0 --value 0 --autoplay
keyframe --clip intro --target card --property opacity --time 1 --value 1 --easing ease-out
keyframe --clip intro --target title --property y --time 1 --by -20 --easing ease-out
check
EOF
forma-cli batch card.frm --steps steps.txt
```
Steps omit the program name, the project path and `--output`. If any step
fails, nothing is saved and the error names the step.

## More tools
- Shapes: `node add|update|delete|duplicate|list` (`node update F ID --fill red --tx 20`).
- Layout: `arrange align|distribute|flip|move|scale|rotate|order|group|ungroup`
  (`--nodes a,b,c`; `arrange align F --nodes title --edge hcenter --to document`).
- Shape magic: `arrange boolean --op union|difference|intersection|exclusion`,
  `arrange morph --source a --target b` (shape-to-shape animation),
  `convert-to-path`, `node add --kind star|polygon|line`.
- Clips: `clip list|add|update|delete` (duration, `--autoplay`, `--loop`, `--easing` for all keys).
- Inspect: `node list F` (IDs + bounds), `clip list F`, `inspect F`,
  `evaluate F --time 1.2` (values at a time), `validate F`.
- Look: `render F --time T --output x.png`, `render F --output x.gif [--clip ID]`.
- Advanced JSON: `apply F --patch patch.json` (RFC 7396; arrays are REPLACED
  entirely), `schema` (format + examples), `capabilities` (full reference).
- Speech: `speak --text … --lang de-DE --output probe.wav` to listen, then
  `embed-speech F --text … --lang de-DE --asset-id voice --time 1` (local engine,
  polished WAV, same sound in every browser). `speech-engines` lists engines.
- Bones, blending, audio: see `capabilities` and CLI.md.

## Common mistakes
- Background added last covers everything → `arrange order F --nodes background --to back`.
- Animating `x` to an absolute position: values are transform offsets; use `--by`.
- Spinning/pulsing around a corner instead of the centre → `--pivot center`.
- Clip never plays → add `--autoplay` or an `on … --play` interaction.
- `fill` keyframes on a shape with a gradient do nothing → animate `gradient` instead.
- Text colour equal to the background → `check` reports `low_contrast`.
- Text x/y is the LEFT BASELINE; centre text with `arrange align --nodes box,label --edge hcenter`
  (and `vcenter`), or on the canvas with `--to document`.
- Writing JSON by hand with guessed field names → use commands; unknown fields are rejected.
"##;
