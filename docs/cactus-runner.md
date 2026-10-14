# Dune / Stride — cactus runner

Open `examples/cactus-runner.html` or `examples/cactus-runner.svg` directly in a script-capable browser. Both contain the complete vector scene, project metadata and runtime; the HTML embeds the SVG rather than fetching an adjacent object. No server, external library, image, audio or network is needed. Links to the editable JSON and these notes are optional.

Click the scene or press Space to jump. HTML also provides Jump, Pause, Play and Restart. The original ten-bone stickfigure walks through a seeded desert with occasional cacti. This is a motion study: cacti do not cause collisions or game-over.

## Reproduce through the CLI

From the repository root:

```sh
export TMPDIR=/home/user/svg_editor/.work-tmp
mkdir -p "$TMPDIR"
cargo build -p forma-cli
python3 scripts/generate-cactus-runner.py
python3 scripts/runner-smoke.py
```

The generator supports `--cli PATH` and `--output DIR`. It constructs original SVG vectors, runs **import-svg → apply → validate → export → evaluate**, and checks that the compiled exporter preserved weighted clip fields and `blendClip` actions. Imported path geometry is retained in the authored nodes. The patch attaches local transforms, bone bindings, clips, machines and reviewed custom JavaScript. Temporary authoring inputs live under `TMPDIR`.

Export deliberately passes **`--trusted-js`** because the small `customJs` controller handles click/Space, jump completion and transport controls. It does not animate geometry, use timers or access the network. Only use this option for reviewed code; a restrictive CSP can block its dynamic compilation. No speech option is used. `--author-only` writes draft JSON only and makes no export/verification claim.

Generated artifacts:

- `examples/cactus-runner.forma.json`: editable schema-1 project.
- `examples/cactus-runner.svg`: standalone interactive export.
- `examples/cactus-runner.html`: self-contained presentation with inline exported SVG and controls.

## Actual weighted blending and independent layers

The contract is `BLENDING_SPEC.md`. Parallel state machines alone are **not** weighted animation blending.

- `scene` independently tracks `running` / `paused`.
- `runner` independently tracks `ground` / `jumping`.
- Three override clips continuously scroll cacti, trail and distant scenery. Their periods are 24, 8 and 32 seconds. Eight cactus positions use seed 731, a 2880-unit loop and 120 units/second; each offscreen wrap is a step, never a sweep across the runner.
- Adjacent **weighted** clips `walk` and `jump` share nine local bone-rotation channels. Initial weights are 1 and 0. Entering `jumping` plays the 1.1-second jump and runs `blendClip` actions toward walk=0 / jump=1 over 0.24 seconds. `clip.end` sends LAND; entry to `ground` fades back over 0.24 seconds while walking continues.
- Only jump contains torso Y. Its weighted contribution mixes against authored Y=310, exercising the sparse-channel baseline rule. Local poses are mixed before forward kinematics.
- Pause explicitly freezes weight ramps as well as clip playback: runtime ramps intentionally advance on paused clips. Resume restores the remaining fade. Restart resets positions, phases, weights and both machines.

## Installed-browser evidence

`runner-smoke.py` uses the existing standard-library Firefox Marionette harness and real wall time, not a simulated runtime or hand-driven `tick`. Scene activation uses WebDriver element clicks. It independently reconstructs weighted torso rotation, sparse Y and parent/child FK from sampled clip times, weights and DOM transforms.

Verified locally with installed **Firefox 157.0.1** (user agent Firefox/157.0), including file-origin inline HTML and standalone SVG:

- Walking bones and cacti change with elapsed time.
- Real scene click produces simultaneous nonzero walk/jump weights: one captured pair was **0.7375 / 0.2625**.
- At jump time **0.412 s**, weights were **0 / 1** and torso Y was **188.96**, versus ground Y=310. Scroll advanced from **0.504 s** while walking to **0.976 s** near the apex.
- Landing crossfades back, then settles to **1 / 0**, Y=310, with scroll at **1.917 s**.
- Pause holds the crossfade and scene; Play resumes; keyboard jump and Restart work.
- A full wall-time 24-second loop respawns **all eight** cacti offscreen and preserves their authored positions/speed.
- Standalone SVG autoplays, accepts a real scene click, blends into jump and returns to walking while scrolling continues.

Evidence from the passing run:

- `.work-tmp/cactus-cli-build.log`
- `.work-tmp/cactus-generate.log`
- `.work-tmp/cactus-smoke.log`
- `.work-tmp/cactus-runner-telemetry.json` — browser identity, 321 samples, named checkpoints and respawn IDs.
- `.work-tmp/cactus-runner.png` — grounded presentation screenshot.
- `.work-tmp/cactus-runner-jump.png` — airborne screenshot.

The runtime emits one known construction advisory, `customJs is preserved but not executed; attach trusted handlers explicitly`, before the trusted export bootstrap executes the controller. The test retains that advisory in telemetry and rejects any other diagnostic; the observed click/fades demonstrate that the bootstrap actually ran.

No runtime blocker remained in this Firefox run. Chromium was not installed/tested. This demo contains no audio or speech, and no audibility claim is made. Reruns overwrite evidence; exact wall-time sample values vary.
