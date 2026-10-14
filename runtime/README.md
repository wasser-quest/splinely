# Forma standalone SVG runtime

`src/forma.js` is a plain self-contained script: no build step, packages, imports, network calls, or external asset URLs. It runs in script-enabled SVG browser documents (`file://` works), inline SVG and script-enabled `<object>` contexts. `<img>` and most image viewers do not run scripts. Export a meaningful static drawing first.

See [`../RUNTIME_API.md`](../RUNTIME_API.md) for the authoritative runtime/export contract and [`../CORE_API.md`](../CORE_API.md) for the Rust schema. The runtime supports its schema v1, including tagged values/easing, transform composition, state overrides/actions, and **global** audio/speech cue time. No Rust/editor files are owned by this component.

## Embed / use

Embed SVG node elements, then XML-escaped JSON metadata with ID `forma-project`, then the entire source in an SVG script. The script auto-mounts its owning root:

```js
const svg = document.currentScript.ownerSVGElement;
const forma = svg.forma; // canonical scoped API
const unsubscribe = forma.on('button', 'click', async () => {
  await forma.audio.unlock(); // trusted user activation required by browsers
  forma.timeline.seek(0);
  forma.clip('intro').seek(0).play();
});
forma.timeline.at(1.5, () => forma.audio.play('hello'));
forma.machine('main').send('OPEN');
forma.variable('speed').set(2);
forma.set('circle', { opacity: 0.5 });
// Later: unsubscribe(); forma.set('circle', null); forma.destroy();
```

`window.forma` is only a convenience alias for a single instance and is removed when another mounts. Never rely on it in a multi-SVG page. `FormaRuntime.createRuntime(svg, project?, {autoTick:false, speech:false})` supports explicit mounting/tests; don't mount a root twice. For explicit options on an exported root, omit the automatic source script's containing SVG association (e.g. load the runtime in the HTML host), or destroy the auto-mounted instance before creating a replacement. CommonJS `require('./src/forma.js')` exposes `createRuntime`, `readProject`, `normalize`, `evaluateTrack`, `interpolate`, `easing` without auto-mounting.

## Bones

Optional `bones[]` and node `boneId` work without changing old no-bones documents. Child offsets start at the parent's tip; attached node transforms start at the bone root. State overrides, clips and `forma.set('bone-id',{x:10,y:20,rotation:45})` compose before FK. Bone patches accept only finite numeric x/y/rotation; `set('bone-id',null)` clears explicit patches. `forma.bones()` returns copied world poses, including tips. Numeric rotation interpolation uses existing easing, not shortest-arc interpolation. `locked` and `originalKind` remain inert node metadata. See the root contract for validation limits and export-local-transform requirements.

## Transport semantics

All times are finite seconds. Clip duration must be positive. Keyframes are sorted stably without mutating the document. Numbers and colors interpolate; arrays/hex colors/discrete values are adapter extensions. Step holds the left key; an exact right key returns its value. Cubic Bezier uses 60 x-inversion bisections matching Rust's solver. Native schema clips compose in document order, later wins; adapter numeric priorities sort before document order. Pause retains a clip sample; stop removes it; set overrides can be removed explicitly.

