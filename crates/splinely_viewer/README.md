# Splinely Viewer

A native desktop WebView for **trusted, interactive SVGs**, not a raster preview.
The default `webview` feature uses Wry 0.57 + Tao 0.37. Linux uses GTK3 and
WebKitGTK 4.1; no HTTP server or external browser is required.

## Build and run

From the repository workspace (edition 2021):

```sh
# Debian/Ubuntu
sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev
# Fedora
sudo dnf install gtk3-devel webkit2gtk4.1-devel

cargo build -p splinely_viewer
cargo run -p splinely_viewer -- examples/demo.svg
cargo run -p splinely_viewer -- examples/demo.splinely.json
# A single authored file (compact binary by default; same model as .splinely.json):
splinely-viewer authored.sply
# Only after reviewing project customJs:
cargo run -p splinely_viewer -- examples/demo.splinely.json --trusted-js
# Same project animated by the Rust runtime (splinely_runtime) started by the viewer:
cargo run -p splinely_viewer -- examples/stickfight.splinely.json --runtime rust
```

Binary name: `splinely-viewer`. Positional input is required except for `--help` /
`--version`. `.sply` and `.json` inputs (including `.splinely.json`) are detected
automatically, case-insensitively, and binary `.sply` is recognised by its magic bytes
whatever the extension; `--project` forces JSON for another extension.
A `.sply` is a binary (or legacy JSON-text) encoding of the canonical
`splinely_document::Project`, identical in contents to `.splinely.json`, **not a ZIP archive**; damaged binary files are rejected with a typed error. Geometry, states, animations,
audio assets and code are embedded in that one file. The viewer generates SVG
in memory and opens it through the same genuine WebView as SVG input; no sidecar
SVG, extraction step or external asset files are needed.

```sh
# No display initialization, JavaScript execution, or audio playback:
splinely-viewer examples/demo.splinely.json --check
# Portable HTML for browser testing; still does not execute scripts:
splinely-viewer examples/demo.splinely.json --check --output preview.html
# The prepared artifact intentionally executes reviewed customJs when opened:
splinely-viewer reviewed.splinely.json --trusted-js --check --output preview.html

# Headless tools without GTK/WebKit development dependencies:
cargo test -p splinely_viewer --no-default-features
cargo run -p splinely_viewer --no-default-features -- examples/demo.svg --check
```

`--output` without `--check` writes HTML and then opens the native window. Output
is atomically replaced using a temporary file in its destination directory. It
cannot alias the input, including through hard links or symlinks; replacement
does not truncate other hard links to an existing output file.

## Runtime: `--runtime js|rust`

`--runtime js` is the default and behaves exactly as before: the JavaScript runtime
(`runtime/src/splinely.js`) runs inside the WebView.

`--runtime rust` makes the viewer start the Rust runtime in its own process:

* `splinely_runtime::Runtime` is built from the loaded `Project` (`.sply` or JSON) and
  advanced with a fixed 1/60 s step from the native event loop (at most 5 catch-up
  steps; a longer stall drops the backlog instead of fast-forwarding).
* The WebView shows `Project::to_static_svg(0.0)` inline (no `data:` iframe, no JS
  runtime, no project metadata, no `eval`). After each tick the changed nodes of the
  evaluated `Frame` are sent as one small JSON message to a dependency-free page script
  (`src/frame_applier.js`) that writes the same attributes the JS runtime writes
  (`transform`, `opacity`, `fill`/`fill-opacity`, `stroke`/`stroke-opacity`/`stroke-width`,
  gradient stops and vector, `width`/`height`/`r`/`font-size`/`d`), by node id. One frame
  is in flight at a time (the page acknowledges it), so a slow WebView never queues frames.
* Pointer, wheel, key, focus and drag events are batched once per animation frame by
  `src/rust_host.js` and posted to the host over `window.ipc` as `{"t":"input", ...}`;
  the host forwards them as `Request::Pointer`/`PointerLeave`/`Event` (name + target node)
  to the Rust runtime. Drag/pointer-capture behaviour of the JS runtime is emulated.
* The IPC channel is the only new capability of the page. Messages are limited to
  16 KiB, strictly parsed (`deny_unknown_fields`), at most 64 events each, event names
  come from a fixed allow-list, targets must be node/group ids of the project (≤256
  bytes), pointer coordinates must be finite and within ±1e6. Anything else is dropped
  and logged on stderr. The CSP stays no-network (`connect-src/frame-src/img-src 'none'`)
  and is narrower than in js mode (no `unsafe-eval`, no iframe).

