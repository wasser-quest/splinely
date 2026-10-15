# Plugins: overview, trust model and walkthrough

Splinely has three different kinds of plugins. They differ in what they can do and
how much you must trust them. **None of them is loaded from project data, and no
native library is ever loaded dynamically.**

| Kind | Format | Runs where | Executes code? | Loaded how |
| --- | --- | --- | --- | --- |
| 1. **Declarative editor tools** | `splinely-plugin.json` (`tools`/`palette`) | Editor and CLI (`plugin apply`) | No – JSON data only | Discovered at editor start from the plugin folder; `plugin apply` takes an explicit path |
| 2. **Browser runtime plugin** | one JavaScript file named by `runtime.entry` | Exported SVG / web player | **Yes – arbitrary JavaScript** | Only by `splinely-cli export --plugin DIR --trusted-js` (or an explicit host call `splinely.use(plugin)`) |
| 3. **Trusted native editor plugin** | Rust source ([`plugin-templates/editor`](../plugin-templates/editor/README.md)) | Editor process | **Yes – full process privileges** | Only compiled in by the editor host's developer (compile-time `EditorPlugin`/`Projection::add_plugin`) |

Kinds 1 and 2 live together in **one plugin folder**; the manifest may declare editor
tools, a runtime part, or both. Example: [`examples/plugins/hover-wiggle`](../examples/plugins/hover-wiggle/).

## Trust model (read this first)

- **Kind 1 is data.** The manifest is parsed against strict allowlists (unknown fields
  and duplicate keys are rejected), bounded in size, and every produced project is checked
  with `Project::validate()` before it replaces anything. It cannot run code. It can only
  add/patch project data, exactly what you could type with the CLI. The result runs on
  the Rust *and* the JavaScript runtime, because it is ordinary project data.
- **Kind 2 is trusted code, not a sandbox.** The runtime file runs in the same page and
  realm as the exported SVG with the same rights as `customJs`: it can touch the DOM,
  `window`, storage and the network. The `ctx` object is a *convenience and consistency*
  API, **not a security boundary**. A malicious plugin can ignore it.
  - It is embedded **only** when you pass `--plugin` **and** `--trusted-js` together.
    `--plugin` alone fails with error code `trust_required` and writes nothing.
  - Project files cannot request a plugin. Opening/exporting a project never loads
    plugin code; a `.sply` with no plugin export contains none.
  - The export records each plugin in `<metadata id="splinely-plugins" data-trust="explicit-trusted-js">`
    (`id`, `entry`, `apiVersion`, `bytes`, `sha256`) so the file's content can be reviewed and
    pinned. This is an integrity *record*, not a signature: the SVG itself carries the code.
  - The editor **never executes** a plugin's runtime file and has no "export with plugin"
    button. It only shows entry, size, API version, hash and a note. Exporting with
    plugins is a deliberate CLI step.
  - The **isolated preview** (`splinely-cli isolated-preview --plugin DIR`, see
    [`isolated-preview.md`](isolated-preview.md)) is the safer way to try a plugin: the
    code runs inside the opaque-origin sandboxed frame, behind the consent checkbox.
    The page lists the embedded plugin ids/hashes. Its limits (not a network firewall, no CPU
    quota) apply.
- **Best-effort lint, not a sandbox.** At load time the runtime file must be UTF-8, at most
  256 KiB, without NUL bytes, and must not contain obvious network/dynamic-code markers
  (`http:`, `https:`, `ws://`, `wss://`, `fetch(`, `XMLHttpRequest`, `WebSocket`, `EventSource`,
  `sendBeacon`, `importScripts`, `import(`, `import.meta`, `eval(`, `Function(`, `</script`,
  `<!--`). A text scan is trivially bypassed (string concatenation, `globalThis["fe"+"tch"]`);
  it only catches accidents and lazy abuse. **Read the code (and its hash) before exporting.**
- **Kind 3 is full-trust native code**, compiled in on purpose. There is no dynamic
  loading, no signing, no permissions. Dynamically loadable native plugins are out of scope
  and will not be added. See the template README.

## Writing the editor part (kind 1): `addBehavior`