- `clip.play()` is idempotent while playing; a finished nonloop restarts at zero. `pause()` holds time. Nonloop `seek(t)` clamps to `[0,duration]`; loop seeks wrap nonnegative time modulo duration. Local negative seeks clamp to zero. `stop()` pauses, resets and deactivates the clip.
- Global timeline starts implicitly with any clip play; pausing the last playing clip also pauses this implicit timeline. `timeline.play()` explicitly starts an independent global clock, even without clips. Stopping a clip does not stop an explicitly started timeline. Natural clip completion leaves the global clock running for later global cues. `timeline.pause()` pauses all clips. `timeline.seek(t)` requires nonnegative time, sets global time, and silently synchronizes activated clips (modulo/clamped as appropriate). It does not activate previously untouched clips. Clip-local seek does not alter global time.
- `timeline.at(t,fn)` observes global time. `{clip:'intro'}` selects local clip time and repeats with that clip's loops. Crossings are `(old,new]`, with zero on play from zero and each loop. Duration fires before zero of the following loop. A backward seek rearms crossings; forward seek skips them. Pause/seek do not call callbacks or speak cues. Callback removals take effect immediately; subscriptions added during a crossing begin with future crossings.
- Skipped render frames process each boundary in chronological segments. Starting clip B at A's 0.5 callback during a one-second tick advances B only 0.5, irrespective of clip storage order. Equal-time order: global then clips in document order, then callbacks in insertion order. Callbacks may pause/seek/play; revision checks prevent stale crossings after mutation. Reentrant `tick()` calls are rejected and async callback rejections enter diagnostics. At most 10,000 segments per tick, then playback pauses with a diagnostic. Nonloop `clip.end` fires once at completion, targeted to the clip ID.
- Audio timeline time follows a running AudioContext clock after unlock. RAF and a 25ms timer use a shared time anchor; manual `tick` should only be used with automatic stepping disabled. Clock-domain changes re-anchor instead of introducing a timestamp jump.

## Events / behavior

`on(id,event,fn)` and `on(event,fn)` return idempotent unsubscribe functions. Handlers get `(detail,forma)`; async rejections and synchronous exceptions are diagnosed. `emit(name,detail,target?)` broadcasts to subscribers then machines; nested broadcasts/sends queue, not recurse. Machines share this instance's variables. State entries initialize exactly once before initial emitted events are drained. Guards compare JSON values structurally; canonical scalar variable assignments preserve declared types. The first matching local transition, then machine transition, wins. Exit actions precede transition actions and destination entry actions. At most 256 items per immediate event drain prevents cycles.

DOM events: `click`, `dblclick`, `pointerenter`, `pointerleave`, `pointerdown`, `pointermove`, `pointerup`, `pointercancel`, `lostpointercapture`, `wheel`, `keydown`, `keyup`, `focus`, `blur`. Detail includes original `event`, target ID, key, pointer ID/buttons, and root SVG-space x/y (inverse screen CTM). Degenerate coordinate conversion reports an error and uses client coordinates. Events in nested SVG roots are ignored by the outer runtime. Keyboard handling is not document-global: give exported controls `tabindex="0"` and accessible labels as appropriate.

`pointerdown` starts capture and emits `dragstart`; moves emit `drag` with root-space `dx,dy` relative to the press; up/cancel/lost capture emits `dragend` and cleans capture. Multi-pointer records are independent. Drag does not change node properties automatically; subscribe and call `set` if desired. Pointer capture is released on destroy. No geometry hit-testing or synthetic accessibility framework is supplied.

Core action names are documented in the root contract. Adapter aliases: `set` (target/properties), `clip.play/pause/stop/seek` (clip/time), `variable.set` (variable/value), `machine.send` (machine/event), `audio.play` (asset), `speech.say` (text/options). Adapter `interactions[]` may bind `{target,event,actions}`. Timers use `timeline.at()` plus `emit()`; arbitrary timer guard strings are not evaluated.

## Audio and speech

`await audio.unlock()` creates/resumes the instance's AudioContext and decodes every embedded asset; concurrent unlocks share work. Errors reject and enter `diagnostics`; retry is possible. Assets must be base64 JSON, never external URLs. Limit: 64 MiB encoded text **per asset** (not a full decoded-document quota). Codec support, browser user-activation policy and real audio output remain platform-dependent. A rejected unlock may retain successfully decoded earlier assets for a retry; destroy frees them.

`audio.play(id,{offset,volume,duration})` returns a one-shot stoppable handle; these sounds are independent of clip sources. `audio.pause()` stops independent sounds, not graphic clips. `audio.seek(seconds)` stops independent sounds and seeks the global timeline. Use timeline pause/resume for cue transport; independent sounds are not automatically resumed. Clips' local cues belong to those clips; core project's cues belong to the independent global timeline and do not repeat when one graphic clip loops.