**Not reproduced in rust mode** (the viewer says so on stderr and in a visible banner
at the top of the window whenever it applies):

* `customJs` is never executed (`--trusted-js` is rejected with `--runtime rust`).
  Behaviour that only scripts provide (custom events/emit, `splinely.on` handlers,
  DOM tweaks, variable-driven code) is missing.
* Audio: no assets, `playAudio`, timeline/clip audio cues and no "Start audio" control;
  the requests are ignored (one note each). Callback host actions are ignored too.
* `prefers-reduced-motion` is not honoured (the JS runtime can freeze animation).
* Accessibility semantics of the web player (`role`, `tabindex`, `aria-*`, keyboard
  activation, focus ring, live region) are not applied; the page shows the plain static SVG.
* DOM event `detail` (coordinates, key, buttons, modifiers) is not forwarded, only the
  event name and target node; pointer position goes through the pointer request only.
* SVG input has no project to evaluate and is rejected; use `--runtime js`.
* Only the properties the JS runtime animates are applied; the SVG itself is the static
  export, so invisible nodes are absent exactly as in the JS export.
* Windows/macOS are untested (only Linux/WebKitGTK was run).

If the Rust runtime cannot load a project, the viewer prints
`--runtime rust is unavailable for this project (<reason>); falling back to the JS
runtime.` and starts the JS runtime instead; it never shows silently wrong output.
`--check` reports `runtime: rust, N nodes evaluated by splinely_runtime` or `runtime: js`.

## Playback and trust

Project JSON is validated by `splinely_document`. Export embeds
`runtime/src/splinely.js` via `include_str!` and uses `Project::to_svg_with_options`.
State machines, hover/click handlers, clips and embedded audio use the same
runtime as browser exports. Project `customJs` is preserved as data but **not
executed unless `--trusted-js` is supplied**. Projects with audio get the runtime
host's user-gesture Start audio / animation control.

An existing SVG is inherently active content: its scripts and handlers can run
without `--trusted-js`. That flag only controls project export; it is **not** an
SVG safety switch. The CLI warns on SVG input. Review SVGs and project code before
opening them. This is **not a sandbox or an untrusted-content viewer**.

The HTML wrapper loads a base64 SVG in an iframe as a real SVG/XML document,
preserving script CDATA and `document.currentScript.ownerSVGElement` behavior.
It does not use an `<img>` (which disables scripting), nor insert XML into the
HTML parser. Root SVG CSS fits the native viewport while retaining its viewBox
and aspect-ratio behavior. ViewBox-less SVGs cannot gain a meaningful scalable
coordinate system automatically. Relative file references are unsupported:
use self-contained exports with embedded assets.

The wrapper CSP denies remote resource loading, connections, forms and objects;
inline scripts/eval are deliberately enabled for trusted playback. Native
navigation callbacks reject destinations other than the local wrapper and SVG
data documents, and deny popup windows. These are defense-in-depth controls,
**not a guarantee of network isolation**: engine/platform behavior, arbitrary
trusted code, navigation paths not exposed by Wry, and browser-opened HTML have
limitations. There is no native privileged JS bridge. Use OS-level isolation if
network or host access must be prevented.

Input must be UTF-8 and is read with a **32 MiB** limit. Prepared SVG also has a
32 MiB limit, and XML parsing is bounded to **200,000 nodes**, with DTDs disabled.
These bounds do not constrain CPU/memory consumed by executing SVG scripts or
browser rendering. `--check` validates document/XML structure and prepares the
wrapper; it cannot prove JavaScript correctness, audio device availability, or
successful browser rendering.

Linux WebKit renderer termination is reported to stderr and returns failure;
macOS uses Wry's renderer-termination hook. Windows Wry does not expose the same
hook here, so renderer failure reporting there is limited to startup errors.
The viewer does not automatically reload a crashed renderer. JavaScript runtime
diagnostics remain inside the runtime/WebView, not a native error console.

## Integration API

Public library signatures:

```rust,ignore
pub type ViewerResult<T> = Result<T, Box<dyn std::error::Error>>;
pub fn prepare(path: &std::path::Path, options: ViewerOptions) -> ViewerResult<PreparedViewer>;
pub fn wrap_svg(svg: &str) -> ViewerResult<String>;
pub fn launch(path: &std::path::Path, options: ViewerOptions) -> ViewerResult<()>;
pub fn launch_prepared(prepared: PreparedViewer) -> ViewerResult<()>;
```