A tool with `"action": {"type": "addBehavior", "behavior": {...}}` inserts a **declarative**
behavior bound to the **selected, unlocked node**: one clip, one state machine and
optional variables / accessible label. Schema (all objects reject unknown fields):

```json
{
  "trigger": { "event": "pointerenter" },
  "clip": {
    "duration": 0.6,
    "tracks": [{ "property": "rotation",
                 "keyframes": [{"time": 0, "value": 0}, {"time": 0.3, "value": 8, "easing": "ease-out"}, {"time": 0.6, "value": 0}] }]
  },
  "variables": { "wiggleCount": 0 },
  "a11yLabel": "Button (bounces when activated)"
}
```

| Field | Rule |
| --- | --- |
| `trigger.event` | `click`, `dblclick`, `pointerenter`, `pointerleave`, `pointerdown`, `pointerup` |
| `clip.duration` | > 0 and ≤ 30 seconds |
| `clip.tracks` | 1–8; `property` ∈ `x y rotation scaleX scaleY opacity fill stroke` |
| `keyframes` | 2–32 per track, strictly increasing `time` within `duration`; `value` is a number, or `#RRGGBB` for `fill`/`stroke`; `easing` ∈ `linear step ease ease-in ease-out ease-in-out` |
| `variables` | optional, ≤ 8 names, values boolean / number / short string |
| `a11yLabel` | optional, ≤ 200 chars; applied only when the node has no accessibility label yet |

**What is merged** into the project (nothing else is touched):

- Clip `plugin-<tool>-clip-N`: not looping, no autoplay, tracks bound to the selected node.
- State machine `plugin-<tool>-machine-N`: one state `idle`, one transition `idle → idle` on
  the trigger event (target = the node) whose actions are `stopClip` then `playClip`
  (so the animation restarts on every trigger).
- Variables are added only if missing. An existing variable of the same JSON type is kept
  (its value is *not* overwritten); a different type is an error.
- The a11y label as described above.
- Ids are allocated collision-free against nodes, bones, clips, blend spaces, state machines,
  audio, groups and followers (`-1`, `-2`, … suffix). Existing items are never replaced.
- Limits: at most 256 clips and 256 state machines per project, the project must stay under
  10,000 nodes and 8 MiB, and the **whole candidate passes `Project::validate()`**. On any
  error the original project is unchanged.

Undo: the editor treats the result like any other edit (the candidate replaces the project through
the normal commit/history path, so one Undo reverts the whole behavior). The CLI writes the result
atomically to `--output` (default: the project file).

See [`drop-in-plugins.md`](drop-in-plugins.md) for the rest of the manifest (`addShape`, `setStyle`,
palette, folders, bounds).

## Writing the runtime part (kind 2)

Manifest:

```json
{ "id": "hover-wiggle", "title": "…", "tools": [ … ],
  "runtime": { "apiVersion": 1, "entry": "runtime.js", "description": "optional, ≤ 240 chars" } }
```

`entry` is one file name inside the plugin folder (ending `.js`, ≤ 64 chars of `A-Za-z0-9_.-`,
no path separators or `..`, not a symlink). `apiVersion` must be `1`. A plugin may have `runtime`
only (no tools).

The file is the **body of a function that returns the plugin object** (the exporter wraps it
in `new Function`; it is not an ES module):

```js
return {
  id: 'hover-wiggle',            // required, /^[A-Za-z][A-Za-z0-9_.-]{0,63}$/, must equal the manifest id
  apiVersion: 1,                 // required, exactly 1
  events: ['pointerenter', 'click'],   // optional, ≤ 32 event names delivered to onEvent
  frameBudgetMs: 4,              // optional, onFrame budget (see below)
  setup(ctx) {},                 // once, after the runtime mounted
  onEvent(event, ctx) {},        // needs a non-empty `events`; event = frozen copy of the event detail (primitive values only) plus `type`
  onState(change, ctx) {},       // state machine changes: {machine, from, to, event}
  onFrame(frame, ctx) {},        // frame = {delta, time, playing}; every animation tick, keep it tiny
};
```

Unknown keys are an error. `ctx` is a frozen object:

