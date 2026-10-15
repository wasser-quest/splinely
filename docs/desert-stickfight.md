# Sand / Accord

Open [`examples/desert-stickfight.html`](../examples/desert-stickfight.html) locally in Firefox. The HTML contains the complete animated SVG, not an image/object or a network-loaded player. The [SVG](../examples/desert-stickfight.svg) also autoplays by itself. The HTML page has no buttons: the film starts by itself on load, shows a phase label and progress, and the final pose holds. Clicking the scene after the ending replays it (scripting events `film.pause`, `film.play` and `film.replay` remain available).

An original 20-second, non-gory sparring short in Cactus Runner’s warm sand, sage ridge, dark teal ink, copper scarf, rounded-stick visual language. Reed wears rust; Clay wears sea-teal. No licensed character shapes, external assets, audio, or libraries are required. Existing runner/stickfight templates were read, not edited. Runtime, UI, CLI, and README are unchanged.

## Story

| Film time | Clip / state | Action |
|---|---|---|
| 0–3s | approach | Alternating strides, bobbing shoulders, settle into guard |
| 3–7s | punches | Reed jabs, Clay slips; Clay answers, Reed ducks |
| 7–11s | sweep | Low sweeping extension; Clay tucks into a high vault and lands |
| 11–15.5s | impact | Windup, guarded contact, stretched-time push, recoil; cream spark rays, no injury |
| 15.5–20s | accord | Guards lower, friendly fist bump, step apart, relaxed breathing |

## Authoring and runtime

Two ten-bone parent-tip rigs: torso, neck, upper/lower arms and upper/lower legs. Filled rounded limb capsules, joints, heads, eyes, scarves and cuffs are bound to bones. Authored world-space gestures are converted to local joint angles; two-segment leg IK generates reachable stylized stances. This is deliberately not a physics/collision simulation; the sweeping heel can lift and feet can slide during transitions.

All geometry movement is canonical numeric clip tracks. One continuously authored performance is sliced at exact boundaries into five non-looping clips, with matching end/start values for every channel. Native `clip.end` transitions on the `story` state machine stop the completed clip and start the next. The final `finished` state holds the last clip’s evaluated pose. Dunes and cacti drift independently; shadows follow roots; dust ellipses expand, squash and fade, and tapered rays pulse at impact. Slow motion is authored by spacing the contact poses over a longer interval, not changing wall-clock speed.

The small reviewed `customJs` controller only implements transport events, deterministic reset of all five clips, and pausing at the ending. It does not animate geometry or use timers, fetch, remote scripts or audio. Export explicitly opts into this reviewed code using `--trusted-js`; do not use that flag for unreviewed projects. The same transport events (`film.play`, `film.pause`, `film.replay`) can be emitted by host scripts through `splinely.emit(...)`; after the ending a scene click replays.

## Reproduce

From the repository root, with the existing compiled CLI and installed Firefox:

```sh
mkdir -p .work-tmp
TMPDIR="$PWD/.work-tmp" python3 scripts/generate-desert-stickfight.py
TMPDIR="$PWD/.work-tmp" python3 scripts/desert-stickfight-smoke.py
```

Generator options: `--cli PATH`, `--output DIRECTORY`. The generator writes only `desert-stickfight*` examples and temporary authoring data under project `.work-tmp`. It uses real CLI `import-svg → apply → validate → export --trusted-js`, producing original source SVG, editable Splinely JSON, self-contained runtime SVG, and embedded-SVG HTML. No runtime modifications or new dependencies.

## Bounded evidence

The smoke test uses the existing standard-library Marionette harness and actual installed headless Firefox, loading local `file://` documents. It never substitutes seek/tick calls for playback. It checks:

- JSON/SVG/HTML project consistency, 20 bones and five clips totaling 20 seconds; static resource attributes contain no remote references.
- Actual elapsed playback (with pause time excluded), ordered phase progression and the held ending.
- Both fighters’ changing bone poses **and** rendered arm transforms in every phase, parent-tip FK, moving cactus scenery, an airborne vault, and transient dust/spark opacity.
- Auto-start on load without any button or input, exact paused clip/bone/DOM samples via the scripting events, position-preserving resume, replay during playback and click-to-replay after completion, and standalone SVG autoplay/motion.

Evidence is written to `.work-tmp/desert-stickfight-telemetry.json` and genuine Firefox screenshots `desert-stickfight-{approach,punches,sweep,impact,accord,finished}.png`. Screenshots were inspected for the vault, contact and friendly resolution. The runtime’s one pre-bootstrap advisory (`customJs is preserved but not executed; attach trusted handlers explicitly`) is retained and explicitly allowed; any other diagnostic fails the test.

Limits: this is a desktop Firefox smoke test, not a cross-browser/accessibility audit, network interception test, exhaustive event-boundary race proof, or a physical-contact solver. Static resource checks plus reviewed controller source support the offline design; no claim of packet-capture verification is made. Browser profiles and screenshots stay in project `.work-tmp`; screenshots/telemetry are test evidence, not shipped scene assets.