`ViewerOptions { project: bool, trusted_js: bool, runtime: RuntimeMode }` defaults
to false/false/`RuntimeMode::Js` (`RuntimeMode::{Js, Rust}`). `PreparedViewer` exposes
`html: String`, `title: String`, `is_svg_input: bool`, `custom_js_disabled: bool`,
`runtime: RuntimeMode` (the runtime actually used; `Js` after a fallback),
`diagnostics: Vec<String>` and `rust_session: Option<RustSession>` (rust mode only;
`launch_prepared` takes it and drives it). `RustSession` (`splinely_viewer::rust_mode`)
is usable without a GUI: `tick()`, `handle_ipc(&str)`, `frame_message()`. Preparation is GUI-free.
Launch must run on the main thread and owns a native event loop until the window
closes or renderer failure is reported. Without `webview`, launch returns an
explicit unavailable-feature error. For an editor already owning its event loop,
spawn the **separate `splinely-viewer` binary**, passing a reviewed SVG/project path;
do not start a second Tao event loop inside the editor or a background thread.

For editor/main integration, save with the existing `Project::to_json()` and
launch `splinely-viewer /path/to/project.sply` (no `--project` flag required).
Add `--trusted-js` only with explicit consent for reviewed `customJs`; it is
never implied by `.sply`.
For a standalone main-thread host, use
`launch(path: &std::path::Path, options: ViewerOptions) -> ViewerResult<()>`
with `ViewerOptions { project: false, trusted_js: consent }`.
The viewer calls the existing
`Project::from_json(json: &str) -> Result<Self, DocumentError>` and
`Project::to_svg_with_options(&self, runtime: &str, options: BrowserExportOptions) -> Result<String, DocumentError>`;
`audio_controls` stays true for user-gesture audio unlocking.
There is no `--static`/script-free viewer option; `--check` prepares without
executing anything, but its resulting HTML remains active playback content.

## Tests and actual verification

```sh
cargo test -p splinely_viewer
TMPDIR="$PWD/.work-tmp" cargo test -p splinely_viewer --no-default-features
TMPDIR="$PWD/.work-tmp" cargo clippy -p splinely_viewer --all-targets --no-default-features -- -D warnings
cargo build -p splinely_viewer
# Opt-in X11 smoke; requires xdotool, ImageMagick import, Python Pillow:
python3 crates/splinely_viewer/tests/native_smoke.py
python3 crates/splinely_viewer/tests/native_smoke.py --svg
python3 crates/splinely_viewer/tests/native_smoke.py --crash
# Opt-in X11 smoke for --runtime rust (click moves/recolours the circle via the Rust runtime):
python3 crates/splinely_viewer/tests/native_smoke_rust.py
```

`tests/rust_runtime.rs` (no GUI; needs `node` for the parity part, skipped without it) drives
`RustSession` like the event loop and replays each scenario through `src/frame_applier.js` on a
fake DOM (`tests/frame_applier.test.cjs`) next to the JS runtime on the same static SVG, comparing
every attribute (numbers within 1e-9) at several times for stickfight, desert-stickfight, rig,
brown-owl (gradients, click), cactus-runner, water-drop, demo (click via IPC) and a follower
scenario with pointer input. `tests/cli.rs` covers `--runtime` in `--check`.

Set `TMPDIR` to a filesystem with sufficient space for builds/test tempfiles
when necessary. Smoke tests run the actual binary, use real pointer events,
and inspect rendered colors. The committed fixture tests the shared runtime's
hover/click states, trusted customJS, embedded WAV unlock/scheduling, resize,
clean close, and (Linux) killed-renderer error reporting. Audio scheduling is
verified, **not audible speaker output**. `--svg` first exports/prepares the
fixture and launches the resulting standalone SVG rather than project JSON.

Headless preparation tests also cover single-file `.sply` opening with embedded
geometry/states/animation/audio/code, strict schema rejection, explicit code
trust, identical `.splinely.json` playback and no sidecar SVG.

Prior native verification (before `.sply` detection was added): Wry 0.57.0, Tao 0.37.1, GTK 3.24.52, WebKitGTK
2.52.5. Native compilation passed; all **11 Rust tests** passed with default
features and with `--no-default-features`. All three native smoke modes passed.
Windows/macOS builds were not tested here.
