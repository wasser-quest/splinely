# Forma Viewer

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

cargo build -p forma_viewer
cargo run -p forma_viewer -- examples/demo.svg
cargo run -p forma_viewer -- examples/demo.forma.json
# A single authored file, with the same JSON contents as .forma.json:
forma-viewer authored.frm
# Only after reviewing project customJs:
cargo run -p forma_viewer -- examples/demo.forma.json --trusted-js
```

Binary name: `forma-viewer`. Positional input is required except for `--help` /
`--version`. `.frm` and `.json` inputs (including `.forma.json`) are detected
automatically, case-insensitively; `--project` forces JSON for another extension.
A `.frm` is a strict canonical `forma_document::Project` JSON document, identical
in contents to `.forma.json`, **not a ZIP archive**. Geometry, states, animations,
audio assets and code are embedded in that one file. The viewer generates SVG
in memory and opens it through the same genuine WebView as SVG input; no sidecar
SVG, extraction step or external asset files are needed. `--speech` enables system browser speech for project
export (voices/offline availability depend on the platform).

```sh
# No display initialization, JavaScript execution, or audio playback:
forma-viewer examples/demo.forma.json --check
# Portable HTML for browser testing; still does not execute scripts:
forma-viewer examples/demo.forma.json --check --output preview.html
# The prepared artifact intentionally executes reviewed customJs when opened:
forma-viewer reviewed.forma.json --trusted-js --check --output preview.html

# Headless tools without GTK/WebKit development dependencies:
cargo test -p forma_viewer --no-default-features
cargo run -p forma_viewer --no-default-features -- examples/demo.svg --check
```

`--output` without `--check` writes HTML and then opens the native window. Output
is atomically replaced using a temporary file in its destination directory. It
cannot alias the input, including through hard links or symlinks; replacement
does not truncate other hard links to an existing output file.

## Playback and trust

Project JSON is validated by `forma_document`. Export embeds
`runtime/src/forma.js` via `include_str!` and uses `Project::to_svg_with_options`.
State machines, hover/click handlers, clips and embedded audio use the same
runtime as browser exports. Project `customJs` is preserved as data but **not
executed unless `--trusted-js` is supplied**. Projects with audio get the runtime
host's user-gesture Start audio / animation control. Speech is opt-in.

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

`ViewerOptions { project: bool, trusted_js: bool, speech: bool }` defaults all
flags to false. `PreparedViewer` exposes `html: String`, `title: String`,
`is_svg_input: bool`, and `custom_js_disabled: bool`. Preparation is GUI-free.
Launch must run on the main thread and owns a native event loop until the window
closes or renderer failure is reported. Without `webview`, launch returns an
explicit unavailable-feature error. For an editor already owning its event loop,
spawn the **separate `forma-viewer` binary**, passing a reviewed SVG/project path;
do not start a second Tao event loop inside the editor or a background thread.

For editor/main integration, save with the existing `Project::to_json()` and
launch `forma-viewer /path/to/project.frm` (no `--project` flag required).
Add `--trusted-js` only with explicit consent for reviewed `customJs`, and
`--speech` only with explicit speech opt-in. Neither flag is implied by `.frm`.
For a standalone main-thread host, use
`launch(path: &std::path::Path, options: ViewerOptions) -> ViewerResult<()>`
with `ViewerOptions { project: false, trusted_js: consent, speech: speech_opt_in }`.
The viewer calls the existing
`Project::from_json(json: &str) -> Result<Self, DocumentError>` and
`Project::to_svg_with_options(&self, runtime: &str, options: BrowserExportOptions) -> Result<String, DocumentError>`;
`audio_controls` stays true for user-gesture audio unlocking.
There is no `--static`/script-free viewer option; `--check` prepares without
executing anything, but its resulting HTML remains active playback content.

## Tests and actual verification

```sh
cargo test -p forma_viewer
TMPDIR=/home/user/svg_editor/.work-tmp cargo test -p forma_viewer --no-default-features
TMPDIR=/home/user/svg_editor/.work-tmp cargo clippy -p forma_viewer --all-targets --no-default-features -- -D warnings
cargo build -p forma_viewer
# Opt-in X11 smoke; requires xdotool, ImageMagick import, Python Pillow:
python3 crates/forma_viewer/tests/native_smoke.py
python3 crates/forma_viewer/tests/native_smoke.py --svg
python3 crates/forma_viewer/tests/native_smoke.py --crash
```

Set `TMPDIR` to a filesystem with sufficient space for builds/test tempfiles
when necessary. Smoke tests run the actual binary, use real pointer events,
and inspect rendered colors. The committed fixture tests the shared runtime's
hover/click states, trusted customJS, embedded WAV unlock/scheduling, resize,
clean close, and (Linux) killed-renderer error reporting. Audio scheduling is
verified, **not audible speaker output**. `--svg` first exports/prepares the
fixture and launches the resulting standalone SVG rather than project JSON.

Headless preparation tests also cover single-file `.frm` opening with embedded
geometry/states/animation/audio/code, strict schema rejection, explicit code
trust, speech opt-in, identical `.forma.json` playback and no sidecar SVG.

Prior native verification (before `.frm` detection was added): Wry 0.57.0, Tao 0.37.1, GTK 3.24.52, WebKitGTK
2.52.5. Native compilation passed; all **11 Rust tests** passed with default
features and with `--no-default-features`. All three native smoke modes passed.
Windows/macOS builds and speech voice playback were not tested here.
