# Additive clips and two-parameter blend spaces

This extends the weighted schema-1 contract in `BLENDING_SPEC.md`; it does not replace legacy override priority or change schemaVersion. Old projects need no migration. `blendSpaces` defaults to `[]` and is omitted when empty; existing clip defaults remain `blendMode: "override"`, `weight: 1`.

## Additive semantics

Set a clip's `blendMode` to `"additive"`. At that clip's ordered position, each tracked channel becomes:

```
result = current + effectiveClipWeight * (sample(localClipTime) - authoredDocumentRestValue)
```

The reference is the immutable node/bone value in the document, **not** the first keyframe, an evaluated state, or the current mixed pose. Editing the rest pose changes the delta. Track samples remain absolute values, so an additive clip matching rest contributes zero. Missing tracks contribute nothing; weight zero preserves absence. Absent stroke uses transparent black as its reference. RGBA is component-wise, not premultiplied; rotations use scalar degrees (no shortest-arc wrapping), scales use scalar differences (not ratios). Local bone deltas apply before FK; motionProgress deltas apply before motion-path projection.

Only numeric and RGBA properties are allowed: no additive path morph or gradient tracks. After **each additive layer**, RGBA, opacity and motionProgress clamp to 0..1, sizes/fontSize clamp at zero; other scalars are not clamped. Additive clips flush any pending contiguous weighted group, just like override boundaries; they are not normalized with neighboring additive layers. States precede clips, explicit overrides follow clips, then followers/FK/motion projection retain their existing behavior.

`blendClip` actions and weight/fade controls now accept weighted **or additive** clips. Linear ramps, interruption, paused/ended fading, stop cancellation, clocks and ordering follow the existing weighted contract. Visual weights never alter audio volume or cue scheduling.

## Two numeric parameters

```json
{
  "variables": {"speed": 1, "turn": 0},
  "blendSpaces": [{
    "id": "locomotion",
    "parameters": ["speed", "turn"],
    "samples": [
      {"clipId": "walk", "position": [0, 0]},
      {"clipId": "run", "position": [2, 0]}
    ]
  }]
}
```

Each sample references an existing **weighted** clip. A clip can belong to only one sample across all spaces. Space IDs share the global document ID namespace. Parameters must be distinct, declared numeric variables; values and sample coordinates must be finite within ±10000000. Each space has 1..256 samples with pairwise separation greater than 0.000001. Booleans/strings/null are not coerced. Invalid inactive clips/spaces and invalid setVariable actions fail validation too.

At an exact sample position its multiplier is 1, all others 0. Else, for Euclidean squared distances `d_i²`, multipliers are `(1/d_i²) / sum(1/d_j²)` (computed with nearest-distance scaling to avoid reciprocal overflow). This is **inverse-distance interpolation, not triangulated barycentric interpolation**. It works for one sample, collinear samples and outside the convex hull, with no extrapolation or special hull snapping. At [1,0] above the multipliers are .5/.5; at [4,0] they are .2/.8.

The multiplier scales the authored/live clip weight **before** ordinary per-property weighted blending. Thus missing channels retain the weighted residual baseline; an inactive sample can reduce total contributing weight. Spaces do **not** activate clips, reset/synchronize clocks, reorder clips, remove override/additive boundaries, or crossfade audio. Author contiguous weighted clips and start the desired layers explicitly (or use autoplay). Parameter changes update the next pose immediately; no extra smoothing is implied.

Rust `EvaluationContext.variables` optionally overrides authored parameter variables; missing values fall back to authored values. Browser `runtime.variable(name).set(number)`, native `Action::SetVariable`, and state actions change live parameters without rewriting the project. Weight getters remain authored/live layer weights, **not** multiplied space weights.

## CLI

```
splinely-cli blend-clip scene.sply --clip offset --mode additive --weight 0.5 --output scene.sply
splinely-cli blend-space set scene.sply --id locomotion --x speed --y turn \
  --sample walk:0:0 --sample run:2:0 --output scene.sply
splinely-cli blend-space list scene.sply
splinely-cli blend-space weights scene.sply --id locomotion --x 1 --y 0
splinely-cli blend-space parameter scene.sply --name speed --value 0.5 --output scene.sply
splinely-cli blend-space remove scene.sply --id locomotion --output scene.sply
```

`set` creates/replaces atomically after validation; it creates missing parameter variables at zero but never changes an existing variable's type, clip mode, autoplay or audio. `remove` preserves clips/variables. `weights` is read-only. Samples use `CLIP:X:Y` (document IDs cannot contain colons). The evaluate command accepts `{"variables":{"speed":2}}` in its context JSON alongside clocks/states/weights/overrides. Invalid edits do not overwrite the output. CLI discovery includes the new model and subcommands.

## Editor

The dedicated `blending_ui` module adds Additive to clip options and a collapsible **2D-Mischräume** section in the timeline: create/edit/delete spaces, choose weighted clips, edit sample positions and axis variable names, inspect multipliers and adjust authored parameters. Draft application is validated atomically and participates in normal document history. Invalid blend-mode changes are disabled; remove a clip's space sample before deleting that clip. Existing layer preview can start all non-override clips together, with independent clocks; weights are not audio gains. There is no triangulation canvas, sync-marker editor, or space-level time synchronization.

### Translation handoff (catalogs intentionally not edited)

The following German source keys require translations in each supported locale; untranslated keys currently fall back to German. Existing shared labels (`Bearbeiten`, `Löschen`, `ID`, `Übernehmen`, `Abbrechen`, etc.) are reused.

- `Additiv`
- `Additiv: Gewicht × (Clipwert − Dokument-Grundpose). Nur Zahlen und RGBA; begrenzte Werte werden geklemmt.`
- `2D-Mischräume`
- `Zwei numerische Variablen steuern inverse quadratische Distanzgewichte. Nur aktive gewichtete Clips werden gemischt; Ebenenvorschau startet sie gemeinsam.`
- `Mischraum hinzufügen`
- `X-Variable`
- `Y-Variable`
- `Stützpunkt hinzufügen`
- `Zuerst einen freien gewichteten Clip erstellen.`

## Verification and limits

Shared canonical fixture: `runtime/tests/fixtures/blend-spaces.json`. Dedicated document/native/CLI/Node tests cover rest deltas, sparse residuals, RGBA, bones, layer boundaries, parameter updates, fades, malformed inputs, atomic edits and immutable defaults. Run:

```
cargo test -p splinely_document -p splinely_runtime -p splinely-cli --tests
cargo test -p splinely_editor --bin splinely blending_ui
node --test runtime/tests/*.test.cjs
python3 scripts/blend-spaces-smoke.py
```

The last command uses the built CLI to export an offline SVG and actual headless Firefox through the existing Marionette harness; checks DOM poses, parameters, fades, overrides and diagnostics. It is not a pixel-diff or a manual editor interaction test. Node tests use mock DOM; Rust and Node share fixture/expected values, not a new generalized cross-process parity framework. No new audio behavior is promised. Broader pre-existing JavaScript adapter/schema differences are not normalized by this change; additive canonical values and the new space fields have dedicated checks.
