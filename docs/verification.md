# Verification of the extended Forma prototype

## Latest integrated verification

`bash scripts/check.sh`: **240 Rust tests passed, one opt-in ffplay test ignored; 60 Node tests passed**. Workspace Clippy with warnings denied and formatting check passed. Genuine Linux ctypes/JSON-lines tests passed against both debug and optimized native runtime builds. Native wire regressions reject nested unknown fields, duplicate keys, oversized integers and typed precision loss without partial mutation.

Integrated: weighted sparse clip groups/crossfades, parallel state machines, native clip clocks, key capture of the unweighted source, sub-millisecond loop periods, timeline completion actions; rulers, form-preserving path insertion/linked tangents, staged freehand and restart-discovered declarative plugins. A native GUI screenshot with rulers, freehand controls and discovered plugin groups is `.work-tmp/recovery-gui-only.png`. Physical tablet pressure/driver behavior was not tested. Pressure retrace coverage and pointer release with secondary touch are deterministic regressions.

Actual offline Firefox checked the regenerated demo's trusted gesture/audio unlock/decode and state, not audible hardware output. Cactus runner Firefox checked jump crossfade, uninterrupted scrolling and all eight respawns; `.work-tmp/cactus-runner-telemetry.json`. Independent REA Chromium captures observed HTML and SVG loading, leg motion, real click jump and return, moving cacti, no page errors. Evidence IDs: HTML `ev_43d299e590168aa02b1ab3337748ce3cdd9687df6c7f7ee4cdb798dd25a2603e`, SVG `ev_012da575548256b7f2a684378754d0710a119b38187aeb7ae3c2852eb9aea4f7`. Both had a local favicon 404; this was not a console-error-free run. Persistent evidence `.work-tmp/new-rea-*-evidence.json` and screenshots. Native Viewer build/preparation passed. A separate genuine X11/Wry launch loaded the SVG; a real mouse click visibly moved the runner from ground to airborne (`.work-tmp/recovery-viewer.png`, `.work-tmp/recovery-viewer-click.png`). This is a bounded visual check, not DOM telemetry. The REA captures remain Chromium, not Wry or native C-ABI evidence.

Native optimized file sizes: approximately 1.4 MiB shared library, 1.5 MiB JSON-lines executable; not a RAM/latency benchmark. Rendering/input hit testing/audio are host responsibilities, and customJs remains inert outside the browser/WebView.

## Historical authoring integration

The Pen/anchor editor, staged state graph, local Snap10, bounded two-bone IK, optional native audio and CLI showcases are integrated. Current verification: **116 Rust tests + 44 Node tests passed**, workspace Clippy with warnings denied passed. One opt-in real `ffplay` test was also run explicitly and passed; this is not physical-speaker audibility proof. Stop during preparation keeps its single-worker slot until completion, preventing reload from accumulating project copies/decoders.

Actual Firefox: shipped demo offline audio unlock/decode and state; four real-SVG pixel comparisons; every rig node matches Rust at four sample times; both HTML showcases pass a 35-second trusted-control/state/speech-request test. See [showcase-tests.md](showcase-tests.md). REA independently captured Chromium screenshots/DOM/network and the complete gallery cycle, but Chromium speech failed in this environment. GDB ran the current native demo exporter and the process exited normally. Native GUI was launched and screenshots showed Pen controls and an authored path layer; geometry/gesture correctness is primarily covered by deterministic tests, not a complete manual UI acceptance pass.

Tool usage guide and remaining limits: [authoring.md](authoring.md). No claim of full Inkscape compatibility, arbitrary IK/skinning, plugin sandboxing or guaranteed offline speech.

## Reproducible checks

```sh
bash scripts/check.sh
cargo clippy --workspace --all-targets -- -D warnings
python3 scripts/svg-corpus.py
python3 scripts/browser-smoke.py examples/demo.svg /tmp/forma-svg-corpus/manifest.json examples/rig.forma.json
```

Earlier local run: **78 Rust tests, 43 Node tests, no failures; Clippy clean**. Corpus: eight installed real SVG fixtures, four imports and four intentional atomic rejections. Requires the installed Mint-X/Adwaita files listed in the generated manifest; not a portable bundled corpus.

Real offline Firefox checks passed: scene ownership/property guards, nested foreignObject isolation, destroy/remount, pointer lifecycle, actual user-gesture Web Audio unlock/decode, timeline and click-state transition. Four original-versus-imported SVG screenshot comparisons: three pixel-identical; the cellular icon differed in 0.0046% of pixels, maximum channel difference 6/255. Bone-driven SVG transforms matched Rust forward-kinematics evaluation for every demo node at 0, 0.5, 1.5 and 2.5 seconds. These are bounded fixture tests, not proof for arbitrary SVGs, physical audio output, latency or Chromium.

## Native GUI

Launched the current debug executable under X11; captured 1200×800 and 840×640 windows. Narrow window retained wrapped toolbars, canvas, layers and timeline. Canvas visibly rendered circle/text. Screenshots are local evidence at `/tmp/forma-final-gui.png` and `/tmp/forma-final-gui-narrow.png`, not shipped assets. Window activation through `_NET_ACTIVE_WINDOW` was unsupported by this window manager; capture and resize nevertheless succeeded. No claim of a comprehensive automated GUI acceptance suite.

GDB export run exited normally. A separate real GUI run stopped at the editor update method, `crates/forma_editor/src/app.rs:665`, through eframe/egui's native rendering stack. The debugger deliberately terminated that inferior after the backtrace; this was not a crash. Logs: `/tmp/forma-final-gdb.log`, `/tmp/forma-final-gdb-ui.log`. GDB warned about a missing Rust auto-load script; breakpoint/backtrace still worked.

## REA artifact evidence

REA inspected a **stripped copy**, not the working executable:

- File: `/tmp/forma-final-rea-stripped`, ELF x86_64, 29,504,984 bytes.
- SHA-256: `e675974b5e7aefae5005ed437f8f77cdd299a2c65a6cdd2abbfa3171bba904ca`.
- Evidence: `ev_cb0f92d4710f2fa58467c6acefd15c9e0dcfc66ac8a46aea87b2807bbd839798`.
- Result: `/tmp/forma-final-rea-layout.json`.

The original debug executable is approximately 222 MiB. Removing debug symbols yielded a roughly 29 MiB copy, motivating the workspace release profile's symbol stripping. **This is not a measured optimized release build or a speed benchmark.** REA artifact inspection and GDB/native screenshots complement source reviews; they do not establish absence of bugs. The earlier REA GUI process scenario only established process lifetime, not visual correctness.

## Remaining limitations

No full SVG importer, graphical Pen/Bézier point editor, IK/mesh skinning, native audio engine, arbitrary path morphing, full visual state graph or sandboxed untrusted scripting. Bones use rigid bindings; paths use bounds-based picking. Custom JS is explicit trust opt-in. Speech remains a prototyping tool with system-dependent voices. Plugins are trusted statically compiled Rust code. See README and PLAN for scope.
