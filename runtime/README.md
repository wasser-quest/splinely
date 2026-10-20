# Splinely standalone SVG runtime

`src/splinely.js` is a plain self-contained script: no build step, packages, imports, network calls, or external asset URLs. It runs in script-enabled SVG browser documents (`file://` works), inline SVG and script-enabled `<object>` contexts. `<img>` and most image viewers do not run scripts. Export a meaningful static drawing first.

See [`../RUNTIME_API.md`](../RUNTIME_API.md) for the authoritative runtime/export contract and [`../CORE_API.md`](../CORE_API.md) for the Rust schema. The runtime supports its schema v1, including tagged values/easing, transform composition, state overrides/actions, and **global** audio cue time. No Rust/editor files are owned by this component.

## Where the runtime is and how to include it

| What | Where | Use it when |
|---|---|---|
| Browser runtime | `runtime/src/splinely.js` (one plain script, about 114 KB) | Animations in any browser, offline |
| `.sply` decoder | `runtime/src/sply_decode.js` (copied into `splinely.js` between markers) | Reading binary `.sply` in the browser |
| Native runtime | `crates/splinely_runtime` (`libsplinely_runtime.so`, `splinely-runtime` JSON-lines process) | Hosts without a browser |
| Desktop viewer | `crates/splinely_viewer` | Opening a project in a native WebView |

**Do not edit `src/splinely.js` by hand.** The decoder section is generated from `sply_decode.js`; after a change run `node scripts/sync-runtime-decoder.cjs` (use `--check` in CI).

### 1. Get a runtime-enabled SVG

The runtime is not loaded from a separate URL. The exporter writes it into the SVG:

```sh
splinely-cli export animation.sply --output animation.svg
```

Open `animation.svg` directly in a browser, or embed it (step 2). The file needs no network access and no second file.

### 2. Embed it in an HTML page

Use an `<object>` element. An `<img>` tag does not run scripts and therefore shows only the still picture:

```html
<object id="movie" type="image/svg+xml" data="animation.svg">Animation not available.</object>
<script>
const movie = document.getElementById('movie');
movie.addEventListener('load', () => {
  const splinely = movie.contentDocument?.documentElement?.splinely;
  if (!splinely) return; // scripts blocked, or the file is not a runtime export
  splinely.clip('intro').play();
});
</script>
```

