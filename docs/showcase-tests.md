# Real-browser showcase smoke

Run from the repository root with Python 3 and Firefox installed. No Python
packages, Selenium, or geckodriver are needed:

```sh
python3 scripts/showcase-smoke.py examples/water-drop.html examples/random-figures.html
```

Supply other wrapper paths explicitly if the CLI generates different filenames.
Both wrappers and their referenced SVG/assets must be under `--root` (default:
repository root). Export with the showcase's reviewed custom JavaScript enabled; do not use an untrusted project. The test does not manufacture
animation, send machine events, seek, or remount the runtime.
Use a fixed seed when generating random figures. The report embeds the exported
project so its seed/custom-JS configuration can be inspected.

Options:

- `--duration 30`: maximum real playback seconds **per wrapper**, increase if a
  complete automatic state cycle takes longer.
- `--interval .25`: wall-clock observation interval.
- `--mount-timeout 10`: SVG mount timeout.
- `--object-selector "object[type='image/svg+xml']"`: select the showcase object.
- `--water-node drop --water-attribute transform`: restrict animation evidence
  to a known water node/attribute (both repeatable). Without these, compare
  transform, geometry and opacity attributes of existing project nodes; changed
  node IDs are recorded. Choose the explicit water IDs for wrappers containing
  unrelated animations.
- `FIREFOX=/path/to/firefox`: choose the browser binary.

The harness serves only a temporary `127.0.0.1` HTTP origin and shuts it down in
`finally`. This avoids Firefox's `file://` same-origin restrictions when inspecting
an HTML object's SVG document. It does not modify exports or introduce an online
export dependency. Only serve a trusted directory; the HTTP root is readable to
local clients during the test.

## Assertions and evidence

- HTML `<object>` contains an SVG whose own `documentElement.splinely` mounted.
- Both showcases start through their authored Play control (`#start-control`).
- Marionette viewport pointer actions click the correctly scrolled, transformed control position. Firefox frame `ElementClick` could silently miss offscreen SVG controls in actual testing, so it is not trusted here. Captured `isTrusted` must confirm the gesture; no JavaScript `.click()` is used.
- Start disappears, the timeline plays and advances, and real rendered node DOM
  attributes differ across wall-clock samples (including the water animation).
- Random figures automatically visit **every declared state of every machine**.
  Initial states are counted only when actually observed; runtime `state.change`
  events record even transitions between polling samples. At least one transition
  must occur. No machine input is supplied by the test.
- Window errors, unhandled rejections, and runtime diagnostics are recorded.
  Unexpected runtime diagnostics fail the test. The existing runtime advisory
  `customJs is preserved but not executed; attach trusted handlers explicitly`
  is retained but exempted: it precedes trusted export-host execution.

Outputs, also written on assertion failures:

- `/tmp/splinely-showcase-results.json`: checks, source project, observations,
  transitions, errors/stack traces and Firefox process log.
- `/tmp/splinely-showcase.png`: current HTML wrapper screenshot, normally random
  figures. A failing case is preserved over a later passing case. JSON identifies
  the captured case and whether capture succeeded.

Exit status is zero only when both showcases pass. Missing exports or browser
startup failures still produce structured JSON, but cannot produce a new browser
screenshot. A pre-existing PNG is not evidence for that run; check
`screenshotCaptured`.

## Observed run

With 0.3.0 both shipped wrappers passed `python3 scripts/showcase-smoke.py --duration 35` in headless Firefox: genuine trusted controls, water DOM motion and every gallery state. The structured report and screenshot are in the paths above. A too-short eight-second run deliberately failed full state coverage; the six 4.8-second clips need a longer observation window.

REA independently captured both HTML/SVG scenes in Chromium 156.0.8078.4: visible movement, timeline/state status and Pause worked; the full gallery cycle completed at 28.8 seconds. Evidence IDs: water `ev_d6419545584bb289d18fa3c368a0a4ad14ebe1bdacdce6f59ddeadb7f2375bfc`, gallery `ev_a2099c86a1f148d64d2b9c468c11c663db8b964e23647d7e2651240a11d771d9`. Reports `/tmp/splinely-showcase-rea-water-drop.json` and `/tmp/splinely-showcase-rea-random-figures.json` include screenshots, DOM, network and errors. No page errors; a local favicon 404 occurred.

## Observed versus unproven

A passing run establishes real Firefox DOM mounting, trusted Start, ongoing
animation and automatic state coverage. It does **not** prove device output. It also does not test direct-file loading, other browsers,
or reproducibility across two seeded exports. Errors preceding observer attachment
are only visible if retained by runtime diagnostics or the Firefox log.
