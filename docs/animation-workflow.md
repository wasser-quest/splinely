# Animation and states, step by step

## A first animation

1. Open **Animieren**, select a shape on the canvas/layer list, or choose a shape/bone in the right inspector's **Einfach animieren · Schritt für Schritt**.
2. Choose a clip in the timeline, or press **+ Clip**. New clips use the existing override defaults (5 seconds, looping, not autoplay).
3. Change the shape's base property in the inspector (a bone's local position/rotation in the bone panel). Choose the time with the seconds slider.
4. Choose the property and press **Schlüssel aus aktuellem Wert hinzufügen**. The inspector also offers **Auswahl animieren → Aktuellen Wert merken**.
5. Change the value, choose another time, and repeat. Play to preview.

The button records the **authored base value**, not the evaluated/weighted preview. It creates a track when needed, inserts sorted keys, and replaces a value at the same time without changing an existing key's easing. Bones support only local X, Y and rotation. Bound shapes also record local values. Protected shapes must first be unlocked.

For a quick start, use **Hüpfen**, **Winken**, or **Aus- / Einblenden**. These add three linear keys over the selected clip's duration: base → displaced/rotated/transparent → base. Opacity is offered only for shapes. Existing tracks are never overwritten by a preset; choose a fresh clip or deliberately remove the track under **Spuren & Schlüssel (Experten)**. Each successful candidate is validated by the core and committed through the normal undo path; a rejected candidate does not partially change the document.

The expert timeline, clip settings, key values/easing, bone panel and playback controls remain available.

## A first state machine

Open **Zustände → Neue Zustandsmaschine · geführt**:

1. Enter two state IDs and choose the initial state. IDs start with an ASCII letter or `_`; remaining characters may also include digits, `_`, `-`, or `.`. Spaces and accented letters are not supported by the schema.
2. Choose a **shape** as the event target. Choose click or hover (the real event name is `pointerenter`). The assistant creates one transition from the first state to the second; it does not invent a return transition.
3. Optionally select a clip for each state's `onEnter`. It adds `PlayClip`.
4. For a weighted clip, optionally enable **Gewicht einblenden**. It adds `BlendClip` **after** `PlayClip`, with the chosen target weight and duration. The fade starts at the current runtime weight (initially the configured clip weight); for a first fade-in, set the clip weight to 0 in clip options and choose target weight 1. `PlayClip` does not reset an existing runtime weight. For repeatable fade-ins, explicitly reset the weight with a zero-duration `BlendClip` before the fade, using the graph/JSON controls.
5. Press **Geführte Zustandsmaschine erstellen**, then test in preview. Native canvas preview handles clicks; test hover through **Ereignis → pointerenter**, with the shape ID, or use the Viewer/browser.

The transition has `guard: None`: matching the event is sufficient. It has no automatic variable condition. The initial state's entry actions run when the runtime machine initializes. Choosing the second state as initial does not create a reverse transition. Use the existing graph/JSON controls for return transitions, conditions, `StopClip`, other actions, and additional states.

A staged/invalid **state-machine JSON** buffer disables guided creation and graph editing until you explicitly apply it or reload it. Guided animation edits never reload this buffer. The assistant's form itself is temporary UI state, not project data.

## Two different meanings of “blend”

- **Compositor mode:** `Override` replaces channels according to playback order. Adjacent `Weighted` clips combine their contributing property channels; unused weight retains the incoming/base pose, and sums above 1 are normalized. Weight affects visual properties, not audio loudness.
- **Weight fade:** `BlendClip` changes the weight of an already active weighted clip over time. It does **not** activate a clip by itself. The assistant explicitly starts the clip first.
- **Transition crossfade:** there is no transition-duration/crossfade field in the schema. The assistant does not promise one. Other clips continue playing; implement a two-sided fade with explicit actions/weighted clips, or stop the old clip in the graph/JSON.

## Source and trust boundaries

These tools author model shapes/bones, not arbitrary elements in raw SVG source. They do not rewrite `raw_svg`, custom JavaScript, source editor buffers, or script trust. Native preview does not execute custom JavaScript; browser-only behavior still requires the existing explicit trust/export workflow. Imported source-only artwork needs actual model nodes before these tools can target it.

## Integration and checks

`panels.rs` includes `animation_assistant.rs` as a private child module. The controls are already wired into the inspector (guided controls in Animieren, compact key shortcut elsewhere), existing timeline and states panel; **no `app.rs` module declaration or Editor fields are needed**. No dependency, schema or runtime changes are required. Pure candidate tests live in `animation_assistant.rs`; panel tests cover undo and preservation of staged JSON/source/trust.

```sh
TMPDIR="$PWD/.work-tmp" cargo test -p splinely_editor --bin splinely app::panels
TMPDIR="$PWD/.work-tmp" cargo clippy -p splinely_editor --bin splinely --tests -- -D warnings
```
