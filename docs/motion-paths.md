# Motion along paths

A node can optionally bind its **local origin (0,0)** to another path node:

```json
"motionPath": { "path": "route", "progress": 0.5, "orient": true }
```

Omit `motionPath` (or use `null`) for the old behavior. `progress` defaults to 0;
`orient` defaults to false. Animate the numeric property `motionProgress` from
0 to 1 using clips/keyframes, weighted blending, state overrides/crossfades or
`setProperty`. Source path transforms and `d` animation are evaluated first.
Motion then adds the sampled scene-space position to the target's translation;
`orient` adds the tangent angle in clockwise degrees to its rotation. Existing
x/y/rotation animation and pointer followers remain additive offsets. Draw the
moving shape around (0,0), not around the path start. Group nesting is cosmetic.

## CLI and editor

```sh
splinely-cli motion-path scene.sply --target ball --path route --orient
splinely-cli keyframe scene.sply --clip travel --target ball --property motionProgress --time 0 --value 0
splinely-cli keyframe scene.sply --clip travel --target ball --property motionProgress --time 2 --value 1 --autoplay
splinely-cli evaluate scene.sply --time 1
splinely-cli export scene.sply --output player.svg
```

In the inspector open **Motion path**, choose a path, set **Path progress** and
optionally **Orient to tangent**. In the timeline choose **Path progress** to
insert keys, or enable Auto-Key before changing the inspector slider. Normal
undo/redo and preview/export use the shared document evaluator. Hidden source
paths are supported. To detach use **None** / `motion-path ... --remove`; first
remove progress tracks, state overrides and actions referencing this binding.
Deleting the source node removes its dependent bindings and progress references.
Invalid edits are rejected without overwriting the project.

## Deliberate limits and numerical contract

- Both source and target must be unbound to bones; the source cannot itself have
  a motion binding. Self-reference, dangling references and chains/cycles fail
  validation. Multiple targets may share one source.
- Source syntax is SVG **M/L/Q/C/Z**, absolute or relative, including implicit
  repeats; other commands must be converted first. Other ordinary paths are
  unaffected. Keep animated source path data within this syntax too.
- Progress is finite and in **0..1**, including both endpoints. Easing overshoot
  is clamped for sampling. A looping clip wraps its clock normally.
- Arc length uses **64 straight chords per segment**, identically in Rust and
  dependency-free JavaScript, not browser-specific SVG geometry APIs. Tangent
  orientation is the sampled chord direction. At corners the incoming chord
  wins; at the final endpoint it uses the last nonzero chord.
- Zero-length segments are ignored. Closed paths include the closing edge;
  disconnected subpaths concatenate lengths **without measuring the jump**.
- A source must have finite, nonzero evaluated length; a collapsed animated
  source (including zero scale) reports an evaluation error/JS diagnostic.
  Budgets count sampled chords, 64 per segment (zero-length ones included),
  independent of the source transform: at most 100,000 per sampled path, and
  at most 100,000 summed over the distinct source paths of a document (each
  source's `d` plus every `d` keyframe and state override targeting it, even
  one that cannot be sampled). The total also holds per evaluation.
  Rust validation and the JS runtime enforce the same numbers. Each source is
  flattened once per evaluation; on paths over 1,024 chords the position is
  found by binary search over the prefix lengths (same chord as the walk).
  This is an approximation, not exact analytic constant-speed integration.
- Orientation rotates around the target's local origin. There is no automatic
  geometry centering, banking, offset-distance editor or bone motion binding.

Tests: `cargo test --workspace`, `node --test runtime/tests/splinely.test.cjs`,
`python3 scripts/motion-smoke.py` (real Firefox, compares browser matrices to
Rust CLI evaluation; requires a built `target/debug/splinely-cli`).