Same-origin rules apply: `file://` depends on the browser, so serve the files over HTTP(S) when you control the host. Full pattern and troubleshooting: [Runtime cookbook](../docs/runtime-cookbook.md#im-html-anzeigen).

### 2b. Mount it inline, without a frame

`<object>` runs the graphic in a frame of its own. Where that is a problem (some browsers on Android close the whole browser while an animation runs in a frame), mount the graphic inline instead, the way Rive artboards are used: the host loads `splinely.js` and `splinely-mount.js`, and the graphic becomes ordinary SVG in the page:

```html
<div id="stage" style="width: 320px"></div>
<script src="splinely.js"></script>
<script src="splinely-mount.js"></script>
<script>
fetch('animation.svg')
  .then((response) => response.text())
  .then((svgText) => {
    const graphic = SplinelyMount.mount(document.getElementById('stage'), svgText);
    if (graphic.runtime) graphic.runtime.clip('intro').play();
    // graphic.destroy() stops it and removes it from the page.
  });
</script>
```

`mount` removes the scripts of the file (the host runs the graphic) and returns `{ svg, runtime, error, destroy() }`. A graphic that cannot start stays visible as its static drawing, and `error` says why. The same API as `createRuntime` applies to `runtime`, and so do its options: `mount(container, source, { focusRing: false })` hides the focus ring the runtime draws around a focused node. A host that draws its own focus, or shows the graphic without keyboard use, switches it off. `scripts/inline-mount-smoke.cjs` checks this in Chromium.

### 3. Use it from Node (no browser)

`src/splinely.js` also loads as CommonJS. Requiring it does not mount anything:

```js
const { createRuntime, readProject } = require('./src/splinely.js');
```

Use `createRuntime(svg, project, { autoTick: false })` in tests and when you drive time yourself. The test suite runs without `npm install`: `node --test runtime/tests/*.test.cjs`.

### 4. Native hosts

Link `libsplinely_runtime.so` through the C ABI, or run the `splinely-runtime` process and exchange JSON lines. Build with `cargo build -p splinely_runtime`; the Python smoke test is `examples/native-runtime/smoke.py`. Details: [native runtime](../docs/native-runtime.md).

## Embed / use

Embed SVG node elements, then XML-escaped JSON metadata with ID `splinely-project`, then the entire source in an SVG script. The script auto-mounts its owning root:

```js
const svg = document.currentScript.ownerSVGElement;
const splinely = svg.splinely; // canonical scoped API
const unsubscribe = splinely.on('button', 'click', async () => {
  await splinely.audio.unlock(); // trusted user activation required by browsers
  splinely.timeline.seek(0);
  splinely.clip('intro').seek(0).play();
});
splinely.timeline.at(1.5, () => splinely.audio.play('hello'));
splinely.machine('main').send('OPEN');
splinely.variable('speed').set(2);
splinely.set('circle', { opacity: 0.5 });
// Later: unsubscribe(); splinely.set('circle', null); splinely.destroy();
```

`window.splinely` is only a convenience alias for a single instance and is removed when another mounts. Never rely on it in a multi-SVG page. `SplinelyRuntime.createRuntime(svg, project?, {autoTick:false})` supports explicit mounting/tests; don't mount a root twice. For explicit options on an exported root, omit the automatic source script's containing SVG association (e.g. load the runtime in the HTML host), or destroy the auto-mounted instance before creating a replacement. CommonJS `require('./src/splinely.js')` exposes `createRuntime`, `readProject`, `normalize`, `evaluateTrack`, `interpolate`, `easing` without auto-mounting.

## Bones

Optional `bones[]` and node `boneId` work without changing old no-bones documents. Child offsets start at the parent's tip; attached node transforms start at the bone root. State overrides, clips and `splinely.set('bone-id',{x:10,y:20,rotation:45})` compose before FK. Bone patches accept only finite numeric x/y/rotation; `set('bone-id',null)` clears explicit patches. `splinely.bones()` returns copied world poses, including tips. Numeric rotation interpolation uses existing easing, not shortest-arc interpolation. `locked` and `originalKind` remain inert node metadata. See the root contract for validation limits and export-local-transform requirements.

## Transport semantics

All times are finite seconds. Clip duration must be positive. Keyframes are sorted stably without mutating the document. Numbers and colors interpolate; arrays/hex colors/discrete values are adapter extensions. Step holds the left key; an exact right key returns its value. Cubic Bezier uses 60 x-inversion bisections matching Rust's solver. Native schema clips compose in document order, later wins; adapter numeric priorities sort before document order. Pause retains a clip sample; stop removes it; set overrides can be removed explicitly.

- `clip.play()` is idempotent while playing; a finished nonloop restarts at zero. `pause()` holds time. Nonloop `seek(t)` clamps to `[0,duration]`; loop seeks wrap nonnegative time modulo duration. Local negative seeks clamp to zero. `stop()` pauses, resets and deactivates the clip.
- Global timeline starts implicitly with any clip play; pausing the last playing clip also pauses this implicit timeline. `timeline.play()` explicitly starts an independent global clock, even without clips. Stopping a clip does not stop an explicitly started timeline. Natural clip completion leaves the global clock running for later global cues. `timeline.pause()` pauses all clips. `timeline.seek(t)` requires nonnegative time, sets global time, and silently synchronizes activated clips (modulo/clamped as appropriate). It does not activate previously untouched clips. Clip-local seek does not alter global time.
- `timeline.at(t,fn)` observes global time. `{clip:'intro'}` selects local clip time and repeats with that clip's loops. Crossings are `(old,new]`, with zero on play from zero and each loop. Duration fires before zero of the following loop. A backward seek rearms crossings; forward seek skips them. Pause/seek do not call callbacks. Callback removals take effect immediately; subscriptions added during a crossing begin with future crossings.
- Skipped render frames process each boundary in chronological segments. Starting clip B at A's 0.5 callback during a one-second tick advances B only 0.5, irrespective of clip storage order. Equal-time order: global then clips in document order, then callbacks in insertion order. Callbacks may pause/seek/play; revision checks prevent stale crossings after mutation. Reentrant `tick()` calls are rejected and async callback rejections enter diagnostics. At most 10,000 segments per tick, then playback pauses with a diagnostic. Nonloop `clip.end` fires once at completion, targeted to the clip ID.
- Audio timeline time follows a running AudioContext clock after unlock. RAF and a 25ms timer use a shared time anchor; manual `tick` should only be used with automatic stepping disabled. Clock-domain changes re-anchor instead of introducing a timestamp jump.

## Events / behavior

`on(id,event,fn)` and `on(event,fn)` return idempotent unsubscribe functions. Handlers get `(detail,splinely)`; async rejections and synchronous exceptions are diagnosed. `emit(name,detail,target?)` broadcasts to subscribers then machines; nested broadcasts/sends queue, not recurse. Machines share this instance's variables. State entries initialize exactly once before initial emitted events are drained. Guards compare JSON values structurally; canonical scalar variable assignments preserve declared types. The first matching local transition, then machine transition, wins. Exit actions precede transition actions and destination entry actions. At most 256 items per immediate event drain prevents cycles.

DOM events: `click`, `dblclick`, `pointerenter`, `pointerleave`, `pointerdown`, `pointermove`, `pointerup`, `pointercancel`, `lostpointercapture`, `wheel`, `keydown`, `keyup`, `focus`, `blur`. Detail includes original `event`, target ID, key, pointer ID/buttons, and root SVG-space x/y (inverse screen CTM). Degenerate coordinate conversion reports an error and uses client coordinates. Events in nested SVG roots are ignored by the outer runtime. Keyboard handling is not document-global: give exported controls `tabindex="0"` and accessible labels as appropriate.

`pointerdown` starts capture and emits `dragstart`; moves emit `drag` with root-space `dx,dy` relative to the press; up/cancel/lost capture emits `dragend` and cleans capture. Multi-pointer records are independent. Drag does not change node properties automatically; subscribe and call `set` if desired. Pointer capture is released on destroy. No geometry hit-testing or synthetic accessibility framework is supplied.

Core action names are documented in the root contract. Adapter aliases: `set` (target/properties), `clip.play/pause/stop/seek` (clip/time), `variable.set` (variable/value), `machine.send` (machine/event), `audio.play` (asset). Adapter `interactions[]` may bind `{target,event,actions}`. Timers use `timeline.at()` plus `emit()`; arbitrary timer guard strings are not evaluated.

## Audio

`await audio.unlock()` creates/resumes the instance's AudioContext and decodes every embedded asset; concurrent unlocks share work. Errors reject and enter `diagnostics`; retry is possible. Assets must be base64 JSON, never external URLs. Limit: 64 MiB encoded text **per asset** (not a full decoded-document quota). Codec support, browser user-activation policy and real audio output remain platform-dependent. A rejected unlock may retain successfully decoded earlier assets for a retry; destroy frees them.

`audio.play(id,{offset,volume,duration})` returns a one-shot stoppable handle; these sounds are independent of clip sources. `audio.pause()` stops independent sounds, not graphic clips. `audio.seek(seconds)` stops independent sounds and seeks the global timeline. Use timeline pause/resume for cue transport; independent sounds are not automatically resumed. Clips' local cues belong to those clips; core project's cues belong to the independent global timeline and do not repeat when one graphic clip loops.

Cue sources are scheduled ahead against `AudioContext.currentTime`, not emitted from render crossings. Pause stops sources; resume recreates overlapping cues at `asset offset + elapsed cue time`. Active seek recreates overlaps and future sources; paused seek is silent. Completed cues are not replayed. Sources are keyed per cue/cycle to prevent duplicate scheduling. Loop-local audio uses a native loop of a padded buffer (silence around each cue) so gaps/skipped RAF frames do not omit audio laps. This costs memory: total cached loop padding is capped at 128 MiB per runtime; invalid/oversized cues are diagnosed once, not repeatedly allocated. Changing playback mode/seek stops and recreates sources. No additive audio effects, external streaming or automated latency calibration.


## Safety / errors / limits

Runtime does not evaluate `customJs`, raw SVG, string guards, URLs or remote assets. Attach explicitly trusted handlers separately. This is not an SVG sanitizer or a sandbox for unrelated scripts. Shape properties cannot install event-handler attributes, URLs, style text or innerHTML. Unknown API IDs throw; schema version/IDs/durations/key times/easings/state targets/cue ranges are validated. Missing DOM nodes and unsupported actions/properties report diagnostics. Handlers and actions fail independently. Latest 100 errors are retained and available through `on('error', ({error})=>...)`. Failed auto-initialization logs a console error and retains the static SVG; explicit construction throws.

No arbitrary path morphing, additive clip blending, automatic plugin translation, user-code execution, legacy browser support, perfect audio/visual sample synchronization, full schema migrations or full resource quotas. Core enforces stricter geometry/ID/property validation than the flexible adapter; exporters should call `Project::validate()`.

## Tests

Requires Node 18+ (verified with Node 22.22.2), no npm install:

```sh
node --check runtime/src/splinely.js
node --test runtime/tests/*.test.cjs
```

Tests cover bone hierarchy/parent tips, attached transforms, numeric rotations/easing, patch composition, invalid rigs/overflow/depth, old documents, interpolation, canonical rendering/priority, precise loops/seeks/end callbacks, cross-clip event timing, global timeline, machine initialization/guards/order/cycle limits, DOM interaction/drag, cleanup, aliases and independent instances, fake Web Audio scheduling/offsets/native loops/memory failure, clock switching, error handling and plain-script auto-mount. Fake audio does **not** verify audible latency, codecs, browser unlock policy or device behavior.

Generate a genuinely offline XML SVG with runtime embedded, and open directly in a browser:

```sh
node runtime/tests/build-browser-smoke.cjs
# Open runtime/tests/.generated/browser-smoke.svg: green PASS or red FAIL.
# Browser screenshot (use an existing empty profile directory):
mkdir -p runtime/tests/.generated/firefox-profile
firefox --headless --no-remote \
  --profile "$PWD/runtime/tests/.generated/firefox-profile" \
  --screenshot "$PWD/runtime/tests/.generated/browser-smoke.png" \
  "file://$PWD/runtime/tests/.generated/browser-smoke.svg"
```

Verified green **PASS** in direct-file Firefox 151.0.1: XML-safe JSON containing `</script> & ]]>`, script auto-mount, core transform sampling, bone hierarchy/attachment/numeric rotation, loop crossings, silent seek, DOM click → machine entry, nested duplicate-ID scoping, alias lifecycle and destroy. This smoke is visual/manual, not a Node assertion of browser output. Script-disabled display says STATIC FALLBACK. Browser matrix beyond Firefox, real codecs/unlock, sound hardware, and CSP-restricted hosts remain manual acceptance work.

## Binary `.sply` decoder (`src/sply_decode.js`)

Dependency-free, ES2018-safe, BigInt-free decoder for the binary project encoding specified in [`../docs/sply-binary-format.md`](../docs/sply-binary-format.md). It works as a browser `<script>` (defines `SplyDecode`) and in Node (`require('./src/sply_decode.js')`); `splinely.js` is not modified and the file can be concatenated in front of it later.

```js
const { decodeSply, decodeSplyBase64, isSply, crc32c } = require('./src/sply_decode.js');
isSply(bytes);            // true when bytes start with the 8-byte magic (Uint8Array | ArrayBuffer)
const tree = decodeSply(bytes);            // plain JSON-equivalent tree; feed it to normalize()/readProject-style code
const tree2 = decodeSplyBase64(b64text);   // standard base64 of a whole .sply file (SVG data-encoding="sply-base64"); whitespace allowed
```

* Every failure throws `Error` whose message starts with `sply:` (bad magic/major/minor/flags, size or CRC32C mismatch, bad string offsets, invalid UTF-8, duplicate strings/keys, unknown tag, non-minimal/overflowing varint, non-finite float, depth > 128, counts larger than the remaining bytes, trailing body bytes, invalid string index, input > 64 MiB). Decoding is linear in file size and never allocates before validating counts.
* Float arrays (tag 9) become plain JS arrays; blob tag 10 becomes the padded standard base64 string; `-0.0` is preserved. JS cannot distinguish `1` from `1.0`.
* Integers: |v| ≤ 2^53−1 are exact. Larger tag 3/4 values (up to 2^64−1 / −2^63) are returned only if exactly representable as a double (e.g. 2^60); otherwise `sply: integer exceeds safe range` is thrown instead of rounding silently.
* Tests: `node --test runtime/tests/sply-decode.test.cjs` (test-only encoder, hostile-input cases, fuzz; additionally runs `crates/splinely_document/tests/fixtures/sply/manifest.json` when present, skipped otherwise).