Cue sources are scheduled ahead against `AudioContext.currentTime`, not emitted from render crossings. Pause stops sources; resume recreates overlapping cues at `asset offset + elapsed cue time`. Active seek recreates overlaps and future sources; paused seek is silent. Completed cues are not replayed. Sources are keyed per cue/cycle to prevent duplicate scheduling. Loop-local audio uses a native loop of a padded buffer (silence around each cue) so gaps/skipped RAF frames do not omit audio laps. This costs memory: total cached loop padding is capped at 128 MiB per runtime; invalid/oversized cues are diagnosed once, not repeatedly allocated. Changing playback mode/seek stops and recreates sources. No additive audio effects, external streaming or automated latency calibration.

Speech is **optional opt-in**, `createRuntime(...,{speech:true})` or `forma.speech.enable()` (`speech.enabled` reports the choice). CLI `--speech` and editor-authored speech cues/actions opt in through the export host. `speech.say(text,{lang,rate,pitch,volume})` uses system synthesis, not assets. It is not reproducibly timed or guaranteed offline. Pause/seek do not replay skipped speech and attempt cancellation of owned speech. The Web Speech API only offers **window-global** cancel, so speech isolation cannot be guaranteed; cancellation is suppressed when multiple Forma instances exist and may then leave speech playing. A single Forma instance can still share the window with unrelated host speech. Use embedded recordings instead for strict instance scoping and deterministic offline output. Bare auto-mounted runtime therefore leaves synthesis disabled by default. The editor/CLI export host adds a user-gesture start control for audio/speech; no user JS is needed for embedded audio.

## Safety / errors / limits

Runtime does not evaluate `customJs`, raw SVG, string guards, URLs or remote assets. Attach explicitly trusted handlers separately. This is not an SVG sanitizer or a sandbox for unrelated scripts. Shape properties cannot install event-handler attributes, URLs, style text or innerHTML. Unknown API IDs throw; schema version/IDs/durations/key times/easings/state targets/cue ranges are validated. Missing DOM nodes and unsupported actions/properties report diagnostics. Handlers and actions fail independently. Latest 100 errors are retained and available through `on('error', ({error})=>...)`. Failed auto-initialization logs a console error and retains the static SVG; explicit construction throws.

No arbitrary path morphing, additive clip blending, automatic plugin translation, user-code execution, legacy browser support, perfect audio/visual sample synchronization, full schema migrations or full resource quotas. Core enforces stricter geometry/ID/property validation than the flexible adapter; exporters should call `Project::validate()`.

## Tests

Requires Node 18+ (verified with Node 22.22.2), no npm install:

```sh
node --check runtime/src/forma.js
node --test runtime/tests/*.test.cjs
```

Tests cover bone hierarchy/parent tips, attached transforms, numeric rotations/easing, patch composition, invalid rigs/overflow/depth, old documents, interpolation, canonical rendering/priority, precise loops/seeks/end callbacks, cross-clip event timing, global timeline, machine initialization/guards/order/cycle limits, DOM interaction/drag, cleanup, aliases and independent instances, fake Web Audio scheduling/offsets/native loops/memory failure, optional speech, clock switching, error handling and plain-script auto-mount. Fake audio does **not** verify audible latency, codecs, browser unlock policy or device behavior.

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

Verified green **PASS** in direct-file Firefox 151.0.1: XML-safe JSON containing `</script> & ]]>`, script auto-mount, core transform sampling, bone hierarchy/attachment/numeric rotation, loop crossings, silent seek, DOM click → machine entry, nested duplicate-ID scoping, alias lifecycle and destroy. This smoke is visual/manual, not a Node assertion of browser output. Script-disabled display says STATIC FALLBACK. Browser matrix beyond Firefox, real codecs/unlock, sound hardware, CSP-restricted hosts and speech remain manual acceptance work.
