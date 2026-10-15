# Splinely

**English** · [Deutsch](README.de.md) · [Español](README.es.md) · [Français](README.fr.md) · [日本語](README.ja.md) · [简体中文](README.zh.md)

![Splinely logo](assets/splinely-logo.svg)

Splinely is a Rust desktop editor with **`.sply` project files** and an embedded JavaScript runtime for interactive SVGs. Draw shapes and paths, animate them with keyframes, bones and state machines, and export a single self-contained SVG. **Current state: a working prototype, not the full scope of [PLAN.md](PLAN.md).**

The editor speaks **English, German, Spanish, French, Japanese and Simplified Chinese**. It follows the system language and can be switched under **Settings › General › Language**. Japanese and Chinese need an installed CJK font (e.g. Noto Sans CJK). The detailed documents in `docs/` are written in German.

## Install with an LLM

Just tell an AI agent with shell access (e.g. Claude Code):

> Install Splinely for me: https://raw.githubusercontent.com/wasser-quest/splinely/main/llms.txt

[`llms.txt`](llms.txt) tells the agent how to download the matching prebuilt package from the releases, verify the checksum, unpack it to `~/.local/opt` and then work with `splinely-cli`.

## Getting started

**No compiling:** prebuilt Linux packages (`splinely-vX.Y.Z-x86_64-linux.tar.gz` with editor, CLI, viewer, runtime and `libsplinely_runtime.so`) are attached to every [release](https://github.com/wasser-quest/splinely/releases/latest). Installation details: [docs/INSTALL-BINARIES.md](docs/INSTALL-BINARIES.md).

From source:

Requirements: Rust/Cargo; a working graphics/window environment for the desktop window. The first build downloads dependencies. Node.js and Python 3 are only needed for the check scripts (`scripts/check.sh`).

On Linux, building the editor and viewer needs the GTK/WebKit, ALSA and D-Bus development packages:

```sh
# Fedora
sudo dnf install gtk3-devel webkit2gtk4.1-devel libsoup3-devel alsa-lib-devel dbus-devel libxkbcommon-devel
# Debian/Ubuntu
sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libasound2-dev libdbus-1-dev libxkbcommon-dev pkg-config
```

```sh
git clone https://github.com/wasser-quest/splinely.git
cd splinely
cargo run -p splinely_editor --bin splinely
# Export without a desktop window:
cargo run -p splinely_editor --bin splinely -- --demo-export /tmp/splinely-demo.svg
```

The editor starts with its own demo. **Drop in a `.sply` file**, use **File → Open …** (Ctrl+O, system file dialog) or **Recent**, or start directly: `cargo run -p splinely_editor --bin splinely -- examples/demo.sply`. Unsaved changes and drawing/source drafts are confirmed before switching and when closing the window; the window title shows the file and `•` when unsaved. **Save** keeps the editable project; SVG is only an optional export.

**Design / Animate / Advanced** separate drawing, animation and code. Select, Pen, Path points and Freehand are exclusive tools; extra options stay collapsed. **Preview**, **Play** and **Open Splinely Viewer** are always at hand. [Usage, gestures and limits](docs/usability.md).

Custom JavaScript only runs after explicit, per-session approval under **Advanced → JavaScript**, never in the native canvas. **Animate → Audio** can turn on a limited `ffplay` preview of global audio cues; preparation happens in the background.

## One project file: `.sply`

Geometry, layers, bones, keyframes, states, blend weights, variables, embedded audio and custom code stay **together in one file**. No extra JSON/SVG or asset folder needed. Internally `.sply` is strictly validated, readable JSON; older `.json`/`.splinely.json` projects stay compatible. The application/runtime and installed editor plugins are not part of the project file. [Format and compatibility](docs/sply-format.md).

Under **Animate**, **Easy animation** (keyframe/preset) and **Guided state machine** (click/hover, visibility, clip start) walk you through the first steps. Weighted blending and timed fades stay available next to the expert tools. [Step by step and blending limits](docs/animation-workflow.md).

```sh
splinely-cli new --demo --output scene.sply
splinely-cli convert legacy.splinely.json --output scene.sply
splinely-viewer scene.sply
# Only for reviewed custom code:
splinely-viewer examples/desert-stickfight.sply --trusted-js
```

## Splinely Viewer

`cargo build -p splinely_viewer` builds the real native SVG WebView. In the editor, **▶ Splinely Viewer** opens a snapshot of the current project including runtime, states and embedded audio; the silent resvg canvas stays separate. Custom JS only after approval under Advanced. Directly without exporting an SVG first: `cargo run -p splinely_viewer --bin splinely-viewer -- examples/brown-owl.sply --trusted-js` (review the bundled code first). Existing SVGs are supported too. Active third-party SVGs are not trusted. Linux needs GTK3/WebKitGTK; [details and tests](crates/splinely_viewer/README.md).

SVG exports contain **Created with Splinely**, the application/schema version and machine-readable generator metadata (`urn:splinely:metadata`). There is no visible watermark; the label is editable and no proof of authenticity.

## Using the runtime

[Runtime cookbook](docs/runtime-cookbook.md): play/pause/restart, clip and global clock, HTML embedding, audio and troubleshooting. `splinely.snapshot()` returns the runtime state as copied JSON data; `splinely.reportError(error)` uses the shared error channel with a bounded history.

## Browser-independent runtime

`splinely_runtime` provides the Splinely document core as a native Rust runtime, a C ABI (`cdylib`) and the `splinely-runtime` JSON-lines process. Other languages can send events, control time/clip weights and read evaluated nodes/bones. The host does drawing, hit testing and audio — no browser, but also no general SVG JavaScript interpreter. [API and Python examples](docs/native-runtime.md).

```sh
cargo build -p splinely_runtime
python3 examples/native-runtime/smoke.py --library target/debug/libsplinely_runtime.so --binary target/debug/splinely-runtime
```

## CLI workflow

CLI and editor share the same document core. CLI results are JSON; errors are JSON on stderr (with a `hint` for the next step) and return a failing exit status. No AI provider, API key or server is required.

**Getting started:** `splinely-cli guide` shows the short workflow with tested recipes. Editing commands save back into the input file when `--output` is omitted, `check` finds typical mistakes (hidden shapes, clips that never play, unreadable text …) with a runnable fix, `batch` runs many steps all or nothing, `clip` manages clips and `on` makes shapes interactive.

```sh
F=/tmp/scene.sply
cargo run -p splinely-cli -- new --output $F --width 800 --height 450 --background "#0b1d3a"
cargo run -p splinely-cli -- node add $F --kind circle --id sun --x 400 --y 420 --radius 60 --fill orange --pivot center
cargo run -p splinely-cli -- keyframe $F --clip sunrise --target sun --property y --time 3 --by -260 --easing ease-out --autoplay --loop
cargo run -p splinely-cli -- on $F --event click --target sun --set sun.fill=gold
cargo run -p splinely-cli -- check $F
cargo run -p splinely-cli -- render $F --time 1.5 --output /tmp/scene.png
```

With JSON and merge patches:

```sh
cargo run -p splinely-cli -- new --demo --output /tmp/draft.splinely.json
cargo run -p splinely-cli -- validate /tmp/draft.splinely.json
cargo run -p splinely-cli -- inspect /tmp/draft.splinely.json
cargo run -p splinely-cli -- evaluate /tmp/draft.splinely.json --time 0.5
printf '%s\n' '{"name":"My animation"}' > /tmp/splinely-patch.json
cargo run -p splinely-cli -- apply /tmp/draft.splinely.json --patch /tmp/splinely-patch.json --output /tmp/final.splinely.json
cargo run -p splinely-cli -- export /tmp/final.splinely.json --output /tmp/final.svg
```

`schema` returns a format guide with complete examples, not a formal JSON Schema. `apply` uses RFC 7396 merge patches: arrays are replaced entirely. `evaluate` runs no state actions, JS or audio; clips that are not active are not played. Details: [CLI.md](CLI.md), [CORE_API.md](CORE_API.md). For other LLMs: [authoring guide](LLM.md) and [copy-paste prompt](prompts/splinely-author.md); `schema` and `capabilities` list the features that are actually supported.

## End-to-end offline example

- [examples/demo.splinely.json](examples/demo.splinely.json): editable project with a circle, a three-second keyframe motion, a click state change and an embedded WAV cue at **1.5 seconds of global transport time**.
- [examples/demo.svg](examples/demo.svg): the actual CLI export with an explicitly approved, short `customJs` handler. Clicking the circle unlocks audio and restarts the motion and transport; on the first click the circle turns orange.
- The sound is **0.4 seconds of synthetic, syllable-like tone, no spoken words**. PCM WAV, mono, 8 kHz, 16 bit; no external audio files.

```sh
# Reproducible: CLI new --demo → apply → validate → export --trusted-js
bash scripts/generate-demo.sh
# Default export: code is only stored; the start control unlocks audio without user code.
cargo run -p splinely-cli -- export examples/demo.splinely.json --output /tmp/demo-safe.svg
# Only after reviewing your own code:
cargo run -p splinely-cli -- export examples/demo.splinely.json --trusted-js --output /tmp/demo-trusted.svg
```

Open `demo.svg` directly in the browser as `file://`, click **Start audio / animation** and then the circle. Do not embed it as `<img>`: SVG scripts don't run there. Without scripts the static initial graphic stays visible. Browsers/CSP can block scripts or dynamic code compilation; audio needs a real user interaction and a supported decoder.

The bundled runtime and the example file need no external libraries or network resources. This is **not a sandbox**: approved user code can make network requests and perform other browser actions. By default `customJs` is kept but not executed; `rawSvg` stays metadata only. Don't export/open unknown projects with `--trusted-js`.

## Vectors, layers and bones

- **Layers:** selection, order, visibility and edit lock. Colors and transparency are in the inspector.
- **Paths:** double-clicking a path edge inserts a shape-preserving anchor; Shift links Bézier handles symmetrically, Shift+Alt keeps the neighbouring length. [Details](docs/path-editing.md). Edit SVG path syntax as well as graphical anchors/Bézier control points; **Pen** draws by click and drag (Enter finishes, Escape cancels). **Snap 10** snaps object positions locally on release. Convert rectangles/circles to paths and back while unchanged. SVG import is deliberately limited and rejects unsupported appearance instead of silently dropping it.
- **Bones:** select/move joints, edit parents and bindings, keyframe rotation. Rigid binding and limited two-bone IK for direct, offset-free child joints; no mesh deformation. Load `examples/rig.splinely.json`; `examples/rig.svg` shows the same animation in the browser.
- **Navigation:** F/Home fits the scene; +/- zooms, 0 sets 100 %. Middle button or Space + left button pans without changing shapes; Space without dragging starts/stops on release. Rulers can be shown/hidden and stay in document coordinates while panning/zooming. The timeline can be hidden. **Preview** separates scene events from editing.
- **Freehand/tablet:** freehand mode creates editable SVG paths, one stroke per undo step. Pressure-sensitive strokes only when force events are available; no blanket hardware/driver promise. [Tablet notes](docs/tablet.md).
- **Drop-in plugins:** put a `splinely-plugin.json` into the plugin folder and restart — declarative shape tools and palettes load automatically without compiling. [Format, folders and examples](docs/drop-in-plugins.md); third-party scripts/libraries never run automatically.
- **Plugins:** static Rust ECS extensions with their own resources and a decorate phase. [Template](plugin-templates/editor/README.md); `splinely-cli plugin` generates source code but never runs it. Native plugins are trusted code, not a sandbox.

```sh
cargo run -p splinely-cli -- import-svg drawing.svg --output drawing.splinely.json
cargo run -p splinely-cli -- rig-demo --output /tmp/rig.splinely.json
cargo run -p splinely-cli -- export /tmp/rig.splinely.json --output /tmp/rig.svg
```

Create and arrange shapes with the CLI and check them as PNG (same logic as the editor):

```sh
cargo run -p splinely-cli -- node add scene.sply --kind circle --x 200 --y 150 --radius 40 --fill tomato --output scene.sply
cargo run -p splinely-cli -- arrange align scene.sply --nodes node-1,node-2 --edge top --output scene.sply
cargo run -p splinely-cli -- render scene.sply --output scene.png --width 800
```

More shape, arrange, bone, layer and conversion commands: [CLI.md](CLI.md). `keyframe` adds single node/bone keys without full array patches.

## Implemented and still open

| Implemented now | Not implemented / deferred |
| --- | --- |
| Rectangle, circle, text, paths, polygon/star/line; multi and box selection, scale/rotate handles, smart and manual guides, context menu, command palette (Ctrl+K), settings (Ctrl+,) with rebindable shortcuts, light/dark theme, UI language and visible grid, record mode (auto-key), history list, size readout at the pointer, visual timeline with draggable keyframes and easing menu, configurable onion skin (keyframes or interval, several ghosts), text editing on the canvas, document colors, presets for new projects, PNG/GIF export, eyedropper, linear/radial gradients, nested groups (tree view, `<g>` export), text to path, align/distribute/flip, curve-preserving boolean operations, path morphing, animated gradients, layer order, copy/paste, colors/transparency, layers, conversion, undo/redo | Morphing between shapes of different topology is approximated (growing/shrinking subpaths), no exact hole matching |
| Load/save project via temporary file + rename; editor autosave for changed projects every 30 s with a recovery dialog | Historical schema migrations, complete resource quotas |
| Timeline, keyframes, easing, independent clip clocks, parallel states, weighted clips and timed crossfades (`blendClip`) | Additive blending, visual blend tree/blend space |
| State model, guards/actions, editable state graph, JSON editing and native state preview | Free drag connections, automatic graph layout |
| Browser events, clip/transport API, embedded audio assets/cues | Native JS execution or guaranteed audio latency; native audio preview covers only global cues via ffplay |
| Static/interactive SVG export, limited SVG vector import, JS/raw SVG source fields | **No complete importer for arbitrary SVGs**; text/images/gradients/filters are rejected on import; raw SVG on its own stays metadata |
| Bevy ECS projection, registrable static plugins, bones and rigid bindings | Dynamic native plugins, mesh skinning, general IK chains, automatic translation of arbitrary Bevy systems into browser code, isolated untrusted preview |

The native preview renders SVG with resvg: paths, rounded corners and transforms are visible. Text uses local system fonts and may differ between browsers. Shapes are picked along their exact outline (fill rule or stroke distance); invisible shapes (opacity 0) can't be clicked. Schema 1 has been extended; old projects load with defaults for new fields. Older Splinely binaries don't understand the new fields. The `.sply` project file (JSON inside) is authoritative; the SVG export is no lossless import round trip.

## Checks

```sh
bash scripts/check.sh
```

The script runs workspace tests, Node runtime tests, real CLI demo generation/evaluation/export and a Python XML/metadata/WAV/resource check. It also checks the shipped example files. **No replacement for a real browser/audio test.** Additionally, `python3 scripts/svg-corpus.py` checks installed real-world SVGs and `python3 scripts/browser-smoke.py examples/demo.svg /tmp/splinely-svg-corpus/manifest.json` checks Firefox including a corpus pixel comparison. Tests cover strict JSON, paths, layers, bones, plugin lifecycle, runtime security regressions and every UI language; XML and the embedded WAV are verified. Audible hardware output and audio latency remain unverified. Reproduction and manual acceptance: [docs/browser-tests.md](docs/browser-tests.md). Native GUI screenshots and GDB/REA artifact checks: [docs/verification.md](docs/verification.md).

## Animated showcases

[Showcase overview](examples/showcase.html) · [Interactive SVGs, direct events and invisible states](docs/interactive-svg.md)

- **[Sand / Accord](examples/desert-stickfight.html)** ([editable `.sply`](examples/desert-stickfight.sply)): an original 20-second short with two bone figures in the warm Cactus Runner desert style, five fight phases and a friendly ending; play/pause/replay. [Project, generation and real Firefox check](docs/desert-stickfight.md).
- [Cactus Runner](examples/cactus-runner.html): running bone figure, scrolling cacti and a click jump with real walk/jump weight blending; independent run and jump states. [Details/check](docs/cactus-runner.md).
- [Cinder / Flux – Stickfight](examples/stickfight.html): an original eight-second animated fight between two bone rigs with dodges/counters, shadows/sparks and play/pause/restart. `python3 scripts/generate-stickfight.py`, check: `python3 scripts/stickfight-smoke.py`.
- [Brown owl](examples/brown-owl.html): an owl on a branch with direct click/hover states – on hover the eyes follow the pointer and the ear tufts perk up; a click makes it close its eyes contentedly and lift its wings. Check: `python3 scripts/owl-smoke.py`.
- [Aoi says hello](examples/anime-hello.html): an original anime-inspired character with a bone hand. **A locally generated WAV is embedded in the SVG**; no internet connection needed. A click unlocks audio. Reproduce: `python3 scripts/generate-anime-demo.py` (needs a local `espeak-ng`/`espeak`); browser check: `python3 scripts/anime-smoke.py`.
- [Water drop on a blue sphere](examples/water-drop.html): self-contained SVG scene with squash/stretch, highlights and ripple; play/pause.
- [Random figures](examples/random-figures.html): reproducible by seed, automatic state sequence with captions. Click **Play**. State changes follow clip ends.
- Both HTML files embed the neighbouring SVG via `object`; no server API. The test server is only a local browser test helper.
- Generate: `splinely-cli water-drop --help`, `splinely-cli random-figures --help`; full examples in [CLI.md](CLI.md). Browser check: `python3 scripts/showcase-smoke.py --duration 35`.

## Logo

An original SVG logo: an S drawn as a Bézier spline with anchors and a handle, plus a motion trail.

## License

MIT, see [LICENSE](LICENSE).