| Member | Effect (all delegate to the public host API, so the same validation applies) |
| --- | --- |
| `pluginId`, `apiVersion` | identification |
| `emit(name, detail?)` | `splinely.emit` – can trigger state machines |
| `getVariable(name)` / `setVariable(name, value)` | `variable(name).get/set`; unknown variable or type change throws |
| `send(machineId, event, detail?)` | `machine(id).send(event, detail)` |
| `playClip(id)` / `stopClip(id)` | `clip(id).play()/stop()` |
| `set(nodeId, props)` | the host `set(id, props)` call (same property validation) |
| `snapshot()` | deep-frozen copy of `splinely.snapshot()` (read only) |
| `report(message)` | add a runtime diagnostic |
| `onDestroy(fn)` | cleanup when the runtime is destroyed |

Behavior guarantees (tested in `runtime/tests/plugins.test.cjs`):

- **Opt-in, zero cost without plugins:** nothing is registered and `tick()` has one `null` check.
  Event subscriptions exist only for the `events` a plugin declares.
- **Exceptions never break the runtime:** every hook is wrapped; errors become diagnostics
  prefixed with the plugin id (`splinely.diagnostics`, `error` event). After 5 errors the plugin is
  disabled (`splinely.plugins()` shows `disabled`/`reason`).
- **Frame budget:** `onFrame` time is measured with `performance.now()` after the call
  (default 4 ms, configurable per plugin). 3 overruns disable the plugin. **Synchronous
  JavaScript cannot be preempted**, so a plugin that loops forever still hangs the page. The
  budget is damage limiting for slow code, not protection from hostile code.
- At most 8 plugins per runtime; `id`s are unique; invalid plugins are reported as diagnostics,
  not thrown.
- Hosts can use the same API without export tooling:
  `SplinelyRuntime.createRuntime(svg, project, {plugins: [pluginObject]})` or `splinely.use(plugin)`.
  `splinely.plugins()` lists id, source, hash, events, hooks, errors.

## Walkthrough with `examples/plugins/hover-wiggle`

```sh
PL=examples/plugins/hover-wiggle
splinely-cli plugin validate $PL                       # manifest, tools, runtime file, sha256, lint result
splinely-cli new --output demo.splinely.json --width 240 --height 120
splinely-cli node add demo.splinely.json --kind rect --id btn --x 60 --y 30 --width 100 --height 50
splinely-cli plugin apply $PL wiggle --project demo.splinely.json --node btn   # editor part (data only)
splinely-cli plugin apply $PL bounce --project demo.splinely.json --node btn
splinely-cli validate demo.splinely.json

splinely-cli export demo.splinely.json --output plain.svg                      # declarative behavior only, no plugin code
splinely-cli export demo.splinely.json --output plugin.svg --plugin $PL --trusted-js   # + runtime part
splinely-cli export demo.splinely.json --output x.svg --plugin $PL            # error trust_required, nothing written
splinely-cli isolated-preview demo.splinely.json --output preview.html --plugin $PL   # sandboxed try-out
```

In a browser, hovering the rectangle plays the wiggle clip and clicking plays the bounce clip in
**both** `plain.svg` and `plugin.svg` (project data). Only in `plugin.svg` does the runtime hook run and
count into the variables `wiggleCount` / `wiggleLast`. This is checked in Chromium, Firefox and WebKit by
the `runtime-plugin` scenario of `scripts/browser-matrix.cjs`
([`browser-tests.md`](browser-tests.md)). `splinely-cli plugin list` shows installed plugins (add
`--dir` for another root); the editor's “Erweiterungen/Extensions” section shows both parts and
the persistent diagnostics of broken plugins.

## What is supported / not supported

- Supported: JSON editor tools incl. `addBehavior`; runtime hook API v1; explicit trusted CLI export
  with hash-pinned metadata; sandboxed isolated preview with plugins; CLI `plugin list|validate|apply`.
- Not supported: loading native libraries at runtime; a plugin sandbox/permission system; plugin
  signatures or a registry; hot reload; runtime plugins in the editor's native viewer; exporting runtime
  plugins from the editor GUI; ES modules/imports inside the plugin file; plugin-defined project formats.
